use crate::compiler::ast::Statement;
use crate::compiler::sema::ty::Type;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
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
    },
    Void,
    Null,
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
