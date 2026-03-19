use crate::compiler::ast::Statement;
use crate::compiler::frontend::lexer::Lexer;
use crate::compiler::frontend::parser::Parser;
use crate::compiler::intrinsic::register_analyzer_intrinsics;
use crate::compiler::sema::checker::SemanticAnalyzer;
use crate::compiler::sema::ty::Type;
use crate::lsp::server::DocumentState;
use tower_lsp::lsp_types::*;

pub fn handle_completion(
    state: &DocumentState,
    uri: &Url,
    position: Position,
    stdlib_path: &str,
) -> Option<CompletionResponse> {
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
                    analyzer.load_stdlib(stdlib_path);

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
                            target_analyzer.load_stdlib(stdlib_path);
                            target_analyzer.analyze(program);

                            for sym in target_analyzer.scope.symbols.values() {
                                // Only suggest if exported AND defined in that file (exclude built-ins)
                                if sym.is_exported && sym.defined_in == abs_p_str {
                                    items.push(CompletionItem {
                                        label: sym.name.clone(),
                                        kind: Some(match sym.ty {
                                            Type::Function(..) => CompletionItemKind::FUNCTION,
                                            Type::Class(_) => CompletionItemKind::CLASS,
                                            Type::Enum(_) => CompletionItemKind::ENUM,
                                            _ => CompletionItemKind::VARIABLE,
                                        }),
                                        detail: Some(format!("{}", sym.ty)),
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
                            return Some(CompletionResponse::Array(items));
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
                        for (mname, minfo) in &class_info.methods {
                            if minfo.is_static {
                                items.push(CompletionItem {
                                    label: mname.clone(),
                                    kind: Some(CompletionItemKind::METHOD),
                                    detail: Some(format!(
                                        "fn({:?}) -> {:?}",
                                        minfo.params, minfo.ret_ty
                                    )),
                                    documentation: minfo
                                        .doc
                                        .as_ref()
                                        .map(|d| Documentation::String(d.clone())),
                                    ..Default::default()
                                });
                            }
                        }
                        for (fname, finfo) in &class_info.fields {
                            if finfo.is_static {
                                items.push(CompletionItem {
                                    label: fname.clone(),
                                    kind: Some(CompletionItemKind::FIELD),
                                    detail: Some(format!("{:?}", finfo.ty)),
                                    documentation: finfo
                                        .doc
                                        .as_ref()
                                        .map(|d| Documentation::String(d.clone())),
                                    ..Default::default()
                                });
                            }
                        }
                        return Some(CompletionResponse::Array(items));
                    }

                    // 2. Instance Access
                    for (span, ty) in &state.node_types {
                        if span.line == position.line as usize + 1
                            && col >= span.column
                            && col <= span.column + obj_name.len()
                        {
                            if let Type::Class(class_name) = ty {
                                if let Some(class_info) = state.classes.get(class_name) {
                                    for (mname, minfo) in &class_info.methods {
                                        if !minfo.is_static {
                                            items.push(CompletionItem {
                                                label: mname.clone(),
                                                kind: Some(CompletionItemKind::METHOD),
                                                detail: Some(format!(
                                                    "fn({:?}) -> {:?}",
                                                    minfo.params, minfo.ret_ty
                                                )),
                                                documentation: minfo
                                                    .doc
                                                    .as_ref()
                                                    .map(|d| Documentation::String(d.clone())),
                                                ..Default::default()
                                            });
                                        }
                                    }
                                    for (fname, finfo) in &class_info.fields {
                                        if !finfo.is_static {
                                            items.push(CompletionItem {
                                                label: fname.clone(),
                                                kind: Some(CompletionItemKind::FIELD),
                                                detail: Some(format!("{:?}", finfo.ty)),
                                                documentation: finfo
                                                    .doc
                                                    .as_ref()
                                                    .map(|d| Documentation::String(d.clone())),
                                                ..Default::default()
                                            });
                                        }
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
                                            documentation: sym
                                                .doc
                                                .as_ref()
                                                .map(|d: &String| Documentation::String(d.clone())),
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                            return Some(CompletionResponse::Array(items));
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
                            documentation: doc.as_ref().map(|d| Documentation::String(d.content())),
                            ..Default::default()
                        });
                    }
                }
                Statement::ClassDeclaration { name, doc, .. } => {
                    if seen.insert(name.clone()) {
                        items.push(CompletionItem {
                            label: name.clone(),
                            kind: Some(CompletionItemKind::CLASS),
                            documentation: doc.as_ref().map(|d| Documentation::String(d.content())),
                            ..Default::default()
                        });
                    }
                }
                Statement::VarDeclaration { name, doc, .. } => {
                    if seen.insert(name.clone()) {
                        items.push(CompletionItem {
                            label: name.clone(),
                            kind: Some(CompletionItemKind::VARIABLE),
                            documentation: doc.as_ref().map(|d| Documentation::String(d.content())),
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
            detail: Some("print<T>(value: T)".to_string()),
            documentation: Some(Documentation::String(
                "Prints a value to the standard output.".to_string(),
            )),
            insert_text: Some("print($1)".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });
    }

    Some(CompletionResponse::Array(items))
}
