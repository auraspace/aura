//! The compiler-side identity of the C runtime contract.

/// ABI identity shared by generated artifacts and `runtime/runtime.c`.
///
/// The identity covers the currently shipped task, value, exception, channel,
/// GC, I/O, FFI, and type-erased payload surface. Patch-level runtime fixes
/// must preserve this value; a layout or calling-convention change must change
/// it before release.
pub const ID: &str = aura_ir::intrinsic_registry::RUNTIME_ABI_ID;

/// Major ABI version retained for artifact/debug metadata.
pub const VERSION: u32 = aura_ir::intrinsic_registry::RUNTIME_ABI_VERSION;
