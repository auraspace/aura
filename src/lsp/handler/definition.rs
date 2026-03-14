use crate::compiler::ast::Span;
use crate::lsp::server::DocumentState;
use tower_lsp::lsp_types::*;

pub fn handle_goto_definition(
    state: &DocumentState,
    uri: &Url,
    position: Position,
) -> Option<Location> {
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
        return Some(Location {
            uri: target_uri,
            range: Range {
                start: Position::new(def_span.line as u32 - 1, def_span.column as u32 - 1),
                end: Position::new(def_span.line as u32 - 1, def_span.column as u32),
            },
        });
    }

    None
}
