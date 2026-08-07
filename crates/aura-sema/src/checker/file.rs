use std::collections::{HashMap, HashSet};

use aura_ast::{decl_package, File, ForeignCallingConvention, ForeignDecl, NominalKind};

use super::{Checker, Local};
use crate::error::SemaError;
use crate::sigs::*;
use crate::ty::Ty;
use crate::util::{subst_ty, type_subst_map};

fn contains_foreign_handle(ty: &Ty) -> bool {
    match ty {
        Ty::ForeignHandle(_) => true,
        Ty::Nullable(inner) | Ty::Task(inner) | Ty::TaskHandle(inner) | Ty::Channel(inner) => {
            contains_foreign_handle(inner)
        }
        Ty::ClassApp { args, .. } | Ty::EnumApp { args, .. } | Ty::InterfaceApp { args, .. } => {
            args.iter().any(contains_foreign_handle)
        }
        Ty::Fun { params, ret } => {
            params.iter().any(contains_foreign_handle) || contains_foreign_handle(ret)
        }
        _ => false,
    }
}

fn is_valid_foreign_handle_tree(ty: &Ty) -> bool {
    match ty {
        Ty::ForeignHandle(inner) => match inner.as_ref() {
            Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Unit => true,
            Ty::ForeignHandle(_) => is_valid_foreign_handle_tree(inner),
            _ => false,
        },
        Ty::Nullable(inner) => is_valid_foreign_handle_tree(inner),
        _ => false,
    }
}

/// Check the recursive ownership shapes that the generated task ABI can copy
/// and release. Foreign handles inside arrays, structs, and enum payloads are
/// valid because their generated hooks retain/drop the opaque handle. Open
/// task/channel/function values are deliberately excluded: they are scheduler
/// objects, not aggregate payloads.
fn task_payload_foreign_handles_supported(checker: &Checker, ty: &Ty, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match ty {
        Ty::ForeignHandle(_) => true,
        Ty::Unit | Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Null => true,
        Ty::Nullable(inner) => task_payload_foreign_handles_supported(checker, inner, depth + 1),
        Ty::Class(name) | Ty::Enum(name) => {
            let (simple, package) = crate::ty::split_nominal(name);
            if simple == "Array" {
                return true;
            }
            let fields = checker
                .classes
                .get(simple)
                .and_then(|items| items.iter().find(|item| item.package == package))
                .map(|item| {
                    item.fields
                        .iter()
                        .map(|field| &field.ty)
                        .collect::<Vec<_>>()
                })
                .or_else(|| {
                    checker.enums.get(simple).and_then(|items| {
                        items
                            .iter()
                            .find(|item| item.package == package)
                            .map(|item| {
                                item.variants
                                    .iter()
                                    .flat_map(|variant| variant.fields.iter().map(|(_, ty)| ty))
                                    .collect::<Vec<_>>()
                            })
                    })
                });
            fields.is_some_and(|fields| {
                fields
                    .into_iter()
                    .all(|field| task_payload_foreign_handles_supported(checker, field, depth + 1))
            })
        }
        Ty::ClassApp { name, args } | Ty::EnumApp { name, args } => {
            let (simple, package) = crate::ty::split_nominal(name);
            if !args
                .iter()
                .all(|arg| task_payload_foreign_handles_supported(checker, arg, depth + 1))
            {
                return false;
            }
            if simple == "Array" {
                return true;
            }
            let subst = type_subst_map(
                checker
                    .classes
                    .get(simple)
                    .and_then(|items| items.iter().find(|item| item.package == package))
                    .map(|item| item.type_params.as_slice())
                    .or_else(|| {
                        checker
                            .enums
                            .get(simple)
                            .and_then(|items| items.iter().find(|item| item.package == package))
                            .map(|item| item.type_params.as_slice())
                    })
                    .unwrap_or(&[]),
                args,
            );
            let fields = checker
                .classes
                .get(simple)
                .and_then(|items| items.iter().find(|item| item.package == package))
                .map(|item| {
                    item.fields
                        .iter()
                        .map(|field| subst_ty(&field.ty, &subst))
                        .collect::<Vec<_>>()
                })
                .or_else(|| {
                    checker.enums.get(simple).and_then(|items| {
                        items
                            .iter()
                            .find(|item| item.package == package)
                            .map(|item| {
                                item.variants
                                    .iter()
                                    .flat_map(|variant| {
                                        variant.fields.iter().map(|(_, ty)| subst_ty(ty, &subst))
                                    })
                                    .collect::<Vec<_>>()
                            })
                    })
                });
            fields.is_some_and(|fields| {
                fields
                    .into_iter()
                    .all(|field| task_payload_foreign_handles_supported(checker, &field, depth + 1))
            })
        }
        Ty::TypeParam(_) | Ty::Interface(_) | Ty::InterfaceApp { .. } => false,
        Ty::Fun { .. } | Ty::Task(_) | Ty::TaskHandle(_) | Ty::Channel(_) => false,
    }
}

