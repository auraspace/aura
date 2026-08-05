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
pub use macros::{
    decode_plugin_request, encode_plugin_request, encode_plugin_response, run_sandboxed_macro,
    MacroError, MacroExpansion, MacroPluginRequest, MacroPluginResponse, MacroSandboxConfig,
    UserDerive, UserMacro, MACRO_PLUGIN_ABI_VERSION,
};
pub use sigs::*;
pub use ty::{nominal_key, nominal_mono_base, split_nominal, Ty};
pub use util::{subst_ty, type_subst_map};

use aura_ast::{File, FunDecl, MemberVisibility, MethodSig};
use checker::Checker;
use std::collections::{HashMap, HashSet};

/// Expand one out-of-process macro response into the checked file.
///
/// The plugin returns a complete Aura source fragment so parsing, package
/// identity, and normal semantic validation remain compiler-owned. This is
/// the package/compiler integration point for the stable RFC-010 process ABI;
/// callers do not need to merge generated AST nodes themselves.
pub fn check_file_with_sandboxed_macro(
    file: &File,
    config: &MacroSandboxConfig,
    request: &MacroPluginRequest,
    derives: &[&dyn UserDerive],
) -> Result<CheckedFile, SemaErrors> {
    let response = run_sandboxed_macro(config, request).map_err(|error| {
        SemaErrors::single(SemaError {
            message: format!("[AURA-MACRO-PLUGIN] [phase=macro]: {}", error.message),
            span: error.span,
        })
    })?;
    let MacroPluginResponse::Expanded { source } = response else {
        let MacroPluginResponse::Failed { message, span } = response else {
            unreachable!("macro response matched above");
        };
        return Err(SemaErrors::single(SemaError {
            message: format!("[AURA-MACRO-PLUGIN] [phase=macro]: {message}"),
            span: if span == aura_ast::Span::new(0, 0) {
                request.invocation_span
            } else {
                span
            },
        }));
    };
    check_file_with_plugin_source(
        file,
        &request.macro_name,
        &source,
        request.invocation_span,
        derives,
    )
}

/// Check a source fragment returned by a procedural macro plugin.
///
/// This lower-level entry point is also useful to compiler hosts that already
/// own the sandbox process but want Aura to retain package and expansion
/// invariants in one place.
pub fn check_file_with_plugin_source(
    file: &File,
    macro_name: &str,
    source: &str,
    invocation_span: aura_ast::Span,
    derives: &[&dyn UserDerive],
) -> Result<CheckedFile, SemaErrors> {
    let generated = aura_parser::parse_file(source).map_err(|error| {
        SemaErrors::single(SemaError {
            message: format!("[AURA-MACRO-PLUGIN] [phase=macro]: {error}"),
            span: invocation_span,
        })
    })?;
    if generated.package.display() != file.package.display() {
        return Err(SemaErrors::single(SemaError {
            message: format!(
                "[AURA-MACRO-PLUGIN] [phase=macro]: generated package `{}` does not match `{}`",
                generated.package.display(),
                file.package.display()
            ),
            span: invocation_span,
        }));
    }
    let mut expanded = file.clone();
    append_plugin_items(&mut expanded, generated);
    let mut checked = check_file_with_macros(&expanded, &[], derives)?;
    checked.expansions.push(ExpansionMetadata {
        phase: "macro".into(),
        macro_name: macro_name.into(),
        generated_item: source.into(),
        invocation_span,
        generated_span: invocation_span,
    });
    checked
        .expansions
        .sort_by_key(|item| (item.invocation_span.start, item.generated_item.clone()));
    Ok(checked)
}

fn append_plugin_items(into: &mut File, generated: File) {
    into.imports.extend(generated.imports);
    into.interfaces.extend(generated.interfaces);
    into.enums.extend(generated.enums);
    into.classes.extend(generated.classes);
    into.type_aliases.extend(generated.type_aliases);
    into.consts.extend(generated.consts);
    into.functions.extend(generated.functions);
    into.foreign_functions.extend(generated.foreign_functions);
    into.async_functions.extend(generated.async_functions);
}

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
    c.errors.extend(expand_interface_defaults(&mut expanded));
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

/// Materialize interface default bodies as ordinary methods on each concrete
/// implementor. The existing class/vtable backend can then dispatch them using
/// the same ownership and generic lowering path as explicit overrides.
fn expand_interface_defaults(file: &mut File) -> Vec<SemaError> {
    let mut errors = Vec::new();
    for class_index in 0..file.classes.len() {
        if class_index >= file.classes.len()
            || file.classes[class_index].kind == aura_ast::NominalKind::Struct
        {
            continue;
        }
        let class = file.classes[class_index].clone();
        let mut defaults: HashMap<String, (MethodSig, String)> = HashMap::new();
        for implemented in &class.implements {
            let mut interface_defaults = HashMap::new();
            let mut seen = HashSet::new();
            collect_interface_defaults(
                file,
                &implemented.name.name,
                &mut seen,
                &mut interface_defaults,
            );
            for (name, method) in interface_defaults {
                if let Some((_, existing_interface)) = defaults.get(&name) {
                    errors.push(SemaError {
                        message: format!(
                            "conflicting default method `{}` from interfaces `{}` and `{}`; override it in the class",
                            name, existing_interface, method.1
                        ),
                        span: method.0.name.span,
                    });
                } else {
                    defaults.insert(name, method);
                }
            }
        }
        for (name, (method, _interface)) in defaults {
            if class
                .methods
                .iter()
                .any(|existing| existing.name.name == name)
            {
                continue;
            }
            let Some(body) = method.body else {
                continue;
            };
            file.classes[class_index].methods.push(FunDecl {
                is_pub: false,
                origin_package: class.origin_package.clone(),
                attributes: method.attributes,
                modifiers: Vec::new(),
                visibility: MemberVisibility::Public,
                is_test: false,
                name: method.name,
                type_params: Vec::new(),
                params: method.params,
                return_type: method.return_type,
                body,
                span: method.span,
            });
        }
    }
    errors
}

fn collect_interface_defaults(
    file: &File,
    name: &str,
    seen: &mut HashSet<String>,
    defaults: &mut HashMap<String, (MethodSig, String)>,
) {
    if !seen.insert(name.to_string()) {
        return;
    }
    let Some(interface) = file.interfaces.iter().find(|item| item.name.name == name) else {
        return;
    };
    for parent in &interface.parents {
        collect_interface_defaults(file, &parent.name.name, seen, defaults);
    }
    for method in &interface.methods {
        if method.body.is_none() {
            continue;
        }
        defaults.insert(method.name.name.clone(), (method.clone(), name.to_string()));
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
