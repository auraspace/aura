use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let root = workspace_root();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    root.join("target")
        .join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

fn run_aurac_build_emit_llvm(source_name: &str, source: &str) -> (PathBuf, String) {
    let root = workspace_root();
    let fixture_dir = unique_temp_dir("e2e-debug-fixture");
    let _ = fs::remove_dir_all(&fixture_dir);
    fs::create_dir_all(&fixture_dir).expect("failed to create fixture dir");
    fs::write(fixture_dir.join(source_name), source).expect("failed to write fixture");

    let aurac = env!("CARGO_BIN_EXE_aurac");
    let output = Command::new(aurac)
        .args(["build", source_name, "--emit=llvm"])
        .current_dir(&fixture_dir)
        .env("AURA_WORKSPACE_ROOT", &root)
        .output()
        .expect("failed to run aurac");

    assert!(
        output.status.success(),
        "aurac build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let llvm_ir =
        fs::read_to_string(fixture_dir.join("main.ll")).expect("failed to read emitted LLVM IR");

    (fixture_dir, llvm_ir)
}

#[test]
fn aurac_check_prints_ast_symbols_and_imports() {
    let root = workspace_root();
    let fixture_dir = root.join("target/e2e-debug-fixture");
    let _ = fs::remove_dir_all(&fixture_dir);
    fs::create_dir_all(&fixture_dir).expect("failed to create fixture dir");
    fs::write(
        fixture_dir.join("main.aura"),
        r#"
import { helper } from "./util";

function main(): i32 {
  return 0;
}
"#,
    )
    .expect("failed to write main fixture");
    fs::write(
        fixture_dir.join("util.aura"),
        r#"
export function helper(x: i32): i32 {
  return x;
}
"#,
    )
    .expect("failed to write util fixture");

    let aurac = env!("CARGO_BIN_EXE_aurac");

    let output = Command::new(aurac)
        .args([
            "check",
            "target/e2e-debug-fixture/main.aura",
            "--emit=ast",
            "--print=symbols",
            "--print=imports",
        ])
        .current_dir(&root)
        .output()
        .expect("failed to run aurac");

    assert!(
        output.status.success(),
        "aurac check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--- AST ---"),
        "missing AST header: {stdout}"
    );
    assert!(
        stdout.contains("--- Symbols ---"),
        "missing symbols header: {stdout}"
    );
    assert!(
        stdout.contains("--- Imports ---"),
        "missing imports header: {stdout}"
    );
    assert!(
        stdout.contains("helper"),
        "missing imported symbol or import target: {stdout}"
    );
    assert!(stdout.contains("main"), "missing entry symbol: {stdout}");
    assert!(
        stdout.contains("./util"),
        "missing resolved import specifier: {stdout}"
    );

    let _ = fs::remove_dir_all(&fixture_dir);
}

#[test]
fn aurac_build_emits_runtime_calls_for_strings_and_allocation() {
    let (fixture_dir, llvm_ir) = run_aurac_build_emit_llvm(
        "main.aura",
        r#"
class Box {
  value: i32;

  function constructor(value: i32): void {
    this.value = value;
  }
}

function main(): void {
  let box = new Box(1);
  println("hello " + "Aura");
}
"#,
    );

    assert!(
        llvm_ir.contains("aura_alloc"),
        "missing aura_alloc in emitted LLVM IR:\n{llvm_ir}"
    );
    assert!(
        llvm_ir.contains("aura_string_new_utf8"),
        "missing aura_string_new_utf8 in emitted LLVM IR:\n{llvm_ir}"
    );
    assert!(
        llvm_ir.contains("aura_string_concat"),
        "missing aura_string_concat in emitted LLVM IR:\n{llvm_ir}"
    );
    assert!(
        llvm_ir.contains("aura_println"),
        "missing aura_println in emitted LLVM IR:\n{llvm_ir}"
    );

    let _ = fs::remove_dir_all(&fixture_dir);
}

#[test]
fn aurac_build_rejects_unsupported_emit_modes_for_clif() {
    let root = workspace_root();
    let fixture_dir = unique_temp_dir("e2e-debug-fixture");
    let _ = fs::remove_dir_all(&fixture_dir);
    fs::create_dir_all(&fixture_dir).expect("failed to create fixture dir");
    fs::write(
        fixture_dir.join("main.aura"),
        r#"
function main(): void {
  println("hello");
}
"#,
    )
    .expect("failed to write fixture");

    let aurac = env!("CARGO_BIN_EXE_aurac");
    let output = Command::new(aurac)
        .args(["build", "main.aura", "--backend=clif", "--emit=llvm"])
        .current_dir(&fixture_dir)
        .env("AURA_WORKSPACE_ROOT", &root)
        .output()
        .expect("failed to run aurac");

    assert!(
        !output.status.success(),
        "expected clif backend to reject --emit=llvm\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("backend `clif` does not support `--emit=llvm` yet"),
        "missing backend rejection message: {stderr}"
    );

    let _ = fs::remove_dir_all(&fixture_dir);
}
