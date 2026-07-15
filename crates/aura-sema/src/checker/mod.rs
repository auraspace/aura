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

pub(crate) struct Local {
    ty: Ty,
    mutable: bool,
}

pub(crate) struct Checker {
    functions: HashMap<String, FunSig>,
    classes: HashMap<String, ClassSig>,
    enums: HashMap<String, EnumSig>,
    /// Variant name → owning enum name (unique across file for C3b).
    variant_to_enum: HashMap<String, String>,
    interfaces: HashMap<String, InterfaceSig>,
    locals: Vec<HashMap<String, Local>>,
    /// Type params in current generic scope (name → bound interface names).
    type_params: HashMap<String, Vec<String>>,
    current_class: Option<String>,
    /// Package of the item whose body/signature is being checked.
    current_package: String,
    /// `import` edges: package → set of imported package names.
    package_imports: HashMap<String, HashSet<String>>,
    mono_classes: HashSet<(String, Vec<Ty>)>,
    mono_enums: HashSet<(String, Vec<Ty>)>,
    mono_funs: HashSet<(String, Vec<Ty>)>,
    call_instantiations: HashMap<u32, CallInstantiation>,
}


impl Checker {
    pub(crate) fn new() -> Self {
        let mut functions = HashMap::new();
        functions.insert(
            "println".into(),
            FunSig {
                name: "println".into(),
                is_pub: true,
                package: String::new(),
                is_test: false,
                type_params: Vec::new(),
                bounds: HashMap::new(),
                params: vec![Ty::String],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            },
        );
        // Testing builtins (RFC-011 MVP)
        functions.insert(
            "assert".into(),
            FunSig {
                name: "assert".into(),
                is_pub: true,
                package: String::new(),
                is_test: false,
                type_params: Vec::new(),
                bounds: HashMap::new(),
                params: vec![Ty::Bool],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            },
        );
        Self {
            functions,
            classes: HashMap::new(),
            enums: HashMap::new(),
            variant_to_enum: HashMap::new(),
            interfaces: HashMap::new(),
            locals: Vec::new(),
            type_params: HashMap::new(),
            current_class: None,
            current_package: String::new(),
            package_imports: HashMap::new(),
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
    pub(crate) fn check_method(
        &mut self,
        class: &ClassDecl,
        m: &FunDecl,
        expected_ret: &Ty,
    ) -> Result<(), SemaError> {
        self.locals.push(HashMap::new());
        let field_locals: Vec<(String, Local)> = self
            .classes
            .get(&class.name.name)
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
