//! Multi-file package loading and minimal `aura.toml` (C3e).

mod fetch;
mod load;
mod lock;
mod registry;
mod semver;
mod toml;
mod types;
mod util;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub use fetch::{
    cache_root_from_env, default_cache_root, expand_dl_template, fetch_and_install,
    install_from_bytes, normalize_cksum, package_src_dir, read_crate_bytes, sha256_hex,
    verify_sha256, ENV_REGISTRY_CACHE,
};
pub use load::{load_package, load_package_default};
pub use registry::{
    default_index_path, index_root_from_env, RegistryConfig, RegistryIndex, VersionMeta,
    ENV_REGISTRY_INDEX,
};
pub use semver::{
    lock_pin_from_meta, parse_req, parse_version, resolve, resolve_lock_pin, RegistryLockPin,
    Version, VersionReq,
};
pub use types::LoadedPackage;
