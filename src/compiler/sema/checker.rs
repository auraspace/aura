use crate::compiler::ast::{Expr, Program, Span, Statement, TypeExpr};
use crate::compiler::frontend::error::{Diagnostic, DiagnosticList};
use crate::compiler::sema::scope::Scope;
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

pub struct ClassInfo {
    pub name: String,
    pub fields: HashMap<String, Type>,
    pub static_fields: HashMap<String, Type>,
    pub methods: HashMap<String, (Vec<Type>, Type, Option<String>)>, // name -> (params, return_ty, doc)
    pub static_methods: HashMap<String, (Vec<Type>, Type, Option<String>)>,
    pub span: Span,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SemanticErrorKind {
    UndefinedVariable(String),
    UndefinedClass(String),
    UndefinedMethod(String, String),
    UndefinedField(String, String),
    TypeMismatch(String, String), // expected, found
    IncompatibleBinaryOperators(String, String, String), // left_ty, op, right_ty
    DuplicateDeclaration(String),
    WrongArgumentCount(String, usize, usize), // name, expected, found
    NotAClass(String),
}

#[derive(Debug, Clone)]
pub struct SemanticError {
    pub kind: SemanticErrorKind,
    pub span: Span,
}

pub struct SemanticAnalyzer {
    pub scope: Box<Scope>,
    pub classes: HashMap<String, ClassInfo>,
    pub current_class: Option<String>,
    pub diagnostics: DiagnosticList,
    pub node_types: HashMap<Span, Type>,
    pub node_definitions: HashMap<Span, Span>,
    pub node_docs: HashMap<Span, String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            scope: Box::new(Scope::new(None)),
            classes: HashMap::new(),
            current_class: None,
            diagnostics: DiagnosticList::new(),
            node_types: HashMap::new(),
            node_definitions: HashMap::new(),
            node_docs: HashMap::new(),
        }
    }

    fn error(&mut self, kind: SemanticErrorKind, span: Span) {
        let msg = match &kind {
            SemanticErrorKind::UndefinedVariable(n) => format!("Undefined variable: {}", n),
            SemanticErrorKind::UndefinedClass(n) => format!("Undefined class: {}", n),
            SemanticErrorKind::UndefinedMethod(c, m) => {
                format!("Method {} not found in class {}", m, c)
            }
            SemanticErrorKind::UndefinedField(c, f) => {
                format!("Field {} not found in class {}", f, c)
            }
            SemanticErrorKind::TypeMismatch(e, f) => {
                format!("Type mismatch: expected {}, found {}", e, f)
            }
            SemanticErrorKind::IncompatibleBinaryOperators(l, op, r) => {
                format!("Incompatible types for operator {}: {} and {}", op, l, r)
            }
            SemanticErrorKind::DuplicateDeclaration(n) => format!("Duplicate declaration: {}", n),
            SemanticErrorKind::WrongArgumentCount(n, e, f) => {
                format!(
                    "Wrong argument count for {}: expected {}, found {}",
                    n, e, f
                )
            }
            SemanticErrorKind::NotAClass(t) => format!("Type {} is not a class", t),
        };
        self.diagnostics
            .push(Diagnostic::error(msg, span.line, span.column));
    }

    pub fn analyze(&mut self, program: Program) {
        // Pass 1: Collect class info from current program
        self.collect_classes(&program);

        // Pass 2: Check statements
        for stmt in program.statements {
            self.check_statement(stmt);
        }
    }

    pub fn collect_classes(&mut self, program: &Program) {
        for stmt in &program.statements {
            if let Statement::ClassDeclaration {
                name,
                fields,
                methods,
                constructor: _,
                span,
                doc,
            } = stmt
            {
                let mut field_map = HashMap::new();
                let mut static_field_map = HashMap::new();
                for f in fields {
                    let ty = self.resolve_type(f.ty.clone());
                    if f.is_static {
                        static_field_map.insert(f.name.clone(), ty);
                    } else {
                        field_map.insert(f.name.clone(), ty);
                    }
                }
                let mut method_map = HashMap::new();
                let mut static_method_map = HashMap::new();
                for m in methods {
                    let param_tys = m
                        .params
                        .iter()
                        .map(|(_, ty)| self.resolve_type(ty.clone()))
                        .collect();
                    let ret_ty = self.resolve_type(m.return_ty.clone());
                    if m.is_static {
                        static_method_map
                            .insert(m.name.clone(), (param_tys, ret_ty, m.doc.clone()));
                    } else {
                        method_map.insert(m.name.clone(), (param_tys, ret_ty, m.doc.clone()));
                    }
                }
                self.classes.insert(
                    name.clone(),
                    ClassInfo {
                        name: name.clone(),
                        fields: field_map,
                        static_fields: static_field_map,
                        methods: method_map,
                        static_methods: static_method_map,
                        span: *span,
                        doc: doc.clone(),
                    },
                );
            }
        }
    }

    pub fn load_stdlib(&mut self) {
        let stdlib_path = "stdlib/std";
        if let Ok(entries) = std::fs::read_dir(stdlib_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("aura") {
                    if let Ok(source) = std::fs::read_to_string(&path) {
                        let mut lexer = crate::compiler::frontend::lexer::Lexer::new(&source);
                        let tokens = lexer.lex_all();
                        let mut parser = crate::compiler::frontend::parser::Parser::new(tokens);
                        let program = parser.parse_program();
                        self.collect_classes(&program);
                    }
                }
            }
        }
    }

    fn resolve_type(&self, te: TypeExpr) -> Type {
        match te {
            TypeExpr::Name(n, _) => match n.as_str() {
                "i32" | "Int32" => Type::Int32,
                "i64" | "Int64" => Type::Int64,
                "f32" | "Float32" => Type::Float32,
                "f64" | "Float64" => Type::Float64,
                "string" | "String" => Type::String,
                "boolean" | "Boolean" => Type::Boolean,
                "void" | "Void" => Type::Void,
                "any" => Type::Unknown,
                _ => Type::Class(n),
            },
            TypeExpr::Union(tys, _) => {
                Type::Union(tys.into_iter().map(|t| self.resolve_type(t)).collect())
            }
            TypeExpr::Generic(name, args, _) => Type::Generic(
                name,
                args.into_iter().map(|t| self.resolve_type(t)).collect(),
            ),
            TypeExpr::Array(base, _) => Type::Array(Box::new(self.resolve_type(*base))),
            TypeExpr::Function(params, ret, _) => Type::Function(
                params.into_iter().map(|p| self.resolve_type(p)).collect(),
                Box::new(self.resolve_type(*ret)),
            ),
        }
    }

    fn is_assignable(&self, src: &Type, target: &Type) -> bool {
        self.is_assignable_internal(src, target, &mut Vec::new())
    }

    fn is_assignable_internal(
        &self,
        src: &Type,
        target: &Type,
        history: &mut Vec<(Type, Type)>,
    ) -> bool {
        if src == target {
            return true;
        }

        let pair = (src.clone(), target.clone());
        if history.contains(&pair) {
            return true;
        }
        history.push(pair);

        let result = match (src, target) {
            (s, Type::Union(options)) => options
                .iter()
                .any(|opt| self.is_assignable_internal(s, opt, history)),
            (Type::Union(options), t) => options
                .iter()
                .all(|opt| self.is_assignable_internal(opt, t, history)),

            (Type::Int32, Type::Int64) => true,

            // Generic types (Nominal for now, e.g. Box<i32> vs Box<i32>)
            (Type::Generic(src_name, src_args), Type::Generic(tgt_name, tgt_args)) => {
                if src_name != tgt_name || src_args.len() != tgt_args.len() {
                    return false;
                }
                for (s, t) in src_args.iter().zip(tgt_args.iter()) {
                    if !self.is_assignable_internal(s, t, history) {
                        return false;
                    }
                }
                true
            }

            // Structural identity for classes
            (Type::Class(src_name), Type::Class(tgt_name)) => {
                let src_info = self.classes.get(src_name);
                let tgt_info = self.classes.get(tgt_name);

                if let (Some(si), Some(ti)) = (src_info, tgt_info) {
                    let mut all_match = true;
                    for (name, tgt_ty) in &ti.fields {
                        if let Some(src_ty) = si.fields.get(name) {
                            if !self.is_assignable_internal(src_ty, tgt_ty, history) {
                                all_match = false;
                                break;
                            }
                        } else {
                            all_match = false;
                            break;
                        }
                    }
                    all_match
                } else {
                    false
                }
            }

            _ => false,
        };

        history.pop();
        result
    }

    fn check_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::VarDeclaration {
                name,
                ty,
                value,
                span,
                doc,
            } => {
                let val_ty = self.check_expr(value);
                let declared_ty = ty
                    .map(|t| self.resolve_type(t))
                    .unwrap_or_else(|| val_ty.clone());
                if !self.is_assignable(&val_ty, &declared_ty) {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            format!("{:?}", declared_ty),
                            format!("{:?}", val_ty),
                        ),
                        span,
                    );
                }
                self.scope.insert(name, declared_ty, false, span, doc);
            }
            Statement::Expression(expr, _) => {
                self.check_expr(expr);
            }
            Statement::Print(expr, _) => {
                self.check_expr(expr);
            }
            Statement::Block(stmts, _) => {
                self.push_scope();
                for s in stmts {
                    self.check_statement(s);
                }
                self.pop_scope();
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                let _cond_ty = self.check_expr(condition.clone());

                if let Expr::TypeTest(ref expr, ref ty_expr, _) = condition {
                    if let Expr::Variable(ref name, _) = **expr {
                        let narrowed_ty = self.resolve_type(ty_expr.clone());

                        self.push_scope();
                        self.scope
                            .insert(name.clone(), narrowed_ty.clone(), false, span, None);
                        self.check_statement(*then_branch);
                        self.pop_scope();

                        if let Some(eb) = else_branch {
                            let original_ty = self
                                .scope
                                .lookup(name)
                                .map(|s| s.ty.clone())
                                .unwrap_or(Type::Unknown);
                            let excluded_ty = original_ty.exclude(&narrowed_ty);

                            self.push_scope();
                            self.scope
                                .insert(name.clone(), excluded_ty, false, span, None);
                            self.check_statement(*eb);
                            self.pop_scope();
                        }
                        return;
                    }
                }

                self.check_statement(*then_branch);
                if let Some(eb) = else_branch {
                    self.check_statement(*eb);
                }
            }
            Statement::While {
                condition, body, ..
            } => {
                self.check_expr(condition);
                self.check_statement(*body);
            }
            Statement::Return(expr, _) => {
                self.check_expr(expr);
            }
            Statement::FunctionDeclaration {
                name,
                params,
                return_ty,
                body,
                span,
                doc,
            } => {
                let param_tys: Vec<Type> = params
                    .iter()
                    .map(|(_, ty)| self.resolve_type(ty.clone()))
                    .collect();
                let ret_ty = self.resolve_type(return_ty);

                // Register function before checking body for recursion
                self.scope.insert(
                    name.clone(),
                    Type::Function(param_tys.clone(), Box::new(ret_ty.clone())),
                    false,
                    span,
                    doc.clone(),
                );

                self.push_scope();
                for (pname, pty) in params {
                    let ty = self.resolve_type(pty.clone());
                    self.scope.insert(pname, ty, true, pty.span(), None);
                }
                self.check_statement(*body);
                self.pop_scope();
            }
            Statement::ClassDeclaration {
                name,
                fields: _,
                methods,
                constructor,
                span: _,
                doc: _,
            } => {
                self.current_class = Some(name.clone());

                if let Some(ctor) = constructor {
                    self.push_scope();
                    self.scope.insert(
                        "this".to_string(),
                        Type::Class(name.clone()),
                        false,
                        ctor.span,
                        None,
                    );
                    for (pname, pty) in ctor.params {
                        let ty = self.resolve_type(pty.clone());
                        self.scope.insert(pname, ty, true, pty.span(), None);
                    }
                    self.check_statement(*ctor.body);
                    self.pop_scope();
                }

                for m in methods {
                    self.push_scope();
                    self.scope.insert(
                        "this".to_string(),
                        Type::Class(name.clone()),
                        false,
                        m.span,
                        None,
                    );
                    for (pname, pty) in m.params {
                        let ty = self.resolve_type(pty.clone());
                        self.scope.insert(pname, ty, true, pty.span(), None);
                    }
                    self.check_statement(*m.body);
                    self.pop_scope();
                }
                self.current_class = None;
            }
            Statement::Error => {}
            Statement::Import { .. } | Statement::Export { .. } => {
                todo!("Imports/exports are not supported in semantic analyzer yet")
            }
        }
    }

    fn check_expr(&mut self, expr: Expr) -> Type {
        let span = expr.span();
        let ty = match expr {
            Expr::Number(_, _) => Type::Int32,
            Expr::StringLiteral(_, _) => Type::String,
            Expr::Variable(name, span) => {
                if let Some(sym) = self.scope.lookup(&name) {
                    if let Some(doc) = &sym.doc {
                        self.node_docs.insert(span, doc.clone());
                    }
                    self.node_definitions.insert(span, sym.span);
                    sym.ty.clone()
                } else if self.classes.contains_key(&name) {
                    Type::Class(name)
                } else {
                    self.error(SemanticErrorKind::UndefinedVariable(name), span);
                    Type::Unknown
                }
            }
            Expr::BinaryOp(left, op, right, span) => {
                let lhs = self.check_expr(*left);
                let rhs = self.check_expr(*right);
                if lhs.is_numeric() && rhs.is_numeric() {
                    lhs
                } else {
                    self.error(
                        SemanticErrorKind::IncompatibleBinaryOperators(
                            format!("{:?}", lhs),
                            op,
                            format!("{:?}", rhs),
                        ),
                        span,
                    );
                    Type::Unknown
                }
            }
            Expr::Assign(name, value, span) => {
                let val_ty = self.check_expr(*value);
                if let Some(sym) = self.scope.lookup(&name) {
                    let expected_ty = sym.ty.clone();
                    if !self.is_assignable(&val_ty, &expected_ty) {
                        self.error(
                            SemanticErrorKind::TypeMismatch(
                                format!("{:?}", expected_ty),
                                format!("{:?}", val_ty),
                            ),
                            span,
                        );
                    }
                } else {
                    self.error(SemanticErrorKind::UndefinedVariable(name), span);
                }
                val_ty
            }
            Expr::Call(name, args, span) => {
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg));
                }

                let sym = self.scope.lookup(&name);
                let function_ty = sym.map(|s| s.ty.clone());
                if let Some(s) = sym {
                    self.node_definitions.insert(span, s.span);
                }

                if let Some(Type::Function(param_tys, ret_ty)) = function_ty {
                    if param_tys.len() != arg_tys.len() {
                        self.error(
                            SemanticErrorKind::WrongArgumentCount(
                                name,
                                param_tys.len(),
                                arg_tys.len(),
                            ),
                            span,
                        );
                        return (*ret_ty).clone();
                    }
                    for (i, arg_ty) in arg_tys.iter().enumerate() {
                        if !self.is_assignable(arg_ty, &param_tys[i]) {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    format!("{:?}", param_tys[i]),
                                    format!("{:?}", arg_ty),
                                ),
                                span,
                            );
                        }
                    }
                    *ret_ty
                } else {
                    Type::Int32 // Default for now
                }
            }
            Expr::New(class_name, args, span) => {
                if let Some(class_info) = self.classes.get(&class_name) {
                    if let Some(doc) = &class_info.doc {
                        self.node_docs.insert(span, doc.clone());
                    }
                    self.node_definitions.insert(span, class_info.span);
                } else {
                    self.error(SemanticErrorKind::UndefinedClass(class_name.clone()), span);
                }
                for arg in args {
                    self.check_expr(arg);
                }
                Type::Class(class_name)
            }
            Expr::MemberAccess(obj, field, span) => {
                let obj_ty = self.check_expr(*obj);
                if let Type::Class(class_name) = obj_ty {
                    if let Some(class_info) = self.classes.get(&class_name) {
                        if let Some(field_ty) = class_info.fields.get(&field) {
                            field_ty.clone()
                        } else if let Some(field_ty) = class_info.static_fields.get(&field) {
                            field_ty.clone()
                        } else {
                            self.error(SemanticErrorKind::UndefinedField(class_name, field), span);
                            Type::Unknown
                        }
                    } else {
                        self.error(SemanticErrorKind::UndefinedClass(class_name), span);
                        Type::Unknown
                    }
                } else {
                    self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                    Type::Unknown
                }
            }
            Expr::MemberAssign(obj, field, value, span) => {
                let obj_ty = self.check_expr(*obj);
                let val_ty = self.check_expr(*value);
                if let Type::Class(class_name) = obj_ty {
                    if let Some(class_info) = self.classes.get(&class_name) {
                        if let Some(field_ty) = class_info.fields.get(&field) {
                            if !self.is_assignable(&val_ty, field_ty) {
                                self.error(
                                    SemanticErrorKind::TypeMismatch(
                                        format!("{:?}", field_ty),
                                        format!("{:?}", val_ty),
                                    ),
                                    span,
                                );
                            }
                        } else {
                            self.error(SemanticErrorKind::UndefinedField(class_name, field), span);
                        }
                    } else {
                        self.error(SemanticErrorKind::UndefinedClass(class_name), span);
                    }
                } else {
                    self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                }
                val_ty
            }
            Expr::MethodCall(obj, method, args, span) => {
                let obj_ty = self.check_expr(*obj);
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg));
                }

                if let Type::Class(class_name) = obj_ty {
                    let method_info = if let Some(class_info) = self.classes.get(&class_name) {
                        if let Some(m) = class_info.methods.get(&method) {
                            Some(m.clone())
                        } else {
                            class_info.static_methods.get(&method).cloned()
                        }
                    } else {
                        None
                    };

                    if let Some((param_tys, ret_ty, doc)) = method_info {
                        if let Some(d) = doc {
                            self.node_docs.insert(span, d.clone());
                        }

                        if param_tys.len() != arg_tys.len() {
                            self.error(
                                SemanticErrorKind::WrongArgumentCount(
                                    method,
                                    param_tys.len(),
                                    arg_tys.len(),
                                ),
                                span,
                            );
                            return ret_ty;
                        }
                        for (i, arg_ty) in arg_tys.iter().enumerate() {
                            if !self.is_assignable(arg_ty, &param_tys[i]) {
                                self.error(
                                    SemanticErrorKind::TypeMismatch(
                                        format!("{:?}", param_tys[i]),
                                        format!("{:?}", arg_ty),
                                    ),
                                    span,
                                );
                            }
                        }
                        ret_ty
                    } else {
                        self.error(SemanticErrorKind::UndefinedMethod(class_name, method), span);
                        Type::Unknown
                    }
                } else {
                    self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                    Type::Unknown
                }
            }
            Expr::This(span) => {
                if let Some(class_name) = &self.current_class {
                    Type::Class(class_name.clone())
                } else {
                    self.error(
                        SemanticErrorKind::UndefinedVariable("this".to_string()),
                        span,
                    );
                    Type::Unknown
                }
            }
            Expr::TypeTest(expr, ty_expr, _) => {
                self.check_expr(*expr);
                self.resolve_type(ty_expr);
                Type::Boolean
            }
            Expr::UnaryOp(op, expr, span) => {
                let expr_ty = self.check_expr(*expr);
                if op == "-" && expr_ty.is_numeric() {
                    expr_ty
                } else {
                    self.error(
                        SemanticErrorKind::IncompatibleBinaryOperators(
                            format!("{:?}", expr_ty),
                            op,
                            "None".to_string(),
                        ),
                        span,
                    );
                    Type::Unknown
                }
            }
            Expr::Error(_) => Type::Unknown,
        };
        self.node_types.insert(span, ty.clone());
        ty
    }

    fn push_scope(&mut self) {
        let current = std::mem::replace(&mut self.scope, Box::new(Scope::new(None)));
        self.scope = Box::new(Scope::new(Some(current)));
    }

    fn pop_scope(&mut self) {
        let mut child = std::mem::replace(&mut self.scope, Box::new(Scope::new(None)));
        if let Some(parent) = child.parent.take() {
            self.scope = parent;
        } else {
            panic!("Popped root scope");
        }
    }
}
