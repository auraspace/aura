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
fn emits_and_runs_string_interpolation_with_integer_operands() {
    let file = parse_file(include_str!("../../../../../corpus/expr/int_tostring.aura")).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let out = PathBuf::from(format!(
        "/tmp/aura-llvm-int-tostring-{}",
        std::process::id()
    ));
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn emits_and_runs_try_catch_finally_with_call_and_payload() {
    let file = parse_file(include_str!("../../../../../corpus/control/try_catch.aura")).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let out = PathBuf::from(format!("/tmp/aura-llvm-try-catch-{}", std::process::id()));
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
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "boom\nfinally\ncaught-int\n"
    );
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn emits_and_runs_try_catch_return_through_finally() {
    let file = parse_file(include_str!(
        "../../../../../corpus/control/try_catch_return.aura"
    ))
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let out = PathBuf::from(format!(
        "/tmp/aura-llvm-try-catch-return-{}",
        std::process::id()
    ));
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "recover\n");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn emits_string_length_and_indexing() {
    let file = parse_file(include_str!(
        "../../../../../corpus/control/for_in_string.aura"
    ))
    .unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert!(module.contains("call i64 @aura_llvm_str_len"));
    assert!(module.contains("zext i8"));
    assert_llvm_compiles(&module, "string-index");
}

#[test]
fn emits_non_unit_control_flow_with_unreachable_joins() {
    let file = parse_file(include_str!("../../../../../corpus/control/if_while.aura")).unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert!(module.contains("define i64 @aura_demo_control_abs"));
    assert_llvm_compiles(&module, "control-flow");
}

#[test]
fn emits_async_mir_as_immediate_tasks() {
    let file = parse_file("package demo\nasync fun tick(): Int { return 1 }\n").unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let module = LlvmBackend::emit_module(&program).unwrap();
    assert!(module.contains("define i64 @aura_demo_tick"));
    assert_llvm_compiles(&module, "async-module");
}

#[test]
fn emits_typed_channel_operations() {
    let file = parse_file(
        "package demo\nfun main() {\n  val channel: Channel<Int> = Channel<Int>(1)\n  channel.send(7)\n  channel.close()\n}\n",
    )
    .unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert!(module.contains("@aura_llvm_channel_new"));
    assert!(module.contains("@aura_llvm_channel_send"));
    assert_llvm_compiles(&module, "channel-module");
}

#[test]
fn emits_llvm_builtins_and_intrinsics() {
    let file = parse_file(
        "package demo\nfun main() {\n  gc_collect()\n  val value: Float = 4.toFloat()\n  assert(value.toInt() == 4)\n  assert_eq(value.toInt(), 4)\n  println(value.toString())\n}\n",
    )
    .unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert!(module.contains("@aura_llvm_gc_collect"));
    assert!(module.contains("@aura_llvm_float_to_string"));
    assert!(module.contains("@aura_llvm_assert"));
    assert_llvm_compiles(&module, "builtin-module");
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

#[test]
fn compiles_integer_main_as_process_status() {
    let file = parse_file("package demo\nfun main(): Int { return 7 }\n").unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let out = PathBuf::from(format!("/tmp/aura-llvm-status-{}", std::process::id()));
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
    let status = std::process::Command::new(&out).status().unwrap();
    assert_eq!(status.code(), Some(7));
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn runs_nullable_string_unwrap() {
    let file = parse_file(
        "package demo\nfun unwrap(value: String?): String { return value!! }\nfun main() { println(unwrap(\"value\")) }\n",
    )
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-unwrap-{}", std::process::id()));
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "value\n");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn runs_unit_enum_switches() {
    let file = parse_file(include_str!("../../../../../corpus/enum/color.aura")).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-enum-{}", std::process::id()));
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "red\ngreen\n");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn runs_primitive_enum_payloads() {
    let file = parse_file(include_str!("../../../../../corpus/enum/payloads.aura")).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!(
        "/tmp/aura-llvm-enum-payload-{}",
        std::process::id()
    ));
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
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "unit\nvalue\npair\n"
    );
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn runs_class_identity_and_primitive_constructor_fields() {
    let file = parse_file(include_str!("../../../../../corpus/class/identity.aura")).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-class-{}", std::process::id()));
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
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "same\ndiff\ndistinct\n"
    );
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn runs_typed_arrays_and_for_in() {
    let file = parse_file(include_str!("../../../../../corpus/control/for_in.aura")).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-array-{}", std::process::id()));
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
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "for-in ok\nhi\naura\nfor-in break ok\n"
    );
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn runs_class_methods_and_field_updates() {
    let file = parse_file(include_str!(
        "../../../../../corpus/class/field_update.aura"
    ))
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-method-{}", std::process::id()));
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "field\n");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn emits_foreign_declarations_without_lowering_bodies() {
    let file = parse_file(include_str!(
        "../../../../../corpus/alpha/ffi_declaration.aura"
    ))
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let module = LlvmBackend::emit_module(&program).unwrap();
    assert!(module.contains("declare i64 @native_abs(i64)"));
    assert_llvm_compiles(&module, "ffi-declaration");
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
