use crate::compiler::ast::Span;
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: Type,
    pub is_param: bool,
    pub is_const: bool,
    pub is_exported: bool,
    pub span: Span,
    pub defined_in: String,
    pub doc: Option<String>,
}

pub struct Scope {
    pub symbols: HashMap<String, Symbol>,
    pub parent: Option<Box<Scope>>,
}

impl Scope {
    pub fn new(parent: Option<Box<Scope>>) -> Self {
        Self {
            symbols: HashMap::new(),
            parent,
        }
    }

    pub fn insert(
        &mut self,
        name: String,
        ty: Type,
        is_param: bool,
        is_const: bool,
        is_exported: bool,
        span: Span,
        defined_in: String,
        doc: Option<String>,
    ) {
        let sym = Symbol {
            name: name.clone(),
            ty,
            is_param,
            is_const,
            is_exported,
            span,
            defined_in,
            doc,
        };
        self.symbols.insert(name, sym);
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        if let Some(sym) = self.symbols.get(name) {
            Some(sym)
        } else if let Some(ref parent) = self.parent {
            parent.lookup(name)
        } else {
            None
        }
    }

    pub fn lookup_local(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}
