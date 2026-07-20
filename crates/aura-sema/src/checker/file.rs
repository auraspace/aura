use std::collections::{HashMap, HashSet};

use aura_ast::{decl_package, File, NominalKind};

use super::Checker;
use crate::error::SemaError;
use crate::sigs::*;
use crate::ty::Ty;

impl Checker {
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
            if !c.type_params.is_empty() && !c.implements.is_empty() {
                self.errors.push(SemaError {
                    message: "C2b: generic classes cannot implement interfaces yet".into(),
                    span: c.name.span,
                });
                continue;
            }
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

            let mut implements = Vec::new();
            for iface in &c.implements {
                let isig = match self.resolve_interface(&iface.name, iface.span) {
                    Ok(i) => i,
                    Err(err) => {
                        self.errors.push(err);
                        continue;
                    }
                };
                // C7i: generic interfaces parse/register but cannot be implemented yet
                // (needs type-arg mono + substituted method checks).
                if !isig.type_params.is_empty() {
                    self.errors.push(SemaError {
                        message: format!(
                            "implementing generic interface `{}` is not supported yet (C7i: monomorphized implements deferred)",
                            iface.name
                        ),
                        span: iface.span,
                    });
                    continue;
                }
                let ikey = crate::ty::nominal_key(&isig.package, &iface.name);
                if implements.contains(&ikey) {
                    self.errors.push(SemaError {
                        message: format!("duplicate implements `{}`", iface.name),
                        span: iface.span,
                    });
                    continue;
                }
                implements.push(ikey);
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

            for iface_key in &implements {
                let iface = self
                    .iface_by_nominal_key(iface_key)
                    .cloned()
                    .expect("implements key must resolve");
                for (mname, im) in &iface.methods {
                    let Some(cm) = methods.get(mname) else {
                        self.errors.push(SemaError {
                            message: format!(
                                "class `{}` does not implement method `{}` required by `{}`",
                                c.name.name, mname, iface.name
                            ),
                            span: c.name.span,
                        });
                        continue;
                    };
                    if cm.params != im.params || cm.ret != im.ret {
                        self.errors.push(SemaError {
                            message: format!(
                                "method `{}` on `{}` does not match interface `{}`",
                                mname, c.name.name, iface.name
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
            let ret = match &f.return_type {
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

        let functions = file
            .functions
            .iter()
            .filter_map(|f| {
                let pkg = decl_package(&f.origin_package, &package).to_string();
                self.fun_in_package(&f.name.name, &pkg).cloned()
            })
            .collect();
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

        Ok(CheckedFile {
            package,
            functions,
            classes,
            enums,
            interfaces,
            mono_classes,
            mono_enums,
            mono_funs,
            call_instantiations: self.call_instantiations.clone(),
            ast: file.clone(),
        })
    }
}
