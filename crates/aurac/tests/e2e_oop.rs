use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn oop_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("oop test lock poisoned")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn build_runtime_if_needed(root: &PathBuf) {
    let output = Command::new("cargo")
        .args(["build", "-p", "aura-rt", "--locked"])
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

fn run_oop_example(root: &PathBuf, aurac: &str, relative_path: &str, expected: &str) {
    let output = Command::new(aurac)
        .args(["run", relative_path])
        .current_dir(root)
        .output()
        .expect("failed to run aurac");

    assert!(
        output.status.success(),
        "aurac run failed for {relative_path}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(expected),
        "missing output `{expected}` for {relative_path}: {stdout}"
    );
}

#[test]
fn aurac_runs_oop_allocation_example_end_to_end() {
    let _guard = oop_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    run_oop_example(&root, aurac, "examples/oop/fields.aura", "42");

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}

#[test]
fn aurac_runs_oop_method_call_example_end_to_end() {
    let _guard = oop_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    run_oop_example(&root, aurac, "examples/oop/methods.aura", "42");

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}

#[test]
fn aurac_runs_oop_inheritance_example_end_to_end() {
    let _guard = oop_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    run_oop_example(&root, aurac, "examples/oop/inheritance.aura", "woof");

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}

#[test]
fn aurac_runs_oop_interface_dispatch_example_end_to_end() {
    let _guard = oop_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    run_oop_example(&root, aurac, "examples/oop/interfaces.aura", "beep");

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}
