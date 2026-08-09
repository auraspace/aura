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
    assert!(output.status.success(), "{output:?}");
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
fn emits_and_runs_class_exception_rethrow_and_finally() {
    let file = parse_file(include_str!(
        "../../../../../corpus/control/class_throw.aura"
    ))
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let out = PathBuf::from(format!("/tmp/aura-llvm-class-throw-{}", std::process::id()));
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
        "class-boom\nfinally\ninner\n"
    );
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn emits_and_runs_float_foreign_function() {
    let file = parse_file(include_str!("../../../../../corpus/alpha/ffi_float.aura")).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let out = PathBuf::from(format!("/tmp/aura-llvm-ffi-float-{}", std::process::id()));
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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ffi-float\n");
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn emits_and_runs_exception_cause_queries() {
    let file = parse_file(include_str!(
        "../../../../../corpus/control/exception_cause_api.aura"
    ))
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let out = PathBuf::from(format!("/tmp/aura-llvm-cause-{}", std::process::id()));
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
        "1\nInt\n0\n2\nmanual\n9\n"
    );
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
fn emits_async_mir_as_scheduler_backed_tasks() {
    let file = parse_file("package demo\nasync fun tick(): Int { return 1 }\n").unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    let module = LlvmBackend::emit_module(&program).unwrap();
    assert!(module.contains("define i64 @aura_demo___aura_async_body_tick"));
    assert!(module.contains("define ptr @aura_demo_tick"));
    assert_llvm_compiles(&module, "async-module");
}

#[test]
fn emits_capture_free_spawn_through_the_task_executor() {
    let file = parse_file(
        "package demo\nfun main() { val task: TaskHandle<Int> = spawn { return 23 } }\n",
    )
    .unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert!(module.contains("call ptr @aura_task_frame_new"));
    assert!(module.contains("call i32 @aura_task_executor_submit"));
    assert!(module.contains("define i32 @aura_llvm_poll_aura_demo___spawn_"));
    assert_llvm_compiles(&module, "spawn-module");
}

#[test]
fn emits_async_call_as_a_task_handle() {
    let file = parse_file(
        "package demo\nasync fun child(): Int { return 7 }\nasync fun parent(): Int { return await child() }\n",
    )
    .unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert_llvm_compiles(&module, "async-await-module");
}

#[test]
fn emits_integer_join_as_a_result_value() {
    let file = parse_file(
        "package demo\nfun main() { val task: TaskHandle<Int> = spawn { return 23 } val result = join(task) }\n",
    )
    .unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert_llvm_compiles(&module, "join-module");
}

#[test]
fn runs_capture_free_spawn_and_join_through_the_scheduler() {
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/async/no_await.aura"),
        "scheduler-no-await",
        "async\n",
    );
}

#[test]
fn runs_integer_spawn_capture_through_the_scheduler() {
    assert_llvm_source_runs(
        "package demo\nfun main() { val value: Int = 23 val task: TaskHandle<Int> = spawn { println(value.toString()) return value } join(task) }\n",
        "scheduler-int-capture",
        "23\n",
    );
}

#[test]
fn runs_string_spawn_capture_with_frame_gc_hooks() {
    let source = "package demo\nfun main() { val value: String = \"captured\" val task: TaskHandle<Int> = spawn { println(value) return 1 } join(task) }\n";
    let file = parse_file(source).unwrap();
    let checked = check_file(&file).unwrap();
    let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
    assert!(module.contains("aura_task_frame_set_gc_stack_map"));
    assert!(module.contains("aura_llvm_stack_map_"));
    assert_llvm_source_runs(source, "scheduler-string-capture", "captured\n");
}

#[test]
fn runs_string_task_completion_and_join() {
    assert_llvm_source_runs(
        "package demo\nfun main() { val task: TaskHandle<String> = spawn { return \"result\" } join(task) }\n",
        "scheduler-string-task",
        "",
    );
}

#[test]
fn runs_string_async_function_through_await() {
    assert_llvm_source_runs(
        "package demo\nasync fun answer(): String { return \"async-result\" }\nfun main() { val task: TaskHandle<Unit> = spawn { val value: String = await answer() println(value) return } join(task) }\n",
        "scheduler-string-await",
        "async-result\n",
    );
}

