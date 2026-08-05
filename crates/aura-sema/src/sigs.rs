//! Checked signatures and monomorphization metadata.

use std::collections::HashMap;

use aura_ast::{File, MemberVisibility, Span};

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
    /// Number of leading parameters that cannot be omitted.
    pub required_params: usize,
    pub is_vararg: bool,
    pub ret: Ty,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassMethodSig {
    pub class: String,
    pub name: String,
    pub params: Vec<Ty>,
    pub required_params: usize,
    pub is_vararg: bool,
    pub ret: Ty,
    pub type_params: Vec<String>,
    pub bounds: HashMap<String, Vec<String>>,
    pub is_static: bool,
    pub is_open: bool,
    pub is_abstract: bool,
    pub is_override: bool,
    pub visibility: MemberVisibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct IfaceMethodSig {
    pub name: String,
    pub params: Vec<Ty>,
    pub required_params: usize,
    pub is_vararg: bool,
    pub ret: Ty,
    pub has_default: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FieldSig {
    pub name: String,
    pub ty: Ty,
    pub mutable: bool,
    pub visibility: MemberVisibility,
}

#[derive(Debug, Clone)]
pub struct ConstructorSig {
    pub params: Vec<Ty>,
    pub required_params: usize,
    pub is_vararg: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ClassSig {
    pub name: String,
    pub is_pub: bool,
    pub package: String,
    /// `false` = class, `true` = struct (value type; no implements).
    pub is_struct: bool,
    pub is_open: bool,
    pub is_abstract: bool,
    pub type_params: Vec<String>,
    /// Bounds per type param name (interface names in C2e).
    pub bounds: HashMap<String, Vec<String>>,
    /// Direct superclass, when this class participates in inheritance.
    pub superclass: Option<Ty>,
    /// Implemented interfaces as `Ty::Interface` or `Ty::InterfaceApp` (C8c).
    pub implements: Vec<Ty>,
    pub fields: Vec<FieldSig>,
    /// Secondary constructors; index 0 is the implicit primary constructor.
    pub constructors: Vec<ConstructorSig>,
    /// All declarations for each method name; `methods` retains the first
    /// declaration for legacy single-method consumers.
    pub method_overloads: HashMap<String, Vec<ClassMethodSig>>,
    pub methods: HashMap<String, ClassMethodSig>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct InterfaceSig {
    pub name: String,
    pub is_pub: bool,
    pub package: String,
    /// C7i/C8c: declared type params; implements may monomorphize.
    pub type_params: Vec<String>,
    pub parents: Vec<Ty>,
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
    /// Declaring package for free-function calls (C3o mangling); empty for builtins/ctors.
    pub package: String,
    pub type_args: Vec<Ty>,
    /// Generic type arguments declared by a class method.
    pub method_type_args: Vec<Ty>,
    pub is_static: bool,
    pub constructor_index: Option<usize>,
    pub declaration_span: Option<Span>,
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
    /// Concrete generic async-function instantiations used.
    pub mono_async_funs: Vec<(String, Vec<Ty>)>,
    /// Concrete generic class-method instantiations used in this file.
    pub mono_methods: Vec<(String, Vec<Ty>, String, Vec<Ty>)>,
    /// Concrete generic interface instantiations used (C8c).
    pub mono_interfaces: Vec<(String, Vec<Ty>)>,
    /// CallExpr.span.start → resolved type arguments (for codegen).
    pub call_instantiations: HashMap<u32, CallInstantiation>,
    /// C10d/e: LambdaExpr.span.start → function type (for codegen).
    pub lambda_tys: HashMap<u32, Ty>,
    /// Expression span start -> resolved expression type for editor queries.
    pub expr_tys: HashMap<(u32, u32), Ty>,
    /// C10h/C12m: LambdaExpr.span.start → outer captures in stable name order.
    pub lambda_captures: HashMap<u32, Vec<LambdaCapture>>,
    /// Attribute metadata retained by the compiler boundary.
    pub attribute_metadata: Vec<AttributeMetadata>,
    /// Synthetic items and their expansion origins.
    pub expansions: Vec<ExpansionMetadata>,
    pub ast: File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeMetadata {
    pub declaration: String,
    pub target: String,
    pub name: String,
    pub retention: MetadataRetention,
    pub args: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataRetention {
    Source,
    Binary,
    Runtime,
}

impl MetadataRetention {
    pub fn abi_code(self) -> u32 {
        match self {
            Self::Source => 0,
            Self::Binary => 1,
            Self::Runtime => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionMetadata {
    pub phase: String,
    pub macro_name: String,
    pub generated_item: String,
    pub invocation_span: Span,
    pub generated_span: Span,
}

/// One free-variable capture of a lambda (C10h/C12m).
#[derive(Debug, Clone)]
pub struct LambdaCapture {
    pub name: String,
    pub ty: Ty,
    /// `true`: shared mutable storage (`var` primitive box, class/Array/Fun
    /// reference). `false`: copy-out (`val`).
    pub by_ref: bool,
}

impl CheckedFile {
    /// Names of locals that are by-ref captured by any lambda (need heap boxes in codegen).
    pub fn by_ref_capture_names(&self) -> std::collections::HashSet<String> {
        let mut s = std::collections::HashSet::new();
        for caps in self.lambda_captures.values() {
            for c in caps {
                if c.by_ref {
                    s.insert(c.name.clone());
                }
            }
        }
        s
    }
}
