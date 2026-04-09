use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn exceptions_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("exceptions test lock poisoned")
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

#[test]
fn aurac_runs_exception_finally_example_end_to_end() {
    let _guard = exceptions_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    let output = Command::new(aurac)
        .args(["run", "examples/exceptions/finally.aura"])
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
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines, vec!["return-body", "return-finally"]);

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}

#[test]
fn aurac_runs_exception_catch_example_end_to_end() {
    let _guard = exceptions_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    let output = Command::new(aurac)
        .args(["run", "examples/exceptions/caught.aura"])
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
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines, vec!["body", "caught", "finally", "after"]);

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}

#[test]
fn aurac_runs_nested_exception_example_end_to_end() {
    let _guard = exceptions_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    let output = Command::new(aurac)
        .args(["run", "examples/exceptions/nested.aura"])
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
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec![
            "outer-try",
            "inner-try",
            "inner-catch",
            "inner-finally",
            "outer-finally",
        ]
    );

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}

#[test]
fn aurac_runs_exception_finally_after_catch_return_example_end_to_end() {
    let _guard = exceptions_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    let output = Command::new(aurac)
        .args(["run", "examples/exceptions/catch_return.aura"])
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
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines, vec!["catch", "finally"]);

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}

#[test]
fn aurac_reports_uncaught_exception_in_main() {
    let _guard = exceptions_test_lock();
    let root = workspace_root();
    build_runtime_if_needed(&root);

    let aurac = env!("CARGO_BIN_EXE_aurac");
    let output = Command::new(aurac)
        .args(["run", "examples/exceptions/uncaught.aura"])
        .current_dir(&root)
        .output()
        .expect("failed to run aurac");

    assert!(
        !output.status.success(),
        "aurac run unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("aura panic: uncaught exception"),
        "missing uncaught-exception panic text: {stderr}"
    );

    let _ = fs::remove_file(root.join("main.o"));
    let _ = fs::remove_file(root.join("a.out"));
    let _ = fs::remove_file(root.join("main.s"));
}
