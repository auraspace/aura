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
    Function(Vec<Type>, Box<Type>),
    Null,
    Union(Vec<Type>),
    Generic(String, Vec<Type>),
    Enum(String),
    Array(Box<Type>),
    Object(HashMap<String, Type>),
    Unknown,
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

            // Structural typing (Basic)
            (Type::Object(src_fields), Type::Object(tgt_fields)) => {
                tgt_fields.iter().all(|(name, tgt_ty)| {
                    src_fields
                        .get(name)
                        .map(|src_ty| src_ty.is_assignable_to(tgt_ty))
                        .unwrap_or(false)
                })
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
                    Type::Unknown
                } else {
                    Type::Union(filtered)
                }
            }
            _ => {
                if self == other {
                    Type::Unknown
                } else {
                    self.clone()
                }
            }
        }
    }
}
impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int32 => write!(f, "number"),
            Type::Int64 => write!(f, "i64"),
            Type::Float32 => write!(f, "f32"),
            Type::Float64 => write!(f, "f64"),
            Type::String => write!(f, "string"),
            Type::Boolean => write!(f, "boolean"),
            Type::Void => write!(f, "void"),
            Type::Class(name) => write!(f, "{}", name),
            Type::Enum(name) => write!(f, "enum {}", name),
            Type::Function(params, ret) => {
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
            Type::Unknown => write!(f, "unknown"),
        }
    }
}
