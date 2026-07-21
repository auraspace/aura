use std::collections::HashMap;

use aura_ast::{CallExpr, Expr, Span};

use super::{is_array_primitive_elem, Checker};
use crate::error::SemaError;
use crate::sigs::{CallInstantiation, ClassSig, EnumSig, EnumVariantSig, FunSig};
use crate::ty::{nominal_key, Ty};
use crate::util::{subst_ty, type_subst_map, unify_ty};

impl Checker {
    pub(crate) fn check_call(
        &mut self,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Ty, SemaError> {
        if let Expr::Field(fe) = c.callee.as_ref() {
            // C3n/C3u: `Alias.fun(...)` or `Alias.Type(...)` where Alias is `import path as Alias`.
            if let Expr::Ident(id) = fe.object.as_ref() {
                if let Some(pkg) = self.import_aliases.get(&id.name).cloned() {
                    let name = fe.field.name.clone();
                    if self.fun_in_package(&name, &pkg).is_some() {
                        if !c.type_args.is_empty() {
                            // Generics on qualified free calls still deferred.
                            return Err(SemaError {
                                message:
                                    "type arguments not allowed on package-qualified calls in C3n"
                                        .into(),
                                span: c.span,
                            });
                        }
                        let sig = self.resolve_fun_in_package(&name, &pkg, fe.field.span)?;
                        return self.check_fun_call_with_sig(&sig, c, expected);
                    }
                    if let Some(class) = self.class_in_package(&name, &pkg).cloned() {
                        return self.check_class_ctor(&class, c, expected);
                    }
                    return Err(SemaError {
                        message: format!("`{name}` is not a member of package `{pkg}`"),
                        span: fe.field.span,
                    });
                }
            }

            if !c.type_args.is_empty() {
                return Err(SemaError {
                    message: "type arguments not allowed on method calls in C2b".into(),
                    span: c.span,
                });
            }
            let raw_obj_ty = self.check_expr(&fe.object)?;
            // C4s: `?.` method call on nullable receiver → nullable result.
            let (obj_ty, safe_wrap) = if fe.safe {
                match &raw_obj_ty {
                    Ty::Nullable(inner) => (inner.as_ref().clone(), true),
                    other => {
                        return Err(SemaError {
                            message: format!(
                                "`?.` requires a nullable receiver, got {}",
                                other.display()
                            ),
                            span: c.span,
                        });
                    }
                }
            } else {
                (raw_obj_ty, false)
            };

            if let Some(cname) = obj_ty.class_name() {
                let class = self
                    .class_by_nominal_key(match &obj_ty {
                        Ty::Class(k) | Ty::ClassApp { name: k, .. } => k.as_str(),
                        _ => cname,
                    })
                    .cloned()
                    .ok_or_else(|| SemaError {
                        message: format!("unknown class `{cname}`"),
                        span: c.span,
                    })?;
                let method =
                    class
                        .methods
                        .get(&fe.field.name)
                        .cloned()
                        .ok_or_else(|| SemaError {
                            message: format!("unknown method `{}` on `{cname}`", fe.field.name),
                            span: fe.field.span,
                        })?;
                let subst = type_subst_map(&class.type_params, obj_ty.class_args());
                let params: Vec<Ty> = method.params.iter().map(|p| subst_ty(p, &subst)).collect();
                let ret = subst_ty(&method.ret, &subst);
                self.check_args(
                    &params,
                    &c.args,
                    &format!("{}.{}", cname, method.name),
                    c.span,
                )?;
                self.note_mono_ty(&obj_ty);
                return Ok(if safe_wrap {
                    Ty::Nullable(Box::new(ret))
                } else {
                    ret
                });
            }

            if let Ty::Interface(iface_name)
            | Ty::InterfaceApp {
                name: iface_name, ..
            } = &obj_ty
            {
                let iface = self
                    .iface_by_nominal_key(iface_name)
                    .ok_or_else(|| SemaError {
                        message: format!(
                            "unknown method `{}` on interface `{iface_name}`",
                            fe.field.name
                        ),
                        span: fe.field.span,
                    })?;
                let method =
                    iface
                        .methods
                        .get(&fe.field.name)
                        .cloned()
                        .ok_or_else(|| SemaError {
                            message: format!(
                                "unknown method `{}` on interface `{iface_name}`",
                                fe.field.name
                            ),
                            span: fe.field.span,
                        })?;
                let subst = type_subst_map(&iface.type_params, obj_ty.iface_args());
                let params: Vec<Ty> = method.params.iter().map(|p| subst_ty(p, &subst)).collect();
                let ret = subst_ty(&method.ret, &subst);
                self.check_args(
                    &params,
                    &c.args,
                    &format!("{}.{}", iface_name, method.name),
                    c.span,
                )?;
                return Ok(ret);
            }

            // Type param with interface bounds: call methods from any bound.
            if let Ty::TypeParam(pname) = &obj_ty {
                let bounds = self.type_params.get(pname).cloned().unwrap_or_default();
                for iface_name in &bounds {
                    if let Some(method) = self
                        .iface_by_nominal_key(iface_name)
                        .and_then(|i| i.methods.get(&fe.field.name))
                        .cloned()
                    {
                        self.check_args(
                            &method.params,
                            &c.args,
                            &format!("{}.{}", iface_name, method.name),
                            c.span,
                        )?;
                        return Ok(method.ret);
                    }
                }
                if bounds.is_empty() {
                    return Err(SemaError {
                        message: format!(
                            "cannot call method `{}` on unbounded type parameter `{pname}`",
                            fe.field.name
                        ),
                        span: fe.field.span,
                    });
                }
                return Err(SemaError {
                    message: format!(
                        "method `{}` not found on bounds of `{pname}` ({})",
                        fe.field.name,
                        bounds.join(", ")
                    ),
                    span: fe.field.span,
                });
            }

            // C4v/C4w: builtin String methods.
            if obj_ty == Ty::String {
                match fe.field.name.as_str() {
                    "isEmpty" => {
                        if !c.args.is_empty() {
                            return Err(SemaError {
                                message: format!(
                                    "`String.isEmpty` expects 0 arguments, got {}",
                                    c.args.len()
                                ),
                                span: c.span,
                            });
                        }
                        return Ok(if safe_wrap {
                            Ty::Nullable(Box::new(Ty::Bool))
                        } else {
                            Ty::Bool
                        });
                    }
                    "charAt" => {
                        // C4w: UTF-8 byte at index as Int (0..255); OOB throws.
                        if c.args.len() != 1 {
                            return Err(SemaError {
                                message: format!(
                                    "`String.charAt` expects 1 argument, got {}",
                                    c.args.len()
                                ),
                                span: c.span,
                            });
                        }
                        let at = self.check_expr(&c.args[0])?;
                        if at != Ty::Int {
                            return Err(SemaError {
                                message: format!(
                                    "`String.charAt` index must be Int, got {}",
                                    at.display()
                                ),
                                span: c.args[0].span(),
                            });
                        }
                        return Ok(if safe_wrap {
                            Ty::Nullable(Box::new(Ty::Int))
                        } else {
                            Ty::Int
                        });
                    }
                    // C5h/C5i/C5j: String predicate methods.
                    "startsWith" | "contains" | "endsWith" => {
                        let mname = fe.field.name.as_str();
                        if c.args.len() != 1 {
                            return Err(SemaError {
                                message: format!(
                                    "`String.{mname}` expects 1 argument, got {}",
                                    c.args.len()
                                ),
                                span: c.span,
                            });
                        }
                        let arg = self.check_expr(&c.args[0])?;
                        if arg != Ty::String {
                            return Err(SemaError {
                                message: format!(
                                    "`String.{mname}` argument must be String, got {}",
                                    arg.display()
                                ),
                                span: c.args[0].span(),
                            });
                        }
                        return Ok(if safe_wrap {
                            Ty::Nullable(Box::new(Ty::Bool))
                        } else {
                            Ty::Bool
                        });
                    }
                    // C12f: indexOf(sub) — UTF-8 byte index of first match; -1 if missing.
                    // Empty sub → 0 (like many languages / strstr).
                    "indexOf" => {
                        if c.args.len() != 1 {
                            return Err(SemaError {
                                message: format!(
                                    "`String.indexOf` expects 1 argument, got {}",
                                    c.args.len()
                                ),
                                span: c.span,
                            });
                        }
                        let arg = self.check_expr(&c.args[0])?;
                        if arg != Ty::String {
                            return Err(SemaError {
                                message: format!(
                                    "`String.indexOf` argument must be String, got {}",
                                    arg.display()
                                ),
                                span: c.args[0].span(),
                            });
                        }
                        return Ok(if safe_wrap {
                            Ty::Nullable(Box::new(Ty::Int))
                        } else {
                            Ty::Int
                        });
                    }
                    // C12g: split(sep) — byte/string separator → Array<String>.
                    // Empty sep rejected at runtime (throw String). Consecutive / trailing seps
                    // yield empty segments (JS-like). Segments are newly allocated owned copies.
                    "split" => {
                        if c.args.len() != 1 {
                            return Err(SemaError {
                                message: format!(
                                    "`String.split` expects 1 argument, got {}",
                                    c.args.len()
                                ),
                                span: c.span,
                            });
                        }
                        let arg = self.check_expr(&c.args[0])?;
                        if arg != Ty::String {
                            return Err(SemaError {
                                message: format!(
                                    "`String.split` argument must be String, got {}",
                                    arg.display()
                                ),
                                span: c.args[0].span(),
                            });
                        }
                        let arr = Ty::ClassApp {
                            name: "Array".into(),
                            args: vec![Ty::String],
                        };
                        self.note_mono_ty(&arr);
                        return Ok(if safe_wrap {
                            Ty::Nullable(Box::new(arr))
                        } else {
                            arr
                        });
                    }
                    // C12h: trim / trimStart / trimEnd — ASCII whitespace MVP
                    // (' ', '\t', '\n', '\r'). No args; returns newly allocated String.
                    "trim" | "trimStart" | "trimEnd" => {
                        let mname = fe.field.name.as_str();
                        if !c.args.is_empty() {
                            return Err(SemaError {
                                message: format!(
                                    "`String.{mname}` expects 0 arguments, got {}",
                                    c.args.len()
                                ),
                                span: c.span,
                            });
                        }
                        return Ok(if safe_wrap {
                            Ty::Nullable(Box::new(Ty::String))
                        } else {
                            Ty::String
                        });
                    }
                    // C12i: toInt() — parse entire string as decimal Int; invalid/overflow → null.
                    // No auto-trim (caller can trim first). Optional leading +/-; digits only.
                    "toInt" => {
                        if !c.args.is_empty() {
                            return Err(SemaError {
                                message: format!(
                                    "`String.toInt` expects 0 arguments, got {}",
                                    c.args.len()
                                ),
                                span: c.span,
                            });
                        }
                        // Already Int?; safe call on null receiver still yields null Int?
                        // (same tagged-opt rep; no double-wrap).
                        return Ok(Ty::Nullable(Box::new(Ty::Int)));
                    }
                    // C11d: exclusive-end substring (UTF-8 byte indices).
                    "substring" => {
                        if c.args.len() != 2 {
                            return Err(SemaError {
                                message: format!(
                                    "`String.substring` expects 2 arguments (start, end), got {}",
                                    c.args.len()
                                ),
                                span: c.span,
                            });
                        }
                        for (i, arg) in c.args.iter().enumerate() {
                            let t = self.check_expr(arg)?;
                            if t != Ty::Int {
                                return Err(SemaError {
                                    message: format!(
                                        "`String.substring` argument {} must be Int, got {}",
                                        i + 1,
                                        t.display()
                                    ),
                                    span: arg.span(),
                                });
                            }
                        }
                        return Ok(if safe_wrap {
                            Ty::Nullable(Box::new(Ty::String))
                        } else {
                            Ty::String
                        });
                    }
                    other => {
                        return Err(SemaError {
                            message: format!("unknown method `{other}` on `String`"),
                            span: fe.field.span,
                        });
                    }
                }
            }

            return Err(SemaError {
                message: format!(
                    "method call requires a class, interface, String, or bounded type parameter, got {}",
                    obj_ty.display()
                ),
                span: c.span,
            });
        }

        let name = match c.callee.as_ref() {
            Expr::Ident(id) => id.name.clone(),
            other => {
                // C10d: call through a function value expression.
                let callee_ty = self.check_expr(other)?;
                return self.check_fun_value_call(&callee_ty, c);
            }
        };

        // C10d: local (or param) of function type `f(x)`.
        if let Some(local) = self.lookup_local(&name) {
            if matches!(local.ty, Ty::Fun { .. }) {
                let ty = local.ty.clone();
                return self.check_fun_value_call(&ty, c);
            }
        }

        // Constructor (possibly generic)
        if self.classes.contains_key(&name) {
            let class = self.resolve_class(&name, c.callee.span())?;
            return self.check_class_ctor(&class, c, expected);
        }

        // Enum variant constructor: Ok(...), Red()
        if let Some(enum_name) = self.variant_to_enum.get(&name).cloned() {
            return self.check_variant_ctor(&enum_name, &name, c, expected);
        }

        // assert_eq(a, b) — same-type equality for Int/String/Bool (RFC-011 MVP)
        if name == "assert_eq" {
            if c.args.len() != 2 {
                return Err(SemaError {
                    message: "`assert_eq` expects 2 arguments".into(),
                    span: c.span,
                });
            }
            let a = self.check_expr(&c.args[0])?;
            let b = self.check_expr(&c.args[1])?;
            if a != b {
                return Err(SemaError {
                    message: format!(
                        "`assert_eq` type mismatch: {} vs {}",
                        a.display(),
                        b.display()
                    ),
                    span: c.span,
                });
            }
            match a {
                Ty::Int | Ty::String | Ty::Bool => Ok(Ty::Unit),
                other => Err(SemaError {
                    message: format!(
                        "`assert_eq` supports Int, String, Bool (got {})",
                        other.display()
                    ),
                    span: c.span,
                }),
            }
        } else {
            // Free function (possibly generic)
            let sig = self.resolve_fun(&name, c.callee.span())?;
            self.check_fun_call_with_sig(&sig, c, expected)
        }
    }

    /// Class/struct constructor (unqualified or `Alias.Type`, C3u).
    pub(crate) fn check_class_ctor(
        &mut self,
        class: &ClassSig,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Ty, SemaError> {
        let name = class.name.clone();
        self.check_visible(&name, class.is_pub, &class.package, c.callee.span())?;
        if c.args.len() != class.fields.len() {
            return Err(SemaError {
                message: format!(
                    "constructor `{}` expects {} argument(s), got {}",
                    name,
                    class.fields.len(),
                    c.args.len()
                ),
                span: c.span,
            });
        }

        let type_args = self.resolve_ctor_type_args(class, c, expected)?;
        self.check_type_args_bounds(
            &class.type_params,
            &class.bounds,
            &type_args,
            c.span,
            &format!("constructor `{}`", class.name),
        )?;
        if class.name == "Array" {
            self.check_array_type_args(&type_args, c.span)?;
        }

        let subst = type_subst_map(&class.type_params, &type_args);
        for (arg, field) in c.args.iter().zip(class.fields.iter()) {
            let exp = subst_ty(&field.ty, &subst);
            let got = self.check_expr_expected(arg, Some(&exp))?;
            if !self.is_assignable(&got, &exp) {
                return Err(SemaError {
                    message: format!(
                        "constructor argument for `{}`: expected {}, got {}",
                        field.name,
                        exp.display(),
                        got.display()
                    ),
                    span: arg.span(),
                });
            }
        }

        let key = nominal_key(&class.package, &name);
        // Always record so Alias.Type(...) codegen can emit a constructor (C3u/C3v).
        self.call_instantiations.insert(
            c.span.start,
            CallInstantiation {
                is_constructor: true,
                name: name.clone(),
                package: class.package.clone(),
                type_args: type_args.clone(),
                variant: None,
            },
        );
        if !type_args.is_empty() {
            let t = Ty::ClassApp {
                name: key.clone(),
                args: type_args.clone(),
            };
            self.note_mono_ty(&t);
        }

        let ret = if type_args.is_empty() {
            Ty::Class(key)
        } else {
            Ty::ClassApp {
                name: key,
                args: type_args,
            }
        };
        Ok(ret)
    }

    /// Shared path for free-function calls (unqualified or `Alias.fun`, C3n).
    pub(crate) fn check_fun_call_with_sig(
        &mut self,
        sig: &FunSig,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Ty, SemaError> {
        let name = sig.name.clone();
        let type_args = self.resolve_fun_type_args(sig, c, expected)?;
        self.check_type_args_bounds(
            &sig.type_params,
            &sig.bounds,
            &type_args,
            c.span,
            &format!("function `{}`", sig.name),
        )?;

        let subst = type_subst_map(&sig.type_params, &type_args);
        let params: Vec<Ty> = sig.params.iter().map(|p| subst_ty(p, &subst)).collect();
        let ret = subst_ty(&sig.ret, &subst);
        self.check_args(&params, &c.args, &name, c.span)?;
        // Always record target package for C3o C symbol mangling.
        self.call_instantiations.insert(
            c.span.start,
            CallInstantiation {
                is_constructor: false,
                name: name.clone(),
                package: sig.package.clone(),
                type_args: type_args.clone(),
                variant: None,
            },
        );
        // C4u: only concrete free-function monomorphs.
        if !type_args.is_empty() && !type_args.iter().any(|a| a.is_open()) {
            self.mono_funs.insert((name, type_args));
        }
        Ok(ret)
    }

    pub(crate) fn check_variant_ctor(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Ty, SemaError> {
        if !c.type_args.is_empty() {
            return Err(SemaError {
                message: "type arguments go on the enum type, not the variant constructor".into(),
                span: c.span,
            });
        }
        let enum_sig = self
            .enums
            .get(enum_name)
            .and_then(|v| {
                if v.len() == 1 {
                    Some(v[0].clone())
                } else {
                    v.iter()
                        .find(|s| s.package == self.current_package)
                        .cloned()
                }
            })
            .ok_or_else(|| SemaError {
                message: format!("unknown enum `{enum_name}`"),
                span: c.span,
            })?;
        self.check_visible(enum_name, enum_sig.is_pub, &enum_sig.package, c.span)?;
        let variant = enum_sig
            .variants
            .iter()
            .find(|v| v.name == variant_name)
            .cloned()
            .ok_or_else(|| SemaError {
                message: format!("unknown variant `{variant_name}`"),
                span: c.span,
            })?;
        if c.args.len() != variant.fields.len() {
            return Err(SemaError {
                message: format!(
                    "variant `{variant_name}` expects {} argument(s), got {}",
                    variant.fields.len(),
                    c.args.len()
                ),
                span: c.span,
            });
        }

        let type_args = self.resolve_enum_type_args(&enum_sig, &variant, c, expected)?;
        self.check_type_args_bounds(
            &enum_sig.type_params,
            &enum_sig.bounds,
            &type_args,
            c.span,
            &format!("enum `{enum_name}`"),
        )?;
        let subst = type_subst_map(&enum_sig.type_params, &type_args);
        for (arg, (_fname, fty)) in c.args.iter().zip(variant.fields.iter()) {
            let exp = subst_ty(fty, &subst);
            let got = self.check_expr_expected(arg, Some(&exp))?;
            if !self.is_assignable(&got, &exp) {
                return Err(SemaError {
                    message: format!(
                        "argument type mismatch for `{variant_name}`: expected {}, got {}",
                        exp.display(),
                        got.display()
                    ),
                    span: arg.span(),
                });
            }
        }

        let key = nominal_key(&enum_sig.package, enum_name);
        let ret = if type_args.is_empty() {
            Ty::Enum(key.clone())
        } else {
            let t = Ty::EnumApp {
                name: key,
                args: type_args.clone(),
            };
            self.note_mono_ty(&t);
            t
        };
        self.call_instantiations.insert(
            c.span.start,
            CallInstantiation {
                is_constructor: true,
                name: enum_name.to_string(),
                package: enum_sig.package.clone(),
                type_args,
                variant: Some(variant_name.to_string()),
            },
        );
        Ok(ret)
    }

    pub(crate) fn resolve_enum_type_args(
        &mut self,
        enum_sig: &EnumSig,
        variant: &EnumVariantSig,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Vec<Ty>, SemaError> {
        if enum_sig.type_params.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(Ty::EnumApp { name, args }) = expected {
            let (simple, _) = crate::ty::split_nominal(name);
            if simple == enum_sig.name && args.len() == enum_sig.type_params.len() {
                return Ok(args.clone());
            }
        }
        if let Some(Ty::Enum(name)) = expected {
            let (simple, _) = crate::ty::split_nominal(name);
            if simple == enum_sig.name && enum_sig.type_params.is_empty() {
                return Ok(Vec::new());
            }
        }
        let mut arg_tys = Vec::new();
        for a in &c.args {
            arg_tys.push(self.check_expr(a)?);
        }
        let patterns: Vec<&Ty> = variant.fields.iter().map(|(_, t)| t).collect();
        self.infer_type_args_from_patterns(
            &enum_sig.type_params,
            &patterns,
            &arg_tys,
            c.span,
            &format!("variant `{}`", variant.name),
        )
    }

    pub(crate) fn check_array_type_args(
        &self,
        type_args: &[Ty],
        span: Span,
    ) -> Result<(), SemaError> {
        if type_args.len() != 1 {
            return Err(SemaError {
                message: format!("`Array` expects 1 type argument, got {}", type_args.len()),
                span,
            });
        }
        if !self.is_array_element_ty(&type_args[0]) {
            // C4x: dedicated message for interface (no layout in Array yet).
            // C6g: enum elements are allowed; keep interface reject clear.
            let detail = match &type_args[0] {
                Ty::Interface(n) | Ty::InterfaceApp { name: n, .. } => {
                    let (simple, _) = crate::ty::split_nominal(n);
                    format!(
                        "`Array` of interface `{simple}` is not supported yet (elements must be Int, Bool, String, class, struct, enum, or Array)"
                    )
                }
                other => format!(
                    "`Array` element type must be Int, Bool, String, class, struct, enum, or Array (got {})",
                    other.display()
                ),
            };
            return Err(SemaError {
                message: detail,
                span,
            });
        }
        Ok(())
    }

    /// C4c/C4q/C6g/C8e: primitives + heap classes + structs + enums + nested Array (not interface).
    /// C8a: type params allowed in generic class/fun fields (mono becomes concrete).
    pub(crate) fn is_array_element_ty(&self, ty: &Ty) -> bool {
        if is_array_primitive_elem(ty) {
            return true;
        }
        match ty {
            // Open mono skipped at record time (C4u); concrete mono uses substituted elem.
            Ty::TypeParam(_) => true,
            // Nested Array: Array<Array<T>> (elem must itself be a valid Array mono).
            Ty::ClassApp { name, args }
                if crate::ty::split_nominal(name).0 == "Array" && args.len() == 1 =>
            {
                self.is_array_element_ty(&args[0])
            }
            Ty::Class(n) | Ty::ClassApp { name: n, .. } => {
                let (simple, pkg) = crate::ty::split_nominal(n);
                let list = match self.classes.get(simple) {
                    Some(l) => l,
                    None => return false,
                };
                list.iter().any(|c| {
                    pkg.is_empty() || c.package == pkg || (c.package.is_empty() && pkg.is_empty())
                })
            }
            Ty::Enum(n) | Ty::EnumApp { name: n, .. } => {
                let (simple, pkg) = crate::ty::split_nominal(n);
                let list = match self.enums.get(simple) {
                    Some(l) => l,
                    None => return false,
                };
                list.iter().any(|e| {
                    pkg.is_empty() || e.package == pkg || (e.package.is_empty() && pkg.is_empty())
                })
            }
            _ => false,
        }
    }

    pub(crate) fn resolve_ctor_type_args(
        &mut self,
        class: &ClassSig,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Vec<Ty>, SemaError> {
        let what = format!("type `{}`", class.name);
        if class.type_params.is_empty() {
            if !c.type_args.is_empty() {
                return Err(SemaError {
                    message: format!("{what} does not take type arguments"),
                    span: c.span,
                });
            }
            return Ok(Vec::new());
        }
        if !c.type_args.is_empty() {
            if c.type_args.len() != class.type_params.len() {
                return Err(SemaError {
                    message: format!(
                        "{what} expects {} type argument(s), got {}",
                        class.type_params.len(),
                        c.type_args.len()
                    ),
                    span: c.span,
                });
            }
            return c
                .type_args
                .iter()
                .map(|t| self.type_from_ref(t))
                .collect::<Result<Vec<_>, _>>();
        }
        if let Some(Ty::ClassApp { name: n, args }) = expected {
            let (simple, _) = crate::ty::split_nominal(n);
            if simple == class.name && args.len() == class.type_params.len() {
                return Ok(args.clone());
            }
        }
        let mut arg_tys = Vec::new();
        for a in &c.args {
            arg_tys.push(self.check_expr(a)?);
        }
        let patterns: Vec<&Ty> = class.fields.iter().map(|f| &f.ty).collect();
        self.infer_type_args_from_patterns(
            &class.type_params,
            &patterns,
            &arg_tys,
            c.span,
            &format!("constructor `{}`", class.name),
        )
    }

    pub(crate) fn resolve_fun_type_args(
        &mut self,
        sig: &FunSig,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Vec<Ty>, SemaError> {
        let what = format!("function `{}`", sig.name);
        if sig.type_params.is_empty() {
            if !c.type_args.is_empty() {
                return Err(SemaError {
                    message: format!("{what} does not take type arguments"),
                    span: c.span,
                });
            }
            return Ok(Vec::new());
        }
        if !c.type_args.is_empty() {
            if c.type_args.len() != sig.type_params.len() {
                return Err(SemaError {
                    message: format!(
                        "{what} expects {} type argument(s), got {}",
                        sig.type_params.len(),
                        c.type_args.len()
                    ),
                    span: c.span,
                });
            }
            return c
                .type_args
                .iter()
                .map(|t| self.type_from_ref(t))
                .collect::<Result<Vec<_>, _>>();
        }
        if let Some(exp) = expected {
            let mut map = HashMap::new();
            if unify_ty(&sig.ret, exp, &mut map).is_ok() {
                let mut args = Vec::new();
                let mut ok = true;
                for p in &sig.type_params {
                    if let Some(t) = map.get(p) {
                        if *t == Ty::Null {
                            ok = false;
                            break;
                        }
                        args.push(t.clone());
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok && args.len() == sig.type_params.len() {
                    return Ok(args);
                }
            }
        }
        let mut arg_tys = Vec::new();
        for a in &c.args {
            arg_tys.push(self.check_expr(a)?);
        }
        if arg_tys.len() != sig.params.len() {
            return Err(SemaError {
                message: format!(
                    "`{}` expects {} argument(s), got {}",
                    sig.name,
                    sig.params.len(),
                    arg_tys.len()
                ),
                span: c.span,
            });
        }
        let patterns: Vec<&Ty> = sig.params.iter().collect();
        self.infer_type_args_from_patterns(&sig.type_params, &patterns, &arg_tys, c.span, &what)
    }

    pub(crate) fn infer_type_args_from_patterns(
        &self,
        type_params: &[String],
        patterns: &[&Ty],
        concretes: &[Ty],
        span: Span,
        what: &str,
    ) -> Result<Vec<Ty>, SemaError> {
        let mut map = HashMap::new();
        for (pat, con) in patterns.iter().zip(concretes.iter()) {
            if let Err(msg) = unify_ty(pat, con, &mut map) {
                return Err(SemaError {
                    message: format!("{what}: {msg}"),
                    span,
                });
            }
        }
        let mut out = Vec::new();
        for p in type_params {
            match map.get(p) {
                Some(t) if *t != Ty::Null => out.push(t.clone()),
                _ => {
                    return Err(SemaError {
                        message: format!(
                            "cannot infer type argument `{p}` for {what}; write it explicitly (e.g. `<…>`)"
                        ),
                        span,
                    });
                }
            }
        }
        Ok(out)
    }

    pub(crate) fn check_args(
        &mut self,
        params: &[Ty],
        args: &[Expr],
        name: &str,
        span: Span,
    ) -> Result<(), SemaError> {
        if args.len() != params.len() {
            return Err(SemaError {
                message: format!(
                    "`{name}` expects {} argument(s), got {}",
                    params.len(),
                    args.len()
                ),
                span,
            });
        }
        for (arg, expected) in args.iter().zip(params.iter()) {
            let got = self.check_expr_expected(arg, Some(expected))?;
            if !self.is_assignable(&got, expected) {
                return Err(SemaError {
                    message: format!(
                        "argument type mismatch for `{name}`: expected {}, got {}",
                        expected.display(),
                        got.display()
                    ),
                    span: arg.span(),
                });
            }
        }
        Ok(())
    }

    /// C10d: call a first-class function value `(params) -> ret`.
    pub(crate) fn check_fun_value_call(
        &mut self,
        callee_ty: &Ty,
        c: &CallExpr,
    ) -> Result<Ty, SemaError> {
        let Ty::Fun { params, ret } = callee_ty else {
            return Err(SemaError {
                message: format!("value of type {} is not callable", callee_ty.display()),
                span: c.callee.span(),
            });
        };
        if !c.type_args.is_empty() {
            return Err(SemaError {
                message: "type arguments not allowed on function-value calls".into(),
                span: c.span,
            });
        }
        self.check_args(params, &c.args, "function value", c.span)?;
        Ok(ret.as_ref().clone())
    }
}
