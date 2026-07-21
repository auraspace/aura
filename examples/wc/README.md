# examples/wc

Dogfood CLI: a small **word-count** tool that exercises process/String surface plus C13 polish (`method-on-temp`, `Int.toString`).

| Surface              | Use in `wc`                                        |
| -------------------- | -------------------------------------------------- |
| `std.io.args()`      | paths and flags after `aura run … --`              |
| `std.io.tryReadFile` | soft-fail read (`null` → error + `exit(1)`)        |
| `String.split`       | lines (`\n`) and words (` `)                       |
| `String.trim`        | skip empty word segments; parse `-n`               |
| method-on-temp       | `segs.get(j).trim()`, `argv.get(i).trim().toInt()` |
| `Int.toString`       | print line/word/byte count columns                 |
| `String.indexOf`     | flag letters in `-lwc`; tab → space rewrite        |
| `String.toInt`       | `-n N` first-N-lines limit                         |
| `String.startsWith`  | detect short flags                                 |
| `std.io.exit`        | usage / I/O failures                               |

## Run

From the monorepo root (so `std/` and `runtime/` resolve):

```bash
# Create a sample file
mkdir -p target/aura
printf 'hello world\nsecond line\n' > target/aura/wc_sample.txt

# Default: lines words bytes path
cargo run -p aura-cli -- run examples/wc -- target/aura/wc_sample.txt
# → 2 4 24 target/aura/wc_sample.txt

# Flags (combinable)
cargo run -p aura-cli -- run examples/wc -- -l target/aura/wc_sample.txt
cargo run -p aura-cli -- run examples/wc -- -w target/aura/wc_sample.txt
cargo run -p aura-cli -- run examples/wc -- -c target/aura/wc_sample.txt
cargo run -p aura-cli -- run examples/wc -- -lwc target/aura/wc_sample.txt

# First N lines only (-n uses String.toInt)
cargo run -p aura-cli -- run examples/wc -- -n 1 target/aura/wc_sample.txt

# Full dogfood verification: check, test, then forwarded-argument run/build
cargo run -p aura-cli -- test examples/wc
cargo run -p aura-cli -- check examples/wc
cargo run -p aura-cli -- run examples/wc -- -lwc target/aura/wc_sample.txt
cargo run -p aura-cli -- build examples/wc -o target/aura/wc
target/aura/wc -n 1 target/aura/wc_sample.txt
```

Installed `aura` (same args after `--`):

```bash
aura run examples/wc -- target/aura/wc_sample.txt
```

The `--` is required: it separates Aura CLI options from the arguments passed
to `examples/wc`. The release rehearsal also checks `-lwc` and `-n 1` through
this forwarded-argument path, plus the built executable.

## Layout

| Path            | Role                                 |
| --------------- | ------------------------------------ |
| `src/main.aura` | CLI parse + counts + `@test` helpers |

## Limits (honest)

- One file path only (no multi-file total row).
- Word split is space/tab based (not full Unicode whitespace).
- Byte count is UTF-8 **byte** length (`String.len`), not grapheme clusters.
- Missing/unreadable path → stderr message + exit 1 via `tryReadFile` + `exit`.
