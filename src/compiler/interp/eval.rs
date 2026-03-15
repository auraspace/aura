use super::Interpreter;
use crate::compiler::ast::{Expr, Statement, TypeExpr};
use crate::compiler::interp::env::{StatementResult, Value};
use crate::compiler::sema::ty::Type;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

impl Interpreter {
    pub(crate) fn resolve_type(&self, te: TypeExpr) -> Type {
        match te {
            TypeExpr::Name(n, _) => match n.as_str() {
                "i32" | "Int32" | "number" | "Number" => Type::Int32,
                "i64" | "Int64" => Type::Int64,
                "f32" | "Float32" => Type::Float32,
                "f64" | "Float64" => Type::Float64,
                "string" | "String" => Type::String,
                "boolean" | "Boolean" => Type::Boolean,
                "void" | "Void" => Type::Void,
                _ => Type::Class(n),
            },
            TypeExpr::Union(tys, _) => Type::Union(
                tys.into_iter()
                    .map(|t| self.resolve_type(t))
                    .collect::<Vec<_>>(),
            ),
            _ => Type::Unknown,
        }
    }

    pub(crate) fn execute_statement(&mut self, stmt: Statement) -> StatementResult {
        if self.pending_exception.is_some() {
            return StatementResult::Throw(self.pending_exception.take().unwrap());
        }
        match stmt {
            Statement::Enum(decl) => {
                let mut members = HashMap::new();
                let mut next_int_val = 0;
                let mut is_string_enum = false;

                for member in &decl.members {
                    let val = if let Some(ref expr) = member.value {
                        let evaled = self.eval_expr(expr.clone());
                        if let Value::String(_) = evaled {
                            is_string_enum = true;
                        } else if let Value::Int(n) = evaled {
                            next_int_val = n + 1;
                        } else if let Value::Int64(n) = evaled {
                            next_int_val = (n + 1) as i32;
                        }
                        evaled
                    } else {
                        if is_string_enum {
                            Value::Null // Should be handled by semantic analyzer
                        } else {
                            let curr = next_int_val;
                            next_int_val += 1;
                            Value::Int(curr)
                        }
                    };
                    members.insert(member.name.clone(), val);
                }

                self.static_fields
                    .insert(decl.name.clone(), Rc::new(RefCell::new(members)));
                self.env
                    .insert(decl.name.clone(), Value::Class(decl.name.clone()));
                StatementResult::None
            }
            Statement::VarDeclaration {
                name,
                name_span: _,
                ty: _,
                value,
                is_const: _,
                span: _,
                doc: _,
            } => {
                let val = self.eval_expr(value);
                self.env.insert(name, val);
                StatementResult::None
            }
            Statement::FunctionDeclaration {
                name,
                name_span: _,
                params,
                return_ty,
                body,
                is_async,
                span: _,
                doc: _,
            } => {
                let func_val = Value::Function {
                    name: Some(name.clone()),
                    params: params.into_iter().map(|(p, _)| p).collect(),
                    return_ty: self.resolve_type(return_ty),
                    body: *body,
                    is_async,
                    captured_env: self.env_stack.clone(),
                };
                self.env.insert(name, func_val);
                StatementResult::None
            }
            Statement::ClassDeclaration {
                name,
                name_span: _,
                fields,
                methods,
                constructor,
                extends: _,
                implements: _,
                is_abstract: _,
                span: _,
                doc: _,
            } => {
                let mut method_map = HashMap::new();
                for m in methods {
                    method_map.insert(m.name.clone(), m);
                }
                if let Some(cons) = constructor {
                    method_map.insert("constructor".to_string(), cons);
                }

                // Initialize static fields
                let mut s_fields = HashMap::new();
                for f in &fields {
                    if f.is_static {
                        let val = if let Some(init) = &f.value {
                            self.eval_expr(init.clone())
                        } else {
                            Value::Null
                        };
                        s_fields.insert(f.name.clone(), val);
                    }
                }
                self.static_fields
                    .insert(name.clone(), Rc::new(RefCell::new(s_fields)));

                self.classes.insert(name.clone(), (fields, method_map));
                self.env.insert(name.clone(), Value::Class(name));
                StatementResult::None
            }
            Statement::Return(expr, _) => {
                let val = self.eval_expr(expr);
                StatementResult::Return(val)
            }
            Statement::Print(expr, _) => {
                let val = self.eval_expr(expr);
                if self.pending_exception.is_some() {
                    return StatementResult::Throw(self.pending_exception.take().unwrap());
                }
                let s = self.stringify(val);
                if let Some(e) = self.pending_exception.take() {
                    return StatementResult::Throw(e);
                }
                println!("{}", s);
                StatementResult::None
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                span: _,
            } => {
                let cond_val = self.eval_expr(condition);
                if cond_val.is_truthy() {
                    self.execute_statement(*then_branch)
                } else if let Some(eb) = else_branch {
                    self.execute_statement(*eb)
                } else {
                    StatementResult::None
                }
            }
            Statement::While {
                condition,
                body,
                span: _,
            } => {
                while self.eval_expr(condition.clone()).is_truthy() {
                    let res = self.execute_statement((*body).clone());
                    if let StatementResult::Return(_) | StatementResult::Throw(_) = res {
                        return res;
                    }
                }
                StatementResult::None
            }
            Statement::Block(stmts, _) => {
                self.push_scope();
                let mut final_res = StatementResult::None;
                for s in stmts {
                    let res = self.execute_statement(s);
                    if let StatementResult::Return(_) | StatementResult::Throw(_) = res {
                        final_res = res;
                        break;
                    }
                }
                self.pop_scope();
                final_res
            }
            Statement::Expression(expr, _) => {
                let _val = self.eval_expr(expr);
                if let Some(e) = self.pending_exception.take() {
                    StatementResult::Throw(e)
                } else {
                    StatementResult::None
                }
            }
            Statement::Error => StatementResult::None,
            Statement::Import { path, .. } => {
                self.load_import(path);
                StatementResult::None
            }
            Statement::TryCatch {
                try_block,
                catch_param,
                catch_block,
                finally_block,
                ..
            } => {
                let mut res = self.execute_statement(*try_block);
                res = match res {
                    StatementResult::Throw(e) => {
                        if let Some(cb) = catch_block {
                            self.push_scope();
                            if let Some((name, _)) = catch_param {
                                self.env.insert(name, e);
                            }
                            let cb_res = self.execute_statement(*cb);
                            self.pop_scope();
                            cb_res
                        } else {
                            StatementResult::Throw(e)
                        }
                    }
                    other => other,
                };

                if let Some(fb) = finally_block {
                    let f_res = self.execute_statement(*fb);
                    if let StatementResult::Return(_) | StatementResult::Throw(_) = f_res {
                        res = f_res;
                    }
                }
                res
            }
            Statement::Export { decl, .. } => self.execute_statement(*decl),
            Statement::Interface(_) => StatementResult::None,
            Statement::Comment(_, _) | Statement::RegularBlockComment(_, _) => {
                StatementResult::None
            }
        }
    }

