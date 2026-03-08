pub mod value;

use crate::compiler::ast::{ClassMethod, Expr, Program, Statement, TypeExpr};
use crate::compiler::sema::ty::Type;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use value::Value;

#[derive(Debug, Clone)]
pub enum StatementResult {
    None,
    Return(Value),
}

pub struct Environment {
    symbols: HashMap<String, Value>,
    pub parent: Option<Box<Environment>>,
}

impl Environment {
    pub fn new(parent: Option<Box<Environment>>) -> Self {
        Self {
            symbols: HashMap::new(),
            parent,
        }
    }

    pub fn insert(&mut self, name: String, val: Value) {
        self.symbols.insert(name, val);
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        if let Some(val) = self.symbols.get(name) {
            Some(val.clone())
        } else if let Some(ref parent) = self.parent {
            parent.lookup(name)
        } else {
            None
        }
    }

    pub fn assign(&mut self, name: &str, val: Value) -> bool {
        if self.symbols.contains_key(name) {
            self.symbols.insert(name.to_string(), val);
            true
        } else if let Some(ref mut parent) = self.parent {
            parent.assign(name, val)
        } else {
            false
        }
    }
}

pub struct Interpreter {
    pub env: Box<Environment>,
    // Store class definitions separately (fields metadata and methods)
    pub classes: HashMap<String, (Vec<String>, HashMap<String, ClassMethod>)>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            env: Box::new(Environment::new(None)),
            classes: HashMap::new(),
        }
    }

    pub fn interpret(&mut self, program: Program) {
        for stmt in program.statements {
            self.execute_statement(stmt);
        }
    }

    fn push_scope(&mut self) {
        let parent = std::mem::replace(&mut self.env, Box::new(Environment::new(None)));
        self.env = Box::new(Environment::new(Some(parent)));
    }

    fn pop_scope(&mut self) {
        let parent = self.env.parent.take().expect("Popped root scope");
        self.env = parent;
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
                _ => Type::Class(n),
            },
            TypeExpr::Union(tys) => Type::Union(
                tys.into_iter()
                    .map(|t| self.resolve_type(t))
                    .collect::<Vec<_>>(),
            ),
            _ => Type::Unknown,
        }
    }

    fn execute_statement(&mut self, stmt: Statement) -> StatementResult {
        match stmt {
            Statement::VarDeclaration { name, ty: _, value } => {
                let val = self.eval_expr(value);
                self.env.insert(name, val);
                StatementResult::None
            }
            Statement::FunctionDeclaration {
                name,
                params,
                return_ty,
                body,
            } => {
                let func_val = Value::Function {
                    name: name.clone(),
                    params: params
                        .into_iter()
                        .map(|(p, ty)| (p, self.resolve_type(ty)))
                        .collect(),
                    return_ty: self.resolve_type(return_ty),
                    body: *body,
                };
                self.env.insert(name, func_val);
                StatementResult::None
            }
            Statement::ClassDeclaration {
                name,
                fields,
                methods,
                constructor,
            } => {
                let mut method_map = HashMap::new();
                for m in methods {
                    method_map.insert(m.name.clone(), m);
                }
                if let Some(cons) = constructor {
                    method_map.insert("constructor".to_string(), cons);
                }
                let field_names = fields.iter().map(|f| f.name.clone()).collect();
                self.classes.insert(name, (field_names, method_map));
                StatementResult::None
            }
            Statement::Return(expr) => {
                let val = self.eval_expr(expr);
                StatementResult::Return(val)
            }
            Statement::Print(expr) => {
                let val = self.eval_expr(expr);
                match val {
                    Value::Int(i) => println!("{}", i),
                    Value::String(s) => println!("{}", s),
                    Value::Boolean(b) => println!("{}", b),
                    Value::Void => (),
                    Value::Null => println!("null"),
                    Value::Instance(c, _) => println!("<Instance of {}>", c),
                    Value::Function { name, .. } => println!("<Function {}>", name),
                }
                StatementResult::None
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
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
            Statement::While { condition, body } => {
                while self.eval_expr(condition.clone()).is_truthy() {
                    let res = self.execute_statement((*body).clone());
                    if let StatementResult::Return(_) = res {
                        return res;
                    }
                }
                StatementResult::None
            }
            Statement::Block(stmts) => {
                self.push_scope();
                let mut final_res = StatementResult::None;
                for s in stmts {
                    let res = self.execute_statement(s);
                    if let StatementResult::Return(_) = res {
                        final_res = res;
                        break;
                    }
                }
                self.pop_scope();
                final_res
            }
            Statement::Expression(expr) => {
                self.eval_expr(expr);
                StatementResult::None
            }
            Statement::Error => StatementResult::None,
        }
    }

    fn eval_expr(&mut self, expr: Expr) -> Value {
        match expr {
            Expr::Number(n) => Value::Int(n),
            Expr::StringLiteral(s) => Value::String(s),
            Expr::Variable(name) => self
                .env
                .lookup(&name)
                .expect(&format!("Undefined variable {}", name)),
            Expr::BinaryOp(lhs, op, rhs) => {
                let left = self.eval_expr(*lhs);
                let right = self.eval_expr(*rhs);
                match (left, right) {
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
                        _ => panic!("Unsupported operator {}", op),
                    },
                    _ => panic!("Operands must be integers for binary op"),
                }
            }
            Expr::Assign(name, val_expr) => {
                let val = self.eval_expr(*val_expr);
                self.env.assign(&name, val.clone());
                val
            }
            Expr::Call(name, args) => {
                let func = self.env.lookup(&name).expect("Function not found");
                if let Value::Function {
                    name: _,
                    params,
                    return_ty: _,
                    body,
                } = func
                {
                    let mut arg_vals = Vec::new();
                    for a in args {
                        arg_vals.push(self.eval_expr(a));
                    }
                    self.push_scope();
                    for (i, (pname, _)) in params.iter().enumerate() {
                        self.env.insert(pname.clone(), arg_vals[i].clone());
                    }
                    let res = self.execute_statement(body);
                    self.pop_scope();
                    if let StatementResult::Return(v) = res {
                        v
                    } else {
                        Value::Void
                    }
                } else {
                    panic!("Not a function");
                }
            }
            Expr::MethodCall(obj_expr, method, args) => {
                let obj = self.eval_expr(*obj_expr);
                if let Value::Instance(class_name, fields_ref) = obj {
                    let m = {
                        let (_, methods) = self.classes.get(&class_name).expect("Class not found");
                        methods.get(&method).expect("Method not found").clone()
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
                } else {
                    panic!("Not an instance");
                }
            }
            Expr::This => self
                .env
                .lookup("this")
                .expect("Usage of this outside of class context"),
            Expr::New(class_name, args) => {
                let (field_names, methods) = {
                    let (fnms, mths) = self
                        .classes
                        .get(&class_name)
                        .expect(&format!("Class {} not found", class_name));
                    (fnms.clone(), mths.clone())
                };

                let mut instance_fields = HashMap::new();
                for f in field_names {
                    instance_fields.insert(f, Value::Null);
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
                        self.env.insert(pname.clone(), arg_vals[i].clone());
                    }

                    self.execute_statement((*cons.body).clone());

                    // After constructor, 'this' might have changed fields
                    let updated_instance = self.env.lookup("this").unwrap();
                    self.pop_scope();
                    updated_instance
                } else {
                    instance
                }
            }
            Expr::MemberAccess(obj_expr, member) => {
                let obj = self.eval_expr(*obj_expr);
                if let Value::Instance(_, fields) = obj {
                    fields
                        .borrow()
                        .get(&member)
                        .cloned()
                        .expect("Field not found")
                } else {
                    panic!("Not an instance");
                }
            }
            Expr::MemberAssign(obj_expr, member, val_expr) => {
                let obj = self.eval_expr(*obj_expr);
                let val = self.eval_expr(*val_expr);
                if let Value::Instance(_, fields) = obj {
                    fields.borrow_mut().insert(member, val.clone());
                    val
                } else {
                    panic!("Not an instance");
                }
            }
            Expr::TypeTest(expr, ty_expr) => {
                let val = self.eval_expr(*expr);
                let target_ty = self.resolve_type(ty_expr);
                // Check if val matches target_ty
                match (val, target_ty) {
                    (Value::Int(_), Type::Int32) => Value::Boolean(true),
                    (Value::String(_), Type::String) => Value::Boolean(true),
                    _ => Value::Boolean(false), // Simple match for now
                }
            }
            Expr::Error => panic!("Compiler bug: reaching error node in interpreter"),
        }
    }
}
