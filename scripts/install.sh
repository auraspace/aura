#!/usr/bin/env bash
# Aura toolchain installer — curl | bash
#   curl -fsSL https://aura.fadosoft.com/install.sh | bash
#
# Versioned layout (rustup-style) under $AURA_HOME (default ~/.aura):
#
#   $AURA_HOME/
#     versions/<version>/
#       bin/aura
#       share/aura/aura_rt.c   # if present in the release tarball
#       meta/version, os, arch, installed_at
#     current -> versions/<version>     # active toolchain
#     bin/aura -> ../current/bin/aura   # PATH entrypoint
#     bin/avm                           # Aura Version Manager (switch active version)
#
# Env:
#   AURA_VERSION       Release without leading v (default: latest GitHub release)
#   AURA_HOME          Root for versions (default: $HOME/.aura)
#   AURA_REPO          GitHub owner/name (default: auraspace/aura)
#   AURA_SET_DEFAULT   1 (default) update `current`; 0 only install side-by-side
#   AURA_LINK_USER_BIN 1 (default) symlink $AURA_HOME/bin/aura → ~/.local/bin/aura
#   AURA_NO_PATH_HINT  1 skip PATH hint
set -euo pipefail

REPO="${AURA_REPO:-auraspace/aura}"
AURA_HOME="${AURA_HOME:-${HOME}/.aura}"
SET_DEFAULT="${AURA_SET_DEFAULT:-1}"
LINK_USER_BIN="${AURA_LINK_USER_BIN:-1}"
API="https://api.github.com/repos/${REPO}"
RELEASES="https://github.com/${REPO}/releases"
INSTALL_TMP=""
INSTALL_CANDIDATE=""

info() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

cleanup_install() {
  [[ -z "$INSTALL_CANDIDATE" ]] || rm -rf "$INSTALL_CANDIDATE"
  [[ -z "$INSTALL_TMP" ]] || rm -rf "$INSTALL_TMP"
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "need \`$1\` on PATH"
}

detect_os_arch() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$os" in
    linux*) os=linux ;;
    darwin*) os=darwin ;;
    *) die "unsupported OS: $(uname -s) (use cargo install — see https://aura.fadosoft.com/docs/install)" ;;
  esac
  case "$arch" in
    x86_64|amd64) arch=amd64 ;;
    aarch64|arm64) arch=arm64 ;;
    *) die "unsupported arch: $(uname -m)" ;;
  esac
  printf '%s %s\n' "$os" "$arch"
}

resolve_version() {
  if [[ -n "${AURA_VERSION:-}" ]]; then
    validate_version "${AURA_VERSION#v}"
    printf '%s\n' "${AURA_VERSION#v}"
    return
  fi
  need_cmd curl
  local tag
  tag="$(curl -fsSL "${API}/releases/latest" 2>/dev/null \
    | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
    | head -1)" || true
  if [[ -z "$tag" ]]; then
    die "could not resolve latest release (set AURA_VERSION=0.1.0-alpha or install from source)"
  fi
  validate_version "${tag#v}"
  printf '%s\n' "${tag#v}"
}

validate_version() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] \
    || die "invalid version: $1 (expected semver such as 0.1.0-alpha)"
}

version_dir() {
  printf '%s\n' "${AURA_HOME}/versions/$1"
}

# avm payload for curl|bash installs. Empty in the repo source of truth;
# site/scripts/sync-install.mjs embeds base64 of scripts/avm into public/install.sh.
# Do not put a large heredoc inside write_avm — bash can hang on that.
# @AVM_EMBED_BEGIN@
AVM_SCRIPT_B64=''
# @AVM_EMBED_END@

