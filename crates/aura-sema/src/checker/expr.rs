use aura_ast::{BinOp, Expr, UnOp};

use super::Checker;
use crate::error::SemaError;
use crate::ty::Ty;
use crate::util::{eq_compatible, subst_ty, type_subst_map};

impl Checker {
    pub(crate) fn note_mono_ty(&mut self, ty: &Ty) {
        match ty {
            Ty::ClassApp { name, args } if !args.is_empty() => {
                self.mono_classes.insert((name.clone(), args.clone()));
            }
            Ty::EnumApp { name, args } if !args.is_empty() => {
                self.mono_enums.insert((name.clone(), args.clone()));
            }
            _ => {}
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
                if let Some(local) = self.lookup_local(&id.name) {
                    return Ok(local.ty.clone());
                }
                Err(SemaError {
                    message: format!("undefined name `{}`", id.name),
                    span: id.span,
                })
            }
            Expr::This(span) => {
                let class = self.current_class.as_ref().ok_or_else(|| SemaError {
                    message: "`this` is only valid inside methods".into(),
                    span: *span,
                })?;
                // Inside generic class, this is the open Class with type params as TypeParam
                let sig = self.classes.get(class).unwrap();
                if sig.type_params.is_empty() {
                    Ok(Ty::Class(class.clone()))
                } else {
                    Ok(Ty::ClassApp {
                        name: class.clone(),
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
            Expr::Field(f) => {
                let obj_ty = self.check_expr(&f.object)?;
                if let Some(cname) = obj_ty.class_name() {
                    let class = self.classes.get(cname).cloned().ok_or_else(|| SemaError {
                        message: format!("unknown class `{cname}`"),
                        span: f.span,
                    })?;
                    let subst = type_subst_map(&class.type_params, obj_ty.class_args());
                    if let Some(field) = class.fields.iter().find(|x| x.name == f.field.name) {
                        return Ok(subst_ty(&field.ty, &subst));
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
                    let iface = self.interfaces.get(iface_name).ok_or_else(|| SemaError {
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
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
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
                        if !eq_compatible(&l, &r) {
                            return Err(SemaError {
                                message: format!(
                                    "cannot compare {} and {}",
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
                    return Err(SemaError {
                        message: format!(
                            "cannot assign {} to `{}` of type {}",
                            value_ty.display(),
                            a.name.name,
                            target.display()
                        ),
                        span: a.value.span(),
                    });
                }
                Ok(target)
            }
            Expr::Call(c) => self.check_call(c, expected),
        }
    }
}
