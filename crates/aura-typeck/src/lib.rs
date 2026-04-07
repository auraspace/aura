use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use aura_ast::{Expr, Ident, LetStmt, Program, Stmt, TopLevel, TypeRef};
use aura_diagnostics::Diagnostic;
use aura_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Unknown,
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    Void,
    Class(String),
}

impl Ty {
    pub fn name(&self) -> Cow<'_, str> {
        match self {
            Ty::Unknown => Cow::Borrowed("<unknown>"),
            Ty::I32 => Cow::Borrowed("i32"),
            Ty::I64 => Cow::Borrowed("i64"),
            Ty::F32 => Cow::Borrowed("f32"),
            Ty::F64 => Cow::Borrowed("f64"),
            Ty::Bool => Cow::Borrowed("bool"),
            Ty::String => Cow::Borrowed("string"),
            Ty::Void => Cow::Borrowed("void"),
            Ty::Class(name) => Cow::Owned(name.clone()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypePosition {
    Value,
    Return,
}

pub fn typeck_program(source: &str, program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    let mut class_names = HashSet::<String>::new();
    for item in &program.items {
        let TopLevel::Class(class_decl) = item else {
            continue;
        };
        if let Some(name) = ident_text(source, &class_decl.name) {
            class_names.insert(name.to_string());
        }
    }

    let mut classes = HashMap::<String, ClassInfo>::new();
    for item in &program.items {
        let TopLevel::Class(class_decl) = item else {
            continue;
        };
        let Some(name) = ident_text(source, &class_decl.name) else {
            continue;
        };
        let mut fields = HashMap::<String, Ty>::new();
        for field in &class_decl.fields {
            let Some(field_name) = ident_text(source, &field.name) else {
                continue;
            };
            let ty = ty_from_type_ref(
                source,
                &field.ty,
                TypePosition::Value,
                &class_names,
                &mut diags,
            );
            fields.entry(field_name.to_string()).or_insert(ty);
        }
        let mut methods = HashMap::<String, MethodSig>::new();
        for method in &class_decl.methods {
            let Some(method_name) = ident_text(source, &method.name) else {
                continue;
            };
            let mut params = Vec::new();
            for param in &method.params {
                let ty = ty_from_type_ref(
                    source,
                    &param.ty,
                    TypePosition::Value,
                    &class_names,
                    &mut diags,
                );
                params.push(ty);
            }
            let return_ty = method
                .return_type
                .as_ref()
                .map(|t| {
                    ty_from_type_ref(source, t, TypePosition::Return, &class_names, &mut diags)
                })
                .unwrap_or(Ty::Void);
            methods
                .entry(method_name.to_string())
                .or_insert(MethodSig { params, return_ty });
        }
        classes
            .entry(name.to_string())
            .or_insert(ClassInfo { fields, methods });
    }

    // Pass 1: predeclare all top-level `let/const` so later type checking can resolve
    // global names regardless of declaration order (matching resolver behavior).
    let mut globals = HashMap::<String, VarInfo>::new();
    for item in &program.items {
        let TopLevel::Stmt(Stmt::Let(s) | Stmt::Const(s)) = item else {
            continue;
        };
        let Some(name) = ident_text(source, &s.name) else {
            continue;
        };
        globals.entry(name.to_string()).or_insert_with(|| VarInfo {
            // Defer annotation checking to Pass 2 to avoid duplicate diagnostics.
            ty: Ty::Unknown,
            mutable: matches!(item, TopLevel::Stmt(Stmt::Let(_))),
            decl_span: s.name.span,
        });
    }

    let mut global_env = Env::new(globals);

    // Pass 2: type-check all top-level statements (including global var initializers).
    for item in &program.items {
        let TopLevel::Stmt(stmt) = item else { continue };
        let expected = Ty::Void;
        typeck_stmt(
            source,
            &mut global_env,
            &expected,
            &class_names,
            &classes,
            None,
            stmt,
            &mut diags,
        );
    }

    // Pass 3: type-check function signatures and bodies.
    for item in &program.items {
        let TopLevel::Function(func) = item else {
            continue;
        };
        if let Some(ret) = &func.return_type {
            let _ = ty_from_type_ref(source, ret, TypePosition::Return, &class_names, &mut diags);
        }

        let mut env = global_env.child();
        for param in &func.params {
            let Some(name) = ident_text(source, &param.name) else {
                continue;
            };
            let ty = ty_from_type_ref(
                source,
                &param.ty,
                TypePosition::Value,
                &class_names,
                &mut diags,
            );
            env.declare(
                name.to_string(),
                VarInfo {
                    ty,
                    mutable: true,
                    decl_span: param.name.span,
                },
            );
        }

        let expected_return = func
            .return_type
            .as_ref()
            .map(|t| ty_from_type_ref(source, t, TypePosition::Return, &class_names, &mut diags))
            .unwrap_or(Ty::Void);

        typeck_block(
            source,
            &mut env,
            &expected_return,
            &class_names,
            &classes,
            None,
            &func.body,
            &mut diags,
        );

        if expected_return != Ty::Void && !block_guarantees_return(&func.body.stmts) {
            diags.push(
                Diagnostic::error(func.span, "missing return on some paths")
                    .with_help("add a `return` statement on all control-flow paths"),
            );
        }
    }

    // Pass 4: type-check class methods (field access via `this`).
    for item in &program.items {
        let TopLevel::Class(class_decl) = item else {
            continue;
        };
        let class_name = ident_text(source, &class_decl.name);
        let class_info = class_name.and_then(|name| classes.get(name));
        for method in &class_decl.methods {
            if let Some(ret) = &method.return_type {
                let _ =
                    ty_from_type_ref(source, ret, TypePosition::Return, &class_names, &mut diags);
            }

            let mut env = global_env.child();
            for param in &method.params {
                let Some(name) = ident_text(source, &param.name) else {
                    continue;
                };
                let ty = ty_from_type_ref(
                    source,
                    &param.ty,
                    TypePosition::Value,
                    &class_names,
                    &mut diags,
                );
                env.declare(
                    name.to_string(),
                    VarInfo {
                        ty,
                        mutable: true,
                        decl_span: param.name.span,
                    },
                );
            }

            let expected_return = method
                .return_type
                .as_ref()
                .map(|t| {
                    ty_from_type_ref(source, t, TypePosition::Return, &class_names, &mut diags)
                })
                .unwrap_or(Ty::Void);

            typeck_block(
                source,
                &mut env,
                &expected_return,
                &class_names,
                &classes,
                class_info,
                &method.body,
                &mut diags,
            );

            if expected_return != Ty::Void && !block_guarantees_return(&method.body.stmts) {
                diags.push(
                    Diagnostic::error(method.span, "missing return on some paths")
                        .with_help("add a `return` statement on all control-flow paths"),
                );
            }
        }
    }

    diags
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VarInfo {
    ty: Ty,
    mutable: bool,
    decl_span: Span,
}

#[derive(Clone, Debug)]
struct ClassInfo {
    fields: HashMap<String, Ty>,
    methods: HashMap<String, MethodSig>,
}

#[derive(Clone, Debug)]
struct MethodSig {
    params: Vec<Ty>,
    return_ty: Ty,
}

#[derive(Clone, Debug)]
struct Env {
    scopes: Vec<HashMap<String, VarInfo>>,
}

impl Env {
    fn new(globals: HashMap<String, VarInfo>) -> Self {
        Self {
            scopes: vec![globals],
        }
    }

    fn child(&self) -> Self {
        Self {
            scopes: vec![self.scopes[0].clone()],
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn declare(&mut self, name: String, info: VarInfo) {
        let current = self.scopes.last_mut().unwrap();
        current.entry(name).or_insert(info);
    }

    fn lookup(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v);
            }
        }
        None
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut VarInfo> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                return scope.get_mut(name);
            }
        }
        None
    }
}

fn typeck_block(
    source: &str,
    env: &mut Env,
    expected_return: &Ty,
    class_names: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    this_class: Option<&ClassInfo>,
    block: &aura_ast::Block,
    diags: &mut Vec<Diagnostic>,
) {
    env.push_scope();
    for stmt in &block.stmts {
        typeck_stmt(
            source,
            env,
            expected_return,
            class_names,
            classes,
            this_class,
            stmt,
            diags,
        );
    }
    env.pop_scope();
}

fn typeck_stmt(
    source: &str,
    env: &mut Env,
    expected_return: &Ty,
    class_names: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    this_class: Option<&ClassInfo>,
    stmt: &Stmt,
    diags: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Let(s) => typeck_let_like(
            source,
            env,
            s,
            true,
            class_names,
            classes,
            this_class,
            diags,
        ),
        Stmt::Const(s) => typeck_let_like(
            source,
            env,
            s,
            false,
            class_names,
            classes,
            this_class,
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
                let _ = typeck_expr(source, env, value, class_names, classes, this_class, diags);
            }
            (None, Ty::Void) => {}
            (None, expected) => {
                diags.push(
                    Diagnostic::error(s.span, "missing return value")
                        .with_help(format!("expected `{}`", expected.name())),
                );
            }
            (Some(value), expected) => {
                let value_ty =
                    typeck_expr(source, env, value, class_names, classes, this_class, diags);
                if value_ty != Ty::Unknown
                    && *expected != Ty::Unknown
                    && !is_assignable(&value_ty, expected)
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
                class_names,
                classes,
                this_class,
                diags,
            );
        }
        Stmt::Block(b) => typeck_block(
            source,
            env,
            expected_return,
            class_names,
            classes,
            this_class,
            b,
            diags,
        ),
        Stmt::If(s) => {
            let cond_ty = typeck_expr(
                source,
                env,
                &s.cond,
                class_names,
                classes,
                this_class,
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
                class_names,
                classes,
                this_class,
                &s.then_block,
                diags,
            );
            if let Some(else_block) = &s.else_block {
                typeck_block(
                    source,
                    env,
                    expected_return,
                    class_names,
                    classes,
                    this_class,
                    else_block,
                    diags,
                );
            }
        }
        Stmt::While(s) => {
            let cond_ty = typeck_expr(
                source,
                env,
                &s.cond,
                class_names,
                classes,
                this_class,
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
                class_names,
                classes,
                this_class,
                &s.body,
                diags,
            );
        }
        Stmt::Empty(_) => {}
    }
}

