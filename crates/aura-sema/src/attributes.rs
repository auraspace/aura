use std::collections::HashSet;

use aura_ast::{Attribute, AttributeArg, AttributeValue, File, Span};

use crate::error::SemaError;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Type,
    Function,
    Method,
    Parameter,
    Field,
    EnumVariant,
    TypeAlias,
    Const,
}

impl Target {
    fn display(self) -> &'static str {
        match self {
            Self::Type => "type declarations",
            Self::Function => "top-level functions",
            Self::Method => "methods",
            Self::Parameter => "parameters",
            Self::Field => "fields",
            Self::EnumVariant => "enum variants",
            Self::TypeAlias => "type aliases",
            Self::Const => "constants",
        }
    }
}

#[derive(Clone, Copy)]
struct AttributeSpec {
    name: &'static str,
    targets: &'static [Target],
    retention: &'static str,
    repeatable: bool,
    conflicts: &'static [&'static str],
}

// M2 registry. Derive is registered as syntax/metadata only; expansion is M4–M6.
const TYPE: &[Target] = &[Target::Type];
const FUNCTION: &[Target] = &[Target::Function];
const PARAMETER: &[Target] = &[Target::Parameter];
const DECLARATION: &[Target] = &[
    Target::Type,
    Target::Function,
    Target::Method,
    Target::Parameter,
    Target::Field,
    Target::EnumVariant,
    Target::TypeAlias,
    Target::Const,
];
const NO_CONFLICTS: &[&str] = &[];
const INLINE_CONFLICTS: &[&str] = &["noinline"];
const NOINLINE_CONFLICTS: &[&str] = &["inline"];

const REGISTRY: &[AttributeSpec] = &[
    AttributeSpec {
        name: "test",
        targets: FUNCTION,
        retention: "Source",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "bench",
        targets: FUNCTION,
        retention: "Source",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "derive",
        targets: TYPE,
        retention: "Source",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "deprecated",
        targets: DECLARATION,
        retention: "Binary",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "inline",
        targets: &[Target::Function, Target::Method],
        retention: "Source",
        repeatable: false,
        conflicts: INLINE_CONFLICTS,
    },
    AttributeSpec {
        name: "noinline",
        targets: &[Target::Function, Target::Method],
        retention: "Source",
        repeatable: false,
        conflicts: NOINLINE_CONFLICTS,
    },
    AttributeSpec {
        name: "cold",
        targets: &[Target::Function, Target::Method],
        retention: "Source",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "throws",
        targets: &[Target::Function, Target::Method],
        retention: "Binary",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "unsafe",
        targets: &[Target::Type, Target::Function, Target::Method],
        retention: "Binary",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "repr",
        targets: TYPE,
        retention: "Binary",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "reflect",
        targets: TYPE,
        retention: "Runtime",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
    AttributeSpec {
        name: "notNull",
        targets: PARAMETER,
        retention: "Source",
        repeatable: false,
        conflicts: NO_CONFLICTS,
    },
];

fn spec(name: &str) -> Option<&'static AttributeSpec> {
    REGISTRY.iter().find(|spec| spec.name == name)
}

fn error(code: &'static str, message: String, span: Span) -> SemaError {
    SemaError {
        message: format!("[{code}] {message}"),
        span,
    }
}