#[test]
fn runs_class_spawn_capture_with_frame_ownership() {
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/async/mutable_spawn_capture.aura"),
        "scheduler-class-capture",
        "2\n2\n",
    );
}

#[test]
fn cancellation_prevents_a_queued_llvm_task_from_running() {
    assert_llvm_source_runs(
        "package demo\nfun main() { val task: TaskHandle<Int> = spawn { println(\"ran\") return 1 } cancel(task) }\n",
        "scheduler-cancel",
        "",
    );
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
fn emits_specialized_async_runtime_operations_from_mir() {
    let cases: &[(&str, &[&str])] = &[
        (
            r#"package std.task
fun spawnBlocking(body: () -> Int): TaskHandle<Int> { throw "intrinsic" }
fun main() {
  val task: TaskHandle<Int> = spawnBlocking(() => 42)
}
"#,
            &["@aura_llvm_spawn_blocking_i64"],
        ),
        (
            r#"package std.io
async fun readFd(fd: Int, capacity: Int): String { throw "intrinsic" }
async fun writeFd(fd: Int, content: String): Int { throw "intrinsic" }
fun main() {
  val read = readFd(0, 1)
  val write = writeFd(1, "x")
}
"#,
            &["@aura_llvm_io_read_fd_task", "@aura_llvm_io_write_fd_task"],
        ),
    ];
    for (source, symbols) in cases {
        let file = parse_file(source).unwrap();
        let checked = check_file(&file).unwrap();
        let program = LoweredProgram::from_checked(checked);
        assert!(
            program.mir_is_complete(),
            "{:?}",
            program.lowering_diagnostics()
        );
        let module = LlvmBackend::emit_module(&program).unwrap();
        for &symbol in *symbols {
            assert!(module.contains(symbol), "missing {symbol}");
        }
        assert_llvm_compiles(&module, "specialized-async");
    }
}

#[test]
fn emits_foreign_handle_lambda_capture_with_ownership_hooks() {
    let file = parse_file(
        r#"package demo
fun makeHandle(): ForeignHandle<Int> { throw "intrinsic" }
fun main() {
  val handle: ForeignHandle<Int> = makeHandle()
  val closure: () -> ForeignHandle<Int> = () => handle
}
"#,
    )
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(
        program.mir_is_complete(),
        "{:?}",
        program.lowering_diagnostics()
    );
    let module = LlvmBackend::emit_module(&program).unwrap();
    assert!(module.contains("aura_llvm_class_retain"));
    assert!(module.contains("aura_llvm_class_release"));
    assert_llvm_compiles(&module, "foreign-handle-lambda");
}

#[test]
fn emits_task_and_channel_lambda_capture_ownership_hooks() {
    let source = r#"package demo
fun main() {
  val task: TaskHandle<Int> = spawn { return 1 }
  val task_closure: () -> TaskHandle<Int> = () => task
  val captured_task: TaskHandle<Int> = task_closure()
  cancel(captured_task)
  val channel: Channel<Int> = Channel<Int>(1)
  val channel_closure: () -> Channel<Int> = () => channel
  val captured_channel: Channel<Int> = channel_closure()
  captured_channel.close()
}
"#;
    let file = parse_file(source).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(
        program.mir_is_complete(),
        "{:?}",
        program.lowering_diagnostics()
    );
    let module = LlvmBackend::emit_module(&program).unwrap();
    assert!(module.contains("aura_task_executor_retain_payload"));
    assert!(module.contains("aura_task_channel_retain"));
    assert!(module.contains("aura_task_executor_release_payload"));
    assert!(module.contains("aura_task_channel_destroy"));
    assert_llvm_compiles(&module, "task-channel-lambda");
    assert_llvm_source_runs(source, "task-channel-lambda-run", "");
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
        "package demo\nfun unwrap(value: String?): String { return value!! }\nfun main() { val optional: String? = \"value\" assert(optional == \"value\") println(unwrap(optional)) }\n",
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
fn runs_nullable_safe_method_calls() {
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/class/safe_call.aura"),
        "safe-call",
        "null\nhi\n",
    );
}

#[test]
fn runs_tagged_nullable_returns_from_null_and_value_paths() {
    assert_llvm_source_runs(
        r#"package demo
fun maybe(value: Int): Int? {
    if (value < 0) { return null }
    return value
}
fun main() {
    assert(maybe(-1) == null)
    assert(maybe(42)!! == 42)
    println("nullable-ok")
}
"#,
        "nullable-return-paths",
        "nullable-ok\n",
    );
}

#[test]
fn runs_nullable_primitive_reassignments() {
    assert_llvm_source_runs(
        r#"package demo
fun main() {
    var flag: Bool? = null
    flag = true
    assert(flag != null)
    var value: Int? = 5
    value = null
    assert(value == null)
    value = 42
    assert(value!! == 42)
    println("nullable-reassign-ok")
}
"#,
        "nullable-reassignments",
        "nullable-reassign-ok\n",
    );
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
fn runs_overridden_method_through_base_reference() {
    let file = parse_file(include_str!(
        "../../../../../corpus/class/override_dispatch_paths.aura"
    ))
    .unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-dispatch-{}", std::process::id()));
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
        "override-dispatch-ok\n"
    );
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

#[test]
fn runs_interface_dispatch_through_an_array() {
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/iface/dispatch_array.aura"),
        "interface-array-dispatch",
        "dispatch\n",
    );
}

#[test]
fn runs_non_capturing_lambda_function_values() {
    assert_llvm_source_runs(
        "package demo\nfun main() { val f = (x: Int) => x + 1 println(f(41).toString()) }\n",
        "lambda-basic",
        "42\n",
    );
}

#[test]
fn runs_immutable_scalar_lambda_capture() {
    assert_llvm_source_runs(
        "package demo.lambda\nfun main() { val base = 40 val add = (x: Int) => base + x println(add(2).toString()) }\n",
        "lambda-capture-int",
        "42\n",
    );
}

#[test]
fn runs_immutable_string_lambda_capture() {
    assert_llvm_source_runs(
        "package demo.lambda\nfun main() { val prefix = \"hi\" val greet = (value: String) => prefix + value println(greet(\"!\")) }\n",
        "lambda-capture-string",
        "hi!\n",
    );
}

#[test]
fn runs_immutable_class_lambda_capture() {
    assert_llvm_source_runs(
        "package demo.lambda\nclass Box(val value: Int) { }\nfun main() { val box = Box(7) val get = () => box.value println(get().toString()) }\n",
        "lambda-capture-class",
        "7\n",
    );
}

#[test]
fn runs_higher_order_lambdas_with_scalar_capture() {
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_hof.aura"),
        "lambda-hof",
        14,
    );
}

