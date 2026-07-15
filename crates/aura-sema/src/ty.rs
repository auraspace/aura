//! Semantic types.

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

