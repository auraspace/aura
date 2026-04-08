use std::collections::HashMap;

use aura_span::Span;

use crate::types::Ty;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarInfo {
    pub(crate) ty: Ty,
    pub(crate) mutable: bool,
    pub(crate) decl_span: Span,
}

#[derive(Clone, Debug)]
pub(crate) struct Env {
    pub(crate) scopes: Vec<HashMap<String, VarInfo>>,
}

impl Env {
    pub(crate) fn new(globals: HashMap<String, VarInfo>) -> Self {
        Self {
            scopes: vec![globals],
        }
    }

    pub(crate) fn child(&self) -> Self {
        Self {
            scopes: vec![self.scopes[0].clone()],
        }
    }

    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub(crate) fn declare(&mut self, name: String, info: VarInfo) {
        let current = self.scopes.last_mut().unwrap();
        current.insert(name, info);
    }

    pub(crate) fn define(&mut self, name: String, ty: Ty, mutable: bool) {
        self.declare(
            name,
            VarInfo {
                ty,
                mutable,
                decl_span: aura_span::Span::empty(aura_span::BytePos::new(0)),
            },
        );
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v);
            }
        }
        None
    }
}
