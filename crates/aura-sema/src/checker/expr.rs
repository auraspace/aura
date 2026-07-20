use aura_ast::{BinOp, Expr, UnOp};

use super::Checker;
use crate::error::SemaError;
use crate::ty::Ty;
use crate::util::{eq_compatible, subst_ty, type_subst_map};

impl Checker {
    pub(crate) fn note_mono_ty(&mut self, ty: &Ty) {
        match ty {
            // C4u: only concrete monomorphs — skip open `Box<T>` from generic method bodies.
            Ty::ClassApp { name, args } if !args.is_empty() => {
                if args.iter().any(|a| a.is_open()) {
                    return;
                }
                let (simple, _) = crate::ty::split_nominal(name);
                let key = (simple.to_string(), args.clone());
                if !self.mono_classes.insert(key) {
                    return;
                }
                // C8e: Array elem monomorphs first (Array_Array_Int needs Array_Int).
                if simple == "Array" {
                    for a in args {
                        self.note_mono_ty(a);
                    }
                } else {
                    // Nested field types (Wrapper_String → Box_String) for codegen.
                    self.expand_nested_mono(simple, args);
                }
            }
            Ty::EnumApp { name, args } if !args.is_empty() => {
                if args.iter().any(|a| a.is_open()) {
                    return;
                }
                let (simple, _) = crate::ty::split_nominal(name);
                self.mono_enums.insert((simple.to_string(), args.clone()));
            }
            Ty::InterfaceApp { name, args } if !args.is_empty() => {
                if args.iter().any(|a| a.is_open()) {
                    return;
                }
                let (simple, _) = crate::ty::split_nominal(name);
                self.mono_interfaces
                    .insert((simple.to_string(), args.clone()));
                for a in args {
                    self.note_mono_ty(a);
                }
            }
            Ty::Nullable(inner) => self.note_mono_ty(inner),
            _ => {}
        }
    }

    /// After recording a concrete class mono, note monomorphs of field types under substitution.
    fn expand_nested_mono(&mut self, simple: &str, args: &[Ty]) {
        let sig = self.classes.get(simple).and_then(|v| {
            if v.len() == 1 {
                Some(v[0].clone())
            } else {
                // Prefer current package; else first match with same type_param arity.
                v.iter()
                    .find(|s| s.package == self.current_package)
                    .or_else(|| v.iter().find(|s| s.type_params.len() == args.len()))
                    .cloned()
            }
        });
        let Some(sig) = sig else {
            return;
        };
        if sig.type_params.len() != args.len() {
            return;
        }
        let map = type_subst_map(&sig.type_params, args);
        for f in &sig.fields {
            let ft = subst_ty(&f.ty, &map);
            self.note_mono_ty(&ft);
        }
        // C9a: note mono interfaces from `implements` after class type-arg subst
        // (`Box<Int>` → `Boxable<Int>`).
        for imp in &sig.implements {
            let it = subst_ty(imp, &map);
            self.note_mono_ty(&it);
        }
    }

    pub(crate) fn check_expr(&mut self, expr: &Expr) -> Result<Ty, SemaError> {
        self.check_expr_expected(expr, None)
    }

