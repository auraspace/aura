//! Emission context and options.

use std::collections::HashMap;

use aura_sema::{CheckedFile, Ty};

pub(crate) struct EmitCtx<'a> {
    pub(crate) checked: &'a CheckedFile,
    /// Mono class key for `this` (e.g. `Box_String` or `User`).
    pub(crate) method_class: Option<&'a str>,
    pub(crate) type_params: Vec<String>,
    pub(crate) type_args: Vec<Ty>,
    /// Local name → type key (`Int`, `Box_String`, `Named`, …)
    pub(crate) locals: Vec<HashMap<String, String>>,
}

impl<'a> EmitCtx<'a> {
    pub(crate) fn push_scope(&mut self) {
        self.locals.push(HashMap::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.locals.pop();
    }

    pub(crate) fn define_local(&mut self, name: &str, ty: String) {
        if let Some(scope) = self.locals.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    pub(crate) fn lookup_local(&self, name: &str) -> Option<&str> {
        for scope in self.locals.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.as_str());
            }
        }
        None
    }
}

/// Emit options for the C backend.
#[derive(Debug, Clone, Copy, Default)]
pub struct EmitOptions {
    /// When true, `aura_main` runs `@test` functions instead of `main`.
    pub test: bool,
}
