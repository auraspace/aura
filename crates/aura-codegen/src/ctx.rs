//! Emission context and options.

use std::collections::{HashMap, HashSet};

use aura_sema::{CheckedFile, Ty};

pub(crate) struct EmitCtx<'a> {
    pub(crate) checked: &'a CheckedFile,
    /// Mono class key for `this` (e.g. `Box_String` or `User`).
    pub(crate) method_class: Option<&'a str>,
    pub(crate) type_params: Vec<String>,
    pub(crate) type_args: Vec<Ty>,
    /// Local name → type key (`Int`, `Box_String`, `Named`, …)
    pub(crate) locals: Vec<HashMap<String, String>>,
    /// Per-scope locals that own an `Array` heap buffer (C3t).
    pub(crate) array_owners: Vec<HashSet<String>>,
    /// Per-scope heap-class locals registered as GC roots (C5g).
    pub(crate) gc_roots: Vec<HashSet<String>>,
    /// Per-scope Array-of-class locals registered for element GC mark (C6e).
    pub(crate) array_gc_roots: Vec<HashSet<String>>,
}

impl<'a> EmitCtx<'a> {
    pub(crate) fn push_scope(&mut self) {
        self.locals.push(HashMap::new());
        self.array_owners.push(HashSet::new());
        self.gc_roots.push(HashSet::new());
        self.array_gc_roots.push(HashSet::new());
    }

    pub(crate) fn pop_scope(&mut self) {
        self.locals.pop();
        self.array_owners.pop();
        self.gc_roots.pop();
        self.array_gc_roots.pop();
    }

    pub(crate) fn define_local(&mut self, name: &str, ty: String) {
        if let Some(scope) = self.locals.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    pub(crate) fn mark_array_owner(&mut self, name: &str) {
        if let Some(scope) = self.array_owners.last_mut() {
            scope.insert(name.to_string());
        }
    }

    /// C5b: drop ownership (after move into another local).
    pub(crate) fn unmark_array_owner(&mut self, name: &str) {
        for scope in self.array_owners.iter_mut().rev() {
            if scope.remove(name) {
                return;
            }
        }
    }

    /// C4r: is this local marked as owning an Array buffer?
    pub(crate) fn is_array_owner(&self, name: &str) -> bool {
        self.array_owners.iter().any(|s| s.contains(name))
    }

    /// All Array-owning locals from innermost to outermost (for free-before-return).
    pub(crate) fn array_owners_all(&self) -> Vec<String> {
        let mut out = Vec::new();
        for scope in self.array_owners.iter().rev() {
            let mut names: Vec<_> = scope.iter().cloned().collect();
            names.sort();
            out.extend(names);
        }
        out
    }

    pub(crate) fn array_owners_current(&self) -> Vec<String> {
        self.array_owners
            .last()
            .map(|s| {
                let mut names: Vec<_> = s.iter().cloned().collect();
                names.sort();
                names
            })
            .unwrap_or_default()
    }

    pub(crate) fn mark_gc_root(&mut self, name: &str) {
        if let Some(scope) = self.gc_roots.last_mut() {
            scope.insert(name.to_string());
        }
    }

    pub(crate) fn gc_roots_current(&self) -> Vec<String> {
        self.gc_roots
            .last()
            .map(|s| {
                let mut names: Vec<_> = s.iter().cloned().collect();
                names.sort();
                names
            })
            .unwrap_or_default()
    }

    pub(crate) fn gc_roots_all(&self) -> Vec<String> {
        let mut out = Vec::new();
        for scope in self.gc_roots.iter().rev() {
            let mut names: Vec<_> = scope.iter().cloned().collect();
            names.sort();
            out.extend(names);
        }
        out
    }

    pub(crate) fn mark_array_gc_root(&mut self, name: &str) {
        if let Some(scope) = self.array_gc_roots.last_mut() {
            scope.insert(name.to_string());
        }
    }

    pub(crate) fn array_gc_roots_current(&self) -> Vec<String> {
        self.array_gc_roots
            .last()
            .map(|s| {
                let mut names: Vec<_> = s.iter().cloned().collect();
                names.sort();
                names
            })
            .unwrap_or_default()
    }

    pub(crate) fn array_gc_roots_all(&self) -> Vec<String> {
        let mut out = Vec::new();
        for scope in self.array_gc_roots.iter().rev() {
            let mut names: Vec<_> = scope.iter().cloned().collect();
            names.sort();
            out.extend(names);
        }
        out
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
