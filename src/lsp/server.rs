use crate::compiler::ast::{Program, Span, Statement};
use crate::compiler::frontend::error::Severity;
use crate::compiler::frontend::lexer::Lexer;
use crate::compiler::frontend::parser::Parser;
use crate::compiler::sema::checker::{ClassInfo, SemanticAnalyzer};
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;
use std::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

pub struct DocumentState {
    pub source: String,
    pub program: Option<Program>,
    pub node_types: HashMap<Span, Type>,
    pub node_definitions: HashMap<Span, Span>,
    pub node_docs: HashMap<Span, String>,
    pub classes: HashMap<String, ClassInfo>,
}

pub struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, DocumentState>>,
    stdlib_path: String,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Aura Language Server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: params.text_document.text,
            version: params.text_document.version,
            language_id: params.text_document.language_id,
        })
        .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
            version: params.text_document.version,
            language_id: "aura".to_string(), // Default value
        })
        .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.lock().unwrap();
        if let Some(state) = docs.get(&uri) {
            // Find the most specific span containing the position
            let mut best_span: Option<Span> = None;
            let mut best_ty: Option<Type> = None;

            for (span, ty) in &state.node_types {
                let line = position.line as usize + 1;
                let col = position.character as usize + 1;

                if span.line == line && span.column <= col {
                    if let Some(prev_span) = best_span {
                        if span.column > prev_span.column {
                            best_span = Some(*span);
                            best_ty = Some(ty.clone());
                        }
                    } else {
                        best_span = Some(*span);
                        best_ty = Some(ty.clone());
                    }
                }
            }

            if let Some(ty) = best_ty {
                let span = best_span.unwrap();
                let doc = state.node_docs.get(&span);

                let mut markdown = format!("```aura\n{}\n```", ty);
                if let Some(doc_str) = doc {
                    markdown.push_str("\n\n---\n\n");
                    markdown.push_str(&format_doc_comment(doc_str));
                }

                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: markdown,
                    }),
                    range: None,
                }));
            }
        }

        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let docs = self.documents.lock().unwrap();
        if let Some(state) = docs.get(&uri) {
            if let Some(program) = &state.program {
                let mut symbols = Vec::new();
                for stmt in &program.statements {
                    match stmt {
                        Statement::FunctionDeclaration { name, span, .. } => {
                            symbols.push(DocumentSymbol {
                                name: name.clone(),
                                detail: None,
                                kind: SymbolKind::FUNCTION,
                                tags: None,
                                #[allow(deprecated)]
                                deprecated: None,
                                range: Range {
                                    start: Position::new(
                                        span.line as u32 - 1,
                                        span.column as u32 - 1,
                                    ),
                                    end: Position::new(span.line as u32 - 1, span.column as u32),
                                },
                                selection_range: Range {
                                    start: Position::new(
                                        span.line as u32 - 1,
                                        span.column as u32 - 1,
                                    ),
                                    end: Position::new(span.line as u32 - 1, span.column as u32),
                                },
                                children: None,
                            });
                        }
                        Statement::ClassDeclaration {
                            name,
                            fields,
                            methods,
                            span,
                            ..
                        } => {
                            let mut children = Vec::new();
                            for f in fields {
                                children.push(DocumentSymbol {
                                    name: f.name.clone(),
                                    detail: None,
                                    kind: SymbolKind::FIELD,
                                    tags: None,
                                    #[allow(deprecated)]
                                    deprecated: None,
                                    range: Range {
                                        start: Position::new(
                                            f.span.line as u32 - 1,
                                            f.span.column as u32 - 1,
                                        ),
                                        end: Position::new(
                                            f.span.line as u32 - 1,
                                            f.span.column as u32,
                                        ),
                                    },
                                    selection_range: Range {
                                        start: Position::new(
                                            f.span.line as u32 - 1,
                                            f.span.column as u32 - 1,
                                        ),
                                        end: Position::new(
                                            f.span.line as u32 - 1,
                                            f.span.column as u32,
                                        ),
                                    },
                                    children: None,
                                });
                            }
                            for m in methods {
                                children.push(DocumentSymbol {
                                    name: m.name.clone(),
                                    detail: None,
                                    kind: SymbolKind::METHOD,
                                    tags: None,
                                    #[allow(deprecated)]
                                    deprecated: None,
                                    range: Range {
                                        start: Position::new(
                                            m.span.line as u32 - 1,
                                            m.span.column as u32 - 1,
                                        ),
                                        end: Position::new(
                                            m.span.line as u32 - 1,
                                            m.span.column as u32,
                                        ),
                                    },
                                    selection_range: Range {
                                        start: Position::new(
                                            m.span.line as u32 - 1,
                                            m.span.column as u32 - 1,
                                        ),
                                        end: Position::new(
                                            m.span.line as u32 - 1,
                                            m.span.column as u32,
                                        ),
                                    },
                                    children: None,
                                });
                            }

                            symbols.push(DocumentSymbol {
                                name: name.clone(),
                                detail: None,
                                kind: SymbolKind::CLASS,
                                tags: None,
                                range: Range {
                                    start: Position::new(
                                        span.line as u32 - 1,
                                        span.column as u32 - 1,
                                    ),
                                    end: Position::new(span.line as u32 - 1, span.column as u32),
                                },
                                selection_range: Range {
                                    start: Position::new(
                                        span.line as u32 - 1,
                                        span.column as u32 - 1,
                                    ),
                                    end: Position::new(span.line as u32 - 1, span.column as u32),
                                },
                                #[allow(deprecated)]
                                deprecated: None,
                                children: Some(children),
                            });
                        }
                        Statement::VarDeclaration { name, span, .. } => {
                            symbols.push(DocumentSymbol {
                                name: name.clone(),
                                detail: None,
                                kind: SymbolKind::VARIABLE,
                                tags: None,
                                #[allow(deprecated)]
                                deprecated: None,
                                range: Range {
                                    start: Position::new(
                                        span.line as u32 - 1,
                                        span.column as u32 - 1,
                                    ),
                                    end: Position::new(span.line as u32 - 1, span.column as u32),
                                },
                                selection_range: Range {
                                    start: Position::new(
                                        span.line as u32 - 1,
                                        span.column as u32 - 1,
                                    ),
                                    end: Position::new(span.line as u32 - 1, span.column as u32),
                                },
                                children: None,
                            });
                        }
                        _ => {}
                    }
                }
                return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
            }
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.lock().unwrap();
        if let Some(state) = docs.get(&uri) {
            let mut items = Vec::new();

            // Check for member access (e.g., "obj.")
            let lines: Vec<&str> = state.source.lines().collect();
            if let Some(line) = lines.get(position.line as usize) {
                let col = position.character as usize;
                if col > 0 {
                    let before = &line[..col];
                    if before.ends_with('.') {
                        let parts: Vec<&str> = before[..before.len() - 1]
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .collect();
                        if let Some(obj_name) = parts.last() {
                            // 1. Static Access
                            if let Some(class_info) = state.classes.get(*obj_name) {
                                for (mname, (p_tys, r_ty, mdoc, _)) in &class_info.static_methods {
                                    items.push(CompletionItem {
                                        label: mname.clone(),
                                        kind: Some(CompletionItemKind::METHOD),
                                        detail: Some(format!("fn({:?}) -> {:?}", p_tys, r_ty)),
                                        documentation: mdoc
                                            .as_ref()
                                            .map(|d| Documentation::String(d.clone())),
                                        ..Default::default()
                                    });
                                }
                                for (fname, (f_ty, _, _)) in &class_info.static_fields {
                                    items.push(CompletionItem {
                                        label: fname.clone(),
                                        kind: Some(CompletionItemKind::FIELD),
                                        detail: Some(format!("{:?}", f_ty)),
                                        ..Default::default()
                                    });
                                }
                                return Ok(Some(CompletionResponse::Array(items)));
                            }

                            // 2. Instance Access
                            for (span, ty) in &state.node_types {
                                if span.line == position.line as usize + 1
                                    && col >= span.column
                                    && col <= span.column + obj_name.len()
                                {
                                    if let Type::Class(class_name) = ty {
                                        if let Some(class_info) = state.classes.get(class_name) {
                                            for (mname, (p_tys, r_ty, mdoc, _)) in
                                                &class_info.methods
                                            {
                                                items.push(CompletionItem {
                                                    label: mname.clone(),
                                                    kind: Some(CompletionItemKind::METHOD),
                                                    detail: Some(format!(
                                                        "fn({:?}) -> {:?}",
                                                        p_tys, r_ty
                                                    )),
                                                    documentation: mdoc
                                                        .as_ref()
                                                        .map(|d| Documentation::String(d.clone())),
                                                    ..Default::default()
                                                });
                                            }
                                            for (fname, (f_ty, _, _)) in &class_info.fields {
                                                items.push(CompletionItem {
                                                    label: fname.clone(),
                                                    kind: Some(CompletionItemKind::FIELD),
                                                    detail: Some(format!("{:?}", f_ty)),
                                                    ..Default::default()
                                                });
                                            }
                                        }
                                    }
                                    return Ok(Some(CompletionResponse::Array(items)));
                                }
                            }
                        }
                    }
                }
            }

            let mut seen = std::collections::HashSet::new();
            if let Some(program) = &state.program {
                for stmt in &program.statements {
                    match stmt {
                        Statement::FunctionDeclaration { name, doc, .. } => {
                            if seen.insert(name.clone()) {
                                items.push(CompletionItem {
                                    label: name.clone(),
                                    kind: Some(CompletionItemKind::FUNCTION),
                                    documentation: doc
                                        .as_ref()
                                        .map(|d| Documentation::String(d.clone())),
                                    ..Default::default()
                                });
                            }
                        }
                        Statement::ClassDeclaration { name, doc, .. } => {
                            if seen.insert(name.clone()) {
                                items.push(CompletionItem {
                                    label: name.clone(),
                                    kind: Some(CompletionItemKind::CLASS),
                                    documentation: doc
                                        .as_ref()
                                        .map(|d| Documentation::String(d.clone())),
                                    ..Default::default()
                                });
                            }
                        }
                        Statement::VarDeclaration { name, doc, .. } => {
                            if seen.insert(name.clone()) {
                                items.push(CompletionItem {
                                    label: name.clone(),
                                    kind: Some(CompletionItemKind::VARIABLE),
                                    documentation: doc
                                        .as_ref()
                                        .map(|d| Documentation::String(d.clone())),
                                    ..Default::default()
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }

            return Ok(Some(CompletionResponse::Array(items)));
        }

        Ok(None)
    }
    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.lock().unwrap();
        if let Some(state) = docs.get(&uri) {
            let mut best_span: Option<Span> = None;
            let mut best_def: Option<Span> = None;

            for (span, def) in &state.node_definitions {
                let line = position.line as usize + 1;
                let col = position.character as usize + 1;

                if span.line == line && span.column <= col {
                    if let Some(prev_span) = best_span {
                        if span.column > prev_span.column {
                            best_span = Some(*span);
                            best_def = Some(*def);
                        }
                    } else {
                        best_span = Some(*span);
                        best_def = Some(*def);
                    }
                }
            }

            if let Some(def) = best_def {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position::new(def.line as u32 - 1, def.column as u32 - 1),
                        end: Position::new(def.line as u32 - 1, def.column as u32),
                    },
                })));
            }
        }

        Ok(None)
    }
}

