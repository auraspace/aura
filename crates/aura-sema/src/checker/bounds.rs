use std::collections::HashMap;

use aura_ast::{Span, TypeParam};

use super::Checker;
use crate::error::SemaError;
use crate::ty::Ty;

impl Checker {
    pub(crate) fn bind_type_params(&mut self, params: &[TypeParam]) -> Result<(), SemaError> {
        self.type_params.clear();
        for p in params {
            if self.type_params.contains_key(&p.name.name) {
                return Err(SemaError {
                    message: format!("duplicate type parameter `{}`", p.name.name),
                    span: p.name.span,
                });
            }
            let mut bounds = Vec::new();
            for b in &p.bounds {
                let isig = match self.resolve_interface(&b.name, b.span) {
                    Ok(i) => i,
                    Err(_) => {
                        return Err(SemaError {
                            message: format!(
                                "unknown bound `{}` (C2e: bounds must be interfaces)",
                                b.name
                            ),
                            span: b.span,
                        });
                    }
                };
                let ikey = crate::ty::nominal_key(&isig.package, &b.name);
                if bounds.contains(&ikey) || bounds.contains(&b.name) {
                    return Err(SemaError {
                        message: format!("duplicate bound `{}` on `{}`", b.name, p.name.name),
                        span: b.span,
                    });
                }
                bounds.push(ikey);
            }
            self.type_params.insert(p.name.name.clone(), bounds);
        }
        Ok(())
    }

    pub(crate) fn bounds_map_from_params(params: &[TypeParam]) -> HashMap<String, Vec<String>> {
        params
            .iter()
            .map(|p| {
                (
                    p.name.name.clone(),
                    p.bounds.iter().map(|b| b.name.clone()).collect(),
                )
            })
            .collect()
    }

    /// Does `ty` satisfy all interface bounds?
    pub(crate) fn satisfies_bounds(
        &self,
        ty: &Ty,
        bounds: &[String],
        span: Span,
    ) -> Result<(), SemaError> {
        for b in bounds {
            if !self.ty_implements(ty, b) {
                return Err(SemaError {
                    message: format!("type {} does not satisfy bound `{}`", ty.display(), b),
                    span,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn ty_implements(&self, ty: &Ty, iface: &str) -> bool {
        let same = |x: &str| {
            x == iface || crate::ty::split_nominal(x).0 == crate::ty::split_nominal(iface).0
        };
        match ty {
            Ty::Class(c) => self
                .class_by_nominal_key(c)
                .map(|cs| {
                    cs.implements.iter().any(|imp| match imp {
                        Ty::Interface(k) | Ty::InterfaceApp { name: k, .. } => same(k),
                        _ => false,
                    })
                })
                .unwrap_or(false),
            // C9a: substitute class mono args into open implements (`: Iface<T>`).
            Ty::ClassApp { name: c, args } => self
                .class_by_nominal_key(c)
                .map(|cs| {
                    let map = if cs.type_params.len() == args.len() {
                        Some(crate::util::type_subst_map(&cs.type_params, args))
                    } else {
                        None
                    };
                    cs.implements.iter().any(|imp| {
                        let concrete = if let Some(ref m) = map {
                            crate::util::subst_ty(imp, m)
                        } else {
                            imp.clone()
                        };
                        match concrete {
                            Ty::Interface(k) | Ty::InterfaceApp { name: k, .. } => same(&k),
                            _ => false,
                        }
                    })
                })
                .unwrap_or(false),
            Ty::Interface(i) | Ty::InterfaceApp { name: i, .. } => same(i),
            Ty::TypeParam(p) => self
                .type_params
                .get(p)
                .map(|bs| bs.iter().any(|x| same(x)))
                .unwrap_or(false),
            _ => false,
        }
    }

    pub(crate) fn check_type_args_bounds(
        &self,
        param_names: &[String],
        bounds: &HashMap<String, Vec<String>>,
        type_args: &[Ty],
        span: Span,
        what: &str,
    ) -> Result<(), SemaError> {
        for (name, arg) in param_names.iter().zip(type_args.iter()) {
            if let Some(bs) = bounds.get(name) {
                if let Err(mut e) = self.satisfies_bounds(arg, bs, span) {
                    e.message = format!("{what}: {}", e.message);
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}
