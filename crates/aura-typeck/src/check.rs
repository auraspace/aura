use std::collections::HashMap;

use aura_ast::{Expr, LetStmt, Stmt};
use aura_diagnostics::Diagnostic;
use aura_span::Span;

use crate::env::Env;
use crate::lib_utils::ident_text;
use crate::types::{
    is_assignable, is_comparable, is_numeric, is_stringable, ty_from_type_ref, unify_numeric,
    ClassInfo, InterfaceInfo, Ty, TypePosition,
};

pub(crate) fn typeck_block(
    source: &str,
    env: &mut Env,
    expected_return: &Ty,
    type_defs: &HashMap<String, crate::types::TyDefKind>,
    classes: &HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    this_class: Option<&str>,
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
    this_class: Option<&str>,
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
                    diags.push(
                        Diagnostic::error(
                            span_of_expr(value),
                            format!(
                                "type mismatch: expected `{}`, got `{}`",
                                expected.name(),
                                value_ty.name()
                            ),
                        )
                        .with_help("make the value match the expected type"),
                    );
                }
            }
        },
        Stmt::Throw(s) => {
            let _ = typeck_expr(
                source,
                env,
                &s.value,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );
        }
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
        Stmt::Try(s) => {
            typeck_block(
                source,
                env,
                expected_return,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                &s.try_block,
                expr_types,
                diags,
            );

            if let Some(catch) = &s.catch {
                env.push_scope();
                let catch_ty = catch
                    .ty
                    .as_ref()
                    .map(|ty| ty_from_type_ref(source, ty, TypePosition::Value, type_defs, diags))
                    .unwrap_or(Ty::Unknown);
                if let Some(name) = ident_text(source, &catch.binding) {
                    env.declare(
                        name.to_string(),
                        crate::VarInfo {
                            ty: catch_ty.clone(),
                            mutable: false,
                            decl_span: catch.binding.span,
                        },
                    );
                }
                expr_types.insert(catch.binding.span, catch_ty);
                typeck_block(
                    source,
                    env,
                    expected_return,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    &catch.block,
                    expr_types,
                    diags,
                );
                env.pop_scope();
            }

            if let Some(finally_block) = &s.finally_block {
                typeck_block(
                    source,
                    env,
                    expected_return,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    finally_block,
                    expr_types,
                    diags,
                );
            }
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
    this_class: Option<&str>,
    allow_this_assignment: bool,
    expr_types: &mut HashMap<Span, Ty>,
    diags: &mut Vec<Diagnostic>,
) -> Ty {
    let ty = match expr {
        Expr::This(span) => {
            if let Some(cname) = this_class {
                Ty::Class(cname.to_string())
            } else {
                diags.push(Diagnostic::error(
                    *span,
                    "invalid use of `this` outside of a class method",
                ));
                Ty::Unknown
            }
        }
        Expr::Ident(ident) => {
            let name = ident_text(source, ident).unwrap_or("");
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
                Ty::Unknown
            } else {
                match op {
                    aura_ast::BinaryOp::Add
                    | aura_ast::BinaryOp::Sub
                    | aura_ast::BinaryOp::Mul
                    | aura_ast::BinaryOp::Div
                    | aura_ast::BinaryOp::Mod => {
                        if matches!(op, aura_ast::BinaryOp::Add)
                            && (lt == Ty::String || rt == Ty::String)
                        {
                            if (lt == Ty::String || is_stringable(&lt, classes))
                                && (rt == Ty::String || is_stringable(&rt, classes))
                            {
                                Ty::String
                            } else {
                                diags.push(
                                    Diagnostic::error(
                                        *span,
                                        format!(
                                            "cannot concatenate `{}` and `{}`",
                                            lt.name(),
                                            rt.name()
                                        ),
                                    )
                                    .with_help("expected stringable operands"),
                                );
                                Ty::Unknown
                            }
                        } else if matches!(op, aura_ast::BinaryOp::Mod) {
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
                        } else {
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
                                    format!(
                                        "cannot order-compare `{}` and `{}`",
                                        lt.name(),
                                        rt.name()
                                    ),
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
        }
        Expr::Assign {
            target,
            value,
            span,
        } => {
            if let Expr::Member { object, field, .. } = &**target {
                let obj_ty = typeck_expr(
                    source,
                    env,
                    object,
                    type_defs,
                    classes,
                    interfaces,
                    this_class,
                    allow_this_assignment,
                    expr_types,
                    diags,
                );

                let field_name = ident_text(source, field).unwrap_or("");
                let field_ty = match obj_ty {
                    Ty::Class(ref name) => {
                        let info = classes.get(name);
                        if let Some(info) = info {
                            if matches!(**object, Expr::This(_)) && !allow_this_assignment {
                                diags.push(
                                    Diagnostic::error(
                                        field.span,
                                        "assignments to `this.<field>` only belong in constructors",
                                    )
                                    .with_help("initialize fields inside the class constructor"),
                                );
                            }
                            info.fields.get(field_name).cloned()
                        } else {
                            None
                        }
                    }
                    Ty::Unknown => None,
                    _ => {
                        diags.push(Diagnostic::error(
                            span_of_expr(object),
                            format!(
                                "cannot access field `{field_name}` on type `{}`",
                                obj_ty.name()
                            ),
                        ));
                        None
                    }
                };

                // Record the type of the target member access if we can
                expr_types.insert(field.span, field_ty.clone().unwrap_or(Ty::Unknown));
                expr_types.insert(
                    span_of_expr(target),
                    field_ty.clone().unwrap_or(Ty::Unknown),
                );

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

                if let Some(field_ty) = field_ty {
                    if value_ty != Ty::Unknown
                        && field_ty != Ty::Unknown
                        && !is_assignable(&value_ty, &field_ty, classes)
                    {
                        diags.push(
                            Diagnostic::error(
                                span_of_expr(value),
                                format!(
                                    "type mismatch: expected `{}`, got `{}`",
                                    field_ty.name(),
                                    value_ty.name()
                                ),
                            )
                            .with_help("make the value match the expected type"),
                        );
                    }
                    value_ty
                } else {
                    if obj_ty != Ty::Unknown {
                        diags.push(Diagnostic::error(
                            field.span,
                            format!("unknown field `{field_name}` on type `{}`", obj_ty.name()),
                        ));
                    }
                    value_ty
                }
            } else if let Some((target_name, target_span)) = assignment_target(source, target) {
                if let Some(var) = env.lookup(&target_name) {
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
                        diags.push(
                            Diagnostic::error(
                                span_of_expr(value),
                                format!(
                                    "type mismatch: expected `{}`, got `{}`",
                                    target_ty.name(),
                                    value_ty.name()
                                ),
                            )
                            .with_help("make the value match the expected type"),
                        );
                    }
                    value_ty
                } else {
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
                    Ty::Unknown
                }
            } else {
                diags.push(
                    Diagnostic::error(*span, "invalid assignment target")
                        .with_help("expected an identifier"),
                );
                Ty::Unknown
            }
        }
        Expr::Call { callee, args, span } => {
            let (sig, _kind_name, callee_ty) = match &**callee {
                Expr::Member { object, field, .. } => {
                    let obj_ty = typeck_expr(
                        source,
                        env,
                        object,
                        type_defs,
                        classes,
                        interfaces,
                        this_class,
                        allow_this_assignment,
                        expr_types,
                        diags,
                    );
                    let (methods, kind_name) = match obj_ty {
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
                            (None, obj_ty.name().to_string())
                        }
                    };

                    let field_name = ident_text(source, field).unwrap_or("");
                    let sig = methods.and_then(|m| m.get(field_name)).cloned();
                    let callee_ty = sig
                        .as_ref()
                        .map(|s| Ty::Function(Box::new(s.clone())))
                        .unwrap_or(Ty::Unknown);

                    if sig.is_none() && methods.is_some() {
                        diags.push(Diagnostic::error(
                            field.span,
                            format!("unknown method `{field_name}` on {kind_name}"),
                        ));
                    }

                    // Record callee type (the member access part)
                    expr_types.insert(field.span, callee_ty.clone());

                    (sig, kind_name, callee_ty)
                }
                Expr::Ident(ident) => {
                    let name = ident_text(source, ident).unwrap_or("");
                    let mut callee_ty = Ty::Unknown;
                    let sig = match env.lookup(name) {
                        Some(info) => {
                            callee_ty = info.ty.clone();
                            if let Ty::Function(sig) = &info.ty {
                                Some((**sig).clone())
                            } else {
                                diags.push(Diagnostic::error(
                                    ident.span,
                                    format!("`{name}` is not a function"),
                                ));
                                None
                            }
                        }
                        None => {
                            diags.push(Diagnostic::error(
                                ident.span,
                                format!("unknown function `{name}`"),
                            ));
                            None
                        }
                    };
                    (sig, format!("function `{name}`"), callee_ty)
                }
                _ => {
                    diags.push(
                        Diagnostic::error(*span, "call expressions are not type-checked yet")
                            .with_help("Phase 3 will type-check functions and calls"),
                    );
                    (None, "unknown".to_string(), Ty::Unknown)
                }
            };

            // Record the callee's type in expr_types (e.g. for the ident or the whole member expr)
            expr_types.insert(span_of_expr(callee), callee_ty);

            if let Some(sig) = sig {
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
                            diags.push(
                                Diagnostic::error(
                                    span_of_expr(arg),
                                    format!(
                                        "type mismatch: expected `{}`, got `{}`",
                                        param_ty.name(),
                                        arg_ty.name()
                                    ),
                                )
                                .with_help("make the value match the expected type"),
                            );
                        }
                    }
                }

                sig.return_ty.clone()
            } else {
                Ty::Unknown
            }
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
            if let Some(name) = ident_text(source, class) {
                if !type_defs.contains_key(name) {
                    diags.push(Diagnostic::error(*span, format!("unknown class `{name}`")));
                    Ty::Unknown
                } else if interfaces.contains_key(name) {
                    diags.push(
                        Diagnostic::error(*span, format!("cannot instantiate interface `{name}`"))
                            .with_help("interfaces can only be implemented by classes"),
                    );
                    Ty::Unknown
                } else {
                    Ty::Class(name.to_string())
                }
            } else {
                Ty::Unknown
            }
        }
        Expr::Member {
            object,
            field,
            span,
        } => {
            let obj_ty = typeck_expr(
                source,
                env,
                object,
                type_defs,
                classes,
                interfaces,
                this_class,
                allow_this_assignment,
                expr_types,
                diags,
            );

            let field_name = ident_text(source, field).unwrap_or("");
            let ty = match obj_ty {
                Ty::Class(ref name) => {
                    let info = classes.get(name);
                    if let Some(info) = info {
                        if let Some(field_ty) = info.fields.get(field_name) {
                            field_ty.clone()
                        } else {
                            diags.push(Diagnostic::error(
                                field.span,
                                format!("unknown field `{field_name}` on class `{name}`"),
                            ));
                            Ty::Unknown
                        }
                    } else {
                        Ty::Unknown
                    }
                }
                Ty::Unknown => Ty::Unknown,
                _ => {
                    diags.push(Diagnostic::error(
                        *span,
                        format!(
                            "cannot access field `{field_name}` on type `{}`",
                            obj_ty.name()
                        ),
                    ));
                    Ty::Unknown
                }
            };
            // Record the field's type
            expr_types.insert(field.span, ty.clone());
            ty
        }
    };

    expr_types.insert(span_of_expr(expr), ty.clone());
    ty
}

