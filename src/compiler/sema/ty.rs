use crate::compiler::ast::TypeParam;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int32,
    Int64,
    Float32,
    Float64,
    String,
    Boolean,
    Void,
    Class(String),
    GenericParam(String), // Added GenericParam(String) here
    Function(Vec<TypeParam>, Vec<Type>, Box<Type>),
    Null,
    Union(Vec<Type>),
    Generic(String, Vec<Type>),
    Enum(String),
    Array(Box<Type>),
    Object(HashMap<String, Type>),
    ClassType(String),
    Error,
}

impl Type {
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64
        )
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Type::Int32 | Type::Int64)
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Type::Float32 | Type::Float64)
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self, Type::Boolean)
    }

    pub fn is_class(&self) -> bool {
        matches!(self, Type::Class(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Type::Array(_))
    }

    pub fn is_assignable_to(&self, other: &Type) -> bool {
        if self == other {
            return true;
        }

        match (self, other) {
            // Union types: A is assignable to B | C if A is assignable to B OR A is assignable to C
            (src, Type::Union(options)) => options.iter().any(|opt| src.is_assignable_to(opt)),

            // Union sources: B | C is assignable to A if BOTH B and C are assignable to A
            (Type::Union(options), target) => {
                options.iter().all(|opt| opt.is_assignable_to(target))
            }

            // Simple numeric promotion for now
            (Type::Int32, Type::Int64) => true,
            (Type::Int32 | Type::Int64, Type::Float32 | Type::Float64) => true,
            (Type::Float32, Type::Float64) => true,

            (Type::Object(src_fields), Type::Object(tgt_fields)) => {
                tgt_fields.iter().all(|(name, tgt_ty)| {
                    src_fields
                        .get(name)
                        .map(|src_ty| src_ty.is_assignable_to(tgt_ty))
                        .unwrap_or(false)
                })
            }

            (
                Type::Function(src_tparams, src_params, src_ret),
                Type::Function(tgt_tparams, tgt_params, tgt_ret),
            ) => {
                // For now, strict equality for type params and parameter types
                // We should eventually implement proper variance and generic substitution check
                src_tparams == tgt_tparams
                    && src_params.len() == tgt_params.len()
                    && src_params
                        .iter()
                        .zip(tgt_params.iter())
                        .all(|(s, t)| s.is_assignable_to(t))
                    && src_ret.is_assignable_to(tgt_ret)
            }

            // TODO: Handle Class vs Object (if Class is nominal or structural)
            // If Aura is structural, a Class(name) should be resolved to its Object structure.
            _ => false,
        }
    }

    pub fn exclude(&self, other: &Type) -> Type {
        match self {
            Type::Union(options) => {
                let filtered: Vec<Type> = options
                    .iter()
                    .filter(|&opt| opt != other)
                    .cloned()
                    .collect();
                if filtered.len() == 1 {
                    filtered[0].clone()
                } else if filtered.is_empty() {
                    Type::Error
                } else {
                    Type::Union(filtered)
                }
            }
            _ => {
                if self == other {
                    Type::Error
                } else {
                    self.clone()
                }
            }
        }
    }

    pub fn simplify(self) -> Type {
        match self {
            Type::Union(options) => {
                if options.is_empty() {
                    return Type::Void;
                }
                let mut unique_options: Vec<Type> = Vec::new();
                for opt in options {
                    if !unique_options.contains(&opt) {
                        unique_options.push(opt);
                    }
                }
                if unique_options.len() == 1 {
                    unique_options.pop().unwrap()
                } else {
                    Type::Union(unique_options)
                }
            }
            _ => self,
        }
    }

    pub fn tag(&self) -> i64 {
        match self {
            Type::Int32 | Type::Int64 => 1,
            Type::Float32 | Type::Float64 => 2,
            Type::Boolean => 3,
            Type::String => 4,
            Type::Class(_) => 5,
            Type::Null => 6,
            Type::Enum(_) => 7,
            _ => 0,
        }
    }
}
impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int32 => write!(f, "number"),
            Type::Int64 => write!(f, "i64"),
            Type::Float32 => write!(f, "f32"),
            Type::Float64 => write!(f, "float"),
            Type::String => write!(f, "string"),
            Type::Boolean => write!(f, "boolean"),
            Type::Void => write!(f, "void"),
            Type::Class(name) => write!(f, "{}", name),
            Type::Enum(name) => write!(f, "enum {}", name),
            Type::GenericParam(name) => write!(f, "{}", name),
            Type::Function(tparams, params, ret) => {
                if !tparams.is_empty() {
                    write!(f, "<")?;
                    for (i, tp) in tparams.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", tp.name)?;
                    }
                    write!(f, ">")?;
                }
                write!(f, "function(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, "): {}", ret)
            }
            Type::Null => write!(f, "null"),
            Type::Union(options) => {
                for (i, opt) in options.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{}", opt)?;
                }
                Ok(())
            }
            Type::Generic(name, args) => {
                write!(f, "{}<", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ">")
            }
            Type::Array(inner) => write!(f, "{}[]", inner),
            Type::Object(fields) => {
                write!(f, "{{ ")?;
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", name, ty)?;
                }
                write!(f, " }}")
            }
            Type::ClassType(name) => write!(f, "Class<{}>", name),
            Type::Error => write!(f, "unknown"),
        }
    }
}
