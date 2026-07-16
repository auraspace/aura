//! Type checker.

mod bounds;
mod call;
mod expr;
mod file;
mod stmt;
mod types;

use std::collections::{HashMap, HashSet};

use aura_ast::{ClassDecl, FunDecl, Span};
use crate::error::SemaError;
use crate::sigs::*;
use crate::ty::Ty;

/// Builtin `Array<T>` primitives (C3j). Heap class elements allowed in C4c via Checker.
pub(crate) fn is_array_primitive_elem(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Bool | Ty::String)
}

pub(crate) struct Local {
    ty: Ty,
    mutable: bool,
}

pub(crate) struct Checker {
    /// Free functions by simple name; multiple packages may share a name (C3o).
    functions: HashMap<String, Vec<FunSig>>,
    /// Classes/structs by simple name; multiple packages may share a name (C3v).
    classes: HashMap<String, Vec<ClassSig>>,
    /// Enums by simple name; multiple packages may share a name (C3v).
    enums: HashMap<String, Vec<EnumSig>>,
    /// Variant name → owning enum name (unique across file for C3b).
    variant_to_enum: HashMap<String, String>,
    /// Interfaces by simple name; multiple packages may share a name (C4d).
    interfaces: HashMap<String, Vec<InterfaceSig>>,
    locals: Vec<HashMap<String, Local>>,
    /// Type params in current generic scope (name → bound interface names).
    type_params: HashMap<String, Vec<String>>,
    current_class: Option<String>,
    /// Package of the item whose body/signature is being checked.
    current_package: String,
    /// `import` edges: package → set of imported package names.
    package_imports: HashMap<String, HashSet<String>>,
    /// `import path as Alias` → package path (C3n). Keys are alias idents.
    import_aliases: HashMap<String, String>,
    /// Nested loop depth for `break` / `continue` (C3i).
    loop_depth: usize,
    mono_classes: HashSet<(String, Vec<Ty>)>,
    mono_enums: HashSet<(String, Vec<Ty>)>,
    mono_funs: HashSet<(String, Vec<Ty>)>,
    call_instantiations: HashMap<u32, CallInstantiation>,
}


impl Checker {
    pub(crate) fn new() -> Self {
        let mut functions: HashMap<String, Vec<FunSig>> = HashMap::new();
        functions.insert(
            "println".into(),
            vec![FunSig {
                name: "println".into(),
                is_pub: true,
                package: String::new(),
                is_test: false,
                type_params: Vec::new(),
                bounds: HashMap::new(),
                params: vec![Ty::String],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            }],
        );
        // Testing builtins (RFC-011 MVP)
        functions.insert(
            "assert".into(),
            vec![FunSig {
                name: "assert".into(),
                is_pub: true,
                package: String::new(),
                is_test: false,
                type_params: Vec::new(),
                bounds: HashMap::new(),
                params: vec![Ty::Bool],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            }],
        );

        // Builtin Array<T> (C3j/C4c) — monomorphized; T ∈ primitives or heap class.
        let mut array_methods = HashMap::new();
        array_methods.insert(
            "get".into(),
            ClassMethodSig {
                class: "Array".into(),
                name: "get".into(),
                params: vec![Ty::Int],
                ret: Ty::TypeParam("T".into()),
                span: Span::new(0, 0),
            },
        );
        array_methods.insert(
            "set".into(),
            ClassMethodSig {
                class: "Array".into(),
                name: "set".into(),
                params: vec![Ty::Int, Ty::TypeParam("T".into())],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            },
        );
        array_methods.insert(
            "push".into(),
            ClassMethodSig {
                class: "Array".into(),
                name: "push".into(),
                params: vec![Ty::TypeParam("T".into())],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            },
        );
        // C3r: pop last element (throws if empty).
        array_methods.insert(
            "pop".into(),
            ClassMethodSig {
                class: "Array".into(),
                name: "pop".into(),
                params: vec![],
                ret: Ty::TypeParam("T".into()),
                span: Span::new(0, 0),
            },
        );
        let mut classes: HashMap<String, Vec<ClassSig>> = HashMap::new();
        classes.insert(
            "Array".into(),
            vec![ClassSig {
                name: "Array".into(),
                is_pub: true,
                package: String::new(),
                is_struct: true,
                type_params: vec!["T".into()],
                bounds: HashMap::new(),
                implements: Vec::new(),
                fields: vec![FieldSig {
                    name: "len".into(),
                    ty: Ty::Int,
                    mutable: false,
                }],
                methods: array_methods,
                span: Span::new(0, 0),
            }],
        );

        Self {
            functions,
            classes,
            enums: HashMap::new(),
            variant_to_enum: HashMap::new(),
            interfaces: HashMap::new(), // Vec per simple name (C4d)
            locals: Vec::new(),
            type_params: HashMap::new(),
            current_class: None,
            current_package: String::new(),
            package_imports: HashMap::new(),
            import_aliases: HashMap::new(),
            loop_depth: 0,
            mono_classes: HashSet::new(),
            mono_enums: HashSet::new(),
            mono_funs: HashSet::new(),
            call_instantiations: HashMap::new(),
        }
    }

