use std::collections::{HashMap, HashSet};

use aura_ast::{decl_package, File, ForeignCallingConvention, ForeignDecl, NominalKind};

use super::Checker;
use crate::error::SemaError;
use crate::sigs::*;
use crate::ty::Ty;
use crate::util::{subst_ty, type_subst_map};

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
            .map(|p| self.type_from_ref(&p.ty))
            .collect::<Result<Vec<_>, _>>();
        let ret = foreign
            .return_type
            .as_ref()
            .map_or(Ok(Ty::Unit), |t| self.type_from_ref(t));
        // FFI-001/FFI-002: the language does not yet have a typed foreign
        // handle value.  Keep that absence fail-closed: runtime-owned task,
        // channel, and handle values must never be smuggled through a C ABI
        // and then retained across an Aura async boundary.  Primitive values
        // (including copied String values) are the only boundary proven by
        // the compiler today.
        let supported_ty = |ty: &Ty| matches!(ty, Ty::Int | Ty::Bool | Ty::String | Ty::Unit);
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
            } else if params.iter().any(|ty| !supported_ty(ty)) {
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
            } else if !supported_ty(ret) {
                self.errors.push(SemaError {
                    message:
                        "[AURA-F1-TYPE] foreign return type must be Int, Bool, String, or Unit"
                            .into(),
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
            let mut method_ok = true;
            for m in &i.methods {
                if methods.contains_key(&m.name.name) {
                    self.errors.push(SemaError {
                        message: format!("duplicate interface method `{}`", m.name.name),
                        span: m.name.span,
                    });
                    method_ok = false;
                    continue;
                }
                let params = match m
                    .params
                    .iter()
                    .map(|p| self.type_from_ref(&p.ty))
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
                methods.insert(
                    m.name.name.clone(),
                    IfaceMethodSig {
                        name: m.name.name.clone(),
                        params,
                        ret,
                        span: m.span,
                    },
                );
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
                    methods,
                    span: i.span,
                });
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
                if self.variant_to_enum.contains_key(&v.name.name)
                    || self.functions.contains_key(&v.name.name)
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
                    .insert(v.name.name.clone(), e.name.name.clone());
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
                .map(|p| self.type_from_ref(&p.ty))
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
                .map(|p| self.type_from_ref(&p.ty))
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
            let task_ty = Ty::Task(Box::new(result_ty.clone()));
            self.note_mono_ty(&task_ty);
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
                    ret: task_ty,
                    span: f.span,
                });
            self.type_params.clear();
        }

        for f in &file.async_functions {
            let pkg = decl_package(&f.origin_package, &file_pkg).to_string();
            let Some(sig) = self.fun_in_package(&f.name.name, &pkg).cloned() else {
                continue;
            };
            self.current_package = pkg;
            if let Ty::Task(result_ty) = sig.ret {
                if let Err(err) = self.check_async_fun(f, &result_ty) {
                    self.errors.push(err);
                }
            }
            self.type_params.clear();
        }

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
            // C9a: generic classes may implement interfaces (`class Box<T> : Iface<T>`).
            self.classes
                .entry(c.name.name.clone())
                .or_default()
                .push(ClassSig {
                    name: c.name.name.clone(),
                    is_pub: c.is_pub,
                    package: pkg,
                    is_struct: c.kind == NominalKind::Struct,
                    type_params: c.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&c.type_params),
                    implements: Vec::new(),
                    fields: Vec::new(),
                    methods: HashMap::new(),
                    span: c.span,
                });
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

            let mut implements: Vec<Ty> = Vec::new();
            for iface_ref in &c.implements {
                if iface_ref.nullable {
                    self.errors.push(SemaError {
                        message: "implements cannot be nullable".into(),
                        span: iface_ref.span,
                    });
                    continue;
                }
                if iface_ref.qualifier.is_some() {
                    // package-qualified implements still resolve via type_from_ref path below
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
                seen.insert(f.name.name.clone(), ());
                fields.push(FieldSig {
                    name: f.name.name.clone(),
                    ty,
                    mutable: f.mutable,
                });
            }

            let mut methods = HashMap::new();
            for m in &c.methods {
                if !m.type_params.is_empty() {
                    self.errors.push(SemaError {
                        message: "C2b: methods cannot declare their own type parameters yet".into(),
                        span: m.name.span,
                    });
                    continue;
                }
                if methods.contains_key(&m.name.name) {
                    self.errors.push(SemaError {
                        message: format!("duplicate method `{}`", m.name.name),
                        span: m.name.span,
                    });
                    continue;
                }
                let params = match m
                    .params
                    .iter()
                    .map(|p| self.type_from_ref(&p.ty))
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
                methods.insert(
                    m.name.name.clone(),
                    ClassMethodSig {
                        class: c.name.name.clone(),
                        name: m.name.name.clone(),
                        params,
                        ret,
                        span: m.span,
                    },
                );
            }

            for imp in &implements {
                let iface_key = imp.iface_key().expect("implements is Interface/App");
                let iface = self
                    .iface_by_nominal_key(iface_key)
                    .cloned()
                    .expect("implements key must resolve");
                let subst = type_subst_map(&iface.type_params, imp.iface_args());
                for (mname, im) in &iface.methods {
                    let Some(cm) = methods.get(mname) else {
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
                    let exp_params: Vec<Ty> =
                        im.params.iter().map(|p| subst_ty(p, &subst)).collect();
                    let exp_ret = subst_ty(&im.ret, &subst);
                    if cm.params != exp_params || cm.ret != exp_ret {
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
                    entry.fields = fields;
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
            if let Some(existing) = self.functions.get(&f.name.name) {
                if existing.iter().any(|s| s.package == pkg) {
                    self.errors.push(SemaError {
                        message: format!("duplicate function `{}` in package `{pkg}`", f.name.name),
                        span: f.name.span,
                    });
                    continue;
                }
            }
            if let Err(err) = self.bind_type_params(&f.type_params) {
                self.errors.push(err);
                self.type_params.clear();
                continue;
            }
            let params = match f
                .params
                .iter()
                .map(|p| self.type_from_ref(&p.ty))
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
            for m in &c.methods {
                let Some(msig) = csig.methods.get(&m.name.name) else {
                    continue;
                };
                let ret = msig.ret.clone();
                if let Err(err) = self.check_method(c, m, &ret) {
                    self.errors.push(err);
                }
            }
            self.current_class = None;
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
            mono_interfaces,
            call_instantiations: self.call_instantiations.clone(),
            lambda_tys: self.lambda_tys.clone(),
            lambda_captures: self.lambda_captures.clone(),
            ast: file.clone(),
        })
    }
}
