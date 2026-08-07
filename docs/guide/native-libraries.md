# Native Library Template

Aura packages can compile package-owned C sources directly into the final
binary. Paths are manifest-relative; sources are compiled as objects and are
therefore statically included in the executable.

```toml
[package]
name = "demo.native"
version = "0.1.0"

[native.sqlite]
sources = ["native/sqlite3.c", "native/sqlite_ffi.c"]
include_dirs = ["native/include"]
defines = ["SQLITE_THREADSAFE=1"]
static = true

[target."aarch64-unknown-linux-gnu".native.sqlite]
sources = ["native/sqlite3-arm.c"]
```

Use explicit-length ABI views for borrowed data and `SQLITE_TRANSIENT` (or an
equivalent copy) whenever native code retains text or binary values. String
and `std.bytes.Buffer` arguments are valid only for the synchronous foreign
call; native code must copy them before returning if it needs them later.
Empty values have a non-owning null/length-zero representation, while embedded
NUL bytes are preserved by the length. Wrap opaque resources in
`ForeignHandle<T>`: Aura code cannot inspect the pointer, retain/release keeps
task captures alive, pinning is synchronous, and the destructor must invalidate
the handle before running child cleanup. Make `close()` idempotent and perform
lexical cleanup explicitly at the owning scope.

Native build failures include the compiler, source, include directories,
defines, linker arguments, and link mode. Successful builds write
`<artifact>.native.meta` with source SHA-256 checksums and active settings.

Copy `examples/native-library-template` for a minimal package containing a
static C library, header, ABI-version entry point, and Aura declarations. For
native tests, put C fixtures under the same `[native.*]` source declaration and
run `aura test --race` to compile the test artifact with ASAN/UBSAN.
`@foreign` declarations should remain restricted to primitive scalars,
pointer-plus-length buffers, and opaque handles.
