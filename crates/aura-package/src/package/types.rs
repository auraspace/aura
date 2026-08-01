//! Loaded package types.

use aura_analysis::{check_file, check_file_with_sandboxed_macro};
use aura_ast::{shift_file_spans, AttributeArg, AttributeValue, File, Span};
use aura_sema::{CheckedFile, MacroPluginRequest, MacroSandboxConfig, SemaErrors};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct SourceEntry {
    pub path: PathBuf,
    pub src: String,
    /// Inclusive start offset in the virtual concatenated source.
    pub base: u32,
    /// Exclusive end offset in the virtual source.
    pub end: u32,
}

/// Loaded compilation unit: one or more `.aura` files of the same package.
#[derive(Debug, Clone)]
pub struct LoadedPackage {
    /// Directory containing `aura.toml` or the single file's parent.
    pub root: PathBuf,
    /// Dotted package name from Aura sources.
    pub package: String,
    /// Binary name from `aura.toml` `[[bin]].name` or package last segment / file stem.
    pub bin_name: String,
    pub sources: Vec<SourceEntry>,
    /// Virtual concatenation of all sources (for fallback diagnostics).
    pub virtual_src: String,
    /// Merged AST with spans rewritten into `virtual_src`.
    pub ast: File,
    /// Declarative macros exported by this package, kept separate from the AST.
    pub(crate) macro_sources: Vec<String>,
    /// Root-package procedural macro executables declared in `[macro_plugins]`.
    pub macro_plugins: std::collections::BTreeMap<String, PathBuf>,
}
impl LoadedPackage {
    /// Check the package and run root-declared RFC-010 plugins for matching
    /// `@derive(Name)` attributes. Dependencies cannot silently execute their
    /// own plugins; only the package being built opts into executable paths.
    pub fn check_with_plugins(&self) -> Result<CheckedFile, SemaErrors> {
        let mut current = self.ast.clone();
        let invocations = current
            .classes
            .iter()
            .flat_map(|class| {
                class.attributes.iter().flat_map(|attribute| {
                    if attribute.name.name != "derive" {
                        return Vec::new();
                    }
                    attribute
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            AttributeArg::Positional(AttributeValue::Ident(name)) => {
                                Some((name.name.clone(), attribute.span))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let has_plugin = invocations
            .iter()
            .any(|(name, _)| self.macro_plugins.contains_key(name));
        if !has_plugin {
            return check_file(&current);
        }

        let mut checked = None;
        for (name, invocation_span) in invocations {
            let Some(plugin) = self.macro_plugins.get(&name) else {
                continue;
            };
            let request = MacroPluginRequest {
                macro_name: name,
                package: self.package.clone(),
                source: self.virtual_src.clone(),
                invocation_span,
            };
            let config = MacroSandboxConfig {
                plugin: plugin.clone(),
                source_root: self.root.clone(),
                ..Default::default()
            };
            let result = check_file_with_sandboxed_macro(&current, &config, &request, &[])?;
            current = result.ast.clone();
            checked = Some(result);
        }
        checked.ok_or_else(|| unreachable!("plugin invocation disappeared during expansion"))
    }

    /// Rebuild the package AST with in-memory editor contents substituted for
    /// matching source paths. Dependency resolution remains identical to the
    /// on-disk package graph.
    pub fn with_overlays(&self, overlays: &HashMap<PathBuf, String>) -> Result<Self, String> {
        let mut sources = Vec::with_capacity(self.sources.len());
        let mut virtual_src = String::new();
        let mut merged: Option<File> = None;

        for entry in &self.sources {
            let text = overlays
                .get(&entry.path)
                .cloned()
                .unwrap_or_else(|| entry.src.clone());
            let mut ast = aura_analysis::parse_file(&text)
                .map_err(|error| format!("error: {}: {}", entry.path.display(), error.message))?;
            let package = ast.package.display();
            stamp_origin(&mut ast, &package);
            if !virtual_src.is_empty() && !virtual_src.ends_with('\n') {
                virtual_src.push('\n');
            }
            let base = virtual_src.len() as u32;
            shift_file_spans(&mut ast, base);
            virtual_src.push_str(&text);
            if !virtual_src.ends_with('\n') {
                virtual_src.push('\n');
            }
            let end = virtual_src.len() as u32;
            append_file(&mut merged, ast);
            sources.push(SourceEntry {
                path: entry.path.clone(),
                src: text,
                base,
                end,
            });
        }

        let mut ast = merged.expect("loaded package has at least one source");
        // Preserve loader-injected imports such as the std.io auto-prelude;
        // they are intentionally absent from the user's source text.
        for import in &self.ast.imports {
            let present = ast.imports.iter().any(|candidate| {
                candidate.path.display() == import.path.display()
                    && candidate.origin_package == import.origin_package
                    && candidate.alias.as_ref().map(|alias| &alias.name)
                        == import.alias.as_ref().map(|alias| &alias.name)
            });
            if !present {
                ast.imports.push(import.clone());
            }
        }

        Ok(Self {
            root: self.root.clone(),
            package: self.package.clone(),
            bin_name: self.bin_name.clone(),
            sources,
            virtual_src,
            ast,
            macro_sources: self.macro_sources.clone(),
            macro_plugins: self.macro_plugins.clone(),
        })
    }

    /// Map a span in the virtual buffer back to a real file path, local source, and local span.
    pub fn locate(&self, span: Span) -> (String, &str, Span) {
        for s in &self.sources {
            if span.start >= s.base && span.start < s.end {
                let local = Span::new(span.start - s.base, span.end.saturating_sub(s.base));
                return (s.path.display().to_string(), s.src.as_str(), local);
            }
        }
        if let Some(s) = self.sources.first() {
            (
                s.path.display().to_string(),
                s.src.as_str(),
                Span::new(0, 0),
            )
        } else {
            ("<unknown>".into(), self.virtual_src.as_str(), span)
        }
    }
}

fn append_file(into: &mut Option<File>, file: File) {
    let Some(into) = into else {
        *into = Some(file);
        return;
    };
    into.imports.extend(file.imports);
    into.interfaces.extend(file.interfaces);
    into.enums.extend(file.enums);
    into.classes.extend(file.classes);
    into.type_aliases.extend(file.type_aliases);
    into.consts.extend(file.consts);
    into.functions.extend(file.functions);
    into.foreign_functions.extend(file.foreign_functions);
    into.async_functions.extend(file.async_functions);
}

fn stamp_origin(ast: &mut File, package: &str) {
    for import in &mut ast.imports {
        import.origin_package = package.to_owned();
    }
    for interface in &mut ast.interfaces {
        interface.origin_package = package.to_owned();
    }
    for enum_decl in &mut ast.enums {
        enum_decl.origin_package = package.to_owned();
    }
    for class in &mut ast.classes {
        class.origin_package = package.to_owned();
        for method in &mut class.methods {
            method.origin_package = package.to_owned();
        }
    }
    for alias in &mut ast.type_aliases {
        alias.origin_package = package.to_owned();
    }
    for constant in &mut ast.consts {
        constant.origin_package = package.to_owned();
    }
    for function in &mut ast.functions {
        function.origin_package = package.to_owned();
    }
    for function in &mut ast.foreign_functions {
        function.origin_package = package.to_owned();
    }
    for function in &mut ast.async_functions {
        function.origin_package = package.to_owned();
    }
}
