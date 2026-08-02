# Aura Runtime

The runtime is organized as one C translation unit with subsystem sources
included in dependency order by `runtime.c`. This keeps static-link behavior
and the compiler ABI deterministic while allowing each subsystem to be
developed and reviewed independently.

## Layout

- `runtime.c`: translation-unit entrypoint and module order.
- `aura_ffi.h`: public C ABI declarations.
- `src/`: runtime implementations grouped by ownership and execution domain.
- `tests/`: native ABI, lifecycle, sanitizer, and protocol fixtures.

The module order in `runtime.c` is part of the build contract. Modules share a
translation unit so private types and static helpers retain their existing C
linkage; consumers link only `runtime.c` and include public headers as needed.

The CLI embeds the complete runtime file set, including `src/` and
`aura_ffi.h`, so installed builds do not depend on the source checkout.
