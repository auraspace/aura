//! Aura AST for compiler milestones C0–C3e (RFC-001 §6.0).

mod nodes;
mod shift;
mod span;

pub use nodes::*;
pub use shift::shift_file_spans;
pub use span::{BytePos, Span};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_display() {
        let p = Path {
            segments: vec![
                Ident {
                    name: "demo".into(),
                    span: Span::new(0, 4),
                },
                Ident {
                    name: "util".into(),
                    span: Span::new(5, 9),
                },
            ],
            span: Span::new(0, 9),
        };
        assert_eq!(p.display(), "demo.util");
    }

    #[test]
    fn span_shift() {
        let s = Span::new(3, 7).shift(10);
        assert_eq!(s, Span::new(13, 17));
    }
}
