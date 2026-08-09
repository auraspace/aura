//! Emit C99 for Aura C0–C3b programs and shell out to a C compiler.
//!
//! Classes/structs → C structs; interfaces/enums → tagged unions.

// The emitter keeps type-specific branches explicit because each branch
// documents a distinct ownership/layout case, even when generated text is
// currently identical.
#![allow(clippy::if_same_then_else)]

pub mod backends;
mod cache;
mod driver;
mod error;
mod options;
mod validation;

// Keep existing crate-internal paths stable while the C implementation is
// physically isolated under `backends::c`.
pub(crate) use backends::c::{
    array_emit, async_compat, build, call_emit, class_emit, ctx, emit, enum_emit, expr, iface,
    mir_emit, names, runtime_abi, stmt,
};

pub use aura_hir::TypedHir;
pub use aura_ir::{CheckedIr, Effect, FunctionIr, LoweredProgram, OwnershipMode, ValueFact};
pub use backends::llvm::options as llvm_options;
pub use backends::llvm::LlvmBackend;
pub use build::{
    build_from_checked, build_from_checked_with_native, build_from_checked_with_options,
    build_from_file, build_tests_from_checked, build_tests_from_checked_with_native,
    build_tests_from_file, emit_c_from_ast, emit_c_from_checked, emit_c_tests_from_ast,
};
pub use cache::{ArtifactCache, ArtifactCacheKey, CacheError};
pub use ctx::EmitOptions;
pub use driver::{
    build_artifact, build_artifact_from_checked, Artifact, Backend as CodegenBackend,
    BackendBuildOptions, BackendCapabilities, BackendOptions, BuildIdentity, CBackend,
    CBackendCompatibility, CBackendDriver, Driver, MirBackend,
};
pub use emit::{emit_c, emit_c_with};
pub use error::CodegenError;
pub use options::{
    Backend, CompileOptions, CompileOptionsBuilder, DiagnosticMode, Lto, NativeSource,
    OptimizationLevel, OptionsError, OutputKind, PanicStrategy, Profile, ProfileSettings,
    ProfileSettingsError, RuntimeAbi, Target,
};
pub use runtime_abi::{ID as RUNTIME_ABI_ID, VERSION as RUNTIME_ABI_VERSION};