    /// Cross-package visibility: same package always; else must be `pub` and imported.
    /// Builtins use empty `package` and are always visible.
    pub(crate) fn check_visible(
        &self,
        name: &str,
        is_pub: bool,
        item_package: &str,
        span: Span,
    ) -> Result<(), SemaError> {
        if item_package.is_empty() {
            return Ok(());
        }
        if item_package == self.current_package {
            return Ok(());
        }
        if !is_pub {
            return Err(SemaError {
                message: format!("`{name}` is private to package `{item_package}`"),
                span,
            });
        }
        let allowed = self
            .package_imports
            .get(&self.current_package)
            .map(|s| s.contains(item_package))
            .unwrap_or(false);
        if !allowed {
            return Err(SemaError {
                message: format!(
                    "`{name}` is in package `{item_package}` which is not imported"
                ),
                span,
            });
        }
        Ok(())
    }

    pub(crate) fn is_visible(&self, name: &str, is_pub: bool, item_package: &str) -> bool {
        self.check_visible(name, is_pub, item_package, Span::new(0, 0))
            .is_ok()
    }

    /// Resolve free function by simple name (C3o: prefer same package, then unique visible).
    pub(crate) fn resolve_fun(&self, name: &str, span: Span) -> Result<FunSig, SemaError> {
        let list = self.functions.get(name).ok_or_else(|| SemaError {
            message: format!("undefined function `{name}`"),
            span,
        })?;
        let visible: Vec<&FunSig> = list
            .iter()
            .filter(|s| self.is_visible(name, s.is_pub, &s.package))
            .collect();
        if visible.is_empty() {
            if let Some(s) = list.first() {
                self.check_visible(name, s.is_pub, &s.package, span)?;
            }
            return Err(SemaError {
                message: format!("undefined function `{name}`"),
                span,
            });
        }
        let same_pkg: Vec<&FunSig> = visible
            .iter()
            .copied()
            .filter(|s| s.package == self.current_package)
            .collect();
        if same_pkg.len() == 1 {
            return Ok(same_pkg[0].clone());
        }
        if visible.len() == 1 {
            return Ok(visible[0].clone());
        }
        let pkgs: Vec<&str> = visible.iter().map(|s| s.package.as_str()).collect();
        Err(SemaError {
            message: format!(
                "ambiguous function `{name}` from packages {}; use `import … as Alias` and `Alias.{name}`",
                pkgs.join(", ")
            ),
            span,
        })
    }

    pub(crate) fn resolve_fun_in_package(
        &self,
        name: &str,
        pkg: &str,
        span: Span,
    ) -> Result<FunSig, SemaError> {
        let list = self.functions.get(name).ok_or_else(|| SemaError {
            message: format!("undefined function `{name}` in package `{pkg}`"),
            span,
        })?;
        let sig = list
            .iter()
            .find(|s| s.package == pkg)
            .cloned()
            .ok_or_else(|| SemaError {
                message: format!("`{name}` is not a member of package `{pkg}`"),
                span,
            })?;
        self.check_visible(name, sig.is_pub, &sig.package, span)?;
        Ok(sig)
    }

    pub(crate) fn fun_in_package(&self, name: &str, pkg: &str) -> Option<&FunSig> {
        self.functions
            .get(name)?
            .iter()
            .find(|s| s.package == pkg)
    }

    pub(crate) fn class_in_package(&self, name: &str, pkg: &str) -> Option<&ClassSig> {
        self.classes
            .get(name)?
            .iter()
            .find(|s| s.package == pkg)
    }

