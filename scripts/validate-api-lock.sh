#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
lock="$root/std/api-lock.tsv"

[[ -f "$lock" ]] || { echo "api lock: missing $lock" >&2; exit 1; }

packages=()
while IFS=$'\t' read -r package status contract notes; do
  [[ -z "${package:-}" || "$package" == \#* ]] && continue
  [[ "$status" == "locked" ]] || {
    echo "api lock: invalid status for $package: $status" >&2
    exit 1
  }
  leaf="${package#std.}"
  manifest="$root/std/$leaf/aura.toml"
  source="$root/std/$leaf/src/lib.aura"
  [[ -f "$manifest" && -f "$source" ]] || {
    echo "api lock: missing source for $package" >&2
    exit 1
  }
  rg -q "name = \"$package\"" "$manifest" || {
    echo "api lock: manifest name mismatch for $package" >&2
    exit 1
  }
  rg -q "^package $package$" "$source" || {
    echo "api lock: package declaration mismatch for $package" >&2
    exit 1
  }
  packages+=("$leaf")
done < "$lock"

digest_file="$root/std/api-symbol-digests.tsv"
[[ -f "$digest_file" ]] || { echo "api lock: missing $digest_file" >&2; exit 1; }
while IFS=$'\t' read -r package expected; do
  [[ -z "${package:-}" || "$package" == \#* ]] && continue
  leaf="${package#std.}"
  source="$root/std/$leaf/src/lib.aura"
  actual="$(rg '^\s*pub (async )?(class|struct|enum|interface|type|fun)' "$source" | sed -E 's/[[:space:]]+/ /g' | shasum -a 256 | cut -d' ' -f1)"
  [[ "$actual" == "$expected" ]] || {
    echo "api lock: public declaration drift in $package" >&2
    exit 1
  }
done < "$digest_file"

builtin_signatures="$root/docs/api/builtin-signatures.tsv"
[[ -f "$builtin_signatures" ]] || {
  echo "api lock: missing $builtin_signatures" >&2
  exit 1
}
for builtin_member in \
  'Array<T> len field -> Int' \
  'Array<T> get (Int) -> T' \
  'Array<T> clone () -> Array<T>' \
  'String toInt () -> Int?' \
  'String substring (Int, Int) -> String' \
  'Int toString () -> String' \
  'spawn body -> TaskHandle<T>' \
  'join TaskHandle<T> -> Result<T, TaskError>' \
  'Channel<T> constructor (Int) -> Channel<T>'; do
  rg -F -q "$builtin_member" "$builtin_signatures" || {
    echo "api lock: builtin signature missing: $builtin_member" >&2
    exit 1
  }
done

for dir in "$root"/std/*; do
  [[ -d "$dir" ]] || continue
  leaf="${dir##*/}"
  found=0
  for expected in "${packages[@]}"; do
    [[ "$leaf" == "$expected" ]] && found=1
  done
  [[ "$found" == 1 ]] || {
    echo "api lock: std/$leaf is not listed in std/api-lock.tsv" >&2
    exit 1
  }
done

for builtin in print println eprint eprintln assert assert_eq gc_collect exception_cause_count; do
  rg -q "\"$builtin\"|name == \"$builtin\"" \
    "$root/crates/aura-sema/src/checker/mod.rs" \
    "$root/crates/aura-sema/src/checker/call.rs" || {
    echo "api lock: builtin $builtin is not registered" >&2
    exit 1
  }
done

for async_surface in Spawn Join Cancel ChannelCreate ChannelSend ChannelReceive ChannelClose; do
  rg -q "AsyncExpr::$async_surface" "$root/crates/aura-sema/src/checker/expr.rs" || {
    echo "api lock: async builtin $async_surface is not checked" >&2
    exit 1
  }
done

for async_type in Task TaskHandle Channel; do
  rg -q "\"$async_type\"" "$root/crates/aura-sema/src/checker/types.rs" || {
    echo "api lock: async builtin type $async_type is not registered" >&2
    exit 1
  }
done

echo "api lock: PASS (${#packages[@]} std packages; public declarations and builtin sentinel set locked)"
