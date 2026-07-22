//! Aura AST for compiler milestones C0–C3e (RFC-001 §6.0).

mod nodes;
mod shift;
mod span;
mod async_state;

pub use nodes::*;
pub use async_state::{AsyncSuspensionKind, AsyncSuspensionPoint};
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

    #[test]
    fn async_nodes_expose_operation_spans_and_shift_children() {
        let operand = Expr::Ident(Ident {
            name: "task".into(),
            span: Span::new(6, 10),
        });
        let mut await_expr = AsyncExpr::Await(AwaitExpr {
            operand: Box::new(operand),
            span: Span::new(0, 10),
        });

        assert_eq!(await_expr.span(), Span::new(0, 10));
        await_expr.shift_spans(4);
        assert_eq!(await_expr.span(), Span::new(4, 14));
        assert_eq!(
            match await_expr {
                AsyncExpr::Await(AwaitExpr { operand, .. }) => operand.span(),
                _ => unreachable!(),
            },
            Span::new(10, 14)
        );
    }

    #[test]
    fn async_function_keeps_declaration_and_body_spans() {
        let mut decl = AsyncFunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: Vec::new(),
            is_test: false,
            name: Ident {
                name: "load".into(),
                span: Span::new(6, 10),
            },
            type_params: Vec::new(),
            params: Vec::new(),
            return_type: None,
            body: Block {
                stmts: Vec::new(),
                span: Span::new(11, 13),
            },
            span: Span::new(0, 13),
        };

        decl.shift_spans(5);
        assert_eq!(decl.name.span, Span::new(11, 15));
        assert_eq!(decl.body.span, Span::new(16, 18));
        assert_eq!(decl.span, Span::new(5, 18));
    }

    #[test]
    fn async_suspension_points_are_lexical_and_deterministic() {
        let span = Span::new(0, 1);
        let ident = |name: &str| Ident { name: name.into(), span };
        let await_expr = |start| {
            Expr::Async(AsyncExpr::Await(AwaitExpr {
                operand: Box::new(Expr::Ident(Ident { name: "task".into(), span })),
                span: Span::new(start, start + 10),
            }))
        };
        let decl = AsyncFunDecl {
            is_pub: false, origin_package: String::new(), attributes: vec![], is_test: false,
            name: ident("run"), type_params: vec![], params: vec![], return_type: None,
            body: Block { stmts: vec![
                Stmt::Expr(await_expr(10)),
                Stmt::If(IfStmt {
                    cond: Expr::Bool(BoolLit { value: true, span }),
                    then_block: Block { stmts: vec![Stmt::Expr(await_expr(30))], span },
                    else_block: None, span,
                }),
            ], span },
            span,
        };
        assert_eq!(decl.suspension_points(), vec![
            AsyncSuspensionPoint { state_id: 1, kind: AsyncSuspensionKind::Await, span: Span::new(10, 20) },
            AsyncSuspensionPoint { state_id: 2, kind: AsyncSuspensionKind::Await, span: Span::new(30, 40) },
        ]);
    }
}
