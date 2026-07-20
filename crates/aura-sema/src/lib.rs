//! Name resolution + typecheck for Aura C0–C3b (enums, match, Result).

mod checker;
mod error;
mod sigs;
mod ty;
mod util;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use error::{SemaError, SemaErrors};
pub use sigs::*;
pub use ty::{nominal_key, nominal_mono_base, split_nominal, Ty};

use aura_ast::File;
use checker::Checker;

/// Typecheck a parsed file.
///
/// C6h: body-level errors are collected so multiple issues can be reported in
/// one `aura check` run. Declaration failures still abort early.
pub fn check_file(file: &File) -> Result<CheckedFile, SemaErrors> {
    let mut c = Checker::new();
    match c.check_file(file) {
        Ok(checked) => {
            if c.errors.is_empty() {
                Ok(checked)
            } else {
                Err(SemaErrors::new(std::mem::take(&mut c.errors)))
            }
        }
        Err(e) => {
            let mut errors = std::mem::take(&mut c.errors);
            errors.insert(0, e);
            Err(SemaErrors::new(errors))
        }
    }
}
