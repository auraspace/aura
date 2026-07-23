#!/usr/bin/env bash
# Verify a complete release asset bundle and its cross-host acceptance reports.
# Cryptographic signature verification is mandatory when --require-signature is used.
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

artifact_dir=""
acceptance_dir=""
version=""
require_signature=0

die() { printf 'release bundle: error: %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir) [[ $# -gt 1 ]] || die '--dir needs a value'; artifact_dir="$2"; shift 2 ;;
    --acceptance-dir) [[ $# -gt 1 ]] || die '--acceptance-dir needs a value'; acceptance_dir="$2"; shift 2 ;;
    --version) [[ $# -gt 1 ]] || die '--version needs a value'; version="${2#v}"; shift 2 ;;
    --require-signature) require_signature=1; shift ;;
    -h|--help) sed -n '2,3p' "$0"; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
done

[[ -n "$artifact_dir" ]] || die '--dir is required'
[[ -n "$acceptance_dir" ]] || die '--acceptance-dir is required'
[[ -n "$version" ]] || die '--version is required'
[[ -d "$artifact_dir" ]] || die "artifact directory is not a directory: $artifact_dir"
[[ -d "$acceptance_dir" ]] || die "acceptance directory is not a directory: $acceptance_dir"

manifest="$artifact_dir/release-manifest.json"
sums="$artifact_dir/SHA256SUMS"
[[ -s "$manifest" ]] || die 'release-manifest.json is missing or empty'
[[ -s "$sums" ]] || die 'SHA256SUMS is missing or empty'

python3 - "$root/scripts/release-targets.tsv" "$artifact_dir" "$acceptance_dir" "$version" "$manifest" "$sums" <<'PY'
import hashlib
import json
import pathlib
import sys

targets_file, artifact_dir_s, acceptance_dir_s, version, manifest_s, sums_s = sys.argv[1:]
artifact_dir = pathlib.Path(artifact_dir_s)
acceptance_dir = pathlib.Path(acceptance_dir_s)
manifest_path = pathlib.Path(manifest_s)

targets = []
for line in pathlib.Path(targets_file).read_text(encoding="utf-8").splitlines():
    if not line.strip() or line.lstrip().startswith("#"):
        continue
    fields = line.split("\t")
    if len(fields) != 6:
        raise SystemExit(f"malformed target row: {line}")
    target, tier, _runner, _format, _install, acceptance = fields
    if tier == "required":
        targets.append((target, acceptance))

try:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
except json.JSONDecodeError as exc:
    raise SystemExit(f"invalid release manifest: {exc}") from exc
if manifest.get("schema_version") != 1:
    raise SystemExit("unsupported release manifest schema")
if manifest.get("version") != version:
    raise SystemExit("release manifest version does not match requested version")
if manifest.get("signing", {}).get("required") is not True:
    raise SystemExit("release manifest does not require signing")

expected_artifacts = []
for target, _mode in targets:
    archive = f"aura-{version}-{target}.tar.gz"
    expected_artifacts.append((target, archive, f"{archive}.sha256"))
manifest_artifacts = manifest.get("artifacts")
if not isinstance(manifest_artifacts, list) or len(manifest_artifacts) != len(expected_artifacts):
    raise SystemExit("release manifest has the wrong artifact count")
for (target, archive, checksum), record in zip(expected_artifacts, manifest_artifacts):
    if (record.get("target"), record.get("archive"), record.get("checksum_file")) != (target, archive, checksum):
        raise SystemExit(f"release manifest target/artifact mismatch for {target}")
    path = artifact_dir / archive
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if record.get("sha256") != digest:
        raise SystemExit(f"release manifest checksum mismatch for {archive}")
    checksum_path = artifact_dir / checksum
    fields = checksum_path.read_text(encoding="utf-8").split()
    if len(fields) != 2 or fields[0].lower() != digest or fields[1].lstrip("*") != archive:
        raise SystemExit(f"per-artifact checksum mismatch for {archive}")

expected_acceptance = []
for target, mode in targets:
    expected_acceptance.append({"target": target, "mode": mode, "outcome": "pass", "report": f"{target}-acceptance.json"})
if manifest.get("acceptance") != expected_acceptance:
    raise SystemExit("release manifest acceptance records do not match target policy")
for record in expected_acceptance:
    report_path = acceptance_dir / record["report"]
    report = json.loads(report_path.read_text(encoding="utf-8"))
    if report.get("target") != record["target"] or report.get("mode") != record["mode"] or report.get("outcome") != "pass":
        raise SystemExit(f"acceptance report does not satisfy target policy: {record['report']}")

expected_payload = {"release-manifest.json"}
for _target, archive, checksum in expected_artifacts:
    expected_payload.update((archive, checksum))
sum_names = set()
for line in pathlib.Path(sums_s).read_text(encoding="utf-8").splitlines():
    fields = line.split()
    if len(fields) != 2:
        raise SystemExit("SHA256SUMS contains a malformed line")
    sum_names.add(fields[1].lstrip("*"))
if sum_names != expected_payload:
    raise SystemExit(f"SHA256SUMS payload set mismatch: {sorted(sum_names)}")
print(f"release bundle: validated {len(expected_artifacts)} target artifact(s)")
PY

(cd "$artifact_dir" && sha256sum --strict --check SHA256SUMS >/dev/null) \
  || die 'SHA256SUMS verification failed'

if [[ "$require_signature" -eq 1 ]]; then
  [[ -s "$artifact_dir/SHA256SUMS.minisig" ]] || die 'required SHA256SUMS.minisig is missing or empty'
  [[ -s "$artifact_dir/minisign.pub" ]] || die 'required minisign.pub is missing or empty'
  command -v minisign >/dev/null 2>&1 || die 'minisign is required for signed bundle verification'
  minisign -Vm "$sums" -p "$artifact_dir/minisign.pub" >/dev/null \
    || die 'minisign verification failed'
fi

printf 'release bundle: PASS version=%s signature_required=%s\n' "$version" "$([[ "$require_signature" -eq 1 ]] && echo yes || echo no)"
