use crate::compiler::frontend::formatter::Formatter;
use crate::lsp::server::DocumentState;
use tower_lsp::lsp_types::*;

pub fn handle_formatting(state: &DocumentState) -> Option<Vec<TextEdit>> {
    if let Some(program) = &state.program {
        let formatter = Formatter::new().with_source(state.source.clone());
        let formatted = formatter.format_program(program);

        let lines: Vec<&str> = state.source.lines().collect();
        let last_line = lines.len() as u32;
        let last_char = lines.last().map(|l| l.len()).unwrap_or(0) as u32;

        return Some(vec![TextEdit {
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(last_line, last_char),
            },
            new_text: formatted,
        }]);
    }
    None
}
