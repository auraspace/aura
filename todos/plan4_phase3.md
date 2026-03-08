# Plan 4 Phase 3: FFI & System Hooks

## 📌 Goals

Implement the macOS/Linux syscall wrappers for I/O and networking. This provides the standard library with a way to interact with the host OS.

## 📝 Tasks

- [x] Create `src/runtime/ffi/mod.rs` — define common FFI traits and generic entry points.
- [x] Implement `src/runtime/ffi/io.rs` — basic raw file I/O operations (read, write, open, close) mapping to `libc` calls.
- [x] Implement `src/runtime/ffi/net.rs` — basic networking (TCP streams and listeners) mapping to native sockets.
- [x] Expose the FFI module through `src/runtime/mod.rs`.
- [x] Write unit tests validating safe FFI boundary behavior.
- [x] Add `libc` dependency to `Cargo.toml`.
