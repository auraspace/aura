#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
status_file="$root/std/api-status.tsv"

[[ -f "$status_file" ]] || { echo "stdlib: missing $status_file" >&2; exit 1; }

packages=()
while IFS=$'\t' read -r package status contract notes; do
  [[ -z "${package:-}" || "$package" == \#* ]] && continue
  case "$status" in implemented|partial|in_progress) ;;
    *) echo "stdlib: invalid status for $package: $status" >&2; exit 1 ;;
  esac
  leaf="${package#std.}"
  manifest="$root/std/$leaf/aura.toml"
  source="$root/std/$leaf/src/lib.aura"
  [[ -f "$manifest" && -f "$source" ]] || {
    echo "stdlib: missing source for $package" >&2; exit 1
  }
  rg -q "name = \"$package\"" "$manifest" || {
    echo "stdlib: manifest name mismatch for $package" >&2; exit 1
  }
  rg -q "^package $package$" "$source" || {
    echo "stdlib: package declaration mismatch for $package" >&2; exit 1
  }
  packages+=("$leaf")
done < "$status_file"

for dir in "$root"/std/*; do
  [[ -d "$dir" ]] || continue
  leaf="${dir##*/}"
  found=0
  for expected in "${packages[@]}"; do
    [[ "$leaf" == "$expected" ]] && found=1
  done
  [[ "$found" == 1 ]] || {
    echo "stdlib: std/$leaf is not listed in std/api-status.tsv" >&2; exit 1
  }
done

for builtin in print println eprint eprintln assert assert_eq gc_collect exception_cause_count; do
  rg -q "\"$builtin\"|name == \"$builtin\"" \
    "$root/crates/aura-sema/src/checker/mod.rs" \
    "$root/crates/aura-sema/src/checker/call.rs" || {
    echo "stdlib: builtin $builtin is not registered" >&2; exit 1
  }
done

for async_surface in Spawn Join Cancel ChannelCreate ChannelSend ChannelReceive ChannelClose; do
  rg -q "AsyncExpr::$async_surface" "$root/crates/aura-sema/src/checker/expr.rs" || {
    echo "stdlib: async builtin $async_surface is not checked" >&2; exit 1
  }
done

for async_type in Task TaskHandle Channel; do
  rg -q "\"$async_type\"" "$root/crates/aura-sema/src/checker/types.rs" || {
    echo "stdlib: async builtin type $async_type is not registered" >&2; exit 1
  }
done

echo "stdlib: PASS (${#packages[@]} packages; implementation manifest and builtin sentinels valid)"
