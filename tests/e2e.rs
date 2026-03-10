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

/// Run a single `.aura` file with the interpreter and compare output.
fn run_test(aura_file: &Path) {
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
    let binary = std::env::var("CARGO_BIN_EXE_aura-rust")
        .unwrap_or_else(|_| "target/debug/aura-rust".to_string());

    let output = Command::new(&binary)
        .arg("--interp")
        .arg(aura_file)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run binary '{}': {}", binary, e));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        panic!(
            "Aura program {:?} exited with non-zero status.\nstdout:\n{}\nstderr:\n{}",
            aura_file, stdout, stderr
        );
    }

    // Normalise: trim trailing whitespace from each line, remove trailing
    // blank lines.
    let actual: Vec<String> = stdout
        .lines()
        // Strip the "Interpreting: ..." banner line that main.rs emits
        .filter(|l| !l.starts_with("Interpreting:") && !l.starts_with("--- Starting"))
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
        "Output mismatch for {:?}\nExpected:\n{}\nActual:\n{}",
        aura_file,
        exp_trimmed.join("\n"),
        actual_trimmed.join("\n")
    );
}

// ───────────────────────────────────────── test cases ─────────────────────────

macro_rules! e2e_test {
    ($test_name:ident, $file:literal) => {
        #[test]
        fn $test_name() {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("e2e")
                .join($file);
            run_test(&path);
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
