use std::path::PathBuf;

use super::LlvmBackend;
use crate::driver::BackendBuildOptions;
use crate::options::{Backend, Lto, OptimizationLevel, OutputKind, PanicStrategy, Profile, Target};
use aura_ir::LoweredProgram;
use aura_parser::parse_file;
use aura_sema::check_file;

#[test]
fn emits_valid_scalar_llvm_module() {
    let file = parse_file("package demo\nfun add(a: Int, b: Int): Int { return a + b }\n").unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert!(module.contains("define i64 @aura_demo_add"));
    assert!(module.contains("add i64"));
    assert_llvm_compiles(&module, "module");
}

#[test]
fn formats_float_constants_for_llvm() {
    assert_eq!(
        super::emit::format_float_constant(1.5),
        "1.50000000000000000"
    );
    assert_eq!(super::emit::format_float_constant(0.0), "0.0");
    assert_eq!(
        super::emit::llvm_type(&aura_sema::Ty::Float).unwrap(),
        "double"
    );
}

#[test]
fn emits_calls_with_the_declared_signature() {
    let file = parse_file(
        "package demo\nfun add(a: Int, b: Int): Int { return a + b }\nfun main() { add(1, 2) }\n",
    )
    .unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert!(module.contains("call i64 @aura_demo_add(i64"));
    assert_llvm_compiles(&module, "call-module");
}

#[test]
fn emits_and_runs_string_operations() {
    let file = parse_file(include_str!(
        "../../../../../corpus/expr/string_concat.aura"
    ))
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-string-{}", std::process::id()));
    let options = BackendBuildOptions {
        backend: Backend::Llvm,
        target: Target::Native,
        profile: Profile::Debug,
        optimization: OptimizationLevel::O0,
        debug: false,
        lto: Lto::Off,
        panic: PanicStrategy::Abort,
        output: OutputKind::Executable,
        features: Vec::new(),
    };
    LlvmBackend::compile(&program, &out, &options).unwrap();
    let output = std::process::Command::new(&out).output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\nlit\nmix\n");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn rejects_async_mir_before_emitting_partial_ir() {
    let file = parse_file("package demo\nasync fun tick(): Int { return 1 }\n").unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let error = LlvmBackend::emit_module(&program).unwrap_err().to_string();
    assert!(error.contains("async MIR"));
}

#[test]
fn llvm_defaults_do_not_require_the_c_runtime_abi() {
    let options = crate::llvm_options();
    assert_eq!(options.backend, Backend::Llvm);
    assert_eq!(options.runtime_abi, None);
    assert!(options.validate().is_ok());
}

#[test]
fn compiles_empty_main_with_clang_when_available() {
    if std::process::Command::new("clang")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let file = parse_file("package demo\nfun main() {}\n").unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-test-{}", std::process::id()));
    let options = BackendBuildOptions {
        backend: Backend::Llvm,
        target: Target::Native,
        profile: Profile::Debug,
        optimization: OptimizationLevel::O0,
        debug: false,
        lto: Lto::Off,
        panic: PanicStrategy::Abort,
        output: OutputKind::Executable,
        features: Vec::new(),
    };
    LlvmBackend::compile(&program, &out, &options).unwrap();
    assert!(std::process::Command::new(&out).status().unwrap().success());
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

fn assert_llvm_compiles(module: &str, suffix: &str) {
    let ir_path = PathBuf::from(format!("/tmp/aura-llvm-{suffix}-{}.ll", std::process::id()));
    let object_path = ir_path.with_extension("o");
    std::fs::write(&ir_path, module).unwrap();
    let output = std::process::Command::new("clang")
        .args(["-x", "ir", "-c"])
        .arg(&ir_path)
        .arg("-o")
        .arg(&object_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_file(ir_path);
    let _ = std::fs::remove_file(object_path);
}
