//! Multi-file package loading and minimal `aura.toml` (C3e).

mod load;
mod lock;
mod registry;
mod toml;
mod types;
mod util;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use load::{load_package, load_package_default};
pub use registry::{
    default_index_path, index_root_from_env, RegistryConfig, RegistryIndex, VersionMeta,
    ENV_REGISTRY_INDEX,
};
pub use types::LoadedPackage;
