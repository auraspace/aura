//! Shared compiler analysis API for batch tools and the language server.
//!
//! The host owns document snapshots and query caches. Parsing and semantic
//! implementation details remain in `aura-parser` and `aura-sema`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

use aura_ast::{File, Span};

pub mod formatter;

pub use aura_diagnostics::Severity;
pub use aura_parser::{
    declarative_macro_names, declarative_macro_sources, parse_file, parse_file_with_macro_sources,
    ParseError,
};
pub use aura_sema::{
    check_file, check_file_with_plugin_source, check_file_with_sandboxed_macro,
    decode_plugin_request, encode_plugin_request, encode_plugin_response, CheckedFile,
    MacroPluginRequest, MacroPluginResponse, MacroSandboxConfig, SemaError, SemaErrors,
    MACRO_PLUGIN_ABI_VERSION,
};

/// Stable identity for a document within an analysis host.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DocumentId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DocumentId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Monotonic revision identifying one immutable workspace snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnapshotId(u64);

impl SnapshotId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
struct Document {
    version: u64,
    source: Arc<str>,
}

#[derive(Default)]
struct QueryCache {
    parsed: BTreeMap<(DocumentId, Arc<str>), Result<Arc<File>, ParseError>>,
    analyzed: BTreeMap<(DocumentId, Arc<str>), Result<Arc<Analysis>, AnalysisError>>,
}

struct WorkspaceState {
    revision: u64,
    documents: BTreeMap<DocumentId, Document>,
    cache: Arc<Mutex<QueryCache>>,
}

/// Long-lived owner of document contents and shared analysis caches.
#[derive(Clone)]
pub struct AnalysisHost {
    state: Arc<RwLock<WorkspaceState>>,
}

impl Default for AnalysisHost {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisHost {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(WorkspaceState {
                revision: 0,
                documents: BTreeMap::new(),
                cache: Arc::new(Mutex::new(QueryCache::default())),
            })),
        }
    }

    /// Insert or replace a document and publish a new workspace revision.
    pub fn set_document(&self, id: impl Into<DocumentId>, source: impl Into<String>) -> SnapshotId {
        let mut state = self.state.write().expect("analysis host lock poisoned");
        state.revision += 1;
        let revision = state.revision;
        let id = id.into();
        state.documents.insert(
            id,
            Document {
                version: revision,
                source: Arc::from(source.into()),
            },
        );
        SnapshotId(revision)
    }

    /// Remove a document and publish a new workspace revision.
    pub fn remove_document(&self, id: &DocumentId) -> SnapshotId {
        let mut state = self.state.write().expect("analysis host lock poisoned");
        state.revision += 1;
        state.documents.remove(id);
        SnapshotId(state.revision)
    }

    pub fn snapshot(&self) -> AnalysisSnapshot {
        let state = self.state.read().expect("analysis host lock poisoned");
        let documents = state
            .documents
            .iter()
            .map(|(id, document)| {
                (
                    id.clone(),
                    Document {
                        version: document.version,
                        source: Arc::clone(&document.source),
                    },
                )
            })
            .collect();
        AnalysisSnapshot {
            id: SnapshotId(state.revision),
            documents: Arc::new(documents),
            cache: Arc::clone(&state.cache),
        }
    }
}

/// Immutable view of the workspace at one revision.
#[derive(Clone)]
pub struct AnalysisSnapshot {
    id: SnapshotId,
    documents: Arc<BTreeMap<DocumentId, Document>>,
    cache: Arc<Mutex<QueryCache>>,
}

impl AnalysisSnapshot {
    pub fn id(&self) -> SnapshotId {
        self.id
    }

    pub fn document_ids(&self) -> impl Iterator<Item = &DocumentId> {
        self.documents.keys()
    }

    pub fn source(&self, id: &DocumentId) -> Result<Arc<str>, QueryError> {
        self.documents
            .get(id)
            .map(|document| Arc::clone(&document.source))
            .ok_or_else(|| QueryError::DocumentNotFound(id.clone()))
    }

    pub fn document_version(&self, id: &DocumentId) -> Result<u64, QueryError> {
        self.documents
            .get(id)
            .map(|document| document.version)
            .ok_or_else(|| QueryError::DocumentNotFound(id.clone()))
    }

    /// Parse a document, reusing a successful or failed result for unchanged text.
    pub fn parse(&self, id: &DocumentId) -> Result<Arc<File>, QueryError> {
        let document = self.document(id)?;
        let key = (id.clone(), Arc::clone(&document.source));
        if let Some(result) = self
            .cache
            .lock()
            .expect("analysis cache lock poisoned")
            .parsed
            .get(&key)
        {
            return result.clone().map_err(QueryError::Parse);
        }
        let result = parse_file(&document.source).map(Arc::new);
        self.cache
            .lock()
            .expect("analysis cache lock poisoned")
            .parsed
            .insert(key, result.clone());
        result.map_err(QueryError::Parse)
    }

