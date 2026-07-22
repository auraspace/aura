//! Emit C99 for Aura C0–C3b programs and shell out to a C compiler.
//!
//! Classes/structs → C structs; interfaces/enums → tagged unions.

mod array_emit;
mod build;
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

pub use build::{build_from_file, build_tests_from_file, emit_c_from_ast, emit_c_tests_from_ast};
pub use ctx::EmitOptions;
pub use driver::{build_artifact, Artifact, BuildIdentity};
pub use emit::{emit_c, emit_c_with};
pub use error::CodegenError;
pub use options::{
    Backend, CompileOptions, CompileOptionsBuilder, DiagnosticMode, Lto, OptimizationLevel,
    OptionsError, OutputKind, PanicStrategy, Profile, ProfileSettings, ProfileSettingsError,
    RuntimeAbi, Target,
};
pub use runtime_abi::{ID as RUNTIME_ABI_ID, VERSION as RUNTIME_ABI_VERSION};
