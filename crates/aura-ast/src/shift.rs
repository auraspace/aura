//! Span rewriting for multi-file virtual buffers.

use crate::nodes::*;
use crate::span::BytePos;

/// Rewrite every span in `file` by adding `delta` (multi-file virtual buffer).
pub fn shift_file_spans(file: &mut File, delta: BytePos) {
    if delta == 0 {
        return;
    }
    file.package.shift_spans(delta);
    file.span = file.span.shift(delta);
    for imp in &mut file.imports {
        shift_import(imp, delta);
    }
    for i in &mut file.interfaces {
        shift_interface(i, delta);
    }
    for e in &mut file.enums {
        shift_enum(e, delta);
    }
    for c in &mut file.classes {
        shift_class(c, delta);
    }
    for t in &mut file.type_aliases {
        shift_type_alias(t, delta);
    }
    for c in &mut file.consts {
        shift_const(c, delta);
    }
    for f in &mut file.functions {
        shift_fun(f, delta);
    }
}

fn shift_type_alias(t: &mut TypeAliasDecl, delta: BytePos) {
    shift_ident(&mut t.name, delta);
    shift_type_ref(&mut t.ty, delta);
    t.span = t.span.shift(delta);
}

fn shift_const(c: &mut ConstDecl, delta: BytePos) {
    shift_ident(&mut c.name, delta);
    shift_type_ref(&mut c.ty, delta);
    shift_expr(&mut c.value, delta);
    c.span = c.span.shift(delta);
}

fn shift_import(imp: &mut ImportDecl, delta: BytePos) {
    imp.path.shift_spans(delta);
    if let Some(a) = &mut imp.alias {
        shift_ident(a, delta);
    }
    imp.span = imp.span.shift(delta);
}

fn shift_ident(i: &mut Ident, delta: BytePos) {
    i.span = i.span.shift(delta);
}

fn shift_type_param(tp: &mut TypeParam, delta: BytePos) {
    shift_ident(&mut tp.name, delta);
    for b in &mut tp.bounds {
        shift_ident(b, delta);
    }
}

fn shift_type_ref(t: &mut TypeRef, delta: BytePos) {
    if let Some(fun) = t.fun.as_mut() {
        for p in &mut fun.params {
            shift_type_ref(p, delta);
        }
        shift_type_ref(&mut fun.ret, delta);
    }
    if let Some(q) = &mut t.qualifier {
        shift_ident(q, delta);
    }
    shift_ident(&mut t.name, delta);
    for a in &mut t.type_args {
        shift_type_ref(a, delta);
    }
    t.span = t.span.shift(delta);
}

fn shift_param(p: &mut Param, delta: BytePos) {
    shift_ident(&mut p.name, delta);
    shift_type_ref(&mut p.ty, delta);
    p.span = p.span.shift(delta);
}

fn shift_method_sig(m: &mut MethodSig, delta: BytePos) {
    shift_ident(&mut m.name, delta);
    for p in &mut m.params {
        shift_param(p, delta);
    }
    if let Some(rt) = &mut m.return_type {
        shift_type_ref(rt, delta);
    }
    m.span = m.span.shift(delta);
}

fn shift_interface(i: &mut InterfaceDecl, delta: BytePos) {
    shift_ident(&mut i.name, delta);
    for tp in &mut i.type_params {
        shift_type_param(tp, delta);
    }
    for m in &mut i.methods {
        shift_method_sig(m, delta);
    }
    i.span = i.span.shift(delta);
}

fn shift_enum_variant(v: &mut EnumVariant, delta: BytePos) {
    shift_ident(&mut v.name, delta);
    for f in &mut v.fields {
        shift_param(f, delta);
    }
    v.span = v.span.shift(delta);
}

fn shift_enum(e: &mut EnumDecl, delta: BytePos) {
    shift_ident(&mut e.name, delta);
    for tp in &mut e.type_params {
        shift_type_param(tp, delta);
    }
    for v in &mut e.variants {
        shift_enum_variant(v, delta);
    }
    e.span = e.span.shift(delta);
}

fn shift_field(f: &mut FieldDecl, delta: BytePos) {
    shift_ident(&mut f.name, delta);
    shift_type_ref(&mut f.ty, delta);
    f.span = f.span.shift(delta);
}

fn shift_class(c: &mut ClassDecl, delta: BytePos) {
    shift_ident(&mut c.name, delta);
    for tp in &mut c.type_params {
        shift_type_param(tp, delta);
    }
    for i in &mut c.implements {
        shift_type_ref(i, delta);
    }
    for f in &mut c.fields {
        shift_field(f, delta);
    }
    for m in &mut c.methods {
        shift_fun(m, delta);
    }
    c.span = c.span.shift(delta);
}

fn shift_fun(f: &mut FunDecl, delta: BytePos) {
    shift_ident(&mut f.name, delta);
    for tp in &mut f.type_params {
        shift_type_param(tp, delta);
    }
    for p in &mut f.params {
        shift_param(p, delta);
    }
    if let Some(rt) = &mut f.return_type {
        shift_type_ref(rt, delta);
    }
    shift_block(&mut f.body, delta);
    f.span = f.span.shift(delta);
}

fn shift_block(b: &mut Block, delta: BytePos) {
    for s in &mut b.stmts {
        shift_stmt(s, delta);
    }
    b.span = b.span.shift(delta);
}

