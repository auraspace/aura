use crate::compiler::ast::Statement;
use crate::lsp::server::DocumentState;
use tower_lsp::lsp_types::*;

pub fn handle_document_symbol(state: &DocumentState) -> Option<DocumentSymbolResponse> {
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
                            start: Position::new(span.line as u32 - 1, span.column as u32 - 1),
                            end: Position::new(span.line as u32 - 1, span.column as u32),
                        },
                        selection_range: Range {
                            start: Position::new(span.line as u32 - 1, span.column as u32 - 1),
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
                                end: Position::new(f.span.line as u32 - 1, f.span.column as u32),
                            },
                            selection_range: Range {
                                start: Position::new(
                                    f.span.line as u32 - 1,
                                    f.span.column as u32 - 1,
                                ),
                                end: Position::new(f.span.line as u32 - 1, f.span.column as u32),
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
                                end: Position::new(m.span.line as u32 - 1, m.span.column as u32),
                            },
                            selection_range: Range {
                                start: Position::new(
                                    m.span.line as u32 - 1,
                                    m.span.column as u32 - 1,
                                ),
                                end: Position::new(m.span.line as u32 - 1, m.span.column as u32),
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
                            start: Position::new(span.line as u32 - 1, span.column as u32 - 1),
                            end: Position::new(span.line as u32 - 1, span.column as u32),
                        },
                        selection_range: Range {
                            start: Position::new(span.line as u32 - 1, span.column as u32 - 1),
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
                            start: Position::new(span.line as u32 - 1, span.column as u32 - 1),
                            end: Position::new(span.line as u32 - 1, span.column as u32),
                        },
                        selection_range: Range {
                            start: Position::new(span.line as u32 - 1, span.column as u32 - 1),
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
                                    member.name_span.column as u32 + member.name.len() as u32 - 1,
                                ),
                            },
                            selection_range: Range {
                                start: Position::new(
                                    member.name_span.line as u32 - 1,
                                    member.name_span.column as u32 - 1,
                                ),
                                end: Position::new(
                                    member.name_span.line as u32 - 1,
                                    member.name_span.column as u32 + member.name.len() as u32 - 1,
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
                            end: Position::new(decl.span.line as u32 - 1, decl.span.column as u32),
                        },
                        selection_range: Range {
                            start: Position::new(
                                decl.span.line as u32 - 1,
                                decl.span.column as u32 - 1,
                            ),
                            end: Position::new(decl.span.line as u32 - 1, decl.span.column as u32),
                        },
                        #[allow(deprecated)]
                        deprecated: None,
                        children: Some(children),
                    });
                }
                _ => {}
            }
        }
        return Some(DocumentSymbolResponse::Nested(symbols));
    }
    None
}
