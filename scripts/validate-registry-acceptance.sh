#!/usr/bin/env bash
# Validate the bounded, offline registry/release acceptance evidence.
# This intentionally rejects a report that presents deferred update signing or
# unavailable production network credentials as a successful production claim.
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

report=""
case "${1:-}" in
  --report)
    [[ $# -eq 2 ]] || { printf 'usage: %s --report FILE\n' "$0" >&2; exit 2; }
    report="$2"
    ;;
  *)
    printf 'usage: %s --report FILE\n' "$0" >&2
    exit 2
    ;;
esac

python3 - "$report" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
try:
    record = json.loads(path.read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError) as exc:
    raise SystemExit(f"invalid registry acceptance report: {exc}") from exc

if record.get("schema_version") != 1:
    raise SystemExit("unsupported registry acceptance report schema")
if record.get("network") is not False or record.get("production_claim") is not False:
    raise SystemExit("offline fixture report must not claim production network acceptance")
if record.get("outcome") != "pass":
    raise SystemExit("registry acceptance did not pass")

publish = record.get("publish")
if not isinstance(publish, dict):
    raise SystemExit("registry acceptance is missing publish evidence")
if publish.get("http_status") != 201:
    raise SystemExit("publish receipt evidence must be HTTP 201")
if publish.get("receipt") != "verified-local-fixture":
    raise SystemExit("publish receipt was not verified by the local fixture")
if publish.get("identity") != "package/version/checksum":
    raise SystemExit("publish receipt identity contract is incomplete")

update = record.get("update")
if not isinstance(update, dict):
    raise SystemExit("registry acceptance is missing update evidence")
if update.get("checksum") != "verified-local-fixture":
    raise SystemExit("update checksum evidence is missing")
if update.get("rollback") != "verified-local-fixture":
    raise SystemExit("update rollback evidence is missing")
if update.get("signature") != "deferred-alpha-primitive":
    raise SystemExit("update signature must be explicitly deferred in alpha")
if record.get("production_credentials") != "not-configured":
    raise SystemExit("production credential limitation must be explicit")

print(f"registry acceptance evidence: PASS ({path})")
PY
