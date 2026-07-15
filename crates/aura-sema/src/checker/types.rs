use aura_ast::TypeRef;

use super::Checker;
use crate::error::SemaError;
use crate::ty::Ty;
impl Checker {
    pub(crate) fn type_from_ref(&self, t: &TypeRef) -> Result<Ty, SemaError> {
        let type_args: Vec<Ty> = t
            .type_args
            .iter()
            .map(|a| self.type_from_ref(a))
            .collect::<Result<Vec<_>, _>>()?;

        let base = match t.name.name.as_str() {
            "Unit" => Ty::Unit,
            "Int" => Ty::Int,
            "Bool" => Ty::Bool,
            "String" => Ty::String,
            other if self.type_params.contains_key(other) => {
                if !type_args.is_empty() {
                    return Err(SemaError {
                        message: format!("type parameter `{other}` cannot take type arguments"),
                        span: t.span,
                    });
                }
                Ty::TypeParam(other.to_string())
            }
            other if self.classes.contains_key(other) => {
                let class = self.classes.get(other).unwrap().clone();
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
                if type_args.is_empty() {
                    Ty::Class(other.to_string())
                } else {
                    Ty::ClassApp {
                        name: other.to_string(),
                        args: type_args,
                    }
                }
            }
            other if self.enums.contains_key(other) => {
                let enum_sig = self.enums.get(other).unwrap().clone();
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
                if type_args.is_empty() {
                    Ty::Enum(other.to_string())
                } else {
                    Ty::EnumApp {
                        name: other.to_string(),
                        args: type_args,
                    }
                }
            }
            other if self.interfaces.contains_key(other) => {
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
                    message: format!("unknown type `{other}`"),
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
                .classes
                .get(c)
                .map(|cs| cs.implements.iter().any(|x| x == i))
                .unwrap_or(false),
            (Ty::ClassApp { name: c, .. }, Ty::Interface(i)) => self
                .classes
                .get(c)
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
