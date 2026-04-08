use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn build_runtime_if_needed(root: &PathBuf) {
    let runtime_artifact = root.join("target/debug/libaura_rt.a");
    if runtime_artifact.exists() {
        return;
    }

    let output = Command::new("cargo")
        .args(["build", "-p", "aura-rt"])
        .current_dir(root)
        .output()
        .expect("failed to run cargo build for aura-rt");

    assert!(
        output.status.success(),
        "cargo build -p aura-rt failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn aurac_runs_oop_allocation_example_end_to_end() {
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    let output = Command::new(aurac)
        .args(["run", "examples/oop/fields.aura"])
        .current_dir(&root)
        .output()
        .expect("failed to run aurac");

    assert!(
        output.status.success(),
        "aurac run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("42"), "missing field output: {stdout}");

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}
