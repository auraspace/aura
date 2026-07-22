//! Type checker.

mod bounds;
mod call;
mod expr;
mod file;
mod stmt;
mod types;

use std::collections::{HashMap, HashSet};

use crate::error::SemaError;
use crate::sigs::*;
use crate::ty::Ty;
use aura_ast::{ClassDecl, FunDecl, Span};

/// Builtin `Array<T>` primitives (C3j). Heap class elements allowed in C4c via Checker.
pub(crate) fn is_array_primitive_elem(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Bool | Ty::String)
}

pub(crate) struct Local {
    ty: Ty,
    mutable: bool,
    /// Lexical frame that owns the storage referenced by this borrow-derived value.
    borrow_source: Option<usize>,
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
    /// C9f: `type Name = T` expansions (simple name → package, target ty).
    type_aliases: HashMap<String, Vec<(String, Ty)>>,
    /// C9g: top-level constants (simple name → package, ty).
    consts: HashMap<String, Vec<(String, Ty)>>,
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
    mono_interfaces: HashSet<(String, Vec<Ty>)>,
    call_instantiations: HashMap<u32, CallInstantiation>,
    /// C10d: LambdaExpr.span.start → Fun type.
    lambda_tys: HashMap<u32, Ty>,
    /// C10h/C12m: LambdaExpr.span.start → captured outer locals.
    lambda_captures: HashMap<u32, Vec<crate::sigs::LambdaCapture>>,
    /// C10h: while checking a lambda body — index of the lambda params frame.
    /// Locals in frames strictly below this are free-variable captures.
    lambda_capture_base: Option<usize>,
    /// C10h/C12m: accumulating captures for the active lambda (name → ty, by_ref).
    lambda_captures_acc: Option<HashMap<String, (Ty, bool)>>,
    /// C10g: when Some, infer lambda block return type (inner = found so far).
    ret_infer: Option<Option<Ty>>,
    /// C6h: statement/body errors collected without aborting the whole file.
    pub(crate) errors: Vec<SemaError>,
}

