/// E2E test runner for the Aura language.
///
/// Each `.aura` file in `tests/e2e/` has an "Expected output" block in its
/// header comments. This test runner:
///   1. Parses the expected lines from those comments.
///   2. Runs the Aura binary with `--interp` on the file.
///   3. Asserts the stdout matches the expected output.
///
/// Run with: cargo test --test e2e
use std::path::Path;
use std::process::Command;

/// Read "// Expected output:" comment block from the file.
/// Lines after that comment (until the first blank / non-comment line) are
/// treated as expected rows.
fn parse_expected(source: &str) -> Vec<String> {
    let mut lines = source.lines();
    let mut capturing = false;
    let mut expected = Vec::new();

    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed.starts_with("// Expected output:") {
            capturing = true;
            continue;
        }
        if capturing {
            if let Some(rest) = trimmed.strip_prefix("// ") {
                expected.push(rest.to_string());
            } else if trimmed == "//" {
                // blank comment line – keep going (empty expected line)
                expected.push(String::new());
            } else {
                // no longer in expected block
                break;
            }
        }
    }
    expected
}

/// Run a single `.aura` file with the selected mode and compare output.
fn run_test(aura_file: &Path, mode_override: Option<&str>) {
    let source = std::fs::read_to_string(aura_file)
        .unwrap_or_else(|e| panic!("Cannot read {:?}: {}", aura_file, e));

    let expected = parse_expected(&source);
    assert!(
        !expected.is_empty(),
        "Test file {:?} has no expected output comment block.\n\
         Add a '// Expected output:' block followed by '// <line>' lines.",
        aura_file
    );

    // Build the path to the binary (use CARGO_BIN_EXE when available, else
    // fall back to a known debug location).
    let binary =
        std::env::var("CARGO_BIN_EXE_aura").unwrap_or_else(|_| "target/debug/aura".to_string());

    let args: Vec<String> = std::env::args().collect();
    let modes_env = std::env::var("AURA_TEST_MODE").unwrap_or_default();

    // Check for modes in command-line arguments (after --)
    let modes_arg = args
        .iter()
        .find(|arg| arg.contains("interp") || arg.contains("compiler") || arg.contains("ir"));

    let modes: Vec<&str> = if let Some(m) = mode_override {
        vec![m]
    } else if let Some(arg) = modes_arg {
        arg.split(',').map(|s| s.trim()).collect()
    } else if !modes_env.is_empty() {
        modes_env.split(',').map(|s| s.trim()).collect()
    } else {
        vec!["interp"]
    };

    for mode in modes {
        let mut cmd = Command::new(&binary);
        match mode {
            "interp" => {
                cmd.arg("--interp");
            }
            "compiler" => {
                // No extra flags for basic compiler
            }
            "ir" => {
                cmd.arg("--ir");
            }
            _ => panic!("Unknown AURA_TEST_MODE: {}", mode),
        }
        cmd.arg(aura_file);

        let output = cmd.output().unwrap_or_else(|e| {
            panic!(
                "Failed to run binary '{}' for mode '{}': {}",
                binary, mode, e
            )
        });

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            panic!(
                "Aura program {:?} exited with non-zero status in mode {}.\nstdout:\n{}\nstderr:\n{}",
                aura_file, mode, stdout, stderr
            );
        }

        // Normalise: trim trailing whitespace from each line, remove trailing
        // blank lines.
        let actual: Vec<String> = stdout
            .lines()
            // Strip the banner lines that main.rs emits
            .filter(|l| {
                !l.starts_with("Interpreting:")
                    && !l.starts_with("Compiling:")
                    && !l.starts_with("--- Starting")
                    && !l.starts_with("--- Running")
                    && !l.starts_with("Assembling")
                    && !l.starts_with("Compiling runtime")
                    && !l.starts_with("Linking")
                    && !l.starts_with("Building runtime")
            })
            .map(|l| l.trim_end().to_string())
            .collect();

        // Remove trailing empties both sides for comparison stability
        let mut actual_trimmed = actual.clone();
        while actual_trimmed
            .last()
            .map(|l: &String| l.is_empty())
            .unwrap_or(false)
        {
            actual_trimmed.pop();
        }
        let mut exp_trimmed = expected.clone();
        while exp_trimmed
            .last()
            .map(|l: &String| l.is_empty())
            .unwrap_or(false)
        {
            exp_trimmed.pop();
        }

        assert_eq!(
            exp_trimmed,
            actual_trimmed,
            "Output mismatch for {:?}\nMode: {}\nExpected:\n{}\nActual:\n{}",
            aura_file,
            mode,
            exp_trimmed.join("\n"),
            actual_trimmed.join("\n")
        );
    }
}

