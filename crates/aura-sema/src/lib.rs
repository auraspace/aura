//! Name resolution + typecheck for Aura C0–C2b (generics mono).

use aura_ast::{
    BinOp, Block, CallExpr, ClassDecl, Expr, File, FunDecl, Span, Stmt, TypeRef, UnOp,
};
use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Unit,
    Int,
    Bool,
    String,
    Null,
    Nullable(Box<Ty>),
    /// Non-generic class, or generic with zero type args.
    Class(String),
    /// Instantiated generic class, e.g. `Box<String>`.
    ClassApp { name: String, args: Vec<Ty> },
    Interface(String),
    /// Type parameter in a generic definition scope (`T`).
    TypeParam(String),
}

impl Ty {
    pub fn display(&self) -> String {
        match self {
            Ty::Unit => "Unit".into(),
            Ty::Int => "Int".into(),
            Ty::Bool => "Bool".into(),
            Ty::String => "String".into(),
            Ty::Null => "Null".into(),
            Ty::Nullable(inner) => format!("{}?", inner.display()),
            Ty::Class(n) => n.clone(),
            Ty::ClassApp { name, args } => {
                let a = args
                    .iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}<{a}>")
            }
            Ty::Interface(n) => n.clone(),
            Ty::TypeParam(n) => n.clone(),
        }
    }

    /// Mangle for C monomorph: `Box_String`, `id_Int`.
    pub fn mono_suffix(&self) -> String {
        match self {
            Ty::Unit => "Unit".into(),
            Ty::Int => "Int".into(),
            Ty::Bool => "Bool".into(),
            Ty::String => "String".into(),
            Ty::Null => "Null".into(),
            Ty::Nullable(inner) => format!("Opt_{}", inner.mono_suffix()),
            Ty::Class(n) => n.clone(),
            Ty::ClassApp { name, args } => {
                let a = args
                    .iter()
                    .map(|t| t.mono_suffix())
                    .collect::<Vec<_>>()
                    .join("_");
                format!("{name}_{a}")
            }
            Ty::Interface(n) => n.clone(),
            Ty::TypeParam(n) => n.clone(),
        }
    }

    pub fn class_name(&self) -> Option<&str> {
        match self {
            Ty::Class(n) => Some(n),
            Ty::ClassApp { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn class_args(&self) -> &[Ty] {
        match self {
            Ty::ClassApp { args, .. } => args,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct FunSig {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassMethodSig {
    pub class: String,
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfaceMethodSig {
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldSig {
    pub name: String,
    pub ty: Ty,
    pub mutable: bool,
}

#[derive(Debug, Clone)]
pub struct ClassSig {
    pub name: String,
    pub type_params: Vec<String>,
    pub implements: Vec<String>,
    pub fields: Vec<FieldSig>,
    pub methods: HashMap<String, ClassMethodSig>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceSig {
    pub name: String,
    pub methods: HashMap<String, IfaceMethodSig>,
    pub span: Span,
}

/// Resolved type arguments for a call site (explicit or inferred).
#[derive(Debug, Clone)]
pub struct CallInstantiation {
    pub is_constructor: bool,
    pub name: String,
    pub type_args: Vec<Ty>,
}

#[derive(Debug, Clone)]
pub struct CheckedFile {
    pub package: String,
    pub functions: Vec<FunSig>,
    pub classes: Vec<ClassSig>,
    pub interfaces: Vec<InterfaceSig>,
    /// Concrete generic class instantiations used in this file.
    pub mono_classes: Vec<(String, Vec<Ty>)>,
    /// Concrete generic function instantiations used.
    pub mono_funs: Vec<(String, Vec<Ty>)>,
    /// CallExpr.span.start → resolved type arguments (for codegen).
    pub call_instantiations: HashMap<u32, CallInstantiation>,
    pub ast: File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemaError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for SemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for SemaError {}

pub fn check_file(file: &File) -> Result<CheckedFile, SemaError> {
    let mut checker = Checker::new();
    checker.check_file(file)
}

struct Local {
    ty: Ty,
    mutable: bool,
}

struct Checker {
    functions: HashMap<String, FunSig>,
    classes: HashMap<String, ClassSig>,
    interfaces: HashMap<String, InterfaceSig>,
    locals: Vec<HashMap<String, Local>>,
    /// Type params in current generic scope (name → bound).
    type_params: HashSet<String>,
    current_class: Option<String>,
    mono_classes: HashSet<(String, Vec<Ty>)>,
    mono_funs: HashSet<(String, Vec<Ty>)>,
    call_instantiations: HashMap<u32, CallInstantiation>,
}

impl Checker {
    fn new() -> Self {
        let mut functions = HashMap::new();
        functions.insert(
            "println".into(),
            FunSig {
                name: "println".into(),
                type_params: Vec::new(),
                params: vec![Ty::String],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            },
        );
        Self {
            functions,
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            locals: Vec::new(),
            type_params: HashSet::new(),
            current_class: None,
            mono_classes: HashSet::new(),
            mono_funs: HashSet::new(),
            call_instantiations: HashMap::new(),
        }
    }

    fn check_file(&mut self, file: &File) -> Result<CheckedFile, SemaError> {
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

        for c in &file.classes {
            if self.classes.contains_key(&c.name.name)
                || self.interfaces.contains_key(&c.name.name)
                || self.functions.contains_key(&c.name.name)
            {
                return Err(SemaError {
                    message: format!("duplicate type/function name `{}`", c.name.name),
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
                    type_params: c.type_params.iter().map(|p| p.name.clone()).collect(),
                    implements: Vec::new(),
                    fields: Vec::new(),
                    methods: HashMap::new(),
                    span: c.span,
                },
            );
        }

        for c in &file.classes {
            // Bind type params while resolving field/method types
            self.type_params = c.type_params.iter().map(|p| p.name.clone()).collect();

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
            {
                return Err(SemaError {
                    message: format!("duplicate function `{}`", f.name.name),
                    span: f.name.span,
                });
            }
            self.type_params = f.type_params.iter().map(|p| p.name.clone()).collect();
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
                    type_params: f.type_params.iter().map(|p| p.name.clone()).collect(),
                    params,
                    ret,
                    span: f.span,
                },
            );
            self.type_params.clear();
        }

        for c in &file.classes {
            self.current_class = Some(c.name.name.clone());
            self.type_params = c.type_params.iter().map(|p| p.name.clone()).collect();
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
            self.type_params = f.type_params.iter().map(|p| p.name.clone()).collect();
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
            interfaces,
            mono_classes,
            mono_funs,
            call_instantiations: self.call_instantiations.clone(),
            ast: file.clone(),
        })
    }

    fn check_method(
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

    fn check_fun(&mut self, f: &FunDecl, expected_ret: &Ty) -> Result<(), SemaError> {
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

    fn current_locals(&self) -> &HashMap<String, Local> {
        self.locals.last().unwrap()
    }

    fn current_locals_mut(&mut self) -> &mut HashMap<String, Local> {
        self.locals.last_mut().unwrap()
    }

    fn lookup_local(&self, name: &str) -> Option<&Local> {
        for scope in self.locals.iter().rev() {
            if let Some(l) = scope.get(name) {
                return Some(l);
            }
        }
        None
    }

    /// Strip one layer of `?` for `name` in the current scope (flow narrowing).
    fn apply_not_null(&mut self, name: &str) {
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

    fn check_block(&mut self, block: &Block, expected_ret: &Ty) -> Result<(), SemaError> {
        self.locals.push(HashMap::new());
        for stmt in &block.stmts {
            self.check_stmt(stmt, expected_ret)?;
        }
        self.locals.pop();
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Stmt, expected_ret: &Ty) -> Result<(), SemaError> {
        match stmt {
            Stmt::Var(v) => {
                let ann_ty = match &v.ty {
                    Some(t) => Some(self.type_from_ref(t)?),
                    None => None,
                };
                let init_ty = self.check_expr_expected(&v.init, ann_ty.as_ref())?;
                let ty = if let Some(ann_ty) = ann_ty {
                    if !self.is_assignable(&init_ty, &ann_ty) {
                        return Err(SemaError {
                            message: format!(
                                "cannot assign {} to `{}` of type {}",
                                init_ty.display(),
                                v.name.name,
                                ann_ty.display()
                            ),
                            span: v.init.span(),
                        });
                    }
                    ann_ty
                } else {
                    if init_ty == Ty::Null {
                        return Err(SemaError {
                            message: "cannot infer type of `null`; add a type annotation".into(),
                            span: v.init.span(),
                        });
                    }
                    if init_ty == Ty::Unit {
                        return Err(SemaError {
                            message: "cannot bind Unit to a local".into(),
                            span: v.init.span(),
                        });
                    }
                    init_ty
                };
                if self.current_locals().contains_key(&v.name.name) {
                    return Err(SemaError {
                        message: format!("duplicate local `{}`", v.name.name),
                        span: v.name.span,
                    });
                }
                self.note_mono_ty(&ty);
                self.current_locals_mut().insert(
                    v.name.name.clone(),
                    Local {
                        ty,
                        mutable: v.mutable,
                    },
                );
                Ok(())
            }
            Stmt::If(i) => {
                let cond = self.check_expr(&i.cond)?;
                if cond != Ty::Bool {
                    return Err(SemaError {
                        message: format!("if condition must be Bool, got {}", cond.display()),
                        span: i.cond.span(),
                    });
                }
                let fact = analyze_null_check(&i.cond);

                // then-branch (narrow if `x != null`)
                self.locals.push(HashMap::new());
                if let Some((ref name, not_null_when_true)) = fact {
                    if not_null_when_true {
                        self.apply_not_null(name);
                    }
                }
                self.check_block(&i.then_block, expected_ret)?;
                self.locals.pop();

                // else-branch (narrow if `x == null` was the condition)
                if let Some(else_b) = &i.else_block {
                    self.locals.push(HashMap::new());
                    if let Some((ref name, not_null_when_true)) = fact {
                        if !not_null_when_true {
                            self.apply_not_null(name);
                        }
                    }
                    self.check_block(else_b, expected_ret)?;
                    self.locals.pop();
                }
                Ok(())
            }
            Stmt::While(w) => {
                let cond = self.check_expr(&w.cond)?;
                if cond != Ty::Bool {
                    return Err(SemaError {
                        message: format!("while condition must be Bool, got {}", cond.display()),
                        span: w.cond.span(),
                    });
                }
                self.check_block(&w.body, expected_ret)?;
                Ok(())
            }
            Stmt::Return(r) => {
                let got = match &r.value {
                    Some(e) => self.check_expr_expected(e, Some(expected_ret))?,
                    None => Ty::Unit,
                };
                if !self.is_assignable(&got, expected_ret) {
                    return Err(SemaError {
                        message: format!(
                            "return type mismatch: expected {}, got {}",
                            expected_ret.display(),
                            got.display()
                        ),
                        span: r.span,
                    });
                }
                Ok(())
            }
            Stmt::Expr(e) => {
                let _ = self.check_expr(e)?;
                Ok(())
            }
        }
    }

    fn note_mono_ty(&mut self, ty: &Ty) {
        if let Ty::ClassApp { name, args } = ty {
            if !args.is_empty() {
                self.mono_classes.insert((name.clone(), args.clone()));
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> Result<Ty, SemaError> {
        self.check_expr_expected(expr, None)
    }

    fn check_expr_expected(
        &mut self,
        expr: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Ty, SemaError> {
        match expr {
            Expr::Ident(id) => {
                if let Some(local) = self.lookup_local(&id.name) {
                    return Ok(local.ty.clone());
                }
                Err(SemaError {
                    message: format!("undefined name `{}`", id.name),
                    span: id.span,
                })
            }
            Expr::This(span) => {
                let class = self.current_class.as_ref().ok_or_else(|| SemaError {
                    message: "`this` is only valid inside methods".into(),
                    span: *span,
                })?;
                // Inside generic class, this is the open Class with type params as TypeParam
                let sig = self.classes.get(class).unwrap();
                if sig.type_params.is_empty() {
                    Ok(Ty::Class(class.clone()))
                } else {
                    Ok(Ty::ClassApp {
                        name: class.clone(),
                        args: sig
                            .type_params
                            .iter()
                            .map(|p| Ty::TypeParam(p.clone()))
                            .collect(),
                    })
                }
            }
            Expr::Int(_) => Ok(Ty::Int),
            Expr::Bool(_) => Ok(Ty::Bool),
            Expr::String(_) => Ok(Ty::String),
            Expr::Null(_) => Ok(Ty::Null),
            Expr::Group(inner, _) => self.check_expr_expected(inner, expected),
            Expr::ForceUnwrap(f) => {
                let t = self.check_expr(&f.expr)?;
                match t {
                    Ty::Nullable(inner) => Ok(*inner),
                    Ty::Null => Err(SemaError {
                        message: "cannot force-unwrap `null`".into(),
                        span: f.span,
                    }),
                    other => Ok(other), // already non-null; !! is a no-op
                }
            }
            Expr::Field(f) => {
                let obj_ty = self.check_expr(&f.object)?;
                if let Some(cname) = obj_ty.class_name() {
                    let class = self.classes.get(cname).cloned().ok_or_else(|| SemaError {
                        message: format!("unknown class `{cname}`"),
                        span: f.span,
                    })?;
                    let subst = type_subst_map(&class.type_params, obj_ty.class_args());
                    if let Some(field) = class.fields.iter().find(|x| x.name == f.field.name) {
                        return Ok(subst_ty(&field.ty, &subst));
                    }
                    if class.methods.contains_key(&f.field.name) {
                        return Err(SemaError {
                            message: format!(
                                "method `{}` must be called (use `.{}()`)",
                                f.field.name, f.field.name
                            ),
                            span: f.field.span,
                        });
                    }
                    return Err(SemaError {
                        message: format!("unknown field `{}` on `{cname}`", f.field.name),
                        span: f.field.span,
                    });
                }
                if let Ty::Interface(iface_name) = &obj_ty {
                    let iface = self.interfaces.get(iface_name).ok_or_else(|| SemaError {
                        message: format!("unknown interface `{iface_name}`"),
                        span: f.span,
                    })?;
                    if iface.methods.contains_key(&f.field.name) {
                        return Err(SemaError {
                            message: format!(
                                "interface method `{}` must be called (use `.{}()`)",
                                f.field.name, f.field.name
                            ),
                            span: f.field.span,
                        });
                    }
                    return Err(SemaError {
                        message: format!(
                            "unknown member `{}` on interface `{iface_name}`",
                            f.field.name
                        ),
                        span: f.field.span,
                    });
                }
                Err(SemaError {
                    message: format!(
                        "field access requires a class or interface type, got {}",
                        obj_ty.display()
                    ),
                    span: f.span,
                })
            }
            Expr::Unary(u) => {
                let t = self.check_expr(&u.expr)?;
                match u.op {
                    UnOp::Neg => {
                        if t != Ty::Int {
                            return Err(SemaError {
                                message: format!("unary `-` requires Int, got {}", t.display()),
                                span: u.span,
                            });
                        }
                        Ok(Ty::Int)
                    }
                    UnOp::Not => {
                        if t != Ty::Bool {
                            return Err(SemaError {
                                message: format!("unary `!` requires Bool, got {}", t.display()),
                                span: u.span,
                            });
                        }
                        Ok(Ty::Bool)
                    }
                }
            }
            Expr::Binary(b) => {
                let l = self.check_expr(&b.left)?;
                let r = self.check_expr(&b.right)?;
                match b.op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                        if l != Ty::Int || r != Ty::Int {
                            return Err(SemaError {
                                message: format!(
                                    "arithmetic requires Int operands, got {} and {}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Int)
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        if l != Ty::Int || r != Ty::Int {
                            return Err(SemaError {
                                message: format!(
                                    "comparison requires Int operands, got {} and {}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Bool)
                    }
                    BinOp::Eq | BinOp::Ne => {
                        if !eq_compatible(&l, &r) {
                            return Err(SemaError {
                                message: format!(
                                    "cannot compare {} and {}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Bool)
                    }
                    BinOp::And | BinOp::Or => {
                        if l != Ty::Bool || r != Ty::Bool {
                            return Err(SemaError {
                                message: format!(
                                    "logical op requires Bool operands, got {} and {}",
                                    l.display(),
                                    r.display()
                                ),
                                span: b.span,
                            });
                        }
                        Ok(Ty::Bool)
                    }
                }
            }
            Expr::Assign(a) => {
                let local = self.lookup_local(&a.name.name).ok_or_else(|| SemaError {
                    message: format!("undefined name `{}`", a.name.name),
                    span: a.name.span,
                })?;
                if !local.mutable {
                    return Err(SemaError {
                        message: format!("cannot assign to immutable binding `{}`", a.name.name),
                        span: a.span,
                    });
                }
                let target = local.ty.clone();
                let value_ty = self.check_expr_expected(&a.value, Some(&target))?;
                if !self.is_assignable(&value_ty, &target) {
                    return Err(SemaError {
                        message: format!(
                            "cannot assign {} to `{}` of type {}",
                            value_ty.display(),
                            a.name.name,
                            target.display()
                        ),
                        span: a.value.span(),
                    });
                }
                Ok(target)
            }
            Expr::Call(c) => self.check_call(c, expected),
        }
    }

    fn check_call(&mut self, c: &CallExpr, expected: Option<&Ty>) -> Result<Ty, SemaError> {
        if let Expr::Field(fe) = c.callee.as_ref() {
            if !c.type_args.is_empty() {
                return Err(SemaError {
                    message: "type arguments not allowed on method calls in C2b".into(),
                    span: c.span,
                });
            }
            let obj_ty = self.check_expr(&fe.object)?;

            if let Some(cname) = obj_ty.class_name() {
                let class = self.classes.get(cname).cloned().ok_or_else(|| SemaError {
                    message: format!("unknown class `{cname}`"),
                    span: c.span,
                })?;
                let method = class
                    .methods
                    .get(&fe.field.name)
                    .cloned()
                    .ok_or_else(|| SemaError {
                        message: format!("unknown method `{}` on `{cname}`", fe.field.name),
                        span: fe.field.span,
                    })?;
                let subst = type_subst_map(&class.type_params, obj_ty.class_args());
                let params: Vec<Ty> = method.params.iter().map(|p| subst_ty(p, &subst)).collect();
                let ret = subst_ty(&method.ret, &subst);
                self.check_args(
                    &params,
                    &c.args,
                    &format!("{}.{}", cname, method.name),
                    c.span,
                )?;
                self.note_mono_ty(&obj_ty);
                return Ok(ret);
            }

            if let Ty::Interface(iface_name) = &obj_ty {
                let method = self
                    .interfaces
                    .get(iface_name)
                    .and_then(|i| i.methods.get(&fe.field.name))
                    .cloned()
                    .ok_or_else(|| SemaError {
                        message: format!(
                            "unknown method `{}` on interface `{iface_name}`",
                            fe.field.name
                        ),
                        span: fe.field.span,
                    })?;
                self.check_args(
                    &method.params,
                    &c.args,
                    &format!("{}.{}", iface_name, method.name),
                    c.span,
                )?;
                return Ok(method.ret);
            }

            return Err(SemaError {
                message: format!(
                    "method call requires a class or interface type, got {}",
                    obj_ty.display()
                ),
                span: c.span,
            });
        }

        let name = match c.callee.as_ref() {
            Expr::Ident(id) => id.name.clone(),
            _ => {
                return Err(SemaError {
                    message: "only direct calls and method calls supported".into(),
                    span: c.span,
                });
            }
        };

        // Constructor (possibly generic)
        if let Some(class) = self.classes.get(&name).cloned() {
            if c.args.len() != class.fields.len() {
                return Err(SemaError {
                    message: format!(
                        "constructor `{}` expects {} argument(s), got {}",
                        name,
                        class.fields.len(),
                        c.args.len()
                    ),
                    span: c.span,
                });
            }

            let type_args = self.resolve_ctor_type_args(&class, c, expected)?;

            let subst = type_subst_map(&class.type_params, &type_args);
            for (arg, field) in c.args.iter().zip(class.fields.iter()) {
                let exp = subst_ty(&field.ty, &subst);
                let got = self.check_expr_expected(arg, Some(&exp))?;
                if !self.is_assignable(&got, &exp) {
                    return Err(SemaError {
                        message: format!(
                            "constructor argument for `{}`: expected {}, got {}",
                            field.name,
                            exp.display(),
                            got.display()
                        ),
                        span: arg.span(),
                    });
                }
            }

            if !type_args.is_empty() {
                self.call_instantiations.insert(
                    c.span.start,
                    CallInstantiation {
                        is_constructor: true,
                        name: name.clone(),
                        type_args: type_args.clone(),
                    },
                );
            }

            let ret = if type_args.is_empty() {
                Ty::Class(name)
            } else {
                let t = Ty::ClassApp {
                    name,
                    args: type_args,
                };
                self.note_mono_ty(&t);
                t
            };
            return Ok(ret);
        }

        // Free function (possibly generic)
        let sig = self.functions.get(&name).cloned().ok_or_else(|| SemaError {
            message: format!("undefined function `{name}`"),
            span: c.callee.span(),
        })?;

        let type_args = self.resolve_fun_type_args(&sig, c, expected)?;

        let subst = type_subst_map(&sig.type_params, &type_args);
        let params: Vec<Ty> = sig.params.iter().map(|p| subst_ty(p, &subst)).collect();
        let ret = subst_ty(&sig.ret, &subst);
        self.check_args(&params, &c.args, &name, c.span)?;
        if !type_args.is_empty() {
            self.call_instantiations.insert(
                c.span.start,
                CallInstantiation {
                    is_constructor: false,
                    name: name.clone(),
                    type_args: type_args.clone(),
                },
            );
            self.mono_funs.insert((name, type_args));
        }
        Ok(ret)
    }

    fn resolve_ctor_type_args(
        &mut self,
        class: &ClassSig,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Vec<Ty>, SemaError> {
        let what = format!("type `{}`", class.name);
        if class.type_params.is_empty() {
            if !c.type_args.is_empty() {
                return Err(SemaError {
                    message: format!("{what} does not take type arguments"),
                    span: c.span,
                });
            }
            return Ok(Vec::new());
        }
        if !c.type_args.is_empty() {
            if c.type_args.len() != class.type_params.len() {
                return Err(SemaError {
                    message: format!(
                        "{what} expects {} type argument(s), got {}",
                        class.type_params.len(),
                        c.type_args.len()
                    ),
                    span: c.span,
                });
            }
            return c
                .type_args
                .iter()
                .map(|t| self.type_from_ref(t))
                .collect::<Result<Vec<_>, _>>();
        }
        if let Some(Ty::ClassApp { name: n, args }) = expected {
            if n == &class.name && args.len() == class.type_params.len() {
                return Ok(args.clone());
            }
        }
        let mut arg_tys = Vec::new();
        for a in &c.args {
            arg_tys.push(self.check_expr(a)?);
        }
        let patterns: Vec<&Ty> = class.fields.iter().map(|f| &f.ty).collect();
        self.infer_type_args_from_patterns(
            &class.type_params,
            &patterns,
            &arg_tys,
            c.span,
            &format!("constructor `{}`", class.name),
        )
    }

    fn resolve_fun_type_args(
        &mut self,
        sig: &FunSig,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Vec<Ty>, SemaError> {
        let what = format!("function `{}`", sig.name);
        if sig.type_params.is_empty() {
            if !c.type_args.is_empty() {
                return Err(SemaError {
                    message: format!("{what} does not take type arguments"),
                    span: c.span,
                });
            }
            return Ok(Vec::new());
        }
        if !c.type_args.is_empty() {
            if c.type_args.len() != sig.type_params.len() {
                return Err(SemaError {
                    message: format!(
                        "{what} expects {} type argument(s), got {}",
                        sig.type_params.len(),
                        c.type_args.len()
                    ),
                    span: c.span,
                });
            }
            return c
                .type_args
                .iter()
                .map(|t| self.type_from_ref(t))
                .collect::<Result<Vec<_>, _>>();
        }
        if let Some(exp) = expected {
            let mut map = HashMap::new();
            if unify_ty(&sig.ret, exp, &mut map).is_ok() {
                let mut args = Vec::new();
                let mut ok = true;
                for p in &sig.type_params {
                    if let Some(t) = map.get(p) {
                        if *t == Ty::Null {
                            ok = false;
                            break;
                        }
                        args.push(t.clone());
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok && args.len() == sig.type_params.len() {
                    return Ok(args);
                }
            }
        }
        let mut arg_tys = Vec::new();
        for a in &c.args {
            arg_tys.push(self.check_expr(a)?);
        }
        if arg_tys.len() != sig.params.len() {
            return Err(SemaError {
                message: format!(
                    "`{}` expects {} argument(s), got {}",
                    sig.name,
                    sig.params.len(),
                    arg_tys.len()
                ),
                span: c.span,
            });
        }
        let patterns: Vec<&Ty> = sig.params.iter().collect();
        self.infer_type_args_from_patterns(
            &sig.type_params,
            &patterns,
            &arg_tys,
            c.span,
            &what,
        )
    }

    fn infer_type_args_from_patterns(
        &self,
        type_params: &[String],
        patterns: &[&Ty],
        concretes: &[Ty],
        span: Span,
        what: &str,
    ) -> Result<Vec<Ty>, SemaError> {
        let mut map = HashMap::new();
        for (pat, con) in patterns.iter().zip(concretes.iter()) {
            if let Err(msg) = unify_ty(pat, con, &mut map) {
                return Err(SemaError {
                    message: format!("{what}: {msg}"),
                    span,
                });
            }
        }
        let mut out = Vec::new();
        for p in type_params {
            match map.get(p) {
                Some(t) if *t != Ty::Null => out.push(t.clone()),
                _ => {
                    return Err(SemaError {
                        message: format!(
                            "cannot infer type argument `{p}` for {what}; write it explicitly (e.g. `<…>`)"
                        ),
                        span,
                    });
                }
            }
        }
        Ok(out)
    }

    fn check_args(
        &mut self,
        params: &[Ty],
        args: &[Expr],
        name: &str,
        span: Span,
    ) -> Result<(), SemaError> {
        if args.len() != params.len() {
            return Err(SemaError {
                message: format!(
                    "`{name}` expects {} argument(s), got {}",
                    params.len(),
                    args.len()
                ),
                span,
            });
        }
        for (arg, expected) in args.iter().zip(params.iter()) {
            let got = self.check_expr_expected(arg, Some(expected))?;
            if !self.is_assignable(&got, expected) {
                return Err(SemaError {
                    message: format!(
                        "argument type mismatch for `{name}`: expected {}, got {}",
                        expected.display(),
                        got.display()
                    ),
                    span: arg.span(),
                });
            }
        }
        Ok(())
    }

    fn type_from_ref(&self, t: &TypeRef) -> Result<Ty, SemaError> {
        let type_args: Vec<Ty> = t
            .type_args
            .iter()
            .map(|a| self.type_from_ref(a))
            .collect::<Result<Vec<_>, _>>()?;

        let base = match t.name.name.as_str() {
            "Unit" => Ty::Unit,
            "Int" => Ty::Int,
            "Bool" => Ty::Bool,
            "String" => Ty::String,
            other if self.type_params.contains(other) => {
                if !type_args.is_empty() {
                    return Err(SemaError {
                        message: format!("type parameter `{other}` cannot take type arguments"),
                        span: t.span,
                    });
                }
                Ty::TypeParam(other.to_string())
            }
            other if self.classes.contains_key(other) => {
                let class = self.classes.get(other).unwrap();
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
                if type_args.is_empty() {
                    Ty::Class(other.to_string())
                } else {
                    Ty::ClassApp {
                        name: other.to_string(),
                        args: type_args,
                    }
                }
            }
            other if self.interfaces.contains_key(other) => {
                if !type_args.is_empty() {
                    return Err(SemaError {
                        message: format!("interface `{other}` cannot take type arguments in C2b"),
                        span: t.span,
                    });
                }
                Ty::Interface(other.to_string())
            }
            other => {
                return Err(SemaError {
                    message: format!("unknown type `{other}`"),
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

    fn is_assignable(&self, from: &Ty, to: &Ty) -> bool {
        if from == to {
            return true;
        }
        match (from, to) {
            (Ty::Null, Ty::Nullable(_)) => true,
            (Ty::Nullable(a), Ty::Nullable(b)) => self.is_assignable(a, b),
            (inner, Ty::Nullable(outer)) if self.is_assignable(inner, outer) => true,
            (Ty::Class(c), Ty::Interface(i)) => self
                .classes
                .get(c)
                .map(|cs| cs.implements.iter().any(|x| x == i))
                .unwrap_or(false),
            // Type params only match themselves (handled by ==)
            _ => false,
        }
    }
}

/// Detect `x != null` / `x == null` / `null != x` / `null == x` for flow narrowing.
/// Returns `(local_name, not_null_when_condition_true)`.
fn analyze_null_check(cond: &Expr) -> Option<(String, bool)> {
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
fn unify_ty(pattern: &Ty, concrete: &Ty, map: &mut HashMap<String, Ty>) -> Result<(), String> {
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
        (
            Ty::ClassApp {
                name: n1,
                args: a1,
            },
            Ty::ClassApp {
                name: n2,
                args: a2,
            },
        ) if n1 == n2 && a1.len() == a2.len() => {
            for (a, b) in a1.iter().zip(a2.iter()) {
                unify_ty(a, b, map)?;
            }
            Ok(())
        }
        (a, b) if a == b => Ok(()),
        (a, b) => Err(format!(
            "cannot unify {} with {}",
            a.display(),
            b.display()
        )),
    }
}

fn type_subst_map(params: &[String], args: &[Ty]) -> HashMap<String, Ty> {
    params
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .collect()
}

fn subst_ty(ty: &Ty, map: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::TypeParam(name) => map.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Nullable(inner) => Ty::Nullable(Box::new(subst_ty(inner, map))),
        Ty::ClassApp { name, args } => Ty::ClassApp {
            name: name.clone(),
            args: args.iter().map(|a| subst_ty(a, map)).collect(),
        },
        other => other.clone(),
    }
}

fn eq_compatible(a: &Ty, b: &Ty) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use aura_parser::parse_file;

    #[test]
    fn mono_suffix() {
        let t = Ty::ClassApp {
            name: "Box".into(),
            args: vec![Ty::String],
        };
        assert_eq!(t.mono_suffix(), "Box_String");
    }

    #[test]
    fn null_flow_narrows_in_if() {
        let src = r#"
package t
fun f(name: String?): String {
  if (name != null) {
    return name
  } else {
    return "x"
  }
}
fun main() {}
"#;
        let file = parse_file(src).expect("parse");
        check_file(&file).expect("check should allow name after != null");
    }

    #[test]
    fn null_flow_rejects_without_check() {
        let src = r#"
package t
fun f(name: String?): String {
  return name
}
fun main() {}
"#;
        let file = parse_file(src).expect("parse");
        let err = check_file(&file).expect_err("should reject String? as String");
        assert!(err.message.contains("return type mismatch") || err.message.contains("String"));
    }

    #[test]
    fn infers_box_and_id_type_args() {
        let src = r#"
package t
class Box<T>(val value: T) {
  fun get(): T { return this.value }
}
fun id<T>(x: T): T { return x }
fun main() {
  val a = Box("hi")
  val b: Box<String> = Box("x")
  id("y")
}
"#;
        let file = parse_file(src).expect("parse");
        let checked = check_file(&file).expect("check");
        assert!(
            checked
                .mono_classes
                .iter()
                .any(|(n, a)| n == "Box" && a == &[Ty::String])
        );
        assert!(
            checked
                .mono_funs
                .iter()
                .any(|(n, a)| n == "id" && a == &[Ty::String])
        );
        assert!(!checked.call_instantiations.is_empty());
    }
}
