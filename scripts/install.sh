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

info() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

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
  printf '%s\n' "${tag#v}"
}

version_dir() {
  printf '%s\n' "${AURA_HOME}/versions/$1"
}

write_avm() {
  # avm = Aura Version Manager (switch active toolchain under $AURA_HOME)
  local avm="${AURA_HOME}/bin/avm"
  mkdir -p "${AURA_HOME}/bin"
  cat >"$avm" <<'AVM'
#!/usr/bin/env bash
# avm — Aura Version Manager
# Usage: avm <version> | avm --list | avm --show
set -euo pipefail
AURA_HOME="${AURA_HOME:-${HOME}/.aura}"
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

list_versions() {
  local d
  [[ -d "${AURA_HOME}/versions" ]] || return 0
  for d in "${AURA_HOME}/versions"/*/; do
    [[ -d "$d" ]] || continue
    basename "$d"
  done | sort
}

show_current() {
  if [[ -L "${AURA_HOME}/current" ]]; then
    basename "$(readlink "${AURA_HOME}/current" 2>/dev/null || readlink -f "${AURA_HOME}/current")"
  elif [[ -d "${AURA_HOME}/current" ]]; then
    # non-symlink edge case
    printf 'current\n'
  else
    printf '(none)\n'
  fi
}

cmd="${1:-}"
case "$cmd" in
  ""|-h|--help)
    cat <<EOF
avm — Aura Version Manager

Usage:
  avm <version>   Activate an installed version
  avm --list      List installed versions
  avm --show      Print active version
EOF
    exit 0
    ;;
  --list|-l)
    list_versions
    exit 0
    ;;
  --show|-s)
    show_current
    exit 0
    ;;
  -*)
    die "unknown option: $cmd"
    ;;
esac

ver="${cmd#v}"
target="${AURA_HOME}/versions/${ver}"
[[ -d "$target" ]] || die "version not installed: ${ver}
Install with:
  curl -fsSL https://aura.fadosoft.com/install.sh | AURA_VERSION=${ver} bash
Available:
$(list_versions | sed 's/^/  /')"

mkdir -p "${AURA_HOME}/bin"
# Relative symlinks so AURA_HOME can be relocated as a tree.
ln -sfn "versions/${ver}" "${AURA_HOME}/current"
ln -sfn "../current/bin/aura" "${AURA_HOME}/bin/aura"
printf 'active: %s\n' "$ver"
if [[ -x "${AURA_HOME}/bin/aura" ]]; then
  "${AURA_HOME}/bin/aura" version || true
fi
AVM
  chmod 755 "$avm"
  # Drop legacy helper name if present from older installers.
  rm -f "${AURA_HOME}/bin/aura-switch"
}

link_current() {
  local version="$1"
  mkdir -p "${AURA_HOME}/bin"
  ln -sfn "versions/${version}" "${AURA_HOME}/current"
  ln -sfn "../current/bin/aura" "${AURA_HOME}/bin/aura"
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
  local name="aura-${version}-${os}-${arch}"
  local url="${RELEASES}/download/v${version}/${name}.tar.gz"
  local vdir tmp stage
  vdir="$(version_dir "$version")"
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/aura-install.XXXXXX")"
  # shellcheck disable=SC2064
  trap "rm -rf '$tmp'" EXIT

  info "downloading ${name}.tar.gz"
  if ! curl -fsSL "$url" -o "${tmp}/aura.tar.gz"; then
    die "download failed: ${url}
  No asset for this platform yet? Install from source:
    git clone --branch v${version} https://github.com/${REPO}.git
    cd aura && cargo install --path crates/aura-cli"
  fi

  info "extracting into versions/${version}"
  tar -xzf "${tmp}/aura.tar.gz" -C "$tmp"
  stage="$(find "$tmp" -mindepth 1 -maxdepth 1 -type d ! -name '.*' | head -1)"
  [[ -n "$stage" ]] || die "empty archive"
  local bin="${stage}/bin/aura"
  [[ -f "$bin" ]] || bin="$(find "$tmp" -type f -name aura | head -1)"
  [[ -n "$bin" && -f "$bin" ]] || die "archive missing aura binary"

  rm -rf "$vdir"
  mkdir -p "${vdir}/bin" "${vdir}/share/aura" "${vdir}/meta"
  if command -v install >/dev/null 2>&1; then
    install -m 755 "$bin" "${vdir}/bin/aura"
  else
    cp "$bin" "${vdir}/bin/aura"
    chmod 755 "${vdir}/bin/aura"
  fi

  # Prefer runtime from tarball share/; else skip (CLI embeds runtime).
  if [[ -f "${stage}/share/aura/aura_rt.c" ]]; then
    cp "${stage}/share/aura/aura_rt.c" "${vdir}/share/aura/aura_rt.c"
  elif [[ -f "${stage}/share/aura_rt.c" ]]; then
    cp "${stage}/share/aura_rt.c" "${vdir}/share/aura/aura_rt.c"
  fi

  printf '%s\n' "$version" >"${vdir}/meta/version"
  printf '%s\n' "$os" >"${vdir}/meta/os"
  printf '%s\n' "$arch" >"${vdir}/meta/arch"
  date -u +"%Y-%m-%dT%H:%M:%SZ" >"${vdir}/meta/installed_at"
  printf '%s\n' "$REPO" >"${vdir}/meta/repo"

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
