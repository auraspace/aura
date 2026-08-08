use crate::options::{Backend, CompileOptions, ProfileSettings};

/// Build defaults for LLVM without importing C backend helpers.
pub fn options() -> CompileOptions {
    let mut options = CompileOptions::default();
    options.backend = Backend::Llvm;
    options.runtime_abi = None;
    options.profile_settings = ProfileSettings {
        backend: Backend::Llvm,
        ..options.profile_settings
    };
    options
}
