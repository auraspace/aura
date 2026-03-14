use crate::compiler::ast::Span;
use crate::compiler::sema::ty::Type;
use crate::lsp::handler::format_doc_comment;
use crate::lsp::server::DocumentState;
use tower_lsp::lsp_types::*;

pub fn handle_hover(state: &DocumentState, position: Position) -> Option<Hover> {
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

        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: None,
        });
    }

    None
}
