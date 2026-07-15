//! Semantic types.

/// C3v: user nominal types encode package as `Name@pkg.path` (empty package → bare `Name`).
pub fn nominal_key(pkg: &str, name: &str) -> String {
    if pkg.is_empty() {
        name.to_string()
    } else {
        format!("{name}@{pkg}")
    }
}

/// Split `Name@pkg` → (`Name`, `pkg`); bare name → (`name`, `""`).
pub fn split_nominal(key: &str) -> (&str, &str) {
    match key.split_once('@') {
        Some((n, p)) => (n, p),
        None => (key, ""),
    }
}

/// C-safe mono base from a nominal key (`Point@demo.math` → `demo_math_Point`).
pub fn nominal_mono_base(key: &str) -> String {
    let (name, pkg) = split_nominal(key);
    if pkg.is_empty() {
        name.to_string()
    } else {
        let mangled: String = pkg
            .chars()
            .map(|c| if c == '.' || c == '-' { '_' } else { c })
            .collect();
        format!("{mangled}_{name}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Unit,
    Int,
    Bool,
    String,
    Null,
    Nullable(Box<Ty>),
    /// Non-generic class; name may be `Name@package` (C3v).
    Class(String),
    /// Instantiated generic class; `name` may be package-qualified key.
    ClassApp { name: String, args: Vec<Ty> },
    /// Non-generic enum; name may be package-qualified key.
    Enum(String),
    /// Instantiated generic enum.
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
            Ty::Class(n) | Ty::Enum(n) => {
                let (name, pkg) = split_nominal(n);
                if pkg.is_empty() {
                    name.to_string()
                } else {
                    format!("{pkg}.{name}")
                }
            }
            Ty::ClassApp { name, args } | Ty::EnumApp { name, args } => {
                let (simple, pkg) = split_nominal(name);
                let a = args
                    .iter()
                    .map(|t| t.display())
                    .collect::<Vec<_>>()
                    .join(", ");
                if pkg.is_empty() {
                    format!("{simple}<{a}>")
                } else {
                    format!("{pkg}.{simple}<{a}>")
                }
            }
            Ty::Interface(n) => {
                let (name, pkg) = split_nominal(n);
                if pkg.is_empty() {
                    name.to_string()
                } else {
                    format!("{pkg}.{name}")
                }
            }
            Ty::TypeParam(n) => n.clone(),
        }
    }

    /// Mangle for C monomorph: `Box_String`, `demo_math_Point`, `Result_Int_String`.
    pub fn mono_suffix(&self) -> String {
        match self {
            Ty::Unit => "Unit".into(),
            Ty::Int => "Int".into(),
            Ty::Bool => "Bool".into(),
            Ty::String => "String".into(),
            Ty::Null => "Null".into(),
            Ty::Nullable(inner) => format!("Opt_{}", inner.mono_suffix()),
            Ty::Class(n) | Ty::Enum(n) => nominal_mono_base(n),
            Ty::ClassApp { name, args } | Ty::EnumApp { name, args } => {
                let base = nominal_mono_base(name);
                let a = args
                    .iter()
                    .map(|t| t.mono_suffix())
                    .collect::<Vec<_>>()
                    .join("_");
                format!("{base}_{a}")
            }
            Ty::Interface(n) => nominal_mono_base(n),
            Ty::TypeParam(n) => n.clone(),
        }
    }

    pub fn class_name(&self) -> Option<&str> {
        match self {
            Ty::Class(n) | Ty::ClassApp { name: n, .. } => Some(split_nominal(n).0),
            _ => None,
        }
    }

    /// Package of a class/enum nominal key (`""` if none).
    pub fn nominal_package(&self) -> &str {
        match self {
            Ty::Class(n) | Ty::Enum(n) | Ty::Interface(n) => split_nominal(n).1,
            Ty::ClassApp { name, .. } | Ty::EnumApp { name, .. } => split_nominal(name).1,
            _ => "",
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
            Ty::Enum(n) | Ty::EnumApp { name: n, .. } => Some(split_nominal(n).0),
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