fn shift_stmt(s: &mut Stmt, delta: BytePos) {
    match s {
        Stmt::Var(v) => {
            shift_ident(&mut v.name, delta);
            if let Some(t) = &mut v.ty {
                shift_type_ref(t, delta);
            }
            shift_expr(&mut v.init, delta);
            v.span = v.span.shift(delta);
        }
        Stmt::If(i) => {
            shift_expr(&mut i.cond, delta);
            shift_block(&mut i.then_block, delta);
            if let Some(e) = &mut i.else_block {
                shift_block(e, delta);
            }
            i.span = i.span.shift(delta);
        }
        Stmt::While(w) => {
            shift_expr(&mut w.cond, delta);
            shift_block(&mut w.body, delta);
            w.span = w.span.shift(delta);
        }
        Stmt::ForRange(f) => {
            shift_ident(&mut f.name, delta);
            shift_expr(&mut f.start, delta);
            shift_expr(&mut f.end, delta);
            shift_block(&mut f.body, delta);
            f.span = f.span.shift(delta);
        }
        Stmt::ForIn(f) => {
            shift_ident(&mut f.name, delta);
            shift_expr(&mut f.iterable, delta);
            shift_block(&mut f.body, delta);
            f.span = f.span.shift(delta);
        }
        Stmt::Break(s) => *s = s.shift(delta),
        Stmt::Continue(s) => *s = s.shift(delta),
        Stmt::Match(m) => {
            shift_expr(&mut m.scrutinee, delta);
            for a in &mut m.arms {
                shift_pattern(&mut a.pattern, delta);
                shift_block(&mut a.body, delta);
                a.span = a.span.shift(delta);
            }
            m.span = m.span.shift(delta);
        }
        Stmt::Try(t) => {
            shift_block(&mut t.try_block, delta);
            if let Some(c) = &mut t.catch {
                shift_ident(&mut c.name, delta);
                shift_type_ref(&mut c.ty, delta);
                shift_block(&mut c.body, delta);
                c.span = c.span.shift(delta);
            }
            if let Some(f) = &mut t.finally {
                shift_block(f, delta);
            }
            t.span = t.span.shift(delta);
        }
        Stmt::Throw(t) => {
            shift_expr(&mut t.value, delta);
            t.span = t.span.shift(delta);
        }
        Stmt::Return(r) => {
            if let Some(v) = &mut r.value {
                shift_expr(v, delta);
            }
            r.span = r.span.shift(delta);
        }
        Stmt::Expr(e) => shift_expr(e, delta),
    }
}

fn shift_pattern(p: &mut Pattern, delta: BytePos) {
    match p {
        Pattern::Variant {
            name,
            bindings,
            span,
        } => {
            shift_ident(name, delta);
            for b in bindings {
                shift_ident(b, delta);
            }
            *span = span.shift(delta);
        }
    }
}

fn shift_expr(e: &mut Expr, delta: BytePos) {
    match e {
        Expr::Ident(i) => shift_ident(i, delta),
        Expr::This(s) => *s = s.shift(delta),
        Expr::Int(l) => l.span = l.span.shift(delta),
        Expr::Bool(l) => l.span = l.span.shift(delta),
        Expr::String(l) => l.span = l.span.shift(delta),
        Expr::Null(s) => *s = s.shift(delta),
        Expr::Call(c) => {
            shift_expr(&mut c.callee, delta);
            for t in &mut c.type_args {
                shift_type_ref(t, delta);
            }
            for a in &mut c.args {
                shift_expr(a, delta);
            }
            c.span = c.span.shift(delta);
        }
        Expr::Field(f) => {
            shift_expr(&mut f.object, delta);
            shift_ident(&mut f.field, delta);
            f.span = f.span.shift(delta);
        }
        Expr::Assign(a) => {
            shift_ident(&mut a.name, delta);
            shift_expr(&mut a.value, delta);
            a.span = a.span.shift(delta);
        }
        Expr::Binary(b) => {
            shift_expr(&mut b.left, delta);
            shift_expr(&mut b.right, delta);
            b.span = b.span.shift(delta);
        }
        Expr::Unary(u) => {
            shift_expr(&mut u.expr, delta);
            u.span = u.span.shift(delta);
        }
        Expr::ForceUnwrap(f) => {
            shift_expr(&mut f.expr, delta);
            f.span = f.span.shift(delta);
        }
        Expr::Is(i) => {
            shift_expr(&mut i.expr, delta);
            shift_type_ref(&mut i.ty, delta);
            i.span = i.span.shift(delta);
        }
        Expr::Group(inner, s) => {
            shift_expr(inner, delta);
            *s = s.shift(delta);
        }
        Expr::If(i) => {
            shift_expr(&mut i.cond, delta);
            shift_block(&mut i.then_block, delta);
            shift_block(&mut i.else_block, delta);
            i.span = i.span.shift(delta);
        }
        Expr::Lambda(l) => {
            for p in &mut l.params {
                shift_ident(&mut p.name, delta);
                shift_type_ref(&mut p.ty, delta);
                p.span = p.span.shift(delta);
            }
            shift_expr(&mut l.body, delta);
            l.span = l.span.shift(delta);
        }
    }
}
