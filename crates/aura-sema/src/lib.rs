//! Name resolution + typecheck for Aura C0–C3b (enums, match, Result).

use aura_ast::{
    BinOp, Block, CallExpr, ClassDecl, Expr, File, FunDecl, MatchStmt, NominalKind, Pattern, Span,
    Stmt, TypeParam, TypeRef, UnOp,
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
    /// Non-generic enum.
    Enum(String),
    /// Instantiated generic enum, e.g. `Result<Int, String>`.
    EnumApp { name: String, args: Vec<Ty> },
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
            Ty::Class(n) | Ty::Enum(n) => n.clone(),
            Ty::ClassApp { name, args } | Ty::EnumApp { name, args } => {
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

    /// Mangle for C monomorph: `Box_String`, `Result_Int_String`.
    pub fn mono_suffix(&self) -> String {
        match self {
            Ty::Unit => "Unit".into(),
            Ty::Int => "Int".into(),
            Ty::Bool => "Bool".into(),
            Ty::String => "String".into(),
            Ty::Null => "Null".into(),
            Ty::Nullable(inner) => format!("Opt_{}", inner.mono_suffix()),
            Ty::Class(n) | Ty::Enum(n) => n.clone(),
            Ty::ClassApp { name, args } | Ty::EnumApp { name, args } => {
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

    pub fn enum_name(&self) -> Option<&str> {
        match self {
            Ty::Enum(n) => Some(n),
            Ty::EnumApp { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn enum_args(&self) -> &[Ty] {
        match self {
            Ty::EnumApp { args, .. } => args,
            _ => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct FunSig {
    pub name: String,
    pub type_params: Vec<String>,
    /// Bounds per type param name (interface names in C2e).
    pub bounds: HashMap<String, Vec<String>>,
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
    /// `false` = class, `true` = struct (value type; no implements).
    pub is_struct: bool,
    pub type_params: Vec<String>,
    /// Bounds per type param name (interface names in C2e).
    pub bounds: HashMap<String, Vec<String>>,
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

#[derive(Debug, Clone)]
pub struct EnumVariantSig {
    pub name: String,
    pub tag: usize,
    pub fields: Vec<(String, Ty)>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumSig {
    pub name: String,
    pub type_params: Vec<String>,
    pub bounds: HashMap<String, Vec<String>>,
    pub variants: Vec<EnumVariantSig>,
    pub span: Span,
}

/// Resolved type arguments for a call site (explicit or inferred).
#[derive(Debug, Clone)]
pub struct CallInstantiation {
    pub is_constructor: bool,
    pub name: String,
    pub type_args: Vec<Ty>,
    /// Set for enum variant constructors (`Ok`, `Err`, …).
    pub variant: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CheckedFile {
    pub package: String,
    pub functions: Vec<FunSig>,
    pub classes: Vec<ClassSig>,
    pub enums: Vec<EnumSig>,
    pub interfaces: Vec<InterfaceSig>,
    /// Concrete generic class instantiations used in this file.
    pub mono_classes: Vec<(String, Vec<Ty>)>,
    /// Concrete generic enum instantiations used.
    pub mono_enums: Vec<(String, Vec<Ty>)>,
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
    enums: HashMap<String, EnumSig>,
    /// Variant name → owning enum name (unique across file for C3b).
    variant_to_enum: HashMap<String, String>,
    interfaces: HashMap<String, InterfaceSig>,
    locals: Vec<HashMap<String, Local>>,
    /// Type params in current generic scope (name → bound interface names).
    type_params: HashMap<String, Vec<String>>,
    current_class: Option<String>,
    mono_classes: HashSet<(String, Vec<Ty>)>,
    mono_enums: HashSet<(String, Vec<Ty>)>,
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
                bounds: HashMap::new(),
                params: vec![Ty::String],
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
            mono_classes: HashSet::new(),
            mono_enums: HashSet::new(),
            mono_funs: HashSet::new(),
            call_instantiations: HashMap::new(),
        }
    }

    fn bind_type_params(&mut self, params: &[TypeParam]) -> Result<(), SemaError> {
        self.type_params.clear();
        for p in params {
            if self.type_params.contains_key(&p.name.name) {
                return Err(SemaError {
                    message: format!("duplicate type parameter `{}`", p.name.name),
                    span: p.name.span,
                });
            }
            let mut bounds = Vec::new();
            for b in &p.bounds {
                if !self.interfaces.contains_key(&b.name) {
                    // Allow class bounds later; C2e: interfaces only
                    return Err(SemaError {
                        message: format!(
                            "unknown bound `{}` (C2e: bounds must be interfaces)",
                            b.name
                        ),
                        span: b.span,
                    });
                }
                if bounds.contains(&b.name) {
                    return Err(SemaError {
                        message: format!("duplicate bound `{}` on `{}`", b.name, p.name.name),
                        span: b.span,
                    });
                }
                bounds.push(b.name.clone());
            }
            self.type_params.insert(p.name.name.clone(), bounds);
        }
        Ok(())
    }

    fn bounds_map_from_params(params: &[TypeParam]) -> HashMap<String, Vec<String>> {
        params
            .iter()
            .map(|p| {
                (
                    p.name.name.clone(),
                    p.bounds.iter().map(|b| b.name.clone()).collect(),
                )
            })
            .collect()
    }

    /// Does `ty` satisfy all interface bounds?
    fn satisfies_bounds(&self, ty: &Ty, bounds: &[String], span: Span) -> Result<(), SemaError> {
        for b in bounds {
            if !self.ty_implements(ty, b) {
                return Err(SemaError {
                    message: format!(
                        "type {} does not satisfy bound `{}`",
                        ty.display(),
                        b
                    ),
                    span,
                });
            }
        }
        Ok(())
    }

    fn ty_implements(&self, ty: &Ty, iface: &str) -> bool {
        match ty {
            Ty::Class(c) | Ty::ClassApp { name: c, .. } => self
                .classes
                .get(c)
                .map(|cs| cs.implements.iter().any(|x| x == iface))
                .unwrap_or(false),
            Ty::Interface(i) => i == iface,
            Ty::TypeParam(p) => self
                .type_params
                .get(p)
                .map(|bs| bs.iter().any(|x| x == iface))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn check_type_args_bounds(
        &self,
        param_names: &[String],
        bounds: &HashMap<String, Vec<String>>,
        type_args: &[Ty],
        span: Span,
        what: &str,
    ) -> Result<(), SemaError> {
        for (name, arg) in param_names.iter().zip(type_args.iter()) {
            if let Some(bs) = bounds.get(name) {
                if let Err(mut e) = self.satisfies_bounds(arg, bs, span) {
                    e.message = format!("{what}: {}", e.message);
                    return Err(e);
                }
            }
        }
        Ok(())
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

    fn check_match(&mut self, m: &MatchStmt, expected_ret: &Ty) -> Result<(), SemaError> {
        let scrut_ty = self.check_expr(&m.scrutinee)?;
        let Some(ename) = scrut_ty.enum_name() else {
            return Err(SemaError {
                message: format!(
                    "`match` requires an enum type, got {}",
                    scrut_ty.display()
                ),
                span: m.scrutinee.span(),
            });
        };
        let enum_sig = self.enums.get(ename).cloned().ok_or_else(|| SemaError {
            message: format!("unknown enum `{ename}`"),
            span: m.scrutinee.span(),
        })?;
        let type_args = scrut_ty.enum_args().to_vec();
        let subst = type_subst_map(&enum_sig.type_params, &type_args);
        self.note_mono_ty(&scrut_ty);

        let mut covered = HashSet::new();
        for arm in &m.arms {
            let Pattern::Variant {
                name,
                bindings,
                span,
            } = &arm.pattern;
            let variant = enum_sig
                .variants
                .iter()
                .find(|v| v.name == name.name)
                .ok_or_else(|| SemaError {
                    message: format!("unknown variant `{}` for enum `{ename}`", name.name),
                    span: *span,
                })?;
            if !covered.insert(variant.name.clone()) {
                return Err(SemaError {
                    message: format!("duplicate match arm for variant `{}`", variant.name),
                    span: *span,
                });
            }
            if bindings.len() != variant.fields.len() {
                return Err(SemaError {
                    message: format!(
                        "variant `{}` has {} field(s), pattern binds {}",
                        variant.name,
                        variant.fields.len(),
                        bindings.len()
                    ),
                    span: *span,
                });
            }
            self.locals.push(HashMap::new());
            for (bind, (fname, fty)) in bindings.iter().zip(variant.fields.iter()) {
                if self.current_locals().contains_key(&bind.name) {
                    return Err(SemaError {
                        message: format!("duplicate binding `{}`", bind.name),
                        span: bind.span,
                    });
                }
                let _ = fname;
                let ty = subst_ty(fty, &subst);
                self.current_locals_mut().insert(
                    bind.name.clone(),
                    Local {
                        ty,
                        mutable: false,
                    },
                );
            }
            for stmt in &arm.body.stmts {
                self.check_stmt(stmt, expected_ret)?;
            }
            self.locals.pop();
        }
        let missing: Vec<_> = enum_sig
            .variants
            .iter()
            .filter(|v| !covered.contains(&v.name))
            .map(|v| v.name.clone())
            .collect();
        if !missing.is_empty() {
            return Err(SemaError {
                message: format!(
                    "non-exhaustive match on `{ename}`; missing: {}",
                    missing.join(", ")
                ),
                span: m.span,
            });
        }
        Ok(())
    }

    fn check_stmt(&mut self, stmt: &Stmt, expected_ret: &Ty) -> Result<(), SemaError> {
        match stmt {
            Stmt::Match(m) => self.check_match(m, expected_ret),
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
        match ty {
            Ty::ClassApp { name, args } if !args.is_empty() => {
                self.mono_classes.insert((name.clone(), args.clone()));
            }
            Ty::EnumApp { name, args } if !args.is_empty() => {
                self.mono_enums.insert((name.clone(), args.clone()));
            }
            _ => {}
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

            // Type param with interface bounds: call methods from any bound.
            if let Ty::TypeParam(pname) = &obj_ty {
                let bounds = self.type_params.get(pname).cloned().unwrap_or_default();
                for iface_name in &bounds {
                    if let Some(method) = self
                        .interfaces
                        .get(iface_name)
                        .and_then(|i| i.methods.get(&fe.field.name))
                        .cloned()
                    {
                        self.check_args(
                            &method.params,
                            &c.args,
                            &format!("{}.{}", iface_name, method.name),
                            c.span,
                        )?;
                        return Ok(method.ret);
                    }
                }
                if bounds.is_empty() {
                    return Err(SemaError {
                        message: format!(
                            "cannot call method `{}` on unbounded type parameter `{pname}`",
                            fe.field.name
                        ),
                        span: fe.field.span,
                    });
                }
                return Err(SemaError {
                    message: format!(
                        "method `{}` not found on bounds of `{pname}` ({})",
                        fe.field.name,
                        bounds.join(", ")
                    ),
                    span: fe.field.span,
                });
            }

            return Err(SemaError {
                message: format!(
                    "method call requires a class, interface, or bounded type parameter, got {}",
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
            self.check_type_args_bounds(
                &class.type_params,
                &class.bounds,
                &type_args,
                c.span,
                &format!("constructor `{}`", class.name),
            )?;

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
                        variant: None,
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

        // Enum variant constructor: Ok(...), Red()
        if let Some(enum_name) = self.variant_to_enum.get(&name).cloned() {
            return self.check_variant_ctor(&enum_name, &name, c, expected);
        }

        // Free function (possibly generic)
        let sig = self.functions.get(&name).cloned().ok_or_else(|| SemaError {
            message: format!("undefined function `{name}`"),
            span: c.callee.span(),
        })?;

        let type_args = self.resolve_fun_type_args(&sig, c, expected)?;
        self.check_type_args_bounds(
            &sig.type_params,
            &sig.bounds,
            &type_args,
            c.span,
            &format!("function `{}`", sig.name),
        )?;

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
                    variant: None,
                },
            );
            self.mono_funs.insert((name, type_args));
        }
        Ok(ret)
    }

    fn check_variant_ctor(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Ty, SemaError> {
        if !c.type_args.is_empty() {
            return Err(SemaError {
                message: "type arguments go on the enum type, not the variant constructor".into(),
                span: c.span,
            });
        }
        let enum_sig = self.enums.get(enum_name).cloned().ok_or_else(|| SemaError {
            message: format!("unknown enum `{enum_name}`"),
            span: c.span,
        })?;
        let variant = enum_sig
            .variants
            .iter()
            .find(|v| v.name == variant_name)
            .cloned()
            .ok_or_else(|| SemaError {
                message: format!("unknown variant `{variant_name}`"),
                span: c.span,
            })?;
        if c.args.len() != variant.fields.len() {
            return Err(SemaError {
                message: format!(
                    "variant `{variant_name}` expects {} argument(s), got {}",
                    variant.fields.len(),
                    c.args.len()
                ),
                span: c.span,
            });
        }

        let type_args = self.resolve_enum_type_args(&enum_sig, &variant, c, expected)?;
        self.check_type_args_bounds(
            &enum_sig.type_params,
            &enum_sig.bounds,
            &type_args,
            c.span,
            &format!("enum `{enum_name}`"),
        )?;
        let subst = type_subst_map(&enum_sig.type_params, &type_args);
        for (arg, (_fname, fty)) in c.args.iter().zip(variant.fields.iter()) {
            let exp = subst_ty(fty, &subst);
            let got = self.check_expr_expected(arg, Some(&exp))?;
            if !self.is_assignable(&got, &exp) {
                return Err(SemaError {
                    message: format!(
                        "argument type mismatch for `{variant_name}`: expected {}, got {}",
                        exp.display(),
                        got.display()
                    ),
                    span: arg.span(),
                });
            }
        }

        let ret = if type_args.is_empty() {
            Ty::Enum(enum_name.to_string())
        } else {
            let t = Ty::EnumApp {
                name: enum_name.to_string(),
                args: type_args.clone(),
            };
            self.note_mono_ty(&t);
            t
        };
        self.call_instantiations.insert(
            c.span.start,
            CallInstantiation {
                is_constructor: true,
                name: enum_name.to_string(),
                type_args,
                variant: Some(variant_name.to_string()),
            },
        );
        Ok(ret)
    }

    fn resolve_enum_type_args(
        &mut self,
        enum_sig: &EnumSig,
        variant: &EnumVariantSig,
        c: &CallExpr,
        expected: Option<&Ty>,
    ) -> Result<Vec<Ty>, SemaError> {
        if enum_sig.type_params.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(Ty::EnumApp { name, args }) = expected {
            if name == &enum_sig.name && args.len() == enum_sig.type_params.len() {
                return Ok(args.clone());
            }
        }
        if let Some(Ty::Enum(name)) = expected {
            if name == &enum_sig.name && enum_sig.type_params.is_empty() {
                return Ok(Vec::new());
            }
        }
        let mut arg_tys = Vec::new();
        for a in &c.args {
            arg_tys.push(self.check_expr(a)?);
        }
        let patterns: Vec<&Ty> = variant.fields.iter().map(|(_, t)| t).collect();
        self.infer_type_args_from_patterns(
            &enum_sig.type_params,
            &patterns,
            &arg_tys,
            c.span,
            &format!("variant `{}`", variant.name),
        )
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
            other if self.type_params.contains_key(other) => {
                if !type_args.is_empty() {
                    return Err(SemaError {
                        message: format!("type parameter `{other}` cannot take type arguments"),
                        span: t.span,
                    });
                }
                Ty::TypeParam(other.to_string())
            }
            other if self.classes.contains_key(other) => {
                let class = self.classes.get(other).unwrap().clone();
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
                if type_args.is_empty() {
                    Ty::Class(other.to_string())
                } else {
                    Ty::ClassApp {
                        name: other.to_string(),
                        args: type_args,
                    }
                }
            }
            other if self.enums.contains_key(other) => {
                let enum_sig = self.enums.get(other).unwrap().clone();
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
                if type_args.is_empty() {
                    Ty::Enum(other.to_string())
                } else {
                    Ty::EnumApp {
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
            (Ty::ClassApp { name: c, .. }, Ty::Interface(i)) => self
                .classes
                .get(c)
                .map(|cs| cs.implements.iter().any(|x| x == i))
                .unwrap_or(false),
            // Bounded type param is assignable to its interface bounds
            (Ty::TypeParam(p), Ty::Interface(i)) => self
                .type_params
                .get(p)
                .map(|bs| bs.iter().any(|x| x == i))
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
        )
        | (
            Ty::EnumApp {
                name: n1,
                args: a1,
            },
            Ty::EnumApp {
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
        Ty::EnumApp { name, args } => Ty::EnumApp {
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
    fn result_enum_and_match() {
        let src = r#"
package t
enum Result<T, E> {
  case Ok(value: T)
  case Err(error: E)
}
fun f(): Result<Int, String> {
  return Ok(1)
}
fun g(r: Result<Int, String>): Int {
  match (r) {
    case Ok(v) => { return v }
    case Err(e) => { return 0 }
  }
}
fun main() {}
"#;
        let file = parse_file(src).expect("parse");
        let checked = check_file(&file).expect("check");
        assert!(checked.enums.iter().any(|e| e.name == "Result"));
        assert!(checked
            .mono_enums
            .iter()
            .any(|(n, a)| n == "Result" && a == &[Ty::Int, Ty::String]));
    }

    #[test]
    fn match_nonexhaustive_errors() {
        let src = r#"
package t
enum Color { case Red case Green }
fun f(c: Color) {
  match (c) {
    case Red => { println("r") }
  }
}
fun main() {}
"#;
        let file = parse_file(src).expect("parse");
        let err = check_file(&file).expect_err("non-exhaustive");
        assert!(err.message.contains("non-exhaustive") || err.message.contains("Green"), "{}", err.message);
    }

    #[test]
    fn struct_fields_and_methods() {
        let src = r#"
package t
struct Point(val x: Int, val y: Int) {
  fun sum(): Int { return this.x + this.y }
}
fun f(): Int {
  val p: Point = Point(1, 2)
  return p.sum()
}
fun main() {}
"#;
        let file = parse_file(src).expect("parse");
        let checked = check_file(&file).expect("check");
        assert!(checked.classes.iter().any(|c| c.is_struct && c.name == "Point"));
    }

    #[test]
    fn bounds_allow_method_on_type_param() {
        let src = r#"
package t
interface Named {
  fun name(): String
}
class User(val n: String) : Named {
  fun name(): String { return this.n }
}
fun greet<T : Named>(x: T): String {
  return x.name()
}
fun main() {
  val s: String = greet(User("hi"))
}
"#;
        let file = parse_file(src).expect("parse");
        check_file(&file).expect("bounded type param method call");
    }

    #[test]
    fn where_multi_bounds_and_reject_unsatisfied() {
        let src_ok = r#"
package t
interface Named { fun name(): String }
interface Id { fun id(): Int }
class Both(val n: String, val i: Int) : Named, Id {
  fun name(): String { return this.n }
  fun id(): Int { return this.i }
}
fun f<T>(x: T) where T : Named, T : Id {
  println(x.name())
}
fun main() { f(Both("a", 1)) }
"#;
        let file = parse_file(src_ok).expect("parse");
        check_file(&file).expect("multi bounds ok");

        let src_bad = r#"
package t
interface Named { fun name(): String }
interface Id { fun id(): Int }
class OnlyNamed(val n: String) : Named {
  fun name(): String { return this.n }
}
fun f<T>(x: T) where T : Named, T : Id {
  println(x.name())
}
fun main() { f(OnlyNamed("a")) }
"#;
        let file = parse_file(src_bad).expect("parse");
        let err = check_file(&file).expect_err("should reject missing Id bound");
        assert!(
            err.message.contains("Id") || err.message.contains("bound"),
            "unexpected: {}",
            err.message
        );
    }

    #[test]
    fn unbounded_type_param_cannot_call_methods() {
        let src = r#"
package t
interface Named { fun name(): String }
fun bad<T>(x: T): String {
  return x.name()
}
fun main() {}
"#;
        let file = parse_file(src).expect("parse");
        let err = check_file(&file).expect_err("unbounded T");
        assert!(
            err.message.contains("unbounded") || err.message.contains("method"),
            "unexpected: {}",
            err.message
        );
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
