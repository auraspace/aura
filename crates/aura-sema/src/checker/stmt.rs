use std::collections::{HashMap, HashSet};

use aura_ast::{Block, MatchStmt, Pattern, Stmt};

use super::{Checker, Local};
use crate::error::SemaError;
use crate::ty::Ty;
use crate::util::{analyze_null_check, subst_ty, type_subst_map};

impl Checker {
    /// Types that may be thrown/caught (C3c primitives + C3g class/struct values).
    pub(crate) fn is_throwable(ty: &Ty) -> bool {
        match ty {
            Ty::String | Ty::Int | Ty::Bool => true,
            Ty::Class(_) | Ty::ClassApp { .. } => true,
            _ => false,
        }
    }

    pub(crate) fn check_block(&mut self, block: &Block, expected_ret: &Ty) -> Result<(), SemaError> {
        self.locals.push(HashMap::new());
        for stmt in &block.stmts {
            self.check_stmt(stmt, expected_ret)?;
        }
        self.locals.pop();
        Ok(())
    }

    pub(crate) fn check_match(&mut self, m: &MatchStmt, expected_ret: &Ty) -> Result<(), SemaError> {
        let scrut_ty = self.check_expr(&m.scrutinee)?;
        let Some(ename) = scrut_ty.enum_name() else {
            return Err(SemaError {
                message: format!(
                    "`match` requires an enum type, got {}",
                    scrut_ty.display()
                ),
                span: m.scrutinee.span(),
            });
        };
        let enum_key = match &scrut_ty {
            crate::ty::Ty::Enum(k) | crate::ty::Ty::EnumApp { name: k, .. } => k.as_str(),
            _ => ename,
        };
        let enum_sig = self
            .enum_by_nominal_key(enum_key)
            .cloned()
            .ok_or_else(|| SemaError {
                message: format!("unknown enum `{ename}`"),
                span: m.scrutinee.span(),
            })?;
        let type_args = scrut_ty.enum_args().to_vec();
        let subst = type_subst_map(&enum_sig.type_params, &type_args);
        self.note_mono_ty(&scrut_ty);

        let mut covered = HashSet::new();
        for arm in &m.arms {
            let Pattern::Variant {
                name,
                bindings,
                span,
            } = &arm.pattern;
            let variant = enum_sig
                .variants
                .iter()
                .find(|v| v.name == name.name)
                .ok_or_else(|| SemaError {
                    message: format!("unknown variant `{}` for enum `{ename}`", name.name),
                    span: *span,
                })?;
            if !covered.insert(variant.name.clone()) {
                return Err(SemaError {
                    message: format!("duplicate match arm for variant `{}`", variant.name),
                    span: *span,
                });
            }
            if bindings.len() != variant.fields.len() {
                return Err(SemaError {
                    message: format!(
                        "variant `{}` has {} field(s), pattern binds {}",
                        variant.name,
                        variant.fields.len(),
                        bindings.len()
                    ),
                    span: *span,
                });
            }
            self.locals.push(HashMap::new());
            for (bind, (fname, fty)) in bindings.iter().zip(variant.fields.iter()) {
                if self.current_locals().contains_key(&bind.name) {
                    return Err(SemaError {
                        message: format!("duplicate binding `{}`", bind.name),
                        span: bind.span,
                    });
                }
                let _ = fname;
                let ty = subst_ty(fty, &subst);
                self.current_locals_mut().insert(
                    bind.name.clone(),
                    Local {
                        ty,
                        mutable: false,
                    },
                );
            }
            for stmt in &arm.body.stmts {
                self.check_stmt(stmt, expected_ret)?;
            }
            self.locals.pop();
        }
        let missing: Vec<_> = enum_sig
            .variants
            .iter()
            .filter(|v| !covered.contains(&v.name))
            .map(|v| v.name.clone())
            .collect();
        if !missing.is_empty() {
            return Err(SemaError {
                message: format!(
                    "non-exhaustive match on `{ename}`; missing: {}",
                    missing.join(", ")
                ),
                span: m.span,
            });
        }
        Ok(())
    }

