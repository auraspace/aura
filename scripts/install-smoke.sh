#!/usr/bin/env bash
# install-smoke.sh — post-install / post-release checklist for aura + avm
#
# Safe defaults: no network, no writes outside a temp dir unless you pass
# flags that intentionally exercise the real installer.
#
# Modes:
#   (default)           Verify an existing install on PATH / $AURA_HOME
#   --local-pkg         Build a release tarball from this checkout, install
#                       into a temp AURA_HOME, run smokes (needs cargo + cc)
#   --from-release      curl install.sh into a temp AURA_HOME (needs network)
#   --checklist         Print the human checklist only (no checks)
#
# Env:
#   AURA_HOME           Install root to inspect (default ~/.aura)
#   AURA_BIN            Override aura binary for default mode
#   AURA_VERSION        Version pin for --from-release / --local-pkg tag
#   AURA_INSTALL_URL    install.sh URL (default https://aura.fadosoft.com/install.sh)
#   SMOKE_KEEP=1        Keep temp AURA_HOME and print its path
#
# Note: avoid large heredocs inside functions — some bash builds hang (see install.sh).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
AURA_HOME="${AURA_HOME:-${HOME}/.aura}"
AURA_INSTALL_URL="${AURA_INSTALL_URL:-https://aura.fadosoft.com/install.sh}"
AURA_VERSION="${AURA_VERSION:-0.1.0-alpha}"
mode="default"
fail=0

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '→ %s\n' "$*"; }
ok() { printf '  ok  %s\n' "$*"; }
bad() { printf '  FAIL %s\n' "$*" >&2; fail=1; }

usage() {
  printf '%s\n' \
    'install-smoke.sh — verify aura install / avm after release (or locally)' \
    '' \
    'Usage:' \
    '  scripts/install-smoke.sh                 # check current install' \
    '  scripts/install-smoke.sh --checklist     # print human steps only' \
    '  scripts/install-smoke.sh --local-pkg     # package + install to temp home' \
    '  scripts/install-smoke.sh --from-release  # curl install.sh to temp home' \
    '  scripts/install-smoke.sh --help' \
    '' \
    'Human checklist (after a GitHub Release is live):' \
    '' \
    '  1. Install (pin recommended):' \
    '       curl -fsSL https://aura.fadosoft.com/install.sh \' \
    '         | AURA_VERSION=0.1.0-alpha bash' \
    '' \
    '  2. PATH — prefer versioned layout:' \
    '       export PATH="$HOME/.aura/bin:$HOME/.local/bin:$PATH"' \
    '       which aura && which avm' \
    '' \
    '  3. Version manager:' \
    '       avm --help' \
    '       avm --list' \
    '       avm --show' \
    '       aura version' \
    '' \
    '  4. Compile smoke (needs system C compiler):' \
    '       rm -rf /tmp/aura-smoke' \
    '       aura new /tmp/aura-smoke' \
    '       aura run /tmp/aura-smoke          # expect: Hello, Aura' \
    '       aura test /tmp/aura-smoke         # if scaffold has @test' \
    '' \
    '  5. Side-by-side install (optional):' \
    '       curl -fsSL https://aura.fadosoft.com/install.sh \' \
    '         | AURA_VERSION=0.1.0-alpha AURA_SET_DEFAULT=0 bash' \
    '       avm 0.1.0-alpha && avm --show' \
    '' \
    '  6. Wrong binary on PATH?' \
    '       which -a aura' \
    '       # $AURA_HOME/bin should win over ~/.cargo/bin' \
    '' \
    'Maintainer local path (no CDN):' \
    '' \
    '  TAG_VERSION=0.1.0-alpha bash scripts/package-release.sh' \
    '  bash scripts/install-smoke.sh --local-pkg' \
    '' \
    'See also: docs/guide/install.md'
}

resolve_aura() {
  if [[ -n "${AURA_BIN:-}" ]]; then
    printf '%s\n' "$AURA_BIN"
    return
  fi
  if [[ -x "${AURA_HOME}/bin/aura" ]]; then
    printf '%s\n' "${AURA_HOME}/bin/aura"
    return
  fi
  if command -v aura >/dev/null 2>&1; then
    command -v aura
    return
  fi
  return 1
}

