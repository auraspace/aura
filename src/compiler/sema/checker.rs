use crate::compiler::ast::{Expr, Program, Statement, TypeExpr};
use crate::compiler::sema::scope::Scope;
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

pub struct ClassInfo {
    pub name: String,
    pub fields: HashMap<String, Type>,
    pub methods: HashMap<String, (Vec<Type>, Type)>, // name -> (params, return_ty)
}

pub struct SemanticAnalyzer {
    pub scope: Box<Scope>,
    pub classes: HashMap<String, ClassInfo>,
    pub current_class: Option<String>,
}

#[derive(Debug)]
pub enum SemanticError {
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

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            scope: Box::new(Scope::new(None)),
            classes: HashMap::new(),
            current_class: None,
        }
    }

    pub fn analyze(&mut self, program: Program) -> Result<(), SemanticError> {
        // Pass 1: Collect class info
        for stmt in &program.statements {
            if let Statement::ClassDeclaration {
                name,
                fields,
                methods,
                ..
            } = stmt
            {
                let mut field_map = HashMap::new();
                for f in fields {
                    field_map.insert(f.name.clone(), self.resolve_type(f.ty.clone()));
                }
                let mut method_map = HashMap::new();
                for m in methods {
                    let param_tys = m
                        .params
                        .iter()
                        .map(|(_, ty)| self.resolve_type(ty.clone()))
                        .collect();
                    let ret_ty = self.resolve_type(m.return_ty.clone());
                    method_map.insert(m.name.clone(), (param_tys, ret_ty));
                }
                self.classes.insert(
                    name.clone(),
                    ClassInfo {
                        name: name.clone(),
                        fields: field_map,
                        methods: method_map,
                    },
                );
            }
        }

        // Pass 2: Check statements
        for stmt in program.statements {
            self.check_statement(stmt)?;
        }
        Ok(())
    }

    fn resolve_type(&self, te: TypeExpr) -> Type {
        match te {
            TypeExpr::Name(n) => match n.as_str() {
                "i32" => Type::Int32,
                "i64" => Type::Int64,
                "f32" => Type::Float32,
                "f64" => Type::Float64,
                "string" => Type::String,
                "boolean" => Type::Boolean,
                "void" => Type::Void,
                "any" => Type::Unknown,
                _ => Type::Class(n),
            },
            TypeExpr::Union(tys) => {
                Type::Union(tys.into_iter().map(|t| self.resolve_type(t)).collect())
            }
            TypeExpr::Generic(name, args) => Type::Generic(
                name,
                args.into_iter().map(|t| self.resolve_type(t)).collect(),
            ),
            _ => Type::Unknown,
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

    fn check_statement(&mut self, stmt: Statement) -> Result<(), SemanticError> {
        match stmt {
            Statement::VarDeclaration { name, ty, value } => {
                let val_ty = self.check_expr(value)?;
                let declared_ty = ty
                    .map(|t| self.resolve_type(t))
                    .unwrap_or_else(|| val_ty.clone());
                if !self.is_assignable(&val_ty, &declared_ty) {
                    return Err(SemanticError::TypeMismatch(
                        format!("{:?}", declared_ty),
                        format!("{:?}", val_ty),
                    ));
                }
                self.scope.insert(name, declared_ty, false);
                Ok(())
            }
            Statement::Expression(expr) => {
                self.check_expr(expr)?;
                Ok(())
            }
            Statement::Print(expr) => {
                self.check_expr(expr)?;
                Ok(())
            }
            Statement::Block(stmts) => {
                self.push_scope();
                for s in stmts {
                    self.check_statement(s)?;
                }
                self.pop_scope();
                Ok(())
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let _cond_ty = self.check_expr(condition.clone())?;

                if let Expr::TypeTest(ref expr, ref ty_expr) = condition {
                    if let Expr::Variable(ref name) = **expr {
                        let narrowed_ty = self.resolve_type(ty_expr.clone());

                        self.push_scope();
                        self.scope.insert(name.clone(), narrowed_ty.clone(), false);
                        self.check_statement(*then_branch)?;
                        self.pop_scope();

                        if let Some(eb) = else_branch {
                            let original_ty = self
                                .scope
                                .lookup(name)
                                .map(|s| s.ty.clone())
                                .unwrap_or(Type::Unknown);
                            let excluded_ty = original_ty.exclude(&narrowed_ty);

                            self.push_scope();
                            self.scope.insert(name.clone(), excluded_ty, false);
                            self.check_statement(*eb)?;
                            self.pop_scope();
                        }
                        return Ok(());
                    }
                }

                self.check_statement(*then_branch)?;
                if let Some(eb) = else_branch {
                    self.check_statement(*eb)?;
                }
                Ok(())
            }
            Statement::While { condition, body } => {
                self.check_expr(condition)?;
                self.check_statement(*body)?;
                Ok(())
            }
            Statement::Return(expr) => {
                self.check_expr(expr)?;
                Ok(())
            }
            Statement::FunctionDeclaration {
                name,
                params,
                return_ty,
                body,
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
                );

                self.push_scope();
                for (pname, pty) in params {
                    let ty = self.resolve_type(pty);
                    self.scope.insert(pname, ty, false);
                }
                self.check_statement(*body)?;
                self.pop_scope();
                Ok(())
            }
            Statement::ClassDeclaration {
                name,
                fields: _,
                methods,
                constructor,
            } => {
                self.current_class = Some(name.clone());

                if let Some(ctor) = constructor {
                    self.push_scope();
                    self.scope
                        .insert("this".to_string(), Type::Class(name.clone()), false);
                    for (pname, pty) in ctor.params {
                        let ty = self.resolve_type(pty);
                        self.scope.insert(pname, ty, false);
                    }
                    self.check_statement(*ctor.body)?;
                    self.pop_scope();
                }

                for m in methods {
                    self.push_scope();
                    self.scope
                        .insert("this".to_string(), Type::Class(name.clone()), false);
                    for (pname, pty) in m.params {
                        let ty = self.resolve_type(pty);
                        self.scope.insert(pname, ty, false);
                    }
                    self.check_statement(*m.body)?;
                    self.pop_scope();
                }
                self.current_class = None;
                Ok(())
            }
            Statement::Error => Ok(()),
        }
    }

    fn check_expr(&mut self, expr: Expr) -> Result<Type, SemanticError> {
        match expr {
            Expr::Number(_) => Ok(Type::Int32),
            Expr::StringLiteral(_) => Ok(Type::String),
            Expr::Variable(name) => {
                let sym = self
                    .scope
                    .lookup(&name)
                    .ok_or(SemanticError::UndefinedVariable(name))?;
                Ok(sym.ty.clone())
            }
            Expr::BinaryOp(left, op, right) => {
                let lhs = self.check_expr(*left)?;
                let rhs = self.check_expr(*right)?;
                if lhs.is_numeric() && rhs.is_numeric() {
                    Ok(lhs)
                } else {
                    Err(SemanticError::IncompatibleBinaryOperators(
                        format!("{:?}", lhs),
                        op,
                        format!("{:?}", rhs),
                    ))
                }
            }
            Expr::Assign(name, value) => {
                let expected_ty = self
                    .scope
                    .lookup(&name)
                    .ok_or(SemanticError::UndefinedVariable(name.clone()))?
                    .ty
                    .clone();
                let val_ty = self.check_expr(*value)?;
                if !self.is_assignable(&val_ty, &expected_ty) {
                    return Err(SemanticError::TypeMismatch(
                        format!("{:?}", expected_ty),
                        format!("{:?}", val_ty),
                    ));
                }
                Ok(val_ty)
            }
            Expr::Call(name, args) => {
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg)?);
                }

                if let Some(sym) = self.scope.lookup(&name) {
                    if let Type::Function(param_tys, ret_ty) = &sym.ty {
                        if param_tys.len() != arg_tys.len() {
                            return Err(SemanticError::WrongArgumentCount(
                                name,
                                param_tys.len(),
                                arg_tys.len(),
                            ));
                        }
                        for (i, arg_ty) in arg_tys.iter().enumerate() {
                            if !self.is_assignable(arg_ty, &param_tys[i]) {
                                return Err(SemanticError::TypeMismatch(
                                    format!("{:?}", param_tys[i]),
                                    format!("{:?}", arg_ty),
                                ));
                            }
                        }
                        return Ok((**ret_ty).clone());
                    }
                }
                Ok(Type::Int64)
            }
            Expr::New(class_name, args) => {
                if !self.classes.contains_key(&class_name) {
                    return Err(SemanticError::UndefinedClass(class_name));
                }
                for arg in args {
                    self.check_expr(arg)?;
                }
                Ok(Type::Class(class_name))
            }
            Expr::MemberAccess(obj, field) => {
                let obj_ty = self.check_expr(*obj)?;
                if let Type::Class(class_name) = obj_ty {
                    let class_info = self
                        .classes
                        .get(&class_name)
                        .ok_or(SemanticError::UndefinedClass(class_name.clone()))?;
                    let field_ty = class_info
                        .fields
                        .get(&field)
                        .ok_or(SemanticError::UndefinedField(class_name, field))?
                        .clone();
                    Ok(field_ty)
                } else {
                    Err(SemanticError::NotAClass(format!("{:?}", obj_ty)))
                }
            }
            Expr::MemberAssign(obj, field, value) => {
                let obj_ty = self.check_expr(*obj)?;
                if let Type::Class(class_name) = obj_ty {
                    let field_ty = {
                        let class_info = self
                            .classes
                            .get(&class_name)
                            .ok_or(SemanticError::UndefinedClass(class_name.clone()))?;
                        class_info
                            .fields
                            .get(&field)
                            .ok_or(SemanticError::UndefinedField(class_name, field))?
                            .clone()
                    };
                    let val_ty = self.check_expr(*value)?;
                    if !self.is_assignable(&val_ty, &field_ty) {
                        return Err(SemanticError::TypeMismatch(
                            format!("{:?}", field_ty),
                            format!("{:?}", val_ty),
                        ));
                    }
                    Ok(val_ty)
                } else {
                    Err(SemanticError::NotAClass(format!("{:?}", obj_ty)))
                }
            }
            Expr::MethodCall(obj, method, args) => {
                let obj_ty = self.check_expr(*obj)?;
                if let Type::Class(class_name) = obj_ty {
                    let (param_tys, ret_ty) = {
                        let class_info = self
                            .classes
                            .get(&class_name)
                            .ok_or(SemanticError::UndefinedClass(class_name.clone()))?;
                        let (pt, rt) = class_info.methods.get(&method).ok_or(
                            SemanticError::UndefinedMethod(class_name.clone(), method.clone()),
                        )?;
                        (pt.clone(), rt.clone())
                    };
                    if param_tys.len() != args.len() {
                        return Err(SemanticError::WrongArgumentCount(
                            method,
                            param_tys.len(),
                            args.len(),
                        ));
                    }
                    for (i, arg) in args.into_iter().enumerate() {
                        let arg_ty = self.check_expr(arg)?;
                        if !self.is_assignable(&arg_ty, &param_tys[i]) {
                            return Err(SemanticError::TypeMismatch(
                                format!("{:?}", param_tys[i]),
                                format!("{:?}", arg_ty),
                            ));
                        }
                    }
                    Ok(ret_ty)
                } else {
                    Err(SemanticError::NotAClass(format!("{:?}", obj_ty)))
                }
            }
            Expr::This => {
                if let Some(class_name) = &self.current_class {
                    Ok(Type::Class(class_name.clone()))
                } else {
                    Err(SemanticError::UndefinedVariable("this".to_string()))
                }
            }
            Expr::TypeTest(expr, ty_expr) => {
                self.check_expr(*expr)?;
                self.resolve_type(ty_expr);
                Ok(Type::Boolean)
            }
            Expr::Error => panic!("Compiler bug: reaching error node in semantic analyzer"),
        }
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
