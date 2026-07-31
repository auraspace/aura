//! Name resolution + typecheck for Aura C0–C3b (enums, match, Result).

mod attributes;
mod checker;
mod derive;
mod error;
mod macros;
mod sigs;
mod ty;
mod util;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use error::{SemaError, SemaErrors};
pub use macros::{MacroError, MacroExpansion, UserDerive, UserMacro};
pub use sigs::*;
pub use ty::{nominal_key, nominal_mono_base, split_nominal, Ty};
pub use util::{subst_ty, type_subst_map};

use aura_ast::File;
use checker::Checker;

/// Typecheck a parsed file.
///
/// C6h/C7g: body- and declaration-level errors are collected so multiple
/// issues can be reported in one `aura check` run when processing can continue.
pub fn check_file(file: &File) -> Result<CheckedFile, SemaErrors> {
    check_file_with_macros(file, &[], &[])
}

/// Typecheck a file after running registered user derives in the derive phase.
/// Built-in derives and user derives share the same collision, ownership, and
/// metadata checks after expansion.
pub fn check_file_with_derives(
    file: &File,
    derives: &[&dyn UserDerive],
) -> Result<CheckedFile, SemaErrors> {
    check_file_with_macros(file, &[], derives)
}

/// Typecheck a file after deterministic AST macro expansion and user derives.
/// Macro callbacks run before built-in/user derive callbacks and before name
/// resolution, matching the expansion order documented by RFC-004/RFC-010.
pub fn check_file_with_macros(
    file: &File,
    macros: &[&dyn UserMacro],
    derives: &[&dyn UserDerive],
) -> Result<CheckedFile, SemaErrors> {
    let mut expanded = file.clone();
    let mut c = Checker::new();
    let mut macro_expansions = Vec::new();
    for macro_impl in macros {
        match macro_impl.expand(&mut expanded) {
            Ok(expansions) => macro_expansions.extend(expansions),
            Err(error) => c.errors.push(SemaError {
                message: format!(
                    "[AURA-MACRO-EXPAND] [phase=macro] `{}`: {}",
                    macro_impl.name(),
                    error.message
                ),
                span: error.span,
            }),
        }
    }
    c.errors.extend(derive::expand_equals(&mut expanded));
    c.errors.extend(derive::expand_hash(&mut expanded));
    c.errors.extend(derive::expand_debug(&mut expanded));
    c.errors.extend(expand_user_derives(&mut expanded, derives));
    c.errors.extend(
        attributes::validate_file(&expanded)
            .into_iter()
            .filter(|error| !is_registered_derive_error(error, derives)),
    );
    match c.check_file(&expanded) {
        Ok(mut checked) => {
            checked.expansions = macro_expansions
                .into_iter()
                .map(|item| ExpansionMetadata {
                    phase: "macro".into(),
                    macro_name: item.macro_name,
                    generated_item: item.generated_item,
                    invocation_span: item.invocation_span,
                    generated_span: item.generated_span,
                })
                .chain(expansion_metadata(file, &expanded))
                .collect();
            checked
                .expansions
                .sort_by_key(|item| (item.invocation_span.start, item.generated_item.clone()));
            if c.errors.is_empty() {
                Ok(checked)
            } else {
                Err(SemaErrors::new(std::mem::take(&mut c.errors)))
            }
        }
        Err(e) => {
            let mut errors = std::mem::take(&mut c.errors);
            errors.insert(0, e);
            Err(SemaErrors::new(errors))
        }
    }
}

fn is_registered_derive_error(error: &SemaError, derives: &[&dyn UserDerive]) -> bool {
    derives.iter().any(|derive| {
        error.message.contains("derive `")
            && error
                .message
                .contains(&format!("derive `{}`", derive.name()))
    })
}

fn expand_user_derives(file: &mut File, derives: &[&dyn UserDerive]) -> Vec<SemaError> {
    let mut errors = Vec::new();
    for class_index in 0..file.classes.len() {
        let source = file.classes[class_index].clone();
        let derive_names = source
            .attributes
            .iter()
            .filter(|attribute| attribute.name.name == "derive")
            .flat_map(|attribute| attribute.args.iter())
            .filter_map(|arg| match arg {
                aura_ast::AttributeArg::Positional(aura_ast::AttributeValue::Ident(name)) => {
                    Some(name.name.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for name in derive_names {
            let Some(derive) = derives.iter().find(|derive| derive.name() == name) else {
                continue;
            };
            let invocation_span = source
                .attributes
                .iter()
                .find(|attribute| {
                    attribute.name.name == "derive"
                        && attribute.args.iter().any(|arg| {
                            matches!(
                                arg,
                                aura_ast::AttributeArg::Positional(
                                    aura_ast::AttributeValue::Ident(ident)
                                ) if ident.name == name
                            )
                        })
                })
                .map(|attribute| attribute.span)
                .unwrap_or(source.span);
            let generated = match derive.expand(&source) {
                Ok(generated) => generated,
                Err(error) => {
                    errors.push(SemaError {
                        message: format!(
                            "[AURA-MACRO-EXPAND] [phase=derive] `{}`: {}",
                            derive.name(),
                            error.message
                        ),
                        span: error.span,
                    });
                    continue;
                }
            };
            for mut method in generated {
                if file.classes[class_index]
                    .methods
                    .iter()
                    .any(|existing| existing.name.name == method.name.name)
                {
                    errors.push(SemaError {
                        message: format!(
                            "[AURA-MACRO-DUPLICATE] [phase=derive] `{}` generated existing method `{}`",
                            derive.name(), method.name.name
                        ),
                        span: invocation_span,
                    });
                    continue;
                }
                method.origin_package = source.origin_package.clone();
                method.span = invocation_span;
                file.classes[class_index].methods.push(method);
            }
        }
    }
    errors
}

fn expansion_metadata(original: &File, expanded: &File) -> Vec<ExpansionMetadata> {
    let mut out = Vec::new();
    for class in &expanded.classes {
        let Some(source) = original
            .classes
            .iter()
            .find(|candidate| candidate.name.name == class.name.name)
        else {
            continue;
        };
        for attribute in &source.attributes {
            if attribute.name.name != "derive" {
                continue;
            }
            for arg in &attribute.args {
                let aura_ast::AttributeArg::Positional(aura_ast::AttributeValue::Ident(name)) = arg
                else {
                    continue;
                };
                let generated_name = match name.name.as_str() {
                    "Equals" | "Eq" => Some("equals"),
                    "Hash" | "HashCode" => Some("hashCode"),
                    "Debug" => Some("toString"),
                    "DebugString" => Some("debugString"),
                    _ => None,
                };
                let candidates = if let Some(generated_name) = generated_name {
                    class
                        .methods
                        .iter()
                        .filter(|method| method.name.name == generated_name)
                        .collect::<Vec<_>>()
                } else {
                    class
                        .methods
                        .iter()
                        .filter(|method| {
                            method.span == attribute.span
                                && !source
                                    .methods
                                    .iter()
                                    .any(|original| original.name.name == method.name.name)
                        })
                        .collect::<Vec<_>>()
                };
                for method in candidates {
                    if !source
                        .methods
                        .iter()
                        .any(|original| original.name.name == method.name.name)
                    {
                        out.push(ExpansionMetadata {
                            phase: "derive".into(),
                            macro_name: name.name.clone(),
                            generated_item: format!("{}.{}", class.name.name, method.name.name),
                            invocation_span: attribute.span,
                            generated_span: method.span,
                        });
                    }
                }
            }
        }
    }
    out.sort_by_key(|item| (item.invocation_span.start, item.generated_item.clone()));
    out
}
