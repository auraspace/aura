use crate::compiler::ast::{Program, Span, Statement};
use crate::compiler::frontend::error::Severity;
use crate::compiler::frontend::formatter::Formatter;
use crate::compiler::frontend::lexer::Lexer;
use crate::compiler::frontend::parser::Parser;
use crate::compiler::intrinsic::register_analyzer_intrinsics;
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
                        Statement::Enum(decl) => {
                            let mut children = Vec::new();
                            for member in &decl.members {
                                children.push(DocumentSymbol {
                                    name: member.name.clone(),
                                    detail: None,
                                    kind: SymbolKind::ENUM_MEMBER,
                                    tags: None,
                                    #[allow(deprecated)]
                                    deprecated: None,
                                    range: Range {
                                        start: Position::new(
                                            member.name_span.line as u32 - 1,
                                            member.name_span.column as u32 - 1,
                                        ),
                                        end: Position::new(
                                            member.name_span.line as u32 - 1,
                                            member.name_span.column as u32
                                                + member.name.len() as u32
                                                - 1,
                                        ),
                                    },
                                    selection_range: Range {
                                        start: Position::new(
                                            member.name_span.line as u32 - 1,
                                            member.name_span.column as u32 - 1,
                                        ),
                                        end: Position::new(
                                            member.name_span.line as u32 - 1,
                                            member.name_span.column as u32
                                                + member.name.len() as u32
                                                - 1,
                                        ),
                                    },
                                    children: None,
                                });
                            }

                            symbols.push(DocumentSymbol {
                                name: decl.name.clone(),
                                detail: None,
                                kind: SymbolKind::ENUM,
                                tags: None,
                                range: Range {
                                    start: Position::new(
                                        decl.span.line as u32 - 1,
                                        decl.span.column as u32 - 1,
                                    ),
                                    end: Position::new(
                                        decl.span.line as u32 - 1,
                                        decl.span.column as u32,
                                    ),
                                },
                                selection_range: Range {
                                    start: Position::new(
                                        decl.span.line as u32 - 1,
                                        decl.span.column as u32 - 1,
                                    ),
                                    end: Position::new(
                                        decl.span.line as u32 - 1,
                                        decl.span.column as u32,
                                    ),
                                },
                                #[allow(deprecated)]
                                deprecated: None,
                                children: Some(children),
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

                    let open_brace = before.rfind('{');
                    let close_brace = before.rfind('}');
                    let is_in_braces = before.contains("import")
                        && open_brace.is_some()
                        && (close_brace.is_none() || open_brace > close_brace);

                    if is_in_braces && (line.contains("} from") || line.contains("from")) {
                        // Extract path
                        let mut path = String::new();
                        if let Some(from_pos) = line.find("from") {
                            let after_from = &line[from_pos + 4..].trim();
                            if (after_from.starts_with('\'') && after_from.len() > 1)
                                || (after_from.starts_with('"') && after_from.len() > 1)
                            {
                                let quote = after_from.chars().next().unwrap();
                                let mut end_quote_idx = None;
                                for (i, c) in after_from[1..].chars().enumerate() {
                                    if c == quote {
                                        end_quote_idx = Some(i + 1);
                                        break;
                                    }
                                }
                                if let Some(eq) = end_quote_idx {
                                    path = after_from[1..eq].to_string();
                                }
                            }
                        }

                        if !path.is_empty() {
                            let mut analyzer = SemanticAnalyzer::new();
                            let file_path = uri.to_file_path().unwrap_or_default();
                            if let Some(parent) = file_path.parent() {
                                analyzer.set_current_dir(parent.to_string_lossy().to_string());
                            }
                            register_analyzer_intrinsics(&mut analyzer);
                            analyzer.load_stdlib(&self.stdlib_path);

                            if let Ok(abs_p) = analyzer.resolve_import_path(&path) {
                                let abs_p_str = abs_p.to_string_lossy().to_string();
                                if let Ok(source) = std::fs::read_to_string(&abs_p) {
                                    let mut lexer = Lexer::new(&source);
                                    let tokens = lexer.lex_all();
                                    let mut parser = Parser::new(tokens, abs_p_str.clone());
                                    let program = parser.parse_program();

                                    let mut target_analyzer = SemanticAnalyzer::new();
                                    if let Some(parent) = abs_p.parent() {
                                        target_analyzer
                                            .set_current_dir(parent.to_string_lossy().to_string());
                                    }
                                    register_analyzer_intrinsics(&mut target_analyzer);
                                    target_analyzer.load_stdlib(&self.stdlib_path);
                                    target_analyzer.analyze(program);

                                    for sym in target_analyzer.scope.symbols.values() {
                                        // Only suggest if exported AND defined in that file (exclude built-ins)
                                        if sym.is_exported && sym.defined_in == abs_p_str {
                                            items.push(CompletionItem {
                                                label: sym.name.clone(),
                                                kind: Some(match sym.ty {
                                                    Type::Function(_, _) => {
                                                        CompletionItemKind::FUNCTION
                                                    }
                                                    Type::Class(_) => CompletionItemKind::CLASS,
                                                    Type::Enum(_) => CompletionItemKind::ENUM,
                                                    _ => CompletionItemKind::VARIABLE,
                                                }),
                                                detail: Some(format!("{}", sym.ty)), // Use format! for nicer type display
                                                documentation: sym
                                                    .doc
                                                    .as_ref()
                                                    .map(|d| Documentation::String(d.clone())),
                                                ..Default::default()
                                            });
                                        }
                                    }
                                    for class in target_analyzer.classes.values() {
                                        if class.is_exported && class.defined_in == abs_p_str {
                                            items.push(CompletionItem {
                                                label: class.name.clone(),
                                                kind: Some(CompletionItemKind::CLASS),
                                                documentation: class
                                                    .doc
                                                    .as_ref()
                                                    .map(|d| Documentation::String(d.clone())),
                                                ..Default::default()
                                            });
                                        }
                                    }
                                    return Ok(Some(CompletionResponse::Array(items)));
                                }
                            }
                        }
                    }

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
                                    } else if let Type::Enum(enum_name) = ty {
                                        // Enum Members
                                        for (fqn, sym) in &state.analyzer_scope {
                                            let prefix = format!("{}.", enum_name);
                                            if fqn.starts_with(&prefix) {
                                                let member_name = &fqn[enum_name.len() + 1..];
                                                items.push(CompletionItem {
                                                    label: member_name.to_string(),
                                                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                                                    detail: Some(format!("{:?}", sym.ty)),
                                                    documentation: sym.doc.as_ref().map(
                                                        |d: &String| {
                                                            Documentation::String(d.clone())
                                                        },
                                                    ),
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
                                        .map(|d| Documentation::String(d.content())),
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
                                        .map(|d| Documentation::String(d.content())),
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
                                        .map(|d| Documentation::String(d.content())),
                                    ..Default::default()
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }

            // 3. Keywords & Built-ins
            let keywords = vec![
                "let",
                "if",
                "else",
                "while",
                "function",
                "return",
                "class",
                "constructor",
                "new",
                "static",
                "this",
                "is",
                "import",
                "export",
                "from",
                "as",
                "async",
                "await",
                "try",
                "catch",
                "throw",
                "finally",
                "null",
            ];

            for kw in keywords {
                if seen.insert(kw.to_string()) {
                    items.push(CompletionItem {
                        label: kw.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        ..Default::default()
                    });
                }
            }

            // Built-in functions
            if seen.insert("print".to_string()) {
                items.push(CompletionItem {
                    label: "print".to_string(),
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail: Some("print(value: any)".to_string()),
                    documentation: Some(Documentation::String(
                        "Prints a value to the standard output.".to_string(),
                    )),
                    insert_text: Some("print($1)".to_string()),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    ..Default::default()
                });
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
            let mut best_def: Option<(String, Span)> = None;

            for (span, def) in &state.node_definitions {
                let line = position.line as usize + 1;
                let col = position.character as usize + 1;

                if span.line == line && span.column <= col {
                    if let Some(prev_span) = best_span {
                        if span.column > prev_span.column {
                            best_span = Some(*span);
                            best_def = Some(def.clone());
                        }
                    } else {
                        best_span = Some(*span);
                        best_def = Some(def.clone());
                    }
                }
            }

            if let Some((def_file, def_span)) = best_def {
                let target_uri = if def_file.is_empty() {
                    uri.clone()
                } else {
                    Url::from_file_path(&def_file).unwrap_or(uri.clone())
                };
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range: Range {
                        start: Position::new(def_span.line as u32 - 1, def_span.column as u32 - 1),
                        end: Position::new(def_span.line as u32 - 1, def_span.column as u32),
                    },
                })));
            }
        }

        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let docs = self.documents.lock().unwrap();

        if let Some(state) = docs.get(&uri) {
            if let Some(program) = &state.program {
                let formatter = Formatter::new().with_source(state.source.clone());
                let formatted = formatter.format_program(program);

                let lines: Vec<&str> = state.source.lines().collect();
                let last_line = lines.len() as u32;
                let last_char = lines.last().map(|l| l.len()).unwrap_or(0) as u32;

                return Ok(Some(vec![TextEdit {
                    range: Range {
                        start: Position::new(0, 0),
                        end: Position::new(last_line, last_char),
                    },
                    new_text: formatted,
                }]));
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