impl Backend {
    pub fn new(client: Client, stdlib_path: String) -> Self {
        Self {
            client,
            documents: Mutex::new(HashMap::new()),
            stdlib_path,
        }
    }

    async fn on_change(&self, params: TextDocumentItem) {
        let source = &params.text;

        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();

        let path_str = params.uri.to_file_path().unwrap_or_default().to_string_lossy().to_string();
        let mut parser = Parser::new(tokens, path_str.clone());
        let program = parser.parse_program();

        let mut analyzer = SemanticAnalyzer::new();
        analyzer.load_stdlib(&self.stdlib_path);
        analyzer.analyze(program.clone());

        let mut diagnostics = Vec::new();

        // Collect Lexer errors
        for diag in &lexer.diagnostics.diagnostics {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(diag.line as u32 - 1, diag.column as u32 - 1),
                    end: Position::new(diag.line as u32 - 1, diag.column as u32),
                },
                severity: Some(match diag.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                    Severity::Info => DiagnosticSeverity::INFORMATION,
                }),
                message: diag.message.clone(),
                ..Default::default()
            });
        }

        // Collect Parser errors
        for diag in &parser.diagnostics.diagnostics {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(diag.line as u32 - 1, diag.column as u32 - 1),
                    end: Position::new(diag.line as u32 - 1, diag.column as u32),
                },
                severity: Some(match diag.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                    Severity::Info => DiagnosticSeverity::INFORMATION,
                }),
                message: diag.message.clone(),
                ..Default::default()
            });
        }

        // Collect Semantic errors
        for diag in &analyzer.diagnostics.diagnostics {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(diag.line as u32 - 1, diag.column as u32 - 1),
                    end: Position::new(diag.line as u32 - 1, diag.column as u32),
                },
                severity: Some(match diag.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                    Severity::Info => DiagnosticSeverity::INFORMATION,
                }),
                message: diag.message.clone(),
                ..Default::default()
            });
        }

        // Update document state
        {
            let mut docs = self.documents.lock().unwrap();
            let file_node_types = analyzer.node_types.get(&path_str).cloned().unwrap_or_default();
            let file_node_definitions = analyzer.node_definitions.get(&path_str).cloned().unwrap_or_default();
            let file_node_docs = analyzer.node_docs.get(&path_str).cloned().unwrap_or_default();

            docs.insert(
                params.uri.clone(),
                DocumentState {
                    source: params.text,
                    program: Some(program),
                    node_types: file_node_types,
                    node_definitions: file_node_definitions,
                    node_docs: file_node_docs,
                    classes: analyzer.classes,
                },
            );
        }

        self.client
            .publish_diagnostics(params.uri, diagnostics, Some(params.version))
            .await;
    }
}

pub async fn run_server(stdlib_path: String) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client, stdlib_path.clone()));
    Server::new(stdin, stdout, socket).serve(service).await;
}

fn format_doc_comment(doc: &str) -> String {
    let mut lines: Vec<String> = doc
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('*') {
                let content = &trimmed[1..];
                if content.starts_with(' ') {
                    &content[1..]
                } else {
                    content
                }
                .trim_end()
            } else {
                line.trim_end()
            }
            .to_string()
        })
        .collect();

    // Trim leading/trailing empty lines
    while lines.first().map_or(false, |s| s.is_empty()) {
        lines.remove(0);
    }
    while lines.last().map_or(false, |s| s.is_empty()) {
        lines.pop();
    }

    lines.join("  \n")
}
