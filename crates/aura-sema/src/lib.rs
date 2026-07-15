//! Name resolution + typecheck for Aura C0–C3b (enums, match, Result).

mod checker;
mod error;
mod sigs;
mod ty;
mod util;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use error::SemaError;
pub use sigs::*;
pub use ty::{nominal_key, nominal_mono_base, split_nominal, Ty};

use aura_ast::File;
use checker::Checker;

/// Typecheck a parsed file.
pub fn check_file(file: &File) -> Result<CheckedFile, SemaError> {
    let mut c = Checker::new();
    c.check_file(file)
}