resolve_avm() {
  if [[ -x "${AURA_HOME}/bin/avm" ]]; then
    printf '%s\n' "${AURA_HOME}/bin/avm"
    return
  fi
  if command -v avm >/dev/null 2>&1; then
    command -v avm
    return
  fi
  # Repo checkout helper
  if [[ -x "${root}/scripts/avm" ]]; then
    printf '%s\n' "${root}/scripts/avm"
    return
  fi
  return 1
}

smoke_cli() {
  local aura="$1"
  local work
  work="$(mktemp -d "${TMPDIR:-/tmp}/aura-install-smoke.XXXXXX")"
  info "CLI smoke with: $aura (workdir $work)"
  if ! "$aura" version >/dev/null; then
    bad "aura version failed"
  else
    ok "aura version"
  fi
  if ! "$aura" new "$work/hello" >/dev/null; then
    bad "aura new failed"
    rm -rf "$work"
    return
  fi
  local out
  if ! out="$("$aura" run "$work/hello" 2>&1)"; then
    bad "aura run failed: $out"
  else
    if printf '%s' "$out" | grep -q 'Hello'; then
      ok "aura run printed greeting"
    else
      bad "aura run unexpected output: $out"
    fi
  fi
  rm -rf "$work"
}

smoke_avm() {
  local avm_bin="$1"
  local home="$2"
  info "avm smoke (AURA_HOME=$home)"
  if AURA_HOME="$home" "$avm_bin" --help >/dev/null; then
    ok "avm --help"
  else
    bad "avm --help failed"
  fi
  if AURA_HOME="$home" "$avm_bin" --list >/dev/null; then
    ok "avm --list"
  else
    bad "avm --list failed"
  fi
  local shown
  if shown="$(AURA_HOME="$home" "$avm_bin" --show)"; then
    ok "avm --show → $shown"
  else
    bad "avm --show failed"
    return
  fi
  if [[ "$shown" != "(none)" && -n "$shown" ]]; then
    if AURA_HOME="$home" "$avm_bin" "$shown" >/dev/null; then
      ok "avm activate $shown"
    else
      bad "avm activate $shown failed"
    fi
  fi
}

check_layout() {
  local home="$1"
  info "Layout under $home"
  if [[ -d "$home/versions" ]]; then
    ok "versions/ present"
    local n
    n="$(find "$home/versions" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | wc -l | tr -d ' ')"
    ok "installed version dirs: $n"
  else
    bad "missing $home/versions"
  fi
  if [[ -e "$home/current" ]]; then
    ok "current → $(readlink "$home/current" 2>/dev/null || echo present)"
  else
    bad "missing $home/current"
  fi
  if [[ -x "$home/bin/aura" ]]; then
    ok "bin/aura executable"
  else
    bad "missing executable $home/bin/aura"
  fi
  if [[ -x "$home/bin/avm" ]]; then
    ok "bin/avm executable"
  else
    bad "missing executable $home/bin/avm"
  fi
}

mode_default() {
  info "Default mode: existing install (AURA_HOME=$AURA_HOME)"
  local aura avm_bin
  if aura="$(resolve_aura)"; then
    ok "aura → $aura"
    smoke_cli "$aura"
  else
    bad "aura not found (set AURA_BIN or install under $AURA_HOME)"
  fi
  if avm_bin="$(resolve_avm)"; then
    ok "avm → $avm_bin"
    if [[ -d "$AURA_HOME/versions" ]]; then
      check_layout "$AURA_HOME"
      smoke_avm "$avm_bin" "$AURA_HOME"
    else
      info "no versioned layout at $AURA_HOME (cargo install only?) — avm --help only"
      if "$avm_bin" --help >/dev/null; then
        ok "avm --help"
      else
        bad "avm --help failed"
      fi
    fi
  else
    bad "avm not found"
  fi
}