#[test]
fn runs_class_and_array_lambda_captures() {
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_capture_class.aura"),
        "lambda-capture-class-fixture",
        49,
    );
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_capture_array.aura"),
        "lambda-capture-array",
        159,
    );
}

#[test]
fn runs_nested_function_lambda_captures() {
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_capture_fun.aura"),
        "lambda-capture-fun",
        89,
    );
}

#[test]
fn runs_mutable_scalar_lambda_captures() {
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_capture_var.aura"),
        "lambda-capture-var",
        52,
    );
}

#[test]
fn runs_mutable_string_lambda_captures() {
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_capture_var_str.aura"),
        "lambda-capture-var-string",
        24,
    );
}

#[test]
fn runs_mutable_function_lambda_captures() {
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_capture_var_fun.aura"),
        "lambda-capture-var-fun",
        22,
    );
}

#[test]
fn runs_mutable_heap_lambda_captures() {
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_capture_var_array.aura"),
        "lambda-capture-var-array",
        38,
    );
    assert_llvm_source_exits(
        include_str!("../../../../../corpus/fun/lambda_capture_var_class.aura"),
        "lambda-capture-var-class",
        72,
    );
}

#[test]
fn runs_mutable_task_and_channel_lambda_captures() {
    assert_llvm_source_runs(
        "package demo\nfun main() { var task: TaskHandle<Int> = spawn { return 1 } val getTask = () => task task = spawn { return 2 } val capturedTask = getTask() cancel(capturedTask) var channel: Channel<Int> = Channel<Int>(1) val getChannel = () => channel channel = Channel<Int>(1) val capturedChannel = getChannel() capturedChannel.send(7) assert(capturedChannel.receive() == 7) capturedChannel.close() println(\"mutable-async-capture-ok\") }\n",
        "mutable-async-captures",
        "mutable-async-capture-ok\n",
    );
}

