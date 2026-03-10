use crate::compiler::ast::Statement;
use crate::compiler::sema::ty::Type;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub enum Value {
    Int(i32),
    String(String),
    Boolean(bool),
    Instance(String, Rc<RefCell<HashMap<String, Value>>>), // class_name, fields
    Function {
        name: String,
        params: Vec<(String, Type)>,
        return_ty: Type,
        body: Statement,
        is_async: bool,
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
            Value::String(s) => Value::String(s.clone()),
            Value::Boolean(b) => Value::Boolean(*b),
            Value::Instance(name, fields) => Value::Instance(name.clone(), fields.clone()),
            Value::Function {
                name,
                params,
                return_ty,
                body,
                is_async,
            } => Value::Function {
                name: name.clone(),
                params: params.clone(),
                return_ty: return_ty.clone(),
                body: body.clone(),
                is_async: *is_async,
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
            Value::String(s) => write!(f, "String({:?})", s),
            Value::Boolean(b) => write!(f, "Boolean({})", b),
            Value::Instance(name, fields) => write!(f, "Instance({}, {:?})", name, fields),
            Value::Function { name, .. } => write!(f, "Function({})", name),
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
