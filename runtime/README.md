# Aura Runtime

Aura programs produced by the C backend link against this native C runtime. It
provides the compiler/runtime ABI for memory management, exceptions, tasks,
channels, native I/O, HTTP, FFI handles, and selected standard-library
primitives.

The runtime is shipped both as the compatibility translation unit and as
linkable static artifacts. `runtime.c` remains the source fallback for embedded
CLI installs; `make` produces `libaurart.a` for C and `make llvm` produces
`libaurart-llvm.a` for LLVM. The archives can be linked directly without
recompiling the runtime for every Aura program.

## Build Contract

The current native build is conceptually:

```text
generated Aura C + runtime/runtime.c + foreign libraries -> native executable

or:

generated Aura C + runtime/libaurart.a + foreign libraries -> native executable

or:

generated Aura LLVM + runtime/libaurart-llvm.a + foreign libraries -> native executable
```

Keeping one runtime translation unit has a few deliberate properties:

- Static helpers stay private without maintaining a large internal header.
- The compiler invokes the system C compiler once for the generated program and
  runtime.
- Installed CLI builds can embed and restore the exact runtime source set.
- LTO can optimize across generated code and runtime code when enabled.

## Linkable Artifact

Build the C archive with `make` and the LLVM archive with `make llvm`. Set
`AURA_RUNTIME_LIB` for C or `AURA_LLVM_RUNTIME_LIB` for LLVM; without those
variables, the source compatibility path remains available.

The include order in `runtime.c` is therefore part of the build contract. A
module may use declarations from earlier modules, but should not depend on a
later module unless the declaration belongs in `src/core/preamble.c` or an internal
header introduced by a future refactor.

## Current Layout

```text
runtime/
|-- runtime.c          # Translation-unit entrypoint and include order
|-- aura_runtime_abi.h # Versioned compiler/runtime ABI identity
|-- aura_ffi.h         # Public ABI for native/foreign callers
|-- src/
|   |-- core/           # Preamble, primitives, exceptions, process
|   |-- memory/         # Allocation, GC roots, tracing, ownership
|   |-- encoding/       # Text encoding and JSON
|   |-- crypto/         # Hashing, HMAC, random bytes, compression
|   |-- task/           # Frames, executor, and channels
|   |-- io/             # Files, sockets, TLS, WebSocket, and operations
|   |-- http/           # Parser, response, and connection handling
|   |-- ffi/            # FFI handles and race-detection ABI
|   `-- stdlib/         # Native standard-library glue
|-- tests/             # Native unit, ABI, concurrency, and sanitizer fixtures
`-- README.md
```

`runtime.c` includes these files in dependency order:

```text
src/core/preamble.c
src/core/core.c
src/core/string.c
src/encoding/encoding.c
src/encoding/json.c
src/crypto/crypto.c
src/io/dns.c
src/io/{io_file,io_tcp,io_udp,io_websocket,io_tls}.c
src/http/{url,mime}.c
src/http/{http_parser,http_response,http_connection}.c
src/stdlib/fs.c
src/stdlib/stdlib_io_fs.c
src/core/exceptions.c
src/memory/gc.c
src/memory/ownership.c
src/ffi/{ffi,abi_race}.c
src/task/{task_frame,task_executor}.c
src/io/io_operations.c
src/task/task_channel.c
src/core/process.c
```

The files are grouped by subsystem while remaining one amalgamated translation
unit. The grouping is organizational; it does not yet introduce separate
headers or independently linkable runtime objects.

Subsystem contents:

| Area                         | Current files                               |
| ---------------------------- | ------------------------------------------- |
| Core                         | `src/core/`                                 |
| Memory and ownership         | `src/memory/gc.c`, `src/memory/ownership.c` |
| Tasks and channels           | `src/task/`                                 |
| Native I/O                   | `src/io/`                                   |
| HTTP                         | `src/http/`                                 |
| Encoding and JSON            | `src/encoding/`                             |
| Crypto and compression       | `src/crypto/`                               |
| FFI and race detection       | `src/ffi/`                                  |
| Standard-library native glue | `src/stdlib/`                               |

The memory subsystem now contains only GC lifecycle/tracing and ownership
helpers. New unrelated features should be placed in the owning subsystem
directory rather than being added to either memory module.

## ABI Boundaries

There are two related but distinct contracts:

1. **Compiler/runtime ABI**: symbols, layouts, and callbacks emitted by
   `aura-codegen` and implemented by the runtime. Its identity and version live
   in `../crates/aura-ir/src/intrinsic_registry.rs` and are exported through
   `aura_runtime_abi.h`.
2. **Foreign C ABI**: supported handles, views, callbacks, and operations
   declared in `aura_ffi.h` for native integrations.

Changing an exported symbol, enum value, struct layout, ownership rule, callback
contract, or error status may be an ABI change even when all repository tests
still compile. Such changes must update the compiler and runtime together and
should include an ABI fixture.

Internal helpers should be `static` where possible. Do not expose a runtime
implementation detail merely to avoid defining a narrow internal interface.

## Ownership Rules

Every boundary must make ownership explicit:

- Document whether pointers are borrowed, retained, transferred, pinned, or
  newly allocated.
- A successful transfer has exactly one owner after the call.
- A destructor or drop callback must be safe for the states documented by its
  API and must run at most once for one ownership unit.
- Task suspension must not retain borrowed stack storage.
- Objects visible to GC across suspension must be represented by registered
  roots or typed frame tracing hooks.
- Native handles crossing asynchronous boundaries must use the FFI pin/retain
  contract rather than raw pointer lifetime assumptions.

Ownership behavior is part of the ABI, not an implementation comment that can
be changed locally.

## Concurrency Rules

