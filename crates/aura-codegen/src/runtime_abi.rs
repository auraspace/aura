//! The compiler-side identity of the C runtime contract.

/// ABI identity shared by generated artifacts and `runtime/aura_rt.c`.
///
/// The identity covers the currently shipped task, value, exception, channel,
/// GC, I/O, and FFI surface. Patch-level runtime fixes must preserve this
/// value; a layout or calling-convention change must change it before release.
pub const ID: &str = "aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1";

/// Major ABI version retained for artifact/debug metadata.
pub const VERSION: u32 = 1;
