//! Multi-file package loading and minimal `aura.toml` (C3e).

mod fetch;
#[allow(dead_code)]
mod archive;
mod load;
mod lock;
mod publish;
mod registry;
mod semver;
mod toml;
mod types;
mod util;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
pub use fetch::{
    cache_root_from_env, crate_source_for_meta, default_cache_root, ensure_installed,
    expand_dl_template, fetch_and_install, install_from_bytes, is_package_installed,
    local_crate_path, normalize_cksum, package_src_dir, read_crate_bytes, sha256_hex,
    verify_sha256, ENV_REGISTRY_CACHE,
};
pub use load::{load_package, load_package_default};
pub use publish::{publish_dry_run, publish_package};
pub use registry::PublishErrorKind;
pub use fetch::ENV_REGISTRY_TOKEN;
pub use registry::{activate_update, current_target, RegistryIndex, UpdateActivation, UpdateDecision};
#[cfg(test)]
pub use registry::{
    default_index_path, index_root_from_env, RegistryConfig, VersionMeta,
    ENV_REGISTRY_INDEX,
};
#[cfg(test)]
pub use semver::{
    lock_pin_from_meta, parse_req, parse_version, resolve, resolve_lock_pin, RegistryLockPin,
    Version, VersionReq,
};
pub use types::LoadedPackage;