- Shared runtime state must have a documented synchronization owner.
- Do not mix atomic and non-atomic access to the same state.
- Callbacks invoked while holding a lock require special care because they can
  re-enter the runtime or acquire another subsystem lock.
- A task may only be queued once at a time.
- Reactor registration, cancellation, and completion must agree on who clears
  wait state and who wakes the frame.
- Optimizations to scheduler or channel synchronization must preserve FIFO and
  ownership behavior covered by the native fixtures.

Use C11 atomics and platform synchronization APIs before introducing custom
assembly synchronization. Handwritten atomic loops need a demonstrated reason
and an explicit memory-ordering argument.

## Future Extensions

The following directories are reserved for future platform-specific and
optimized implementations; they are not part of the current runtime tree:

```text
runtime/
|-- include/                 # Public and private headers when extracted
|-- src/platform/             # Darwin, Linux, POSIX, and future Windows glue
|-- arch/
|   |-- arm64/                # ARM64 intrinsics or assembly implementations
|   `-- x86_64/               # x86-64 intrinsics or assembly implementations
`-- bench/
```

Adding files to these directories must preserve the single-translation-unit
build until the compiler driver explicitly supports compiling and linking
multiple runtime objects. Every relocation or new source file also requires
updating the file inventory in `../crates/aura-cli/src/runtime_path.rs` and the
runtime packaging tests in `../crates/aura-codegen/src/build.rs`.

## Platform and Architecture Code

Platform code is selected by operating system; architecture code is selected by
CPU instruction set. Keep those concerns separate:

```text
src/platform/darwin/       # kqueue, getentropy, Darwin socket behavior
src/platform/linux/        # epoll, eventfd, getrandom
src/platform/posix/        # Shared POSIX fallback

arch/arm64/crypto/         # ARM SHA2/NEON implementation
arch/x86_64/crypto/        # SHA-NI/AVX implementation
```

Portable implementations remain under the owning subsystem, for example
`src/crypto/sha256.c`. Architecture implementations must use the same internal
function contract and retain a portable fallback.

## Assembly and Intrinsics

Assembly is an optimization layer, not the canonical implementation. Add it
only after a benchmark identifies a stable CPU-bound hot path and optimized C
or compiler intrinsics do not produce acceptable code.

Use these naming and placement conventions:

```text
arch/arm64/crypto/sha256_arm64.S
arch/arm64/encoding/base64_neon.c
arch/x86_64/crypto/sha256_shani.S
arch/x86_64/encoding/base64_avx2.c
```

- Use `.S` when assembly needs preprocessing and conditional macros.
- Use `.s` only for assembly that requires no preprocessing.
- Prefer intrinsics in an architecture-specific `.c` file before handwritten
  assembly.
- Never expose an architecture-specific symbol through `aura_ffi.h`.
- Select implementations in a subsystem dispatch module, such as
  `src/crypto/dispatch.c`.
- Preserve the portable implementation in tests and run the same vectors
  against every accelerated implementation.
- Feature detection must distinguish compile-time availability from runtime CPU
  support, particularly for portable x86-64 binaries.

Separate `.S` files cannot be included as C source by `runtime.c`. Adding one
therefore requires the compiler driver to compile the assembly into an object
and link it, or requires an intrinsics implementation that can remain part of
the C translation unit.

## Testing

Most native fixtures define `AURA_RUNTIME_NO_MAIN` and include `runtime.c`
directly. This intentionally tests the same amalgamated source used by generated
programs.

Useful validation commands from the repository root include:

```bash
# Compiler/runtime integration
cargo test -p aura-codegen

# Native FFI acceptance matrix
bash scripts/ffi-regression.sh

# Representative native sanitizer matrix
cargo build -p aura-cli
bash scripts/sanitizer-smoke.sh --native-only

# Compile the runtime translation unit with strict warnings
cc -D_POSIX_C_SOURCE=200809L -std=c11 -Wall -Wextra -Werror \
  -c runtime/runtime.c -o /tmp/aura-runtime.o

# SHA-256 portable vs dispatched backend (1 MiB input, 32 iterations)
cc -D_POSIX_C_SOURCE=200809L -std=c11 -Wall -Wextra -Werror \
  -o /tmp/aura-sha-bench runtime/bench/sha256.c -lz -pthread
/tmp/aura-sha-bench
```

For a focused native fixture, compile the file under `runtime/tests/` rather
than constructing a separate mock runtime. Add `-pthread`, `-lz`, or sanitizer
flags when required by the fixture and host.

An optimization change must include correctness vectors plus a benchmark that
compares the portable and accelerated implementations on the same inputs.
Throughput results should record the host architecture, compiler, flags, input
sizes, and whether CPU feature dispatch selected the accelerated path.

The benchmark fixture reports those fields directly. On the development arm64
host, clang with the default flags measured 0.587s portable versus 0.131s for
the dispatched ARM SHA2 backend at 32 MiB total input. The x86-64 binary also
builds cleanly; on the current Rosetta environment CPU dispatch selected the
portable fallback because SHA-NI was not reported by the guest CPU.

## Change Checklist

Before landing a runtime change:

1. Identify whether generated C, public FFI callers, or only internal code uses
   the affected symbols.
2. Preserve ownership, error, cancellation, and cleanup behavior on every exit
   path.
3. Add or update the smallest native fixture that crosses the changed boundary.
4. Run strict-warning compilation and the relevant sanitizer/concurrency tests.
5. Update `agents/debts.md` when the change introduces, discovers, resolves, or
   narrows deferred runtime behavior.
6. Update the embedded runtime source inventory when adding, removing, or moving
   runtime files.

The runtime should remain portable, inspectable C by default. Platform-specific
syscalls, intrinsics, and assembly are welcome when they are isolated behind a
portable contract and justified by measurements.
