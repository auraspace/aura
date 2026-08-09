//! Typed high-level IR produced by semantic analysis.

use aura_sema::CheckedFile;

/// Semantic facts consumed by lowering. HIR owns no backend or MIR state.
#[derive(Debug, Clone)]
pub struct TypedHir {
    checked: CheckedFile,
}

impl TypedHir {
    pub fn new(checked: CheckedFile) -> Self {
        Self { checked }
    }

    pub fn checked(&self) -> &CheckedFile {
        &self.checked
    }

    pub fn into_checked(self) -> CheckedFile {
        self.checked
    }
}
