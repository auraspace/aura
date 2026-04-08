use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use aura_ast::TypeRef;
use aura_diagnostics::Diagnostic;
use aura_span::Span;

use crate::lib_utils::ident_text;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Unknown,
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Void,
    Class(String),
    Interface(String),
    Function(Box<MethodSig>),
}

impl Ty {
    pub fn name(&self) -> Cow<'_, str> {
        match self {
            Ty::Unknown => Cow::Borrowed("<unknown>"),
            Ty::I32 => Cow::Borrowed("i32"),
            Ty::I64 => Cow::Borrowed("i64"),
            Ty::F32 => Cow::Borrowed("f32"),
            Ty::F64 => Cow::Borrowed("f64"),
            Ty::Bool => Cow::Borrowed("bool"),
            Ty::String => Cow::Borrowed("string"),
            Ty::Void => Cow::Borrowed("void"),
            Ty::Class(name) => Cow::Owned(name.clone()),
            Ty::Interface(name) => Cow::Owned(name.clone()),
            Ty::Function(sig) => {
                let params: Vec<_> = sig.params.iter().map(|p| p.name()).collect();
                Cow::Owned(format!(
                    "function({}) => {}",
                    params.join(", "),
                    sig.return_ty.name()
                ))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypePosition {
    Value,
    Return,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TyDefKind {
    Class,
    Interface,
}

#[derive(Clone, Debug)]
pub struct TypedProgram {
    pub classes: HashMap<String, ClassInfo>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub functions: HashMap<String, MethodSig>,
    pub expression_types: HashMap<Span, Ty>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassInfo {
    pub fields: HashMap<String, Ty>,
    pub methods: HashMap<String, MethodSig>,
    pub implements: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceInfo {
    pub methods: HashMap<String, MethodSig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodSig {
    pub params: Vec<Ty>,
    pub return_ty: Ty,
}

pub(crate) fn is_numeric(ty: &Ty) -> bool {
    matches!(ty, Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64)
}

pub(crate) fn unify_numeric(a: &Ty, b: &Ty) -> Ty {
    match (a, b) {
        (Ty::F64, _) | (_, Ty::F64) => Ty::F64,
        (Ty::F32, _) | (_, Ty::F32) => Ty::F32,
        (Ty::I64, _) | (_, Ty::I64) => Ty::I64,
        _ => Ty::I32,
    }
}

pub(crate) fn is_comparable(a: &Ty, b: &Ty) -> bool {
    a == b || (is_numeric(a) && is_numeric(b))
}

pub(crate) fn is_assignable(from: &Ty, to: &Ty, classes: &HashMap<String, ClassInfo>) -> bool {
    if *from == Ty::Unknown || *to == Ty::Unknown {
        return true;
    }
    if from == to {
        return true;
    }

    match (from, to) {
        (Ty::I32, Ty::I64) => true,
        (Ty::I32, Ty::F32) | (Ty::I32, Ty::F64) => true,
        (Ty::I64, Ty::F64) | (Ty::I64, Ty::F32) => true,
        (Ty::F32, Ty::F64) => true,
        (Ty::Class(cname), Ty::Interface(iname)) => {
            if let Some(cinfo) = classes.get(cname) {
                cinfo.implements.contains(iname)
            } else {
                false
            }
        }
        _ => false,
    }
}

pub(crate) fn ty_from_type_ref(
    source: &str,
    ty: &TypeRef,
    pos: TypePosition,
    type_defs: &HashMap<String, TyDefKind>,
    diags: &mut Vec<Diagnostic>,
) -> Ty {
    let Some(name) = ident_text(source, &ty.name) else {
        diags.push(Diagnostic::error(ty.span, "invalid type reference"));
        return Ty::Unknown;
    };

    if let Some(builtin) = parse_builtin_ty(name) {
        if builtin == Ty::Void && pos != TypePosition::Return {
            diags.push(
                Diagnostic::error(
                    ty.span,
                    "type `void` is only valid as a function return type",
                )
                .with_help("remove the annotation or use a non-void type"),
            );
            return Ty::Unknown;
        }
        return builtin;
    }

    if let Some(kind) = type_defs.get(name) {
        match kind {
            TyDefKind::Class => return Ty::Class(name.to_string()),
            TyDefKind::Interface => return Ty::Interface(name.to_string()),
        }
    }

    diags.push(
        Diagnostic::error(ty.span, format!("unknown type `{name}`"))
            .with_help("built-in types: i32, i64, f32, f64, bool, string, void"),
    );
    Ty::Unknown
}

pub(crate) fn parse_builtin_ty(name: &str) -> Option<Ty> {
    match name {
        "i32" => Some(Ty::I32),
        "i64" => Some(Ty::I64),
        "f32" => Some(Ty::F32),
        "f64" => Some(Ty::F64),
        "bool" => Some(Ty::Bool),
        "string" => Some(Ty::String),
        "void" => Some(Ty::Void),
        _ => None,
    }
}
