//! Multi-file package loading and minimal `aura.toml` (C3e).

#[allow(dead_code)]
mod archive;
mod fetch;
mod integrity;
mod load;
mod lock;
mod manifest;
mod origin;
mod proxy;
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
pub use integrity::{ChecksumDatabase, ChecksumRecord};
/// Parse-only entry points used by fuzzers and editor integrations.
pub use load::parse_manifest_for_fuzz;
pub use load::{
    dependency_graph, load_package, load_package_default, load_package_read_only,
    load_package_read_only_with_std, load_workspace, workspace_members, DependencyNode,
};
pub use lock::parse_lockfile_for_fuzz;
pub use manifest::{add_dependency, remove_dependency};
pub use proxy::ProxyReadThrough;
pub use registry::{
    activate_update, current_target, RegistryIndex, UpdateDecision, DEFAULT_REGISTRY_PROXY,
    ENV_REGISTRY_INDEX,
};
#[cfg(test)]
pub use registry::{default_index_path, index_root_from_env, RegistryConfig, VersionMeta};
#[cfg(test)]
pub use semver::{parse_req, parse_version, OriginLockPin, Version, VersionReq};
pub use toml::NativeBuildConfig;
pub use types::LoadedPackage;
