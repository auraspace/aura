use crate::compiler::ast::Statement;
use crate::compiler::sema::ty::Type;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum StatementResult {
    None,
    Return(Value),
    Throw(Value),
}

pub enum Value {
    Int(i32),
    Int64(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Instance(String, Rc<RefCell<HashMap<String, Value>>>), // class_name, fields
    Function {
        name: Option<String>,
        params: Vec<(String, Type)>,
        return_ty: Type,
        body: Statement,
        is_async: bool,
        captured_env: Vec<Rc<RefCell<HashMap<String, Value>>>>,
    },
    Void,
    Null,
    Class(String),
    Array(Rc<RefCell<Vec<Value>>>),
    Promise(Box<Value>),
    NativeFunction(NativeFunc),
}

pub type NativeFunc = std::rc::Rc<dyn Fn(Vec<Value>) -> Value>;

impl Clone for Value {
    fn clone(&self) -> Self {
        match self {
            Value::Int(i) => Value::Int(*i),
            Value::Int64(i) => Value::Int64(*i),
            Value::Float(f) => Value::Float(*f),
            Value::String(s) => Value::String(s.clone()),
            Value::Boolean(b) => Value::Boolean(*b),
            Value::Instance(name, fields) => Value::Instance(name.clone(), fields.clone()),
            Value::Function {
                name,
                params,
                return_ty,
                body,
                is_async,
                captured_env,
            } => Value::Function {
                name: name.clone(),
                params: params.clone(),
                return_ty: return_ty.clone(),
                body: body.clone(),
                is_async: *is_async,
                captured_env: captured_env.clone(),
            },
            Value::Void => Value::Void,
            Value::Null => Value::Null,
            Value::Class(name) => Value::Class(name.clone()),
            Value::Array(elems) => Value::Array(elems.clone()),
            Value::Promise(val) => Value::Promise(val.clone()),
            Value::NativeFunction(f) => Value::NativeFunction(f.clone()),
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(i) => write!(f, "Int({})", i),
            Value::Int64(i) => write!(f, "Int64({})", i),
            Value::Float(val) => write!(f, "Float({})", val),
            Value::String(s) => write!(f, "String({:?})", s),
            Value::Boolean(b) => write!(f, "Boolean({})", b),
            Value::Instance(name, fields) => write!(f, "Instance({}, {:?})", name, fields),
            Value::Function { name, .. } => write!(f, "Function({:?})", name),
            Value::Void => write!(f, "Void"),
            Value::Null => write!(f, "Null"),
            Value::Class(name) => write!(f, "Class({})", name),
            Value::Array(elems) => write!(f, "Array({:?})", elems.borrow()),
            Value::Promise(val) => write!(f, "Promise({:?})", val),
            Value::NativeFunction(_) => write!(f, "NativeFunction"),
        }
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            Value::Null => false,
            _ => true,
        }
    }
}

pub struct Environment {
    pub symbols: Rc<RefCell<HashMap<String, Value>>>,
    pub parent: Option<Box<Environment>>,
}

impl Environment {
    pub fn new(parent: Option<Box<Environment>>) -> Self {
        Self {
            symbols: Rc::new(RefCell::new(HashMap::new())),
            parent,
        }
    }

    pub fn insert(&mut self, name: String, val: Value) {
        self.symbols.borrow_mut().insert(name, val);
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        if let Some(val) = self.symbols.borrow().get(name) {
            Some(val.clone())
        } else if let Some(ref parent) = self.parent {
            parent.lookup(name)
        } else {
            None
        }
    }

    pub fn assign(&mut self, name: &str, val: Value) -> bool {
        if self.symbols.borrow().contains_key(name) {
            self.symbols.borrow_mut().insert(name.to_string(), val);
            true
        } else if let Some(ref mut parent) = self.parent {
            parent.assign(name, val)
        } else {
            false
        }
    }
}
