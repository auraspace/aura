#!/usr/bin/env bash
# Prepare a release commit: dump version (Cargo.toml + Cargo.lock) + changelog + notes stub.
#
# Usage:
#   scripts/prepare-release.sh <version> [options]
#
# Examples:
#   scripts/prepare-release.sh 0.1.0-alpha
#   scripts/prepare-release.sh 0.1.0 --dry-run
#   scripts/prepare-release.sh 0.2.0-beta.1 --no-commit
#   scripts/prepare-release.sh 0.1.0-alpha --message "First public dogfood"
#
# Options:
#   --dry-run       Print planned changes; do not write or commit
#   --no-commit     Update files only (leave unstaged unless --stage)
#   --stage         Stage updated files (implied by commit mode)
#   --force         Allow dirty tree / overwrite existing CHANGELOG section
#   --message TEXT  Short summary line under the CHANGELOG heading
#   --since REF     Git range start for changelog bullets (default: last v* tag)
#   --date YYYY-MM-DD  Override date (default: today UTC)
#
# After a successful commit, cut the tag yourself (not done here):
#   git push origin HEAD
#   git tag v<version> && git push origin v<version>
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

usage() {
  sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
info() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }

VERSION=""
DRY_RUN=0
DO_COMMIT=1
DO_STAGE=0
FORCE=0
MESSAGE=""
SINCE=""
DATE="$(date -u +%Y-%m-%d)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --no-commit) DO_COMMIT=0; shift ;;
    --stage) DO_STAGE=1; shift ;;
    --force) FORCE=1; shift ;;
    --message)
      [[ $# -ge 2 ]] || die "--message needs a value"
      MESSAGE="$2"
      shift 2
      ;;
    --since)
      [[ $# -ge 2 ]] || die "--since needs a ref"
      SINCE="$2"
      shift 2
      ;;
    --date)
      [[ $# -ge 2 ]] || die "--date needs YYYY-MM-DD"
      DATE="$2"
      shift 2
      ;;
    -*)
      die "unknown option: $1 (try --help)"
      ;;
    *)
      if [[ -z "$VERSION" ]]; then
        VERSION="$1"
        shift
      else
        die "unexpected argument: $1"
      fi
      ;;
  esac
done

[[ -n "$VERSION" ]] || usage 1

# Normalize: strip accidental leading v
VERSION="${VERSION#v}"
TAG="v${VERSION}"

# Validate release version (semver-ish: X.Y.Z or X.Y.Z-prerelease)
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
  die "invalid version '$VERSION' (want e.g. 0.1.0, 0.1.0-alpha, 0.2.0-rc.1)"
fi

# Cargo package version: keep full prerelease string (Cargo accepts 0.1.0-alpha).
CARGO_VERSION="$VERSION"

CARGO_TOML="$ROOT/Cargo.toml"
CARGO_LOCK="$ROOT/Cargo.lock"
CHANGELOG="$ROOT/CHANGELOG.md"
NOTES_DIR="$ROOT/docs/releases"
NOTES_FILE="$NOTES_DIR/${VERSION}.md"

current_cargo_version() {
  grep -E '^version = ' "$CARGO_TOML" | head -1 | sed 's/.*"\(.*\)"/\1/'
}

last_version_tag() {
  # Prefer tags that look like vX.Y.Z…
  git tag -l 'v*' --sort=-v:refname 2>/dev/null | head -1 || true
}

resolve_since() {
  if [[ -n "$SINCE" ]]; then
    printf '%s\n' "$SINCE"
    return
  fi
  local t
  t="$(last_version_tag)"
  if [[ -n "$t" ]]; then
    printf '%s\n' "$t"
  else
    # First release: empty range start → use root commit parent trick below
    printf '\n'
  fi
}

# Collect conventional-ish bullets from git log.
collect_commits() {
  local since="$1"
  local range
  if [[ -n "$since" ]]; then
    range="${since}..HEAD"
  else
    range="HEAD"
  fi
  # Exclude previous release commits from the bullet list
  git log "$range" --no-merges --pretty=format:'%s' 2>/dev/null \
    | grep -vE '^release:' \
    | head -80 \
    | sed 's/^/- /' || true
}

ensure_changelog_file() {
  if [[ ! -f "$CHANGELOG" ]]; then
    printf '# Changelog\n\n' >"$CHANGELOG"
  fi
}

changelog_has_section() {
  grep -qE "^## ${VERSION//./\\.}( |$|\\()" "$CHANGELOG" 2>/dev/null
}

build_changelog_section() {
  local since bullets body summary
  since="$(resolve_since)"
  bullets="$(collect_commits "$since")"
  if [[ -z "$bullets" ]]; then
    bullets="- (no commits listed — edit this section before tagging)"
  fi

  if [[ -n "$MESSAGE" ]]; then
    summary="$MESSAGE"
  else
    summary="Release \`${VERSION}\`."
  fi

  body="## ${VERSION} (${DATE})

${summary}"

  if [[ -f "$NOTES_FILE" ]] || [[ "$DRY_RUN" -eq 0 ]]; then
    body+="

Full notes: [\`docs/releases/${VERSION}.md\`](docs/releases/${VERSION}.md)."
  fi

  body+="

### Changes

${bullets}
"
  printf '%s\n' "$body"
}

insert_changelog_section() {
  local section="$1"
  ensure_changelog_file
  if changelog_has_section; then
    if [[ "$FORCE" -eq 0 ]]; then
      die "CHANGELOG.md already has section for ${VERSION} (use --force to rebuild it)"
    fi
    # Drop the existing section for this version (until next ## or EOF)
    local tmp
    tmp="$(mktemp)"
    awk -v ver="$VERSION" '
      BEGIN { skip=0 }
      /^## / {
        if ($0 ~ "^## " ver "( |$|\\()") { skip=1; next }
        else if (skip) { skip=0 }
      }
      !skip { print }
    ' "$CHANGELOG" >"$tmp"
    mv "$tmp" "$CHANGELOG"
  fi

  local tmp
  tmp="$(mktemp)"
  if head -1 "$CHANGELOG" | grep -qE '^# '; then
    {
      head -1 "$CHANGELOG"
      echo
      printf '%s\n' "$section"
      # rest after first line, strip leading blank lines once
      tail -n +2 "$CHANGELOG" | sed -e '1{/^$/d;}'
    } >"$tmp"
  else
    {
      echo "# Changelog"
      echo
      printf '%s\n' "$section"
      cat "$CHANGELOG"
    } >"$tmp"
  fi
  mv "$tmp" "$CHANGELOG"
}

set_cargo_version() {
  local cur
  cur="$(current_cargo_version)"
  if [[ "$cur" == "$CARGO_VERSION" ]]; then
    info "Cargo.toml version already $CARGO_VERSION"
  else
    # Only touch [workspace.package] version line (first ^version = )
    local tmp
    tmp="$(mktemp)"
    awk -v ver="$CARGO_VERSION" '
      BEGIN { done=0 }
      !done && /^version = "/ {
        print "version = \"" ver "\""
        done=1
        next
      }
      { print }
    ' "$CARGO_TOML" >"$tmp"
    mv "$tmp" "$CARGO_TOML"
    info "Cargo.toml: $cur → $CARGO_VERSION"
  fi
}

# Keep Cargo.lock in the release commit (binary workspace — lock is source of truth for CI).
refresh_cargo_lock() {
  if ! command -v cargo >/dev/null 2>&1; then
    warn "cargo not on PATH — left Cargo.lock unchanged (stage it manually if versions drift)"
    return
  fi
  # Rewrites package versions for workspace members after Cargo.toml bump.
  cargo generate-lockfile --quiet
  info "refreshed Cargo.lock"
}

scaffold_notes() {
  mkdir -p "$NOTES_DIR"
  if [[ -f "$NOTES_FILE" ]]; then
    info "notes already exist: docs/releases/${VERSION}.md"
    return
  fi
  cat >"$NOTES_FILE" <<EOF
# Aura ${VERSION}

| Field       | Value                         |
| ----------- | ----------------------------- |
| **Version** | \`${VERSION}\`                |
| **Date**    | ${DATE}                       |
| **Status**  | Pre-release / release         |
| **Tag**     | \`${TAG}\`                    |

## Install

\`\`\`bash
curl -fsSL https://aura.fadosoft.com/install.sh | AURA_VERSION=${VERSION} bash
aura version
aura-switch --list
\`\`\`

## Highlights

- (edit before tagging)

## Known limits

- See previous freeze notes / roadmap for deferred work.
EOF
  info "created docs/releases/${VERSION}.md (edit highlights before tagging)"
}

check_clean_enough() {
  if [[ "$FORCE" -eq 1 ]] || [[ "$DO_COMMIT" -eq 0 ]]; then
    return
  fi
  # Allow files this script rewrites; block unrelated dirt when committing.
  local dirty
  dirty="$(git status --porcelain | while IFS= read -r line; do
    # porcelain v1: XY PATH  or  XY ORIG -> PATH
    st="${line:0:2}"
    path="${line:3}"
    path="${path##* -> }"
    case "$path" in
      Cargo.toml|Cargo.lock|CHANGELOG.md|docs/releases/*) ;;
      *) printf '%s %s\n' "$st" "$path" ;;
    esac
  done || true)"
  if [[ -n "$dirty" ]]; then
    die "working tree has unrelated changes; commit/stash them or pass --force

$dirty"
  fi
}

print_flow_next() {
  # Prefer printf over a long heredoc (some terminals/pagers mishandle cat).
  printf '\n'
  printf 'Release commit ready for %s.\n' "${VERSION}"
  printf '\n'
  printf 'Next (manual — this script does not push or tag):\n'
  printf '\n'
  printf '  1. Review:\n'
  printf '       git show --stat\n'
  printf '       $EDITOR docs/releases/%s.md CHANGELOG.md\n' "${VERSION}"
  printf '\n'
  printf '  2. Push the release commit:\n'
  printf '       git push origin HEAD\n'
  printf '\n'
  printf '  3. Cut the tag (triggers GitHub Actions → tarballs + GH Release):\n'
  printf '       git tag %s\n' "${TAG}"
  printf '       git push origin %s\n' "${TAG}"
  printf '\n'
  printf '  4. Install smoke:\n'
  printf '       curl -fsSL https://aura.fadosoft.com/install.sh | AURA_VERSION=%s bash\n' "${VERSION}"
  printf '       aura version\n'
  printf '\n'
  printf 'Flow:  prepare-release → push commit → push tag v* → CI Release → install.sh\n'
}

# --- main ---

info "prepare release ${VERSION} (tag ${TAG}, cargo ${CARGO_VERSION}, date ${DATE})"
PREV="$(last_version_tag)"
SINCE_RES="$(resolve_since)"
if [[ -n "$SINCE_RES" ]]; then
  info "changelog commits since ${SINCE_RES}"
elif [[ -n "$PREV" ]]; then
  info "changelog commits since ${PREV}"
else
  info "no previous v* tag — changelog uses recent commits on HEAD"
fi

SECTION="$(build_changelog_section)"

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo
  echo "----- planned Cargo.toml version -----"
  echo "$CARGO_VERSION  (current: $(current_cargo_version))"
  echo
  echo "----- Cargo.lock -----"
  echo "refresh via cargo generate-lockfile and include in commit"
  echo
  echo "----- planned CHANGELOG section -----"
  printf '%s\n' "$SECTION"
  echo "----- notes file -----"
  if [[ -f "$NOTES_FILE" ]]; then
    echo "keep existing $NOTES_FILE"
  else
    echo "create $NOTES_FILE (stub)"
  fi
  echo
  echo "commit: $([[ $DO_COMMIT -eq 1 ]] && echo yes || echo no)"
  exit 0
fi

check_clean_enough
set_cargo_version
refresh_cargo_lock
insert_changelog_section "$SECTION"
scaffold_notes

if [[ "$DO_COMMIT" -eq 1 ]] || [[ "$DO_STAGE" -eq 1 ]]; then
  git add "$CARGO_TOML" "$CARGO_LOCK" "$CHANGELOG" "$NOTES_FILE"
  # If notes already existed, still ensure it's in the index when part of release
  git add "$NOTES_DIR/${VERSION}.md" 2>/dev/null || true
fi

if [[ "$DO_COMMIT" -eq 1 ]]; then
  if git diff --cached --quiet; then
    die "nothing staged to commit (version already set and changelog unchanged?)"
  fi
  # Avoid git opening a pager on hook/commit output in some environments.
  GIT_PAGER=cat git -c core.pager=cat commit -m "release: ${VERSION}"
  info "created commit: release: ${VERSION}"
  print_flow_next
  info "done — exiting"
  exit 0
fi

info "files updated (no commit). Review with: git diff"
if [[ "$DO_STAGE" -eq 1 ]]; then
  info "staged: Cargo.toml Cargo.lock CHANGELOG.md docs/releases/${VERSION}.md"
fi
exit 0