write_avm() {
  # avm = Aura Version Manager (switch active toolchain under $AURA_HOME)
  local avm="${AURA_HOME}/bin/avm"
  mkdir -p "${AURA_HOME}/bin"

  local tmp_avm="${avm}.tmp.$$"
  rm -f "$tmp_avm"
  if [[ -n "${AVM_SCRIPT_B64}" ]]; then
    # CDN / built install.sh: decode embedded payload (no large heredoc-in-function).
    if printf '%s' "$AVM_SCRIPT_B64" | base64 -d >"$tmp_avm" 2>/dev/null \
      || printf '%s' "$AVM_SCRIPT_B64" | base64 -D >"$tmp_avm" 2>/dev/null \
      || printf '%s' "$AVM_SCRIPT_B64" | base64 --decode >"$tmp_avm" 2>/dev/null; then
      :
    else
      rm -f "$tmp_avm"
      die "failed to decode embedded avm (need base64)"
    fi
  else
    # Repo checkout: copy scripts/avm next to this installer (or AURA_INSTALL_AVM).
    local src="" here
    if [[ -n "${AURA_INSTALL_AVM:-}" && -f "${AURA_INSTALL_AVM}" ]]; then
      src="${AURA_INSTALL_AVM}"
    elif [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
      here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
      if [[ -f "${here}/avm" ]]; then
        src="${here}/avm"
      fi
    fi
    [[ -n "$src" ]] || die "avm payload missing (run site sync-install, or place scripts/avm next to install.sh)"
    if command -v install >/dev/null 2>&1; then
      install -m 755 "$src" "$tmp_avm"
    else
      cp "$src" "$tmp_avm"
      chmod 755 "$tmp_avm"
    fi
  fi

  chmod 755 "$tmp_avm"
  [[ -s "$tmp_avm" ]] || { rm -f "$tmp_avm"; die "avm payload is empty"; }
  mv -f "$tmp_avm" "$avm"
  # Drop legacy helper name if present from older installers.
  rm -f "${AURA_HOME}/bin/aura-switch"
}

link_current() {
  local version="$1"
  local current_tmp="${AURA_HOME}/.current.tmp.$$"
  local aura_tmp="${AURA_HOME}/bin/.aura.tmp.$$"
  [[ -d "${AURA_HOME}/versions/${version}" ]] || die "cannot activate missing version: ${version}"
  if [[ -e "${AURA_HOME}/current" && ! -L "${AURA_HOME}/current" ]]; then
    die "refusing to replace malformed non-symlink: ${AURA_HOME}/current"
  fi
  if [[ -e "${AURA_HOME}/bin/aura" && ! -L "${AURA_HOME}/bin/aura" ]]; then
    die "refusing to replace non-symlink: ${AURA_HOME}/bin/aura"
  fi
  mkdir -p "${AURA_HOME}/bin"
  rm -f "$current_tmp" "$aura_tmp"
  ln -s "versions/${version}" "$current_tmp"
  ln -s "../current/bin/aura" "$aura_tmp"
  mv -f "$current_tmp" "${AURA_HOME}/current"
  mv -f "$aura_tmp" "${AURA_HOME}/bin/aura"
  write_avm
  info "active version → ${version} (${AURA_HOME}/current)"
}

link_user_bin() {
  [[ "$LINK_USER_BIN" == "1" ]] || return 0
  local user_bin="${HOME}/.local/bin"
  mkdir -p "$user_bin"
  ln -sfn "${AURA_HOME}/bin/aura" "${user_bin}/aura"
  if [[ -x "${AURA_HOME}/bin/avm" ]]; then
    ln -sfn "${AURA_HOME}/bin/avm" "${user_bin}/avm"
  fi
  # Clean up pre-rename helper symlink.
  rm -f "${user_bin}/aura-switch"
  info "linked ${user_bin}/aura → ${AURA_HOME}/bin/aura"
  info "linked ${user_bin}/avm → ${AURA_HOME}/bin/avm"
}

download_and_install() {
  local version="$1" os="$2" arch="$3"
  validate_version "$version"
  [[ "$os/$arch" == "linux/amd64" || "$os/$arch" == "darwin/amd64" || "$os/$arch" == "darwin/arm64" ]] \
    || die "unsupported release target: ${os}/${arch} (supported: linux/amd64, darwin/amd64, darwin/arm64)"
  local name="aura-${version}-${os}-${arch}"
  local url="${RELEASES}/download/v${version}/${name}.tar.gz"
  local checksum_url="${url}.sha256"
  local vdir tmp stage candidate backup expected actual bin
  vdir="$(version_dir "$version")"
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-install.XXXXXX")"
  INSTALL_TMP="$tmp"
  trap cleanup_install EXIT

  info "downloading ${name}.tar.gz"
  if ! curl -fsSL --retry 2 --connect-timeout 15 --max-time 600 "$url" -o "${tmp}/aura.tar.gz"; then
    die "download failed: ${url}
  No asset for this platform yet? Install from source:
    git clone --branch v${version} https://github.com/${REPO}.git
    cd aura && cargo install --path crates/aura-cli"
  fi
  if ! curl -fsSL --retry 2 --connect-timeout 15 --max-time 60 "$checksum_url" -o "${tmp}/aura.tar.gz.sha256"; then
    die "checksum download failed: ${checksum_url}"
  fi
  expected="$(awk 'NF { print $1; exit }' "${tmp}/aura.tar.gz.sha256")"
  [[ "$expected" =~ ^[[:xdigit:]]{64}$ ]] || die "malformed checksum file: ${checksum_url}"
  if command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "${tmp}/aura.tar.gz" | awk '{print $1}')"
  elif command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "${tmp}/aura.tar.gz" | awk '{print $1}')"
  else
    die "need shasum or sha256sum to verify release checksum"
  fi
  [[ "${actual,,}" == "${expected,,}" ]] || die "checksum mismatch for ${name}.tar.gz"

  info "extracting into versions/${version}"
  tar -xzf "${tmp}/aura.tar.gz" -C "$tmp"
  stage="$(find "$tmp" -mindepth 1 -maxdepth 1 -type d ! -name '.*' | head -1)"
  [[ -n "$stage" ]] || die "empty archive"
  local bin="${stage}/bin/aura"
  [[ -f "$bin" ]] || bin="$(find "$tmp" -type f -name aura | head -1)"
  [[ -n "$bin" && -f "$bin" ]] || die "archive missing aura binary"

  candidate="${AURA_HOME}/versions/.${version}.install.$$"
  INSTALL_CANDIDATE="$candidate"
  rm -rf "$candidate"
  mkdir -p "${candidate}/bin" "${candidate}/share/aura" "${candidate}/meta"
  if command -v install >/dev/null 2>&1; then
    install -m 755 "$bin" "${candidate}/bin/aura"
  else
    cp "$bin" "${candidate}/bin/aura"
    chmod 755 "${candidate}/bin/aura"
  fi

  # Prefer runtime from tarball share/; else skip (CLI embeds runtime).
  if [[ -f "${stage}/share/aura/aura_rt.c" ]]; then
    cp "${stage}/share/aura/aura_rt.c" "${candidate}/share/aura/aura_rt.c"
  elif [[ -f "${stage}/share/aura_rt.c" ]]; then
    cp "${stage}/share/aura_rt.c" "${candidate}/share/aura/aura_rt.c"
  fi

  # Std packages (io/assert/collections) when present in the archive.
  if [[ -d "${stage}/share/aura/std" ]]; then
    mkdir -p "${candidate}/share/aura/std"
    if command -v rsync >/dev/null 2>&1; then
      rsync -a "${stage}/share/aura/std/" "${candidate}/share/aura/std/"
    else
      cp -R "${stage}/share/aura/std/." "${candidate}/share/aura/std/"
    fi
  fi

  printf '%s\n' "$version" >"${candidate}/meta/version"
  printf '%s\n' "$os" >"${candidate}/meta/os"
  printf '%s\n' "$arch" >"${candidate}/meta/arch"
  date -u +"%Y-%m-%dT%H:%M:%SZ" >"${candidate}/meta/installed_at"
  printf '%s\n' "$REPO" >"${candidate}/meta/repo"
  [[ -x "${candidate}/bin/aura" ]] || die "malformed archive: aura binary is not executable"
  [[ "$(cat "${candidate}/meta/version")" == "$version" ]] || die "malformed install metadata"

  mkdir -p "${AURA_HOME}/versions"
  backup="${vdir}.backup.$$"
  rm -rf "$backup"
  rm -rf "$tmp"
  INSTALL_TMP=""
  if [[ -e "$vdir" ]]; then
    mv "$vdir" "$backup"
  fi
  if ! mv "$candidate" "$vdir"; then
    [[ -e "$backup" ]] && mv "$backup" "$vdir"
    die "could not publish ${vdir}; previous install was preserved"
  fi
  rm -rf "$backup"
  INSTALL_CANDIDATE=""
  trap - EXIT

  info "installed ${vdir}"

  if [[ "$SET_DEFAULT" == "1" ]]; then
    link_current "$version"
  else
    write_avm
    info "left current unchanged (AURA_SET_DEFAULT=0); run: avm ${version}"
  fi

  link_user_bin

  if [[ -x "${AURA_HOME}/bin/aura" ]]; then
    "${AURA_HOME}/bin/aura" version || true
  elif [[ -x "${vdir}/bin/aura" ]]; then
    "${vdir}/bin/aura" version || true
  fi
}

