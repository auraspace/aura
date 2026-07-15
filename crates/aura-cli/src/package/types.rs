//! Loaded package types.

use aura_ast::{File, Span};
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
}
impl LoadedPackage {
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
