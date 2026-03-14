use crate::compiler::ast::{Program, Span};
use crate::compiler::frontend::lexer::Lexer;
use crate::compiler::frontend::parser::Parser;
use crate::compiler::intrinsic::register_analyzer_intrinsics;
use crate::compiler::sema::checker::{ClassInfo, SemanticAnalyzer};
use crate::compiler::sema::ty::Type;
use crate::lsp::handler::completion::handle_completion;
use crate::lsp::handler::definition::handle_goto_definition;
use crate::lsp::handler::diagnostic::collect_diagnostics;
use crate::lsp::handler::formatting::handle_formatting;
use crate::lsp::handler::hover::handle_hover;
use crate::lsp::handler::symbol::handle_document_symbol;
use std::collections::HashMap;
use std::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

pub struct DocumentState {
    pub source: String,
    pub program: Option<Program>,
    pub node_types: HashMap<Span, Type>,
    pub node_definitions: HashMap<Span, (String, Span)>,
    pub node_docs: HashMap<Span, String>,
    pub classes: HashMap<String, ClassInfo>,
    pub analyzer_scope: HashMap<String, crate::compiler::sema::scope::Symbol>,
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
                document_formatting_provider: Some(OneOf::Left(true)),
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
            return Ok(handle_hover(state, position));
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
            return Ok(handle_document_symbol(state));
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.lock().unwrap();
        if let Some(state) = docs.get(&uri) {
            return Ok(handle_completion(state, &uri, position, &self.stdlib_path));
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
            if let Some(loc) = handle_goto_definition(state, &uri, position) {
                return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
            }
        }

        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let docs = self.documents.lock().unwrap();

        if let Some(state) = docs.get(&uri) {
            return Ok(handle_formatting(state));
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

        let path_str = params
            .uri
            .to_file_path()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut parser = Parser::new(tokens, path_str.clone());
        let program = parser.parse_program();

        let mut analyzer = SemanticAnalyzer::new();
        register_analyzer_intrinsics(&mut analyzer);
        analyzer.load_stdlib(&self.stdlib_path);
        analyzer.analyze(program.clone());

        let diagnostics = collect_diagnostics(&lexer, &parser, &analyzer);

        // Update document state
        {
            let mut docs = self.documents.lock().unwrap();
            let file_node_types = analyzer
                .node_types
                .get(&path_str)
                .cloned()
                .unwrap_or_default();
            let file_node_definitions = analyzer
                .node_definitions
                .get(&path_str)
                .cloned()
                .unwrap_or_default();
            let file_node_docs = analyzer
                .node_docs
                .get(&path_str)
                .cloned()
                .unwrap_or_default();

            docs.insert(
                params.uri.clone(),
                DocumentState {
                    source: params.text,
                    program: Some(program),
                    node_types: file_node_types,
                    node_definitions: file_node_definitions,
                    node_docs: file_node_docs,
                    classes: analyzer.classes,
                    analyzer_scope: analyzer.scope.symbols.clone(),
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