    /// Parse and typecheck a document through the shared compiler path.
    pub fn analyze(&self, id: &DocumentId) -> Result<Arc<Analysis>, QueryError> {
        let document = self.document(id)?;
        let key = (id.clone(), Arc::clone(&document.source));
        if let Some(result) = self
            .cache
            .lock()
            .expect("analysis cache lock poisoned")
            .analyzed
            .get(&key)
        {
            return result.clone().map_err(QueryError::Analysis);
        }
        let result = match self.parse(id) {
            Ok(ast) => check_file(&ast)
                .map(|checked| {
                    let ir = aura_ir::LoweredProgram::from_checked(checked.clone()).ir;
                    Analysis {
                        ast: (*ast).clone(),
                        checked,
                        ir,
                    }
                })
                .map_err(AnalysisError::Sema)
                .map(Arc::new),
            Err(QueryError::DocumentNotFound(id)) => {
                return Err(QueryError::DocumentNotFound(id));
            }
            Err(QueryError::Parse(error)) => Err(AnalysisError::Parse(error)),
            Err(QueryError::Analysis(error)) => Err(error),
        };
        self.cache
            .lock()
            .expect("analysis cache lock poisoned")
            .analyzed
            .insert(key, result.clone());
        result.map_err(QueryError::Analysis)
    }

    /// Return all syntax or semantic errors for one document at this snapshot.
    pub fn diagnostics(&self, id: &DocumentId) -> Result<Vec<Diagnostic>, QueryError> {
        match self.analyze(id) {
            Ok(_) => Ok(Vec::new()),
            Err(QueryError::Analysis(AnalysisError::Parse(error))) => Ok(vec![Diagnostic {
                severity: Severity::Error,
                message: error.message,
                span: error.span,
            }]),
            Err(QueryError::Analysis(AnalysisError::Sema(errors))) => Ok(errors
                .errors
                .into_iter()
                .map(|error| Diagnostic {
                    severity: Severity::Error,
                    message: error.message,
                    span: error.span,
                })
                .collect()),
            Err(error) => Err(error),
        }
    }

    fn document(&self, id: &DocumentId) -> Result<&Document, QueryError> {
        self.documents
            .get(id)
            .ok_or_else(|| QueryError::DocumentNotFound(id.clone()))
    }
}

/// A source diagnostic independent of the LSP protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

/// Error returned when a snapshot query cannot produce a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    DocumentNotFound(DocumentId),
    Parse(ParseError),
    Analysis(AnalysisError),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DocumentNotFound(id) => write!(f, "document `{id}` is not in this snapshot"),
            Self::Parse(error) => write!(f, "{error}"),
            Self::Analysis(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for QueryError {}

/// Successful analysis of one source document.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub ast: File,
    pub checked: CheckedFile,
    /// Backend-neutral facts produced by the shared analysis path.
    pub ir: aura_ir::CheckedIr,
}

/// Failure at one compiler phase while analyzing a source document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisError {
    Parse(ParseError),
    Sema(SemaErrors),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Sema(errors) => write!(f, "{errors}"),
        }
    }
}

impl std::error::Error for AnalysisError {}

impl From<ParseError> for AnalysisError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<SemaErrors> for AnalysisError {
    fn from(errors: SemaErrors) -> Self {
        Self::Sema(errors)
    }
}

/// Parse and typecheck one source document through the shared analysis path.
pub fn analyze_file(source: &str) -> Result<Analysis, AnalysisError> {
    let ast = parse_file(source)?;
    let checked = check_file(&ast)?;
    let ir = aura_ir::LoweredProgram::from_checked(checked.clone()).ir;
    Ok(Analysis { ast, checked, ir })
}

#[cfg(test)]
mod tests {
    use super::{analyze_file, AnalysisError, AnalysisHost, DocumentId, QueryError, Severity};
    use std::sync::Arc;

    #[test]
    fn analyzes_valid_source() {
        let result = analyze_file("package demo\nfun main() {}\n").unwrap();
        assert_eq!(result.checked.package, "demo");
        assert_eq!(result.checked.functions.len(), 1);
        assert_eq!(result.ir.package, "demo");
    }

    #[test]
    fn preserves_parse_phase_errors() {
        let error = analyze_file("package").unwrap_err();
        assert!(matches!(error, AnalysisError::Parse(_)));
    }

    #[test]
    fn snapshots_are_immutable_and_revisions_are_monotonic() {
        let host = AnalysisHost::new();
        let id = DocumentId::from("file:///main.aura");
        let first = host.set_document(id.clone(), "package demo\nfun main() {}\n");
        let snapshot = host.snapshot();
        host.set_document(id.clone(), "package changed\nfun main() {}\n");
        let latest = host.snapshot();

        assert!(latest.id() > snapshot.id());
        assert_eq!(first, snapshot.id());
        assert!(snapshot.source(&id).unwrap().contains("package demo"));
        assert!(latest.source(&id).unwrap().contains("package changed"));
    }

    #[test]
    fn unchanged_documents_reuse_query_results() {
        let host = AnalysisHost::new();
        let id = DocumentId::from("main.aura");
        host.set_document(id.clone(), "package demo\nfun main() {}\n");
        let first = host.snapshot();
        let first_ast = first.parse(&id).unwrap();
        let first_analysis = first.analyze(&id).unwrap();
        let second = host.snapshot();
        let second_ast = second.parse(&id).unwrap();
        let second_analysis = second.analyze(&id).unwrap();

        assert!(Arc::ptr_eq(&first_ast, &second_ast));
        assert!(Arc::ptr_eq(&first_analysis, &second_analysis));
    }

    #[test]
    fn diagnostics_are_phase_aware() {
        let host = AnalysisHost::new();
        let id = DocumentId::from("main.aura");
        host.set_document(id.clone(), "package\n");
        let diagnostics = host.snapshot().diagnostics(&id).unwrap();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn missing_documents_are_query_errors() {
        let snapshot = AnalysisHost::new().snapshot();
        let error = snapshot
            .parse(&DocumentId::from("missing.aura"))
            .unwrap_err();
        assert!(matches!(error, QueryError::DocumentNotFound(_)));
    }
}
