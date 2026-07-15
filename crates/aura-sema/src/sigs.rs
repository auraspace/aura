//! Checked signatures and monomorphization metadata.

use std::collections::HashMap;

use aura_ast::{File, Span};

use crate::ty::Ty;

#[derive(Debug, Clone)]
pub struct FunSig {
    pub name: String,
    pub is_pub: bool,
    /// Declaring package (builtins use empty package and are always visible).
    pub package: String,
    pub is_test: bool,
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
    pub is_pub: bool,
    pub package: String,
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
    pub is_pub: bool,
    pub package: String,
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
    pub is_pub: bool,
    pub package: String,
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
