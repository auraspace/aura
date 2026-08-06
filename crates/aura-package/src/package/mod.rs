//! Multi-file package loading and minimal `aura.toml` (C3e).

#[allow(dead_code)]
mod archive;
mod fetch;
mod load;
mod lock;
mod manifest;
mod origin;
mod registry;
mod semver;
mod toml;
mod types;
mod util;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use fetch::ENV_REGISTRY_TOKEN;
#[cfg(test)]
pub use fetch::{
    cache_root_from_env, crate_source_for_meta, default_cache_root, expand_dl_template,
    install_from_bytes, is_package_installed, local_crate_path, normalize_cksum, package_src_dir,
    read_crate_bytes, sha256_hex, verify_sha256, ENV_REGISTRY_CACHE,
};
pub use load::{
    load_package, load_package_default, load_package_read_only, load_package_read_only_with_std,
};
pub use manifest::{add_dependency, remove_dependency};
pub use registry::{activate_update, current_target, RegistryIndex, UpdateDecision};
#[cfg(test)]
pub use registry::{
    default_index_path, index_root_from_env, RegistryConfig, VersionMeta, ENV_REGISTRY_INDEX,
};
#[cfg(test)]
pub use semver::{parse_req, parse_version, OriginLockPin, Version, VersionReq};
pub use types::LoadedPackage;
