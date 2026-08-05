use std::collections::HashMap;

use aura_ast::TypeRef;

use super::Checker;
use crate::error::SemaError;
use crate::ty::{nominal_key, Ty};
impl Checker {
    pub(crate) fn type_from_ref(&self, t: &TypeRef) -> Result<Ty, SemaError> {
        if t.reference {
            if t.nullable {
                return Err(SemaError {
                    message: "borrow reference types must be non-null (`ref T`, not `ref T?`)"
                        .into(),
                    span: t.span,
                });
            }
            if t.fun.is_some() {
                return Err(SemaError {
                    message: "borrow references to function types are not allowed in the MVP"
                        .into(),
                    span: t.span,
                });
            }
            if t.type_args
                .iter()
                .any(type_ref_contains_reference_for_async)
            {
                return Err(SemaError {
                    message: "nested borrow reference types are not allowed in the MVP".into(),
                    span: t.span,
                });
            }
        }
        // C10f: function type `(T) -> U`.
        if let Some(fun) = &t.fun {
            if fun.params.iter().any(type_ref_contains_reference_for_async)
                || type_ref_contains_reference_for_async(&fun.ret)
            {
                return Err(SemaError {
                    message: "borrow references cannot appear in function types in the MVP".into(),
                    span: t.span,
                });
            }
            let params = fun
                .params
                .iter()
                .map(|p| self.type_from_ref(p))
                .collect::<Result<Vec<_>, _>>()?;
            let ret = self.type_from_ref(&fun.ret)?;
            let ty = Ty::Fun {
                params,
                ret: Box::new(ret),
            };
            return Ok(if t.nullable {
                Ty::Nullable(Box::new(ty))
            } else {
                ty
            });
        }

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
            "Task" | "TaskHandle" | "Channel" | "ForeignHandle" => {
                if qualified_pkg.is_some() {
                    return Err(SemaError {
                        message: format!(
                            "builtin async/foreign type `{}` cannot be package-qualified",
                            t.name.name
                        ),
                        span: t.span,
                    });
                }
                if type_args.len() != 1 {
                    return Err(SemaError {
                        message: format!(
                            "type `{}` expects exactly one type argument, got {}",
                            t.name.name,
                            type_args.len()
                        ),
                        span: t.span,
                    });
                }
                if type_ref_contains_reference_for_async(&t.type_args[0]) {
                    return Err(SemaError {
                        message: format!("borrow reference cannot be stored in `{}`", t.name.name),
                        span: t.type_args[0].span,
                    });
                }
                let inner = Box::new(type_args.into_iter().next().unwrap());
                if t.name.name == "ForeignHandle"
                    && type_ref_contains_reference_for_async(&t.type_args[0])
                {
                    return Err(SemaError {
                        message: "borrow reference cannot be stored in `ForeignHandle`".into(),
                        span: t.type_args[0].span,
                    });
                }
                match t.name.name.as_str() {
                    "Task" => Ty::Task(inner),
                    "TaskHandle" => Ty::TaskHandle(inner),
                    "Channel" => Ty::Channel(inner),
                    _ => Ty::ForeignHandle(inner),
                }
            }
            // C9f: type alias expansion (non-generic).
            other if self.type_aliases.contains_key(other) && qualified_pkg.is_none() => {
                if !type_args.is_empty() {
                    return Err(SemaError {
                        message: format!("type alias `{other}` cannot take type arguments"),
                        span: t.span,
                    });
                }
                let entries = self.type_aliases.get(other).unwrap();
                let ty = if entries.len() == 1 {
                    entries[0].1.clone()
                } else {
                    entries
                        .iter()
                        .find(|(p, _)| p == &self.current_package)
                        .map(|(_, t)| t.clone())
                        .or_else(|| entries.first().map(|(_, t)| t.clone()))
                        .ok_or_else(|| SemaError {
                            message: format!("unknown type `{other}`"),
                            span: t.span,
                        })?
                };
                // Skip further base construction — apply nullability below.
                if t.nullable {
                    return Ok(Ty::Nullable(Box::new(ty)));
                }
                return Ok(ty);
            }
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
                            message: format!("type `{other}` is not a member of package `{pkg}`"),
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
                let iface = if let Some(pkg) = qualified_pkg {
                    self.resolve_interface_in_package(other, pkg, t.span)?
                } else {
                    self.resolve_interface(other, t.span)?
                };
                if type_args.len() != iface.type_params.len() {
                    return Err(SemaError {
                        message: format!(
                            "interface `{}` expects {} type argument(s), got {}",
                            other,
                            iface.type_params.len(),
                            type_args.len()
                        ),
                        span: t.span,
                    });
                }
                let key = crate::ty::nominal_key(&iface.package, other);
                if type_args.is_empty() {
                    Ty::Interface(key)
                } else {
                    Ty::InterfaceApp {
                        name: key,
                        args: type_args,
                    }
                }
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

    /// Does class implement the target interface (exact mono for InterfaceApp)?
    pub(crate) fn class_implements(&self, class_key: &str, target: &Ty) -> bool {
        self.class_implements_with_args(class_key, &[], target)
    }

    /// C9a: generic class mono — substitute class type params into implements.
    pub(crate) fn class_implements_with_args(
        &self,
        class_key: &str,
        class_args: &[Ty],
        target: &Ty,
    ) -> bool {
        let Some(cs) = self.class_by_nominal_key(class_key) else {
            return false;
        };
        let map = if !class_args.is_empty() && cs.type_params.len() == class_args.len() {
            Some(crate::util::type_subst_map(&cs.type_params, class_args))
        } else {
            None
        };
        cs.implements.iter().any(|imp| {
            let concrete = if let Some(ref m) = map {
                crate::util::subst_ty(imp, m)
            } else {
                imp.clone()
            };
            Self::iface_ty_matches(&concrete, target) || self.interface_extends(&concrete, target)
        })
    }

    /// Whether an interface value can be viewed as one of its parent interfaces.
    pub(crate) fn interface_extends(&self, from: &Ty, target: &Ty) -> bool {
        let mut seen = std::collections::HashSet::new();
        self.interface_extends_inner(from, target, &mut seen)
    }

    fn interface_extends_inner(
        &self,
        from: &Ty,
        target: &Ty,
        seen: &mut std::collections::HashSet<Ty>,
    ) -> bool {
        if !seen.insert(from.clone()) {
            return false;
        }
        let Some(key) = from.iface_key() else {
            return false;
        };
        let Some(iface) = self.iface_by_nominal_key(key) else {
            return false;
        };
        let subst = crate::util::type_subst_map(&iface.type_params, from.iface_args());
        iface.parents.iter().any(|parent| {
            let parent = crate::util::subst_ty(parent, &subst);
            Self::iface_ty_matches(&parent, target)
                || self.interface_extends_inner(&parent, target, seen)
        })
    }

    /// All methods available on an interface, including inherited parents.
    pub(crate) fn interface_methods(
        &self,
        iface_ty: &Ty,
    ) -> HashMap<String, crate::sigs::IfaceMethodSig> {
        let mut methods = HashMap::new();
        let mut seen = std::collections::HashSet::new();
        self.collect_interface_methods(iface_ty, &mut methods, &mut seen);
        methods
    }

    fn collect_interface_methods(
        &self,
        iface_ty: &Ty,
        methods: &mut HashMap<String, crate::sigs::IfaceMethodSig>,
        seen: &mut std::collections::HashSet<Ty>,
    ) {
        if !seen.insert(iface_ty.clone()) {
            return;
        }
        let Some(key) = iface_ty.iface_key() else {
            return;
        };
        let Some(iface) = self.iface_by_nominal_key(key) else {
            return;
        };
        let subst = crate::util::type_subst_map(&iface.type_params, iface_ty.iface_args());
        for parent in &iface.parents {
            self.collect_interface_methods(&crate::util::subst_ty(parent, &subst), methods, seen);
        }
        for (name, method) in &iface.methods {
            methods.insert(
                name.clone(),
                crate::sigs::IfaceMethodSig {
                    name: method.name.clone(),
                    params: method
                        .params
                        .iter()
                        .map(|p| crate::util::subst_ty(p, &subst))
                        .collect(),
                    ret: crate::util::subst_ty(&method.ret, &subst),
                    has_default: method.has_default,
                    required_params: method.required_params,
                    is_vararg: method.is_vararg,
                    span: method.span,
                },
            );
        }
    }

    /// Match implemented iface vs expected: non-generic by key; generic exact args.
    pub(crate) fn iface_ty_matches(implemented: &Ty, target: &Ty) -> bool {
        match (implemented, target) {
            (Ty::Interface(a), Ty::Interface(b)) => {
                a == b || crate::ty::split_nominal(a).0 == crate::ty::split_nominal(b).0
            }
            (Ty::InterfaceApp { name: a, args: aa }, Ty::InterfaceApp { name: b, args: ba }) => {
                (a == b || crate::ty::split_nominal(a).0 == crate::ty::split_nominal(b).0)
                    && aa == ba
            }
            // Non-generic target does not match a mono implementor (and vice versa).
            _ => false,
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
            (Ty::Class(c), Ty::Interface(_) | Ty::InterfaceApp { .. }) => {
                self.class_implements(c, to)
            }
            (Ty::ClassApp { name: c, args }, Ty::Interface(_) | Ty::InterfaceApp { .. }) => {
                self.class_implements_with_args(c, args, to)
            }
            (
                Ty::Interface(_) | Ty::InterfaceApp { .. },
                Ty::Interface(_) | Ty::InterfaceApp { .. },
            ) => self.interface_extends(from, to),
            (Ty::Class(child), Ty::Class(parent)) => self.class_extends(child, parent, &[]),
            (Ty::ClassApp { name: child, args }, Ty::Class(parent)) => {
                self.class_extends(child, parent, args)
            }
            (Ty::ClassApp { name: child, args }, target @ Ty::ClassApp { .. }) => {
                self.class_extends_to(child, args, target)
            }
            // Bounded type param is assignable to its interface bounds (non-generic)
            (Ty::TypeParam(p), Ty::Interface(i)) => self
                .type_params
                .get(p)
                .map(|bs| {
                    bs.iter().any(|x| {
                        x == i || crate::ty::split_nominal(x).0 == crate::ty::split_nominal(i).0
                    })
                })
                .unwrap_or(false),
            // C10d: function types — params contravariant, ret covariant (MVP: invariant).
            (
                Ty::Fun {
                    params: fp,
                    ret: fr,
                },
                Ty::Fun {
                    params: tp,
                    ret: tr,
                },
            ) if fp.len() == tp.len() => {
                fp.iter().zip(tp.iter()).all(|(a, b)| a == b) && fr.as_ref() == tr.as_ref()
            }
            // Type params only match themselves (handled by ==)
            _ => false,
        }
    }

    /// Check the direct-superclass chain for class assignability.
    pub(crate) fn class_extends(
        &self,
        child_key: &str,
        parent_key: &str,
        child_args: &[Ty],
    ) -> bool {
        self.class_extends_to(child_key, child_args, &Ty::Class(parent_key.to_string()))
    }

    /// Check the superclass chain against an exact (possibly generic) parent type.
    fn class_extends_to(&self, child_key: &str, child_args: &[Ty], target: &Ty) -> bool {
        let mut current = Some((child_key.to_string(), child_args.to_vec()));
        let mut seen = std::collections::HashSet::new();
        while let Some((key, args)) = current {
            if !seen.insert(key.clone()) {
                return false;
            }
            let Some(class) = self.class_by_nominal_key(&key) else {
                return false;
            };
            let Some(superclass) = &class.superclass else {
                return false;
            };
            let subst = if !args.is_empty() && class.type_params.len() == args.len() {
                Some(crate::util::type_subst_map(&class.type_params, &args))
            } else {
                None
            };
            let parent = subst
                .as_ref()
                .map(|map| crate::util::subst_ty(superclass, map))
                .unwrap_or_else(|| superclass.clone());
            if &parent == target {
                return true;
            }
            match parent {
                Ty::Class(parent) => current = Some((parent, Vec::new())),
                Ty::ClassApp { name, args } => {
                    current = Some((name, args));
                }
                _ => return false,
            }
        }
        false
    }
}

pub(crate) fn type_ref_contains_reference_for_async(t: &TypeRef) -> bool {
    t.reference
        || t.type_args
            .iter()
            .any(type_ref_contains_reference_for_async)
        || t.fun.as_ref().is_some_and(|fun| {
            fun.params.iter().any(type_ref_contains_reference_for_async)
                || type_ref_contains_reference_for_async(&fun.ret)
        })
}
