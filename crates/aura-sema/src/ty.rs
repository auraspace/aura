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
    ClassApp {
        name: String,
        args: Vec<Ty>,
    },
    /// Non-generic enum; name may be package-qualified key.
    Enum(String),
    /// Instantiated generic enum.
    EnumApp {
        name: String,
        args: Vec<Ty>,
    },
    Interface(String),
    /// Instantiated generic interface (`Iterable<Int>`).
    InterfaceApp {
        name: String,
        args: Vec<Ty>,
    },
    /// Type parameter in a generic definition scope (`T`).
    TypeParam(String),
    /// C10d: function type `(params) -> ret` (first-class / lambdas).
    Fun {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
    /// C22: result of invoking an async function; it must be awaited.
    Task(Box<Ty>),
    /// C22: owned handle returned by spawning an async task body.
    TaskHandle(Box<Ty>),
    /// C22: bounded FIFO channel carrying owned values.
    Channel(Box<Ty>),
    /// FFI-001/FFI-002: opaque foreign resource handle.  The type argument is
    /// a compile-time tag only; the resource is never dereferenced by Aura.
    ForeignHandle(Box<Ty>),
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
            Ty::InterfaceApp { name, args } => {
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
            Ty::TypeParam(n) => n.clone(),
            Ty::Fun { params, ret } => {
                let ps = params
                    .iter()
                    .map(|p| p.display())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({ps}) -> {}", ret.display())
            }
            Ty::Task(inner) => format!("Task<{}>", inner.display()),
            Ty::TaskHandle(inner) => format!("TaskHandle<{}>", inner.display()),
            Ty::Channel(inner) => format!("Channel<{}>", inner.display()),
            Ty::ForeignHandle(inner) => format!("ForeignHandle<{}>", inner.display()),
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
            Ty::InterfaceApp { name, args } => {
                let base = nominal_mono_base(name);
                let a = args
                    .iter()
                    .map(|t| t.mono_suffix())
                    .collect::<Vec<_>>()
                    .join("_");
                format!("{base}_{a}")
            }
            Ty::TypeParam(n) => n.clone(),
            // C10d: `Fun_Int_Bool__String` for `(Int, Bool) -> String`
            Ty::Fun { params, ret } => {
                let ps = params
                    .iter()
                    .map(|p| p.mono_suffix())
                    .collect::<Vec<_>>()
                    .join("_");
                if ps.is_empty() {
                    format!("Fun__{}", ret.mono_suffix())
                } else {
                    format!("Fun_{ps}__{}", ret.mono_suffix())
                }
            }
            Ty::Task(inner) => format!("Task_{}", inner.mono_suffix()),
            Ty::TaskHandle(inner) => format!("TaskHandle_{}", inner.mono_suffix()),
            Ty::Channel(inner) => format!("Channel_{}", inner.mono_suffix()),
            Ty::ForeignHandle(inner) => format!("ForeignHandle_{}", inner.mono_suffix()),
        }
    }

    pub fn class_name(&self) -> Option<&str> {
        match self {
            Ty::Class(n) | Ty::ClassApp { name: n, .. } => Some(split_nominal(n).0),
            _ => None,
        }
    }

    /// True if this type still mentions an unbound type parameter (open mono).
    pub fn is_open(&self) -> bool {
        match self {
            Ty::TypeParam(_) => true,
            Ty::Nullable(inner) => inner.is_open(),
            Ty::ClassApp { args, .. }
            | Ty::EnumApp { args, .. }
            | Ty::InterfaceApp { args, .. } => args.iter().any(|a| a.is_open()),
            Ty::Fun { params, ret } => params.iter().any(|p| p.is_open()) || ret.is_open(),
            Ty::Task(inner)
            | Ty::TaskHandle(inner)
            | Ty::Channel(inner)
            | Ty::ForeignHandle(inner) => inner.is_open(),
            _ => false,
        }
    }

    /// Package of a class/enum nominal key (`""` if none).
    pub fn nominal_package(&self) -> &str {
        match self {
            Ty::Class(n) | Ty::Enum(n) | Ty::Interface(n) => split_nominal(n).1,
            Ty::ClassApp { name, .. }
            | Ty::EnumApp { name, .. }
            | Ty::InterfaceApp { name, .. } => split_nominal(name).1,
            _ => "",
        }
    }

    pub fn iface_name(&self) -> Option<&str> {
        match self {
            Ty::Interface(n) | Ty::InterfaceApp { name: n, .. } => Some(split_nominal(n).0),
            _ => None,
        }
    }

    pub fn iface_key(&self) -> Option<&str> {
        match self {
            Ty::Interface(n) | Ty::InterfaceApp { name: n, .. } => Some(n.as_str()),
            _ => None,
        }
    }

    pub fn iface_args(&self) -> &[Ty] {
        match self {
            Ty::InterfaceApp { args, .. } => args,
            _ => &[],
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