    pub(crate) fn eval_expr(&mut self, expr: Expr) -> Value {
        if self.pending_exception.is_some() {
            return Value::Void;
        }
        match expr {
            Expr::Number(n, _) => {
                if n >= i32::MIN as i64 && n <= i32::MAX as i64 {
                    Value::Int(n as i32)
                } else {
                    Value::Int64(n)
                }
            }
            Expr::Null(_) => Value::Null,
            Expr::ArrayLiteral(elements, _) => {
                let mut vals = Vec::new();
                for e in elements {
                    vals.push(self.eval_expr(e));
                }
                Value::Array(Rc::new(RefCell::new(vals)))
            }
            Expr::StringLiteral(s, _) => Value::String(s),
            Expr::Template(parts, _) => {
                use crate::compiler::ast::TemplatePart;
                let mut out = String::new();
                for part in parts {
                    match part {
                        TemplatePart::Str(s) => out.push_str(&s),
                        TemplatePart::Expr(e) => {
                            let val = self.eval_expr(*e);
                            match val {
                                Value::String(s) => out.push_str(&s),
                                Value::Int(n) => out.push_str(&n.to_string()),
                                Value::Int64(n) => out.push_str(&n.to_string()),
                                Value::Boolean(b) => out.push_str(if b { "true" } else { "false" }),
                                Value::Null => out.push_str("null"),
                                _ => panic!("Cannot interpolate {:?}", val),
                            }
                        }
                    }
                }
                Value::String(out)
            }
            Expr::Await(expr, _) => {
                // In synchronous interpreter, await just evaluates and returns the value
                self.eval_expr(*expr)
            }
            Expr::Variable(name, _) => {
                if let Some(val) = self.env.lookup(&name) {
                    val
                } else if self.classes.contains_key(&name) {
                    Value::Class(name)
                } else {
                    panic!("Undefined variable {}", name)
                }
            }
            Expr::BinaryOp(lhs, op, rhs, _) => {
                let left = self.eval_expr(*lhs);
                if op == "&&" {
                    if !left.is_truthy() {
                        return left;
                    }
                    return self.eval_expr(*rhs);
                } else if op == "||" {
                    if left.is_truthy() {
                        return left;
                    }
                    return self.eval_expr(*rhs);
                }

                let right = self.eval_expr(*rhs);
                match (&left, &right) {
                    (Value::Int(l), Value::Int(r)) => match op.as_str() {
                        "+" => Value::Int(l + r),
                        "-" => Value::Int(l - r),
                        "*" => Value::Int(l * r),
                        "/" => Value::Int(l / r),
                        "%" => Value::Int(l % r),
                        "==" => Value::Boolean(l == r),
                        "!=" => Value::Boolean(l != r),
                        "<" => Value::Boolean(l < r),
                        "<=" => Value::Boolean(l <= r),
                        ">" => Value::Boolean(l > r),
                        ">=" => Value::Boolean(l >= r),
                        "|" => Value::Int(l | r),
                        _ => panic!("Unsupported operator {} for integers", op),
                    },
                    (Value::Int64(l), Value::Int64(r)) => match op.as_str() {
                        "+" => Value::Int64(l + r),
                        "-" => Value::Int64(l - r),
                        "*" => Value::Int64(l * r),
                        "/" => Value::Int64(l / r),
                        "%" => Value::Int64(l % r),
                        "==" => Value::Boolean(l == r),
                        "!=" => Value::Boolean(l != r),
                        "<" => Value::Boolean(l < r),
                        "<=" => Value::Boolean(l <= r),
                        ">" => Value::Boolean(l > r),
                        ">=" => Value::Boolean(l >= r),
                        "|" => Value::Int64(l | r),
                        _ => panic!("Unsupported operator {} for i64", op),
                    },
                    (Value::String(l), Value::String(r)) => match op.as_str() {
                        "+" => Value::String(format!("{}{}", l, r)),
                        "==" => Value::Boolean(l == r),
                        "!=" => Value::Boolean(l != r),
                        _ => panic!("Unsupported operator {} for strings", op),
                    },
                    (Value::String(l), Value::Int(r)) => match op.as_str() {
                        "+" => Value::String(format!("{}{}", l, r)),
                        _ => panic!("Unsupported operator {} for string and int", op),
                    },
                    (Value::Int(l), Value::String(r)) => match op.as_str() {
                        "+" => Value::String(format!("{}{}", l, r)),
                        _ => panic!("Unsupported operator {} for int and string", op),
                    },
                    (Value::Null, Value::Null) => match op.as_str() {
                        "==" => Value::Boolean(true),
                        "!=" => Value::Boolean(false),
                        _ => panic!("Unsupported operator {} for null", op),
                    }
                    (Value::Null, _) | (_, Value::Null) => match op.as_str() {
                        "==" => Value::Boolean(false),
                        "!=" => Value::Boolean(true),
                        _ => panic!("Unsupported operator {} for null comparison", op),
                    }
                    (l, r) => panic!("Operands must be same type (integers or strings) for binary op, got {:?} and {:?}", l, r),
                }
            }
            Expr::Assign(name, val_expr, _) => {
                let val = self.eval_expr(*val_expr);
                self.env.assign(&name, val.clone());
                val
            }
            Expr::Call(name, _, args, _) => {
                let func = self.env.lookup(&name).expect("Function not found");
                if let Value::Function {
                    name: _,
                    params,
                    return_ty: _,
                    body,
                    is_async: _,
                    captured_env: _,
                } = func
                {
                    let mut arg_vals = Vec::new();
                    for a in args {
                        arg_vals.push(self.eval_expr(a));
                    }
                    self.push_scope();
                    for (i, pname) in params.iter().enumerate() {
                        self.env.insert(pname.clone(), arg_vals[i].clone());
                    }
                    let res = self.execute_statement(body);
                    self.pop_scope();
                    if let StatementResult::Return(v) = res {
                        v
                    } else if let StatementResult::Throw(e) = res {
                        self.pending_exception = Some(e);
                        Value::Void
                    } else {
                        Value::Void
                    }
                } else if let Value::NativeFunction(f) = func {
                    let mut arg_vals = Vec::new();
                    for a in args {
                        arg_vals.push(self.eval_expr(a));
                    }
                    f(arg_vals)
                } else {
                    panic!("Not a function");
                }
            }
            Expr::MethodCall(obj_expr, method, _, args, _) => {
                let obj = self.eval_expr(*obj_expr);
                match obj {
                    Value::Instance(class_name, fields_ref) => {
                        let m = {
                            let (_, methods) = self
                                .classes
                                .get(&class_name)
                                .expect(&format!("Class {} not found", class_name));
                            methods
                                .get(&method)
                                .expect(&format!(
                                    "Method {} not found in class {}",
                                    method, class_name
                                ))
                                .clone()
                        };

                        let mut arg_vals = Vec::new();
                        for a in args {
                            arg_vals.push(self.eval_expr(a));
                        }

                        self.push_scope();
                        self.env
                            .insert("this".to_string(), Value::Instance(class_name, fields_ref));
                        for (i, (pname, _)) in m.params.iter().enumerate() {
                            self.env.insert(pname.clone(), arg_vals[i].clone());
                        }

                        let res = self.execute_statement((*m.body).clone());
                        self.pop_scope();
                        if let StatementResult::Return(v) = res {
                            v
                        } else {
                            Value::Void
                        }
                    }
                    Value::Class(class_name) => {
                        if class_name == "Promise" {
                            let mut arg_vals = Vec::new();
                            for a in &args {
                                arg_vals.push(self.eval_expr(a.clone()));
                            }

                            match method.as_str() {
                                "all" => {
                                    if let Some(Value::Array(promises)) = arg_vals.get(0) {
                                        let mut resolved = Vec::new();
                                        for p in promises.borrow().iter() {
                                            if let Value::Promise(v) = p {
                                                resolved.push((**v).clone());
                                            } else {
                                                resolved.push(p.clone());
                                            }
                                        }
                                        return Value::Promise(Box::new(Value::Array(Rc::new(
                                            RefCell::new(resolved),
                                        ))));
                                    }
                                    panic!("Promise.all expects an array");
                                }
                                "allSettled" => {
                                    if let Some(Value::Array(promises)) = arg_vals.get(0) {
                                        let mut results = Vec::new();
                                        for p in promises.borrow().iter() {
                                            let mut map = HashMap::new();
                                            map.insert(
                                                "status".to_string(),
                                                Value::String("fulfilled".to_string()),
                                            );
                                            if let Value::Promise(v) = p {
                                                map.insert("value".to_string(), (**v).clone());
                                            } else {
                                                map.insert("value".to_string(), p.clone());
                                            }
                                            results.push(Value::Instance(
                                                "PromiseResult".to_string(),
                                                Rc::new(RefCell::new(map)),
                                            ));
                                        }
                                        return Value::Promise(Box::new(Value::Array(Rc::new(
                                            RefCell::new(results),
                                        ))));
                                    }
                                    panic!("Promise.allSettled expects an array");
                                }
                                "any" => {
                                    if let Some(Value::Array(promises)) = arg_vals.get(0) {
                                        if let Some(p) = promises.borrow().iter().next() {
                                            // In our synchronous interpreter, we just pick the first one
                                            if let Value::Promise(v) = p {
                                                return Value::Promise(v.clone());
                                            }
                                            return Value::Promise(Box::new(p.clone()));
                                        }
                                        panic!("Promise.any with empty array");
                                    }
                                    panic!("Promise.any expects an array");
                                }
                                "race" => {
                                    if let Some(Value::Array(promises)) = arg_vals.get(0) {
                                        if let Some(p) = promises.borrow().get(0) {
                                            if let Value::Promise(v) = p {
                                                return Value::Promise(v.clone());
                                            }
                                            return Value::Promise(Box::new(p.clone()));
                                        }
                                        panic!("Promise.race with empty array");
                                    }
                                    panic!("Promise.race expects an array");
                                }
                                _ => {}
                            }
                        }

                        let m = {
                            let (_, methods) = self
                                .classes
                                .get(&class_name)
                                .expect(&format!("Class {} not found", class_name));
                            methods
                                .get(&method)
                                .expect(&format!(
                                    "Static method {} not found in class {}",
                                    method, class_name
                                ))
                                .clone()
                        };

                        let mut arg_vals = Vec::new();
                        for a in &args {
                            arg_vals.push(self.eval_expr(a.clone()));
                        }

                        self.push_scope();
                        for (i, (pname, _)) in m.params.iter().enumerate() {
                            self.env.insert(pname.clone(), arg_vals[i].clone());
                        }

                        let res = self.execute_statement((*m.body).clone());
                        self.pop_scope();
                        if let StatementResult::Return(v) = res {
                            v
                        } else {
                            Value::Void
                        }
                    }
                    Value::String(s) => {
                        let mut arg_vals = vec![Value::String(s)];
                        for a in args {
                            arg_vals.push(self.eval_expr(a));
                        }
                        let intrinsic_name = format!("__str_{}", method);
                        if let Some(Value::NativeFunction(f)) = self.env.lookup(&intrinsic_name) {
                            f(arg_vals)
                        } else {
                            panic!("String method {} not found", method);
                        }
                    }
                    Value::Array(a) => {
                        let mut arg_vals = vec![Value::Array(a)];
                        for a in args {
                            arg_vals.push(self.eval_expr(a));
                        }
                        let intrinsic_name = format!("__arr_{}", method);
                        if let Some(Value::NativeFunction(f)) = self.env.lookup(&intrinsic_name) {
                            f(arg_vals)
                        } else {
                            panic!("Array method {} not found", method);
                        }
                    }
                    _ => panic!("Method call on non-instance and non-class: {:?}", obj),
                }
            }
            Expr::This(_) => self
                .env
                .lookup("this")
                .expect("Usage of this outside of class context"),
            Expr::New(class_name, _, args, _) => {
                let (field_names, methods) = {
                    let (fnms, mths) = self
                        .classes
                        .get(&class_name)
                        .expect(&format!("Class {} not found", class_name));
                    (fnms.clone(), mths.clone())
                };

                let mut instance_fields = HashMap::new();
                for f in field_names {
                    if !f.is_static {
                        let val = if let Some(init) = &f.value {
                            self.eval_expr(init.clone())
                        } else {
                            Value::Null
                        };
                        instance_fields.insert(f.name.clone(), val);
                    }
                }

                let instance =
                    Value::Instance(class_name.clone(), Rc::new(RefCell::new(instance_fields)));

                if let Some(cons) = methods.get("constructor") {
                    let mut arg_vals = Vec::new();
                    for a in args {
                        arg_vals.push(self.eval_expr(a));
                    }

                    self.push_scope();
                    self.env.insert("this".to_string(), instance.clone());
                    for (i, (pname, _)) in cons.params.iter().enumerate() {
                        let val = if i < arg_vals.len() {
                            arg_vals[i].clone()
                        } else {
                            Value::Null
                        };
                        self.env.insert(pname.clone(), val);
                    }

                    let res = self.execute_statement((*cons.body).clone());

                    if let StatementResult::Throw(e) = res {
                        self.pending_exception = Some(e);
                        self.pop_scope();
                        return Value::Void;
                    }

                    // After constructor, 'this' might have changed fields
                    let updated_instance = self.env.lookup("this").unwrap();
                    self.pop_scope();
                    updated_instance
                } else {
                    instance
                }
            }
            Expr::MemberAccess(obj_expr, member, _, _) => {
                let obj = self.eval_expr(*obj_expr);
                match obj {
                    Value::Instance(_, fields) => fields
                        .borrow()
                        .get(&member)
                        .cloned()
                        .expect("Field not found"),
                    Value::Class(class_name) => {
                        let fields = self
                            .static_fields
                            .get(&class_name)
                            .expect("Class static fields not found");
                        fields
                            .borrow()
                            .get(&member)
                            .cloned()
                            .expect("Static field not found")
                    }
                    _ => panic!("Not an instance or class for member access: {:?}", obj),
                }
            }
            Expr::MemberAssign(obj_expr, member, val_expr, _, _) => {
                let obj = self.eval_expr(*obj_expr);
                let val = self.eval_expr(*val_expr);
                match obj {
                    Value::Instance(_, fields) => {
                        fields.borrow_mut().insert(member, val.clone());
                        val
                    }
                    Value::Class(class_name) => {
                        let fields = self
                            .static_fields
                            .get(&class_name)
                            .expect("Class static fields not found");
                        fields.borrow_mut().insert(member, val.clone());
                        val
                    }
                    _ => panic!("Not an instance or class for member assign: {:?}", obj),
                }
            }
            Expr::TypeTest(expr, ty_expr, _) => {
                let val = self.eval_expr(*expr);
                let target_ty = self.resolve_type(ty_expr);
                match (val, target_ty) {
                    (Value::Int(_), Type::Int32) => Value::Boolean(true),
                    (Value::Int64(_), Type::Int64) => Value::Boolean(true),
                    (Value::String(_), Type::String) => Value::Boolean(true),
                    (Value::Boolean(_), Type::Boolean) => Value::Boolean(true),
                    _ => Value::Boolean(false),
                }
            }
            Expr::UnaryOp(op, expr, _) => {
                let val = self.eval_expr(*expr);
                match val {
                    Value::Int(i) => {
                        if op == "-" {
                            Value::Int(-i)
                        } else {
                            panic!("Unsupported unary operator {}", op);
                        }
                    }
                    _ => panic!("Unary operator {} only supported for integers", op),
                }
            }
            Expr::Throw(expr, _) => {
                let val = self.eval_expr(*expr);
                self.pending_exception = Some(val);
                Value::Void
            }
            Expr::Index(obj_expr, index_expr, _) => {
                let obj = self.eval_expr(*obj_expr);
                let index = self.eval_expr(*index_expr);
                match (obj, index) {
                    (Value::Array(a), Value::Int(i)) => {
                        let borrowed = a.borrow();
                        if i < 0 || i as usize >= borrowed.len() {
                            panic!("Array index out of bounds: {}", i);
                        }
                        borrowed[i as usize].clone()
                    }
                    (Value::String(s), Value::Int(i)) => {
                        if i < 0 || i as usize >= s.len() {
                            panic!("String index out of bounds: {}", i);
                        }
                        Value::String(s.chars().nth(i as usize).unwrap().to_string())
                    }
                    _ => panic!("Index operation not supported for these types"),
                }
            }
            Expr::Super(_) => self
                .env
                .lookup("this")
                .expect("Usage of super outside of class context"),
            Expr::SuperCall(_, _) => {
                // Not fully supported in interpreter yet, but needs to be exhaustive
                Value::Void
            }
            Expr::Error(_) => panic!("Compiler bug: reaching error node in interpreter"),
        }
    }

