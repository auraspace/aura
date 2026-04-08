use std::collections::HashMap;

use aura_ast::{Expr, LetStmt, Stmt};
use aura_diagnostics::Diagnostic;
use aura_span::Span;

use crate::env::Env;
use crate::lib_utils::ident_text;
use crate::types::{
    is_assignable, is_comparable, is_numeric, ty_from_type_ref, unify_numeric, ClassInfo,
    InterfaceInfo, Ty, TypePosition,
};

pub(crate) fn typeck_block(
    source: &str,
    env: &mut Env,
    expected_return: &Ty,
    type_defs: &HashMap<String, crate::types::TyDefKind>,
    classes: &HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    this_class: Option<&ClassInfo>,
    allow_this_assignment: bool,
    block: &aura_ast::Block,
    expr_types: &mut HashMap<Span, Ty>,
    diags: &mut Vec<Diagnostic>,
) {
    env.push_scope();
    for stmt in &block.stmts {
        typeck_stmt(
            source,
            env,
            expected_return,
            type_defs,
            classes,
            interfaces,
            this_class,
            allow_this_assignment,
            stmt,
            expr_types,
            diags,
        );
    }
    env.pop_scope();
}

pub(crate) fn typeck_stmt(
    source: &str,
    env: &mut Env,
    expected_return: &Ty,
    type_defs: &HashMap<String, crate::types::TyDefKind>,
    classes: &HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    this_class: Option<&ClassInfo>,
    allow_this_assignment: bool,
    stmt: &Stmt,
    expr_types: &mut HashMap<Span, Ty>,
    diags: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Let(s) => typeck_let_like(
            source,
            env,
            s,
            true,
            type_defs,
            classes,
            interfaces,
            this_class,
            allow_this_assignment,
            expr_types,
            diags,
        ),
        Stmt::Const(s) => typeck_let_like(
            source,
            env,
            s,
            false,
            type_defs,
            classes,
            interfaces,
            this_class,
            allow_this_assignment,
            expr_types,
            diags,
        ),
        Stmt::Return(s) => match (&s.value, expected_return) {
            (Some(value), Ty::Void) => {
                diags.push(
                    Diagnostic::error(
                        span_of_expr(value),
                        "cannot return a value from a `void` function",
                    )
                    .with_help("remove the value or declare a non-void return type"),
                );
                let _ = typeck_expr(
                    source,
                    env,
                    value,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    expr_types,
                    diags,
                );
            }
            (None, Ty::Void) => {}
            (None, expected) => {
                diags.push(
                    Diagnostic::error(s.span, "missing return value")
                        .with_help(format!("expected `{}`", expected.name())),
                );
            }
            (Some(value), expected) => {
                let value_ty = typeck_expr(
                    source,
                    env,
                    value,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    expr_types,
                    diags,
                );
                if value_ty != Ty::Unknown
                    && *expected != Ty::Unknown
                    && !is_assignable(&value_ty, expected, classes)
                {
                    diags.push(Diagnostic::error(
                        span_of_expr(value),
                        format!(
                            "type mismatch: expected `{}`, got `{}`",
                            expected.name(),
                            value_ty.name()
                        ),
                    ));
                }
            }
        },
        Stmt::Expr(s) => {
            let _ = typeck_expr(
                source,
                env,
                &s.expr,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );
        }
        Stmt::Block(b) => typeck_block(
            source,
            env,
            expected_return,
            type_defs,
            classes,
            interfaces,
            this_class,
            allow_this_assignment,
            b,
            expr_types,
            diags,
        ),
        Stmt::If(s) => {
            let cond_ty = typeck_expr(
                source,
                env,
                &s.cond,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );
            if cond_ty != Ty::Unknown && cond_ty != Ty::Bool {
                diags.push(
                    Diagnostic::error(span_of_expr(&s.cond), "condition must be of type `bool`")
                        .with_help(format!("got `{}`", cond_ty.name())),
                );
            }
            typeck_block(
                source,
                env,
                expected_return,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                &s.then_block,
                expr_types,
                diags,
            );
            if let Some(else_block) = &s.else_block {
                typeck_block(
                    source,
                    env,
                    expected_return,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    else_block,
                    expr_types,
                    diags,
                );
            }
        }
        Stmt::While(s) => {
            let cond_ty = typeck_expr(
                source,
                env,
                &s.cond,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );
            if cond_ty != Ty::Unknown && cond_ty != Ty::Bool {
                diags.push(
                    Diagnostic::error(span_of_expr(&s.cond), "condition must be of type `bool`")
                        .with_help(format!("got `{}`", cond_ty.name())),
                );
            }
            typeck_block(
                source,
                env,
                expected_return,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                &s.body,
                expr_types,
                diags,
            );
        }
        Stmt::Empty(_) => {}
    }
}

