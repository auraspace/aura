use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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
function helper(x: i32): i32 {
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