    pub(crate) fn check_stmt(&mut self, stmt: &Stmt, expected_ret: &Ty) -> Result<(), SemaError> {
        match stmt {
            Stmt::Match(m) => self.check_match(m, expected_ret),
            Stmt::Throw(t) => {
                let ty = self.check_expr(&t.value)?;
                if matches!(ty, Ty::Unit | Ty::Null) {
                    return Err(SemaError {
                        message: format!("cannot throw {}", ty.display()),
                        span: t.value.span(),
                    });
                }
                // C3c/C3g: String / Int / Bool / class / struct (no interface, no enum).
                if Self::is_throwable(&ty) {
                    Ok(())
                } else {
                    Err(SemaError {
                        message: format!(
                            "cannot throw {}; only String, Int, Bool, class, or struct",
                            ty.display()
                        ),
                        span: t.value.span(),
                    })
                }
            }
            Stmt::Try(t) => {
                self.check_block(&t.try_block, expected_ret)?;
                if let Some(c) = &t.catch {
                    let catch_ty = self.type_from_ref(&c.ty)?;
                    if !Self::is_throwable(&catch_ty) {
                        return Err(SemaError {
                            message: format!(
                                "catch type must be String, Int, Bool, class, or struct (got {})",
                                catch_ty.display()
                            ),
                            span: c.ty.span,
                        });
                    }
                    self.locals.push(HashMap::new());
                    self.current_locals_mut().insert(
                        c.name.name.clone(),
                        Local {
                            ty: catch_ty,
                            mutable: false,
                        },
                    );
                    for stmt in &c.body.stmts {
                        self.check_stmt(stmt, expected_ret)?;
                    }
                    self.locals.pop();
                }
                if let Some(f) = &t.finally {
                    self.check_block(f, expected_ret)?;
                }
                Ok(())
            }
            Stmt::Var(v) => {
                let ann_ty = match &v.ty {
                    Some(t) => Some(self.type_from_ref(t)?),
                    None => None,
                };
                let init_ty = self.check_expr_expected(&v.init, ann_ty.as_ref())?;
                let ty = if let Some(ann_ty) = ann_ty {
                    if !self.is_assignable(&init_ty, &ann_ty) {
                        return Err(SemaError {
                            message: format!(
                                "cannot assign {} to `{}` of type {}",
                                init_ty.display(),
                                v.name.name,
                                ann_ty.display()
                            ),
                            span: v.init.span(),
                        });
                    }
                    ann_ty
                } else {
                    if init_ty == Ty::Null {
                        return Err(SemaError {
                            message: "cannot infer type of `null`; add a type annotation".into(),
                            span: v.init.span(),
                        });
                    }
                    if init_ty == Ty::Unit {
                        return Err(SemaError {
                            message: "cannot bind Unit to a local".into(),
                            span: v.init.span(),
                        });
                    }
                    init_ty
                };
                if self.current_locals().contains_key(&v.name.name) {
                    return Err(SemaError {
                        message: format!("duplicate local `{}`", v.name.name),
                        span: v.name.span,
                    });
                }
                self.note_mono_ty(&ty);
                self.current_locals_mut().insert(
                    v.name.name.clone(),
                    Local {
                        ty,
                        mutable: v.mutable,
                    },
                );
                Ok(())
            }
            Stmt::If(i) => {
                let cond = self.check_expr(&i.cond)?;
                if cond != Ty::Bool {
                    return Err(SemaError {
                        message: format!("if condition must be Bool, got {}", cond.display()),
                        span: i.cond.span(),
                    });
                }
                let fact = analyze_null_check(&i.cond);

                // then-branch (narrow if `x != null`)
                self.locals.push(HashMap::new());
                if let Some((ref name, not_null_when_true)) = fact {
                    if not_null_when_true {
                        self.apply_not_null(name);
                    }
                }
                self.check_block(&i.then_block, expected_ret)?;
                self.locals.pop();

                // else-branch (narrow if `x == null` was the condition)
                if let Some(else_b) = &i.else_block {
                    self.locals.push(HashMap::new());
                    if let Some((ref name, not_null_when_true)) = fact {
                        if !not_null_when_true {
                            self.apply_not_null(name);
                        }
                    }
                    self.check_block(else_b, expected_ret)?;
                    self.locals.pop();
                }
                Ok(())
            }
            Stmt::While(w) => {
                let cond = self.check_expr(&w.cond)?;
                if cond != Ty::Bool {
                    return Err(SemaError {
                        message: format!("while condition must be Bool, got {}", cond.display()),
                        span: w.cond.span(),
                    });
                }
                self.loop_depth += 1;
                let r = self.check_block(&w.body, expected_ret);
                self.loop_depth -= 1;
                r
            }
            Stmt::ForRange(f) => {
                let start_ty = self.check_expr(&f.start)?;
                if start_ty != Ty::Int {
                    return Err(SemaError {
                        message: format!(
                            "for-range start must be Int, got {}",
                            start_ty.display()
                        ),
                        span: f.start.span(),
                    });
                }
                let end_ty = self.check_expr(&f.end)?;
                if end_ty != Ty::Int {
                    return Err(SemaError {
                        message: format!("for-range end must be Int, got {}", end_ty.display()),
                        span: f.end.span(),
                    });
                }
                self.locals.push(HashMap::new());
                if self.current_locals().contains_key(&f.name.name) {
                    return Err(SemaError {
                        message: format!("duplicate binding `{}` in for loop", f.name.name),
                        span: f.name.span,
                    });
                }
                self.current_locals_mut().insert(
                    f.name.name.clone(),
                    Local {
                        ty: Ty::Int,
                        mutable: false,
                    },
                );
                self.loop_depth += 1;
                let mut body_err = Ok(());
                for stmt in &f.body.stmts {
                    if let Err(e) = self.check_stmt(stmt, expected_ret) {
                        body_err = Err(e);
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.locals.pop();
                body_err
            }
            Stmt::ForIn(f) => {
                let iter_ty = self.check_expr(&f.iterable)?;
                let elem_ty = match &iter_ty {
                    // C3w: for (b in string) yields UTF-8 bytes as Int.
                    Ty::String => Ty::Int,
                    Ty::ClassApp { name, args }
                        if crate::ty::split_nominal(name).0 == "Array" && args.len() == 1 =>
                    {
                        args[0].clone()
                    }
                    other => {
                        return Err(SemaError {
                            message: format!(
                                "for-in iterable must be Array<T> or String, got {}",
                                other.display()
                            ),
                            span: f.iterable.span(),
                        });
                    }
                };
                self.locals.push(HashMap::new());
                if self.current_locals().contains_key(&f.name.name) {
                    return Err(SemaError {
                        message: format!("duplicate binding `{}` in for loop", f.name.name),
                        span: f.name.span,
                    });
                }
                self.current_locals_mut().insert(
                    f.name.name.clone(),
                    Local {
                        ty: elem_ty,
                        mutable: false,
                    },
                );
                self.loop_depth += 1;
                let mut body_err = Ok(());
                for stmt in &f.body.stmts {
                    if let Err(e) = self.check_stmt(stmt, expected_ret) {
                        body_err = Err(e);
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.locals.pop();
                body_err
            }
            Stmt::Break(span) => {
                if self.loop_depth == 0 {
                    return Err(SemaError {
                        message: "`break` is only valid inside a loop".into(),
                        span: *span,
                    });
                }
                Ok(())
            }
            Stmt::Continue(span) => {
                if self.loop_depth == 0 {
                    return Err(SemaError {
                        message: "`continue` is only valid inside a loop".into(),
                        span: *span,
                    });
                }
                Ok(())
            }
            Stmt::Return(r) => {
                let got = match &r.value {
                    Some(e) => self.check_expr_expected(e, Some(expected_ret))?,
                    None => Ty::Unit,
                };
                if !self.is_assignable(&got, expected_ret) {
                    return Err(SemaError {
                        message: format!(
                            "return type mismatch: expected {}, got {}",
                            expected_ret.display(),
                            got.display()
                        ),
                        span: r.span,
                    });
                }
                Ok(())
            }
            Stmt::Expr(e) => {
                let _ = self.check_expr(e)?;
                Ok(())
            }
        }
    }
}