pub(crate) fn typeck_expr(
    source: &str,
    env: &mut Env,
    expr: &Expr,
    type_defs: &HashMap<String, crate::types::TyDefKind>,
    classes: &HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    this_class: Option<&ClassInfo>,
    allow_this_assignment: bool,
    expr_types: &mut HashMap<Span, Ty>,
    diags: &mut Vec<Diagnostic>,
) -> Ty {
    let ty = match expr {
        Expr::This(span) => {
            if this_class.is_some() {
                for (name, info) in classes {
                    if std::ptr::eq(info, this_class.unwrap()) {
                        return Ty::Class(name.clone());
                    }
                }
                Ty::Unknown
            } else {
                diags.push(Diagnostic::error(
                    *span,
                    "invalid use of `this` outside of a class method",
                ));
                Ty::Unknown
            }
        }
        Expr::Ident(ident) => {
            let Some(name) = ident_text(source, ident) else {
                return Ty::Unknown;
            };
            env.lookup(name)
                .map(|v| v.ty.clone())
                .unwrap_or(Ty::Unknown)
        }
        Expr::IntLit(_) => Ty::I32,
        Expr::FloatLit(_) => Ty::F64,
        Expr::StringLit(_) => Ty::String,
        Expr::BoolLit(_, _) => Ty::Bool,
        Expr::Paren { expr, .. } => typeck_expr(
            source,
            env,
            expr,
            type_defs,
            classes,
            interfaces,
            this_class,
            allow_this_assignment,
            expr_types,
            diags,
        ),
        Expr::Unary { op, expr, span } => {
            let inner = typeck_expr(
                source,
                env,
                expr,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );
            match op {
                aura_ast::UnaryOp::Neg => {
                    if inner != Ty::Unknown && !is_numeric(&inner) {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!("cannot apply unary `-` to `{}`", inner.name()),
                            )
                            .with_help("expected a numeric type"),
                        );
                        Ty::Unknown
                    } else {
                        inner
                    }
                }
                aura_ast::UnaryOp::Not => {
                    if inner != Ty::Unknown && inner != Ty::Bool {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!("cannot apply unary `!` to `{}`", inner.name()),
                            )
                            .with_help("expected `bool`"),
                        );
                        Ty::Unknown
                    } else {
                        Ty::Bool
                    }
                }
            }
        }
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => {
            let lt = typeck_expr(
                source,
                env,
                left,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );
            let rt = typeck_expr(
                source,
                env,
                right,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );
            if lt == Ty::Unknown || rt == Ty::Unknown {
                return Ty::Unknown;
            }
            match op {
                aura_ast::BinaryOp::Add
                | aura_ast::BinaryOp::Sub
                | aura_ast::BinaryOp::Mul
                | aura_ast::BinaryOp::Div => {
                    if is_numeric(&lt) && is_numeric(&rt) {
                        unify_numeric(&lt, &rt)
                    } else {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!(
                                    "cannot apply arithmetic operator to `{}` and `{}`",
                                    lt.name(),
                                    rt.name()
                                ),
                            )
                            .with_help("expected numeric operands"),
                        );
                        Ty::Unknown
                    }
                }
                aura_ast::BinaryOp::EqEq | aura_ast::BinaryOp::NotEq => {
                    if is_comparable(&lt, &rt) {
                        Ty::Bool
                    } else {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!(
                                    "cannot compare `{}` and `{}` for equality",
                                    lt.name(),
                                    rt.name()
                                ),
                            )
                            .with_help("operands must be the same type (or both numeric)"),
                        );
                        Ty::Unknown
                    }
                }
                aura_ast::BinaryOp::Lt
                | aura_ast::BinaryOp::LtEq
                | aura_ast::BinaryOp::Gt
                | aura_ast::BinaryOp::GtEq => {
                    if is_numeric(&lt) && is_numeric(&rt) {
                        Ty::Bool
                    } else {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!("cannot order-compare `{}` and `{}`", lt.name(), rt.name()),
                            )
                            .with_help("expected numeric operands"),
                        );
                        Ty::Unknown
                    }
                }
                aura_ast::BinaryOp::AndAnd | aura_ast::BinaryOp::OrOr => {
                    if lt == Ty::Bool && rt == Ty::Bool {
                        Ty::Bool
                    } else {
                        diags.push(
                            Diagnostic::error(
                                *span,
                                format!(
                                    "cannot apply boolean operator to `{}` and `{}`",
                                    lt.name(),
                                    rt.name()
                                ),
                            )
                            .with_help("expected `bool` operands"),
                        );
                        Ty::Unknown
                    }
                }
            }
        }
        Expr::Assign {
            target,
            value,
            span,
        } => {
            if let Expr::Member { object, field, .. } = &**target {
                if matches!(**object, Expr::This(_)) {
                    let Some(class_info) = this_class else {
                        diags.push(Diagnostic::error(
                            *span,
                            "invalid assignment to `this` field outside of a class method",
                        ));
                        let _ = typeck_expr(
                            source,
                            env,
                            value,
                            type_defs,
                            classes,
                            interfaces,
                            this_class,
                            allow_this_assignment,
                            expr_types,
                            diags,
                        );
                        return Ty::Unknown;
                    };
                    let Some(field_name) = ident_text(source, field) else {
                        return Ty::Unknown;
                    };
                    if !allow_this_assignment {
                        let value_ty = typeck_expr(
                            source,
                            env,
                            value,
                            type_defs,
                            classes,
                            interfaces,
                            this_class,
                            allow_this_assignment,
                            expr_types,
                            diags,
                        );
                        diags.push(
                            Diagnostic::error(
                                field.span,
                                "assignments to `this.<field>` only belong in constructors",
                            )
                            .with_help("initialize fields inside the class constructor"),
                        );
                        return value_ty;
                    }
                    let Some(field_ty) = class_info.fields.get(field_name) else {
                        diags.push(Diagnostic::error(
                            field.span,
                            format!("unknown field `{field_name}` on `this`"),
                        ));
                        let _ = typeck_expr(
                            source,
                            env,
                            value,
                            type_defs,
                            classes,
                            interfaces,
                            this_class,
                            allow_this_assignment,
                            expr_types,
                            diags,
                        );
                        return Ty::Unknown;
                    };

                    let value_ty = typeck_expr(
                        source,
                        env,
                        value,
                        type_defs,
                        classes,
                        interfaces,
                        this_class,
                        allow_this_assignment,
                        expr_types,
                        diags,
                    );
                    if value_ty != Ty::Unknown
                        && *field_ty != Ty::Unknown
                        && !is_assignable(&value_ty, field_ty, classes)
                    {
                        diags.push(Diagnostic::error(
                            span_of_expr(value),
                            format!(
                                "type mismatch: expected `{}`, got `{}`",
                                field_ty.name(),
                                value_ty.name()
                            ),
                        ));
                    }

                    return value_ty;
                }
            }

            let (target_name, target_span) = match assignment_target(source, target) {
                Some(v) => v,
                None => {
                    diags.push(
                        Diagnostic::error(*span, "invalid assignment target")
                            .with_help("expected an identifier"),
                    );
                    return Ty::Unknown;
                }
            };

            let Some(var) = env.lookup(&target_name) else {
                let _ = typeck_expr(
                    source,
                    env,
                    value,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    expr_types,
                    diags,
                );
                return Ty::Unknown;
            };

            let target_ty = var.ty.clone();
            let target_mutable = var.mutable;

            if !target_mutable {
                diags.push(
                    Diagnostic::error(
                        target_span,
                        format!("cannot assign to `const` binding `{target_name}`"),
                    )
                    .with_help("change `const` to `let` if it should be mutable"),
                );
            }

            let value_ty = typeck_expr(
                source,
                env,
                value,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );
            if value_ty != Ty::Unknown
                && target_ty != Ty::Unknown
                && !is_assignable(&value_ty, &target_ty, classes)
            {
                diags.push(Diagnostic::error(
                    span_of_expr(value),
                    format!(
                        "type mismatch: expected `{}`, got `{}`",
                        target_ty.name(),
                        value_ty.name()
                    ),
                ));
            }

            value_ty
        }
        Expr::Call { callee, args, span } => {
            let (m_methods, _) = match &**callee {
                Expr::Member { object, field, .. } => {
                    let (m_methods, kind_name) = match &**object {
                        Expr::This(_) => (this_class.map(|c| &c.methods), "this".to_string()),
                        other => {
                            let ty = typeck_expr(
                                source,
                                env,
                                other,
                                type_defs,
                                classes,
                                interfaces,
                                this_class,
                                allow_this_assignment,
                                expr_types,
                                diags,
                            );
                            match ty {
                                Ty::Class(ref name) => (
                                    classes.get(name).map(|c| &c.methods),
                                    format!("class `{name}`"),
                                ),
                                Ty::Interface(ref name) => (
                                    interfaces.get(name).map(|i| &i.methods),
                                    format!("interface `{name}`"),
                                ),
                                Ty::Unknown => (None, "unknown".to_string()),
                                _ => {
                                    diags.push(Diagnostic::error(
                                        *span,
                                        "method call target must be a class or interface instance",
                                    ));
                                    (None, ty.name().to_string())
                                }
                            }
                        }
                    };

                    let Some(methods) = m_methods else {
                        return Ty::Unknown;
                    };

                    let Some(field_name) = ident_text(source, field) else {
                        return Ty::Unknown;
                    };

                    let Some(sig) = methods.get(field_name) else {
                        diags.push(Diagnostic::error(
                            field.span,
                            format!("unknown method `{field_name}` on {kind_name}"),
                        ));
                        return Ty::Unknown;
                    };
                    (Some(sig), kind_name)
                }
                _ => {
                    diags.push(
                        Diagnostic::error(*span, "call expressions are not type-checked yet")
                            .with_help("Phase 3 will type-check functions and calls"),
                    );
                    (None, "unknown".to_string())
                }
            };

            let Some(sig) = m_methods else {
                return Ty::Unknown;
            };

            if args.len() != sig.params.len() {
                diags.push(Diagnostic::error(
                    *span,
                    format!(
                        "expected {} argument(s), got {}",
                        sig.params.len(),
                        args.len()
                    ),
                ));
            }

            for (idx, arg) in args.iter().enumerate() {
                let arg_ty = typeck_expr(
                    source,
                    env,
                    arg,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    expr_types,
                    diags,
                );
                if let Some(param_ty) = sig.params.get(idx) {
                    if arg_ty != Ty::Unknown
                        && *param_ty != Ty::Unknown
                        && !is_assignable(&arg_ty, param_ty, classes)
                    {
                        diags.push(Diagnostic::error(
                            span_of_expr(arg),
                            format!(
                                "type mismatch: expected `{}`, got `{}`",
                                param_ty.name(),
                                arg_ty.name()
                            ),
                        ));
                    }
                }
            }

            sig.return_ty.clone()
        }
        Expr::New { class, args, span } => {
            for arg in args {
                let _ = typeck_expr(
                    source,
                    env,
                    arg,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    expr_types,
                    diags,
                );
            }
            let Some(name) = ident_text(source, class) else {
                return Ty::Unknown;
            };
            if !type_defs.contains_key(name) {
                diags.push(Diagnostic::error(*span, format!("unknown class `{name}`")));
                return Ty::Unknown;
            }
            if interfaces.contains_key(name) {
                diags.push(
                    Diagnostic::error(*span, format!("cannot instantiate interface `{name}`"))
                        .with_help("interfaces can only be implemented by classes"),
                );
                return Ty::Unknown;
            }
            Ty::Class(name.to_string())
        }
        Expr::Member {
            object,
            field,
            span,
        } => {
            if matches!(**object, Expr::This(_)) {
                let Some(class_info) = this_class else {
                    diags.push(Diagnostic::error(
                        *span,
                        "invalid use of `this` outside of a class method",
                    ));
                    return Ty::Unknown;
                };
                let Some(field_name) = ident_text(source, field) else {
                    return Ty::Unknown;
                };
                let Some(field_ty) = class_info.fields.get(field_name) else {
                    diags.push(Diagnostic::error(
                        field.span,
                        format!("unknown field `{field_name}` on `this`"),
                    ));
                    return Ty::Unknown;
                };
                field_ty.clone()
            } else {
                diags.push(
                    Diagnostic::error(*span, "member access is not type-checked yet")
                        .with_help("Phase 3 will add class/interface typing"),
                );
                Ty::Unknown
            }
        }
    };

    expr_types.insert(span_of_expr(expr), ty.clone());
    ty
}