impl Checker {
    pub(crate) fn new() -> Self {
        let mut functions: HashMap<String, Vec<FunSig>> = HashMap::new();
        for name in ["print", "println", "eprint", "eprintln"] {
            functions.insert(
                name.into(),
                vec![FunSig {
                    name: name.into(),
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
        }
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
        // C5m: runtime STW collect (roots from codegen C5g).
        functions.insert(
            "gc_collect".into(),
            vec![FunSig {
                name: "gc_collect".into(),
                is_pub: true,
                package: String::new(),
                is_test: false,
                type_params: Vec::new(),
                bounds: HashMap::new(),
                params: vec![],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            }],
        );

        // Builtin Array<T> (C3j/C4c/C6g) — monomorphized; T ∈ primitives, class, struct, or enum.
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
        // C4f: clear length (keep capacity).
        array_methods.insert(
            "clear".into(),
            ClassMethodSig {
                class: "Array".into(),
                name: "clear".into(),
                params: vec![],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            },
        );
        // C4n: isEmpty — len == 0.
        array_methods.insert(
            "isEmpty".into(),
            ClassMethodSig {
                class: "Array".into(),
                name: "isEmpty".into(),
                params: vec![],
                ret: Ty::Bool,
                span: Span::new(0, 0),
            },
        );
        // C4o: reserve(n) — ensure capacity >= n.
        array_methods.insert(
            "reserve".into(),
            ClassMethodSig {
                class: "Array".into(),
                name: "reserve".into(),
                params: vec![Ty::Int],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            },
        );
        // C9c: clone() — owning buffer copy (nested Array elems deep-copied).
        array_methods.insert(
            "clone".into(),
            ClassMethodSig {
                class: "Array".into(),
                name: "clone".into(),
                params: vec![],
                ret: Ty::ClassApp {
                    name: "Array".into(),
                    args: vec![Ty::TypeParam("T".into())],
                },
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
            type_aliases: HashMap::new(),
            consts: HashMap::new(),
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
            mono_interfaces: HashSet::new(),
            call_instantiations: HashMap::new(),
            lambda_tys: HashMap::new(),
            lambda_captures: HashMap::new(),
            lambda_capture_base: None,
            lambda_captures_acc: None,
            ret_infer: None,
            errors: Vec::new(),
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
                message: format!("`{name}` is in package `{item_package}` which is not imported"),
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
        // C4g: prefer std.* over builtins (empty package) when both are visible.
        let non_builtin: Vec<&FunSig> = visible
            .iter()
            .copied()
            .filter(|s| !s.package.is_empty())
            .collect();
        if non_builtin.len() == 1 {
            return Ok(non_builtin[0].clone());
        }
        let std_pref: Vec<&FunSig> = non_builtin
            .iter()
            .copied()
            .filter(|s| s.package.starts_with("std."))
            .collect();
        if std_pref.len() == 1 {
            return Ok(std_pref[0].clone());
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
        self.functions.get(name)?.iter().find(|s| s.package == pkg)
    }

    pub(crate) fn class_in_package(&self, name: &str, pkg: &str) -> Option<&ClassSig> {
        self.classes.get(name)?.iter().find(|s| s.package == pkg)
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
        self.enums.get(name)?.iter().find(|s| s.package == pkg)
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
        self.interfaces.get(name)?.iter().find(|s| s.package == pkg)
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

    /// C4i: true if ty is a user struct (by-value aggregate).
    pub(crate) fn is_struct_ty(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Class(n) | Ty::ClassApp { name: n, .. } => self
                .class_by_nominal_key(n)
                .map(|c| c.is_struct)
                .unwrap_or(false),
            Ty::Nullable(inner) => self.is_struct_ty(inner),
            _ => false,
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
                                borrow_source: None,
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
            let param_frame = self.locals.len() - 1;
            if self.current_locals().contains_key(&p.name.name) {
                return Err(SemaError {
                    message: format!("parameter `{}` shadows field or is duplicate", p.name.name),
                    span: p.name.span,
                });
            }
            self.current_locals_mut().insert(
                p.name.name.clone(),
                Local {
                    ty,
                    mutable: false,
                    borrow_source: if p.ty.reference {
                        Some(param_frame)
                    } else {
                        None
                    },
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
            let param_frame = self.locals.len() - 1;
            self.note_mono_ty(&ty);
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
                    borrow_source: if p.ty.reference {
                        Some(param_frame)
                    } else {
                        None
                    },
                },
            );
        }
        self.note_mono_ty(expected_ret);
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
        self.lookup_local_frame(name).map(|(_, l)| l)
    }

    /// Local binding plus the frame index it was found in (0 = outermost).
    pub(crate) fn lookup_local_frame(&self, name: &str) -> Option<(usize, &Local)> {
        for (i, scope) in self.locals.iter().enumerate().rev() {
            if let Some(l) = scope.get(name) {
                return Some((i, l));
            }
        }
        None
    }

    pub(crate) fn borrow_source_frame(&self, expr: &aura_ast::Expr) -> Option<usize> {
        match expr {
            aura_ast::Expr::Ident(id) => self
                .lookup_local_frame(&id.name)
                .and_then(|(_, local)| local.borrow_source),
            aura_ast::Expr::Group(inner, _) => self.borrow_source_frame(inner),
            aura_ast::Expr::ForceUnwrap(f) => self.borrow_source_frame(&f.expr),
            // A field view borrows from its receiver.  Restrict this propagation
            // to receivers whose lexical lifetime we can name; temporaries and
            // calls must not become borrow sources.
            aura_ast::Expr::Field(f) if !f.safe && f.field.name != "len" => match f.object.as_ref()
            {
                aura_ast::Expr::This(_) => self.locals.len().checked_sub(1),
                _ => self
                    .borrow_source_frame(f.object.as_ref())
                    .or_else(|| self.lookup_local_frame_for_owner(f.object.as_ref())),
            },
            aura_ast::Expr::If(i) => {
                fn block_source(checker: &Checker, block: &aura_ast::Block) -> Option<usize> {
                    match block.stmts.last() {
                        Some(aura_ast::Stmt::Expr(expr)) => checker.borrow_source_frame(expr),
                        _ => None,
                    }
                }
                block_source(self, &i.then_block)
                    .into_iter()
                    .chain(block_source(self, &i.else_block))
                    .min()
            }
            _ => None,
        }
    }

    fn lookup_local_frame_for_owner(&self, expr: &aura_ast::Expr) -> Option<usize> {
        match expr {
            aura_ast::Expr::Ident(id) => self.lookup_local_frame(&id.name).map(|(frame, _)| frame),
            aura_ast::Expr::Group(inner, _) => self.lookup_local_frame_for_owner(inner),
            _ => None,
        }
    }

    pub(crate) fn borrow_initializer_frame(&self, expr: &aura_ast::Expr) -> Option<usize> {
        match expr {
            aura_ast::Expr::Ident(id) => self
                .lookup_local_frame(&id.name)
                .map(|(frame, local)| local.borrow_source.unwrap_or(frame)),
            aura_ast::Expr::Group(inner, _) => self.borrow_initializer_frame(inner),
            aura_ast::Expr::ForceUnwrap(f) => self.borrow_initializer_frame(&f.expr),
            aura_ast::Expr::Field(f) if !f.safe && f.field.name != "len" => match f.object.as_ref()
            {
                aura_ast::Expr::This(_) => self.locals.len().checked_sub(1),
                _ => self
                    .borrow_source_frame(f.object.as_ref())
                    .or_else(|| self.lookup_local_frame_for_owner(f.object.as_ref())),
            },
            _ => None,
        }
    }

    /// C10h/C12k/C12l/C13e: capturable outer `val` — Int/Bool/String, heap class (GC ptr),
    /// Array (non-owning header view in env; outer scope owns the buffer), or Fun
    /// (fat pointer copy with nested env retain/release).
    /// Rejects struct, enum, interface (later).
    pub(crate) fn is_lambda_capturable_ty(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Int | Ty::Bool | Ty::String => true,
            // C13e: nested Fun capture (shallow {env,fn} + env refcount).
            Ty::Fun { .. } => true,
            Ty::Class(n) | Ty::ClassApp { name: n, .. } => {
                let simple = crate::ty::split_nominal(n).0;
                if simple == "Array" {
                    // C12l: Array is capturable as a view (codegen copies header only).
                    return true;
                }
                !self.is_struct_ty(ty)
            }
            _ => false,
        }
    }

    /// C12m/C20a: capturable outer `var` by shared mutable storage. Primitive
    /// values use boxes; heap classes, Array views, and Fun values are carried
    /// by reference so downstream codegen can preserve mutation/identity.
    pub(crate) fn is_lambda_var_capturable_ty(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Int | Ty::Bool | Ty::String | Ty::Fun { .. } => true,
            Ty::Class(n) | Ty::ClassApp { name: n, .. } => {
                let simple = crate::ty::split_nominal(n).0;
                simple == "Array" || !self.is_struct_ty(ty)
            }
            _ => false,
        }
    }

    /// C13h/C13e/C13f/C20a: human-readable list of currently supported lambda captures.
    pub(crate) fn lambda_capture_supported_list() -> &'static str {
        "`val` Int/Bool/String/class/Array (view)/Fun, `var` Int/Bool/String/class/Array/Fun (by ref)"
    }

    /// C10h/C12m/C13h: if `name` resolves to an outer local of the active lambda, record a capture.
    pub(crate) fn note_lambda_capture(
        &mut self,
        name: &str,
        frame: usize,
        ty: &Ty,
        mutable: bool,
        span: Span,
    ) -> Result<(), SemaError> {
        let Some(base) = self.lambda_capture_base else {
            return Ok(());
        };
        if frame >= base {
            return Ok(());
        }
        if self
            .lookup_local_frame(name)
            .and_then(|(_, local)| local.borrow_source)
            .is_some()
        {
            return Err(SemaError {
                message: format!("cannot capture borrow `{name}` in lambda; borrows cannot escape their lexical scope"),
                span,
            });
        }
        let supported = Self::lambda_capture_supported_list();
        let by_ref = if mutable {
            if !self.is_lambda_var_capturable_ty(ty) {
                // C13h/C20a: reject only unsupported mutable value categories.
                return Err(SemaError {
                    message: format!(
                        "cannot capture `var` `{name}` of type {} in lambda; supported captures: {supported}",
                        ty.display()
                    ),
                    span,
                });
            }
            true
        } else {
            if !self.is_lambda_capturable_ty(ty) {
                // C13h: e.g. Fun, enum, interface — list what is allowed today.
                return Err(SemaError {
                    message: format!(
                        "cannot capture `{name}` of type {} in lambda; supported captures: {supported}",
                        ty.display()
                    ),
                    span,
                });
            }
            false
        };
        if let Some(acc) = self.lambda_captures_acc.as_mut() {
            acc.entry(name.to_string())
                .or_insert_with(|| (ty.clone(), by_ref));
        }
        Ok(())
    }

    /// C5c: best-effort similar name among locals, functions, and types.
    pub(crate) fn suggest_name(&self, bad: &str) -> Option<String> {
        if bad.is_empty() {
            return None;
        }
        let mut candidates: Vec<String> = Vec::new();
        for scope in &self.locals {
            candidates.extend(scope.keys().cloned());
        }
        candidates.extend(self.functions.keys().cloned());
        candidates.extend(self.classes.keys().cloned());
        candidates.extend(self.enums.keys().cloned());
        candidates.extend(self.interfaces.keys().cloned());
        candidates.extend(self.import_aliases.keys().cloned());
        candidates.sort();
        candidates.dedup();
        let mut best: Option<(usize, String)> = None;
        for c in candidates {
            if c == bad {
                continue;
            }
            let d = edit_distance(bad, &c);
            // Only suggest close names (typos / case).
            if d == 0 || d > 2 {
                continue;
            }
            if d <= bad.len().max(1) {
                match &best {
                    None => best = Some((d, c)),
                    Some((bd, _)) if d < *bd => best = Some((d, c)),
                    Some((bd, bname)) if d == *bd && c < *bname => best = Some((d, c)),
                    _ => {}
                }
            }
        }
        best.map(|(_, s)| s)
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
        let borrow_source = local.borrow_source;
        self.current_locals_mut().insert(
            name.to_string(),
            Local {
                ty,
                mutable,
                borrow_source,
            },
        );
    }
}

/// Levenshtein distance for C5c name suggestions (small strings only).
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}