    pub(crate) fn check_expr_expected(
        &mut self,
        expr: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Ty, SemaError> {
        match expr {
            Expr::Ident(id) => {
                if let Some((frame, local)) = self.lookup_local_frame(&id.name) {
                    let ty = local.ty.clone();
                    let mutable = local.mutable;
                    self.note_lambda_capture(&id.name, frame, &ty, mutable, id.span)?;
                    return Ok(ty);
                }
                // C9g: top-level const.
                if let Some(entries) = self.consts.get(&id.name) {
                    let ty = if entries.len() == 1 {
                        entries[0].1.clone()
                    } else {
                        entries
                            .iter()
                            .find(|(p, _)| p == &self.current_package)
                            .map(|(_, t)| t.clone())
                            .unwrap_or_else(|| entries[0].1.clone())
                    };
                    return Ok(ty);
                }
                if self.import_aliases.contains_key(&id.name) {
                    return Err(SemaError {
                        message: format!(
                            "package alias `{}` cannot be used as a value (use `{}.member`)",
                            id.name, id.name
                        ),
                        span: id.span,
                    });
                }
                // C5c: suggest similar local / function / type names.
                let mut msg = format!("undefined name `{}`", id.name);
                if let Some(hint) = self.suggest_name(&id.name) {
                    msg.push_str(&format!("; did you mean `{hint}`?"));
                }
                Err(SemaError {
                    message: msg,
                    span: id.span,
                })
            }
            Expr::This(span) => {
                let class = self.current_class.as_ref().ok_or_else(|| SemaError {
                    message: "`this` is only valid inside methods".into(),
                    span: *span,
                })?;
                // Inside generic class, this is the open Class with type params as TypeParam
                let sig = self
                    .class_in_package(class, &self.current_package)
                    .or_else(|| {
                        self.classes.get(class).and_then(|v| {
                            if v.len() == 1 {
                                Some(&v[0])
                            } else {
                                None
                            }
                        })
                    })
                    .unwrap();
                let key = crate::ty::nominal_key(&sig.package, class);
                if sig.type_params.is_empty() {
                    Ok(Ty::Class(key))
                } else {
                    Ok(Ty::ClassApp {
                        name: key,
                        args: sig
                            .type_params
                            .iter()
                            .map(|p| Ty::TypeParam(p.clone()))
                            .collect(),
                    })
                }
            }
            Expr::Int(_) => Ok(Ty::Int),
            Expr::Bool(_) => Ok(Ty::Bool),
            Expr::String(_) => Ok(Ty::String),
            Expr::Null(_) => Ok(Ty::Null),
            Expr::Group(inner, _) => self.check_expr_expected(inner, expected),
            Expr::If(i) => {
                // C4t: if-expression — both branches yield same type from last expr.
                let ct = self.check_expr(&i.cond)?;
                if ct != Ty::Bool {
                    return Err(SemaError {
                        message: format!("if condition must be Bool, got {}", ct.display()),
                        span: i.cond.span(),
                    });
                }
                let then_ty = self.block_result_ty(&i.then_block)?;
                let else_ty = self.block_result_ty(&i.else_block)?;
                if !self.is_assignable(&then_ty, &else_ty)
                    && !self.is_assignable(&else_ty, &then_ty)
                {
                    return Err(SemaError {
                        message: format!(
                            "if-expression branches have incompatible types {} and {}",
                            then_ty.display(),
                            else_ty.display()
                        ),
                        span: i.span,
                    });
                }
                // Prefer non-null / more specific when one assignable to other.
                if self.is_assignable(&then_ty, &else_ty) {
                    Ok(else_ty)
                } else {
                    Ok(then_ty)
                }
            }
            Expr::ForceUnwrap(f) => {
                let t = self.check_expr(&f.expr)?;
                match t {
                    Ty::Nullable(inner) => Ok(*inner),
                    Ty::Null => Err(SemaError {
                        message: "cannot force-unwrap `null`".into(),
                        span: f.span,
                    }),
                    other => Ok(other), // already non-null; !! is a no-op
                }
            }
            // C9i: `expr is Type` → Bool
            Expr::Is(i) => {
                let _ = self.check_expr(&i.expr)?;
                let target = self.type_from_ref(&i.ty)?;
                match &target {
                    Ty::Class(_)
                    | Ty::ClassApp { .. }
                    | Ty::Interface(_)
                    | Ty::InterfaceApp { .. } => {}
                    other => {
                        return Err(SemaError {
                            message: format!(
                                "`is` target must be a class or interface type, got {}",
                                other.display()
                            ),
                            span: i.ty.span,
                        });
                    }
                }
                if i.ty.nullable {
                    return Err(SemaError {
                        message: "`is` target cannot be nullable".into(),
                        span: i.ty.span,
                    });
                }
                Ok(Ty::Bool)
            }
            Expr::Field(f) => {
                let obj_ty = self.check_expr(&f.object)?;
                // C4s: `?.` requires nullable receiver; result is nullable unless already.
                let (obj_ty, safe_wrap) = if f.safe {
                    match &obj_ty {
                        Ty::Nullable(inner) => (inner.as_ref().clone(), true),
                        Ty::Null => {
                            return Err(SemaError {
                                message: "`?.` on null literal is not useful".into(),
                                span: f.span,
                            });
                        }
                        other => {
                            return Err(SemaError {
                                message: format!(
                                    "`?.` requires a nullable receiver, got {}",
                                    other.display()
                                ),
                                span: f.span,
                            });
                        }
                    }
                } else {
                    (obj_ty, false)
                };
                // C4p: String.len — UTF-8 byte length.
                if obj_ty == Ty::String && f.field.name == "len" {
                    let t = Ty::Int;
                    return Ok(if safe_wrap {
                        Ty::Nullable(Box::new(t))
                    } else {
                        t
                    });
                }
                if let Some(cname) = obj_ty.class_name() {
                    let key = match &obj_ty {
                        Ty::Class(k) | Ty::ClassApp { name: k, .. } => k.as_str(),
                        _ => cname,
                    };
                    let class =
                        self.class_by_nominal_key(key)
                            .cloned()
                            .ok_or_else(|| SemaError {
                                message: format!("unknown class `{cname}`"),
                                span: f.span,
                            })?;
                    let subst = type_subst_map(&class.type_params, obj_ty.class_args());
                    if let Some(field) = class.fields.iter().find(|x| x.name == f.field.name) {
                        let t = subst_ty(&field.ty, &subst);
                        return Ok(if safe_wrap {
                            Ty::Nullable(Box::new(t))
                        } else {
                            t
                        });
                    }
                    if class.methods.contains_key(&f.field.name) {
                        return Err(SemaError {
                            message: format!(
                                "method `{}` must be called (use `.{}()`)",
                                f.field.name, f.field.name
                            ),
                            span: f.field.span,
                        });
                    }
                    return Err(SemaError {
                        message: format!("unknown field `{}` on `{cname}`", f.field.name),
                        span: f.field.span,
                    });
                }
                if let Ty::Interface(iface_name) = &obj_ty {
                    let iface = self
                        .iface_by_nominal_key(iface_name)
                        .ok_or_else(|| SemaError {
                            message: format!("unknown interface `{iface_name}`"),
                            span: f.span,
                        })?;
                    if iface.methods.contains_key(&f.field.name) {
                        return Err(SemaError {
                            message: format!(
                                "interface method `{}` must be called (use `.{}()`)",
                                f.field.name, f.field.name
                            ),
                            span: f.field.span,
                        });
                    }
                    return Err(SemaError {
                        message: format!(
                            "unknown member `{}` on interface `{iface_name}`",
                            f.field.name
                        ),
                        span: f.field.span,
                    });
                }
                Err(SemaError {
                    message: format!(
                        "field access requires a class or interface type, got {}",
                        obj_ty.display()
                    ),
                    span: f.span,
                })
            }
            Expr::Unary(u) => {
                let t = self.check_expr(&u.expr)?;
                match u.op {
                    UnOp::Neg => {
                        if t != Ty::Int {
                            return Err(SemaError {
                                message: format!("unary `-` requires Int, got {}", t.display()),
                                span: u.span,
                            });
                        }
                        Ok(Ty::Int)
                    }
                    UnOp::Not => {
                        if t != Ty::Bool {
                            return Err(SemaError {
                                message: format!("unary `!` requires Bool, got {}", t.display()),
                                span: u.span,
                            });
                        }
                        Ok(Ty::Bool)
                    }
                }
            }
            Expr::Binary(b) => {
                let l = self.check_expr(&b.left)?;
                let r = self.check_expr(&b.right)?;
                match b.op {
                    BinOp::Add => {
                        // C9d: String + String → String (heap concat in codegen).
                        if l == Ty::String && r == Ty::String {
                            return Ok(Ty::String);
                        }
                        if l != Ty::Int || r != Ty::Int {
                            return Err(SemaError {
                                message: format!(
                                    "operator `+` requires Int or String operands, got {} and {}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Int)
                    }
                    BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                        if l != Ty::Int || r != Ty::Int {
                            return Err(SemaError {
                                message: format!(
                                    "arithmetic requires Int operands, got {} and {}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Int)
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        if l != Ty::Int || r != Ty::Int {
                            return Err(SemaError {
                                message: format!(
                                    "comparison requires Int operands, got {} and {}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Bool)
                    }
                    BinOp::Eq | BinOp::Ne => {
                        // C4i: reject struct/enum/interface equality (no C aggregate ==).
                        if self.is_struct_ty(&l) || self.is_struct_ty(&r) {
                            return Err(SemaError {
                                message: format!(
                                    "cannot compare struct values with `==`/`!=` (got {} and {}); compare fields instead",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        if !eq_compatible(&l, &r) {
                            let hint = if matches!(
                                (&l, &r),
                                (Ty::Enum(_), _)
                                    | (_, Ty::Enum(_))
                                    | (Ty::EnumApp { .. }, _)
                                    | (_, Ty::EnumApp { .. })
                                    | (Ty::Interface(_), _)
                                    | (_, Ty::Interface(_))
                            ) {
                                " (enum/interface equality is not supported in MVP)"
                            } else {
                                ""
                            };
                            return Err(SemaError {
                                message: format!(
                                    "cannot compare {} and {}{hint}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Bool)
                    }
                    BinOp::And | BinOp::Or => {
                        if l != Ty::Bool || r != Ty::Bool {
                            return Err(SemaError {
                                message: format!(
                                    "logical op requires Bool operands, got {} and {}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Bool)
                    }
                    BinOp::Coalesce => {
                        // C4m: `a ?: b` — a is T?, b assignable to T; result T.
                        let Ty::Nullable(inner) = &l else {
                            return Err(SemaError {
                                message: format!(
                                    "`?:` left operand must be nullable, got {}",
                                    l.display()
                                ),
                                span: b.span,
                            });
                        };
                        if !self.is_assignable(&r, inner) {
                            return Err(SemaError {
                                message: format!(
                                    "`?:` right operand type {} is not assignable to {}",
                                    r.display(),
                                    inner.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(inner.as_ref().clone())
                    }
                }
            }
            Expr::Assign(a) => {
                let local = self.lookup_local(&a.name.name).ok_or_else(|| SemaError {
                    message: format!("undefined name `{}`", a.name.name),
                    span: a.name.span,
                })?;
                if !local.mutable {
                    return Err(SemaError {
                        message: format!("cannot assign to immutable binding `{}`", a.name.name),
                        span: a.span,
                    });
                }
                let target = local.ty.clone();
                let value_ty = self.check_expr_expected(&a.value, Some(&target))?;
                if !self.is_assignable(&value_ty, &target) {
                    // C5k: explicit expected/found wording for assign mismatches.
                    return Err(SemaError {
                        message: format!(
                            "type mismatch assigning to `{}`: expected {}, found {}",
                            a.name.name,
                            target.display(),
                            value_ty.display()
                        ),
                        span: a.value.span(),
                    });
                }
                Ok(target)
            }
            Expr::Call(c) => self.check_call(c, expected),
            // C10d/g/h: lambda (expr/block body); C10h outer `val` captures.
            Expr::Lambda(l) => {
                let mut param_tys = Vec::with_capacity(l.params.len());
                self.locals.push(std::collections::HashMap::new());
                for p in &l.params {
                    let ty = self.type_from_ref(&p.ty)?;
                    if self.current_locals().contains_key(&p.name.name) {
                        self.locals.pop();
                        return Err(SemaError {
                            message: format!("duplicate lambda parameter `{}`", p.name.name),
                            span: p.name.span,
                        });
                    }
                    self.current_locals_mut().insert(
                        p.name.name.clone(),
                        super::Local {
                            ty: ty.clone(),
                            mutable: false,
                        },
                    );
                    param_tys.push(ty);
                }
                // C10h: params frame is the capture base; outer frames are free vars.
                let prev_base = self.lambda_capture_base.replace(self.locals.len() - 1);
                let prev_acc = self
                    .lambda_captures_acc
                    .replace(std::collections::HashMap::new());
                let expected_ret = match expected {
                    Some(Ty::Fun { ret, .. }) => Some(ret.as_ref()),
                    _ => None,
                };
                let body_result = match &l.body {
                    aura_ast::LambdaBody::Expr(body) => {
                        self.check_expr_expected(body, expected_ret)
                    }
                    aura_ast::LambdaBody::Block(block) => {
                        if let Some(exp_ret) = expected_ret {
                            self.check_block(block, exp_ret).map(|_| exp_ret.clone())
                        } else {
                            let prev = self.ret_infer.take();
                            self.ret_infer = Some(None);
                            let r = self
                                .check_block(block, &Ty::Unit)
                                .map(|_| self.ret_infer.take().and_then(|s| s).unwrap_or(Ty::Unit));
                            self.ret_infer = prev;
                            r
                        }
                    }
                };
                let body_ty = match body_result {
                    Ok(t) => t,
                    Err(e) => {
                        self.lambda_capture_base = prev_base;
                        self.lambda_captures_acc = prev_acc;
                        self.locals.pop();
                        return Err(e);
                    }
                };
                let captures_map = self.lambda_captures_acc.take().unwrap_or_default();
                self.lambda_capture_base = prev_base;
                self.lambda_captures_acc = prev_acc;
                self.locals.pop();
                let mut captures: Vec<(String, Ty)> = captures_map.into_iter().collect();
                captures.sort_by(|a, b| a.0.cmp(&b.0));
                self.lambda_captures.insert(l.span.start, captures);
                if let Some(exp_ret) = expected_ret {
                    if !self.is_assignable(&body_ty, exp_ret) {
                        let span = match &l.body {
                            aura_ast::LambdaBody::Expr(e) => e.span(),
                            aura_ast::LambdaBody::Block(b) => b.span,
                        };
                        return Err(SemaError {
                            message: format!(
                                "lambda body type mismatch: expected {}, found {}",
                                exp_ret.display(),
                                body_ty.display()
                            ),
                            span,
                        });
                    }
                }
                let fun_ty = Ty::Fun {
                    params: param_tys,
                    ret: Box::new(body_ty),
                };
                let result = if let Some(exp) = expected {
                    if !self.is_assignable(&fun_ty, exp) {
                        return Err(SemaError {
                            message: format!(
                                "type mismatch: expected {}, found {}",
                                exp.display(),
                                fun_ty.display()
                            ),
                            span: l.span,
                        });
                    }
                    exp.clone()
                } else {
                    fun_ty
                };
                self.lambda_tys.insert(l.span.start, result.clone());
                Ok(result)
            }
        }
    }
}
