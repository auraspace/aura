//! Multi-file package loading and minimal `aura.toml` (C3e).

mod load;
mod lock;
mod toml;
mod types;
mod util;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use load::{load_package, load_package_default};
pub use types::LoadedPackage;
