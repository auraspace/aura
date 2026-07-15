//! Name resolution + light typecheck for Aura C0+ / C1b (RFC-001 §6.0).

use aura_ast::*;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Unit,
    Int,
    Bool,
    String,
    /// Bottom-ish null literal; only assignable to nullable types.
    Null,
    Nullable(Box<Ty>),
    /// Nominal class type.
    Class(String),
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
        }
    }

    pub fn as_class(&self) -> Option<&str> {
        match self {
            Ty::Class(n) => Some(n),
            Ty::Nullable(inner) => inner.as_class(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FunSig {
    pub name: String,
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MethodSig {
    pub class: String,
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
    pub fields: Vec<FieldSig>,
    pub methods: HashMap<String, MethodSig>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CheckedFile {
    pub package: String,
    pub functions: Vec<FunSig>,
    pub classes: Vec<ClassSig>,
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
    locals: Vec<HashMap<String, Local>>,
    /// When typechecking a method body, the enclosing class name.
    current_class: Option<String>,
}

impl Checker {
    fn new() -> Self {
        let mut functions = HashMap::new();
        functions.insert(
            "println".into(),
            FunSig {
                name: "println".into(),
                params: vec![Ty::String],
                ret: Ty::Unit,
                span: Span::new(0, 0),
            },
        );
        Self {
            functions,
            classes: HashMap::new(),
            locals: Vec::new(),
            current_class: None,
        }
    }

    fn check_file(&mut self, file: &File) -> Result<CheckedFile, SemaError> {
        // Pass 1: reserve class names (so fields may reference peer classes)
        for c in &file.classes {
            if self.classes.contains_key(&c.name.name) {
                return Err(SemaError {
                    message: format!("duplicate class `{}`", c.name.name),
                    span: c.name.span,
                });
            }
            if self.functions.contains_key(&c.name.name) {
                return Err(SemaError {
                    message: format!("class name `{}` conflicts with a function", c.name.name),
                    span: c.name.span,
                });
            }
            self.classes.insert(
                c.name.name.clone(),
                ClassSig {
                    name: c.name.name.clone(),
                    fields: Vec::new(),
                    methods: HashMap::new(),
                    span: c.span,
                },
            );
        }

        // Pass 2: fields + method signatures
        for c in &file.classes {
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
                    MethodSig {
                        class: c.name.name.clone(),
                        name: m.name.name.clone(),
                        params,
                        ret,
                        span: m.span,
                    },
                );
            }

            let entry = self.classes.get_mut(&c.name.name).unwrap();
            entry.fields = fields;
            entry.methods = methods;
        }

        // Register free functions
        for f in &file.functions {
            if self.functions.contains_key(&f.name.name) || self.classes.contains_key(&f.name.name)
            {
                return Err(SemaError {
                    message: format!("duplicate function `{}`", f.name.name),
                    span: f.name.span,
                });
            }
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
                    params,
                    ret,
                    span: f.span,
                },
            );
        }

        // Check method bodies
        for c in &file.classes {
            self.current_class = Some(c.name.name.clone());
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
        }

        // Check free functions
        for f in &file.functions {
            let ret = self.functions.get(&f.name.name).unwrap().ret.clone();
            self.check_fun(f, &ret)?;
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

        Ok(CheckedFile {
            package,
            functions,
            classes,
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
        // Fields as locals (mutable per field)
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
                let init_ty = self.check_expr(&v.init)?;
                let ty = if let Some(ann) = &v.ty {
                    let ann_ty = self.type_from_ref(ann)?;
                    if !is_assignable(&init_ty, &ann_ty) {
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
                self.check_block(&i.then_block, expected_ret)?;
                if let Some(else_b) = &i.else_block {
                    self.check_block(else_b, expected_ret)?;
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
                    Some(e) => self.check_expr(e)?,
                    None => Ty::Unit,
                };
                if !is_assignable(&got, expected_ret) {
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

    fn check_expr(&mut self, expr: &Expr) -> Result<Ty, SemaError> {
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
                Ok(Ty::Class(class.clone()))
            }
            Expr::Int(_) => Ok(Ty::Int),
            Expr::Bool(_) => Ok(Ty::Bool),
            Expr::String(_) => Ok(Ty::String),
            Expr::Null(_) => Ok(Ty::Null),
            Expr::Group(inner, _) => self.check_expr(inner),
            Expr::Field(f) => {
                let obj_ty = self.check_expr(&f.object)?;
                let class_name = match &obj_ty {
                    Ty::Class(n) => n.clone(),
                    other => {
                        return Err(SemaError {
                            message: format!(
                                "field access requires a class type, got {}",
                                other.display()
                            ),
                            span: f.span,
                        });
                    }
                };
                let class = self.classes.get(&class_name).ok_or_else(|| SemaError {
                    message: format!("unknown class `{class_name}`"),
                    span: f.span,
                })?;
                if let Some(field) = class.fields.iter().find(|x| x.name == f.field.name) {
                    return Ok(field.ty.clone());
                }
                // Method name alone is not a value; must be called.
                if class.methods.contains_key(&f.field.name) {
                    return Err(SemaError {
                        message: format!(
                            "method `{}` must be called (use `.{}()`)",
                            f.field.name, f.field.name
                        ),
                        span: f.field.span,
                    });
                }
                Err(SemaError {
                    message: format!("unknown field `{}` on `{class_name}`", f.field.name),
                    span: f.field.span,
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
                let value_ty = self.check_expr(&a.value)?;
                if !is_assignable(&value_ty, &target) {
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
            Expr::Call(c) => self.check_call(c),
        }
    }

    fn check_call(&mut self, c: &CallExpr) -> Result<Ty, SemaError> {
        // Method call: obj.method(args)
        if let Expr::Field(fe) = c.callee.as_ref() {
            let obj_ty = self.check_expr(&fe.object)?;
            let class_name = match &obj_ty {
                Ty::Class(n) => n.clone(),
                other => {
                    return Err(SemaError {
                        message: format!(
                            "method call requires a class type, got {}",
                            other.display()
                        ),
                        span: c.span,
                    });
                }
            };
            let method = self
                .classes
                .get(&class_name)
                .and_then(|cl| cl.methods.get(&fe.field.name))
                .cloned()
                .ok_or_else(|| SemaError {
                    message: format!("unknown method `{}` on `{class_name}`", fe.field.name),
                    span: fe.field.span,
                })?;
            if c.args.len() != method.params.len() {
                return Err(SemaError {
                    message: format!(
                        "`{}.{}` expects {} argument(s), got {}",
                        class_name,
                        method.name,
                        method.params.len(),
                        c.args.len()
                    ),
                    span: c.span,
                });
            }
            for (arg, expected) in c.args.iter().zip(method.params.iter()) {
                let got = self.check_expr(arg)?;
                if !is_assignable(&got, expected) {
                    return Err(SemaError {
                        message: format!(
                            "argument type mismatch: expected {}, got {}",
                            expected.display(),
                            got.display()
                        ),
                        span: arg.span(),
                    });
                }
            }
            return Ok(method.ret);
        }

        // Free function or constructor
        let name = match c.callee.as_ref() {
            Expr::Ident(id) => id.name.clone(),
            _ => {
                return Err(SemaError {
                    message: "only direct calls and method calls supported in C1b".into(),
                    span: c.span,
                });
            }
        };

        // Constructor
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
            for (arg, field) in c.args.iter().zip(class.fields.iter()) {
                let got = self.check_expr(arg)?;
                if !is_assignable(&got, &field.ty) {
                    return Err(SemaError {
                        message: format!(
                            "constructor argument for `{}`: expected {}, got {}",
                            field.name,
                            field.ty.display(),
                            got.display()
                        ),
                        span: arg.span(),
                    });
                }
            }
            return Ok(Ty::Class(name));
        }

        let sig = self.functions.get(&name).cloned().ok_or_else(|| SemaError {
            message: format!("undefined function `{name}`"),
            span: c.callee.span(),
        })?;
        if c.args.len() != sig.params.len() {
            return Err(SemaError {
                message: format!(
                    "`{name}` expects {} argument(s), got {}",
                    sig.params.len(),
                    c.args.len()
                ),
                span: c.span,
            });
        }
        for (arg, expected) in c.args.iter().zip(sig.params.iter()) {
            let got = self.check_expr(arg)?;
            if !is_assignable(&got, expected) {
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
        Ok(sig.ret)
    }

    fn type_from_ref(&self, t: &TypeRef) -> Result<Ty, SemaError> {
        let base = match t.name.name.as_str() {
            "Unit" => Ty::Unit,
            "Int" => Ty::Int,
            "Bool" => Ty::Bool,
            "String" => Ty::String,
            other if self.classes.contains_key(other) => Ty::Class(other.to_string()),
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
}

fn is_assignable(from: &Ty, to: &Ty) -> bool {
    if from == to {
        return true;
    }
    match (from, to) {
        (Ty::Null, Ty::Nullable(_)) => true,
        (Ty::Nullable(a), Ty::Nullable(b)) => is_assignable(a, b),
        (inner, Ty::Nullable(outer)) if inner == outer.as_ref() => true,
        _ => false,
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

    #[test]
    fn assignable_null_to_nullable() {
        assert!(is_assignable(
            &Ty::Null,
            &Ty::Nullable(Box::new(Ty::String))
        ));
        assert!(!is_assignable(&Ty::Null, &Ty::String));
    }

    #[test]
    fn eq_null_and_string_opt() {
        assert!(eq_compatible(
            &Ty::Nullable(Box::new(Ty::String)),
            &Ty::Null
        ));
    }
}
