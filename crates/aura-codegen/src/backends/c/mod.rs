//! Legacy C backend implementation.
//!
//! All C-specific lowering, ownership emission, runtime ABI, and native build
//! glue lives in this module. The parent crate exposes only the stable driver
//! and option contracts to other backends.

pub(crate) mod array_emit;
pub(crate) mod async_compat;
pub(crate) mod build;
pub(crate) mod call_emit;
pub(crate) mod class_emit;
pub(crate) mod ctx;
pub(crate) mod emit;
pub(crate) mod enum_emit;
pub(crate) mod expr;
pub(crate) mod iface;
pub(crate) mod mir_emit;
pub(crate) mod names;
pub(crate) mod runtime_abi;
pub(crate) mod stmt;

/// Adapter marker for the legacy C implementation.
pub struct CBackend;
