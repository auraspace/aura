#!/usr/bin/env bash
# Validate bounded, offline registry/release acceptance evidence.
# Offline crypto is real and required; this validator does not turn it into a
# claim that a live production registry or production credentials were tested.
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

if record.get("schema_version") != 4:
    raise SystemExit("unsupported registry acceptance report schema")
if record.get("network") is not False or record.get("production_claim") is not False:
    raise SystemExit("offline fixture report must not claim production network acceptance")
if record.get("outcome") != "pass":
    raise SystemExit("registry acceptance did not pass")
if record.get("protocol") != "rfc005-go-style-origin-v1":
    raise SystemExit("registry acceptance protocol evidence is incomplete")
if record.get("registry_fixture") != "u8_local_origin_release_acceptance":
    raise SystemExit("registry acceptance fixture identity is incomplete")
if record.get("cross_host") != "artifact-file-acceptance":
    raise SystemExit("registry acceptance cross-host limitation is incomplete")
host = record.get("host")
if not isinstance(host, str) or "-" not in host or not all(host.split("-", 1)):
    raise SystemExit("registry acceptance host evidence is incomplete")

origin_read = record.get("origin_read")
if not isinstance(origin_read, dict) or origin_read.get("transport") != "direct-git":
    raise SystemExit("direct Git origin evidence is incomplete")
for field in ("tag_to_commit", "archive_checksum", "warm_cache_offline"):
    if origin_read.get(field) is not True:
        raise SystemExit(f"direct Git origin acceptance field is not proven: {field}")

proxy = record.get("proxy")
if not isinstance(proxy, dict) or proxy.get("status") != "prepared-not-served":
    raise SystemExit("proxy preparation boundary is incomplete")
if proxy.get("protocol") != "aura-origin-v1":
    raise SystemExit("proxy protocol boundary is incomplete")
if proxy.get("objects") != ["@v/list", "@v/{version}.info", "@v/{version}.mod", "@v/{version}.zip"]:
    raise SystemExit("proxy read object shapes are incomplete")

publish = record.get("publish")
if not isinstance(publish, dict):
    raise SystemExit("registry acceptance is missing publish evidence")
if publish.get("origin") != "verified-local-fixture":
    raise SystemExit("origin publication evidence was not verified by the local fixture")
if publish.get("archive") != "materialized-at-origin":
    raise SystemExit("origin archive materialization evidence is incomplete")
if publish.get("identity") != "package/version/checksum":
    raise SystemExit("origin publication identity contract is incomplete")

update = record.get("update")
if not isinstance(update, dict):
    raise SystemExit("registry acceptance is missing update evidence")
if update.get("checksum") != "verified-local-fixture":
    raise SystemExit("update checksum evidence is missing")
if update.get("rollback") != "verified-local-fixture":
    raise SystemExit("update rollback evidence is missing")
if update.get("signature") != "verified-aura-sig-v1":
    raise SystemExit("update signature must be verified with aura-sig-v1")

crypto = record.get("crypto")
if not isinstance(crypto, dict):
    raise SystemExit("registry acceptance is missing cryptographic evidence")
if crypto.get("format") != "aura-sig-v1":
    raise SystemExit("unsupported registry signature format")
for field in ("trusted_key_verification", "tamper_rejection", "replay_rejection", "fail_closed"):
    if crypto.get(field) is not True:
        raise SystemExit(f"cryptographic acceptance field is not proven: {field}")
if record.get("production_credentials") != "not-configured":
    raise SystemExit("production credential limitation must be explicit")

print(f"registry acceptance evidence: PASS ({path})")
PY