pub(crate) fn block_guarantees_return(stmts: &[Stmt]) -> bool {
    for stmt in stmts {
        if stmt_guarantees_return(stmt) {
            return true;
        }
    }
    false
}

fn stmt_guarantees_return(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return(_) => true,
        Stmt::Block(b) => block_guarantees_return(&b.stmts),
        Stmt::If(s) => {
            let Some(else_block) = &s.else_block else {
                return false;
            };
            block_guarantees_return(&s.then_block.stmts)
                && block_guarantees_return(&else_block.stmts)
        }
        Stmt::While(_) => false,
        Stmt::Let(_) | Stmt::Const(_) | Stmt::Expr(_) | Stmt::Empty(_) => false,
    }
}

pub(crate) fn typeck_let_like(
    source: &str,
    env: &mut Env,
    stmt: &LetStmt,
    is_mutable: bool,
    type_defs: &HashMap<String, crate::types::TyDefKind>,
    classes: &HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    this_class: Option<&ClassInfo>,
    allow_this_assignment: bool,
    expr_types: &mut HashMap<Span, Ty>,
    diags: &mut Vec<Diagnostic>,
) {
    let declared_ty = stmt
        .ty
        .as_ref()
        .map(|t| ty_from_type_ref(source, t, TypePosition::Value, type_defs, diags));

    let init_ty = stmt
        .init
        .as_ref()
        .map(|e| {
            typeck_expr(
                source,
                env,
                e,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            )
        })
        .unwrap_or(Ty::Unknown);

    if !is_mutable && stmt.init.is_none() {
        diags.push(
            Diagnostic::error(stmt.span, "`const` bindings must have an initializer")
                .with_help("add `= <expr>` or change `const` to `let`"),
        );
    }

    if declared_ty.is_none() && stmt.init.is_none() {
        diags.push(
            Diagnostic::error(
                stmt.span,
                "variable declaration needs a type annotation or an initializer",
            )
            .with_help("add `: <type>` or `= <expr>`"),
        );
    }

    if let (Some(expected), Some(init)) = (declared_ty.as_ref(), stmt.init.as_ref()) {
        if init_ty != Ty::Unknown
            && *expected != Ty::Unknown
            && *expected != Ty::Void
            && !is_assignable(&init_ty, expected, classes)
        {
            diags.push(
                Diagnostic::error(
                    span_of_expr(init),
                    format!(
                        "type mismatch: expected `{}`, got `{}`",
                        expected.name(),
                        init_ty.name()
                    ),
                )
                .with_help("change the initializer or the declared type"),
            );
        }
    }

    let inferred_ty = declared_ty.clone().unwrap_or(init_ty.clone());

    let Some(name) = ident_text(source, &stmt.name) else {
        return;
    };

    if inferred_ty == Ty::Unknown && stmt.ty.is_none() && stmt.init.is_some() {
        diags.push(
            Diagnostic::error(
                stmt.span,
                format!("cannot infer type of `{name}` from initializer"),
            )
            .with_help("add an explicit type annotation like `: i32`"),
        );
    }

    let info = crate::env::VarInfo {
        ty: inferred_ty,
        mutable: is_mutable,
        decl_span: stmt.name.span,
    };

    if let Some(existing) = env.lookup_mut(name) {
        if existing.ty == Ty::Unknown && info.ty != Ty::Unknown {
            existing.ty = info.ty;
        }
        return;
    }

    env.declare(name.to_string(), info);
}

fn assignment_target(source: &str, expr: &Expr) -> Option<(String, Span)> {
    match expr {
        Expr::Ident(ident) => Some((ident_text(source, ident)?.to_string(), ident.span)),
        Expr::Paren { expr, .. } => assignment_target(source, expr),
        _ => None,
    }
}

pub(crate) fn span_of_expr(expr: &Expr) -> Span {
    match expr {
        Expr::Ident(i) => i.span,
        Expr::This(s) => *s,
        Expr::IntLit(s) => *s,
        Expr::FloatLit(s) => *s,
        Expr::StringLit(s) => *s,
        Expr::BoolLit(_, s) => *s,
        Expr::Unary { span, .. } => *span,
        Expr::Binary { span, .. } => *span,
        Expr::Assign { span, .. } => *span,
        Expr::Call { span, .. } => *span,
        Expr::New { span, .. } => *span,
        Expr::Member { span, .. } => *span,
        Expr::Paren { span, .. } => *span,
    }
}