    pub(crate) fn stringify(&mut self, val: Value) -> String {
        match val {
            Value::Int(i) => i.to_string(),
            Value::Int64(i) => i.to_string(),
            Value::String(s) => s,
            Value::Boolean(b) => (if b { "true" } else { "false" }).to_string(),
            Value::Void => "void".to_string(),
            Value::Null => "null".to_string(),
            Value::Instance(class_name, fields_ref) => {
                let m = self
                    .classes
                    .get(&class_name)
                    .and_then(|(_, methods)| methods.get("toString").cloned());

                if let Some(m) = m {
                    self.push_scope();
                    self.env.insert(
                        "this".to_string(),
                        Value::Instance(class_name.clone(), fields_ref),
                    );
                    let res = self.execute_statement((*m.body).clone());
                    self.pop_scope();

                    match res {
                        StatementResult::Return(v) => self.stringify(v),
                        StatementResult::Throw(e) => {
                            self.pending_exception = Some(e);
                            "<error in toString>".to_string()
                        }
                        _ => format!("<Instance of {}>", class_name),
                    }
                } else {
                    format!("<Instance of {}>", class_name)
                }
            }
            Value::Array(elements) => {
                let mut s = "[".to_string();
                let borrowed = elements.borrow();
                for (i, el) in borrowed.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    if let Value::String(str_val) = el {
                        s.push('"');
                        s.push_str(&str_val);
                        s.push('"');
                    } else {
                        s.push_str(&self.stringify(el.clone()));
                    }
                }
                s.push(']');
                s
            }
            Value::Function { name, .. } => format!("<Function {:?}>", name),
            Value::Class(name) => format!("<Class {}>", name),
            Value::Promise(val) => format!("<Promise: resolved to {:?}>", val),
            Value::NativeFunction(_) => "<NativeFunction>".to_string(),
        }
    }
}