fn typeck_let_like(
    source: &str,
    env: &mut Env,
    s: &LetStmt,
    mutable: bool,
    type_defs: &HashMap<String, crate::types::TyDefKind>,
    classes: &HashMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
    this_class: Option<&str>,
    allow_this_assignment: bool,
    expr_types: &mut HashMap<Span, Ty>,
    diags: &mut Vec<Diagnostic>,
) {
    let name = match ident_text(source, &s.name) {
        Some(v) => v,
        None => return,
    };

    let mut expected_ty = Ty::Unknown;
    if let Some(ann) = &s.ty {
        expected_ty = ty_from_type_ref(source, ann, TypePosition::Value, type_defs, diags);
    }

    let mut value_ty = Ty::Unknown;
    if let Some(value) = &s.init {
        value_ty = typeck_expr(
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

    let final_ty = if expected_ty != Ty::Unknown {
        if value_ty != Ty::Unknown && !is_assignable(&value_ty, &expected_ty, classes) {
            diags.push(
                Diagnostic::error(
                    s.init.as_ref().map(span_of_expr).unwrap_or(s.span),
                    format!(
                        "type mismatch: expected `{}`, got `{}`",
                        expected_ty.name(),
                        value_ty.name()
                    ),
                )
                .with_help("make the value match the expected type"),
            );
        }
        expected_ty
    } else {
        if value_ty == Ty::Unknown && s.init.is_some() {
            // Already reported or unknown due to errors
        } else if value_ty == Ty::Unknown {
            diags.push(Diagnostic::error(
                s.span,
                format!("binding `{name}` needs a type annotation or an initializer"),
            ));
        }
        value_ty
    };

    expr_types.insert(s.span, final_ty.clone());
    expr_types.insert(s.name.span, final_ty.clone());
    env.define(name.to_string(), final_ty, mutable);
}

fn assignment_target(source: &str, expr: &Expr) -> Option<(String, Span)> {
    match expr {
        Expr::Ident(ident) => ident_text(source, ident).map(|s| (s.to_string(), ident.span)),
        _ => None,
    }
}

pub(crate) fn span_of_expr(expr: &Expr) -> Span {
    match expr {
        Expr::This(span) => *span,
        Expr::Ident(ident) => ident.span,
        Expr::IntLit(span) => *span,
        Expr::FloatLit(span) => *span,
        Expr::StringLit(span) => *span,
        Expr::BoolLit(_, span) => *span,
        Expr::Paren { span, .. } => *span,
        Expr::Unary { span, .. } => *span,
        Expr::Binary { span, .. } => *span,
        Expr::Call { span, .. } => *span,
        Expr::Assign { span, .. } => *span,
        Expr::New { span, .. } => *span,
        Expr::Member { span, .. } => *span,
    }
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
            let then_ret = block_guarantees_return(&s.then_block.stmts);
            let else_ret = s
                .else_block
                .as_ref()
                .map(|b| block_guarantees_return(&b.stmts))
                .unwrap_or(false);
            then_ret && else_ret
        }
        Stmt::Try(s) => {
            if let Some(finally_block) = &s.finally_block {
                if block_guarantees_return(&finally_block.stmts) {
                    true
                } else {
                    let try_ret = block_guarantees_return(&s.try_block.stmts);
                    let catch_ret = s
                        .catch
                        .as_ref()
                        .map(|catch| block_guarantees_return(&catch.block.stmts))
                        .unwrap_or(false);
                    try_ret && catch_ret
                }
            } else {
                let try_ret = block_guarantees_return(&s.try_block.stmts);
                let catch_ret = s
                    .catch
                    .as_ref()
                    .map(|catch| block_guarantees_return(&catch.block.stmts))
                    .unwrap_or(false);
                try_ret && catch_ret
            }
        }
        _ => false,
    }
}
