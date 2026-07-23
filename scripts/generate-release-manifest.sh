#!/usr/bin/env bash
# Generate the deterministic release manifest consumed by the release gate.
# Usage: scripts/generate-release-manifest.sh --dir DIR --acceptance-dir DIR --version VERSION
set -Eeuo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

artifact_dir=""
acceptance_dir=""
version=""
output=""

die() { printf 'release manifest: error: %s\n' "$*" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir) [[ $# -gt 1 ]] || die '--dir needs a value'; artifact_dir="$2"; shift 2 ;;
    --acceptance-dir) [[ $# -gt 1 ]] || die '--acceptance-dir needs a value'; acceptance_dir="$2"; shift 2 ;;
    --version) [[ $# -gt 1 ]] || die '--version needs a value'; version="${2#v}"; shift 2 ;;
    --output) [[ $# -gt 1 ]] || die '--output needs a value'; output="$2"; shift 2 ;;
    -h|--help) sed -n '2,3p' "$0"; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
done

[[ -n "$artifact_dir" ]] || die '--dir is required'
[[ -n "$acceptance_dir" ]] || die '--acceptance-dir is required'
[[ -n "$version" ]] || die '--version is required'
[[ -d "$artifact_dir" ]] || die "artifact directory is not a directory: $artifact_dir"
[[ -d "$acceptance_dir" ]] || die "acceptance directory is not a directory: $acceptance_dir"
output="${output:-$artifact_dir/release-manifest.json}"
mkdir -p "$(dirname "$output")"

python3 - "$root/scripts/release-targets.tsv" "$artifact_dir" "$acceptance_dir" "$version" "$output" <<'PY'
import hashlib
import json
import pathlib
import sys

targets_file, artifact_dir_s, acceptance_dir_s, version, output_s = sys.argv[1:]
artifact_dir = pathlib.Path(artifact_dir_s)
acceptance_dir = pathlib.Path(acceptance_dir_s)

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

if not targets:
    raise SystemExit("target manifest has no required targets")

def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

artifacts = []
acceptance_records = []
for target, expected_mode in targets:
    archive_name = f"aura-{version}-{target}.tar.gz"
    checksum_name = f"{archive_name}.sha256"
    archive = artifact_dir / archive_name
    checksum = artifact_dir / checksum_name
    if not archive.is_file():
        raise SystemExit(f"missing required artifact: {archive_name}")
    if not checksum.is_file():
        raise SystemExit(f"missing required checksum: {checksum_name}")
    checksum_lines = checksum.read_text(encoding="utf-8").splitlines()
    if len(checksum_lines) != 1:
        raise SystemExit(f"checksum file must contain exactly one line: {checksum_name}")
    fields = checksum_lines[0].split()
    if len(fields) != 2 or fields[1].lstrip("*") != archive_name:
        raise SystemExit(f"checksum file names the wrong artifact: {checksum_name}")
    actual = sha256(archive)
    if fields[0].lower() != actual:
        raise SystemExit(f"checksum mismatch for {archive_name}")
    artifacts.append({
        "target": target,
        "archive": archive_name,
        "checksum_file": checksum_name,
        "sha256": actual,
    })

    report_name = f"{target}-acceptance.json"
    report_path = acceptance_dir / report_name
    if not report_path.is_file():
        raise SystemExit(f"missing acceptance report: {report_name}")
    try:
        report = json.loads(report_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid acceptance report {report_name}: {exc}") from exc
    if report.get("schema_version") != 2:
        raise SystemExit(f"unsupported acceptance report schema: {report_name}")
    if report.get("target") != target:
        raise SystemExit(f"acceptance target mismatch in {report_name}")
    if report.get("mode") != expected_mode:
        raise SystemExit(f"acceptance mode mismatch in {report_name}: expected {expected_mode}")
    if report.get("outcome") != "pass":
        raise SystemExit(f"acceptance did not pass: {report_name}")
    host = report.get("host")
    if not isinstance(host, dict) or not isinstance(host.get("os"), str) or not isinstance(host.get("arch"), str):
        raise SystemExit(f"acceptance host evidence is incomplete: {report_name}")
    execution = "ran" if expected_mode == "native" else "not-run"
    if report.get("execution") != execution:
        raise SystemExit(f"acceptance execution evidence mismatch in {report_name}: expected {execution}")
    if expected_mode == "native" and f"{host['os']}-{host['arch']}" != target:
        raise SystemExit(f"native acceptance host does not match target in {report_name}")
    acceptance_records.append({
        "target": target,
        "mode": report["mode"],
        "execution": execution,
        "outcome": report["outcome"],
        "report": report_name,
    })

manifest = {
    "schema_version": 1,
    "version": version,
    "artifacts": artifacts,
    "acceptance": acceptance_records,
    "signing": {
        "algorithm": "minisign",
        "manifest": "SHA256SUMS",
        "signature": "SHA256SUMS.minisig",
        "public_key": "minisign.pub",
        "required": True,
    },
}
pathlib.Path(output_s).write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
print(f"release manifest: wrote {output_s}")
PY
