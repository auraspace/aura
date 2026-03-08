use crate::compiler::ast::{Program, Span};
use crate::compiler::frontend::error::Severity;
use crate::compiler::frontend::lexer::Lexer;
use crate::compiler::frontend::parser::Parser;
use crate::compiler::sema::checker::SemanticAnalyzer;
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
}

pub struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, DocumentState>>,
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
                return Ok(Some(Hover {
                    contents: HoverContents::Scalar(MarkedString::String(format!(
                        "type: {:?}",
                        ty
                    ))),
                    range: None,
                }));
            }
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
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Mutex::new(HashMap::new()),
        }
    }

    async fn on_change(&self, params: TextDocumentItem) {
        let source = &params.text;

        let mut lexer = Lexer::new(source);
        let tokens = lexer.lex_all();

        let mut parser = Parser::new(tokens);
        let program = parser.parse_program();

        let mut analyzer = SemanticAnalyzer::new();
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
            docs.insert(
                params.uri.clone(),
                DocumentState {
                    source: params.text,
                    program: Some(program),
                    node_types: analyzer.node_types,
                    node_definitions: analyzer.node_definitions,
                },
            );
        }

        self.client
            .publish_diagnostics(params.uri, diagnostics, Some(params.version))
            .await;
    }
}

pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