mode_local_pkg() {
  command -v cargo >/dev/null || die "cargo required for --local-pkg"
  command -v cc >/dev/null || command -v clang >/dev/null || die "C compiler required for --local-pkg"
  local ver="${AURA_VERSION}"
  info "Local package smoke (TAG_VERSION=$ver)"
  (
    cd "$root"
    TAG_VERSION="$ver" bash scripts/package-release.sh
  )
  local tarball
  tarball="$(ls -1 "$root"/dist/aura-"${ver}"-*.tar.gz 2>/dev/null | head -1 || true)"
  [[ -n "$tarball" && -f "$tarball" ]] || die "no tarball under dist/ after package-release.sh"

  local tmp
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-local-smoke.XXXXXX")"
  # shellcheck disable=SC2064
  [[ "${SMOKE_KEEP:-0}" == "1" ]] || trap "rm -rf '$tmp'" EXIT

  export AURA_HOME="$tmp/home"
  mkdir -p "$AURA_HOME"
  info "Installing tarball into AURA_HOME=$AURA_HOME"
  local unpack="$tmp/unpack"
  mkdir -p "$unpack"
  tar -xzf "$tarball" -C "$unpack"
  local tree
  tree="$(find "$unpack" -mindepth 1 -maxdepth 1 -type d | head -1)"
  [[ -n "$tree" ]] || die "empty tarball layout"
  local dest="$AURA_HOME/versions/${ver}"
  mkdir -p "$AURA_HOME/versions"
  rm -rf "$dest"
  mv "$tree" "$dest"
  local artifact_name os arch
  artifact_name="$(basename "$tarball" .tar.gz)"
  if [[ "$artifact_name" =~ ^aura-(.+)-(linux|darwin)-(amd64|arm64)$ ]]; then
    os="${BASH_REMATCH[1]}"
    arch="${BASH_REMATCH[2]}"
  else
    die "unexpected local artifact name: $artifact_name"
  fi
  mkdir -p "$dest/meta"
  printf '%s\n' "$ver" >"$dest/meta/version"
  printf '%s\n' "$os" >"$dest/meta/os"
  printf '%s\n' "$arch" >"$dest/meta/arch"
  mkdir -p "$AURA_HOME/bin"
  # Prefer packaged avm; fall back to repo scripts/avm
  if [[ -x "$dest/bin/avm" ]]; then
    cp "$dest/bin/avm" "$AURA_HOME/bin/avm"
  else
    cp "$root/scripts/avm" "$AURA_HOME/bin/avm"
  fi
  chmod 755 "$AURA_HOME/bin/avm"
  AURA_HOME="$AURA_HOME" "$AURA_HOME/bin/avm" "$ver"

  check_layout "$AURA_HOME"
  smoke_avm "$AURA_HOME/bin/avm" "$AURA_HOME"
  smoke_cli "$AURA_HOME/bin/aura"

  if [[ "${SMOKE_KEEP:-0}" == "1" ]]; then
    info "kept AURA_HOME=$AURA_HOME"
  fi
}

mode_from_release() {
  command -v curl >/dev/null || die "curl required for --from-release"
  command -v tar >/dev/null || die "tar required for --from-release"
  local tmp
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-release-smoke.XXXXXX")"
  # shellcheck disable=SC2064
  [[ "${SMOKE_KEEP:-0}" == "1" ]] || trap "rm -rf '$tmp'" EXIT

  export AURA_HOME="$tmp/home"
  mkdir -p "$AURA_HOME"
  info "curl install → AURA_HOME=$AURA_HOME version=$AURA_VERSION"
  # Isolate user bin links
  curl -fsSL "$AURA_INSTALL_URL" | AURA_VERSION="$AURA_VERSION" AURA_HOME="$AURA_HOME" AURA_LINK_USER_BIN=0 bash

  check_layout "$AURA_HOME"
  smoke_avm "$AURA_HOME/bin/avm" "$AURA_HOME"
  smoke_cli "$AURA_HOME/bin/aura"

  if [[ "${SMOKE_KEEP:-0}" == "1" ]]; then
    info "kept AURA_HOME=$AURA_HOME"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --checklist) mode="checklist" ;;
    --local-pkg) mode="local-pkg" ;;
    --from-release) mode="from-release" ;;
    *) die "unknown arg: $1 (try --help)" ;;
  esac
  shift
done

case "$mode" in
  checklist) usage; exit 0 ;;
  default) mode_default ;;
  local-pkg) mode_local_pkg ;;
  from-release) mode_from_release ;;
esac

if [[ "$fail" -ne 0 ]]; then
  printf '\ninstall-smoke: FAILED\n' >&2
  exit 1
fi
printf '\ninstall-smoke: all checks passed\n'