path_hint() {
  [[ "${AURA_NO_PATH_HINT:-}" == "1" ]] && return
  local need_home=0 need_local=0
  case ":${PATH}:" in
    *":${AURA_HOME}/bin:"*) ;;
    *) need_home=1 ;;
  esac
  case ":${PATH}:" in
    *":${HOME}/.local/bin:"*) ;;
    *) need_local=1 ;;
  esac
  if [[ "$need_home" -eq 0 && "$need_local" -eq 0 ]]; then
    return
  fi
  cat <<EOF

Add Aura to your PATH (shell profile):

  export AURA_HOME="${AURA_HOME}"
  export PATH="\${AURA_HOME}/bin:\$HOME/.local/bin:\$PATH"

Then:

  aura version
  avm --list                  # installed versions
  avm 0.1.0-alpha             # switch active (after multi-version install)
  aura new hello && aura run hello

Docs: https://aura.fadosoft.com/docs/install
EOF
}

main() {
  need_cmd uname
  need_cmd tar
  need_cmd curl
  need_cmd mktemp
  need_cmd find
  need_cmd ln
  need_cmd basename
  need_cmd sort
  need_cmd date

  read -r os arch <<<"$(detect_os_arch)"
  local version
  version="$(resolve_version)"
  info "Aura installer (v${version}, ${os}/${arch})"
  info "AURA_HOME=${AURA_HOME}"
  download_and_install "$version" "$os" "$arch"
  path_hint
  info "done"
}

main "$@"