impl Checker {
    fn validate_foreign_decl(&mut self, foreign: &ForeignDecl) {
        if !matches!(foreign.convention, ForeignCallingConvention::C) {
            let name = match &foreign.convention {
                ForeignCallingConvention::C => "C".to_string(),
                ForeignCallingConvention::Other { name, .. } => name.clone(),
            };
            self.errors.push(SemaError {
                message: format!("[AURA-F1-CONVENTION] unsupported foreign calling convention `{name}`; only `C` is supported"),
                span: foreign.name.span,
            });
        }
        let Some(library) = &foreign.library else {
            self.errors.push(SemaError {
                message: "[AURA-F1-LIBRARY] foreign declaration requires `library = \"...\"`"
                    .into(),
                span: foreign.span,
            });
            return;
        };
        if library.name.is_empty()
            || library.name.starts_with('-')
            || library.name.contains('/')
            || library.name.contains('\\')
        {
            self.errors.push(SemaError {
                message: format!(
                    "[AURA-F1-LIBRARY] invalid foreign library `{}`; use a plain library name",
                    library.name
                ),
                span: library.span,
            });
        }
        let Some(target) = &foreign.target else {
            self.errors.push(SemaError {
                message: "[AURA-F1-TARGET] foreign declaration requires `target = \"native\"` or a supported host triple".into(),
                span: foreign.span,
            });
            return;
        };
        let host = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => "linux-x86_64",
            ("macos", "x86_64") => "macos-x86_64",
            ("macos", "aarch64") => "macos-aarch64",
            _ => "unsupported-host",
        };
        let supported = ["native", "linux-x86_64", "macos-x86_64", "macos-aarch64"];
        if !supported.contains(&target.triple.as_str()) {
            self.errors.push(SemaError {
                message: format!(
                    "[AURA-F1-TARGET] unsupported foreign target `{}`",
                    target.triple
                ),
                span: target.span,
            });
        } else if target.triple != "native" && target.triple != host {
            self.errors.push(SemaError {
                message: format!("[AURA-F1-TARGET] foreign target `{}` does not match host `{host}`; cross-target linking is not supported", target.triple),
                span: target.span,
            });
        }
        if foreign.link.is_none() {
            self.errors.push(SemaError {
                message: "[AURA-F1-LINK] foreign declaration requires `link = \"dynamic\"` or `\"static\"`".into(),
                span: foreign.span,
            });
        }
        let Some(abi) = &foreign.abi else {
            self.errors.push(SemaError {
                message: "[AURA-F1-ABI] foreign declaration requires `abi = 1, abi_id = \"c\"`"
                    .into(),
                span: foreign.span,
            });
            return;
        };
        if abi.version != 1 || abi.identity != "c" {
            self.errors.push(SemaError {
                message: format!("[AURA-F1-ABI] unsupported foreign ABI `{}` version {}; only `c` version 1 is supported", abi.identity, abi.version),
                span: abi.span,
            });
        }
        self.type_params.clear();
        let params = foreign
            .params
            .iter()
            .map(|p| self.param_ty(p))
            .collect::<Result<Vec<_>, _>>();
        let ret = foreign
            .return_type
            .as_ref()
            .map_or(Ok(Ty::Unit), |t| self.type_from_ref(t));
        // FFI-001/FFI-002: ForeignHandle<T> is the one typed opaque handle
        // accepted by the compiler.  It is only a tag around the runtime's
        // tombstoned AuraFfiOpaqueHandle*; Aura cannot dereference it.  The
        // The code generator pins this handle for the synchronous foreign
        // call. Compiler-generated TASK/AWAIT pin storage is still absent, so
        // async functions and scheduler-owned Task/TaskHandle/Channel values
        // remain fail-closed.
        let supported_ty =
            |ty: &Ty| matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Unit);
        fn foreign_handle_kind(ty: &Ty) -> Option<&'static str> {
            match ty {
                Ty::Task(_) => Some("Task"),
                Ty::TaskHandle(_) => Some("TaskHandle"),
                Ty::Channel(_) => Some("Channel"),
                Ty::Nullable(inner) => foreign_handle_kind(inner),
                _ => None,
            }
        }
        if let Ok(params) = params {
            if let Some(kind) = params.iter().find_map(foreign_handle_kind) {
                self.errors.push(SemaError {
                    message: format!(
                        "[AURA-F4-BOUNDARY] foreign parameter cannot expose runtime-owned `{kind}`; typed foreign handles are rejected until their async pin/ownership proof exists"
                    ),
                    span: foreign.span,
                });
            } else if params
                .iter()
                .any(|ty| !supported_ty(ty) && !is_valid_foreign_handle_tree(ty))
            {
                self.errors.push(SemaError { message: "[AURA-F1-TYPE] only Int, Bool, String, and Unit are supported at the FFI boundary".into(), span: foreign.span });
            }
        } else {
            self.errors.push(SemaError {
                message: "[AURA-F1-TYPE] foreign parameter type is not supported".into(),
                span: foreign.span,
            });
        }
        if let Ok(ref ret) = ret {
            if let Some(kind) = foreign_handle_kind(ret) {
                self.errors.push(SemaError {
                    message: format!(
                        "[AURA-F4-BOUNDARY] foreign return cannot expose runtime-owned `{kind}`; typed foreign handles are rejected until their async pin/ownership proof exists"
                    ),
                    span: foreign.span,
                });
            } else if !supported_ty(ret) && !is_valid_foreign_handle_tree(ret) {
                self.errors.push(SemaError {
                    message: "[AURA-F1-TYPE] foreign return type must be Int, Bool, String, Unit, or a tagged ForeignHandle<T>".into(),
                    span: foreign.span,
                });
            }
        } else {
            self.errors.push(SemaError {
                message: "[AURA-F1-TYPE] foreign return type is not supported".into(),
                span: foreign.span,
            });
        }
        if foreign.params.iter().any(|p| p.ty.reference)
            || foreign.return_type.as_ref().is_some_and(|t| t.reference)
        {
            self.errors.push(SemaError {
                message: "[AURA-F1-TYPE] foreign declarations cannot use Aura borrow references"
                    .into(),
                span: foreign.span,
            });
        }
        if let Some(failure) = &foreign.failure {
            if failure != "status" {
                self.errors.push(SemaError {
                    message: format!("[AURA-F2-FAILURE] unsupported foreign failure convention `{failure}`; only `status` is supported"),
                    span: foreign.span,
                });
            } else if !matches!(ret.as_ref(), Ok(ty) if matches!(ty, Ty::Int)) {
                self.errors.push(SemaError {
                    message:
                        "[AURA-F2-FAILURE] `failure = \"status\"` requires an Int return value"
                            .into(),
                    span: foreign.span,
                });
            }
        }
        self.type_params.clear();
    }

    /// C7g: declaration-phase errors are collected into `self.errors` and later
    /// decls/bodies still run when safe (mirror C6h body multi-error).
    pub(crate) fn check_file(&mut self, file: &File) -> Result<CheckedFile, SemaError> {
        let file_pkg = file.package.display();
        self.current_package = file_pkg.clone();
        self.type_alias_refs.clear();
        for alias in &file.type_aliases {
            let pkg = decl_package(&alias.origin_package, &file_pkg).to_string();
            self.type_alias_refs
                .entry(alias.name.name.clone())
                .or_default()
                .push((pkg, alias.ty.clone()));
        }
        self.package_imports.clear();
        self.import_aliases.clear();
        self.package_imports.entry(file_pkg.clone()).or_default();
        for imp in &file.imports {
            let from = decl_package(&imp.origin_package, &file_pkg).to_string();
            let target = imp.path.display();
            self.package_imports
                .entry(from)
                .or_default()
                .insert(target.clone());
            if let Some(alias) = &imp.alias {
                if self.import_aliases.contains_key(&alias.name) {
                    self.errors.push(SemaError {
                        message: format!("duplicate import alias `{}`", alias.name),
                        span: alias.span,
                    });
                    continue;
                }
                // Alias lives in the importing package's name space (used when
                // current_package is `from`). Store globally for C3n lookup.
                self.import_aliases.insert(alias.name.clone(), target);
            }
        }
        // Every package that contributes decls can see itself.
        for i in &file.interfaces {
            self.package_imports
                .entry(decl_package(&i.origin_package, &file_pkg).to_string())
                .or_default();
        }
        // Register class/struct headers before resolving enum fields. Enum
        // payloads may contain value structs or heap classes, and type
        // resolution must see both nominal tables in the same declaration
        // pass.
        for c in &file.classes {
            let pkg = decl_package(&c.origin_package, &file_pkg).to_string();
            if self
                .interfaces
                .get(&c.name.name)
                .map(|v| v.iter().any(|i| i.package == pkg))
                .unwrap_or(false)
                || self
                    .functions
                    .get(&c.name.name)
                    .map(|v| v.iter().any(|f| f.package == pkg))
                    .unwrap_or(false)
            {
                self.errors.push(SemaError {
                    message: format!(
                        "duplicate type/function name `{}` in package `{pkg}`",
                        c.name.name
                    ),
                    span: c.name.span,
                });
                continue;
            }
            if let Some(existing) = self.classes.get(&c.name.name) {
                if existing.iter().any(|s| s.package == pkg) {
                    self.errors.push(SemaError {
                        message: format!(
                            "duplicate type/function name `{}` in package `{pkg}`",
                            c.name.name
                        ),
                        span: c.name.span,
                    });
                    continue;
                }
            }
            if c.kind == NominalKind::Struct && !c.implements.is_empty() {
                self.errors.push(SemaError {
                    message: "structs cannot implement interfaces".into(),
                    span: c.name.span,
                });
                continue;
            }
            let has_open = c.modifiers.contains(&aura_ast::Modifier::Open);
            let has_final = c.modifiers.contains(&aura_ast::Modifier::Final);
            let has_abstract = c.modifiers.contains(&aura_ast::Modifier::Abstract);
            if has_open && has_final {
                self.errors.push(SemaError {
                    message: "class cannot be both `open` and `final`".into(),
                    span: c.name.span,
                });
            }
            if has_abstract && has_final {
                self.errors.push(SemaError {
                    message: "abstract class cannot be `final`".into(),
                    span: c.name.span,
                });
            }
            self.classes
                .entry(c.name.name.clone())
                .or_default()
                .push(ClassSig {
                    name: c.name.name.clone(),
                    is_pub: c.is_pub,
                    package: pkg,
                    is_struct: c.kind == NominalKind::Struct,
                    is_open: c.modifiers.contains(&aura_ast::Modifier::Open)
                        || c.modifiers.contains(&aura_ast::Modifier::Abstract),
                    is_abstract: c.modifiers.contains(&aura_ast::Modifier::Abstract),
                    type_params: c.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&c.type_params),
                    superclass: None,
                    implements: Vec::new(),
                    fields: Vec::new(),
                    primary_required_params: c
                        .fields
                        .iter()
                        .take_while(|f| f.default.is_none())
                        .count(),
                    constructors: Vec::new(),
                    method_overloads: HashMap::new(),
                    methods: HashMap::new(),
                    span: c.span,
                });
        }

        for e in &file.enums {
            self.package_imports
                .entry(decl_package(&e.origin_package, &file_pkg).to_string())
                .or_default();
        }
        for c in &file.classes {
            self.package_imports
                .entry(decl_package(&c.origin_package, &file_pkg).to_string())
                .or_default();
        }
        for f in &file.functions {
            self.package_imports
                .entry(decl_package(&f.origin_package, &file_pkg).to_string())
                .or_default();
        }
        for f in &file.foreign_functions {
            self.package_imports
                .entry(decl_package(&f.origin_package, &file_pkg).to_string())
                .or_default();
        }

        for i in &file.interfaces {
            let pkg = decl_package(&i.origin_package, &file_pkg).to_string();
            // C4d: same simple name ok across packages; shadow only same-package class/iface.
            if self
                .classes
                .get(&i.name.name)
                .map(|v| v.iter().any(|c| c.package == pkg))
                .unwrap_or(false)
            {
                self.errors.push(SemaError {
                    message: format!("duplicate type name `{}` in package `{pkg}`", i.name.name),
                    span: i.name.span,
                });
                continue;
            }
            if let Some(existing) = self.interfaces.get(&i.name.name) {
                if existing.iter().any(|s| s.package == pkg) {
                    self.errors.push(SemaError {
                        message: format!(
                            "duplicate type name `{}` in package `{pkg}`",
                            i.name.name
                        ),
                        span: i.name.span,
                    });
                    continue;
                }
            }
            self.current_package = pkg.clone();
            if let Err(err) = self.bind_type_params(&i.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }
            let mut methods = HashMap::new();
            let mut method_overloads: HashMap<String, Vec<IfaceMethodSig>> = HashMap::new();
            let mut method_ok = true;
            for m in &i.methods {
                let params = match m
                    .params
                    .iter()
                    .map(|p| self.param_ty(p))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(p) => p,
                    Err(e) => {
                        self.errors.push(e);
                        method_ok = false;
                        continue;
                    }
                };
                let ret = match &m.return_type {
                    Some(t) if t.reference => {
                        self.errors.push(SemaError {
                            message: "borrow references cannot be returned from functions".into(),
                            span: t.span,
                        });
                        method_ok = false;
                        continue;
                    }
                    Some(t) => match self.type_from_ref(t) {
                        Ok(t) => t,
                        Err(e) => {
                            self.errors.push(e);
                            method_ok = false;
                            continue;
                        }
                    },
                    None => Ty::Unit,
                };
                let method = IfaceMethodSig {
                    name: m.name.name.clone(),
                    params,
                    ret,
                    has_default: m.body.is_some(),
                    required_params: m.params.iter().take_while(|p| p.default.is_none()).count(),
                    is_vararg: m.params.last().is_some_and(|p| p.is_vararg),
                    span: m.span,
                };
                let overloads = method_overloads.entry(m.name.name.clone()).or_default();
                if overloads
                    .iter()
                    .any(|existing| existing.params == method.params)
                {
                    self.errors.push(SemaError {
                        message: format!(
                            "duplicate interface method overload `{}` with the same parameter types",
                            m.name.name
                        ),
                        span: m.name.span,
                    });
                    method_ok = false;
                    continue;
                }
                if !methods.contains_key(&m.name.name) {
                    methods.insert(m.name.name.clone(), method.clone());
                }
                overloads.push(method);
            }

            self.type_params.clear();
            if !method_ok && methods.is_empty() {
                continue;
            }
            self.interfaces
                .entry(i.name.name.clone())
                .or_default()
                .push(InterfaceSig {
                    name: i.name.name.clone(),
                    is_pub: i.is_pub,
                    package: pkg,
                    type_params: i.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    parents: Vec::new(),
                    method_overloads,
                    methods,
                    span: i.span,
                });
        }

        // Resolve parents only after every interface header is registered, so forward
        // declarations participate in the same visibility and generic checks.
        let mut resolved_interface_parents = Vec::new();
        for i in &file.interfaces {
            let pkg = decl_package(&i.origin_package, &file_pkg).to_string();
            self.current_package = pkg.clone();
            if let Err(err) = self.bind_type_params(&i.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }
            let mut parents = Vec::new();
            for parent_ref in &i.parents {
                match self.type_from_ref(parent_ref) {
                    Ok(parent @ (Ty::Interface(_) | Ty::InterfaceApp { .. })) => {
                        parents.push(parent);
                    }
                    Ok(other) => self.errors.push(SemaError {
                        message: format!(
                            "interface `{}` can only extend interfaces, got `{}`",
                            i.name.name,
                            other.display()
                        ),
                        span: parent_ref.span,
                    }),
                    Err(err) => self.errors.push(err),
                }
            }
            self.type_params.clear();
            resolved_interface_parents.push((i.name.name.clone(), pkg, parents));
        }
        for (name, pkg, parents) in resolved_interface_parents {
            if let Some(iface) = self
                .interfaces
                .get_mut(&name)
                .and_then(|items| items.iter_mut().find(|iface| iface.package == pkg))
            {
                iface.parents = parents;
            }
        }

        // First pass: register enum names (fields resolved in second pass with type params).
        for e in &file.enums {
            let pkg = decl_package(&e.origin_package, &file_pkg).to_string();
            if self
                .interfaces
                .get(&e.name.name)
                .map(|v| v.iter().any(|i| i.package == pkg))
                .unwrap_or(false)
                || self.functions.contains_key(&e.name.name)
            {
                self.errors.push(SemaError {
                    message: format!("duplicate type/function name `{}`", e.name.name),
                    span: e.name.span,
                });
                continue;
            }
            // C3v: same simple name allowed across packages.
            if let Some(existing) = self.enums.get(&e.name.name) {
                if existing.iter().any(|s| s.package == pkg) {
                    self.errors.push(SemaError {
                        message: format!("duplicate enum `{}` in package `{pkg}`", e.name.name),
                        span: e.name.span,
                    });
                    continue;
                }
            }
            self.enums
                .entry(e.name.name.clone())
                .or_default()
                .push(EnumSig {
                    name: e.name.name.clone(),
                    is_pub: e.is_pub,
                    package: pkg,
                    type_params: e.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&e.type_params),
                    variants: Vec::new(),
                    span: e.span,
                });
        }

        for e in &file.enums {
            let pkg = decl_package(&e.origin_package, &file_pkg).to_string();
            if self.enum_in_package(&e.name.name, &pkg).is_none() {
                continue;
            }
            self.current_package = pkg.clone();
            if let Err(err) = self.bind_type_params(&e.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }
            let mut variants = Vec::new();
            let mut seen_v = HashSet::new();
            for v in e.variants.iter() {
                if !seen_v.insert(v.name.name.clone()) {
                    self.errors.push(SemaError {
                        message: format!("duplicate variant `{}`", v.name.name),
                        span: v.name.span,
                    });
                    continue;
                }
                if self.functions.contains_key(&v.name.name)
                    || self.classes.contains_key(&v.name.name)
                    || self.enums.contains_key(&v.name.name)
                {
                    self.errors.push(SemaError {
                        message: format!(
                            "variant `{}` conflicts with an existing name",
                            v.name.name
                        ),
                        span: v.name.span,
                    });
                    continue;
                }
                let mut fields = Vec::new();
                let mut seen_f = HashSet::new();
                let mut fields_ok = true;
                for f in &v.fields {
                    if !seen_f.insert(f.name.name.clone()) {
                        self.errors.push(SemaError {
                            message: format!(
                                "duplicate field `{}` on variant `{}`",
                                f.name.name, v.name.name
                            ),
                            span: f.name.span,
                        });
                        fields_ok = false;
                        continue;
                    }
                    match self.type_from_ref(&f.ty) {
                        Ok(ty) => fields.push((f.name.name.clone(), ty)),
                        Err(err) => {
                            self.errors.push(err);
                            fields_ok = false;
                        }
                    }
                }
                if !fields_ok {
                    continue;
                }
                self.variant_to_enum
                    .entry(v.name.name.clone())
                    .or_insert_with(|| e.name.name.clone());
                let tag = variants.len();
                variants.push(EnumVariantSig {
                    name: v.name.name.clone(),
                    tag,
                    fields,
                    span: v.span,
                });
            }
            if let Some(list) = self.enums.get_mut(&e.name.name) {
                if let Some(entry) = list.iter_mut().find(|s| s.package == pkg) {
                    entry.variants = variants;
                }
            }
            self.type_params.clear();
        }

        // C22h: async declarations are callable like ordinary functions, but
        // their call result is a Task<T>; the body itself returns T.
        // F2: foreign declarations are ordinary callable signatures, but have
        // no Aura body. Register them before checking any function body,
        // including async bodies.
        let mut foreign_names = HashSet::new();
        for foreign in &file.foreign_functions {
            let pkg = decl_package(&foreign.origin_package, &file_pkg).to_string();
            self.current_package = pkg.clone();
            if !foreign_names.insert(foreign.name.name.clone())
                || self
                    .functions
                    .get(&foreign.name.name)
                    .is_some_and(|items| items.iter().any(|sig| sig.package == pkg))
            {
                self.errors.push(SemaError {
                    message: format!(
                        "duplicate foreign function `{}` in package `{pkg}`",
                        foreign.name.name
                    ),
                    span: foreign.name.span,
                });
                continue;
            }
            self.validate_foreign_decl(foreign);
            self.type_params.clear();
            let params = match foreign
                .params
                .iter()
                .map(|p| self.param_ty(p))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(params) => params,
                Err(_) => continue,
            };
            let ret = match foreign.return_type.as_ref() {
                Some(ty) => match self.type_from_ref(ty) {
                    Ok(ret) => ret,
                    Err(_) => continue,
                },
                None => Ty::Unit,
            };
            for ty in &params {
                self.note_mono_ty(ty);
            }
            self.note_mono_ty(&ret);
            self.functions
                .entry(foreign.name.name.clone())
                .or_default()
                .push(FunSig {
                    name: foreign.name.name.clone(),
                    is_pub: foreign.is_pub,
                    package: pkg,
                    is_test: false,
                    type_params: Vec::new(),
                    bounds: HashMap::new(),
                    params,
                    required_params: foreign
                        .params
                        .iter()
                        .filter(|p| p.default.is_none())
                        .count(),
                    is_vararg: foreign.params.last().is_some_and(|p| p.is_vararg),
                    ret,
                    span: foreign.span,
                });
        }

        for f in &file.async_functions {
            let pkg = decl_package(&f.origin_package, &file_pkg).to_string();
            self.current_package = pkg.clone();
            if self
                .functions
                .get(&f.name.name)
                .is_some_and(|existing| existing.iter().any(|s| s.package == pkg))
            {
                self.errors.push(SemaError {
                    message: format!("duplicate function `{}` in package `{pkg}`", f.name.name),
                    span: f.name.span,
                });
                continue;
            }
            if let Err(err) = self.bind_type_params(&f.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }
            let params = match f
                .params
                .iter()
                .map(|p| self.param_ty(p))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(params) => params,
                Err(err) => {
                    self.errors.push(err);
                    self.type_params.clear();
                    continue;
                }
            };
            let result_ty = match &f.return_type {
                Some(t) if t.reference => {
                    self.errors.push(SemaError {
                        message: "borrow references cannot be returned from async functions".into(),
                        span: t.span,
                    });
                    self.type_params.clear();
                    continue;
                }
                Some(t) => match self.type_from_ref(t) {
                    Ok(ty) => ty,
                    Err(err) => {
                        self.errors.push(err);
                        self.type_params.clear();
                        continue;
                    }
                },
                None => Ty::Unit,
            };
            if contains_foreign_handle(&result_ty)
                && !task_payload_foreign_handles_supported(self, &result_ty, 0)
            {
                self.errors.push(SemaError {
                    message: "[AURA-F4-BOUNDARY] this Task<T> payload contains a ForeignHandle in an unsupported scheduler or open generic shape".into(),
                    span: f.span,
                });
                self.type_params.clear();
                continue;
            }
            let task_ty = Ty::Task(Box::new(result_ty.clone()));
            self.note_mono_ty(&task_ty);
            self.async_funs.insert((pkg.clone(), f.name.name.clone()));
            self.functions
                .entry(f.name.name.clone())
                .or_default()
                .push(FunSig {
                    name: f.name.name.clone(),
                    is_pub: f.is_pub,
                    package: pkg,
                    is_test: f.is_test,
                    type_params: f.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&f.type_params),
                    params,
                    required_params: f.params.iter().take_while(|p| p.default.is_none()).count(),
                    is_vararg: f.params.last().is_some_and(|p| p.is_vararg),
                    ret: task_ty,
                    span: f.span,
                });
            self.type_params.clear();
        }

        for c in &file.classes {
            let pkg = decl_package(&c.origin_package, &file_pkg).to_string();
            if self.class_in_package(&c.name.name, &pkg).is_none() {
                continue;
            }
            self.current_package = pkg.clone();
            // Bind type params while resolving field/method types
            if let Err(err) = self.bind_type_params(&c.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }

            let mut superclass: Option<Ty> = None;
            let mut implements: Vec<Ty> = Vec::new();
            if let Some(parent_ref) = &c.superclass {
                match self.type_from_ref(parent_ref) {
                    Ok(parent_ty @ (Ty::Class(_) | Ty::ClassApp { .. })) => {
                        let class_key = parent_ty.class_name().unwrap_or_default();
                        if let Some(parent) = self.class_by_nominal_key(class_key) {
                            if parent.is_struct {
                                self.errors.push(SemaError {
                                    message: "structs cannot be used as superclasses".into(),
                                    span: parent_ref.span,
                                });
                            } else if parent.name == c.name.name && parent.package == pkg {
                                self.errors.push(SemaError {
                                    message: format!(
                                        "class `{}` cannot extend itself",
                                        c.name.name
                                    ),
                                    span: parent_ref.span,
                                });
                            } else if !parent.is_open {
                                self.errors.push(SemaError {
                                    message: format!(
                                        "class `{}` is final and cannot be extended",
                                        parent.name
                                    ),
                                    span: parent_ref.span,
                                });
                            } else if let Some(parent_decl) =
                                file.classes.iter().find(|candidate| {
                                    candidate.name.name == parent.name
                                        && decl_package(&candidate.origin_package, &file_pkg)
                                            == parent.package
                                })
                            {
                                if c.superclass_args.len() != parent_decl.fields.len() {
                                    self.errors.push(SemaError {
                                        message: format!(
                                            "superclass `{}` expects {} constructor argument(s), got {}",
                                            parent.name,
                                            parent_decl.fields.len(),
                                            c.superclass_args.len()
                                        ),
                                        span: parent_ref.span,
                                    });
                                } else {
                                    superclass = Some(parent_ty);
                                }
                            } else {
                                superclass = Some(parent_ty);
                            }
                        }
                    }
                    Ok(_) => self.errors.push(SemaError {
                        message: "superclass must name a class".into(),
                        span: parent_ref.span,
                    }),
                    Err(err) => self.errors.push(err),
                }
            }
            for iface_ref in &c.implements {
                if iface_ref.nullable {
                    self.errors.push(SemaError {
                        message: "implements cannot be nullable".into(),
                        span: iface_ref.span,
                    });
                    continue;
                }
                // A class in the colon list is the direct superclass; interfaces
                // remain in `implements`.
                let resolved = match self.type_from_ref(iface_ref) {
                    Ok(ty) => ty,
                    Err(err) => {
                        self.errors.push(err);
                        continue;
                    }
                };
                if matches!(resolved, Ty::Class(_) | Ty::ClassApp { .. }) {
                    let class_key = resolved.class_name().unwrap_or_default();
                    let Some(parent) = self.class_by_nominal_key(class_key).cloned() else {
                        self.errors.push(SemaError {
                            message: format!("unknown superclass `{}`", iface_ref.name.name),
                            span: iface_ref.span,
                        });
                        continue;
                    };
                    if parent.is_struct {
                        self.errors.push(SemaError {
                            message: "structs cannot be used as superclasses".into(),
                            span: iface_ref.span,
                        });
                        continue;
                    }
                    if parent.name == c.name.name && parent.package == pkg {
                        self.errors.push(SemaError {
                            message: format!("class `{}` cannot extend itself", c.name.name),
                            span: iface_ref.span,
                        });
                        continue;
                    }
                    if !parent.is_open {
                        self.errors.push(SemaError {
                            message: format!(
                                "class `{}` is final and cannot be extended",
                                parent.name
                            ),
                            span: iface_ref.span,
                        });
                        continue;
                    }
                    if superclass.is_some() {
                        self.errors.push(SemaError {
                            message: format!(
                                "class `{}` has more than one superclass",
                                c.name.name
                            ),
                            span: iface_ref.span,
                        });
                        continue;
                    }
                    let parent_decl = file.classes.iter().find(|candidate| {
                        candidate.name.name == parent.name
                            && decl_package(&candidate.origin_package, &file_pkg) == parent.package
                    });
                    let expected_args = parent_decl.map(|decl| decl.fields.len()).unwrap_or(0);
                    if expected_args != 0 {
                        self.errors.push(SemaError {
                            message: format!(
                                "superclass `{}` expects {} constructor argument(s), got 0",
                                parent.name, expected_args
                            ),
                            span: iface_ref.span,
                        });
                    } else {
                        superclass = Some(resolved);
                    }
                    continue;
                }
                let isig = match self.resolve_interface(&iface_ref.name.name, iface_ref.span) {
                    Ok(i) => i,
                    Err(err) => {
                        self.errors.push(err);
                        continue;
                    }
                };
                let type_args: Vec<Ty> = match iface_ref
                    .type_args
                    .iter()
                    .map(|a| self.type_from_ref(a))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(a) => a,
                    Err(err) => {
                        self.errors.push(err);
                        continue;
                    }
                };
                if type_args.len() != isig.type_params.len() {
                    self.errors.push(SemaError {
                        message: format!(
                            "interface `{}` expects {} type argument(s), got {}",
                            iface_ref.name.name,
                            isig.type_params.len(),
                            type_args.len()
                        ),
                        span: iface_ref.span,
                    });
                    continue;
                }
                let ikey = crate::ty::nominal_key(&isig.package, &iface_ref.name.name);
                let imp_ty = if type_args.is_empty() {
                    Ty::Interface(ikey)
                } else {
                    // C9a: open type params allowed on generic class implements
                    // (`: Iterable<T>`). Concrete mono is noted when the class is
                    // monomorphized (expand_nested_mono).
                    let app = Ty::InterfaceApp {
                        name: ikey,
                        args: type_args,
                    };
                    if !app.is_open() {
                        self.note_mono_ty(&app);
                    }
                    app
                };
                if implements.iter().any(|x| {
                    // same base interface (reject re-implement even with different args for MVP)
                    x.iface_key().map(crate::ty::split_nominal)
                        == imp_ty.iface_key().map(crate::ty::split_nominal)
                }) {
                    self.errors.push(SemaError {
                        message: format!("duplicate implements `{}`", iface_ref.name.name),
                        span: iface_ref.span,
                    });
                    continue;
                }
                implements.push(imp_ty);
            }

            let mut fields = Vec::new();
            let mut seen = HashMap::new();
            for f in &c.fields {
                if seen.contains_key(&f.name.name) {
                    self.errors.push(SemaError {
                        message: format!("duplicate field `{}`", f.name.name),
                        span: f.name.span,
                    });
                    continue;
                }
                if f.ty.reference {
                    self.errors.push(SemaError {
                        message: "borrow references cannot be stored in fields".into(),
                        span: f.ty.span,
                    });
                    continue;
                }
                let ty = match self.type_from_ref(&f.ty) {
                    Ok(t) => t,
                    Err(err) => {
                        self.errors.push(err);
                        continue;
                    }
                };
                if let Some(default) = &f.default {
                    match self.check_expr_expected(default, Some(&ty)) {
                        Ok(got) if self.is_assignable(&got, &ty) => {}
                        Ok(got) => self.errors.push(SemaError {
                            message: format!(
                                "default value for field `{}`: expected {}, got {}",
                                f.name.name,
                                ty.display(),
                                got.display()
                            ),
                            span: default.span(),
                        }),
                        Err(err) => self.errors.push(err),
                    }
                }
                seen.insert(f.name.name.clone(), ());
                fields.push(FieldSig {
                    name: f.name.name.clone(),
                    ty,
                    mutable: f.mutable,
                    visibility: f.visibility,
                });
            }

            // Superclass constructor arguments are evaluated in the child
            // constructor scope, where declared fields are available by name.
            if let Some(parent_ty) = &superclass {
                if let Some(parent_key) = parent_ty.class_name() {
                    if let Some(parent) = self.class_by_nominal_key(parent_key).cloned() {
                        let subst = type_subst_map(&parent.type_params, parent_ty.class_args());
                        self.locals.push(HashMap::new());
                        for field in &fields {
                            self.current_locals_mut().insert(
                                field.name.clone(),
                                Local {
                                    ty: field.ty.clone(),
                                    mutable: field.mutable,
                                    borrow_source: None,
                                },
                            );
                        }
                        for (arg, field) in c.superclass_args.iter().zip(parent.fields.iter()) {
                            let expected = subst_ty(&field.ty, &subst);
                            match self.check_expr_expected(arg, Some(&expected)) {
                                Ok(got) if self.is_assignable(&got, &expected) => {}
                                Ok(got) => self.errors.push(SemaError {
                                    message: format!(
                                        "superclass constructor argument for `{}`: expected {}, got {}",
                                        field.name,
                                        expected.display(),
                                        got.display()
                                    ),
                                    span: arg.span(),
                                }),
                                Err(err) => self.errors.push(err),
                            }
                        }
                        self.locals.pop();
                    }
                }
            }

            let mut constructors = Vec::new();
            for ctor in &c.constructors {
                let params = match ctor
                    .params
                    .iter()
                    .map(|p| self.param_ty(p))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(params) => params,
                    Err(err) => {
                        self.errors.push(err);
                        continue;
                    }
                };
                constructors.push(crate::sigs::ConstructorSig {
                    params,
                    required_params: ctor
                        .params
                        .iter()
                        .take_while(|p| p.default.is_none())
                        .count(),
                    is_vararg: ctor.params.last().is_some_and(|p| p.is_vararg),
                    span: ctor.span,
                });
            }
            let mut methods = HashMap::new();
            let mut method_overloads: HashMap<String, Vec<ClassMethodSig>> = HashMap::new();
            for m in &c.methods {
                let class_params = c.type_params.clone();
                if let Err(err) = self.bind_nested_type_params(&m.type_params) {
                    self.errors.push(err);
                    continue;
                }
                let params = match m
                    .params
                    .iter()
                    .map(|p| self.param_ty(p))
                    .collect::<Result<Vec<_>, _>>()
                {
                    Ok(p) => p,
                    Err(err) => {
                        self.errors.push(err);
                        continue;
                    }
                };
                let ret = match &m.return_type {
                    Some(t) if t.reference => {
                        self.errors.push(SemaError {
                            message: "borrow references cannot be returned from functions".into(),
                            span: t.span,
                        });
                        continue;
                    }
                    Some(t) => match self.type_from_ref(t) {
                        Ok(t) => t,
                        Err(err) => {
                            self.errors.push(err);
                            continue;
                        }
                    },
                    None => Ty::Unit,
                };
                self.type_params.clear();
                if let Err(err) = self.bind_type_params(&class_params) {
                    self.errors.push(err);
                    continue;
                }
                let method = ClassMethodSig {
                    class: c.name.name.clone(),
                    name: m.name.name.clone(),
                    params,
                    required_params: m.params.iter().take_while(|p| p.default.is_none()).count(),
                    is_vararg: m.params.last().is_some_and(|p| p.is_vararg),
                    ret,
                    type_params: m.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&m.type_params),
                    is_static: m.modifiers.contains(&aura_ast::Modifier::Static),
                    is_open: m.modifiers.contains(&aura_ast::Modifier::Open),
                    is_abstract: m.modifiers.contains(&aura_ast::Modifier::Abstract),
                    is_override: m.modifiers.contains(&aura_ast::Modifier::Override),
                    visibility: m.visibility,
                    span: m.span,
                };
                let overloads = method_overloads.entry(m.name.name.clone()).or_default();
                if overloads.iter().any(|existing| {
                    existing.params == method.params
                        && existing.type_params.len() == method.type_params.len()
                }) {
                    self.errors.push(SemaError {
                        message: format!(
                            "duplicate method overload `{}` with the same parameter types",
                            m.name.name
                        ),
                        span: m.name.span,
                    });
                    continue;
                }
                if !methods.contains_key(&m.name.name) {
                    methods.insert(m.name.name.clone(), method.clone());
                }
                overloads.push(method);
            }

            for imp in &implements {
                // Parent interface methods are part of this implementation contract.
                for (mname, im) in self.interface_methods(imp) {
                    if im.has_default {
                        continue;
                    }
                    let Some(cm) = methods.get(&mname) else {
                        self.errors.push(SemaError {
                            message: format!(
                                "class `{}` does not implement method `{}` required by `{}`",
                                c.name.name,
                                mname,
                                imp.display()
                            ),
                            span: c.name.span,
                        });
                        continue;
                    };
                    if cm.params != im.params || cm.ret != im.ret {
                        self.errors.push(SemaError {
                            message: format!(
                                "method `{}` on `{}` does not match interface `{}`",
                                mname,
                                c.name.name,
                                imp.display()
                            ),
                            span: cm.span,
                        });
                    }
                }
            }

            if let Some(list) = self.classes.get_mut(&c.name.name) {
                if let Some(entry) = list.iter_mut().find(|s| s.package == pkg) {
                    entry.implements = implements;
                    entry.superclass = superclass;
                    entry.fields = fields;
                    entry.primary_required_params = c
                        .fields
                        .iter()
                        .take_while(|field| field.default.is_none())
                        .count();
                    entry.constructors = constructors;
                    entry.method_overloads = method_overloads;
                    entry.methods = methods;
                }
            }
            self.type_params.clear();
        }

        // C9f: type aliases (after nominal types exist; before fun signatures).
        for t in &file.type_aliases {
            let pkg = decl_package(&t.origin_package, &file_pkg).to_string();
            self.current_package = pkg.clone();
            if self.classes.contains_key(&t.name.name)
                || self.enums.contains_key(&t.name.name)
                || self.interfaces.contains_key(&t.name.name)
                || self.type_aliases.contains_key(&t.name.name)
            {
                self.errors.push(SemaError {
                    message: format!("duplicate type name `{}`", t.name.name),
                    span: t.name.span,
                });
                continue;
            }
            let ty = match self.type_from_ref(&t.ty) {
                Ok(ty) => ty,
                Err(err) => {
                    self.errors.push(err);
                    continue;
                }
            };
            self.type_aliases
                .entry(t.name.name.clone())
                .or_default()
                .push((pkg, ty));
        }

        // C9g: top-level constants (literal values only in MVP).
        for c in &file.consts {
            let pkg = decl_package(&c.origin_package, &file_pkg).to_string();
            self.current_package = pkg.clone();
            if self.functions.contains_key(&c.name.name)
                || self.consts.contains_key(&c.name.name)
                || self.classes.contains_key(&c.name.name)
            {
                self.errors.push(SemaError {
                    message: format!("duplicate name `{}`", c.name.name),
                    span: c.name.span,
                });
                continue;
            }
            let ty = match self.type_from_ref(&c.ty) {
                Ok(ty) => ty,
                Err(err) => {
                    self.errors.push(err);
                    continue;
                }
            };
            let vty = match self.check_expr(&c.value) {
                Ok(t) => t,
                Err(err) => {
                    self.errors.push(err);
                    continue;
                }
            };
            if !self.is_assignable(&vty, &ty) {
                self.errors.push(SemaError {
                    message: format!(
                        "const `{}`: expected {}, found {}",
                        c.name.name,
                        ty.display(),
                        vty.display()
                    ),
                    span: c.value.span(),
                });
                continue;
            }
            // MVP: only Int/Bool/String/null literals (and simple unary -int).
            let ok_lit = match &c.value {
                aura_ast::Expr::Int(_)
                | aura_ast::Expr::Bool(_)
                | aura_ast::Expr::String(_)
                | aura_ast::Expr::Null(_) => true,
                aura_ast::Expr::Unary(u)
                    if matches!(u.op, aura_ast::UnOp::Neg)
                        && matches!(u.expr.as_ref(), aura_ast::Expr::Int(_)) =>
                {
                    true
                }
                _ => false,
            };
            if !ok_lit {
                self.errors.push(SemaError {
                    message: format!(
                        "const `{}` value must be a literal (Int/Bool/String/null) in C9g",
                        c.name.name
                    ),
                    span: c.value.span(),
                });
                continue;
            }
            self.consts
                .entry(c.name.name.clone())
                .or_default()
                .push((pkg, ty));
        }

        for f in &file.functions {
            let pkg = decl_package(&f.origin_package, &file_pkg).to_string();
            // Resolve param/return types in the function's package (cross-package merge).
            self.current_package = pkg.clone();
            if self
                .interfaces
                .get(&f.name.name)
                .map(|v| v.iter().any(|i| i.package == pkg))
                .unwrap_or(false)
                || self.variant_to_enum.contains_key(&f.name.name)
                || self
                    .classes
                    .get(&f.name.name)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false)
                || self
                    .enums
                    .get(&f.name.name)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false)
                || self.consts.contains_key(&f.name.name)
            {
                self.errors.push(SemaError {
                    message: format!("duplicate type/function name `{}`", f.name.name),
                    span: f.name.span,
                });
                continue;
            }
            if let Err(err) = self.bind_type_params(&f.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }
            let params = match f
                .params
                .iter()
                .map(|p| self.param_ty(p))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(p) => p,
                Err(err) => {
                    self.errors.push(err);
                    self.type_params.clear();
                    continue;
                }
            };
            for p in &params {
                self.note_mono_ty(p);
            }
            let ret = match &f.return_type {
                Some(t) if t.reference => {
                    self.errors.push(SemaError {
                        message: "borrow references cannot be returned from functions".into(),
                        span: t.span,
                    });
                    self.type_params.clear();
                    continue;
                }
                Some(t) => match self.type_from_ref(t) {
                    Ok(t) => t,
                    Err(err) => {
                        self.errors.push(err);
                        self.type_params.clear();
                        continue;
                    }
                },
                None => Ty::Unit,
            };
            self.note_mono_ty(&ret);
            if self.functions.get(&f.name.name).is_some_and(|existing| {
                existing.iter().any(|sig| {
                    sig.package == pkg
                        && sig.type_params.len() == f.type_params.len()
                        && sig.params == params
                })
            }) {
                self.errors.push(SemaError {
                    message: format!(
                        "duplicate function overload `{}` with the same parameter types in package `{pkg}`",
                        f.name.name
                    ),
                    span: f.name.span,
                });
                self.type_params.clear();
                continue;
            }
            self.functions
                .entry(f.name.name.clone())
                .or_default()
                .push(FunSig {
                    name: f.name.name.clone(),
                    is_pub: f.is_pub,
                    package: pkg,
                    is_test: f.is_test,
                    type_params: f.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&f.type_params),
                    params,
                    required_params: f.params.iter().take_while(|p| p.default.is_none()).count(),
                    is_vararg: f.params.last().is_some_and(|p| p.is_vararg),
                    ret,
                    span: f.span,
                });
            self.type_params.clear();
        }

        for c in &file.classes {
            let pkg = decl_package(&c.origin_package, &file_pkg).to_string();
            let Some(csig) = self.class_in_package(&c.name.name, &pkg).cloned() else {
                continue;
            };
            self.current_package = pkg.clone();
            self.current_class = Some(c.name.name.clone());
            if let Err(err) = self.bind_type_params(&c.type_params) {
                self.errors.push(err);
                self.current_class = None;
                self.type_params.clear();
                continue;
            }
            if let Some(parent_ty) = &csig.superclass {
                for m in &c.methods {
                    let Some(parent_method) =
                        self.class_method_in_hierarchy(parent_ty, &m.name.name)
                    else {
                        if m.modifiers.contains(&aura_ast::Modifier::Override) {
                            self.errors.push(SemaError {
                                message: format!(
                                    "method `{}` does not override a superclass method",
                                    m.name.name
                                ),
                                span: m.name.span,
                            });
                        }
                        continue;
                    };
                    let Some(method_sig) = csig.methods.get(&m.name.name) else {
                        continue;
                    };
                    if !m.modifiers.contains(&aura_ast::Modifier::Override) {
                        self.errors.push(SemaError {
                            message: format!(
                                "method `{}` shadows a superclass method; use `override`",
                                m.name.name
                            ),
                            span: m.name.span,
                        });
                    } else if !parent_method.is_open && !parent_method.is_abstract {
                        self.errors.push(SemaError {
                            message: format!(
                                "method `{}` is not open in the superclass",
                                m.name.name
                            ),
                            span: m.name.span,
                        });
                    } else if method_sig.params != parent_method.params
                        || method_sig.ret != parent_method.ret
                    {
                        self.errors.push(SemaError {
                            message: format!(
                                "method `{}` override signature does not match superclass",
                                m.name.name
                            ),
                            span: m.name.span,
                        });
                    }
                }
            } else {
                for m in &c.methods {
                    if m.modifiers.contains(&aura_ast::Modifier::Override) {
                        self.errors.push(SemaError {
                            message: format!(
                                "method `{}` does not override a superclass method",
                                m.name.name
                            ),
                            span: m.name.span,
                        });
                    }
                }
            }
            for m in &c.methods {
                if m.modifiers.contains(&aura_ast::Modifier::Abstract) {
                    if !csig.is_abstract {
                        self.errors.push(SemaError {
                            message: format!(
                                "abstract method `{}` requires an abstract class",
                                m.name.name
                            ),
                            span: m.name.span,
                        });
                    }
                    if m.modifiers.contains(&aura_ast::Modifier::Static)
                        || m.modifiers.contains(&aura_ast::Modifier::Final)
                    {
                        self.errors.push(SemaError {
                            message: format!(
                                "abstract method `{}` cannot be static or final",
                                m.name.name
                            ),
                            span: m.name.span,
                        });
                    }
                }
                let Some(msig) = csig.methods.get(&m.name.name) else {
                    continue;
                };
                if m.modifiers.contains(&aura_ast::Modifier::Abstract) {
                    continue;
                }
                // Each method gets a fresh class-generic scope before its own
                // type parameters are layered on by `check_method`.
                if let Err(err) = self.bind_type_params(&c.type_params) {
                    self.errors.push(err);
                    continue;
                }
                // Class async methods are represented in the AST as methods
                // returning `Task<T>`, while their body returns the inner `T`.
                // Check the body against that inner result just like a
                // top-level async function.
                let ret = match msig.ret.clone() {
                    Ty::Task(inner) => *inner,
                    other => other,
                };
                if let Err(err) = self.check_method(c, m, &ret) {
                    self.errors.push(err);
                }
            }
            for ctor in &c.constructors {
                self.locals.push(HashMap::new());
                for field in &csig.fields {
                    self.current_locals_mut().insert(
                        field.name.clone(),
                        Local {
                            ty: field.ty.clone(),
                            mutable: field.mutable,
                            borrow_source: None,
                        },
                    );
                }
                let mut valid = true;
                for param in &ctor.params {
                    match self.param_ty(param) {
                        Ok(ty) => {
                            self.current_locals_mut().insert(
                                param.name.name.clone(),
                                Local {
                                    ty,
                                    mutable: false,
                                    borrow_source: None,
                                },
                            );
                        }
                        Err(err) => {
                            self.errors.push(err);
                            valid = false;
                        }
                    }
                }
                if let Err(err) = self.validate_params(&ctor.params) {
                    self.errors.push(err);
                    valid = false;
                }
                if ctor.delegation_args.len() != csig.fields.len() {
                    self.errors.push(SemaError {
                        message: format!(
                            "constructor delegation expects {} argument(s), got {}",
                            csig.fields.len(),
                            ctor.delegation_args.len()
                        ),
                        span: ctor.span,
                    });
                    valid = false;
                }
                for (arg, field) in ctor.delegation_args.iter().zip(csig.fields.iter()) {
                    match self.check_expr_expected(arg, Some(&field.ty)) {
                        Ok(got) if self.is_assignable(&got, &field.ty) => {}
                        Ok(got) => {
                            self.errors.push(SemaError {
                                message: format!(
                                    "constructor delegation for `{}`: expected {}, got {}",
                                    field.name,
                                    field.ty.display(),
                                    got.display()
                                ),
                                span: arg.span(),
                            });
                            valid = false;
                        }
                        Err(err) => {
                            self.errors.push(err);
                            valid = false;
                        }
                    }
                }
                if valid {
                    if let Err(err) = self.check_block(&ctor.body, &Ty::Unit) {
                        self.errors.push(err);
                    }
                }
                self.locals.pop();
            }
            self.current_class = None;
            self.type_params.clear();
        }

        // Check async bodies only after all class signatures are complete so
        // constructors and class members are available in async code.
        for f in &file.async_functions {
            let pkg = decl_package(&f.origin_package, &file_pkg).to_string();
            let Some(sig) = self.fun_in_package(&f.name.name, &pkg).cloned() else {
                continue;
            };
            self.current_package = pkg;
            if let Err(err) = self.bind_type_params(&f.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }
            if let Ty::Task(result_ty) = sig.ret {
                if let Err(err) = self.check_async_fun(f, &result_ty) {
                    self.errors.push(err);
                }
            }
            self.type_params.clear();
        }

        for f in &file.functions {
            let pkg = decl_package(&f.origin_package, &file_pkg).to_string();
            let Some(fsig) = self.fun_in_package(&f.name.name, &pkg).cloned() else {
                continue;
            };
            self.current_package = pkg.clone();
            if let Err(err) = self.bind_type_params(&f.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }
            let ret = fsig.ret;
            if let Err(err) = self.check_fun(f, &ret) {
                self.errors.push(err);
            }
            self.type_params.clear();
        }

        let package = file_pkg;

        let mut functions: Vec<FunSig> = file
            .functions
            .iter()
            .filter_map(|f| {
                let pkg = decl_package(&f.origin_package, &package).to_string();
                self.fun_in_package(&f.name.name, &pkg).cloned()
            })
            .collect();
        for foreign in &file.foreign_functions {
            let pkg = decl_package(&foreign.origin_package, &package).to_string();
            if let Some(sig) = self.fun_in_package(&foreign.name.name, &pkg).cloned() {
                functions.push(sig);
            }
        }
        let classes = file
            .classes
            .iter()
            .filter_map(|c| {
                let pkg = decl_package(&c.origin_package, &package).to_string();
                self.class_in_package(&c.name.name, &pkg).cloned()
            })
            .collect();
        let interfaces = file
            .interfaces
            .iter()
            .filter_map(|i| {
                let pkg = decl_package(&i.origin_package, &package).to_string();
                self.iface_in_package(&i.name.name, &pkg).cloned()
            })
            .collect();
        let enums = file
            .enums
            .iter()
            .filter_map(|e| {
                let pkg = decl_package(&e.origin_package, &package).to_string();
                self.enum_in_package(&e.name.name, &pkg).cloned()
            })
            .collect();

        let mut mono_classes: Vec<_> = self.mono_classes.iter().cloned().collect();
        mono_classes.sort_by(|a, b| {
            let sa = format!(
                "{}_{}",
                a.0,
                a.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            let sb = format!(
                "{}_{}",
                b.0,
                b.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            sa.cmp(&sb)
        });
        let mut mono_enums: Vec<_> = self.mono_enums.iter().cloned().collect();
        mono_enums.sort_by(|a, b| {
            let sa = format!(
                "{}_{}",
                a.0,
                a.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            let sb = format!(
                "{}_{}",
                b.0,
                b.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            sa.cmp(&sb)
        });
        let mut mono_funs: Vec<_> = self.mono_funs.iter().cloned().collect();
        mono_funs.sort_by(|a, b| {
            let sa = format!(
                "{}_{}",
                a.0,
                a.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            let sb = format!(
                "{}_{}",
                b.0,
                b.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            sa.cmp(&sb)
        });
        let mut mono_async_funs: Vec<_> = self.mono_async_funs.iter().cloned().collect();
        mono_async_funs.sort_by(|a, b| {
            let sa = format!(
                "{}_{}",
                a.0,
                a.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            let sb = format!(
                "{}_{}",
                b.0,
                b.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            sa.cmp(&sb)
        });
        let mut mono_methods: Vec<_> = self.mono_methods.iter().cloned().collect();
        mono_methods.sort_by_key(|(class, class_args, method, method_args)| {
            format!("{class}_{class_args:?}_{method}_{method_args:?}")
        });
        let mut mono_interfaces: Vec<_> = self.mono_interfaces.iter().cloned().collect();
        mono_interfaces.sort_by(|a, b| {
            let sa = format!(
                "{}_{}",
                a.0,
                a.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            let sb = format!(
                "{}_{}",
                b.0,
                b.1.iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join("_")
            );
            sa.cmp(&sb)
        });

        Ok(CheckedFile {
            package,
            functions,
            classes,
            enums,
            interfaces,
            mono_classes,
            mono_enums,
            mono_funs,
            mono_async_funs,
            mono_methods,
            mono_interfaces,
            call_instantiations: self.call_instantiations.clone(),
            lambda_tys: self.lambda_tys.clone(),
            expr_tys: self.expr_tys.clone(),
            lambda_captures: self.lambda_captures.clone(),
            attribute_metadata: crate::attributes::collect_metadata(file),
            expansions: Vec::new(),
            ast: file.clone(),
        })
    }
}
