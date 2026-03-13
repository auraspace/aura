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

e2e_test!(test_01_basic_types, "01_basic_types.aura");
e2e_test!(test_02_variables_inference, "02_variables_inference.aura");
e2e_test!(test_03_arithmetic, "03_arithmetic.aura");
e2e_test!(test_04_comparison, "04_comparison.aura");
e2e_test!(test_05_if_else, "05_if_else.aura");
e2e_test!(test_06_while_loop, "06_while_loop.aura");
e2e_test!(test_07_functions, "07_functions.aura");
e2e_test!(test_08_recursion, "08_recursion.aura");
e2e_test!(test_09_classes_basic, "09_classes_basic.aura");
e2e_test!(test_10_classes_methods, "10_classes_methods.aura");
e2e_test!(test_11_classes_static, "11_classes_inheritance.aura");
e2e_test!(test_12_mutation_loop, "12_mutation_loop.aura");
e2e_test!(test_13_string_literals, "13_string_literals.aura");
e2e_test!(test_14_nested_functions, "14_nested_functions.aura");
e2e_test!(test_15_loop_sum, "15_loop_sum.aura");
e2e_test!(test_16_unary_negation, "16_unary_negation.aura");
e2e_test!(test_17_multi_class, "17_multi_class.aura");
e2e_test!(test_18_assign_update, "18_assign_update.aura");
e2e_test!(
    test_19_function_returning_string,
    "19_function_returning_string.aura"
);
e2e_test!(test_20_class_method_chain, "20_class_method_chain.aura");
e2e_test!(test_21_template_literal, "21_template_literal.aura");
e2e_test!(test_22_async_test, "22_async_test.aura");
e2e_test!(test_23_math, "23_math.aura");
e2e_test!(test_24_try_catch_basic, "24_try_catch_basic.aura");
e2e_test!(test_25_try_catch_nested, "25_try_catch_nested.aura");
e2e_test!(test_26_fs_basic, "26_fs_basic.aura");
e2e_test!(test_30_stdlib_string_array, "30_stdlib_string_array.aura");
e2e_test!(test_40_net_tcp, "40_net_tcp.aura");
e2e_test!(test_41_http_client, "41_http_client.aura");
e2e_test!(test_44_stdlib_date, "44_stdlib_date.aura");
e2e_test!(test_60_enum_numeric, "60_enum_numeric.aura");
e2e_test!(test_61_enum_string, "61_enum_string.aura");
e2e_test!(
    test_62_enum_string_global_scope,
    "62_enum_string_global_scope.aura"
);