    /// Resolve class by simple name (C3v: same-package first, else unique visible).
    pub(crate) fn resolve_class(&self, name: &str, span: Span) -> Result<ClassSig, SemaError> {
        let list = self.classes.get(name).ok_or_else(|| SemaError {
            message: format!("unknown type `{name}`"),
            span,
        })?;
        let visible: Vec<&ClassSig> = list
            .iter()
            .filter(|s| self.is_visible(name, s.is_pub, &s.package))
            .collect();
        if visible.is_empty() {
            if let Some(s) = list.first() {
                self.check_visible(name, s.is_pub, &s.package, span)?;
            }
            return Err(SemaError {
                message: format!("unknown type `{name}`"),
                span,
            });
        }
        let same_pkg: Vec<&ClassSig> = visible
            .iter()
            .copied()
            .filter(|s| s.package == self.current_package)
            .collect();
        if same_pkg.len() == 1 {
            return Ok(same_pkg[0].clone());
        }
        if visible.len() == 1 {
            return Ok(visible[0].clone());
        }
        let pkgs: Vec<&str> = visible.iter().map(|s| s.package.as_str()).collect();
        Err(SemaError {
            message: format!(
                "ambiguous type `{name}` from packages {}; use `import … as Alias` and `Alias.{name}`",
                pkgs.join(", ")
            ),
            span,
        })
    }

    pub(crate) fn resolve_class_in_package(
        &self,
        name: &str,
        pkg: &str,
        span: Span,
    ) -> Result<ClassSig, SemaError> {
        let class = self
            .class_in_package(name, pkg)
            .cloned()
            .ok_or_else(|| SemaError {
                message: format!("type `{name}` is not a member of package `{pkg}`"),
                span,
            })?;
        self.check_visible(name, class.is_pub, &class.package, span)?;
        Ok(class)
    }

    pub(crate) fn enum_in_package(&self, name: &str, pkg: &str) -> Option<&EnumSig> {
        self.enums
            .get(name)?
            .iter()
            .find(|s| s.package == pkg)
    }

    pub(crate) fn resolve_enum(&self, name: &str, span: Span) -> Result<EnumSig, SemaError> {
        let list = self.enums.get(name).ok_or_else(|| SemaError {
            message: format!("unknown type `{name}`"),
            span,
        })?;
        let visible: Vec<&EnumSig> = list
            .iter()
            .filter(|s| self.is_visible(name, s.is_pub, &s.package))
            .collect();
        if visible.is_empty() {
            if let Some(s) = list.first() {
                self.check_visible(name, s.is_pub, &s.package, span)?;
            }
            return Err(SemaError {
                message: format!("unknown type `{name}`"),
                span,
            });
        }
        let same_pkg: Vec<&EnumSig> = visible
            .iter()
            .copied()
            .filter(|s| s.package == self.current_package)
            .collect();
        if same_pkg.len() == 1 {
            return Ok(same_pkg[0].clone());
        }
        if visible.len() == 1 {
            return Ok(visible[0].clone());
        }
        let pkgs: Vec<&str> = visible.iter().map(|s| s.package.as_str()).collect();
        Err(SemaError {
            message: format!(
                "ambiguous type `{name}` from packages {}; use `import … as Alias` and `Alias.{name}`",
                pkgs.join(", ")
            ),
            span,
        })
    }

    /// Look up class by nominal key (`Name` or `Name@pkg`).
    pub(crate) fn class_by_nominal_key(&self, key: &str) -> Option<&ClassSig> {
        let (name, pkg) = crate::ty::split_nominal(key);
        if pkg.is_empty() {
            let list = self.classes.get(name)?;
            if list.len() == 1 {
                Some(&list[0])
            } else {
                list.iter().find(|s| s.package == self.current_package)
            }
        } else {
            self.class_in_package(name, pkg)
        }
    }

    pub(crate) fn enum_by_nominal_key(&self, key: &str) -> Option<&EnumSig> {
        let (name, pkg) = crate::ty::split_nominal(key);
        if pkg.is_empty() {
            let list = self.enums.get(name)?;
            if list.len() == 1 {
                Some(&list[0])
            } else {
                list.iter().find(|s| s.package == self.current_package)
            }
        } else {
            self.enum_in_package(name, pkg)
        }
    }

    pub(crate) fn iface_in_package(&self, name: &str, pkg: &str) -> Option<&InterfaceSig> {
        self.interfaces
            .get(name)?
            .iter()
            .find(|s| s.package == pkg)
    }

    /// C4d: resolve interface by simple name (same-package first, else unique visible).
    pub(crate) fn resolve_interface(
        &self,
        name: &str,
        span: Span,
    ) -> Result<InterfaceSig, SemaError> {
        let list = self.interfaces.get(name).ok_or_else(|| SemaError {
            message: format!("unknown type `{name}`"),
            span,
        })?;
        let visible: Vec<&InterfaceSig> = list
            .iter()
            .filter(|s| self.is_visible(name, s.is_pub, &s.package))
            .collect();
        if visible.is_empty() {
            if let Some(s) = list.first() {
                self.check_visible(name, s.is_pub, &s.package, span)?;
            }
            return Err(SemaError {
                message: format!("unknown type `{name}`"),
                span,
            });
        }
        let same_pkg: Vec<&InterfaceSig> = visible
            .iter()
            .copied()
            .filter(|s| s.package == self.current_package)
            .collect();
        if same_pkg.len() == 1 {
            return Ok(same_pkg[0].clone());
        }
        if visible.len() == 1 {
            return Ok(visible[0].clone());
        }
        let pkgs: Vec<&str> = visible.iter().map(|s| s.package.as_str()).collect();
        Err(SemaError {
            message: format!(
                "ambiguous type `{name}` from packages {}; use `import … as Alias` and `Alias.{name}`",
                pkgs.join(", ")
            ),
            span,
        })
    }