fn block_guarantees_return(stmts: &[Stmt]) -> bool {
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

fn typeck_let_like(
    source: &str,
    env: &mut Env,
    stmt: &LetStmt,
    is_mutable: bool,
    class_names: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    this_class: Option<&ClassInfo>,
    diags: &mut Vec<Diagnostic>,
) {
    let declared_ty = stmt
        .ty
        .as_ref()
        .map(|t| ty_from_type_ref(source, t, TypePosition::Value, class_names, diags));

    // Like the resolver, evaluate the initializer before the binding is in scope.
    let init_ty = stmt
        .init
        .as_ref()
        .map(|e| typeck_expr(source, env, e, class_names, classes, this_class, diags))
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
            && !is_assignable(&init_ty, expected)
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

    let info = VarInfo {
        ty: inferred_ty,
        mutable: is_mutable,
        decl_span: stmt.name.span,
    };

    // If already declared (e.g. top-level predecl), update its type when we learn it.
    if let Some(existing) = env.lookup_mut(name) {
        if existing.ty == Ty::Unknown && info.ty != Ty::Unknown {
            existing.ty = info.ty;
        }
        return;
    }

    env.declare(name.to_string(), info);
}

fn typeck_expr(
    source: &str,
    env: &mut Env,
    expr: &Expr,
    class_names: &HashSet<String>,
    classes: &HashMap<String, ClassInfo>,
    this_class: Option<&ClassInfo>,
    diags: &mut Vec<Diagnostic>,
) -> Ty {
    match expr {
        Expr::This(_) => Ty::Unknown,
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
        Expr::Paren { expr, .. } => {
            typeck_expr(source, env, expr, class_names, classes, this_class, diags)
        }
        Expr::Unary { op, expr, span } => {
            let inner = typeck_expr(source, env, expr, class_names, classes, this_class, diags);
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
            let lt = typeck_expr(source, env, left, class_names, classes, this_class, diags);
            let rt = typeck_expr(source, env, right, class_names, classes, this_class, diags);
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
                            class_names,
                            classes,
                            this_class,
                            diags,
                        );
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
                        let _ = typeck_expr(
                            source,
                            env,
                            value,
                            class_names,
                            classes,
                            this_class,
                            diags,
                        );
                        return Ty::Unknown;
                    };

                    let value_ty =
                        typeck_expr(source, env, value, class_names, classes, this_class, diags);
                    if value_ty != Ty::Unknown
                        && *field_ty != Ty::Unknown
                        && !is_assignable(&value_ty, field_ty)
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
                // Resolver will report unknown identifiers; keep this as Unknown for now.
                let _ = typeck_expr(source, env, value, class_names, classes, this_class, diags);
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

            let value_ty = typeck_expr(source, env, value, class_names, classes, this_class, diags);
            if value_ty != Ty::Unknown
                && target_ty != Ty::Unknown
                && !is_assignable(&value_ty, &target_ty)
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
            if let Expr::Member { object, field, .. } = &**callee {
                let mut class_name: Option<String> = None;
                let class_info = match &**object {
                    Expr::This(_) => {
                        let Some(class_info) = this_class else {
                            diags.push(Diagnostic::error(
                                *span,
                                "invalid use of `this` outside of a class method",
                            ));
                            return Ty::Unknown;
                        };
                        Some(class_info)
                    }
                    other => match typeck_expr(
                        source,
                        env,
                        other,
                        class_names,
                        classes,
                        this_class,
                        diags,
                    ) {
                        Ty::Class(name) => {
                            class_name = Some(name.clone());
                            classes.get(&name)
                        }
                        Ty::Unknown => None,
                        _ => {
                            diags.push(Diagnostic::error(
                                *span,
                                "method call target must be a class instance",
                            ));
                            return Ty::Unknown;
                        }
                    },
                };

                let Some(field_name) = ident_text(source, field) else {
                    return Ty::Unknown;
                };
                let Some(class_info) = class_info else {
                    if let Some(name) = class_name {
                        diags.push(Diagnostic::error(*span, format!("unknown class `{name}`")));
                    }
                    return Ty::Unknown;
                };
                let Some(sig) = class_info.methods.get(field_name) else {
                    diags.push(Diagnostic::error(
                        field.span,
                        format!("unknown method `{field_name}`"),
                    ));
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
                    let arg_ty =
                        typeck_expr(source, env, arg, class_names, classes, this_class, diags);
                    if let Some(param_ty) = sig.params.get(idx) {
                        if arg_ty != Ty::Unknown
                            && *param_ty != Ty::Unknown
                            && !is_assignable(&arg_ty, param_ty)
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
            } else {
                diags.push(
                    Diagnostic::error(*span, "call expressions are not type-checked yet")
                        .with_help("Phase 3 will type-check functions and calls"),
                );
                Ty::Unknown
            }
        }
        Expr::New { class, args, span } => {
            for arg in args {
                let _ = typeck_expr(source, env, arg, class_names, classes, this_class, diags);
            }
            let Some(name) = ident_text(source, class) else {
                return Ty::Unknown;
            };
            if !class_names.contains(name) {
                diags.push(Diagnostic::error(*span, format!("unknown class `{name}`")));
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
    }
}

fn assignment_target(source: &str, expr: &Expr) -> Option<(String, Span)> {
    match expr {
        Expr::Ident(ident) => Some((ident_text(source, ident)?.to_string(), ident.span)),
        Expr::Paren { expr, .. } => assignment_target(source, expr),
        _ => None,
    }
}

fn span_of_expr(expr: &Expr) -> Span {
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

fn is_numeric(ty: &Ty) -> bool {
    matches!(ty, Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64)
}

fn unify_numeric(a: &Ty, b: &Ty) -> Ty {
    match (a, b) {
        (Ty::F64, _) | (_, Ty::F64) => Ty::F64,
        (Ty::F32, _) | (_, Ty::F32) => Ty::F32,
        (Ty::I64, _) | (_, Ty::I64) => Ty::I64,
        _ => Ty::I32,
    }
}

fn is_comparable(a: &Ty, b: &Ty) -> bool {
    a == b || (is_numeric(a) && is_numeric(b))
}

fn is_assignable(from: &Ty, to: &Ty) -> bool {
    if *from == Ty::Unknown || *to == Ty::Unknown {
        return true;
    }
    if from == to {
        return true;
    }

    match (from, to) {
        (Ty::I32, Ty::I64) => true,
        (Ty::I32, Ty::F32) | (Ty::I32, Ty::F64) => true,
        (Ty::I64, Ty::F64) | (Ty::I64, Ty::F32) => true,
        (Ty::F32, Ty::F64) => true,
        _ => false,
    }
}

fn ty_from_type_ref(
    source: &str,
    ty: &TypeRef,
    pos: TypePosition,
    class_names: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) -> Ty {
    let Some(name) = ident_text(source, &ty.name) else {
        diags.push(Diagnostic::error(ty.span, "invalid type reference"));
        return Ty::Unknown;
    };

    if let Some(builtin) = parse_builtin_ty(name) {
        if builtin == Ty::Void && pos != TypePosition::Return {
            diags.push(
                Diagnostic::error(
                    ty.span,
                    "type `void` is only valid as a function return type",
                )
                .with_help("remove the annotation or use a non-void type"),
            );
        }
        return builtin;
    }

    if class_names.contains(name) {
        return Ty::Class(name.to_string());
    }

    diags.push(
        Diagnostic::error(ty.span, format!("unknown type `{name}`"))
            .with_help("built-in types: i32, i64, f32, f64, bool, string, void"),
    );
    Ty::Unknown
}

fn parse_builtin_ty(name: &str) -> Option<Ty> {
    match name {
        "i32" => Some(Ty::I32),
        "i64" => Some(Ty::I64),
        "f32" => Some(Ty::F32),
        "f64" => Some(Ty::F64),
        "bool" => Some(Ty::Bool),
        "string" => Some(Ty::String),
        "void" => Some(Ty::Void),
        _ => None,
    }
}

fn ident_text<'a>(source: &'a str, ident: &Ident) -> Option<&'a str> {
    let start = ident.span.start.raw() as usize;
    let end = ident.span.end.raw() as usize;
    source.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_builtin_types() {
        let src = r#"
function f(a: i32, b: string): void { return; }
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn reports_unknown_type() {
        let src = r#"
function f(a: Foo): i32 { return 0; }
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unknown type"));
    }

    #[test]
    fn rejects_void_outside_return_position() {
        let src = r#"
function f(a: void): i32 { return 0; }
let x: void = 0;
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 2, "{diags:#?}");
        assert!(diags
            .iter()
            .all(|d| d.message.contains("only valid as a function return type")));
    }

    #[test]
    fn infers_let_type_from_initializer_and_allows_assignment() {
        let src = r#"
function main(): void {
  let x = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_missing_type_and_initializer() {
        let src = r#"
function main(): void {
  let x;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0]
            .message
            .contains("type annotation or an initializer"));
    }

    #[test]
    fn allows_widening_assignment_to_annotated_type() {
        let src = r#"
function main(): void {
  let x: i64 = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_type_mismatch_in_initializer() {
        let src = r#"
function main(): void {
  let x: i32 = 1.0;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("type mismatch"));
    }

    #[test]
    fn rejects_assignment_to_const() {
        let src = r#"
function main(): void {
  const x: i32 = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("cannot assign to `const`"));
    }

    #[test]
    fn rejects_missing_return_on_some_paths() {
        let src = r#"
function f(): i32 {
  if (true) { return 1; }
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("missing return"));
    }

    #[test]
    fn rejects_return_type_mismatch() {
        let src = r#"
function f(): i32 {
  return true;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("type mismatch"));
    }

    #[test]
    fn rejects_return_value_in_void_function() {
        let src = r#"
function f(): void {
  return 1;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let diags = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("cannot return a value"));
    }
}
