use aura_ast::TypeRef;

use super::Checker;
use crate::error::SemaError;
use crate::ty::{nominal_key, Ty};
impl Checker {
    pub(crate) fn type_from_ref(&self, t: &TypeRef) -> Result<Ty, SemaError> {
        let type_args: Vec<Ty> = t
            .type_args
            .iter()
            .map(|a| self.type_from_ref(a))
            .collect::<Result<Vec<_>, _>>()?;

        // C3u: `Alias.Type` — resolve type in the package bound to Alias.
        let qualified_pkg = if let Some(q) = &t.qualifier {
            let pkg = self.import_aliases.get(&q.name).ok_or_else(|| SemaError {
                message: format!("unknown package alias `{}` in type", q.name),
                span: q.span,
            })?;
            Some(pkg.as_str())
        } else {
            None
        };

        let base = match t.name.name.as_str() {
            "Unit" | "Int" | "Bool" | "String" if qualified_pkg.is_some() => {
                return Err(SemaError {
                    message: format!(
                        "primitive type `{}` cannot be package-qualified",
                        t.name.name
                    ),
                    span: t.span,
                });
            }
            "Unit" => Ty::Unit,
            "Int" => Ty::Int,
            "Bool" => Ty::Bool,
            "String" => Ty::String,
            other if self.type_params.contains_key(other) => {
                if qualified_pkg.is_some() {
                    return Err(SemaError {
                        message: format!("type parameter `{other}` cannot be package-qualified"),
                        span: t.span,
                    });
                }
                if !type_args.is_empty() {
                    return Err(SemaError {
                        message: format!("type parameter `{other}` cannot take type arguments"),
                        span: t.span,
                    });
                }
                Ty::TypeParam(other.to_string())
            }
            other if self.classes.contains_key(other) => {
                let class = if let Some(pkg) = qualified_pkg {
                    self.resolve_class_in_package(other, pkg, t.span)?
                } else {
                    self.resolve_class(other, t.span)?
                };
                if type_args.len() != class.type_params.len() {
                    return Err(SemaError {
                        message: format!(
                            "type `{}` expects {} type argument(s), got {}",
                            other,
                            class.type_params.len(),
                            type_args.len()
                        ),
                        span: t.span,
                    });
                }
                if !type_args.is_empty() {
                    self.check_type_args_bounds(
                        &class.type_params,
                        &class.bounds,
                        &type_args,
                        t.span,
                        &format!("type `{other}`"),
                    )?;
                }
                if other == "Array" {
                    self.check_array_type_args(&type_args, t.span)?;
                }
                let key = nominal_key(&class.package, other);
                if type_args.is_empty() {
                    Ty::Class(key)
                } else {
                    Ty::ClassApp {
                        name: key,
                        args: type_args,
                    }
                }
            }
            other if self.enums.contains_key(other) => {
                let enum_sig = if let Some(pkg) = qualified_pkg {
                    self.enum_in_package(other, pkg)
                        .cloned()
                        .ok_or_else(|| SemaError {
                            message: format!(
                                "type `{other}` is not a member of package `{pkg}`"
                            ),
                            span: t.span,
                        })?
                } else {
                    self.resolve_enum(other, t.span)?
                };
                if let Some(pkg) = qualified_pkg {
                    self.check_visible(other, enum_sig.is_pub, &enum_sig.package, t.span)?;
                    let _ = pkg;
                }
                if type_args.len() != enum_sig.type_params.len() {
                    return Err(SemaError {
                        message: format!(
                            "type `{}` expects {} type argument(s), got {}",
                            other,
                            enum_sig.type_params.len(),
                            type_args.len()
                        ),
                        span: t.span,
                    });
                }
                if !type_args.is_empty() {
                    self.check_type_args_bounds(
                        &enum_sig.type_params,
                        &enum_sig.bounds,
                        &type_args,
                        t.span,
                        &format!("type `{other}`"),
                    )?;
                }
                let key = nominal_key(&enum_sig.package, other);
                if type_args.is_empty() {
                    Ty::Enum(key)
                } else {
                    Ty::EnumApp {
                        name: key,
                        args: type_args,
                    }
                }
            }
            other if self.interfaces.contains_key(other) => {
                let iface = self.interfaces.get(other).unwrap();
                if let Some(pkg) = qualified_pkg {
                    if iface.package != pkg {
                        return Err(SemaError {
                            message: format!(
                                "type `{other}` is not a member of package `{pkg}`"
                            ),
                            span: t.span,
                        });
                    }
                }
                self.check_visible(other, iface.is_pub, &iface.package, t.span)?;
                if !type_args.is_empty() {
                    return Err(SemaError {
                        message: format!("interface `{other}` cannot take type arguments in C2b"),
                        span: t.span,
                    });
                }
                Ty::Interface(other.to_string())
            }
            other => {
                return Err(SemaError {
                    message: if let Some(pkg) = qualified_pkg {
                        format!("unknown type `{other}` in package `{pkg}`")
                    } else {
                        format!("unknown type `{other}`")
                    },
                    span: t.span,
                });
            }
        };
        if t.nullable {
            if matches!(base, Ty::Unit) {
                return Err(SemaError {
                    message: "`Unit?` is not allowed".into(),
                    span: t.span,
                });
            }
            Ok(Ty::Nullable(Box::new(base)))
        } else {
            Ok(base)
        }
    }

    pub(crate) fn is_assignable(&self, from: &Ty, to: &Ty) -> bool {
        if from == to {
            return true;
        }
        match (from, to) {
            (Ty::Null, Ty::Nullable(_)) => true,
            (Ty::Nullable(a), Ty::Nullable(b)) => self.is_assignable(a, b),
            (inner, Ty::Nullable(outer)) if self.is_assignable(inner, outer) => true,
            (Ty::Class(c), Ty::Interface(i)) => self
                .class_by_nominal_key(c)
                .map(|cs| cs.implements.iter().any(|x| x == i))
                .unwrap_or(false),
            (Ty::ClassApp { name: c, .. }, Ty::Interface(i)) => self
                .class_by_nominal_key(c)
                .map(|cs| cs.implements.iter().any(|x| x == i))
                .unwrap_or(false),
            // Bounded type param is assignable to its interface bounds
            (Ty::TypeParam(p), Ty::Interface(i)) => self
                .type_params
                .get(p)
                .map(|bs| bs.iter().any(|x| x == i))
                .unwrap_or(false),
            // Type params only match themselves (handled by ==)
            _ => false,
        }
    }
}