fn validate_attributes(attributes: &[Attribute], target: Target, errors: &mut Vec<SemaError>) {
    let mut seen = HashSet::new();
    for attribute in attributes {
        let name = attribute.name.name.as_str();
        let Some(spec) = spec(name) else {
            errors.push(error(
                "AURA-M2-UNKNOWN",
                format!("unknown attribute `@{name}`"),
                attribute.span,
            ));
            continue;
        };
        debug_assert!(!spec.retention.is_empty());
        if !spec.targets.contains(&target) {
            errors.push(error(
                "AURA-M2-TARGET",
                format!(
                    "attribute `@{name}` is not allowed on {}; expected one of {}",
                    target.display(),
                    spec.targets
                        .iter()
                        .map(|target| target.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                attribute.span,
            ));
        }
        if !spec.repeatable && !seen.insert(name) {
            errors.push(error(
                "AURA-M2-DUPLICATE",
                format!("attribute `@{name}` may appear at most once on a declaration"),
                attribute.span,
            ));
        }
        for prior in seen.iter() {
            if spec.conflicts.contains(prior) {
                errors.push(error(
                    "AURA-M2-CONFLICT",
                    format!("attribute `@{name}` conflicts with `@{prior}`"),
                    attribute.span,
                ));
            }
        }
        validate_arguments(attribute, spec, errors);
    }
}

fn validate_arguments(attribute: &Attribute, spec: &AttributeSpec, errors: &mut Vec<SemaError>) {
    let invalid = |message: String, span: Span, errors: &mut Vec<SemaError>| {
        errors.push(error("AURA-M2-ARGS", message, span));
    };
    match spec.name {
        "test" => {
            for arg in &attribute.args {
                match arg {
                    AttributeArg::Positional(AttributeValue::Ident(value))
                        if value.name == "ignore" => {}
                    AttributeArg::Named { name, value, .. }
                        if (name.name == "ignore" && is_bool(value))
                            || (name.name == "tag" && is_string(value)) => {}
                    _ => invalid(
                        "`@test` accepts `ignore`, `ignore = true|false`, or `tag = \"...\"`"
                            .into(),
                        arg.span(),
                        errors,
                    ),
                }
            }
        }
        "derive" => {
            if attribute.args.is_empty()
                || attribute
                    .args
                    .iter()
                    .any(|arg| !matches!(arg, AttributeArg::Positional(AttributeValue::Ident(_))))
            {
                invalid(
                    "`@derive` requires one or more positional trait names".into(),
                    attribute.span,
                    errors,
                );
            }
        }
        "deprecated" => {
            if attribute.args.len() > 1
                || attribute
                    .args
                    .iter()
                    .any(|arg| !is_valid_deprecated_arg(arg))
            {
                invalid(
                    "`@deprecated` accepts one message string or `since = \"...\"`".into(),
                    attribute.span,
                    errors,
                );
            }
        }
        "repr" => {
            if attribute.args.len() != 1
                || !matches!(
                    attribute.args.first(),
                    Some(AttributeArg::Positional(AttributeValue::Ident(_)))
                )
            {
                invalid(
                    "`@repr` requires one positional representation name".into(),
                    attribute.span,
                    errors,
                );
            }
        }
        "bench"
        | "inline"
        | "noinline"
        | "cold"
        | "throws"
        | "unsafe"
        | "reflect"
        | "notNull" if !attribute.args.is_empty() => invalid(
            format!("`@{}` does not accept arguments", spec.name),
            attribute.span,
            errors,
        ),
        _ => {}
    }
}

fn is_bool(value: &AttributeValue) -> bool {
    matches!(value, AttributeValue::Bool { .. })
}

fn is_string(value: &AttributeValue) -> bool {
    matches!(value, AttributeValue::String { .. })
}

fn is_valid_deprecated_arg(arg: &AttributeArg) -> bool {
    match arg {
        AttributeArg::Positional(AttributeValue::String { .. }) => true,
        AttributeArg::Named { name, value, .. } => name.name == "since" && is_string(value),
        _ => false,
    }
}

pub(crate) fn validate_file(file: &File) -> Vec<SemaError> {
    let mut errors = Vec::new();
    for interface in &file.interfaces {
        validate_attributes(&interface.attributes, Target::Type, &mut errors);
        for method in &interface.methods {
            validate_attributes(&method.attributes, Target::Method, &mut errors);
            for parameter in &method.params {
                validate_attributes(&parameter.attributes, Target::Parameter, &mut errors);
            }
        }
    }
    for enumeration in &file.enums {
        validate_attributes(&enumeration.attributes, Target::Type, &mut errors);
        for variant in &enumeration.variants {
            validate_attributes(&variant.attributes, Target::EnumVariant, &mut errors);
            for field in &variant.fields {
                validate_attributes(&field.attributes, Target::Parameter, &mut errors);
            }
        }
    }
    for class in &file.classes {
        validate_attributes(&class.attributes, Target::Type, &mut errors);
        for field in &class.fields {
            validate_attributes(&field.attributes, Target::Field, &mut errors);
        }
        for method in &class.methods {
            validate_attributes(&method.attributes, Target::Method, &mut errors);
            for parameter in &method.params {
                validate_attributes(&parameter.attributes, Target::Parameter, &mut errors);
            }
        }
    }
    for alias in &file.type_aliases {
        validate_attributes(&alias.attributes, Target::TypeAlias, &mut errors);
    }
    for constant in &file.consts {
        validate_attributes(&constant.attributes, Target::Const, &mut errors);
    }
    for function in &file.functions {
        validate_attributes(&function.attributes, Target::Function, &mut errors);
        for parameter in &function.params {
            validate_attributes(&parameter.attributes, Target::Parameter, &mut errors);
        }
    }
    for function in &file.async_functions {
        validate_attributes(&function.attributes, Target::Function, &mut errors);
        for parameter in &function.params {
            validate_attributes(&parameter.attributes, Target::Parameter, &mut errors);
        }
    }
    errors
}
