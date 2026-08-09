use aura_analysis::{
    formatter::format_source, parse_file, AnalysisHost, Diagnostic, DocumentId, Severity,
};
use aura_ast::{File, FunDecl, Span};
use aura_lexer::{lex, TokenKind};
use aura_package::{load_package_read_only, load_package_read_only_with_std};
use aura_sema::check_file;
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

const SERVER_NAME: &str = "auralsp";

#[path = "completion.rs"]
mod completion;
#[path = "editor.rs"]
mod editor;

type ExpressionTypes = Arc<HashMap<(u32, u32), aura_sema::Ty>>;
type PackageExpressionResult = (ExpressionTypes, HashMap<PathBuf, u32>);
type CancellationToken = Arc<AtomicBool>;
type CancellationRegistry = Arc<Mutex<HashMap<String, CancellationToken>>>;

struct PackageExpressionCache {
    revision: u64,
    expression_types: Option<ExpressionTypes>,
    source_bases: HashMap<PathBuf, u32>,
}

#[derive(Debug, Clone)]
struct BindingRecord {
    id: u64,
    name: String,
    kind: u32,
    scope: String,
    span: Span,
}

pub fn run_stdio() -> io::Result<()> {
    run_stdio_with_std_root(None)
}

/// Run the LSP with sources and diagnostics pinned to one toolchain std root.
pub fn run_stdio_with_std_root(std_root: Option<PathBuf>) -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let (work_tx, work_rx) = mpsc::channel::<(Value, Option<CancellationToken>)>();
    let cancellations: CancellationRegistry = Arc::new(Mutex::new(HashMap::new()));
    let worker_cancellations = Arc::clone(&cancellations);
    let output_error = Arc::new(Mutex::new(None::<io::Error>));
    let worker_output_error = Arc::clone(&output_error);
    let worker = std::thread::spawn(move || {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        let mut server = Server::with_std_root(std_root);
        while let Ok((message, token)) = work_rx.recv() {
            let id = message.get("id").cloned();
            if let Some(token) = &token {
                server.active_cancellation = Some(Arc::clone(token));
            }
            let response = if token
                .as_ref()
                .is_some_and(|token| token.load(Ordering::Acquire))
            {
                id.as_ref().map(cancelled_response)
            } else {
                server.handle(message)
            };
            let response = if token
                .as_ref()
                .is_some_and(|token| token.load(Ordering::Acquire))
            {
                id.as_ref().map(cancelled_response)
            } else {
                response
            };
            let write_result = (|| -> io::Result<()> {
                if let Some(response) = response {
                    write_message(&mut output, &response)?;
                }
                while let Some(notification) = server.pending_notifications.pop_front() {
                    write_message(&mut output, &notification)?;
                }
                Ok(())
            })();
            server.active_cancellation = None;
            if let Some(id) = id {
                if let Ok(mut requests) = worker_cancellations.lock() {
                    requests.remove(&request_id_key(&id));
                }
            }
            if let Err(error) = write_result {
                if let Ok(mut output_error) = worker_output_error.lock() {
                    *output_error = Some(error);
                }
                break;
            }
            if server.exited {
                break;
            }
        }
    });

    while let Some(message) = read_message(&mut input)? {
        let id = message.get("id").cloned();
        if message.get("method").and_then(Value::as_str) == Some("$/cancelRequest") {
            if let Some(request_id) = message.get("params").and_then(|params| params.get("id")) {
                if let Ok(requests) = cancellations.lock() {
                    if let Some(token) = requests.get(&request_id_key(request_id)) {
                        token.store(true, Ordering::Release);
                    }
                }
            }
        }
        let token = id.as_ref().map(|id| {
            let token = Arc::new(AtomicBool::new(false));
            if let Ok(mut requests) = cancellations.lock() {
                requests.insert(request_id_key(id), Arc::clone(&token));
            }
            token
        });
        if work_tx.send((message, token)).is_err() {
            break;
        }
    }
    drop(work_tx);
    let _ = worker.join();
    if let Ok(mut error) = output_error.lock() {
        if let Some(error) = error.take() {
            return Err(error);
        }
    }
    Ok(())
}

fn cancelled_response(id: &Value) -> Value {
    json!({
        "jsonrpc":"2.0",
        "id":id,
        "error":{"code":-32800,"message":"request cancelled"}
    })
}

fn read_message(input: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut content_length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse::<usize>().map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid Content-Length: {error}"),
                )
            })?);
        }
    }
    let length = content_length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP message has no Content-Length",
        )
    })?;
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body).map(Some).map_err(|error| {
        io::Error::new(io::ErrorKind::InvalidData, format!("invalid JSON: {error}"))
    })
}

