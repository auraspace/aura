use crate::{AsyncExpr, AsyncFunDecl, Block, Expr, LambdaBody, Span, Stmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncSuspensionKind {
    Await,
}

/// A bounded, compiler-only description of an async suspension point.
/// State zero is the initial invocation; suspension states start at one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncSuspensionPoint {
    pub state_id: u32,
    pub kind: AsyncSuspensionKind,
    pub span: Span,
}

impl AsyncFunDecl {
    /// Return suspension points in deterministic lexical traversal order.
    pub fn suspension_points(&self) -> Vec<AsyncSuspensionPoint> {
        let mut points = Vec::new();
        let mut next_state = 1;
        collect_block(&self.body, &mut next_state, &mut points);
        points
    }
}

fn collect_block(block: &Block, next: &mut u32, points: &mut Vec<AsyncSuspensionPoint>) {
    for stmt in &block.stmts {
        collect_stmt(stmt, next, points);
    }
}

fn collect_stmt(stmt: &Stmt, next: &mut u32, points: &mut Vec<AsyncSuspensionPoint>) {
    match stmt {
        Stmt::Var(v) => collect_expr(&v.init, next, points),
        Stmt::If(i) => {
            collect_expr(&i.cond, next, points);
            collect_block(&i.then_block, next, points);
            if let Some(b) = &i.else_block {
                collect_block(b, next, points);
            }
        }
        Stmt::While(w) => {
            collect_expr(&w.cond, next, points);
            collect_block(&w.body, next, points);
        }
        Stmt::ForRange(f) => {
            collect_expr(&f.start, next, points);
            collect_expr(&f.end, next, points);
            collect_block(&f.body, next, points);
        }
        Stmt::ForIn(f) => {
            collect_expr(&f.iterable, next, points);
            collect_block(&f.body, next, points);
        }
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Match(m) => {
            collect_expr(&m.scrutinee, next, points);
            for arm in &m.arms {
                collect_block(&arm.body, next, points);
            }
        }
        Stmt::Try(t) => {
            collect_block(&t.try_block, next, points);
            if let Some(c) = &t.catch {
                collect_block(&c.body, next, points);
            }
            if let Some(b) = &t.finally {
                collect_block(b, next, points);
            }
        }
        Stmt::Throw(t) => collect_expr(&t.value, next, points),
        Stmt::Return(r) => {
            if let Some(e) = &r.value {
                collect_expr(e, next, points);
            }
        }
        Stmt::Expr(e) => collect_expr(e, next, points),
    }
}

fn collect_expr(expr: &Expr, next: &mut u32, points: &mut Vec<AsyncSuspensionPoint>) {
    match expr {
        Expr::Call(c) => {
            collect_expr(&c.callee, next, points);
            for a in &c.args {
                collect_expr(a, next, points);
            }
        }
        Expr::Field(f) => collect_expr(&f.object, next, points),
        Expr::Assign(a) => collect_expr(&a.value, next, points),
        Expr::Binary(b) => {
            collect_expr(&b.left, next, points);
            collect_expr(&b.right, next, points);
        }
        Expr::Unary(u) => collect_expr(&u.expr, next, points),
        Expr::ForceUnwrap(f) => collect_expr(&f.expr, next, points),
        Expr::Is(i) => collect_expr(&i.expr, next, points),
        Expr::Group(e, _) => collect_expr(e, next, points),
        Expr::If(i) => {
            collect_expr(&i.cond, next, points);
            collect_block(&i.then_block, next, points);
            collect_block(&i.else_block, next, points);
        }
        Expr::Lambda(l) => match &l.body {
            LambdaBody::Expr(e) => collect_expr(e, next, points),
            LambdaBody::Block(b) => collect_block(b, next, points),
        },
        Expr::Async(a) => match a {
            AsyncExpr::Await(a) => {
                collect_expr(&a.operand, next, points);
                points.push(AsyncSuspensionPoint {
                    state_id: *next,
                    kind: AsyncSuspensionKind::Await,
                    span: a.span,
                });
                *next += 1;
            }
            AsyncExpr::Spawn(_) => {}
            AsyncExpr::Join(j) => collect_expr(&j.handle, next, points),
            AsyncExpr::Cancel(c) => collect_expr(&c.handle, next, points),
            AsyncExpr::ChannelCreate(c) => collect_expr(&c.capacity, next, points),
            AsyncExpr::ChannelSend(s) => {
                collect_expr(&s.channel, next, points);
                collect_expr(&s.value, next, points);
            }
            AsyncExpr::ChannelReceive(r) => collect_expr(&r.channel, next, points),
            AsyncExpr::ChannelClose(c) => collect_expr(&c.channel, next, points),
        },
        Expr::Ident(_)
        | Expr::This(_)
        | Expr::Int(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::Null(_) => {}
    }
}
