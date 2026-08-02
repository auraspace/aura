//! Emit C99 for Aura C0–C3b programs and shell out to a C compiler.
//!
//! Classes/structs → C structs; interfaces/enums → tagged unions.

// The emitter keeps type-specific branches explicit because each branch
// documents a distinct ownership/layout case, even when generated text is
// currently identical.
#![allow(clippy::if_same_then_else)]

mod array_emit;
mod async_model;
mod build;
mod cache;
mod call_emit;
mod class_emit;
mod ctx;
mod driver;
mod emit;
mod enum_emit;
mod error;
mod expr;
mod iface;
mod names;
mod options;
mod runtime_abi;
mod stmt;
mod validation;

pub use build::{
    build_from_checked, build_from_file, build_tests_from_checked, build_tests_from_file,
    emit_c_from_ast, emit_c_from_checked, emit_c_tests_from_ast,
};
pub use cache::{ArtifactCache, ArtifactCacheKey, CacheError};
pub use ctx::EmitOptions;
pub use driver::{build_artifact, build_artifact_from_checked, Artifact, BuildIdentity};
pub use emit::{emit_c, emit_c_with};
pub use error::CodegenError;
pub use options::{
    Backend, CompileOptions, CompileOptionsBuilder, DiagnosticMode, Lto, OptimizationLevel,
    OptionsError, OutputKind, PanicStrategy, Profile, ProfileSettings, ProfileSettingsError,
    RuntimeAbi, Target,
};
pub use runtime_abi::{ID as RUNTIME_ABI_ID, VERSION as RUNTIME_ABI_VERSION};