// ───────────────────────────────────────── test cases ─────────────────────────

macro_rules! e2e_test {
    ($test_name:ident, $file:literal) => {
        mod $test_name {
            use super::*;

            #[test]
            fn interp() {
                let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests")
                    .join("e2e")
                    .join($file);
                run_test(&path, Some("interp"));
            }

            #[test]
            fn compiler() {
                let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests")
                    .join("e2e")
                    .join($file);
                run_test(&path, Some("compiler"));
            }
        }
    };
}

// --- 01_core ---
e2e_test!(core_types, "01_core/01_types.aura");
e2e_test!(core_inference, "01_core/02_inference.aura");
e2e_test!(core_arithmetic, "01_core/03_arithmetic.aura");
e2e_test!(core_comparison, "01_core/04_comparison.aura");
e2e_test!(core_strings, "01_core/05_strings.aura");
e2e_test!(core_unary, "01_core/06_unary.aura");
e2e_test!(core_assignment, "01_core/07_assignment.aura");
e2e_test!(core_template, "01_core/08_template.aura");

// --- 02_control_flow ---
e2e_test!(flow_if_else, "02_control_flow/01_if_else.aura");
e2e_test!(flow_while, "02_control_flow/02_while.aura");
e2e_test!(flow_mutation, "02_control_flow/03_mutation.aura");
e2e_test!(flow_loop_sum, "02_control_flow/04_loop_sum.aura");

// --- 03_functions ---
e2e_test!(fn_basic, "03_functions/01_basic.aura");
e2e_test!(fn_recursion, "03_functions/02_recursion.aura");
e2e_test!(fn_nested, "03_functions/03_nested.aura");
e2e_test!(fn_return_string, "03_functions/04_return_string.aura");

// --- 04_oop ---
e2e_test!(oop_basic, "04_oop/01_basic.aura");
e2e_test!(oop_methods, "04_oop/02_methods.aura");
e2e_test!(oop_inheritance, "04_oop/03_inheritance.aura");
e2e_test!(oop_multi_class, "04_oop/04_multi_class.aura");
e2e_test!(oop_chaining, "04_oop/05_chaining.aura");
e2e_test!(oop_access_modifiers, "04_oop/06_access_modifiers.aura");

// --- 05_enums ---
e2e_test!(enum_numeric, "05_enums/01_numeric.aura");
e2e_test!(enum_string, "05_enums/02_string.aura");
e2e_test!(enum_global_scope, "05_enums/03_global_scope.aura");

// --- 06_async ---
e2e_test!(async_basic, "06_async/01_async.aura");

// --- 07_error_handling ---
e2e_test!(error_basic, "07_error_handling/01_basic.aura");
e2e_test!(error_nested, "07_error_handling/02_nested.aura");

// --- 08_stdlib ---
e2e_test!(std_math, "08_stdlib/01_math.aura");
e2e_test!(std_fs, "08_stdlib/02_fs.aura");
e2e_test!(std_string_array, "08_stdlib/03_string_array.aura");
e2e_test!(std_net_tcp, "08_stdlib/04_net_tcp.aura");
e2e_test!(std_http_client, "08_stdlib/05_http_client.aura");
e2e_test!(std_http_server, "08_stdlib/06_http_server.aura");
e2e_test!(std_http_types, "08_stdlib/07_http_types.aura");
e2e_test!(std_date, "08_stdlib/08_date.aura");
