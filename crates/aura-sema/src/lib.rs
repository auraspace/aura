//! Name resolution + typecheck for Aura C0–C3b (enums, match, Result).

mod attributes;
mod checker;
mod derive;
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
pub use util::{subst_ty, type_subst_map};

use aura_ast::File;
use checker::Checker;

/// Typecheck a parsed file.
///
/// C6h/C7g: body- and declaration-level errors are collected so multiple
/// issues can be reported in one `aura check` run when processing can continue.
pub fn check_file(file: &File) -> Result<CheckedFile, SemaErrors> {
    let mut expanded = file.clone();
    let mut c = Checker::new();
    c.errors.extend(derive::expand_equals(&mut expanded));
    c.errors.extend(derive::expand_hash(&mut expanded));
    c.errors.extend(derive::expand_debug(&mut expanded));
    c.errors.extend(attributes::validate_file(&expanded));
    match c.check_file(&expanded) {
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