    pub(crate) fn resolve_interface_in_package(
        &self,
        name: &str,
        pkg: &str,
        span: Span,
    ) -> Result<InterfaceSig, SemaError> {
        let iface = self
            .iface_in_package(name, pkg)
            .cloned()
            .ok_or_else(|| SemaError {
                message: format!("type `{name}` is not a member of package `{pkg}`"),
                span,
            })?;
        self.check_visible(name, iface.is_pub, &iface.package, span)?;
        Ok(iface)
    }

    /// Look up interface by nominal key (`Name` or `Name@pkg`).
    pub(crate) fn iface_by_nominal_key(&self, key: &str) -> Option<&InterfaceSig> {
        let (name, pkg) = crate::ty::split_nominal(key);
        if pkg.is_empty() {
            let list = self.interfaces.get(name)?;
            if list.len() == 1 {
                Some(&list[0])
            } else {
                list.iter().find(|s| s.package == self.current_package)
            }
        } else {
            self.iface_in_package(name, pkg)
        }
    }
    pub(crate) fn check_method(
        &mut self,
        class: &ClassDecl,
        m: &FunDecl,
        expected_ret: &Ty,
    ) -> Result<(), SemaError> {
        self.locals.push(HashMap::new());
        let pkg = if class.origin_package.is_empty() {
            self.current_package.as_str()
        } else {
            class.origin_package.as_str()
        };
        let field_locals: Vec<(String, Local)> = self
            .class_in_package(&class.name.name, pkg)
            .map(|sig| {
                sig.fields
                    .iter()
                    .map(|f| {
                        (
                            f.name.clone(),
                            Local {
                                ty: f.ty.clone(),
                                mutable: f.mutable,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (name, local) in field_locals {
            self.current_locals_mut().insert(name, local);
        }
        for p in &m.params {
            let ty = self.type_from_ref(&p.ty)?;
            if self.current_locals().contains_key(&p.name.name) {
                return Err(SemaError {
                    message: format!(
                        "parameter `{}` shadows field or is duplicate",
                        p.name.name
                    ),
                    span: p.name.span,
                });
            }
            self.current_locals_mut().insert(
                p.name.name.clone(),
                Local {
                    ty,
                    mutable: false,
                },
            );
        }
        self.check_block(&m.body, expected_ret)?;
        self.locals.pop();
        Ok(())
    }

    pub(crate) fn check_fun(&mut self, f: &FunDecl, expected_ret: &Ty) -> Result<(), SemaError> {
        self.locals.push(HashMap::new());
        for p in &f.params {
            let ty = self.type_from_ref(&p.ty)?;
            if self.current_locals().contains_key(&p.name.name) {
                return Err(SemaError {
                    message: format!("duplicate parameter `{}`", p.name.name),
                    span: p.name.span,
                });
            }
            self.current_locals_mut().insert(
                p.name.name.clone(),
                Local {
                    ty,
                    mutable: false,
                },
            );
        }
        self.check_block(&f.body, expected_ret)?;
        self.locals.pop();
        Ok(())
    }

    pub(crate) fn current_locals(&self) -> &HashMap<String, Local> {
        self.locals.last().unwrap()
    }

    pub(crate) fn current_locals_mut(&mut self) -> &mut HashMap<String, Local> {
        self.locals.last_mut().unwrap()
    }

    pub(crate) fn lookup_local(&self, name: &str) -> Option<&Local> {
        for scope in self.locals.iter().rev() {
            if let Some(l) = scope.get(name) {
                return Some(l);
            }
        }
        None
    }

    /// Strip one layer of `?` for `name` in the current scope (flow narrowing).
    pub(crate) fn apply_not_null(&mut self, name: &str) {
        let Some(local) = self.lookup_local(name) else {
            return;
        };
        let Ty::Nullable(inner) = &local.ty else {
            return;
        };
        let ty = *inner.clone();
        let mutable = local.mutable;
        self.current_locals_mut().insert(
            name.to_string(),
            Local { ty, mutable },
        );
    }
}