fn write_message(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

struct Server {
    host: AnalysisHost,
    documents: Map<String, Value>,
    initialized: bool,
    shutdown: bool,
    exited: bool,
    pending_notifications: VecDeque<Value>,
    workspace_roots: Vec<PathBuf>,
    std_root: Option<PathBuf>,
    cancelled_requests: HashSet<String>,
    reported_analysis_failures: RefCell<HashSet<String>>,
    package_expression_cache: RefCell<HashMap<PathBuf, PackageExpressionCache>>,
    binding_records: HashMap<String, Vec<BindingRecord>>,
    next_binding_id: u64,
    active_cancellation: Option<CancellationToken>,
}

impl Server {
    #[cfg(test)]
    fn new() -> Self {
        Self::with_std_root(None)
    }

    fn with_std_root(std_root: Option<PathBuf>) -> Self {
        Self {
            host: AnalysisHost::new(),
            documents: Map::new(),
            initialized: false,
            shutdown: false,
            exited: false,
            pending_notifications: VecDeque::new(),
            workspace_roots: Vec::new(),
            std_root,
            cancelled_requests: HashSet::new(),
            reported_analysis_failures: RefCell::new(HashSet::new()),
            package_expression_cache: RefCell::new(HashMap::new()),
            binding_records: HashMap::new(),
            next_binding_id: 1,
            active_cancellation: None,
        }
    }

    fn handle(&mut self, message: Value) -> Option<Value> {
        let method = message.get("method")?.as_str()?.to_owned();
        let id = message.get("id").cloned();
        if id.is_none() {
            self.handle_notification(&method, message.get("params").unwrap_or(&Value::Null));
            return self.pending_notifications.pop_front();
        }
        let id = id.unwrap();
        if self.cancelled_requests.remove(&request_id_key(&id)) {
            return Some(json!({
                "jsonrpc":"2.0",
                "id":id,
                "error":{"code":-32800,"message":"request cancelled"}
            }));
        }
        let result = match method.as_str() {
            "initialize" => Ok(self.initialize(message.get("params"))),
            "shutdown" => {
                self.shutdown = true;
                Ok(Value::Null)
            }
            "textDocument/formatting" => self.format_document(message.get("params")),
            "textDocument/documentSymbol" => Ok(self.document_symbols(message.get("params"))),
            "textDocument/completion" => Ok(completion::completion(self, message.get("params"))),
            "textDocument/hover" => Ok(self.hover(message.get("params"))),
            "textDocument/definition" => Ok(self.definition(message.get("params"))),
            "textDocument/documentHighlight" => Ok(self.document_highlight(message.get("params"))),
            "textDocument/references" => Ok(self.references(message.get("params"))),
            "textDocument/rename" => self.rename(message.get("params")),
            "workspace/symbol" => Ok(self.workspace_symbols(message.get("params"))),
            "textDocument/codeAction" => Ok(self.code_actions(message.get("params"))),
            "textDocument/diagnostic" => Ok(self.pull_diagnostics(message.get("params"))),
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match result {
            Ok(result) => json!({"jsonrpc":"2.0", "id":id, "result":result}),
            Err((code, message)) => {
                json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
            }
        })
    }

    fn initialize(&mut self, params: Option<&Value>) -> Value {
        self.initialized = true;
        self.workspace_roots = workspace_roots(params);
        self.index_workspace();
        json!({
            "serverInfo": {"name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {
                "textDocumentSync": {"openClose":true,"change":2},
                "positionEncoding": "utf-16",
                "documentFormattingProvider": true,
                "documentSymbolProvider": true,
                "completionProvider": {"triggerCharacters": ["."]},
                "hoverProvider": true,
                "definitionProvider": true,
                "documentHighlightProvider": true,
                "referencesProvider": true,
                "renameProvider": true,
                "workspaceSymbolProvider": true,
                "codeActionProvider": {"codeActionKinds":["quickfix","source.format"]},
                "diagnosticProvider": {"interFileDependencies": false, "workspaceDiagnostics": false},
                "workspace": {"workspaceFolders": {"supported": true, "changeNotifications": true}}
            }
        })
    }

    fn index_workspace(&mut self) {
        let mut roots = self.workspace_roots.clone();
        if let Some(std_root) = &self.std_root {
            roots.push(std_root.clone());
        }
        for root in roots {
            self.index_root(&root);
        }
    }

    fn index_root(&mut self, root: &Path) {
        let mut files = Vec::new();
        collect_aura_files(root, &mut files);
        for path in files {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let uri = path_to_uri(&path);
            self.documents
                .entry(uri.clone())
                .or_insert_with(|| json!({"version":0,"text":text,"diskText":text,"open":false}));
            self.refresh_binding_index(&uri, &text);
            self.host.set_document(DocumentId::from(uri), text);
        }
    }

    fn handle_notification(&mut self, method: &str, params: &Value) {
        match method {
            "initialized" => {}
            "exit" => self.exited = true,
            "$/cancelRequest" => {
                if let Some(id) = params.get("id") {
                    self.cancelled_requests.insert(request_id_key(id));
                }
            }
            "textDocument/didOpen" => self.did_open(params),
            "textDocument/didChange" => self.did_change(params),
            "textDocument/didClose" => self.did_close(params),
            "textDocument/didSave" => self.did_save(params),
            "workspace/didChangeWatchedFiles" => self.did_change_watched_files(params),
            "workspace/didChangeWorkspaceFolders" => self.did_change_workspace_folders(params),
            _ => {}
        }
    }

    fn did_open(&mut self, params: &Value) {
        let Some(document) = params.get("textDocument") else {
            return;
        };
        let Some(uri) = document.get("uri").and_then(Value::as_str) else {
            return;
        };
        let Some(text) = document.get("text").and_then(Value::as_str) else {
            return;
        };
        let version = document.get("version").and_then(Value::as_i64).unwrap_or(0);
        let disk_text = self
            .documents
            .get(uri)
            .and_then(|document| document.get("diskText"))
            .cloned();
        let mut overlay = json!({"version":version,"text":text,"open":true});
        if let Some(disk_text) = disk_text {
            overlay["diskText"] = disk_text;
        }
        self.documents.insert(uri.to_owned(), overlay);
        self.refresh_binding_index(uri, text);
        self.publish_diagnostics(uri, text);
    }

    fn did_change(&mut self, params: &Value) {
        let Some(document) = params.get("textDocument") else {
            return;
        };
        let Some(uri) = document.get("uri").and_then(Value::as_str) else {
            return;
        };
        let version = document.get("version").and_then(Value::as_i64).unwrap_or(0);
        let Some(current) = self.documents.get(uri).cloned() else {
            return;
        };
        if !current
            .get("open")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return;
        }
        if version < current.get("version").and_then(Value::as_i64).unwrap_or(0) {
            return;
        }
        let Some(changes) = params.get("contentChanges").and_then(Value::as_array) else {
            return;
        };
        let Some(previous_text) = current.get("text").and_then(Value::as_str) else {
            return;
        };
        let disk_text = current.get("diskText").cloned();
        let Some(text) = apply_content_changes(previous_text, changes) else {
            return;
        };
        let mut overlay = json!({"version":version,"text":text,"open":true});
        if let Some(disk_text) = disk_text {
            overlay["diskText"] = disk_text;
        }
        self.documents.insert(uri.to_owned(), overlay);
        self.refresh_binding_index(uri, &text);
        self.publish_diagnostics(uri, &text);
    }

    fn did_close(&mut self, params: &Value) {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|d| d.get("uri"))
            .and_then(Value::as_str)
        else {
            return;
        };
        let was_open = self
            .documents
            .get(uri)
            .and_then(|document| document.get("open"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !was_open {
            return;
        }
        let disk_text = self
            .documents
            .get(uri)
            .and_then(|document| document.get("diskText"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(disk_text) = disk_text {
            if let Some(document) = self.documents.get_mut(uri) {
                document["open"] = Value::Bool(false);
                document["version"] = json!(0);
                document["text"] = Value::String(disk_text.clone());
            }
            self.refresh_binding_index(uri, &disk_text);
            self.host.set_document(DocumentId::from(uri), disk_text);
        } else {
            self.documents.remove(uri);
            self.binding_records.remove(uri);
            self.host.remove_document(&DocumentId::from(uri));
        }
        self.pending_notifications.push_back(json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":uri,"diagnostics":[]}}));
    }

    fn did_save(&mut self, params: &Value) {
        let Some(uri) = params
            .get("textDocument")
            .and_then(|document| document.get("uri"))
            .and_then(Value::as_str)
        else {
            return;
        };
        let Some(document) = self.documents.get_mut(uri) else {
            return;
        };
        let current_text = document
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let saved_text = params
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or(&current_text)
            .to_owned();
        document["diskText"] = Value::String(saved_text.clone());
        if !document
            .get("open")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            document["text"] = Value::String(saved_text.clone());
            self.refresh_binding_index(uri, &saved_text);
            self.host.set_document(DocumentId::from(uri), saved_text);
        }
    }

    fn did_change_watched_files(&mut self, params: &Value) {
        let Some(changes) = params.get("changes").and_then(Value::as_array) else {
            return;
        };
        for change in changes {
            let Some(uri) = change.get("uri").and_then(Value::as_str) else {
                continue;
            };
            if self
                .documents
                .get(uri)
                .and_then(|document| document.get("open"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            let Some(path) = uri_to_path(uri) else {
                continue;
            };
            if let Ok(text) = fs::read_to_string(&path) {
                self.documents.insert(
                    uri.to_owned(),
                    json!({"version":0,"text":text,"diskText":text,"open":false}),
                );
                self.refresh_binding_index(uri, &text);
                self.host.set_document(DocumentId::from(uri), text);
            } else {
                self.documents.remove(uri);
                self.binding_records.remove(uri);
                self.host.remove_document(&DocumentId::from(uri));
                self.pending_notifications.push_back(json!({
                    "jsonrpc":"2.0",
                    "method":"textDocument/publishDiagnostics",
                    "params":{"uri":uri,"diagnostics":[]}
                }));
            }
        }
    }

    fn did_change_workspace_folders(&mut self, params: &Value) {
        let Some(event) = params.get("event") else {
            return;
        };
        if let Some(removed) = event.get("removed").and_then(Value::as_array) {
            for folder in removed {
                let Some(path) = folder
                    .get("uri")
                    .and_then(Value::as_str)
                    .and_then(uri_to_path)
                else {
                    continue;
                };
                let uris = self
                    .documents
                    .keys()
                    .filter(|uri| uri_to_path(uri).is_some_and(|file| file.starts_with(&path)))
                    .cloned()
                    .collect::<Vec<_>>();
                for uri in uris {
                    if self
                        .documents
                        .get(&uri)
                        .and_then(|document| document.get("open"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    self.documents.remove(&uri);
                    self.binding_records.remove(&uri);
                    self.host.remove_document(&DocumentId::from(uri.as_str()));
                }
                self.workspace_roots.retain(|root| root != &path);
            }
        }
        if let Some(added) = event.get("added").and_then(Value::as_array) {
            for folder in added {
                if let Some(path) = folder
                    .get("uri")
                    .and_then(Value::as_str)
                    .and_then(uri_to_path)
                {
                    self.workspace_roots.push(path);
                }
            }
            self.workspace_roots.sort();
            self.workspace_roots.dedup();
            self.index_workspace();
        }
    }

    fn publish_diagnostics(&mut self, uri: &str, text: &str) {
        let id = DocumentId::from(uri);
        self.host.set_document(id.clone(), text.to_owned());
        let diagnostics = self.diagnostics_for_document(uri, text, &id);
        let version = self
            .documents
            .get(uri)
            .and_then(|document| document.get("version"))
            .cloned()
            .unwrap_or_else(|| json!(0));
        self.pending_notifications.push_back(json!({
            "jsonrpc":"2.0", "method":"textDocument/publishDiagnostics",
            "params":{"uri":uri,"version":version,"diagnostics":diagnostics}
        }));
    }

    fn pull_diagnostics(&self, params: Option<&Value>) -> Value {
        let Some(uri) = params
            .and_then(|p| p.get("textDocument"))
            .and_then(|d| d.get("uri"))
            .and_then(Value::as_str)
        else {
            return json!({"kind":"full","items":[]});
        };
        let Some(document) = self.documents.get(uri) else {
            return json!({"kind":"full","items":[]});
        };
        let text = document.get("text").and_then(Value::as_str).unwrap_or("");
        let id = DocumentId::from(uri);
        let diagnostics = self.diagnostics_for_document(uri, text, &id);
        let version = document.get("version").and_then(Value::as_i64).unwrap_or(0);
        let result_id = format!("{uri}@{version}");
        if params
            .and_then(|params| params.get("previousResultId"))
            .and_then(Value::as_str)
            == Some(result_id.as_str())
        {
            return json!({"kind":"unchanged","resultId":result_id});
        }
        json!({"kind":"full","resultId":result_id,"items":diagnostics})
    }

    fn diagnostics_for_document(&self, uri: &str, text: &str, id: &DocumentId) -> Vec<Value> {
        self.package_diagnostics(uri, text).unwrap_or_else(|| {
            self.host
                .snapshot()
                .diagnostics(id)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| diagnostic_json(text, &item))
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    // Semantic checking needs the package unit, not an individual source file.
    fn package_diagnostics(&self, uri: &str, text: &str) -> Option<Vec<Value>> {
        let target_path = uri_to_path(uri)?;
        let manifest = manifest_for(&target_path)?;
        let overlays = self
            .documents
            .iter()
            .filter_map(|(document_uri, document)| {
                let path = uri_to_path(document_uri)?;
                let text = document.get("text")?.as_str()?.to_owned();
                Some((path, text))
            })
            .collect::<HashMap<_, _>>();
        let mut overlays = overlays;
        overlays.insert(target_path.clone(), text.to_owned());
        let package = self
            .std_root
            .as_deref()
            .map_or_else(
                || load_package_read_only(&manifest),
                |std_root| load_package_read_only_with_std(&manifest, std_root),
            )
            .ok()?
            .with_overlays(&overlays)
            .ok()?;
        let target = package
            .sources
            .iter()
            .find(|source| source.path == target_path)?;
        let target_base = target.base;
        let target_end = target.end;
        let source_bases = package
            .sources
            .iter()
            .map(|source| (source.path.clone(), source.base))
            .collect::<HashMap<_, _>>();
        let revision = self.host.snapshot().id().get();

        let errors = match check_file(&package.ast) {
            Ok(checked) => {
                self.store_package_expression_cache(
                    manifest,
                    revision,
                    Some((Arc::new(checked.expr_tys), source_bases)),
                );
                return Some(Vec::new());
            }
            Err(errors) => {
                self.store_package_expression_cache(manifest, revision, None);
                errors.errors
            }
        };
        Some(
            errors
                .into_iter()
                .filter(|error| error.span.start >= target_base && error.span.start < target_end)
                .map(|error| Diagnostic {
                    severity: Severity::Error,
                    message: error.message,
                    span: Span::new(
                        error.span.start - target_base,
                        error.span.end.saturating_sub(target_base),
                    ),
                })
                .map(|diagnostic| diagnostic_json(text, &diagnostic))
                .collect(),
        )
    }

    fn format_document(&self, params: Option<&Value>) -> Result<Value, (i32, String)> {
        let uri = params
            .and_then(|p| p.get("textDocument"))
            .and_then(|d| d.get("uri"))
            .and_then(Value::as_str)
            .ok_or((-32602, "formatting requires textDocument.uri".into()))?;
        let document = self
            .documents
            .get(uri)
            .ok_or((-32602, format!("document `{uri}` is not open")))?;
        let source = document.get("text").and_then(Value::as_str).unwrap_or("");
        let formatted = format_source(source).map_err(|error| (-32602, error))?;
        if formatted == source {
            return Ok(json!([]));
        }
        Ok(json!([{
            "range": full_document_range(source),
            "newText": formatted
        }]))
    }

    fn document_symbols(&self, params: Option<&Value>) -> Value {
        let Some(uri) = uri_param(params) else {
            return json!([]);
        };
        let Some(source) = self.document_text(uri) else {
            return json!([]);
        };
        let Some((_, file)) = editor::parse_editor_file(source, source.len()) else {
            return json!([]);
        };
        json!(collect_symbols(source, &file))
    }

    fn hover(&self, params: Option<&Value>) -> Value {
        let Some(uri) = uri_param(params) else {
            return Value::Null;
        };
        let Some(source) = self.document_text(uri) else {
            return Value::Null;
        };
        let offset = params
            .and_then(|p| p.get("position"))
            .map(|p| position_to_offset(source, p))
            .unwrap_or(0);
        let Some(word_span) = word_span_at(source, offset) else {
            return Value::Null;
        };
        let name = source[word_span.start as usize..word_span.end as usize].to_owned();
        let Some((definition_uri, symbol)) = self.definition_symbol_at(uri, &name, word_span)
        else {
            return Value::Null;
        };
        let expression_type = self
            .expression_type(uri, word_span, symbol.range)
            .filter(|_| symbol.kind == 13);
        let local_type_name = expression_type
            .as_ref()
            .and_then(nominal_type_name)
            .map(str::to_owned)
            .or_else(|| self.local_initializer_type_name(uri, symbol.range));
        let local_type_declaration = local_type_name
            .as_deref()
            .and_then(|type_name| self.find_declaration_symbol(type_name));
        let semantic_detail = self.semantic_detail(&definition_uri, &name, &symbol);
        let primary_signature = semantic_detail.clone().unwrap_or_else(|| {
            if let Some(ty) = expression_type.as_ref() {
                format!("{}: {}", name, ty.display())
            } else if let Some(type_name) = local_type_name.as_deref() {
                format!("{}: {}", name, type_name)
            } else if symbol.detail.is_empty() {
                symbol.name.clone()
            } else {
                compact_symbol_detail(&symbol.detail)
            }
        });
        let type_signature = if symbol.kind == 13 {
            local_type_declaration
                .as_ref()
                .map(|(_, type_symbol)| compact_symbol_detail(&type_symbol.detail))
                .filter(|type_signature| type_signature != &primary_signature)
        } else {
            None
        };
        let documentation = self
            .document_text(&definition_uri)
            .map(|text| documentation_before(text, symbol.range.start as usize))
            .unwrap_or_default();
        let documentation = if documentation.is_empty() {
            local_type_declaration
                .as_ref()
                .and_then(|(type_uri, type_symbol)| {
                    self.document_text(type_uri)
                        .map(|text| documentation_before(text, type_symbol.range.start as usize))
                })
                .unwrap_or_default()
        } else {
            documentation
        };
        let mut contents = vec![json!({
            "language": "aura",
            "value": primary_signature
        })];
        if let Some(type_signature) = type_signature {
            contents.push(Value::String("---".to_owned()));
            contents.push(json!({
                "language": "aura",
                "value": type_signature
            }));
        }
        if !documentation.is_empty() {
            contents.push(Value::String("---".to_owned()));
            contents.push(Value::String(documentation));
        }
        json!({"contents":contents,"range":span_range(source, word_span)})
    }

    fn definition(&self, params: Option<&Value>) -> Value {
        let Some(uri) = uri_param(params) else {
            return json!([]);
        };
        let Some(source) = self.document_text(uri) else {
            return json!([]);
        };
        let offset = params
            .and_then(|p| p.get("position"))
            .map(|p| position_to_offset(source, p))
            .unwrap_or(0);
        let Some(name) = word_at(source, offset) else {
            return json!([]);
        };
        let Some(origin_span) = word_span_at(source, offset) else {
            return json!([]);
        };
        let Some((definition_uri, symbol)) = self.definition_symbol_at(uri, &name, origin_span)
        else {
            return json!([]);
        };
        let definition_source = self.document_text(&definition_uri).unwrap_or("");
        let result = json!([{
            "originSelectionRange": span_range(source, origin_span),
            "targetUri": definition_uri,
            "targetRange": span_range(definition_source, symbol.range),
            "targetSelectionRange": span_range(definition_source, symbol.span)
        }]);
        result
    }

    fn document_highlight(&self, params: Option<&Value>) -> Value {
        let Some(uri) = uri_param(params) else {
            return json!([]);
        };
        let Some(source) = self.document_text(uri) else {
            return json!([]);
        };
        let offset = params
            .and_then(|p| p.get("position"))
            .map(|p| position_to_offset(source, p))
            .unwrap_or(0);
        let Some(name) = word_at(source, offset) else {
            return json!([]);
        };
        if self.find_symbol(&name).is_none() {
            return json!([]);
        }
        json!(word_occurrences(source, &name)
            .into_iter()
            .map(|span| json!({"range":span_range(source, span),"kind":1}))
            .collect::<Vec<_>>())
    }

    fn references(&self, params: Option<&Value>) -> Value {
        let Some(uri) = uri_param(params) else {
            return json!([]);
        };
        let Some(source) = self.document_text(uri) else {
            return json!([]);
        };
        let offset = params
            .and_then(|p| p.get("position"))
            .map(|p| position_to_offset(source, p))
            .unwrap_or(0);
        let Some(word_span) = word_span_at(source, offset) else {
            return json!([]);
        };
        let name = &source[word_span.start as usize..word_span.end as usize];
        let Some((target_uri, target_symbol)) = self.definition_symbol_at(uri, name, word_span)
        else {
            return json!([]);
        };
        let target_id = self.binding_id(&target_uri, &target_symbol);
        let include_declaration = params
            .and_then(|p| p.get("context"))
            .and_then(|context| context.get("includeDeclaration"))
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut locations = Vec::new();
        for (document_uri, document) in &self.documents {
            if self.is_cancelled() {
                return Value::Array(locations);
            }
            let Some(text) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            for occurrence in word_occurrences(text, name) {
                if self.is_cancelled() {
                    return Value::Array(locations);
                }
                let Some((resolved_uri, resolved_symbol)) =
                    self.find_symbol_at(document_uri, name, occurrence)
                else {
                    continue;
                };
                if self.binding_id(&resolved_uri, &resolved_symbol) != target_id {
                    continue;
                }
                if !include_declaration && declaration_at(text, name, occurrence) {
                    continue;
                }
                locations.push(json!({"uri":document_uri,"range":span_range(text, occurrence)}));
            }
        }
        locations.sort_by(|left, right| {
            (
                left["uri"].as_str(),
                left["range"]["start"]["line"].as_u64(),
            )
                .cmp(&(
                    right["uri"].as_str(),
                    right["range"]["start"]["line"].as_u64(),
                ))
        });
        Value::Array(locations)
    }

    fn rename(&self, params: Option<&Value>) -> Result<Value, (i32, String)> {
        let uri = uri_param(params).ok_or((-32602, "rename requires textDocument.uri".into()))?;
        let source = self
            .document_text(uri)
            .ok_or((-32602, format!("document `{uri}` is not open")))?;
        let offset = params
            .and_then(|p| p.get("position"))
            .map(|p| position_to_offset(source, p))
            .unwrap_or(0);
        let word_span = word_span_at(source, offset)
            .ok_or((-32602, "rename position is not an identifier".into()))?;
        let name = &source[word_span.start as usize..word_span.end as usize];
        let (target_uri, target_symbol) =
            self.definition_symbol_at(uri, name, word_span).ok_or((
                -32602,
                format!("cannot safely rename unresolved symbol `{name}`"),
            ))?;
        let target_id = self.binding_id(&target_uri, &target_symbol);
        let new_name = params
            .and_then(|p| p.get("newName"))
            .and_then(Value::as_str)
            .ok_or((-32602, "rename requires newName".into()))?;
        if !valid_identifier(new_name) {
            return Err((-32602, "newName must be a valid Aura identifier".into()));
        }
        let mut document_changes = Vec::new();
        for (document_uri, document) in &self.documents {
            if self.is_cancelled() {
                return Ok(json!({"documentChanges":document_changes}));
            }
            let Some(text) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let edits = word_occurrences(text, name)
                .into_iter()
                .filter_map(|span| {
                    if self.is_cancelled() {
                        return None;
                    }
                    let (resolved_uri, resolved_symbol) =
                        self.find_symbol_at(document_uri, name, span)?;
                    (self.binding_id(&resolved_uri, &resolved_symbol) == target_id)
                        .then(|| json!({"range":span_range(text, span),"newText":new_name}))
                })
                .collect::<Vec<_>>();
            if edits.is_empty() {
                continue;
            }
            let version = document.get("version").and_then(Value::as_i64).unwrap_or(0);
            document_changes
                .push(json!({"textDocument":{"uri":document_uri,"version":version},"edits":edits}));
        }
        Ok(json!({"documentChanges":document_changes}))
    }

    fn workspace_symbols(&self, params: Option<&Value>) -> Value {
        let query = params
            .and_then(|p| p.get("query"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut results = Vec::new();
        for (uri, document) in &self.documents {
            if self.is_cancelled() {
                return Value::Array(results);
            }
            let Some(source) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some((_, file)) = editor::parse_editor_file(source, source.len()) else {
                continue;
            };
            for symbol in flatten_symbols(&declaration_symbols(source, &file)) {
                if self.is_cancelled() {
                    return Value::Array(results);
                }
                if !symbol.name.to_ascii_lowercase().contains(&query) {
                    continue;
                }
                results.push(json!({
                    "name":symbol.name,
                    "kind":symbol.kind,
                    "containerName":file.package.display(),
                    "location":{"uri":uri,"range":span_range(source, symbol.range)}
                }));
                if results.len() >= 1000 {
                    return Value::Array(results);
                }
            }
        }
        results.sort_by(|left, right| {
            (left["name"].as_str(), left["location"]["uri"].as_str())
                .cmp(&(right["name"].as_str(), right["location"]["uri"].as_str()))
        });
        Value::Array(results)
    }

    fn code_actions(&self, params: Option<&Value>) -> Value {
        let Some(uri) = uri_param(params) else {
            return json!([]);
        };
        let Some(document) = self.documents.get(uri) else {
            return json!([]);
        };
        let Some(source) = document.get("text").and_then(Value::as_str) else {
            return json!([]);
        };
        let version = document.get("version").and_then(Value::as_i64).unwrap_or(0);
        let mut actions = Vec::new();
        if let Some(diagnostics) = params
            .and_then(|params| params.get("context"))
            .and_then(|context| context.get("diagnostics"))
            .and_then(Value::as_array)
        {
            for diagnostic in diagnostics {
                let Some(message) = diagnostic.get("message").and_then(Value::as_str) else {
                    continue;
                };
                let Some(new_name) = did_you_mean_name(message) else {
                    continue;
                };
                let Some(range) = diagnostic.get("range") else {
                    continue;
                };
                actions.push(json!({
                    "title":format!("Replace with `{new_name}`"),
                    "kind":"quickfix",
                    "diagnostics":[diagnostic],
                    "isPreferred":true,
                    "edit":{"documentChanges":[{"textDocument":{"uri":uri,"version":version},"edits":[{"range":range,"newText":new_name}]}]}
                }));
            }
        }
        let Ok(formatted) = format_source(source) else {
            return Value::Array(actions);
        };
        if formatted != source {
            actions.push(json!({
                "title":"Format document",
                "kind":"source.format",
                "isPreferred":true,
                "edit":{"documentChanges":[{"textDocument":{"uri":uri,"version":version},"edits":[{"range":full_document_range(source),"newText":formatted}]}]}
            }));
        }
        Value::Array(actions)
    }

    fn find_symbol(&self, name: &str) -> Option<(String, Symbol)> {
        self.find_symbols(name).into_iter().next()
    }

    fn find_declaration_symbol(&self, name: &str) -> Option<(String, Symbol)> {
        for (uri, document) in &self.documents {
            let Some(source) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some((_, file)) = editor::parse_editor_file(source, source.len()) else {
                continue;
            };
            if let Some(symbol) = flatten_symbols(&declaration_symbols(source, &file))
                .into_iter()
                .find(|symbol| symbol.name == name)
            {
                return Some((uri.clone(), symbol.clone()));
            }
        }
        None
    }

    fn find_declaration_in_package(&self, package: &str, name: &str) -> Option<(String, Symbol)> {
        for (uri, document) in &self.documents {
            let Some(source) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some((_, file)) = editor::parse_editor_file(source, source.len()) else {
                continue;
            };
            if file.package.display() != package {
                continue;
            }
            if let Some(symbol) = flatten_symbols(&declaration_symbols(source, &file))
                .into_iter()
                .find(|symbol| symbol.name == name)
            {
                return Some((uri.clone(), symbol.clone()));
            }
        }
        None
    }

    fn import_alias_symbol(&self, package: &str, alias: &str) -> Option<(String, Symbol)> {
        for (uri, document) in &self.documents {
            let Some(source) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some((_, file)) = editor::parse_editor_file(source, source.len()) else {
                continue;
            };
            if file.package.display() != package {
                continue;
            }
            return Some((
                uri.clone(),
                Symbol {
                    name: alias.to_owned(),
                    kind: 9,
                    detail: format!("import {package} as {alias}"),
                    span: file.package.span,
                    range: file.package.span,
                    children: Vec::new(),
                },
            ));
        }
        None
    }

    fn find_function_return_type_name(&self, name: &str) -> Option<String> {
        for document in self.documents.values() {
            let Some(source) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some((_, file)) = editor::parse_editor_file(source, source.len()) else {
                continue;
            };
            let return_type = file
                .functions
                .iter()
                .find(|function| function.name.name == name)
                .and_then(|function| function.return_type.as_ref())
                .or_else(|| {
                    file.async_functions
                        .iter()
                        .find(|function| function.name.name == name)
                        .and_then(|function| function.return_type.as_ref())
                })
                .or_else(|| {
                    file.foreign_functions
                        .iter()
                        .find(|function| function.name.name == name)
                        .and_then(|function| function.return_type.as_ref())
                });
            if let Some(return_type) = return_type {
                return Some(source_span(source, return_type.span));
            }
        }
        None
    }

    fn find_symbol_at(&self, uri: &str, name: &str, word_span: Span) -> Option<(String, Symbol)> {
        let source = self.document_text(uri)?;
        let (_, file) = editor::parse_editor_file(source, source.len())?;
        let locals = local_symbols(source, &file);

        if let Some(symbol) = locals
            .iter()
            .find(|symbol| symbol.name == name && symbol.span == word_span)
        {
            return Some((uri.to_owned(), symbol.clone()));
        }

        if let Some(scope) = enclosing_callable_span(&file, word_span.start) {
            if let Some(symbol) = locals
                .iter()
                .filter(|symbol| {
                    symbol.name == name
                        && symbol.span.start <= word_span.start
                        && symbol.range.start >= scope.start
                        && symbol.range.end <= scope.end
                })
                .max_by_key(|symbol| symbol.span.start)
            {
                return Some((uri.to_owned(), symbol.clone()));
            }
        }

        if let Some(package) = imported_package_for_alias(&file, name) {
            return self.import_alias_symbol(&package, name);
        }

        if let Some(symbol) = flatten_symbols(&declaration_symbols(source, &file))
            .into_iter()
            .find(|symbol| symbol.name == name && symbol.span == word_span)
        {
            return Some((uri.to_owned(), symbol.clone()));
        }

        if let Some(qualifier) = qualifier_before(source, word_span) {
            if !matches!(qualifier, "this" | "super") {
                if let Some(package) = imported_package_for_alias(&file, qualifier) {
                    if let Some(symbol) = self.find_declaration_in_package(&package, name) {
                        return Some(symbol);
                    }
                }
            }
        }

        flatten_symbols(&declaration_symbols(source, &file))
            .into_iter()
            .find(|symbol| symbol.name == name)
            .cloned()
            .map(|symbol| (uri.to_owned(), symbol))
            .or_else(|| self.find_symbol(name))
    }

    fn definition_symbol_at(
        &self,
        uri: &str,
        name: &str,
        word_span: Span,
    ) -> Option<(String, Symbol)> {
        let source = self.document_text(uri)?;
        let (_, file) = editor::parse_editor_file(source, source.len())?;
        if let Some(symbol) = self.resolved_call_declaration(uri, name, word_span) {
            return Some(symbol);
        }
        if let Some(qualifier) = qualifier_before(source, word_span) {
            if !matches!(qualifier, "this" | "super")
                && imported_package_for_alias(&file, qualifier).is_none()
            {
                let receiver_span = receiver_span(source, word_span.start as usize)?;
                let receiver_name = self
                    .expression_type_at(uri, receiver_span)
                    .and_then(|ty| nominal_type_name(&ty).map(str::to_owned))
                    .or_else(|| {
                        receiver_field_type_name(source, &file, receiver_span, word_span.start)
                    })?;
                return self.find_member_declaration(&receiver_name, name, source, word_span);
            }
        }
        self.find_symbol_at(uri, name, word_span)
    }

    /// Resolve a call-site to the declaration selected by semantic checking.
    /// The span is stable across overloads with identical arity and avoids
    /// reconstructing type-driven overload selection in the LSP layer.
    fn resolved_call_declaration(
        &self,
        uri: &str,
        name: &str,
        word_span: Span,
    ) -> Option<(String, Symbol)> {
        let analysis = self.host.snapshot().analyze(&DocumentId::from(uri)).ok()?;
        let declaration_span = analysis
            .checked
            .call_instantiations
            .iter()
            .filter(|(start, call)| {
                **start <= word_span.start
                    && call.name == name
                    && word_span.start.saturating_sub(**start) <= 4096
            })
            .max_by_key(|(start, _)| **start)
            .and_then(|(_, call)| call.declaration_span)?;

        for (document_uri, document) in &self.documents {
            let Some(text) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some((_, file)) = editor::parse_editor_file(text, text.len()) else {
                continue;
            };
            if let Some(symbol) = flatten_symbols(&declaration_symbols(text, &file))
                .into_iter()
                .find(|symbol| symbol.range == declaration_span)
            {
                return Some((document_uri.clone(), symbol.clone()));
            }
        }
        None
    }

    fn expression_type_at(&self, uri: &str, span: Span) -> Option<aura_sema::Ty> {
        let (expression_types, base) = self.expression_types_for_document(uri)?;
        let span = span.shift(base);
        expression_types
            .get(&(span.start, span.end))
            .cloned()
            .or_else(|| {
                expression_types
                    .iter()
                    .filter(|((start, end), _)| *start >= span.start && *end <= span.end)
                    .max_by_key(|((start, end), _)| end - start)
                    .map(|(_, ty)| ty.clone())
            })
    }

    fn find_member_declaration(
        &self,
        receiver_name: &str,
        name: &str,
        source: &str,
        word_span: Span,
    ) -> Option<(String, Symbol)> {
        let receiver_name = nominal_short_name(receiver_name);
        let argument_count = call_argument_count(source, word_span.end as usize);
        for (uri, document) in &self.documents {
            let Some(source) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some((_, file)) = editor::parse_editor_file(source, source.len()) else {
                continue;
            };
            let Some(symbol) = declaration_symbols(source, &file)
                .into_iter()
                .find(|symbol| nominal_short_name(&symbol.name) == receiver_name)
            else {
                continue;
            };
            let mut members = symbol
                .children
                .into_iter()
                .filter(|member| member.name == name)
                .collect::<Vec<_>>();
            members.sort_by_key(|member| member.range.start);
            let member = argument_count
                .and_then(|count| {
                    members
                        .iter()
                        .find(|member| function_parameter_count(&member.detail) == Some(count))
                })
                .or_else(|| members.first())
                .cloned();
            if let Some(member) = member {
                return Some((uri.clone(), member));
            }
        }
        None
    }

    fn semantic_detail(&self, uri: &str, name: &str, symbol: &Symbol) -> Option<String> {
        let analysis = self.host.snapshot().analyze(&DocumentId::from(uri)).ok();
        let Some(analysis) = analysis else {
            return (symbol.kind == 6 || symbol.kind == 12)
                .then(|| compact_symbol_detail(&symbol.detail));
        };
        if let Some(function) = analysis
            .checked
            .functions
            .iter()
            .find(|function| function.name == name)
        {
            return Some(format_function_signature(
                &function.name,
                &function.params,
                &function.ret,
            ));
        }
        for class in &analysis.checked.classes {
            if class.name == name {
                return Some(format!(
                    "{} {}",
                    if class.is_struct { "struct" } else { "class" },
                    class.name
                ));
            }
            if let Some(field) = class.fields.iter().find(|field| field.name == name) {
                return Some(format!("{}: {}", field.name, field.ty.display()));
            }
            if let Some(method) = class
                .method_overloads
                .get(name)
                .and_then(|methods| methods.iter().find(|method| method.span == symbol.range))
                .or_else(|| class.methods.get(name))
            {
                return Some(format_function_signature(
                    &method.name,
                    &method.params,
                    &method.ret,
                ));
            }
        }
        if let Some(interface) = analysis
            .checked
            .interfaces
            .iter()
            .find(|interface| interface.name == name)
        {
            return Some(format!("interface {}", interface.name));
        }
        if let Some(enumeration) = analysis
            .checked
            .enums
            .iter()
            .find(|enumeration| enumeration.name == name)
        {
            return Some(format!("enum {}", enumeration.name));
        }
        None
    }

    fn find_symbols(&self, name: &str) -> Vec<(String, Symbol)> {
        let mut matches = Vec::new();
        for (uri, document) in &self.documents {
            let Some(source) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Some((_, file)) = editor::parse_editor_file(source, source.len()) else {
                continue;
            };
            for symbol in flatten_symbols(&declaration_symbols(source, &file)) {
                if symbol.name == name {
                    matches.push((uri.clone(), symbol.clone()));
                }
            }
            for symbol in self.local_symbols_for_document(uri, source, &file) {
                if symbol.name == name {
                    matches.push((uri.clone(), symbol));
                }
            }
        }
        matches
    }

    fn is_cancelled(&self) -> bool {
        self.active_cancellation
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Acquire))
    }

    /// Keep binding identities stable when an edit shifts declaration spans.
    /// Matching is scoped by symbol kind/name and the nearest declaration in
    /// the same callable or class; unmatched declarations receive new IDs.
    fn refresh_binding_index(&mut self, uri: &str, source: &str) {
        let Ok(file) = parse_file(source) else {
            return;
        };
        let mut symbols = Vec::new();
        for symbol in declaration_symbols(source, &file) {
            symbols.extend(
                flatten_symbols(std::slice::from_ref(&symbol))
                    .into_iter()
                    .cloned(),
            );
        }
        symbols.extend(local_symbols(source, &file));
        let mut candidates = symbols
            .into_iter()
            .map(|symbol| BindingRecord {
                id: 0,
                name: symbol.name.clone(),
                kind: symbol.kind,
                scope: binding_scope(&file, &symbol),
                span: symbol.span,
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|record| record.span.start);

        let previous = self.binding_records.remove(uri).unwrap_or_default();
        let mut used = vec![false; previous.len()];
        for candidate in &mut candidates {
            let best = previous
                .iter()
                .enumerate()
                .filter(|(index, record)| {
                    !used[*index]
                        && record.name == candidate.name
                        && record.kind == candidate.kind
                        && record.scope == candidate.scope
                })
                .min_by_key(|(_, record)| {
                    u64::from(record.span.start.abs_diff(candidate.span.start))
                })
                .map(|(index, record)| (index, record.id));
            if let Some((index, id)) = best {
                used[index] = true;
                candidate.id = id;
            } else {
                candidate.id = self.next_binding_id;
                self.next_binding_id = self.next_binding_id.saturating_add(1);
            }
        }
        self.binding_records.insert(uri.to_owned(), candidates);
    }

    fn binding_id(&self, uri: &str, symbol: &Symbol) -> String {
        self.binding_records
            .get(uri)
            .and_then(|records| {
                records.iter().find(|record| {
                    record.name == symbol.name
                        && record.kind == symbol.kind
                        && record.span == symbol.span
                })
            })
            .map(|record| format!("binding:{}", record.id))
            .unwrap_or_else(|| {
                format!(
                    "synthetic:{}:{}:{}:{}",
                    uri, symbol.name, symbol.kind, symbol.span.start
                )
            })
    }

    fn document_text(&self, uri: &str) -> Option<&str> {
        self.documents.get(uri)?.get("text")?.as_str()
    }

    fn local_symbols_for_document(&self, uri: &str, source: &str, file: &File) -> Vec<Symbol> {
        let mut symbols = local_symbols(source, file);
        for symbol in &mut symbols {
            if symbol.detail != symbol.name {
                continue;
            }
            symbol.detail = self
                .expression_type(uri, symbol.span, symbol.range)
                .map(|ty| format!("{}: {}", symbol.name, ty.display()))
                .unwrap_or_else(|| symbol.name.clone());
        }
        symbols
    }

    fn expression_type(
        &self,
        uri: &str,
        span: Span,
        declaration_span: Span,
    ) -> Option<aura_sema::Ty> {
        let (expression_types, base) = self.expression_types_for_document(uri)?;
        let package_span = span.shift(base);
        let package_declaration_span = declaration_span.shift(base);
        let direct = expression_types
            .get(&(package_span.start, package_span.end))
            .cloned();
        let declaration_type = direct.is_none().then(|| {
            expression_types
                .iter()
                .filter(|((start, end), _)| {
                    *start >= package_declaration_span.start && *end <= package_declaration_span.end
                })
                .max_by_key(|((start, end), _)| end - start)
                .map(|(_, ty)| ty.clone())
        });
        let declaration_type = declaration_type.flatten();
        let initializer_type = if direct.is_none() && declaration_type.is_none() {
            self.initializer_type(uri, declaration_span, &expression_types, base)
        } else {
            None
        };
        direct.or(declaration_type).or(initializer_type)
    }

    fn expression_types_for_document(&self, uri: &str) -> Option<(ExpressionTypes, u32)> {
        if let Some(target_path) = uri_to_path(uri) {
            if let Some(manifest) = manifest_for(&target_path) {
                let revision = self.host.snapshot().id().get();
                if let Some(cached) = self.package_expression_cache.borrow().get(&manifest) {
                    if cached.revision == revision {
                        let expression_types = cached.expression_types.as_ref()?.clone();
                        let base = *cached.source_bases.get(&target_path)?;
                        return Some((expression_types, base));
                    }
                }
                let overlays = self
                    .documents
                    .iter()
                    .filter_map(|(document_uri, document)| {
                        let path = uri_to_path(document_uri)?;
                        let text = document.get("text")?.as_str()?.to_owned();
                        Some((path, text))
                    })
                    .collect::<HashMap<_, _>>();
                let package = match self.std_root.as_deref().map_or_else(
                    || load_package_read_only(&manifest),
                    |std_root| load_package_read_only_with_std(&manifest, std_root),
                ) {
                    Ok(package) => package,
                    Err(error) => {
                        self.log_analysis_failure(uri, "package-load", error);
                        self.store_package_expression_cache(manifest, revision, None);
                        return None;
                    }
                };
                let package = match package.with_overlays(&overlays) {
                    Ok(package) => package,
                    Err(error) => {
                        self.log_analysis_failure(uri, "package-overlay", error);
                        self.store_package_expression_cache(manifest, revision, None);
                        return None;
                    }
                };
                let source_bases = package
                    .sources
                    .iter()
                    .map(|source| (source.path.clone(), source.base))
                    .collect::<HashMap<_, _>>();
                let Some(base) = package
                    .sources
                    .iter()
                    .find(|source| source.path == target_path)
                    .map(|source| source.base)
                else {
                    self.log_analysis_failure(
                        uri,
                        "source-map",
                        "target document is missing from the loaded package",
                    );
                    self.store_package_expression_cache(manifest, revision, None);
                    return None;
                };
                let checked = match check_file(&package.ast) {
                    Ok(checked) => checked,
                    Err(error) => {
                        self.log_analysis_failure(uri, "package-check", error);
                        self.store_package_expression_cache(manifest, revision, None);
                        return None;
                    }
                };
                let expression_types = Arc::new(checked.expr_tys);
                self.store_package_expression_cache(
                    manifest,
                    revision,
                    Some((expression_types.clone(), source_bases)),
                );
                return Some((expression_types, base));
            }
        }

        let analysis = match self.host.snapshot().analyze(&DocumentId::from(uri)) {
            Ok(analysis) => analysis,
            Err(error) => {
                self.log_analysis_failure(uri, "document-check", error);
                return None;
            }
        };
        Some((Arc::new(analysis.checked.expr_tys.clone()), 0))
    }

    fn store_package_expression_cache(
        &self,
        manifest: PathBuf,
        revision: u64,
        result: Option<PackageExpressionResult>,
    ) {
        let (expression_types, source_bases) = result
            .map(|(expression_types, source_bases)| (Some(expression_types), source_bases))
            .unwrap_or_else(|| (None, HashMap::new()));
        let mut cache = self.package_expression_cache.borrow_mut();
        if cache.len() >= 32 && !cache.contains_key(&manifest) {
            cache.clear();
        }
        cache.insert(
            manifest,
            PackageExpressionCache {
                revision,
                expression_types,
                source_bases,
            },
        );
    }

    fn log_analysis_failure(&self, uri: &str, stage: &str, error: impl std::fmt::Display) {
        let version = self
            .documents
            .get(uri)
            .and_then(|document| document.get("version"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let key = format!("{uri}@{version}:{stage}");
        let mut reported = self.reported_analysis_failures.borrow_mut();
        if reported.len() >= 512 {
            reported.clear();
        }
        if !reported.insert(key) {
            return;
        }
        drop(reported);

        let rendered = error.to_string();
        let (error_count, first_error) = analysis_error_summary(&rendered);
        eprintln!(
            "[auralsp:analysis] failed uri={uri} version={version} stage={stage} error_count={error_count} first={first_error:?}"
        );
    }

    fn initializer_type(
        &self,
        uri: &str,
        declaration_span: Span,
        expression_types: &std::collections::HashMap<(u32, u32), aura_sema::Ty>,
        base: u32,
    ) -> Option<aura_sema::Ty> {
        let source = self.document_text(uri)?;
        let start = declaration_span.end as usize;
        // Package analysis may contain virtual-buffer spans; never slice the
        // current document with an offset from that larger buffer.
        if start > source.len() || !source.is_char_boundary(start) {
            return None;
        }
        let line_end = source[start..]
            .find('\n')
            .map(|offset| start + offset)
            .unwrap_or(source.len());
        let equals = source[start..line_end].find('=')? + start + 1;
        let package_equals = equals as u32 + base;
        let package_line_end = line_end as u32 + base;
        expression_types
            .iter()
            .filter(|((expr_start, expr_end), _)| {
                *expr_start >= package_equals && *expr_end <= package_line_end
            })
            .max_by_key(|((expr_start, expr_end), _)| expr_end - expr_start)
            .map(|(_, ty)| ty.clone())
    }

    fn local_initializer_type_name(&self, uri: &str, declaration_span: Span) -> Option<String> {
        let source = self.document_text(uri)?;
        let declaration =
            source.get(declaration_span.start as usize..declaration_span.end as usize)?;
        if let Some(type_name) =
            self.local_method_call_return_type(source, declaration_span, declaration)
        {
            return Some(type_name);
        }
        let initializer = declaration.split_once('=')?.1.trim_start();
        let end = initializer
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(initializer.len());
        let name = &initializer[..end];
        let (_, symbol) = self.find_declaration_symbol(name)?;
        match symbol.kind {
            5 => Some(symbol.name),
            12 => self.find_function_return_type_name(name),
            _ => None,
        }
    }

    fn local_method_call_return_type(
        &self,
        source: &str,
        declaration_span: Span,
        declaration: &str,
    ) -> Option<String> {
        let (_, file) = editor::parse_editor_file(source, source.len())?;
        let initializer = declaration.split_once('=')?.1.trim();
        let call_head = initializer.split_once('(')?.0.trim_end();
        let dot = call_head.rfind('.')?;
        let receiver_text = call_head[..dot].trim_end();
        let method_name = call_head[dot + 1..].trim();
        let receiver_start = declaration_span.start as usize + declaration.find(receiver_text)?;
        let receiver_span = Span::new(
            receiver_start as u32,
            (receiver_start + receiver_text.len()) as u32,
        );
        let receiver_name =
            receiver_field_type_name(source, &file, receiver_span, declaration_span.start)?;
        if let Some(class) = file.classes.iter().find(|class| {
            nominal_short_name(&class.name.name) == nominal_short_name(&receiver_name)
        }) {
            if let Some(method) = class
                .methods
                .iter()
                .find(|method| method.name.name == method_name)
            {
                return method
                    .return_type
                    .as_ref()
                    .map(|ty| source_span(source, ty.span));
            }
        }
        for document in self.documents.values() {
            let Some(other_source) = document.get("text").and_then(Value::as_str) else {
                continue;
            };
            let Ok(other_file) = parse_file(other_source) else {
                continue;
            };
            let Some(class) = other_file.classes.iter().find(|class| {
                nominal_short_name(&class.name.name) == nominal_short_name(&receiver_name)
            }) else {
                continue;
            };
            let Some(method) = class
                .methods
                .iter()
                .find(|method| method.name.name == method_name)
            else {
                continue;
            };
            return method
                .return_type
                .as_ref()
                .map(|ty| source_span(other_source, ty.span));
        }
        None
    }
}

fn analysis_error_summary(error: &str) -> (usize, String) {
    let mut errors = error.lines().map(str::trim).filter(|line| !line.is_empty());
    let first = errors
        .next()
        .unwrap_or("unknown analysis failure")
        .to_owned();
    (1 + errors.count(), first)
}

const KEYWORDS: &[&str] = &[
    "package",
    "import",
    "pub",
    "fun",
    "async",
    "class",
    "struct",
    "interface",
    "enum",
    "type",
    "const",
    "val",
    "var",
    "if",
    "else",
    "while",
    "for",
    "in",
    "match",
    "case",
    "return",
    "throw",
    "try",
    "catch",
    "finally",
    "true",
    "false",
    "await",
    "spawn",
    "join",
];

#[derive(Debug, Clone)]
struct Symbol {
    name: String,
    kind: u32,
    detail: String,
    span: Span,
    range: Span,
    children: Vec<Symbol>,
}

fn collect_symbols(source: &str, file: &File) -> Vec<Value> {
    declaration_symbols(source, file)
        .into_iter()
        .map(|symbol| symbol_json(source, &symbol))
        .collect()
}

fn declaration_symbols(source: &str, file: &File) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    symbols.extend(
        file.functions
            .iter()
            .map(|function| function_symbol(source, function)),
    );
    symbols.extend(file.async_functions.iter().map(|function| Symbol {
        name: function.name.name.clone(),
        kind: 12,
        detail: source_span(source, function.span),
        span: function.name.span,
        range: function.span,
        children: Vec::new(),
    }));
    symbols.extend(file.foreign_functions.iter().map(|function| Symbol {
        name: function.name.name.clone(),
        kind: 12,
        detail: source_span(source, function.span),
        span: function.name.span,
        range: function.span,
        children: Vec::new(),
    }));
    symbols.extend(file.classes.iter().map(|class| {
        Symbol {
            name: class.name.name.clone(),
            kind: 5,
            detail: source_span(source, class.span),
            span: class.name.span,
            range: class.span,
            children: class
                .fields
                .iter()
                .map(|field| Symbol {
                    name: field.name.name.clone(),
                    kind: 8,
                    detail: source_span(source, field.span),
                    span: field.name.span,
                    range: field.span,
                    children: Vec::new(),
                })
                .chain(
                    class
                        .methods
                        .iter()
                        .map(|method| function_symbol(source, method)),
                )
                .collect(),
        }
    }));
    symbols.extend(file.interfaces.iter().map(|interface| {
        Symbol {
            name: interface.name.name.clone(),
            kind: 11,
            detail: source_span(source, interface.span),
            span: interface.name.span,
            range: interface.span,
            children: interface
                .methods
                .iter()
                .map(|method| Symbol {
                    name: method.name.name.clone(),
                    kind: 6,
                    detail: source_span(source, method.span),
                    span: method.name.span,
                    range: method.span,
                    children: Vec::new(),
                })
                .collect(),
        }
    }));
    symbols.extend(file.enums.iter().map(|enumeration| {
        Symbol {
            name: enumeration.name.name.clone(),
            kind: 10,
            detail: source_span(source, enumeration.span),
            span: enumeration.name.span,
            range: enumeration.span,
            children: enumeration
                .variants
                .iter()
                .map(|variant| Symbol {
                    name: variant.name.name.clone(),
                    kind: 22,
                    detail: source_span(source, variant.span),
                    span: variant.name.span,
                    range: variant.span,
                    children: Vec::new(),
                })
                .collect(),
        }
    }));
    symbols.extend(file.type_aliases.iter().map(|alias| Symbol {
        name: alias.name.name.clone(),
        kind: 26,
        detail: source_span(source, alias.span),
        span: alias.name.span,
        range: alias.span,
        children: Vec::new(),
    }));
    symbols.extend(file.consts.iter().map(|constant| Symbol {
        name: constant.name.name.clone(),
        kind: 14,
        detail: source_span(source, constant.span),
        span: constant.name.span,
        range: constant.span,
        children: Vec::new(),
    }));
    symbols.sort_by_key(|symbol| symbol.span.start);
    symbols
}

fn local_symbols(source: &str, file: &File) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    for function in &file.functions {
        collect_function_locals(source, &function.params, &function.body, &mut symbols);
    }
    for function in &file.async_functions {
        collect_function_locals(source, &function.params, &function.body, &mut symbols);
    }
    for class in &file.classes {
        for method in &class.methods {
            collect_function_locals(source, &method.params, &method.body, &mut symbols);
        }
    }
    symbols.sort_by_key(|symbol| symbol.span.start);
    symbols
}

fn enclosing_callable_span(file: &File, offset: u32) -> Option<Span> {
    file.functions
        .iter()
        .map(|function| function.span)
        .chain(file.async_functions.iter().map(|function| function.span))
        .chain(
            file.classes
                .iter()
                .flat_map(|class| class.methods.iter().map(|method| method.span)),
        )
        .filter(|span| span.start <= offset && offset <= span.end)
        .min_by_key(|span| span.end.saturating_sub(span.start))
}

fn collect_function_locals(
    source: &str,
    params: &[aura_ast::Param],
    body: &aura_ast::Block,
    symbols: &mut Vec<Symbol>,
) {
    for param in params {
        symbols.push(Symbol {
            name: param.name.name.clone(),
            kind: 13,
            detail: format!(
                "{}: {}",
                param.name.name,
                source_span(source, param.ty.span)
            ),
            span: param.name.span,
            range: param.span,
            children: Vec::new(),
        });
    }
    collect_block_locals(source, body, symbols);
}

fn collect_block_locals(source: &str, block: &aura_ast::Block, symbols: &mut Vec<Symbol>) {
    for statement in &block.stmts {
        match statement {
            aura_ast::Stmt::Var(variable) => {
                symbols.push(Symbol {
                    name: variable.name.name.clone(),
                    kind: 13,
                    detail: variable
                        .ty
                        .as_ref()
                        .map(|ty| {
                            format!("{}: {}", variable.name.name, source_span(source, ty.span))
                        })
                        .unwrap_or_else(|| variable.name.name.clone()),
                    span: variable.name.span,
                    range: variable.span,
                    children: Vec::new(),
                });
            }
            aura_ast::Stmt::If(if_stmt) => {
                collect_block_locals(source, &if_stmt.then_block, symbols);
                if let Some(block) = &if_stmt.else_block {
                    collect_block_locals(source, block, symbols);
                }
            }
            aura_ast::Stmt::While(while_stmt) => {
                collect_block_locals(source, &while_stmt.body, symbols);
            }
            aura_ast::Stmt::ForRange(for_stmt) => {
                symbols.push(loop_symbol(&for_stmt.name, for_stmt.span));
                collect_block_locals(source, &for_stmt.body, symbols);
            }
            aura_ast::Stmt::ForIn(for_stmt) => {
                symbols.push(loop_symbol(&for_stmt.name, for_stmt.span));
                collect_block_locals(source, &for_stmt.body, symbols);
            }
            aura_ast::Stmt::Match(match_stmt) => {
                for arm in &match_stmt.arms {
                    let aura_ast::Pattern::Variant { bindings, .. } = &arm.pattern;
                    for binding in bindings {
                        symbols.push(loop_symbol(binding, arm.span));
                    }
                    collect_block_locals(source, &arm.body, symbols);
                }
            }
            aura_ast::Stmt::Try(try_stmt) => {
                collect_block_locals(source, &try_stmt.try_block, symbols);
                if let Some(catch) = &try_stmt.catch {
                    symbols.push(loop_symbol(&catch.name, catch.span));
                    collect_block_locals(source, &catch.body, symbols);
                }
                if let Some(block) = &try_stmt.finally {
                    collect_block_locals(source, block, symbols);
                }
            }
            _ => {}
        }
    }
}

fn loop_symbol(name: &aura_ast::Ident, span: Span) -> Symbol {
    Symbol {
        name: name.name.clone(),
        kind: 13,
        detail: format!("{} (local)", name.name),
        span: name.span,
        range: span,
        children: Vec::new(),
    }
}

fn function_symbol(source: &str, function: &FunDecl) -> Symbol {
    Symbol {
        name: function.name.name.clone(),
        kind: 12,
        detail: source_span(source, function.span),
        span: function.name.span,
        range: function.span,
        children: Vec::new(),
    }
}

fn symbol_json(source: &str, symbol: &Symbol) -> Value {
    let mut value = json!({"name":symbol.name,"kind":symbol.kind,"range":span_range(source, symbol.range),"selectionRange":span_range(source, symbol.span),"detail":symbol.detail});
    if !symbol.children.is_empty() {
        value["children"] = json!(symbol
            .children
            .iter()
            .map(|child| symbol_json(source, child))
            .collect::<Vec<_>>());
    }
    value
}

fn source_span(source: &str, span: Span) -> String {
    let start = span.start as usize;
    let end = (span.end as usize).min(source.len());
    source.get(start..end).unwrap_or("").trim().to_owned()
}

fn compact_symbol_detail(detail: &str) -> String {
    let detail = detail.trim();
    let end = detail
        .find('{')
        .or_else(|| detail.find(" = "))
        .unwrap_or(detail.len());
    detail[..end].trim().to_owned()
}

fn documentation_before(source: &str, declaration_start: usize) -> String {
    let declaration_start = declaration_start.min(source.len());
    let line_start = source[..declaration_start]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let prefix = &source[..line_start];
    let lines = prefix.lines().rev();
    let mut comments = Vec::new();
    for line in lines {
        let trimmed = line.trim_start();
        let Some(comment) = trimmed
            .strip_prefix("///")
            .or_else(|| trimmed.strip_prefix("//"))
        else {
            break;
        };
        comments.push(comment.trim().to_owned());
    }
    comments.reverse();
    comments.join("\n")
}

fn format_function_signature(name: &str, params: &[aura_sema::Ty], ret: &aura_sema::Ty) -> String {
    let params = params
        .iter()
        .map(aura_sema::Ty::display)
        .collect::<Vec<_>>()
        .join(", ");
    format!("fun {name}({params}): {}", ret.display())
}

fn call_argument_count(source: &str, offset: usize) -> Option<usize> {
    let tokens = lex(source).ok()?;
    let open = tokens.iter().position(|token| {
        token.span.start as usize >= offset && matches!(token.kind, TokenKind::LParen)
    })?;
    let mut depth = 1usize;
    let mut count = 0usize;
    let mut has_argument = false;
    for token in tokens.into_iter().skip(open + 1) {
        match token.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(if has_argument { count + 1 } else { 0 });
                }
            }
            TokenKind::RBracket | TokenKind::RBrace => depth = depth.saturating_sub(1),
            TokenKind::Comma if depth == 1 => count += 1,
            _ if depth == 1 => has_argument = true,
            _ => {}
        }
    }
    None
}

fn function_parameter_count(detail: &str) -> Option<usize> {
    call_argument_count(detail, 0)
}

fn did_you_mean_name(message: &str) -> Option<&str> {
    let suffix = message.strip_prefix("undefined name")?;
    let marker = "did you mean `";
    let start = suffix.find(marker)? + marker.len();
    let rest = &suffix[start..];
    let end = rest.find('`')?;
    let name = &rest[..end];
    valid_identifier(name).then_some(name)
}

fn uri_param(params: Option<&Value>) -> Option<&str> {
    params?.get("textDocument")?.get("uri")?.as_str()
}

fn request_id_key(id: &Value) -> String {
    serde_json::to_string(id).unwrap_or_else(|_| "null".to_owned())
}

fn position_to_offset(source: &str, position: &Value) -> usize {
    let target_line = position.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
    let target_character = position
        .get("character")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut offset = 0;
    for (line, line_text) in source.split_inclusive('\n').enumerate() {
        if line == target_line {
            return offset
                + utf16_column_to_byte(line_text.trim_end_matches(['\r', '\n']), target_character);
        }
        offset += line_text.len();
    }
    source.len()
}

fn apply_content_changes(source: &str, changes: &[Value]) -> Option<String> {
    let mut result = source.to_owned();
    for change in changes {
        let text = change.get("text")?.as_str()?;
        let Some(range) = change.get("range") else {
            result = text.to_owned();
            continue;
        };
        let start = checked_position_to_offset(&result, range.get("start")?)?;
        let end = checked_position_to_offset(&result, range.get("end")?)?;
        if start > end {
            return None;
        }
        result.replace_range(start..end, text);
    }
    Some(result)
}

fn checked_position_to_offset(source: &str, position: &Value) -> Option<usize> {
    let line = position.get("line")?.as_u64()? as usize;
    let character = position.get("character")?.as_u64()? as usize;
    let line_text = source.split('\n').nth(line)?.trim_end_matches('\r');
    if character > line_text.encode_utf16().count() {
        return None;
    }
    Some(position_to_offset(source, position))
}

fn manifest_for(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .map(|directory| directory.join("aura.toml"))
        .find(|manifest| manifest.is_file())
}

fn workspace_roots(params: Option<&Value>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(uri) = params
        .and_then(|p| p.get("rootUri"))
        .and_then(Value::as_str)
    {
        if let Some(path) = uri_to_path(uri) {
            roots.push(path);
        }
    }
    if let Some(folders) = params
        .and_then(|p| p.get("workspaceFolders"))
        .and_then(Value::as_array)
    {
        for folder in folders {
            if let Some(uri) = folder.get("uri").and_then(Value::as_str) {
                if let Some(path) = uri_to_path(uri) {
                    roots.push(path);
                }
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

fn collect_aura_files(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') || name == "target" {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_aura_files(&entry_path, files);
        } else if file_type.is_file()
            && entry_path.extension().and_then(|ext| ext.to_str()) == Some("aura")
        {
            files.push(entry_path);
        }
    }
    files.sort();
}

fn path_to_uri(path: &Path) -> String {
    let path = path.to_string_lossy();
    let mut uri = String::from("file://");
    for byte in path.as_bytes() {
        let byte = *byte;
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b':') {
            uri.push(byte as char);
        } else {
            uri.push('%');
            uri.push(
                char::from_digit((byte >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            uri.push(
                char::from_digit((byte & 0x0f) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
    uri
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let mut bytes = Vec::with_capacity(encoded.len());
    let mut chars = encoded.as_bytes().iter().copied();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            let high = hex_value(chars.next()?)?;
            let low = hex_value(chars.next()?)?;
            bytes.push(high * 16 + low);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn utf16_column_to_byte(line: &str, target: usize) -> usize {
    let mut units = 0;
    for (index, character) in line.char_indices() {
        if units >= target {
            return index;
        }
        units += character.len_utf16();
        if units > target {
            return index + character.len_utf8();
        }
    }
    line.len()
}

fn word_start(source: &str, offset: usize) -> usize {
    let mut start = offset.min(source.len());
    while start > 0 {
        let Some(character) = source[..start].chars().next_back() else {
            break;
        };
        if !(character.is_ascii_alphanumeric() || character == '_') {
            break;
        }
        start -= character.len_utf8();
    }
    start
}

fn import_completion_context(source: &str, offset: usize) -> Option<(usize, &str)> {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..offset];
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("import ")?;
    if rest.contains(" as ") {
        return None;
    }
    let prefix_start = line_start + line.len() - rest.len();
    Some((prefix_start, rest.trim()))
}

fn receiver_span(source: &str, member_start: usize) -> Option<Span> {
    let (dot, character) = source[..member_start].char_indices().next_back()?;
    if character != '.' {
        return None;
    }
    // Keep `?.` out of the receiver span while completion is requested after
    // its dot.
    let receiver_end = if dot > 0 && source.as_bytes().get(dot - 1) == Some(&b'?') {
        dot - 1
    } else {
        dot
    };
    let mut receiver_start = word_start(source, receiver_end);
    while receiver_start > 0 {
        let separator_len = if receiver_start >= 2
            && source.as_bytes().get(receiver_start - 2..receiver_start) == Some(b"?.")
        {
            2
        } else if source.as_bytes().get(receiver_start - 1) == Some(&b'.') {
            1
        } else {
            break;
        };
        let previous_end = receiver_start - separator_len;
        let previous_start = word_start(source, previous_end);
        if previous_start == previous_end {
            break;
        }
        receiver_start = previous_start;
    }
    (receiver_start < receiver_end).then_some(Span::new(receiver_start as u32, receiver_end as u32))
}

fn nominal_type_name(ty: &aura_sema::Ty) -> Option<&str> {
    match ty {
        aura_sema::Ty::Nullable(inner) => nominal_type_name(inner),
        _ => ty.class_name().or_else(|| ty.iface_name()),
    }
}

fn receiver_field_type_name(
    source: &str,
    file: &File,
    receiver_span: Span,
    member_offset: u32,
) -> Option<String> {
    let receiver = source.get(receiver_span.start as usize..receiver_span.end as usize)?;
    let fields = receiver.strip_prefix("this.")?.split('.');
    let class = file
        .classes
        .iter()
        .find(|class| class.span.start <= member_offset && member_offset <= class.span.end)?;
    let mut current_class = class;
    let mut current_type = None;
    let mut fields = fields.peekable();
    while let Some(field_name) = fields.next() {
        let field = current_class
            .fields
            .iter()
            .find(|field| field.name.name == field_name)?;
        let next_type = source
            .get(field.ty.span.start as usize..field.ty.span.end as usize)
            .map(str::to_owned)
            .unwrap_or_else(|| field.ty.name.name.clone());
        current_type = Some(next_type.clone());
        if fields.peek().is_some() {
            current_class = file.classes.iter().find(|candidate| {
                nominal_short_name(&candidate.name.name) == nominal_short_name(&next_type)
            })?;
        }
    }
    current_type
}

fn nominal_short_name(name: &str) -> &str {
    name.split('@')
        .next()
        .unwrap_or(name)
        .rsplit('.')
        .next()
        .unwrap_or(name)
}

fn word_at(source: &str, offset: usize) -> Option<String> {
    let span = word_span_at(source, offset)?;
    Some(source[span.start as usize..span.end as usize].to_owned())
}

fn word_span_at(source: &str, offset: usize) -> Option<Span> {
    let offset = offset.min(source.len());
    let start = word_start(source, offset);
    let mut end = offset;
    while end < source.len() {
        let character = source[end..].chars().next()?;
        if !(character.is_ascii_alphanumeric() || character == '_') {
            break;
        }
        end += character.len_utf8();
    }
    (start < end).then_some(Span::new(start as u32, end as u32))
}

fn qualifier_before(source: &str, word_span: Span) -> Option<&str> {
    let before = source.get(..word_span.start as usize)?.strip_suffix('.')?;
    let span = word_span_at(before, before.len())?;
    before.get(span.start as usize..span.end as usize)
}

fn imported_package_for_alias(file: &File, alias: &str) -> Option<String> {
    file.imports
        .iter()
        .find_map(|import| (import.alias.as_ref()?.name == alias).then(|| import.path.display()))
}

fn word_occurrences(source: &str, name: &str) -> Vec<Span> {
    lex(source)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|token| match token.kind {
            TokenKind::Ident(identifier) if identifier == name => Some(token.span),
            _ => None,
        })
        .collect()
}

fn declaration_at(source: &str, name: &str, span: Span) -> bool {
    let before = &source[..span.start as usize];
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let line = source[line_start..].trim_start();
    line.starts_with("fun ")
        || line.starts_with("async fun ")
        || line.starts_with("class ")
        || line.starts_with("struct ")
        || line.starts_with("interface ")
        || line.starts_with("enum ")
        || line.starts_with("type ")
        || line.starts_with("const ")
        || line.contains(&format!(" fun {name}"))
}

fn binding_scope(file: &File, symbol: &Symbol) -> String {
    let offset = symbol.span.start;
    let mut best: Option<(u32, String)> = None;
    let mut consider = |span: Span, scope: String| {
        if span.start <= offset && offset <= span.end {
            let width = span.end.saturating_sub(span.start);
            if best.as_ref().is_none_or(|(current, _)| width < *current) {
                best = Some((width, scope));
            }
        }
    };
    for function in &file.functions {
        consider(function.span, format!("fun:{}", function.name.name));
    }
    for function in &file.async_functions {
        consider(function.span, format!("async:{}", function.name.name));
    }
    for function in &file.foreign_functions {
        consider(function.span, format!("foreign:{}", function.name.name));
    }
    for class in &file.classes {
        consider(class.span, format!("class:{}", class.name.name));
        for method in &class.methods {
            consider(
                method.span,
                format!("method:{}::{}", class.name.name, method.name.name),
            );
        }
    }
    best.map(|(_, scope)| scope)
        .unwrap_or_else(|| "module".to_owned())
}

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(character) if character.is_ascii_alphabetic() || character == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn flatten_symbols(symbols: &[Symbol]) -> Vec<&Symbol> {
    let mut flattened = Vec::new();
    for symbol in symbols {
        flattened.push(symbol);
        flattened.extend(flatten_symbols(&symbol.children));
    }
    flattened
}

fn diagnostic_json(source: &str, diagnostic: &Diagnostic) -> Value {
    let mut value = json!({
        "range": span_range(source, diagnostic.span),
        "severity": match diagnostic.severity { Severity::Error => 1, Severity::Warning => 2, Severity::Info => 3, Severity::Help => 4 },
        "source": "aura",
        "message": diagnostic.message
    });
    if let Some(code) = diagnostic_code(&diagnostic.message) {
        value["code"] = Value::String(code.to_owned());
    }
    if let Some(new_name) = did_you_mean_name(&diagnostic.message) {
        value["data"] = json!({
            "suggestions": [{
                "newText": new_name,
                "range": span_range(source, diagnostic.span)
            }]
        });
    }
    value
}

fn diagnostic_code(message: &str) -> Option<&str> {
    let rest = message.strip_prefix('[')?;
    let end = rest.find(']')?;
    let code = &rest[..end];
    (!code.is_empty()
        && code
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_')))
    .then_some(code)
}

fn span_range(source: &str, span: Span) -> Value {
    json!({"start": position(source, span.start as usize), "end": position(source, span.end as usize)})
}

fn position(source: &str, offset: usize) -> Value {
    let bounded = offset.min(source.len());
    let bounded = (0..=bounded)
        .rev()
        .find(|index| source.is_char_boundary(*index))
        .unwrap_or(0);
    let prefix = &source[..bounded];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, tail)| tail)
        .encode_utf16()
        .count();
    json!({"line":line,"character":column})
}

fn full_document_range(source: &str) -> Value {
    json!({
        "start": {"line": 0, "character": 0},
        "end": position(source, source.len())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        analysis_error_summary, diagnostic_code, diagnostic_json, path_to_uri, position_to_offset,
        word_span_at, Server,
    };
    use aura_analysis::{Diagnostic, Severity};
    use aura_ast::Span;
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn hover_contents_text(response: &serde_json::Value) -> String {
        response["result"]["contents"]
            .as_array()
            .expect("hover contents array")
            .iter()
            .filter_map(|content| {
                content
                    .as_str()
                    .or_else(|| content.get("value").and_then(serde_json::Value::as_str))
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[test]
    fn analysis_error_summary_counts_errors_without_repeating_the_log() {
        assert_eq!(
            analysis_error_summary("undefined `a`\nundefined `b`\nundefined `c`"),
            (3, "undefined `a`".to_owned())
        );
        assert_eq!(
            analysis_error_summary(""),
            (1, "unknown analysis failure".to_owned())
        );
    }

    fn workspace_path(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative)
    }

    fn open_workspace_document(
        root: &Path,
        relative: &str,
        overlay: Option<String>,
    ) -> serde_json::Value {
        let path = root.join(relative);
        let text = overlay.unwrap_or_else(|| fs::read_to_string(&path).expect("workspace source"));
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_to_uri(root),"capabilities":{}}}));
        server
            .handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":path_to_uri(&path),"version":1,"text":text}}}))
            .expect("diagnostic notification")
    }

    #[test]
    fn package_diagnostics_resolve_std_io() {
        let root = workspace_path("corpus/std_io/exit");
        let notification = open_workspace_document(&root, "src/main.aura", None);
        assert!(notification["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn aura_toolchain_std_drives_diagnostics_definition_and_docs() {
        let root =
            std::env::temp_dir().join(format!("aura-lsp-toolchain-std-{}", std::process::id()));
        let workspace = root.join("workspace");
        let std_root = root.join("toolchain/share/aura/std");
        let main_path = workspace.join("src/main.aura");
        let error_path = std_root.join("error/src/lib.aura");
        fs::create_dir_all(main_path.parent().unwrap()).unwrap();
        fs::create_dir_all(error_path.parent().unwrap()).unwrap();
        fs::create_dir_all(std_root.join("io/src")).unwrap();
        fs::write(
            workspace.join("aura.toml"),
            "[package]\nname = \"demo\"\n\n[[bin]]\nname = \"demo\"\npath = \"src\"\n",
        )
        .unwrap();
        fs::write(
            &main_path,
            "package demo\nimport std.error as Errors\npub fun main(): Errors.Error { return Errors.protocol(\"test\", 400) }\n",
        )
        .unwrap();
        fs::write(
            std_root.join("error/aura.toml"),
            "[package]\nname = \"std.error\"\n\n[[bin]]\nname = \"error\"\npath = \"src\"\n",
        )
        .unwrap();
        fs::write(
            &error_path,
            "package std.error\n/// Error payload from this Aura version.\npub class Error(pub val message: String) {}\n/// Available only in this Aura version.\npub fun protocol(message: String, code: Int): Error { return Error(message) }\n",
        )
        .unwrap();
        fs::write(
            std_root.join("io/aura.toml"),
            "[package]\nname = \"std.io\"\n\n[[bin]]\nname = \"io\"\npath = \"src\"\n",
        )
        .unwrap();
        fs::write(std_root.join("io/src/lib.aura"), "package std.io\n").unwrap();

        let source = fs::read_to_string(&main_path).unwrap();
        let uri = path_to_uri(&main_path);
        let mut server = Server::with_std_root(Some(std_root));
        server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_to_uri(&workspace),"capabilities":{}}}));
        let diagnostics = server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"text":source}}})).unwrap();
        assert!(diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty());

        let type_line = source.lines().nth(2).unwrap();
        let alias_character = type_line.find("Errors").unwrap();
        let type_character = type_line.find("Error {").unwrap();
        let protocol_character = type_line.find("protocol").unwrap();

        let alias_hover = server.handle(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":uri},"position":{"line":2,"character":alias_character}}})).unwrap();
        assert!(hover_contents_text(&alias_hover).contains("import std.error as Errors"));
        let type_hover = server.handle(json!({"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{"textDocument":{"uri":uri},"position":{"line":2,"character":type_character}}})).unwrap();
        assert!(hover_contents_text(&type_hover).contains("Error payload from this Aura version."));
        let type_definition = server.handle(json!({"jsonrpc":"2.0","id":4,"method":"textDocument/definition","params":{"textDocument":{"uri":uri},"position":{"line":2,"character":type_character}}})).unwrap();
        assert_eq!(
            type_definition["result"][0]["targetUri"],
            path_to_uri(&error_path)
        );

        let definition = server.handle(json!({"jsonrpc":"2.0","id":5,"method":"textDocument/definition","params":{"textDocument":{"uri":uri},"position":{"line":2,"character":protocol_character}}})).unwrap();
        assert_eq!(
            definition["result"][0]["targetUri"],
            path_to_uri(&error_path)
        );
        let hover = server.handle(json!({"jsonrpc":"2.0","id":6,"method":"textDocument/hover","params":{"textDocument":{"uri":uri},"position":{"line":2,"character":protocol_character}}})).unwrap();
        assert!(hover_contents_text(&hover).contains("Available only in this Aura version."));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_diagnostics_resolve_transitive_path_dependencies() {
        let root = workspace_path("corpus/import/nested_app");
        let overlay = fs::read_to_string(root.join("src/main.aura"))
            .expect("workspace source")
            .replace("println(wrap())", "println(wrap() + \"!\")");
        let notification = open_workspace_document(&root, "src/main.aura", Some(overlay));
        assert!(notification["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn publishes_diagnostics_for_open_document() {
        let mut server = Server::new();
        let response =
            server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}));
        let response = response.unwrap();
        assert_eq!(
            response["result"]["capabilities"]["textDocumentSync"]["change"],
            2
        );
        assert_eq!(
            response["result"]["capabilities"]["positionEncoding"],
            "utf-16"
        );
        let capabilities = &response["result"]["capabilities"];
        assert_eq!(capabilities["documentFormattingProvider"], true);
        assert_eq!(capabilities["documentSymbolProvider"], true);
        assert_eq!(
            capabilities["completionProvider"]["triggerCharacters"],
            json!(["."])
        );
        assert_eq!(capabilities["hoverProvider"], true);
        assert_eq!(capabilities["definitionProvider"], true);
        assert_eq!(capabilities["documentHighlightProvider"], true);
        assert_eq!(capabilities["referencesProvider"], true);
        assert_eq!(capabilities["renameProvider"], true);
        assert_eq!(capabilities["workspaceSymbolProvider"], true);
        assert_eq!(
            capabilities["codeActionProvider"]["codeActionKinds"],
            json!(["quickfix", "source.format"])
        );
        assert_eq!(
            capabilities["diagnosticProvider"]["workspaceDiagnostics"],
            false
        );
        let notification = server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///main.aura","version":1,"text":"package\n"}}})).unwrap();
        assert_eq!(notification["method"], "textDocument/publishDiagnostics");
        assert_eq!(
            notification["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn publishes_missing_return_diagnostic() {
        let mut server = Server::new();
        let source = "package demo\nclass Notebook(val items: Array<String>) {}\npub fun notebook(): Notebook {\n  var a = Notebook(Array(0))\n}\n";
        let notification = server
            .handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///main.aura","version":1,"text":source}}}))
            .expect("diagnostic notification");
        let diagnostics = notification["params"]["diagnostics"]
            .as_array()
            .expect("diagnostics array");
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic["message"].as_str().is_some_and(|message| {
                message.contains("missing return") && message.contains("Notebook")
            })
        }));
    }

    #[test]
    fn stale_change_does_not_replace_document() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":2,"text":"package demo\n"}}}));
        server.pending_notifications.clear();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"main.aura","version":1},"contentChanges":[{"text":"package\n"}]}}));
        assert_eq!(server.documents["main.aura"]["text"], "package demo\n");
    }

    #[test]
    fn cancelled_request_is_rejected_before_execution() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":42}}));
        let response = server
            .handle(
                json!({"jsonrpc":"2.0","id":42,"method":"workspace/symbol","params":{"query":""}}),
            )
            .unwrap();
        assert_eq!(response["error"]["code"], -32800);
    }

    #[test]
    fn diagnostic_codes_are_preserved_at_the_lsp_boundary() {
        assert_eq!(
            diagnostic_code("[AURA-F1-TYPE] bad foreign type"),
            Some("AURA-F1-TYPE")
        );
        assert_eq!(diagnostic_code("undefined name `x`"), None);
    }

    #[test]
    fn diagnostics_expose_precise_structured_name_suggestions() {
        let diagnostic = Diagnostic {
            severity: Severity::Error,
            message: "undefined name `mian`; did you mean `main`".into(),
            span: Span::new(4, 8),
        };
        let value = diagnostic_json("fun mian() {}", &diagnostic);
        assert_eq!(value["data"]["suggestions"][0]["newText"], "main");
        assert_eq!(
            value["data"]["suggestions"][0]["range"]["start"]["character"],
            4
        );
        assert_eq!(
            value["data"]["suggestions"][0]["range"]["end"]["character"],
            8
        );
    }

    #[test]
    fn pull_diagnostics_returns_unchanged_for_the_same_snapshot() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":4,"text":"package demo\n"}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":16,"method":"textDocument/diagnostic","params":{"textDocument":{"uri":"main.aura"}}})).unwrap();
        let result_id = response["result"]["resultId"].as_str().unwrap().to_owned();
        let unchanged = server.handle(json!({"jsonrpc":"2.0","id":17,"method":"textDocument/diagnostic","params":{"textDocument":{"uri":"main.aura"},"previousResultId":result_id}})).unwrap();
        assert_eq!(unchanged["result"]["kind"], "unchanged");
    }

    #[test]
    fn applies_incremental_utf16_changes_in_order() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":"package demo\nfun main() {}\n"}}}));
        server.pending_notifications.clear();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"main.aura","version":2},"contentChanges":[{"range":{"start":{"line":1,"character":4},"end":{"line":1,"character":8}},"text":"run"}]}}));
        assert_eq!(
            server.documents["main.aura"]["text"],
            "package demo\nfun run() {}\n"
        );
    }

    #[test]
    fn formats_open_document_with_one_whole_document_edit() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":"package demo\nfun main(){return}\n"}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{"textDocument":{"uri":"main.aura"},"options":{}}})).unwrap();
        let edits = response["result"].as_array().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0]["range"]["start"], json!({"line":0,"character":0}));
        assert_eq!(edits[0]["newText"], "package demo\nfun main() { return }\n");
    }

    #[test]
    fn formatting_invalid_source_returns_invalid_params() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":"package\n"}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{"textDocument":{"uri":"main.aura"}}})).unwrap();
        assert_eq!(response["error"]["code"], -32602);
    }

    #[test]
    fn document_symbols_and_completion_use_ast_declarations() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":"package demo\nfun main() {}\nfun helper() {}\n"}}}));
        let symbols = server.handle(json!({"jsonrpc":"2.0","id":3,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"main.aura"}}})).unwrap();
        assert_eq!(symbols["result"][0]["name"], "main");
        assert_eq!(symbols["result"][1]["name"], "helper");

        let completion = server.handle(json!({"jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":0,"character":2}}})).unwrap();
        assert!(completion["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "package"));
        let workspace = server.handle(json!({"jsonrpc":"2.0","id":8,"method":"workspace/symbol","params":{"query":"help"}})).unwrap();
        assert_eq!(workspace["result"][0]["name"], "helper");
    }

    #[test]
    fn navigation_references_and_rename_are_workspace_bounded() {
        let mut server = Server::new();
        let source = "package demo\nfun helper() {}\nfun main() { helper() }\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":7,"text":source}}}));
        let definition = server.handle(json!({"jsonrpc":"2.0","id":5,"method":"textDocument/definition","params":{"textDocument":{"uri":"main.aura"},"position":{"line":2,"character":14}}})).unwrap();
        assert_eq!(definition["result"][0]["targetRange"]["start"]["line"], 1);
        assert_eq!(
            definition["result"][0]["originSelectionRange"],
            json!({
                "start": {"line": 2, "character": 13},
                "end": {"line": 2, "character": 19}
            })
        );
        let highlights = server.handle(json!({"jsonrpc":"2.0","id":19,"method":"textDocument/documentHighlight","params":{"textDocument":{"uri":"main.aura"},"position":{"line":2,"character":14}}})).unwrap();
        assert_eq!(highlights["result"].as_array().unwrap().len(), 2);
        assert_eq!(highlights["result"][0]["kind"], 1);
        let hover = server.handle(json!({"jsonrpc":"2.0","id":10,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":2,"character":14}}})).unwrap();
        assert!(hover_contents_text(&hover).contains("fun helper()"));
        let references = server.handle(json!({"jsonrpc":"2.0","id":6,"method":"textDocument/references","params":{"textDocument":{"uri":"main.aura"},"position":{"line":2,"character":14},"context":{"includeDeclaration":true}}})).unwrap();
        assert_eq!(references["result"].as_array().unwrap().len(), 2);
        let rename = server.handle(json!({"jsonrpc":"2.0","id":7,"method":"textDocument/rename","params":{"textDocument":{"uri":"main.aura"},"position":{"line":2,"character":14},"newName":"assist"}})).unwrap();
        assert_eq!(
            rename["result"]["documentChanges"][0]["textDocument"]["version"],
            7
        );
        assert_eq!(
            rename["result"]["documentChanges"][0]["edits"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn references_and_rename_keep_same_named_bindings_in_separate_scopes() {
        let mut server = Server::new();
        let source = "package demo\nfun first(value: Int) { return value }\nfun second(value: Int) { return value }\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));

        let references = server.handle(json!({"jsonrpc":"2.0","id":1,"method":"textDocument/references","params":{"textDocument":{"uri":"main.aura"},"position":{"line":2,"character":35},"context":{"includeDeclaration":true}}})).unwrap();
        assert_eq!(references["result"].as_array().unwrap().len(), 2);
        assert!(references["result"]
            .as_array()
            .unwrap()
            .iter()
            .all(|location| { location["range"]["start"]["line"] == 2 }));

        let rename = server.handle(json!({"jsonrpc":"2.0","id":2,"method":"textDocument/rename","params":{"textDocument":{"uri":"main.aura"},"position":{"line":2,"character":35},"newName":"item"}})).unwrap();
        let edits = rename["result"]["documentChanges"][0]["edits"]
            .as_array()
            .unwrap();
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|edit| edit["range"]["start"]["line"] == 2));
    }

    #[test]
    fn binding_identity_survives_span_shifts_across_document_edits() {
        let mut server = Server::new();
        let before = "package demo\nfun main() { helper() }\nfun helper() {}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":before}}}));
        let usage_span = word_span_at(before, before.find("helper() }").unwrap()).unwrap();
        let (definition_uri, definition) = server
            .definition_symbol_at("main.aura", "helper", usage_span)
            .expect("definition");
        let original_id = server.binding_id(&definition_uri, &definition);

        let after = "package demo\n\nfun main() { helper() }\nfun helper() {}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"main.aura","version":2},"contentChanges":[{"text":after}]}}));
        let usage_span = word_span_at(after, after.find("helper() }").unwrap()).unwrap();
        let (definition_uri, definition) = server
            .definition_symbol_at("main.aura", "helper", usage_span)
            .expect("definition after edit");
        assert_eq!(server.binding_id(&definition_uri, &definition), original_id);
    }

    #[test]
    fn definition_resolves_awaited_async_function() {
        let mut server = Server::new();
        let source = r#"package http_health_aura

async fun funcA() {
}

async fun main() {
    await funcA()
}
"#;
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));

        let definition = server.handle(json!({"jsonrpc":"2.0","id":20,"method":"textDocument/definition","params":{"textDocument":{"uri":"main.aura"},"position":{"line":6,"character":12}}})).unwrap();

        assert_eq!(definition["result"][0]["targetUri"], "main.aura");
        assert_eq!(
            definition["result"][0]["targetRange"]["start"],
            json!({"line":2,"character":0})
        );
    }

    #[test]
    fn definition_does_not_resolve_builtin_member_to_same_named_user_method() {
        let root = workspace_path("examples/notes");
        let path = root.join("src/notebook.aura");
        let source = fs::read_to_string(&path).unwrap();
        let uri = path_to_uri(&path);
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_to_uri(&root)}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("return this.items.isEmpty()"))
            .unwrap();
        let character = source.lines().nth(line).unwrap().find("isEmpty").unwrap();

        let definition = server.handle(json!({"jsonrpc":"2.0","id":20,"method":"textDocument/definition","params":{"textDocument":{"uri":uri},"position":{"line":line,"character":character}}})).unwrap();

        assert_eq!(definition["result"], json!([]));
    }

    #[test]
    fn definition_resolves_member_on_the_receiver_type() {
        let source = "package demo\n\
class Notebook(val empty: Bool) {\n\
    pub fun isEmpty(): Bool { return this.empty }\n\
}\n\
fun main() {\n\
    val nb = Notebook(false)\n\
    nb.isEmpty()\n\
}\n";
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));

        let definition = server.handle(json!({"jsonrpc":"2.0","id":21,"method":"textDocument/definition","params":{"textDocument":{"uri":"main.aura"},"position":{"line":6,"character":7}}})).unwrap();

        assert_eq!(
            definition["result"][0]["targetSelectionRange"]["start"],
            json!({"line":2,"character":8})
        );
    }

    #[test]
    fn code_action_offers_safe_format_edit() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":3,"text":"package demo\nfun main(){return}\n"}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":9,"method":"textDocument/codeAction","params":{"textDocument":{"uri":"main.aura"},"range":{"start":{"line":0,"character":0},"end":{"line":1,"character":0},"context":{"diagnostics":[]}}}})).unwrap();
        assert_eq!(response["result"][0]["kind"], "source.format");
        assert_eq!(
            response["result"][0]["edit"]["documentChanges"][0]["textDocument"]["version"],
            3
        );
        let quickfix = server.handle(json!({"jsonrpc":"2.0","id":12,"method":"textDocument/codeAction","params":{"textDocument":{"uri":"main.aura"},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"context":{"diagnostics":[{"range":{"start":{"line":1,"character":9},"end":{"line":1,"character":13}},"message":"undefined name `mian`; did you mean `main`"}]}}})).unwrap();
        let quickfix = quickfix["result"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["kind"] == "quickfix")
            .unwrap();
        assert_eq!(
            quickfix["edit"]["documentChanges"][0]["edits"][0]["newText"],
            "main"
        );
    }

    #[test]
    fn indexes_workspace_sources_and_restores_disk_text_after_close() {
        let root = std::env::temp_dir().join(format!("aura-lsp-workspace-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("lib.aura");
        std::fs::write(&path, "package demo\nfun disk() {}\n").unwrap();
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_to_uri(&root)}}));
        let uri = path_to_uri(&path);
        let symbols = server.handle(json!({"jsonrpc":"2.0","id":2,"method":"workspace/symbol","params":{"query":"disk"}})).unwrap();
        assert_eq!(symbols["result"][0]["name"], "disk");
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"text":"package demo\nfun edited() {}\n"}}}));
        server.pending_notifications.clear();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"package demo\nfun changed() {}\n"}]}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
        assert!(server.documents[&uri]["text"]
            .as_str()
            .unwrap()
            .contains("fun disk"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn utf16_positions_exclude_crlf_and_preserve_surrogate_boundaries() {
        let source = "😀\r\nfun main() {}\r\n";
        assert_eq!(
            position_to_offset(source, &json!({"line":0,"character":2})),
            4
        );
        assert_eq!(
            position_to_offset(source, &json!({"line":1,"character":4})),
            10
        );
        assert_eq!(
            position_to_offset(source, &json!({"line":1,"character":0})),
            6
        );
    }

    #[test]
    fn file_uri_escapes_uri_delimiters() {
        assert_eq!(
            path_to_uri(Path::new("/tmp/a #b%.aura")),
            "file:///tmp/a%20%23b%25.aura"
        );
    }

    #[test]
    fn completion_uses_checked_receiver_type_for_members() {
        let mut server = Server::new();
        let source = "package demo\nclass Box(val value: Int) {\n  /// Returns the stored value.\n  fun size(): Int { return value }\n}\nfun main() {\n  val box: Box = Box(1)\n  val x: Int = box.value\n}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = "  val x: Int = box.";
        let response = server.handle(json!({"jsonrpc":"2.0","id":11,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":7,"character":line.encode_utf16().count()}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "value" && item["detail"] == "value: Int"));
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item["label"] == "size"
                    && item["detail"].as_str().unwrap().contains("size")
                    && item["documentation"]["value"] == "Returns the stored value."
            }));
        assert_eq!(
            response["result"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["label"] == "value")
                .count(),
            1
        );
    }

    #[test]
    fn completion_suggests_builtin_string_and_int_members() {
        let mut server = Server::new();
        let source = "package demo\nfun main() {\n  val text: String = \"a\"\n  val number: Int = 1\n  val values: Array<Int> = Array<Int>(0)\n  text.substring(0, 1)\n  number.toString()\n  number.hash()\n  values.push(1)\n}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));

        let string_response = server.handle(json!({"jsonrpc":"2.0","id":21,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":5,"character":7}}})).unwrap();
        let string_items = string_response["result"]["items"].as_array().unwrap();
        assert!(
            string_items.iter().any(|item| item["label"] == "substring"),
            "string completion labels: {string_items:?}"
        );
        assert!(string_items
            .iter()
            .any(|item| item["label"] == "startsWith"));
        assert!(!string_items.iter().any(|item| item["label"] == "toString"));

        let int_response = server.handle(json!({"jsonrpc":"2.0","id":22,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":6,"character":9}}})).unwrap();
        let int_items = int_response["result"]["items"].as_array().unwrap();
        assert!(int_items.iter().any(|item| item["label"] == "toString"));
        let hash_response = server.handle(json!({"jsonrpc":"2.0","id":23,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":7,"character":9}}})).unwrap();
        assert!(hash_response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "hash"));
        assert!(!int_items.iter().any(|item| item["label"] == "substring"));

        let array_response = server.handle(json!({"jsonrpc":"2.0","id":24,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":8,"character":9}}})).unwrap();
        let array_items = array_response["result"]["items"].as_array().unwrap();
        assert!(array_items.iter().any(|item| item["label"] == "len"));
        let push = array_items
            .iter()
            .find(|item| item["label"] == "push")
            .unwrap();
        assert!(push["documentation"]["value"]
            .as_str()
            .unwrap()
            .contains("Appends"));
        assert!(array_items.iter().any(|item| item["label"] == "clone"));
    }

    #[test]
    fn completion_orders_members_without_leaking_keywords() {
        let mut server = Server::new();
        let source = "package demo\n\
class Request(pub val body: String, pub val bodyReader: String) {\n\
    pub fun method(): String { return this.body }\n\
}\n\
class Context(pub val request: Request) {\n\
    pub fun read(): String { return this.request. }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("this.request."))
            .unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let character = line_text.find("this.request.").unwrap() + "this.request.".len();
        let response = server.handle(json!({"jsonrpc":"2.0","id":36,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":character}}})).unwrap();
        let items = response["result"]["items"].as_array().unwrap();
        let labels = items
            .iter()
            .map(|item| item["label"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["body", "bodyReader", "method"]);
        assert!(!labels.contains(&"async"));
        assert!(items.iter().all(|item| item["sortText"].is_string()));
    }

    #[test]
    fn completion_recovers_local_primitive_receiver_before_member_name_is_complete() {
        let mut server = Server::new();
        let source = "package demo\nfun test() {\n  val a: String = \"ABC\"\n  val b = a.\n}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = source.lines().position(|line| line.contains("a.")).unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let character = line_text.find("a.").unwrap() + "a.".len();
        let response = server.handle(json!({"jsonrpc":"2.0","id":44,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "substring"));
        assert!(!response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "toString"));
    }

    #[test]
    fn completion_recovers_local_receiver_inferred_from_method_call() {
        let mut server = Server::new();
        let source = "package demo\n\
class Request() {\n\
    pub fun target(): String { return \"/\" }\n\
}\n\
class Context(pub val request: Request) {\n\
    pub fun query(): String {\n\
        val target = this.request.target()\n\
        val queryStart = target.\n\
        return target\n\
    }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("queryStart = target."))
            .unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let character = line_text.find("target.").unwrap() + "target.".len();
        let response = server.handle(json!({"jsonrpc":"2.0","id":45,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "substring"));

        let target_line = source
            .lines()
            .position(|line| line.contains("val target ="))
            .unwrap();
        let target_character = source
            .lines()
            .nth(target_line)
            .unwrap()
            .find("target")
            .unwrap();
        let hover = server.handle(json!({"jsonrpc":"2.0","id":46,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":target_line,"character":target_character}}})).unwrap();
        assert!(
            hover_contents_text(&hover).contains("target: String"),
            "hover: {hover}"
        );
    }

    #[test]
    fn completion_recovers_aura_web_imported_request_case() {
        let mut server = Server::new();
        let request_source = "package std.http\n\
pub class Request() {\n\
    /// Returns the HTTP request target.\n\
    pub fun target(): String { return \"/\" }\n\
}\n";
        let context_source = "package aura.web\n\
import std.http\n\
import std.json\n\
pub class Context(pub val request: Request) {\n\
    pub fun query(): String {\n\
        val target = this.request.target()\n\
        val queryStart = target.\n\
        return target\n\
    }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"request.aura","version":1,"text":request_source}}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"context.aura","version":1,"text":context_source}}}));
        let line = context_source
            .lines()
            .position(|line| line.contains("queryStart = target."))
            .unwrap();
        let line_text = context_source.lines().nth(line).unwrap();
        let character = line_text.find("target.").unwrap() + "target.".len();
        let response = server.handle(json!({"jsonrpc":"2.0","id":47,"method":"textDocument/completion","params":{"textDocument":{"uri":"context.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert!(
            response["result"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["label"] == "substring"),
            "completion: {response}"
        );

        let target_line = context_source
            .lines()
            .position(|line| line.contains("val target ="))
            .unwrap();
        let target_character = context_source
            .lines()
            .nth(target_line)
            .unwrap()
            .find("target")
            .unwrap();
        let hover = server.handle(json!({"jsonrpc":"2.0","id":48,"method":"textDocument/hover","params":{"textDocument":{"uri":"context.aura"},"position":{"line":target_line,"character":target_character}}})).unwrap();
        assert!(
            hover_contents_text(&hover).contains("target: String"),
            "hover: {hover}"
        );
    }

    #[test]
    fn completion_and_hover_recover_real_aura_web_context_fixture() {
        let mut server = Server::new();
        let request_source = include_str!("../../../std/http/src/lib.aura");
        let context_source = include_str!("../../../examples/aura-web/src/request/context.aura")
            .replacen(
                "val queryStart = target.indexOf(\"?\")",
                "val queryStart = target.",
                1,
            );
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"std-http.aura","version":1,"text":request_source}}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"context.aura","version":1,"text":context_source}}}));

        let line = context_source
            .lines()
            .position(|line| line.contains("val queryStart = target."))
            .unwrap();
        let line_text = context_source.lines().nth(line).unwrap();
        let character = line_text.find("target.").unwrap() + "target.".len();
        let completion = server.handle(json!({"jsonrpc":"2.0","id":49,"method":"textDocument/completion","params":{"textDocument":{"uri":"context.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert!(
            completion["result"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["label"] == "substring"),
            "completion: {completion}"
        );

        let target_line = context_source
            .lines()
            .position(|line| line.contains("val target ="))
            .unwrap();
        let target_character = context_source
            .lines()
            .nth(target_line)
            .unwrap()
            .find("target")
            .unwrap();
        let hover = server.handle(json!({"jsonrpc":"2.0","id":50,"method":"textDocument/hover","params":{"textDocument":{"uri":"context.aura"},"position":{"line":target_line,"character":target_character}}})).unwrap();
        assert!(
            hover_contents_text(&hover).contains("target: String"),
            "hover: {hover}"
        );
    }

    #[test]
    fn completion_orders_locals_before_keywords() {
        let mut server = Server::new();
        let source = "package demo\nfun main() {\n  val answer = 1\n  \n}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":37,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":3,"character":2}}})).unwrap();
        let items = response["result"]["items"].as_array().unwrap();
        let answer = items
            .iter()
            .position(|item| item["label"] == "answer")
            .unwrap();
        let async_keyword = items
            .iter()
            .position(|item| item["label"] == "async")
            .unwrap();
        assert!(answer < async_keyword);
    }

    #[test]
    fn completion_includes_function_parameters_and_locals() {
        let mut server = Server::new();
        let source = "package demo\nfun main(): Int {\n  val answer = 1\n  return answer\n}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":13,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":1,"character":0}}})).unwrap();
        let answer = response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["label"] == "answer")
            .unwrap();
        assert_eq!(answer["detail"], "answer: Int");
        assert!(!answer["detail"].as_str().unwrap().contains("inferred"));
    }

    #[test]
    fn completion_suggests_indexed_import_packages() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"lib.aura","version":1,"text":"package demo.util\nfun helper() {}\n"}}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":"package demo\nimport demo\n"}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":15,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":1,"character":11}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "demo.util"));
    }

    #[test]
    fn completion_includes_types_and_symbols_from_other_documents() {
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"lib.aura","version":1,"text":"package demo\npub class Widget() {}\npub interface Runnable {}\npub enum Status { Ready }\npub type Alias = Widget\npub const LIMIT: Int = 1\npub fun helper() {}\n"}}}));
        let source = "package demo\nfun main() {\n  hel\n}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":24,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":2,"character":5}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "helper"));

        let type_source = "package demo\nfun main() {\n  val value: Wid\n}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"type.aura","version":1,"text":type_source}}}));
        let type_response = server.handle(json!({"jsonrpc":"2.0","id":25,"method":"textDocument/completion","params":{"textDocument":{"uri":"type.aura"},"position":{"line":2,"character":15}}})).unwrap();
        assert!(type_response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "Widget"));
    }

    #[test]
    fn completion_suggests_member_declared_in_another_package_source_file() {
        let root = std::env::temp_dir().join(format!(
            "aura-lsp-completion-cross-file-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("aura.toml"), "[package]\nname = \"demo\"\n").unwrap();
        std::fs::write(
            root.join("types.aura"),
            "package demo\npub class Box(val value: Int) {}\n",
        )
        .unwrap();
        let main = root.join("main.aura");
        let source =
            "package demo\nfun main() {\n  val box: Box = Box(1)\n  val x: Int = box.value\n}\n";
        std::fs::write(&main, source).unwrap();

        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","id":26,"method":"initialize","params":{"rootUri":path_to_uri(&root)}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":path_to_uri(&main),"version":1,"text":source}}}));
        let line = "  val x: Int = box.";
        let response = server.handle(json!({"jsonrpc":"2.0","id":27,"method":"textDocument/completion","params":{"textDocument":{"uri":path_to_uri(&main)},"position":{"line":3,"character":line.encode_utf16().count()}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "value"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completion_resolves_this_field_receiver_while_member_expression_is_incomplete() {
        let mut server = Server::new();
        let source = "package demo\n\
class Request() {\n\
    pub fun target(): String { return \"/\" }\n\
}\n\
class Context(pub val request: Request) {\n\
    pub fun method(): String { return this.request. }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("this.request."))
            .unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let character =
            line_text.find("this.request.").unwrap() + "this.request.".encode_utf16().count();
        let response = server.handle(json!({"jsonrpc":"2.0","id":30,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert!(
            response["result"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["label"] == "target"),
            "completion: {}",
            response
        );
    }

    #[test]
    fn editor_features_recover_incomplete_member_expression() {
        let mut server = Server::new();
        let source = "package demo\n\
class Request() {\n\
    /// Returns the request target.\n\
    pub fun target(): String { return \"/\" }\n\
}\n\
class Context(pub val request: Request) {\n\
    pub fun method(): String { return this.request. }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("this.request."))
            .unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let request_offset = line_text.find("request").unwrap();
        let request_character = request_offset + "request".len();

        let symbols = server.handle(json!({"jsonrpc":"2.0","id":38,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":"main.aura"}}})).unwrap();
        assert!(symbols["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| symbol["name"] == "Context"));

        let hover = server.handle(json!({"jsonrpc":"2.0","id":39,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":request_character}}})).unwrap();
        assert!(
            hover["result"]["contents"].to_string().contains("request"),
            "hover: {hover}"
        );

        let definition = server.handle(json!({"jsonrpc":"2.0","id":40,"method":"textDocument/definition","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":request_character}}})).unwrap();
        assert_eq!(definition["result"].as_array().unwrap().len(), 1);

        let highlights = server.handle(json!({"jsonrpc":"2.0","id":41,"method":"textDocument/documentHighlight","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":request_character}}})).unwrap();
        assert_eq!(highlights["result"].as_array().unwrap().len(), 2);

        let references = server.handle(json!({"jsonrpc":"2.0","id":42,"method":"textDocument/references","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":request_character},"context":{"includeDeclaration":true}}})).unwrap();
        assert_eq!(references["result"].as_array().unwrap().len(), 2);

        let rename = server.handle(json!({"jsonrpc":"2.0","id":43,"method":"textDocument/rename","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":request_character},"newName":"req"}})).unwrap();
        assert_eq!(
            rename["result"]["documentChanges"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn completion_typechecks_a_recovered_nested_receiver() {
        let mut server = Server::new();
        let source = "package demo\n\
class Response() {\n\
    pub fun target(): String { return \"/\" }\n\
}\n\
class Request(pub val response: Response) {}\n\
class Context(pub val request: Request) {\n\
    pub fun method(): String { return this.request.response.ta }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("this.request.response.ta"))
            .unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let character = line_text.find("this.request.response.ta").unwrap()
            + "this.request.response.ta".encode_utf16().count();
        let response = server.handle(json!({"jsonrpc":"2.0","id":31,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "target"));
    }

    #[test]
    fn completion_recovers_generic_this_field_receiver() {
        let mut server = Server::new();
        let source = "package demo\n\
class Box<T>(val value: T) {\n\
    pub fun unwrap(): T { return this.value }\n\
}\n\
class Context(pub val box: Box<String>) {\n\
    pub fun read(): String { return this.box. }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("this.box."))
            .unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let character = line_text.find("this.box.").unwrap() + "this.box.".len();
        let response = server.handle(json!({"jsonrpc":"2.0","id":32,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "value"));
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "unwrap"));
    }

    #[test]
    fn completion_recovers_nullable_safe_receiver() {
        let mut server = Server::new();
        let source = "package demo\n\
class Request() {\n\
    pub fun target(): String { return \"/\" }\n\
}\n\
class Context(pub val request: Request?) {\n\
    pub fun method(): String { return this.request?.ta }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("this.request?.ta"))
            .unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let character = line_text.find("this.request?.ta").unwrap() + "this.request?.ta".len();
        let response = server.handle(json!({"jsonrpc":"2.0","id":33,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "target"));
    }

    #[test]
    fn completion_recovers_generic_receiver_from_another_source_file() {
        let root = std::env::temp_dir().join(format!(
            "aura-lsp-completion-generic-cross-file-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("aura.toml"), "[package]\nname = \"demo\"\n").unwrap();
        std::fs::write(
            root.join("types.aura"),
            "package demo.types\npub class Box<T>(val value: T) {\n  pub fun unwrap(): T { return this.value }\n}\n",
        )
        .unwrap();
        let main = root.join("main.aura");
        let source = "package demo\nimport demo.types as Types\nclass Context(pub val box: Types.Box<String>) {\n  pub fun read(): String { return this.box. }\n}\n";
        std::fs::write(&main, source).unwrap();

        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","id":34,"method":"initialize","params":{"rootUri":path_to_uri(&root)}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":path_to_uri(&main),"version":1,"text":source}}}));
        let line = source
            .lines()
            .position(|line| line.contains("this.box."))
            .unwrap();
        let line_text = source.lines().nth(line).unwrap();
        let character = line_text.find("this.box.").unwrap() + "this.box.".len();
        let response = server.handle(json!({"jsonrpc":"2.0","id":35,"method":"textDocument/completion","params":{"textDocument":{"uri":path_to_uri(&main)},"position":{"line":line,"character":character}}})).unwrap();
        assert!(response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "unwrap"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_updates_disk_overlay_and_watched_files_refresh_closed_sources() {
        let root = std::env::temp_dir().join(format!("aura-lsp-watch-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("main.aura");
        std::fs::write(&path, "package demo\nfun disk() {}\n").unwrap();
        let uri = path_to_uri(&path);
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_to_uri(&root)}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"text":"package demo\nfun edit() {}\n"}}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri},"text":"package demo\nfun saved() {}\n"}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
        assert!(server.document_text(&uri).unwrap().contains("fun saved"));
        std::fs::write(&path, "package demo\nfun changed_on_disk() {}\n").unwrap();
        server.handle(json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri,"type":2}]}}));
        assert!(server
            .document_text(&uri)
            .unwrap()
            .contains("changed_on_disk"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hover_includes_adjacent_source_documentation() {
        let mut server = Server::new();
        let source =
            "package demo\n/// Adds two values.\n/// The result is their sum.\npub fun add(a: Int, b: Int): Int { return a + b }\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":14,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":3,"character":9}}})).unwrap();
        let contents = hover_contents_text(&response);
        assert!(contents.contains("Adds two values.\nThe result is their sum."));
        assert!(!contents.contains("/ Adds two values."));
        assert!(!contents.contains("Defined in"));
        assert!(!contents.contains("{ return"));
        assert!(!contents.contains("Go to definition"));
    }

    #[test]
    fn completion_includes_adjacent_source_documentation() {
        let mut server = Server::new();
        let source =
            "package demo\n/// Adds two values.\nfun add(a: Int, b: Int): Int { return a + b }\nfun main() { ad }\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":15,"method":"textDocument/completion","params":{"textDocument":{"uri":"main.aura"},"position":{"line":3,"character":18}}})).unwrap();
        let item = response["result"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["label"] == "add")
            .unwrap();
        assert_eq!(item["documentation"]["kind"], "markdown");
        assert_eq!(item["documentation"]["value"], "Adds two values.");
    }

    #[test]
    fn hover_includes_class_and_member_documentation() {
        let mut server = Server::new();
        let source = "package demo\n\
/// Represents a notebook.\n\
pub class Notebook(val count: Int) {\n\
    /// Creates an empty notebook.\n\
    pub fun empty(): Notebook { return Notebook(0) }\n\
}\n\
pub fun notebook(): Notebook { return Notebook(0) }\n\
pub fun main() {\n\
    val nb = notebook()\n\
    nb.empty()\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));

        let declaration_hover = server.handle(json!({"jsonrpc":"2.0","id":16,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":8,"character":4}}})).unwrap();
        let declaration_contents = hover_contents_text(&declaration_hover);
        assert!(declaration_contents.contains("nb: demo.Notebook"));
        assert!(declaration_contents.contains("Represents a notebook."));

        let member_hover = server.handle(json!({"jsonrpc":"2.0","id":17,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":9,"character":4}}})).unwrap();
        assert!(hover_contents_text(&member_hover).contains("Creates an empty notebook."));

        let local_hover = server.handle(json!({"jsonrpc":"2.0","id":18,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":9,"character":1}}})).unwrap();
        let local_contents = hover_contents_text(&local_hover);
        assert!(local_contents.contains("nb: demo.Notebook"));
        assert!(local_contents.contains("Represents a notebook."));
        assert!(!local_contents.contains("Defined in"));
    }

    #[test]
    fn hover_resolves_member_documentation_from_receiver_type() {
        let mut server = Server::new();
        let source = "package demo\n\
class Request(val code: String) {\n\
    /// Return the request method, such as `GET` or `POST`.\n\
    pub fun method(): String { return \"GET\" }\n\
}\n\
class Context(val request: Request) {\n\
    pub fun method(): String { return this.request.method() }\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));

        let line = source
            .lines()
            .position(|line| line.contains("this.request.method"))
            .unwrap();
        let character = source.lines().nth(line).unwrap().rfind("method").unwrap();
        let response = server.handle(json!({"jsonrpc":"2.0","id":19,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":line,"character":character}}})).unwrap();

        let contents = hover_contents_text(&response);
        assert!(contents.contains("Return the request method, such as `GET` or `POST`."));
    }

    #[test]
    fn hover_resolves_the_selected_overload_signature_and_documentation() {
        let mut server = Server::new();
        let source = "package demo\n\
class Request(val value: String) {\n\
    /// Reads the request method without an argument.\n\
    pub fun method(): String { return this.value }\n\
    /// Reads the request method using a fallback.\n\
    pub fun method(fallback: String): String { return fallback }\n\
}\n\
fun main() {\n\
    val request = Request(\"GET\")\n\
    request.method()\n\
    request.method(\"POST\")\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));

        let no_arg_line = source
            .lines()
            .position(|line| line.contains("request.method()"))
            .unwrap();
        let no_arg_character = source
            .lines()
            .nth(no_arg_line)
            .unwrap()
            .find("method")
            .unwrap();
        let no_arg = server.handle(json!({"jsonrpc":"2.0","id":28,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":no_arg_line,"character":no_arg_character}}})).unwrap();
        let no_arg_contents = hover_contents_text(&no_arg);
        assert!(no_arg_contents.contains("fun method(): String"));
        assert!(no_arg_contents.contains("without an argument"));
        assert!(!no_arg_contents.contains("using a fallback"));

        let one_arg_line = source
            .lines()
            .position(|line| line.contains("request.method(\"POST\")"))
            .unwrap();
        let one_arg_character = source
            .lines()
            .nth(one_arg_line)
            .unwrap()
            .find("method")
            .unwrap();
        let one_arg = server.handle(json!({"jsonrpc":"2.0","id":29,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":one_arg_line,"character":one_arg_character}}})).unwrap();
        let one_arg_contents = hover_contents_text(&one_arg);
        assert!(
            one_arg_contents.contains("fun method(String): String"),
            "one-arg hover: {one_arg_contents}"
        );
        assert!(one_arg_contents.contains("using a fallback"));
        assert!(!one_arg_contents.contains("without an argument"));
    }

    #[test]
    fn hover_uses_semantic_overload_selection_when_arity_matches() {
        let mut server = Server::new();
        let source = "package demo\n\
class Request() {\n\
    /// Integer overload.\n\
    pub fun method(value: Int): String { return value.toString() }\n\
    /// String overload.\n\
    pub fun method(value: String): String { return value }\n\
}\n\
fun main() {\n\
    val request = Request()\n\
    request.method(1)\n\
    request.method(\"POST\")\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));

        let int_line = source
            .lines()
            .position(|line| line.contains("request.method(1)"))
            .unwrap();
        let int_character = source
            .lines()
            .nth(int_line)
            .unwrap()
            .find("method")
            .unwrap();
        let int_hover = server
            .handle(json!({"jsonrpc":"2.0","id":30,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":int_line,"character":int_character}}}))
            .unwrap();
        let int_contents = hover_contents_text(&int_hover);
        assert!(int_contents.contains("Integer overload"), "{int_contents}");
        assert!(!int_contents.contains("String overload"), "{int_contents}");

        let string_line = source
            .lines()
            .position(|line| line.contains("request.method(\"POST\")"))
            .unwrap();
        let string_character = source
            .lines()
            .nth(string_line)
            .unwrap()
            .find("method")
            .unwrap();
        let string_hover = server
            .handle(json!({"jsonrpc":"2.0","id":31,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":string_line,"character":string_character}}}))
            .unwrap();
        let string_contents = hover_contents_text(&string_hover);
        assert!(
            string_contents.contains("String overload"),
            "{string_contents}"
        );
        assert!(
            !string_contents.contains("Integer overload"),
            "{string_contents}"
        );
    }

    #[test]
    fn hover_infers_local_type_from_class_constructor_initializer() {
        let mut server = Server::new();
        let source = "package examples.notes\n\
import std.io as Io\n\
/// In-memory notebook with load/save support.\n\
class Notebook(val items: Array<String>) {\n\
    pub fun len(): Int { return this.items.len }\n\
    pub fun clear() { this.items.clear() }\n\
}\n\
pub fun notebook(): Notebook {\n\
    val nb = Notebook(Array(0))\n\
    return nb\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":source}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":20,"method":"textDocument/hover","params":{"textDocument":{"uri":"main.aura"},"position":{"line":8,"character":4}}})).unwrap();
        let contents = hover_contents_text(&response);
        assert!(contents.contains("nb: examples.notes.Notebook"));
        assert!(contents.contains("In-memory notebook with load/save support."));
    }

    #[test]
    fn hover_infers_local_type_in_notes_example() {
        let mut server = Server::new();
        let source = "package examples.notes\n\
/// In-memory notebook.\n\
class Notebook(val items: Array<String>) {}\n\
pub fun notebook(): Notebook {\n\
    val nb = Notebook(Array(0))\n\
    return nb\n\
}\n";
        let line = source
            .lines()
            .position(|line| line.contains("val nb = Notebook(Array(0))"))
            .unwrap();
        let character = source.lines().nth(line).unwrap().find("nb").unwrap();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"notes.aura","version":1,"text":source}}}));
        let response = server.handle(json!({"jsonrpc":"2.0","id":21,"method":"textDocument/hover","params":{"textDocument":{"uri":"notes.aura"},"position":{"line":line,"character":character}}})).unwrap();
        let contents = hover_contents_text(&response);
        assert!(
            contents.contains("nb: examples.notes.Notebook"),
            "{contents}"
        );
        assert!(contents.contains("In-memory notebook."), "{contents}");
    }

    #[test]
    fn hover_uses_package_analysis_for_notes_example() {
        let root = workspace_path("examples/notes");
        let manifest = root.join("aura.toml");
        let main_path = root.join("src/main.aura");
        let source = fs::read_to_string(&main_path).unwrap();
        let uri = path_to_uri(&main_path);
        let mut server = Server::new();
        server.handle(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":path_to_uri(&root)}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"version":1,"text":source}}}));
        let warmed_revision = server
            .package_expression_cache
            .borrow()
            .get(&manifest)
            .expect("diagnostics should warm the package cache")
            .revision;

        let line = source
            .lines()
            .position(|line| line.contains("val path ="))
            .unwrap();
        let character = source.lines().nth(line).unwrap().find("path").unwrap();
        let response = server.handle(json!({"jsonrpc":"2.0","id":22,"method":"textDocument/hover","params":{"textDocument":{"uri":uri},"position":{"line":line,"character":character}}})).unwrap();
        let contents = hover_contents_text(&response);

        assert!(contents.contains("path: String"), "{contents}");
        assert_eq!(
            server.package_expression_cache.borrow()[&manifest].revision,
            warmed_revision,
            "hover should reuse the diagnostics analysis"
        );

        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":source}]}}));
        assert!(
            server.package_expression_cache.borrow()[&manifest].revision > warmed_revision,
            "document changes must invalidate the cached package revision"
        );
    }

    #[test]
    fn hover_prefers_local_symbol_in_the_requested_document() {
        let mut server = Server::new();
        let main_source = "package examples.notes\n\
pub fun main() {\n\
    val nb = 1\n\
}\n";
        let notebook_source = "package examples.notes\n\
class Notebook(val items: Array<String>) {}\n\
pub fun notebook(): Notebook {\n\
    val nb = Notebook(Array(0))\n\
    missing()\n\
    return nb\n\
}\n";
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"main.aura","version":1,"text":main_source}}}));
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"notebook.aura","version":1,"text":notebook_source}}}));

        let line = notebook_source
            .lines()
            .position(|line| line.contains("val nb ="))
            .unwrap();
        let character = notebook_source
            .lines()
            .nth(line)
            .unwrap()
            .find("nb")
            .unwrap();
        let response = server.handle(json!({"jsonrpc":"2.0","id":22,"method":"textDocument/hover","params":{"textDocument":{"uri":"notebook.aura"},"position":{"line":line,"character":character}}})).unwrap();
        let contents = hover_contents_text(&response);

        assert!(contents.contains("nb: Notebook"), "{contents}");
        let definition = server.handle(json!({"jsonrpc":"2.0","id":25,"method":"textDocument/definition","params":{"textDocument":{"uri":"notebook.aura"},"position":{"line":line,"character":character}}})).unwrap();
        assert_eq!(definition["result"][0]["targetUri"], "notebook.aura");
    }

    #[test]
    fn hover_infers_local_type_from_function_return_type_when_analysis_fails() {
        let mut server = Server::new();
        let source = "package examples.notes\n\
class Notebook(val items: Array<String>) {}\n\
pub fun notebook(): Notebook {\n\
    return Notebook(Array(0))\n\
}\n\
pub fun notebookFromText(text: String): Notebook {\n\
    val nb = notebook()\n\
    missing()\n\
    return nb\n\
}\n";
        let line = source
            .lines()
            .position(|line| line.contains("val nb = notebook()"))
            .unwrap();
        let character = source.lines().nth(line).unwrap().find("nb").unwrap();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"notebook.aura","version":1,"text":source}}}));

        let response = server.handle(json!({"jsonrpc":"2.0","id":23,"method":"textDocument/hover","params":{"textDocument":{"uri":"notebook.aura"},"position":{"line":line,"character":character}}})).unwrap();
        let contents = hover_contents_text(&response);

        assert!(contents.contains("nb: Notebook"), "{contents}");
        assert!(
            contents.contains("class Notebook(val items: Array<String>)"),
            "{contents}"
        );
        let hover_items = response["result"]["contents"].as_array().unwrap();
        assert_eq!(hover_items[0]["language"], "aura");
        assert_eq!(hover_items[0]["value"], "nb: Notebook");
        assert_eq!(hover_items[1], "---");
        assert_eq!(hover_items[2]["language"], "aura");
        assert_eq!(
            hover_items[2]["value"],
            "class Notebook(val items: Array<String>)"
        );
        assert_eq!(hover_items.len(), 3);
    }

    #[test]
    fn hover_preserves_closing_generic_bracket_for_constructor_field() {
        let mut server = Server::new();
        let source = "package examples.notes\n\
class Notebook(val items: Array<String>) {\n\
    pub fun broken() { missing() }\n\
}\n";
        let line = source
            .lines()
            .position(|line| line.contains("class Notebook"))
            .unwrap();
        let character = source.lines().nth(line).unwrap().find("items").unwrap();
        server.handle(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"notebook.aura","version":1,"text":source}}}));

        let response = server.handle(json!({"jsonrpc":"2.0","id":24,"method":"textDocument/hover","params":{"textDocument":{"uri":"notebook.aura"},"position":{"line":line,"character":character}}})).unwrap();
        let contents = hover_contents_text(&response);

        assert!(contents.contains("val items: Array<String>"), "{contents}");
    }
}
