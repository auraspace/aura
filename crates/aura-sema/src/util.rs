//! Type unification and substitution helpers.

use std::collections::HashMap;

use crate::ty::Ty;
use aura_ast::{BinOp, Expr};

pub(crate) fn analyze_null_check(cond: &Expr) -> Option<(String, bool)> {
    let cond = match cond {
        Expr::Group(inner, _) => inner.as_ref(),
        other => other,
    };
    let Expr::Binary(b) = cond else {
        return None;
    };
    let not_null_when_true = match b.op {
        BinOp::Ne => true,
        BinOp::Eq => false,
        _ => return None,
    };
    match (b.left.as_ref(), b.right.as_ref()) {
        (Expr::Ident(id), Expr::Null(_)) | (Expr::Null(_), Expr::Ident(id)) => {
            Some((id.name.clone(), not_null_when_true))
        }
        _ => None,
    }
}

/// Unify `pattern` (may contain type params) with a concrete `concrete` type.
pub(crate) fn unify_ty(
    pattern: &Ty,
    concrete: &Ty,
    map: &mut HashMap<String, Ty>,
) -> Result<(), String> {
    match (pattern, concrete) {
        (Ty::TypeParam(p), c) => {
            if matches!(c, Ty::Null) {
                return Ok(());
            }
            if let Some(ex) = map.get(p) {
                if ex != c {
                    return Err(format!(
                        "conflicting bindings for `{p}`: {} vs {}",
                        ex.display(),
                        c.display()
                    ));
                }
            } else {
                map.insert(p.clone(), c.clone());
            }
            Ok(())
        }
        (Ty::Nullable(_p), Ty::Null) => Ok(()),
        (Ty::Nullable(p), c) => unify_ty(p, c, map),
        (Ty::ClassApp { name: n1, args: a1 }, Ty::ClassApp { name: n2, args: a2 })
        | (Ty::EnumApp { name: n1, args: a1 }, Ty::EnumApp { name: n2, args: a2 })
            if n1 == n2 && a1.len() == a2.len() =>
        {
            for (a, b) in a1.iter().zip(a2.iter()) {
                unify_ty(a, b, map)?;
            }
            Ok(())
        }
        (a, b) if a == b => Ok(()),
        (a, b) => Err(format!("cannot unify {} with {}", a.display(), b.display())),
    }
}

pub(crate) fn type_subst_map(params: &[String], args: &[Ty]) -> HashMap<String, Ty> {
    params.iter().cloned().zip(args.iter().cloned()).collect()
}

pub(crate) fn subst_ty(ty: &Ty, map: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::TypeParam(name) => map.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Nullable(inner) => Ty::Nullable(Box::new(subst_ty(inner, map))),
        Ty::ClassApp { name, args } => Ty::ClassApp {
            name: name.clone(),
            args: args.iter().map(|a| subst_ty(a, map)).collect(),
        },
        Ty::EnumApp { name, args } => Ty::EnumApp {
            name: name.clone(),
            args: args.iter().map(|a| subst_ty(a, map)).collect(),
        },
        Ty::InterfaceApp { name, args } => Ty::InterfaceApp {
            name: name.clone(),
            args: args.iter().map(|a| subst_ty(a, map)).collect(),
        },
        other => other.clone(),
    }
}

pub(crate) fn eq_compatible(a: &Ty, b: &Ty) -> bool {
    if is_aggregate_eq_forbidden(a) || is_aggregate_eq_forbidden(b) {
        return false;
    }
    if a == b {
        return true;
    }
    match (a, b) {
        (Ty::Null, Ty::Nullable(_)) | (Ty::Nullable(_), Ty::Null) => true,
        (Ty::Null, Ty::Null) => true,
        (Ty::Nullable(x), y) if x.as_ref() == y => true,
        (x, Ty::Nullable(y)) if x == y.as_ref() => true,
        _ => false,
    }
}

/// C4i: struct/enum (and interface) values cannot use `==` / `!=` in MVP.
/// Class refs use pointer identity; primitives/String compare by value/content.
fn is_aggregate_eq_forbidden(ty: &Ty) -> bool {
    match ty {
        Ty::Enum(_) | Ty::EnumApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. } => true,
        Ty::Nullable(inner) => is_aggregate_eq_forbidden(inner),
        // Structs are Ty::Class with is_struct in ClassSig — checked in expr with Checker.
        _ => false,
    }
}