#[test]
fn runs_generic_class_methods_with_owned_array_fields() {
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/generic/array_field_return.aura"),
        "generic-array-field",
        "kept\nb2\nok\nh2k\nd99\nh3k\nf7\nalive\n",
    );
}

#[test]
fn runs_generic_free_function_and_nested_class_monomorphs() {
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/generic/id.aura"),
        "generic-id",
        "Hello, Aura\nok\n",
    );
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/generic/nested.aura"),
        "generic-nested",
        "nested\nnested\ndeep\nfrom maker\n",
    );
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/generic/nested_array_transform.aura"),
        "generic-nested-array",
        "nested-array-ok\n",
    );
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/generic/constructor_subst.aura"),
        "generic-constructor-subst",
        "generic-constructor-subst-ok\n",
    );
}

#[test]
fn runs_string_and_array_builtin_methods() {
    assert_llvm_source_runs(
        r#"package demo
fun main() {
    val value = " Hello, Aura "
    assert(value.trim() == "Hello, Aura")
    assert(value.trimStart() == "Hello, Aura ")
    assert(value.trimEnd() == " Hello, Aura")
    assert(value.toLower() == " hello, aura ")
    assert(value.toUpper() == " HELLO, AURA ")
    assert(value.startsWith(" Hello"))
    assert(value.contains("Aura"))
    assert(value.endsWith(" "))
    assert(value.indexOf("Aura") == 8)
    assert(value.charAt(1) == 72)
    assert(value.substring(1, 6) == "Hello")
    val parts = "a,b".split(",")
    assert(parts.len == 2)
    val values = Array<Int>(0)
    values.push(1)
    values.reserve(4)
    val copy = values.clone()
    values.clear()
    assert(values.isEmpty())
    assert(copy.get(0) == 1)
    println("ok")
}
"#,
        "builtins",
        "ok\n",
    );
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

#[test]
fn runs_heap_enum_payloads_with_nested_ownership() {
    assert_llvm_source_runs(
        r#"
package demo.llvm_enum_heap

enum Inner {
    case Text(value: String)
}

enum Outer {
    case Nested(value: Inner)
}

fun main() {
    val inner = Text("owned")
    val outer = Nested(inner)
    println("ok")
}
"#,
        "enum-heap-payload",
        "ok\n",
    );
}

#[test]
fn runs_enum_exception_catch_dispatch() {
    assert_llvm_source_runs(
        r#"
package demo.llvm_enum_exception

enum Problem {
    case Bad(value: Int)
}

fun main() {
    try {
        throw Bad(7)
    } catch (error: Problem) {
        println("caught")
    }
}
"#,
        "enum-exception",
        "caught\n",
    );
}

#[test]
fn runs_class_array_field_reassignment_with_ownership() {
    assert_llvm_source_runs(
        include_str!("../../../../../corpus/generic/array_field_move.aura"),
        "class-array-field",
        "moved\nh2\nok\nx0\nb1\nrep\n",
    );
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

fn assert_llvm_source_runs(source: &str, suffix: &str, expected: &str) {
    let file = parse_file(source).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-{suffix}-{}", std::process::id()));
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
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}

fn assert_llvm_source_exits(source: &str, suffix: &str, expected: i32) {
    let file = parse_file(source).unwrap();
    let checked = check_file(&file).unwrap();
    let program = LoweredProgram::from_checked(checked);
    assert!(program.mir_is_complete());
    let out = PathBuf::from(format!("/tmp/aura-llvm-{suffix}-{}", std::process::id()));
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
    assert_eq!(status.code(), Some(expected));
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(out.with_file_name(format!(
        "{}.aura.ll",
        out.file_name().unwrap().to_string_lossy()
    )));
}
