use std::collections::{HashMap, HashSet};

use aura_ast::{File, NominalKind};

use super::Checker;
use crate::error::SemaError;
use crate::sigs::*;
use crate::ty::Ty;

impl Checker {
    pub(crate) fn check_file(&mut self, file: &File) -> Result<CheckedFile, SemaError> {
        for i in &file.interfaces {
            if self.interfaces.contains_key(&i.name.name)
                || self.classes.contains_key(&i.name.name)
            {
                return Err(SemaError {
                    message: format!("duplicate type name `{}`", i.name.name),
                    span: i.name.span,
                });
            }
            let mut methods = HashMap::new();
            for m in &i.methods {
                if methods.contains_key(&m.name.name) {
                    return Err(SemaError {
                        message: format!("duplicate interface method `{}`", m.name.name),
                        span: m.name.span,
                    });
                }
                let params = m
                    .params
                    .iter()
                    .map(|p| self.type_from_ref(&p.ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let ret = match &m.return_type {
                    Some(t) => self.type_from_ref(t)?,
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
            self.interfaces.insert(
                i.name.name.clone(),
                InterfaceSig {
                    name: i.name.name.clone(),
                    methods,
                    span: i.span,
                },
            );
        }

        // First pass: register enum names (fields resolved in second pass with type params).
        for e in &file.enums {
            if self.enums.contains_key(&e.name.name)
                || self.interfaces.contains_key(&e.name.name)
                || self.classes.contains_key(&e.name.name)
                || self.functions.contains_key(&e.name.name)
            {
                return Err(SemaError {
                    message: format!("duplicate type/function name `{}`", e.name.name),
                    span: e.name.span,
                });
            }
            self.enums.insert(
                e.name.name.clone(),
                EnumSig {
                    name: e.name.name.clone(),
                    type_params: e.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&e.type_params),
                    variants: Vec::new(),
                    span: e.span,
                },
            );
        }

        for e in &file.enums {
            self.bind_type_params(&e.type_params)?;
            let mut variants = Vec::new();
            let mut seen_v = HashSet::new();
            for (tag, v) in e.variants.iter().enumerate() {
                if !seen_v.insert(v.name.name.clone()) {
                    return Err(SemaError {
                        message: format!("duplicate variant `{}`", v.name.name),
                        span: v.name.span,
                    });
                }
                if self.variant_to_enum.contains_key(&v.name.name)
                    || self.functions.contains_key(&v.name.name)
                    || self.classes.contains_key(&v.name.name)
                    || self.enums.contains_key(&v.name.name)
                {
                    return Err(SemaError {
                        message: format!(
                            "variant `{}` conflicts with an existing name",
                            v.name.name
                        ),
                        span: v.name.span,
                    });
                }
                let mut fields = Vec::new();
                let mut seen_f = HashSet::new();
                for f in &v.fields {
                    if !seen_f.insert(f.name.name.clone()) {
                        return Err(SemaError {
                            message: format!(
                                "duplicate field `{}` on variant `{}`",
                                f.name.name, v.name.name
                            ),
                            span: f.name.span,
                        });
                    }
                    fields.push((f.name.name.clone(), self.type_from_ref(&f.ty)?));
                }
                self.variant_to_enum
                    .insert(v.name.name.clone(), e.name.name.clone());
                variants.push(EnumVariantSig {
                    name: v.name.name.clone(),
                    tag,
                    fields,
                    span: v.span,
                });
            }
            self.enums.get_mut(&e.name.name).unwrap().variants = variants;
            self.type_params.clear();
        }

        for c in &file.classes {
            if self.classes.contains_key(&c.name.name)
                || self.interfaces.contains_key(&c.name.name)
                || self.enums.contains_key(&c.name.name)
                || self.functions.contains_key(&c.name.name)
            {
                return Err(SemaError {
                    message: format!("duplicate type/function name `{}`", c.name.name),
                    span: c.name.span,
                });
            }
            if c.kind == NominalKind::Struct && !c.implements.is_empty() {
                return Err(SemaError {
                    message: "structs cannot implement interfaces".into(),
                    span: c.name.span,
                });
            }
            if !c.type_params.is_empty() && !c.implements.is_empty() {
                return Err(SemaError {
                    message: "C2b: generic classes cannot implement interfaces yet".into(),
                    span: c.name.span,
                });
            }
            self.classes.insert(
                c.name.name.clone(),
                ClassSig {
                    name: c.name.name.clone(),
                    is_struct: c.kind == NominalKind::Struct,
                    type_params: c.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&c.type_params),
                    implements: Vec::new(),
                    fields: Vec::new(),
                    methods: HashMap::new(),
                    span: c.span,
                },
            );
        }

        for c in &file.classes {
            // Bind type params while resolving field/method types
            self.bind_type_params(&c.type_params)?;

            let mut implements = Vec::new();
            for iface in &c.implements {
                if !self.interfaces.contains_key(&iface.name) {
                    return Err(SemaError {
                        message: format!("unknown interface `{}`", iface.name),
                        span: iface.span,
                    });
                }
                if implements.contains(&iface.name) {
                    return Err(SemaError {
                        message: format!("duplicate implements `{}`", iface.name),
                        span: iface.span,
                    });
                }
                implements.push(iface.name.clone());
            }

            let mut fields = Vec::new();
            let mut seen = HashMap::new();
            for f in &c.fields {
                if seen.contains_key(&f.name.name) {
                    return Err(SemaError {
                        message: format!("duplicate field `{}`", f.name.name),
                        span: f.name.span,
                    });
                }
                let ty = self.type_from_ref(&f.ty)?;
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
                    return Err(SemaError {
                        message: "C2b: methods cannot declare their own type parameters yet"
                            .into(),
                        span: m.name.span,
                    });
                }
                if methods.contains_key(&m.name.name) {
                    return Err(SemaError {
                        message: format!("duplicate method `{}`", m.name.name),
                        span: m.name.span,
                    });
                }
                let params = m
                    .params
                    .iter()
                    .map(|p| self.type_from_ref(&p.ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let ret = match &m.return_type {
                    Some(t) => self.type_from_ref(t)?,
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

            for iface_name in &implements {
                let iface = self.interfaces.get(iface_name).unwrap().clone();
                for (mname, im) in &iface.methods {
                    let Some(cm) = methods.get(mname) else {
                        return Err(SemaError {
                            message: format!(
                                "class `{}` does not implement method `{}` required by `{}`",
                                c.name.name, mname, iface_name
                            ),
                            span: c.name.span,
                        });
                    };
                    if cm.params != im.params || cm.ret != im.ret {
                        return Err(SemaError {
                            message: format!(
                                "method `{}` on `{}` does not match interface `{}`",
                                mname, c.name.name, iface_name
                            ),
                            span: cm.span,
                        });
                    }
                }
            }

            let entry = self.classes.get_mut(&c.name.name).unwrap();
            entry.implements = implements;
            entry.fields = fields;
            entry.methods = methods;
            self.type_params.clear();
        }

        for f in &file.functions {
            if self.functions.contains_key(&f.name.name)
                || self.classes.contains_key(&f.name.name)
                || self.interfaces.contains_key(&f.name.name)
                || self.enums.contains_key(&f.name.name)
                || self.variant_to_enum.contains_key(&f.name.name)
            {
                return Err(SemaError {
                    message: format!("duplicate function `{}`", f.name.name),
                    span: f.name.span,
                });
            }
            self.bind_type_params(&f.type_params)?;
            let params = f
                .params
                .iter()
                .map(|p| self.type_from_ref(&p.ty))
                .collect::<Result<Vec<_>, _>>()?;
            let ret = match &f.return_type {
                Some(t) => self.type_from_ref(t)?,
                None => Ty::Unit,
            };
            self.functions.insert(
                f.name.name.clone(),
                FunSig {
                    name: f.name.name.clone(),
                    is_test: f.is_test,
                    type_params: f.type_params.iter().map(|p| p.name.name.clone()).collect(),
                    bounds: Self::bounds_map_from_params(&f.type_params),
                    params,
                    ret,
                    span: f.span,
                },
            );
            self.type_params.clear();
        }

        for c in &file.classes {
            self.current_class = Some(c.name.name.clone());
            self.bind_type_params(&c.type_params)?;
            for m in &c.methods {
                let ret = self
                    .classes
                    .get(&c.name.name)
                    .unwrap()
                    .methods
                    .get(&m.name.name)
                    .unwrap()
                    .ret
                    .clone();
                self.check_method(c, m, &ret)?;
            }
            self.current_class = None;
            self.type_params.clear();
        }

        for f in &file.functions {
            self.bind_type_params(&f.type_params)?;
            let ret = self.functions.get(&f.name.name).unwrap().ret.clone();
            self.check_fun(f, &ret)?;
            self.type_params.clear();
        }

        let package = file
            .package
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(".");

        let functions = file
            .functions
            .iter()
            .map(|f| self.functions.get(&f.name.name).unwrap().clone())
            .collect();
        let classes = file
            .classes
            .iter()
            .map(|c| self.classes.get(&c.name.name).unwrap().clone())
            .collect();
        let interfaces = file
            .interfaces
            .iter()
            .map(|i| self.interfaces.get(&i.name.name).unwrap().clone())
            .collect();
        let enums = file
            .enums
            .iter()
            .map(|e| self.enums.get(&e.name.name).unwrap().clone())
            .collect();

        let mut mono_classes: Vec<_> = self.mono_classes.iter().cloned().collect();
        mono_classes.sort_by(|a, b| {
            let sa = format!(
                "{}_{}",
                a.0,
                a.1.iter().map(|t| t.display()).collect::<Vec<_>>().join("_")
            );
            let sb = format!(
                "{}_{}",
                b.0,
                b.1.iter().map(|t| t.display()).collect::<Vec<_>>().join("_")
            );
            sa.cmp(&sb)
        });
        let mut mono_enums: Vec<_> = self.mono_enums.iter().cloned().collect();
        mono_enums.sort_by(|a, b| {
            let sa = format!(
                "{}_{}",
                a.0,
                a.1.iter().map(|t| t.display()).collect::<Vec<_>>().join("_")
            );
            let sb = format!(
                "{}_{}",
                b.0,
                b.1.iter().map(|t| t.display()).collect::<Vec<_>>().join("_")
            );
            sa.cmp(&sb)
        });
        let mut mono_funs: Vec<_> = self.mono_funs.iter().cloned().collect();
        mono_funs.sort_by(|a, b| {
            let sa = format!(
                "{}_{}",
                a.0,
                a.1.iter().map(|t| t.display()).collect::<Vec<_>>().join("_")
            );
            let sb = format!(
                "{}_{}",
                b.0,
                b.1.iter().map(|t| t.display()).collect::<Vec<_>>().join("_")
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
