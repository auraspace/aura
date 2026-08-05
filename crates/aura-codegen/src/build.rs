//! Shell out to a C compiler.

use std::path::{Path, PathBuf};

use aura_ast::File;
use aura_ir::LoweredProgram;
use aura_sema::CheckedFile;

use crate::ctx::EmitOptions;
use crate::driver::{Artifact, CBackend, Driver};
use crate::error::CodegenError;
use crate::options::CompileOptions;

pub fn emit_c_from_ast(file: &File) -> Result<String, CodegenError> {
    Driver::new(CBackend).emit(file, EmitOptions::default())
}

pub fn emit_c_from_checked(checked: &CheckedFile) -> String {
    let program = LoweredProgram::from_checked(checked.clone());
    crate::emit::emit_c_with_program(&program, EmitOptions::default())
}

pub fn emit_c_tests_from_ast(file: &File) -> Result<String, CodegenError> {
    Driver::new(CBackend).emit(
        file,
        EmitOptions {
            test: true,
            ..Default::default()
        },
    )
}

/// Typecheck + emit C + compile with the system C compiler (`CC` or `cc`).
pub fn build_from_file(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    build_from_file_with(
        file,
        out_bin,
        runtime_c,
        CompileOptions::default(),
        EmitOptions::default(),
    )
}

pub fn build_from_checked(
    checked: &CheckedFile,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    crate::driver::build_artifact_from_checked(
        checked,
        out_bin,
        runtime_c,
        CompileOptions::default(),
        EmitOptions::default(),
    )
    .map(Artifact::into_path)
}

pub fn build_tests_from_file(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    build_from_file_with(
        file,
        out_bin,
        runtime_c,
        CompileOptions::default(),
        EmitOptions {
            test: true,
            ..Default::default()
        },
    )
}

pub fn build_tests_from_checked(
    checked: &CheckedFile,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    crate::driver::build_artifact_from_checked(
        checked,
        out_bin,
        runtime_c,
        CompileOptions::default(),
        EmitOptions {
            test: true,
            ..Default::default()
        },
    )
    .map(Artifact::into_path)
}

pub(crate) fn build_from_file_with(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
    compile_options: CompileOptions,
    opts: EmitOptions,
) -> Result<PathBuf, CodegenError> {
    Driver::new(CBackend)
        .build(file, out_bin, runtime_c, compile_options, opts)
        .map(Artifact::into_path)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        process::{Command, Stdio},
        thread,
    };

    use aura_ast::{
        AsyncExpr, AsyncFunDecl, AwaitExpr, Block, CallExpr, CancelExpr, ChannelCloseExpr,
        ChannelCreateExpr, ChannelReceiveExpr, ChannelSendExpr, Expr, File, FunDecl, Ident, IntLit,
        JoinExpr, Path, ReturnStmt, Span, SpawnExpr, Stmt, TypeRef,
    };

    use super::{build_from_file, build_from_file_with, emit_c_from_ast, emit_c_tests_from_ast};
    use crate::driver::{CBackend, Driver};
    use crate::{Backend, CompileOptions, DiagnosticMode, OutputKind, Profile, RuntimeAbi, Target};
    use aura_parser::parse_file;

    fn empty_program() -> File {
        let span = Span::new(0, 1);
        let ident = |name: &str| Ident {
            name: name.into(),
            span,
        };
        File {
            package: Path {
                segments: vec![ident("demo")],
                span,
            },
            imports: vec![],
            interfaces: vec![],
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            consts: vec![],
            functions: vec![FunDecl {
                is_pub: false,
                origin_package: String::new(),
                attributes: vec![],
                modifiers: vec![],
                visibility: aura_ast::MemberVisibility::Package,
                is_test: false,
                name: ident("main"),
                type_params: vec![],
                params: vec![],
                return_type: None,
                body: Block {
                    stmts: vec![],
                    span,
                },
                span,
            }],
            foreign_functions: vec![],
            async_functions: vec![],
            span,
        }
    }

    fn copy_runtime_fixture(
        root: &std::path::Path,
        stem: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let runtime_root = std::env::temp_dir().join(format!("{stem}-runtime"));
        fn copy_dir(source: &std::path::Path, destination: &std::path::Path) {
            fs::create_dir_all(destination).expect("create runtime fixture directory");
            for entry in fs::read_dir(source).expect("read runtime directory") {
                let entry = entry.expect("runtime entry");
                let source_path = entry.path();
                let destination_path = destination.join(entry.file_name());
                if source_path.is_dir() {
                    copy_dir(&source_path, &destination_path);
                } else {
                    fs::copy(&source_path, &destination_path).expect("copy runtime module");
                }
            }
        }

        fs::create_dir_all(&runtime_root).expect("create runtime fixture");
        fs::copy(
            root.join("runtime/runtime.c"),
            runtime_root.join("runtime.c"),
        )
        .expect("copy runtime entrypoint");
        fs::copy(
            root.join("runtime/aura_ffi.h"),
            runtime_root.join("aura_ffi.h"),
        )
        .expect("copy runtime header");
        copy_dir(&root.join("runtime/src"), &runtime_root.join("src"));
        (runtime_root.join("runtime.c"), runtime_root)
    }

    #[test]
    fn legacy_builds_use_current_compile_defaults() {
        let options = CompileOptions::default();

        assert_eq!(options.backend, Backend::C);
        assert_eq!(options.target, Target::Native);
        assert_eq!(options.profile, Profile::Debug);
        assert_eq!(options.runtime_abi, Some(RuntimeAbi::AuraRtC));
        assert_eq!(options.output, OutputKind::Executable);
    }

    #[test]
    fn release_build_embeds_runtime_and_runs_as_single_executable() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-release-link-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        let options = CompileOptions::builder()
            .backend(Backend::C)
            .target(Target::Native)
            .profile(Profile::Release)
            .runtime_abi(RuntimeAbi::AuraRtC)
            .output(OutputKind::Executable)
            .diagnostics(DiagnosticMode::Human)
            .build()
            .expect("complete release options");

        build_from_file_with(
            &empty_program(),
            &bin,
            &root.join("runtime/runtime.c"),
            options,
            crate::ctx::EmitOptions::default(),
        )
        .expect("link release executable with embedded runtime");
        let output = Command::new(&bin).output().expect("run release executable");
        assert!(
            output.status.success(),
            "release executable failed: {output:?}"
        );

        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn supported_profiles_rebuild_reproducibly_on_native_host() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let runtime = root.join("runtime/runtime.c");
        let dir = std::env::temp_dir();

        for profile in [
            Profile::Debug,
            Profile::Dev,
            Profile::Test,
            Profile::Release,
        ] {
            let stem = format!("aura-matrix-{}-{}", profile.name(), std::process::id());
            let bin = dir.join(&stem);
            let generated_c = dir.join(format!("{stem}.aura.c"));
            let options = CompileOptions::builder()
                .backend(Backend::C)
                .target(Target::Native)
                .profile(profile)
                .runtime_abi(RuntimeAbi::AuraRtC)
                .output(OutputKind::Executable)
                .diagnostics(DiagnosticMode::Human)
                .build()
                .expect("complete matrix options");

            let first = Driver::new(CBackend)
                .build(
                    &empty_program(),
                    &bin,
                    &runtime,
                    options.clone(),
                    crate::ctx::EmitOptions::default(),
                )
                .expect("cold matrix build");
            let first_bytes = fs::read(first.path()).expect("read first artifact");
            let _ = fs::remove_file(&bin);
            let _ = fs::remove_file(&generated_c);
            let second = Driver::new(CBackend)
                .build(
                    &empty_program(),
                    &bin,
                    &runtime,
                    options,
                    crate::ctx::EmitOptions::default(),
                )
                .expect("warm matrix build");
            assert_eq!(first.identity(), second.identity());
            let second_bytes = fs::read(second.path()).expect("read second artifact");
            if cfg!(target_os = "macos") {
                // Apple linkers may vary Mach-O metadata between equivalent builds.
                assert_eq!(first_bytes.len(), second_bytes.len());
            } else {
                assert_eq!(first_bytes, second_bytes);
            }
            assert!(Command::new(second.path())
                .status()
                .expect("run matrix artifact")
                .success());

            for path in [bin, generated_c] {
                let _ = fs::remove_file(path);
            }
        }
    }

    #[test]
    fn mismatched_runtime_abi_stops_before_generated_main() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-abi-mismatch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        let (runtime, runtime_root) = copy_runtime_fixture(root, &stem);
        let module = runtime_root.join("src/ffi/abi_race.c");
        let source = fs::read_to_string(&module).expect("read ABI module");
        let mismatched = source.replace(
            "#define AURA_RT_ABI_VERSION 1u",
            "#define AURA_RT_ABI_VERSION 2u",
        );
        assert_ne!(source, mismatched, "test must change runtime ABI version");
        fs::write(&module, mismatched).expect("write mismatched runtime");

        build_from_file(&empty_program(), &bin, &runtime).expect("compile mismatched artifact");
        let output = Command::new(&bin)
            .output()
            .expect("run mismatched artifact");
        assert_eq!(output.status.code(), Some(78));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("expected version 1"), "{stderr}");
        assert!(stderr.contains("available version 2"), "{stderr}");
        assert!(
            output.stdout.is_empty(),
            "user code must not run: {output:?}"
        );

        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_dir_all(runtime_root);
    }

    #[test]
    fn mismatched_runtime_ffi_abi_stops_before_generated_main() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-ffi-abi-mismatch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        let (runtime, runtime_root) = copy_runtime_fixture(root, &stem);
        let module = runtime_root.join("src/ffi/abi_race.c");
        let source = fs::read_to_string(&module).expect("read ABI module");
        let mismatched = source.replace(
            "aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1",
            "aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=2;type=1",
        );
        assert_ne!(source, mismatched, "test must change the FFI ABI identity");
        fs::write(&module, mismatched).expect("write mismatched runtime");

        build_from_file(&empty_program(), &bin, &runtime).expect("compile mismatched artifact");
        let output = Command::new(&bin)
            .output()
            .expect("run mismatched artifact");
        assert_eq!(output.status.code(), Some(78));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("expected version 1"), "{stderr}");
        assert!(stderr.contains("ffi=1"), "{stderr}");
        assert!(stderr.contains("ffi=2"), "{stderr}");
        assert!(
            output.stdout.is_empty(),
            "user code must not run: {output:?}"
        );

        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_dir_all(runtime_root);
    }

    #[test]
    fn invalid_linker_option_surfaces_before_false_executable() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-ffi-linker-failure-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        let runtime = root.join("runtime/runtime.c");
        let _ = fs::remove_file(&bin);
        let _ = fs::remove_file(&generated_c);

        // Keep the existing C backend/linker boundary, but request a linker
        // flavor that cannot exist. The driver must return a compile error and
        // must not report an Artifact for a path that was never linked.
        let mut options = CompileOptions::default();
        options.profile_settings.linker =
            Some(format!("aura-missing-linker-{}", std::process::id()));
        let error = build_from_file_with(
            &empty_program(),
            &bin,
            &runtime,
            options,
            crate::ctx::EmitOptions::default(),
        )
        .expect_err("missing linker must fail the build");

        match error {
            crate::error::CodegenError::Compile(message) => {
                assert!(message.contains("failed with status"), "{message}");
                assert!(
                    message.contains(&generated_c.display().to_string()),
                    "{message}"
                );
            }
            other => panic!("expected deterministic linker compile error, got {other:?}"),
        }
        assert!(
            !bin.exists(),
            "failed linker must not leave a false executable at {}",
            bin.display()
        );
        assert!(
            generated_c.exists(),
            "the emitted C is the diagnostic source"
        );

        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn native_ffi_primitive_fixture_calls_and_static_links() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir().join(format!("aura-ffi-primitives-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create fixture directory");
        let object = dir.join("ffi_primitives.o");
        let archive = dir.join("libaura_ffi_primitives.a");
        let bin = dir.join("program");
        let generated_c = dir.join("program.aura.c");
        let fixture = root.join("crates/aura-codegen/fixtures/ffi_primitives.c");

        let compile_fixture = Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
            .arg(&fixture)
            .arg("-o")
            .arg(&object)
            .status()
            .expect("spawn fixture compiler");
        assert!(compile_fixture.success(), "fixture compile failed");
        let archive_status = Command::new("ar")
            .args(["rcs"])
            .arg(&archive)
            .arg(&object)
            .status()
            .expect("spawn archive tool");
        assert!(archive_status.success(), "fixture archive failed");

        let source = r#"package demo
@foreign(library = "aura_ffi_primitives", target = "native", link = "static", abi = 1, abi_id = "c")
extern "C" fun aura_ffi_add(value: Int): Int
@foreign(library = "aura_ffi_primitives", target = "native", link = "static", abi = 1, abi_id = "c")
extern "C" fun aura_ffi_enabled(): Bool
@foreign(library = "aura_ffi_primitives", target = "native", link = "static", abi = 1, abi_id = "c")
extern "C" fun aura_ffi_label(): String
@foreign(library = "aura_ffi_primitives", target = "native", link = "static", abi = 1, abi_id = "c")
extern "C" fun aura_ffi_touch(value: String): Unit
@foreign(library = "aura_ffi_primitives", target = "native", link = "static", abi = 1, abi_id = "c", failure = "status")
extern "C" fun aura_ffi_status(value: Int): Int
fun main() {
  val sum = aura_ffi_add(41)
  println(sum.toString())
  if (aura_ffi_enabled()) { println(aura_ffi_label()) }
  aura_ffi_touch("borrowed")
  println(aura_ffi_status(99).toString())
}
"#;
        let file = parse_file(source).expect("parse F2 fixture");
        let options = CompileOptions::builder()
            .backend(Backend::C)
            .target(Target::Native)
            .profile(Profile::Release)
            .runtime_abi(RuntimeAbi::AuraRtC)
            .output(OutputKind::Executable)
            .diagnostics(DiagnosticMode::Human)
            .foreign_library_path(&dir)
            .build()
            .expect("complete F2 options");
        build_from_file_with(
            &file,
            &bin,
            &root.join("runtime/runtime.c"),
            options,
            crate::ctx::EmitOptions::default(),
        )
        .expect("link F2 fixture");
        let generated = fs::read_to_string(&generated_c).expect("read generated F2 C");
        assert!(generated.contains("extern int64_t aura_ffi_add(int64_t);"));
        assert!(generated.contains("aura_ffi_add(INT64_C(41))"));
        assert!(generated.contains("aura_ffi_map_error((int32_t)(aura_ffi_status(INT64_C(99))))"));
        assert!(!generated.contains("aura_fn_aura_ffi_add"));
        let output = Command::new(&bin).output().expect("run F2 fixture");
        assert!(output.status.success(), "F2 fixture failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "42\nffi-borrowed\n7\n"
        );

        for path in [bin, generated_c, object, archive] {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn builds_and_runs_no_await_async_function() {
        let span = Span::new(0, 1);
        let ident = |name: &str| Ident {
            name: name.into(),
            span,
        };
        let int_ty = || TypeRef {
            qualifier: None,
            name: ident("Int"),
            type_args: vec![],
            nullable: false,
            reference: false,
            span,
            fun: None,
        };
        let async_fun = AsyncFunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: vec![],
            is_test: false,
            name: ident("answer"),
            type_params: vec![],
            params: vec![],
            return_type: Some(int_ty()),
            body: Block {
                stmts: vec![Stmt::Return(ReturnStmt {
                    value: Some(Expr::Int(IntLit { value: 42, span })),
                    span,
                })],
                span,
            },
            span,
        };
        let main_fun = FunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: vec![],
            modifiers: vec![],
            visibility: aura_ast::MemberVisibility::Package,
            is_test: false,
            name: ident("main"),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Expr(Expr::Call(CallExpr {
                    callee: Box::new(Expr::Ident(ident("answer"))),
                    type_args: vec![],
                    args: vec![],
                    span,
                }))],
                span,
            },
            span,
        };
        let file = File {
            package: Path {
                segments: vec![ident("demo")],
                span,
            },
            imports: vec![],
            interfaces: vec![],
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            consts: vec![],
            functions: vec![main_fun],
            foreign_functions: vec![],
            async_functions: vec![async_fun],
            span,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-c22l-{}", std::process::id()));
        let generated_c = dir.join(format!("aura-c22l-{}.aura.c", std::process::id()));
        let runtime = root.join("runtime/runtime.c");
        build_from_file(&file, &bin, &runtime).expect("compile generated async C");
        let generated = std::fs::read_to_string(&generated_c).expect("read generated async C");
        assert!(generated.contains("switch (aura_task_frame_resume_state(frame))"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 1)"));
        let status = Command::new(&bin).status().expect("run generated binary");
        assert!(status.success());
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_no_await_scheduler_payload_results() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun relay(input: TaskHandle<Int>): TaskHandle<Int> { return input }
async fun make_channel(channel: Channel<Int>): Channel<Int> { return channel }
fun main() {
  val source: TaskHandle<Int> = spawn { return 23 }
  relay(source)
  val source_channel: Channel<Int> = Channel<Int>(1)
  make_channel(source_channel)
  gc_collect()
}
"#,
        )
        .expect("parse no-await scheduler payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit no-await scheduler payload fixture");
        assert!(generated.contains("aura_task_executor_retain_payload"));
        assert!(generated.contains("aura_task_channel_retain"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-no-await-scheduler-payload-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        let generated = emit_c_from_ast(&file).expect("emit payload enum std.json encode fixture");
        assert!(
            generated.contains("aura_json_encode_variant"),
            "payload enum encoder was not emitted"
        );
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile no-await scheduler payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run no-await scheduler payload fixture");
        assert!(
            output.status.success(),
            "no-await scheduler payload failed: {output:?}"
        );
        assert!(output.stdout.is_empty());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_scheduler_payload_result() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun ready(): Int { return 1 }
async fun relay(input: TaskHandle<Int>): TaskHandle<Int> {
  var saved: TaskHandle<Int> = input
  if (true) {
    val value: Int = await ready()
    println(value.toString())
  }
  return saved
}
fun main() {
  val source: TaskHandle<Int> = spawn { return 29 }
  relay(source)
  gc_collect()
}
"#,
        )
        .expect("parse general CFG scheduler payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG scheduler payload fixture");
        assert!(generated.contains("aura async general CFG"));
        assert!(generated.contains("aura_task_executor_retain_payload"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-scheduler-payload-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG scheduler payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG scheduler payload fixture");
        assert!(
            output.status.success(),
            "general CFG scheduler payload failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_generic_join_wrapper_with_concrete_payload() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun joinTask<T>(task: TaskHandle<T>): Result<T, TaskError> { return join(task) }
fun main() {
  val task: TaskHandle<Int> = spawn { return 42 }
  val result: Result<Int, TaskError> = joinTask(task)
  match (result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse generic join wrapper fixture");
        let generated = emit_c_from_ast(&file).expect("emit generic join wrapper fixture");
        assert!(generated.contains("aura_enum_std_io_Result_Int_std_io_TaskError"));
        assert!(!generated.contains("aura_enum_std_io_Result_T_std_io_TaskError"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-generic-join-wrapper-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic join wrapper fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic join wrapper fixture");
        assert!(
            output.status.success(),
            "generic join wrapper failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn no_await_async_frame_roots_non_this_class_parameter_until_poll() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Box(var value: Int) {}
async fun read(box: Box): Int {
  gc_collect()
  return box.value
}
fun launch(): TaskHandle<Int> {
  val box: Box = Box(41)
  return spawn {
    val value: Int = await read(box)
    return value
  }
}
fun main() {
  val task = launch()
  gc_collect()
  val result: Result<Int, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse no-await class parameter frame fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit no-await class parameter frame fixture");
        assert!(generated.contains("aura_task_frame_set_gc_mark(frame"));
        assert!(generated.contains("aura_gc_add_root((void **)&data->box)"));
        assert!(generated.contains("aura_gc_remove_root((void **)&data->box)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-no-await-class-frame-root-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile no-await class parameter frame fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run no-await class parameter frame fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "41\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_task_join_with_owned_struct_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
struct Packet(val text: String, val code: Int) {}
async fun produce(): Packet { return Packet("ready", 17) }
fun main() {
  val captured: Packet = Packet("captured", 23)
  val task = spawn {
    val value: Packet = await produce()
    println(captured.text)
    return value
  }
  val outcome: Result<Packet, TaskError> = join(task)
  match (outcome) {
    case Ok(value) => {
      println(value.text)
      println(value.code.toString())
    }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse struct task outcome fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-task-struct-outcome-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile struct task outcome fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run struct task outcome fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "captured\nready\n17\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_task_join_with_struct_heap_class_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Child(var value: Int) {}
struct Packet(val child: Child, val text: String) {}
async fun produce(): Packet { return Packet(Child(41), "ready") }
fun main() {
  val task = spawn { val value: Packet = await produce() return value }
  gc_collect()
  val first: Result<Packet, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.child.value.toString()) println(value.text) }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Packet, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.child.value.toString()) println(value.text) }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse struct heap-class task outcome fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit struct heap-class task outcome fixture");
        assert!(generated.contains("aura_gc_mark_ptr((void *)value->child)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-task-struct-class-outcome-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile struct heap-class task outcome fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run struct heap-class task outcome fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "41\nready\n41\nready\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_task_join_with_nested_enum_struct_class_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Child(var value: Int) {}
struct Payload(val child: Child) {}
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Envelope { case Wrapped(value: Payload) }
async fun produce(): Envelope { return Wrapped(Payload(Child(73))) }
fun main() {
  val task = spawn { val value: Envelope = await produce() return value }
  gc_collect()
  val first: Result<Envelope, TaskError> = join(task)
  match (first) {
    case Ok(value) => { match (value) { case Wrapped(packet) => { println(packet.child.value.toString()) } } }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Envelope, TaskError> = join(task)
  match (second) {
    case Ok(value) => { match (value) { case Wrapped(packet) => { println(packet.child.value.toString()) } } }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse nested enum-struct-class task outcome fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit nested enum-struct-class task outcome fixture");
        assert!(generated.contains("_Payload_mark(&value->data.Wrapped.value)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-task-nested-enum-struct-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested enum-struct-class task outcome fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested enum-struct-class task outcome fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_join_with_mixed_generic_enum_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Child(var value: Int) {}
enum Box<T> { case Text(value: String) case Item(value: T) }
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun produce(): Box<Child> { return Item(Child(91)) }
fun printBox(value: Box<Child>) {
  match (value) {
    case Text(text) => { println(text) }
    case Item(child) => { println(child.value.toString()) }
  }
}
fun main() {
  val task = spawn { val value: Box<Child> = await produce() return value }
  val first: Result<Box<Child>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { printBox(value) }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Box<Child>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { printBox(value) }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse mixed generic enum task outcome fixture");
        emit_c_from_ast(&file).expect("emit mixed generic enum task outcome fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-task-mixed-generic-enum-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mixed generic enum task outcome fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run mixed generic enum task outcome fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "91\n91\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_join_with_optional_primitive_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun produce(value: Int?): Int? { return value }
fun printResult(result: Result<Int?, TaskError>) {
  match (result) {
    case Ok(value) => {
      if (value == null) { println("none") } else { println(value!!.toString()) }
    }
    case Err(error) => { println("failed") }
  }
}
fun main() {
  val task = spawn { val value: Int? = await produce(73) return value }
  val first: Result<Int?, TaskError> = join(task)
  printResult(first)
  gc_collect()
  val second: Result<Int?, TaskError> = join(task)
  printResult(second)
}
"#,
        )
        .expect("parse optional primitive task outcome fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-task-optional-outcome-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile optional primitive task outcome fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run optional primitive task outcome fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_join_with_nullable_class_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Box(val value: Int) {}
async fun tick(): Unit { }
async fun produce(value: Box?): Box? { await tick() return value }
fun printResult(result: Result<Box?, TaskError>) {
  match (result) {
    case Ok(value) => {
      if (value == null) { println("none") } else { println(value!!.value.toString()) }
    }
    case Err(error) => { println("failed") }
  }
}
fun main() {
  val task = spawn { val value: Box? = await produce(Box(73)) return value }
  val first: Result<Box?, TaskError> = join(task)
  printResult(first)
  gc_collect()
  val second: Result<Box?, TaskError> = join(task)
  printResult(second)
}
"#,
        )
        .expect("parse nullable class task outcome fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-task-nullable-class-outcome-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nullable class task outcome fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nullable class task outcome fixture");
        assert!(
            output.status.success(),
            "nullable class outcome failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_join_with_nullable_array_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun produce(): Array<Int>? {
  val values: Array<Int> = Array<Int>(0)
  values.push(73)
  return values
}
fun main() {
  val task = spawn { val value: Array<Int>? = await produce() return value }
  val first: Result<Array<Int>?, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value!!.get(0).toString()) }
    case Err(error) => { println("first-error") }
  }
  gc_collect()
  val second: Result<Array<Int>?, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value!!.get(0).toString()) }
    case Err(error) => { println("second-error") }
  }
}
"#,
        )
        .expect("parse nullable array task outcome fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-task-nullable-array-outcome-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nullable array task outcome fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nullable array task outcome fixture");
        assert!(
            output.status.success(),
            "nullable array outcome failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn gc_marks_nested_heap_class_fields_while_async_capture_is_live() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Child(var value: Int) {}
class Parent(var child: Child) {}
fun launch(): TaskHandle<Int> {
  val parent: Parent = Parent(Child(41))
  return spawn {
    gc_collect()
    return parent.child.value
  }
}
fun main() {
  val task = launch()
  gc_collect()
  val result: Result<Int, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse nested class async capture GC fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested class async capture GC fixture");
        assert!(generated.contains("aura_gc_alloc_typed"));
        assert!(generated.contains("aura_gc_mark_ptr((void *)self->child)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-class-async-capture-gc-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested class async capture GC fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested class async capture GC fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "41\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_compiler_generated_async_read_fd() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun readFd(fd: Int, capacity: Int): String { return "" }
fun main() {
  val task = spawn {
    val value: String = await readFd(0, 1)
    return value
  }
  gc_collect()
  val outcome: Result<String, TaskError> = join(task)
  match (outcome) {
    case Ok(value) => { println(value!!) }
    case Err(error) => { println("failed") }
  }
  val cancelled = spawn {
    val value: String = await readFd(0, 1)
    return value
  }
  cancel(cancelled)
  gc_collect()
}
"#,
        )
        .expect("parse generated async readFd fixture");
        let generated = emit_c_from_ast(&file).expect("emit generated async readFd fixture");
        assert!(generated.contains("compiler-generated std.io.readFd"));
        assert!(generated.contains("aura_task_frame_wait_fd(frame"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 1)"));
        assert!(generated.contains("aura_io_read_fd"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-read-fd-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generated async readFd fixture");
        let mut child = Command::new(&bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn generated async readFd fixture");
        child
            .stdin
            .take()
            .expect("readFd stdin")
            .write_all(b"A")
            .expect("write readFd input");
        let output = child
            .wait_with_output()
            .expect("wait generated async readFd fixture");
        assert!(output.status.success(), "readFd fixture failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "A\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_compiler_generated_async_read_file_foreign_handle() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun readFile(file: ForeignHandle<Int>, capacity: Int): String { return "" }
fun main() {}
"#,
        )
        .expect("parse generated async readFile fixture");
        let generated = emit_c_from_ast(&file).expect("emit generated async readFile fixture");
        assert!(generated.contains("compiler-generated std.io.readFile"));
        assert!(generated.contains("aura_ffi_handle_pin_for_boundary"));
        assert!(generated.contains("aura_file_read"));
        assert!(generated.contains("aura_task_frame_wait_file"));
        assert!(generated.contains("aura_ffi_handle_unpin(&data->pin)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-read-file-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generated async readFile fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_owned_std_io_file_handle_and_releases_it_lexically() {
        let path = format!("/tmp/aura-owned-file-{}", std::process::id());
        let file = aura_parser::parse_file(
            &r#"package std.io
fun openFile(path: String, mode: Int): ForeignHandle<Int> { throw "intrinsic" }
async fun readFile(file: ForeignHandle<Int>, capacity: Int): String { return "" }
fun main() {
  val file: ForeignHandle<Int> = openFile("/tmp/aura-owned-file", 1)
  gc_collect()
}
"#
            .replace("/tmp/aura-owned-file", &path),
        )
        .expect("parse owned std.io file fixture");
        let generated = emit_c_from_ast(&file).expect("emit owned std.io file fixture");
        assert!(generated.contains("aura_file_open"));
        assert!(generated.contains("aura_ffi_handle_new"));
        assert!(generated.contains("aura_destroy_file_resource"));
        assert!(generated.contains("aura_ffi_handle_drop(&file)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-owned-file-handle-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile owned std.io file fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run owned std.io file fixture");
        assert!(
            output.status.success(),
            "owned file fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_and_runs_spawn_capture_of_owned_file_handle() {
        let path = format!("/tmp/aura-spawn-file-handle-{}-data", std::process::id());
        let file = aura_parser::parse_file(
            &r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun openFile(path: String, mode: Int): ForeignHandle<Int> { throw "intrinsic" }
async fun writeFile(file: ForeignHandle<Int>, content: String): Int { return 0 }
fun readFile(path: String): String { return "" }
fun main() {
  val output: ForeignHandle<Int> = openFile("/tmp/aura-spawn-file-handle", 1)
  val task = spawn {
    val written: Int = await writeFile(output, "alpha-io")
    return written
  }
  val outcome: Result<Int, TaskError> = join(task)
  match (outcome) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  println(readFile("/tmp/aura-spawn-file-handle"))
}
"#
            .replace("/tmp/aura-spawn-file-handle", &path),
        )
        .expect("parse spawn file-handle fixture");
        let generated = emit_c_from_ast(&file).expect("emit spawn file-handle fixture");
        assert!(generated.contains("aura_ffi_handle_retain(__spawn_data->output)"));
        assert!(generated.contains("aura_ffi_handle_drop(&data->output)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-file-handle-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile spawn file-handle fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run spawn file-handle fixture");
        assert!(
            output.status.success(),
            "spawn file-handle fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "8\nalpha-io\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_and_runs_async_file_round_trip_with_repeated_joins_and_gc() {
        let path = format!(
            "/tmp/aura-async-file-round-trip-{}-data",
            std::process::id()
        );
        let source = format!(
            r#"package std.io
enum TaskError {{ case Failed(error: String) case Cancelled }}
enum Result<T, E> {{ case Ok(value: T) case Err(error: E) }}
fun openFile(path: String, mode: Int): ForeignHandle<Int> {{ throw "intrinsic" }}
async fun writeFile(file: ForeignHandle<Int>, content: String): Int {{ return 0 }}
async fun readFile(file: ForeignHandle<Int>, capacity: Int): String {{ return "" }}
async fun writeThrough(file: ForeignHandle<Int>): Int {{
  val count: Int = await writeFile(file, "round-trip")
  return count
}}
fun main() {{
  val output: ForeignHandle<Int> = openFile("{path}", 1)
  val writer = spawn {{
    val count: Int = await writeThrough(output)
    return count
  }}
  gc_collect()
  val written: Result<Int, TaskError> = join(writer)
  match (written) {{
    case Ok(value) => {{ println(value.toString()) }}
    case Err(error) => {{ println("write-failed") }}
  }}
  val written_again: Result<Int, TaskError> = join(writer)
  match (written_again) {{
    case Ok(value) => {{ println(value.toString()) }}
    case Err(error) => {{ println("write-failed") }}
  }}
  val input: ForeignHandle<Int> = openFile("{path}", 0)
  val reader = spawn {{
    val value: String = await readFile(input, 64)
    return value
  }}
  gc_collect()
  val read: Result<String, TaskError> = join(reader)
  match (read) {{
    case Ok(value) => {{ println(value) }}
    case Err(error) => {{ println("read-failed") }}
  }}
  val read_again: Result<String, TaskError> = join(reader)
  match (read_again) {{
    case Ok(value) => {{ println(value) }}
    case Err(error) => {{ println("read-failed") }}
  }}
  val cancelled = spawn {{
    val value: String = await readFile(input, 64)
    return value
  }}
  cancel(cancelled)
  gc_collect()
}}
"#
        );
        let file = aura_parser::parse_file(&source).expect("parse async file round-trip fixture");
        let generated = emit_c_from_ast(&file).expect("emit async file round-trip fixture");
        assert!(generated.contains("compiler-generated std.io.readFile"));
        assert!(generated.contains("compiler-generated std.io.writeFile"));
        assert!(generated.contains("aura_ffi_handle_retain(data->file)"));
        assert!(generated.contains("aura_ffi_handle_pin_for_boundary"));
        assert!(generated.contains("aura_task_frame_wait_file"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-file-round-trip-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async file round-trip fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async file round-trip fixture");
        assert!(
            output.status.success(),
            "async file round-trip fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "10\n10\nround-trip\nround-trip\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_and_runs_caller_owned_file_task_through_two_async_io_awaits() {
        let path = format!(
            "/tmp/aura-caller-owned-file-task-{}-data",
            std::process::id()
        );
        let source = format!(
            r#"package std.io
enum TaskError {{ case Failed(error: String) case Cancelled }}
enum Result<T, E> {{ case Ok(value: T) case Err(error: E) }}
fun openFile(path: String, mode: Int): ForeignHandle<Int> {{ throw "intrinsic" }}
async fun writeFile(file: ForeignHandle<Int>, content: String): Int {{ return 0 }}
async fun readFile(file: ForeignHandle<Int>, capacity: Int): String {{ return "" }}
async fun produce(file: ForeignHandle<Int>): ForeignHandle<Int> {{
  return file
}}
async fun readThrough(file_task: Task<ForeignHandle<Int>>): String {{
  var file: ForeignHandle<Int> = openFile("{path}", 0)
  var i: Int = 0
  while (i < 1) {{
    val received: ForeignHandle<Int> = await file_task
    file = received
    i = i + 1
  }}
  val value: String = await readFile(file, 64)
  gc_collect()
  return value
}}
fun main() {{
  val output: ForeignHandle<Int> = openFile("{path}", 1)
  val writer = spawn {{
    val count: Int = await writeFile(output, "caller-owned")
    return count
  }}
  val written: Result<Int, TaskError> = join(writer)
  val input: ForeignHandle<Int> = openFile("{path}", 0)
  val task = spawn {{
    val value: String = await readThrough(produce(input))
    return value
  }}
  gc_collect()
  val first: Result<String, TaskError> = join(task)
  match (first) {{
    case Ok(value) => {{ println(value) }}
    case Err(error) => {{ println("failed") }}
  }}
  val second: Result<String, TaskError> = join(task)
  match (second) {{
    case Ok(value) => {{ println(value) }}
    case Err(error) => {{ println("failed-repeat") }}
  }}
}}
"#
        );
        let file = parse_file(&source).expect("parse caller-owned file task fixture");
        let generated = emit_c_from_ast(&file).expect("emit caller-owned file task fixture");
        assert!(generated.contains("aura async general CFG String lowering"));
        assert!(generated.contains("compiler-generated std.io.readFile"));
        assert!(generated.contains("compiler-generated std.io.writeFile"));
        assert!(generated.contains("data->await_task_owned = false"));
        assert!(generated.contains("aura_ffi_handle_pin_for_boundary"));
        assert!(generated.contains("aura_ffi_handle_drop(&file)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-caller-owned-file-task-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile caller-owned file task fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run caller-owned file task fixture");
        assert!(
            output.status.success(),
            "caller-owned file task fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "caller-owned\ncaller-owned\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn compiles_compiler_generated_async_write_file_foreign_handle() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun writeFile(file: ForeignHandle<Int>, content: String): Int { return 0 }
fun main() {}
"#,
        )
        .expect("parse generated async writeFile fixture");
        let generated = emit_c_from_ast(&file).expect("emit generated async writeFile fixture");
        assert!(generated.contains("compiler-generated std.io.writeFile"));
        assert!(generated.contains("aura_ffi_handle_pin_for_boundary"));
        assert!(generated.contains("aura_file_write"));
        assert!(generated.contains("data->offset"));
        assert!(generated.contains("aura_ffi_handle_unpin(&data->pin)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-write-file-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generated async writeFile fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_compiler_generated_async_tcp_stream_foreign_handle() {
        let file = aura_parser::parse_file(
            r#"package std.net
async fun readStream(stream: ForeignHandle<Int>, capacity: Int): String { return "" }
async fun writeStream(stream: ForeignHandle<Int>, content: String): Int { return 0 }
fun main() {}
"#,
        )
        .expect("parse generated async TCP stream fixture");
        let generated = emit_c_from_ast(&file).expect("emit generated async TCP stream fixture");
        assert!(generated.contains("compiler-generated std.net.readStream"));
        assert!(generated.contains("compiler-generated std.net.writeStream"));
        assert!(generated.contains("aura_tcp_stream_read"));
        assert!(generated.contains("aura_tcp_stream_write"));
        assert!(generated.contains("aura_task_frame_wait_tcp_stream"));
        assert!(generated.contains("aura_ffi_handle_unpin(&data->pin)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-tcp-stream-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generated async TCP stream fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_async_unit_await_with_control_flow() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun pause(): Unit {}
async fun handler(ok: Bool): Unit {
  await pause()
  if (ok) { println("ready") } else { println("not-ready") }
}
fun main() {
  handler(true)
}
"#,
        )
        .expect("parse async Unit control-flow fixture");
        let generated = emit_c_from_ast(&file).expect("emit async Unit control-flow fixture");
        assert!(generated.contains("aura async general CFG Unit lowering"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-unit-control-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async Unit control-flow fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_cfg_with_incompatible_nested_shadowing() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun text(): String { return "inner" }
async fun choose(flag: Bool): Int {
  val value: Int = 7
  if (flag) {
    val value: String = await text()
    println(value)
    return value.len
  }
  return value
}
fun main() {
  val first = spawn { val ignored: Int = await choose(true) return }
  val second = spawn { val ignored: Int = await choose(false) return }
  join(first)
  join(second)
}
"#,
        )
        .expect("parse async incompatible shadowing fixture");
        let generated = emit_c_from_ast(&file).expect("emit async incompatible shadowing fixture");
        assert!(generated.contains("__aura_shadow_"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-shadowing-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async incompatible shadowing fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async incompatible shadowing fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "inner\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_std_net_connect_to_owned_tcp_foreign_handle() {
        let file = aura_parser::parse_file(
            r#"package std.net
fun listen(endpoint: String): ForeignHandle<Int> { throw "intrinsic" }
async fun accept(listener: ForeignHandle<Int>): ForeignHandle<Int> { throw "intrinsic" }
fun closeListener(listener: ForeignHandle<Int>): Bool { throw "intrinsic" }
fun closeStream(stream: ForeignHandle<Int>): Bool { throw "intrinsic" }
fun connect(endpoint: String, timeout: Int): ForeignHandle<Int> { throw "intrinsic" }
async fun readStream(stream: ForeignHandle<Int>, capacity: Int): String { return "" }
fun main() {}
"#,
        )
        .expect("parse std.net.connect fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.net.connect fixture");
        assert!(generated.contains("aura_tcp_listener_bind"));
        assert!(generated.contains("aura_destroy_tcp_listener_resource"));
        assert!(generated.contains("aura_tcp_listener_accept"));
        assert!(generated.contains("aura_task_frame_wait_tcp_listener"));
        assert!(generated.contains("aura_tcp_listener_close"));
        assert!(generated.contains("aura_tcp_stream_close"));
        assert!(generated.contains("aura_tcp_stream_connect"));
        assert!(generated.contains("aura_destroy_tcp_stream_resource"));
        assert!(generated.contains("aura_ffi_handle_new((void *)__stream"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-net-connect-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.net.connect fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_net_connect_write_close_flow() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback test server");
        let port = listener.local_addr().expect("read loopback port").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept Aura std.net client");
            let mut payload = [0_u8; 2];
            stream
                .read_exact(&mut payload)
                .expect("read Aura std.net payload");
            assert_eq!(&payload, b"g5");
        });

        let source = format!(
            r#"package std.net
fun connect(endpoint: String, timeout: Int): ForeignHandle<Int> {{ throw "intrinsic" }}
fun closeStream(stream: ForeignHandle<Int>): Bool {{ throw "intrinsic" }}
async fun writeStream(stream: ForeignHandle<Int>, content: String): Int {{ return 0 }}
fun main() {{
  val task = spawn {{
    val stream: ForeignHandle<Int> = connect("127.0.0.1:{port}", 1000)
    val sent: Int = await writeStream(stream, "g5")
    closeStream(stream)
  }}
  join(task)
}}
"#
        );
        let file = aura_parser::parse_file(&source).expect("parse std.net runtime flow");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-net-runtime-flow-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.net runtime flow");
        let output = Command::new(&bin)
            .output()
            .expect("run std.net runtime flow");
        assert!(
            output.status.success(),
            "std.net runtime flow failed: {output:?}"
        );
        server.join().expect("join loopback test server");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_net_listen_close_flow() {
        let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
        let port = reservation
            .local_addr()
            .expect("read reserved loopback port")
            .port();
        drop(reservation);

        let source = format!(
            r#"package std.net
fun listen(endpoint: String): ForeignHandle<Int> {{ throw "intrinsic" }}
fun closeListener(listener: ForeignHandle<Int>): Bool {{ throw "intrinsic" }}
fun main() {{
  val listener: ForeignHandle<Int> = listen("127.0.0.1:{port}")
  closeListener(listener)
}}
"#
        );
        let file = aura_parser::parse_file(&source).expect("parse std.net listen flow");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-net-listen-flow-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.net listen flow");
        let output = Command::new(&bin)
            .output()
            .expect("run std.net listen flow");
        assert!(
            output.status.success(),
            "std.net listen flow failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_net_accept_close_flow() {
        let reservation = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
        let port = reservation
            .local_addr()
            .expect("read reserved loopback port")
            .port();
        drop(reservation);

        let source = format!(
            r#"package std.net
fun listen(endpoint: String): ForeignHandle<Int> {{ throw "intrinsic" }}
async fun accept(listener: ForeignHandle<Int>): ForeignHandle<Int> {{ throw "intrinsic" }}
fun closeListener(listener: ForeignHandle<Int>): Bool {{ throw "intrinsic" }}
fun closeStream(stream: ForeignHandle<Int>): Bool {{ throw "intrinsic" }}
async fun readStream(stream: ForeignHandle<Int>, capacity: Int): String {{ return "" }}
fun main() {{
  val listener: ForeignHandle<Int> = listen("127.0.0.1:{port}")
  val task = spawn {{
    val stream: ForeignHandle<Int> = await accept(listener)
    val payload: String = await readStream(stream, 2)
    if (payload.len != 2) {{ throw "bad payload" }}
    closeStream(stream)
    return
  }}
  join(task)
  closeListener(listener)
}}
"#
        );
        let file = aura_parser::parse_file(&source).expect("parse std.net accept flow");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-net-accept-close-flow-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.net accept flow");
        let process = Command::new(&bin)
            .spawn()
            .expect("start std.net accept flow");
        let address = format!("127.0.0.1:{port}");
        let client = thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                match std::net::TcpStream::connect(&address) {
                    Ok(mut stream) => {
                        stream.write_all(b"g5").expect("send std.net read payload");
                        return;
                    }
                    Err(error) if std::time::Instant::now() < deadline => {
                        let _ = error;
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(error) => panic!("connect std.net accept client: {error}"),
                }
            }
        });
        let output = process
            .wait_with_output()
            .expect("wait std.net accept flow");
        assert!(
            output.status.success(),
            "std.net accept flow failed: {output:?}"
        );
        client.join().expect("join std.net accept client");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_std_http_typed_borrowed_request_response_accessors() {
        let file = aura_parser::parse_file(
            r#"package std.http
fun requestMethod(request: ForeignHandle<Int>): String { throw "intrinsic" }
fun requestTarget(request: ForeignHandle<Int>): String { throw "intrinsic" }
fun requestVersion(request: ForeignHandle<Int>): String { throw "intrinsic" }
fun requestHeaderCount(request: ForeignHandle<Int>): Int { throw "intrinsic" }
fun requestHeaderName(request: ForeignHandle<Int>, index: Int): String { throw "intrinsic" }
fun requestHeaderValue(request: ForeignHandle<Int>, index: Int): String { throw "intrinsic" }
fun requestBody(request: ForeignHandle<Int>): String { throw "intrinsic" }
fun responseStatus(response: ForeignHandle<Int>): Int { throw "intrinsic" }
fun responseKeepAlive(response: ForeignHandle<Int>): Bool { throw "intrinsic" }
fun responseSetStatus(response: ForeignHandle<Int>, status: Int): Bool { throw "intrinsic" }
fun responseSetKeepAlive(response: ForeignHandle<Int>, keepAlive: Bool): Bool { throw "intrinsic" }
fun responseSetBody(response: ForeignHandle<Int>, body: String): Bool { throw "intrinsic" }
fun responseAddHeader(response: ForeignHandle<Int>, name: String, value: String): Bool { throw "intrinsic" }
fun main() {}
"#,
        )
        .expect("parse std.http accessor fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.http accessor fixture");
        assert!(generated.contains("aura_http_request_method"));
        assert!(generated.contains("aura_http_request_body_length"));
        assert!(generated.contains("aura_http_request_header_count"));
        assert!(generated.contains("aura_http_request_header_name"));
        assert!(generated.contains("aura_http_request_header_value"));
        assert!(generated.contains("aura_http_response_set_status"));
        assert!(generated.contains("aura_http_response_add_header"));
        assert!(generated.contains("aura_ffi_handle_pin_for_boundary"));
        assert!(generated.contains("aura_ffi_handle_unpin(&__pin)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-http-accessors-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.http accessor fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_std_http_request_body_async_method() {
        let file = aura_parser::parse_file(
            r#"package std.http
pub class RequestBody(private val handle: ForeignHandle<Int>) {
  pub async fun readChunk(capacity: Int): String { throw "intrinsic" }
}
async fun consume(body: RequestBody): String {
  return await body.readChunk(3)
}
fun main() {}
"#,
        )
        .expect("parse std.http async method fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.http async method fixture");
        assert!(generated.contains("std.http.RequestBody.readChunk"));
        assert!(generated.contains("aura_http_request_read_body"));
        assert!(generated.contains("aura_http_request_wait_body"));
        assert!(generated.contains("aura_http_request_body_read_begin"));
        assert!(generated.contains("aura_http_request_body_read_end"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-http-read-chunk-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.http async method fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_std_http_response_write_chunk_async_method() {
        let file = aura_parser::parse_file(
            r#"package std.http
pub class Response(private val handle: ForeignHandle<Int>, private val connection: ForeignHandle<Int>) {
  pub async fun writeChunk(body: String): Unit { throw "intrinsic" }
}
async fun produce(response: Response): Unit {
  await response.writeChunk("first")
  await response.writeChunk("second")
}
fun main() {}
"#,
        )
        .expect("parse std.http response stream fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.http response stream fixture");
        assert!(generated.contains("std.http.Response.writeChunk"));
        assert!(generated.contains("aura_http_response_stream_begin"));
        assert!(generated.contains("aura_http_connection_stream_write"));
        assert!(generated.contains("aura_http_connection_wait_write"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-http-write-chunk-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.http response stream fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_async_class_method_with_await() {
        let file = aura_parser::parse_file(
            r#"package demo
pub class Counter(val value: Int) {
  async fun current(): Int {
    val n: Int = await one()
    return this.value
  }
  async fun currentTwice(): Int {
    val first: Int = await one()
    val second: Int = await one()
    return this.value
  }
}
async fun one(): Int { return 1 }
async fun read(counter: Counter): Int { return await counter.current() }
fun main() {}
"#,
        )
        .expect("parse generic async class method fixture");
        let generated = emit_c_from_ast(&file).expect("emit generic async class method fixture");
        assert!(generated.contains("aura_async_poll_demo_Counter_current"));
        assert!(generated.contains("aura_cls_demo_Counter * this = data->a_this;"));
        assert!(generated.contains("aura_async_poll_demo_Counter_currentTwice"));
        assert!(generated.contains("aura_method_demo_Counter_current"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-generic-async-class-method-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic async class method fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_async_method_on_generic_class_mono() {
        let file = aura_parser::parse_file(
            r#"package demo
class Box<T>(val value: T) {
  async fun ping(): Unit {
    await one()
  }
}
async fun one(): Unit { }
fun main() {
  val box: Box<Int> = Box(7)
  box.ping()
}
"#,
        )
        .expect("parse generic async class method fixture");
        let generated = emit_c_from_ast(&file).expect("emit generic async class method fixture");
        assert!(generated.contains("aura_async_poll_demo_Box_ping_demo_Box_Int"));
        assert!(generated.contains("aura_method_demo_Box_Int_ping"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-generic-async-method-mono-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic async class method fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic async class method fixture");
        assert!(
            output.status.success(),
            "generic async method failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_generic_free_async_function_mono() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun accept<T>(value: T): T {
  await one()
  return value
}
async fun one(): Unit { }
async fun read(): Unit {
  val value: Int = await accept(41)
  println(value.toString())
}
async fun readArray(): Unit {
  val value: Array<String> = await accept(Array<String>(2))
  println(value.len.toString())
}
fun main() {
  read()
  readArray()
}
"#,
        )
        .expect("parse generic free async fixture");
        let generated = emit_c_from_ast(&file).expect("emit generic free async fixture");
        assert!(generated.contains("aura_fn_demo_accept_Int"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-generic-free-async-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic free async fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic free async fixture");
        assert!(
            output.status.success(),
            "generic free async failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "41\n2");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_open_generic_async_identity_with_erased_payload_abi() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun tick(): Unit { }
async fun identity<T>(value: T): T { await tick() await tick() return value }
fun main() { }
"#,
        )
        .expect("parse open generic async identity fixture");
        let generated = emit_c_from_ast(&file).expect("emit open generic async identity fixture");
        assert!(generated.contains("aura_fn_demo_identity(AuraTypeErasedValue value)"));
        assert!(generated.contains("aura_open_erased_poll_demo_identity(AuraTaskFrame *frame) {\n  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;"));
        assert!(generated.contains("aura_task_frame_set_erased_result"));
        assert!(generated.contains("aura_type_erased_clone(&value"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task_0)"));
        assert!(generated.contains("data->await_task_1"));
        assert!(generated.contains("aura_task_frame_set_gc_mark(frame, aura_open_erased_mark"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-open-generic-identity-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile open generic async identity fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run open generic async identity fixture");
        assert!(
            output.status.success(),
            "open generic identity failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_open_generic_async_identity_without_suspension() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun identity<T>(value: T): T { return value }
fun main() { }
"#,
        )
        .expect("parse no-await open generic identity fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit no-await open generic identity fixture");
        assert!(generated.contains("aura_open_erased_data_demo_identity_state *data"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-open-generic-identity-no-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile no-await open generic identity fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run no-await open generic identity fixture");
        assert!(
            output.status.success(),
            "no-await open generic identity failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_open_generic_async_result_from_awaited_erased_value() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun identity<T>(value: T): T { return value }
async fun relay<T>(value: T): T { val result: T = await identity(value) return result }
fun main() { }
"#,
        )
        .expect("parse open generic async result fixture");
        let generated = emit_c_from_ast(&file).expect("emit open generic async result fixture");
        assert!(generated.contains("aura_fn_demo_relay(AuraTypeErasedValue value)"));
        assert!(generated.contains("aura_task_frame_result_erased(data->await_task_0"));
        assert!(generated.contains("aura_type_erased_drop(&data->value)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-open-generic-result-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile open generic async result fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_open_generic_async_descriptor_forwarding() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun tick(): Unit { }
async fun identity<T>(value: T): T { await tick() return value }
async fun forward<T>(value: T): T { return await identity(value) }
fun main() { }
"#,
        )
        .expect("parse open generic async forwarding fixture");
        let generated = emit_c_from_ast(&file).expect("emit open generic async forwarding fixture");
        assert!(generated.contains("aura_fn_demo_forward(AuraTypeErasedValue value)"));
        assert!(generated.contains("aura_task_frame_result_erased(data->await_task"));
        assert!(generated.contains("aura_open_erased_forward_mark_demo_forward"));
        assert!(generated.contains("aura_open_erased_forward_poll_demo_forward(AuraTaskFrame *frame) {\n  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-open-generic-forward-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile open generic async forwarding fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run open generic async forwarding fixture");
        assert!(output.status.success(), "{output:?}");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_function_returning_fun_payload() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun tick(): Unit { }
async fun produce(): (Int) -> Int {
  await tick()
  return (x: Int) => x + 1
}
async fun read(): Unit {
  val f: (Int) -> Int = await produce()
  println(f(4).toString())
}
fun main() {
  read()
}
"#,
        )
        .expect("parse async fun payload fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-fun-payload-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async fun payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async fun payload fixture");
        assert!(
            output.status.success(),
            "async fun payload failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "5");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_generic_class_method_returning_type_parameter() {
        let file = aura_parser::parse_file(
            r#"package demo
class Node(val value: Int) {}
class Box<T>(val value: T) {
  async fun reveal(): T {
    await one()
    return this.value
  }
}
async fun one(): Unit { }
async fun read(box: Box<Int>): Unit {
  val value: Int = await box.reveal()
  println(value.toString())
}
async fun readText(box: Box<String>): Unit {
  val value: String = await box.reveal()
  println(value)
}
async fun readArray(box: Box<Array<Int>>): Unit {
  val value: Array<Int> = await box.reveal()
  println(value.len.toString())
}
async fun readChild(box: Box<Node>): Unit {
  val value: Node = await box.reveal()
  println(value.value.toString())
}
async fun readNestedArray(box: Box<Array<Node>>): Unit {
  val value: Array<Node> = await box.reveal()
  println(value.len.toString())
  println(value.get(0)!!.value.toString())
}
fun main() {
  val box: Box<Int> = Box(7)
  read(box)
  val text: Box<String> = Box("ok")
  readText(text)
  val numbers: Array<Int> = Array(1)
  numbers.push(1)
  val arrayBox: Box<Array<Int>> = Box(numbers)
  readArray(arrayBox)
  val childBox: Box<Node> = Box(Node(9))
  readChild(childBox)
  val nodes: Array<Node> = Array(0)
  nodes.push(Node(11))
  val nestedArrayBox: Box<Array<Node>> = Box(nodes)
  readNestedArray(nestedArrayBox)
}
"#,
        )
        .expect("parse generic async class payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit generic async class payload fixture");
        assert!(generated.contains("aura_async_poll_demo_Box_reveal_demo_Box_Int"));
        assert!(generated.contains("aura_async_poll_demo_Box_reveal_demo_Box_String"));
        assert!(generated.contains("aura_async_poll_demo_Box_reveal_demo_Box_Array_Int"));
        assert!(generated.contains("aura_method_demo_Box_demo_Node_reveal"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-generic-async-class-payload-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic async class payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic async class payload fixture");
        assert!(
            output.status.success(),
            "generic async class payload failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "7\nok\n2\n9\n1\n11\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_class_method_branch_loop() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Counter(val value: Int) {
  async fun accumulate(limit: Int): Unit {
    var index: Int = 0
    var total: Int = 0
    while (index < limit) {
      val step: Int = await one()
      if (step > 0) {
        total = total + this.value
      }
      index = index + 1
    }
    println(total.toString())
  }
}
async fun one(): Int { return 1 }
fun main() {
  val counter = Counter(7)
  counter.accumulate(2)
}
"#,
        )
        .expect("parse async class method branch-loop fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit async class method branch-loop fixture");
        assert!(generated.contains("aura async general CFG Unit lowering"));
        assert!(generated.contains("aura_cls_std_io_Counter * this = data->a_this;"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-class-method-branch-loop-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async class method branch-loop fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async class method branch-loop fixture");
        assert!(
            output.status.success(),
            "async class method branch-loop fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "14\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_class_method_await_in_condition() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Handler(val label: String) {
  async fun dispatch(): Unit {
    if (await ready()) {
      println(this.label)
    } else {
      println("not-ready")
    }
  }
}
async fun ready(): Bool { return true }
fun main() { Handler("ready").dispatch() }
"#,
        )
        .expect("parse async class await-condition fixture");
        let generated = emit_c_from_ast(&file).expect("emit async class await-condition fixture");
        assert!(generated.contains("aura async general CFG Unit lowering"));
        assert!(generated.contains("__aura_async_cond_"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-class-await-condition-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async class await-condition fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async class await-condition fixture");
        assert!(
            output.status.success(),
            "async class await-condition fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ready\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_class_method_await_while_condition() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Handler() {
  async fun drain(limit: Int): Unit {
    var index: Int = 0
    while (await ready(index < limit)) {
      index = index + 1
    }
    println(index.toString())
  }
}
async fun ready(value: Bool): Bool { return value }
fun main() { Handler().drain(2) }
"#,
        )
        .expect("parse async class await-while-condition fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit async class await-while-condition fixture");
        assert!(generated.contains("aura async general CFG Unit lowering"));
        assert!(generated.contains("__aura_async_cond_"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-async-class-await-while-condition-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async class await-while-condition fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async class await-while-condition fixture");
        assert!(
            output.status.success(),
            "async class await-while-condition fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn lowers_std_task_is_cancelled_inside_async_frame() {
        let file = aura_parser::parse_file(
            r#"package std.task
fun isCancelled(): Bool { return false }
async fun probe(): Bool {
  return isCancelled()
}
fun main() {}
"#,
        )
        .expect("parse std.task cancellation query fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.task cancellation query fixture");
        assert!(generated.contains("aura_task_frame_cancel_requested(frame) != 0"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-task-is-cancelled-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.task cancellation query fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_task_cancel_after() {
        let file = aura_parser::parse_file(
            r#"package std.task
pub fun cancelAfter<T>(task: TaskHandle<T>, milliseconds: Int): Bool { return false }
async fun work(): Unit { }
fun main() {
  val task = spawn { val value: Int = 1 return }
  if (cancelAfter(task, 10)) { println("deadline-set") }
}
"#,
        )
        .expect("parse std.task cancelAfter fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.task cancelAfter fixture");
        assert!(generated.contains("aura_task_frame_set_cancel_deadline"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-task-cancel-after-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.task cancelAfter fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.task cancelAfter fixture");
        assert!(
            output.status.success(),
            "std.task cancelAfter failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "deadline-set\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_task_link_cancellation() {
        let file = aura_parser::parse_file(
            r#"package std.task
pub fun linkCancellation<P, C>(parent: TaskHandle<P>, child: TaskHandle<C>): Bool { return false }
fun main() {
  val parent = spawn { return }
  val child = spawn { return }
  if (linkCancellation(parent, child)) { println("linked") }
}
"#,
        )
        .expect("parse std.task linkCancellation fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.task linkCancellation fixture");
        assert!(generated.contains("aura_task_frame_link_cancellation"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-task-link-cancellation-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.task linkCancellation fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.task linkCancellation fixture");
        assert!(
            output.status.success(),
            "std.task linkCancellation failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "linked\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_dns_numeric_resolution() {
        let file = aura_parser::parse_file(
            r#"package std.dns
pub fun resolveHost(host: String, preferIpv6: Bool): String? { throw "dns intrinsic" }
fun main() {
  val address = resolveHost("127.0.0.1", false)
  if (address != null) { println(address) }
}
"#,
        )
        .expect("parse std.dns resolver fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.dns resolver fixture");
        assert!(generated.contains("aura_dns_resolve_host"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-dns-resolve-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.dns resolver fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.dns resolver fixture");
        assert!(
            output.status.success(),
            "std.dns resolver failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "127.0.0.1\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_std_tls_openssl_bridge() {
        let file = aura_parser::parse_file(
            r#"package std.tls
pub class Config(pub val serverName: String, pub val verifyPeer: Bool) {}
pub class Certificate(pub val subject: String, pub val issuer: String) {}
pub class Connection(pub val endpoint: String) {
  pub async fun read(capacity: Int): String { throw "intrinsic" }
  pub async fun write(content: String): Int { throw "intrinsic" }
  pub fun close(): Unit { throw "intrinsic" }
}
pub fun config(serverName: String, verifyPeer: Bool): Config { return Config(serverName, verifyPeer) }
pub async fun connect(endpoint: String, options: Config): Connection { throw "intrinsic" }
pub fun loadCertificate(path: String): Certificate { throw "intrinsic" }
fun main() {}
"#,
        )
        .expect("parse std.tls fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.tls fixture");
        assert!(generated.contains("AURA_TLS_REQUIRED"));
        assert!(generated.contains("aura_tls_connect"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-tls-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.tls fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_log_levels() {
        let file = aura_parser::parse_file(
            r#"package std.log
pub fun debug(message: String) { }
pub fun info(message: String) { }
pub fun warn(message: String) { }
pub fun error(message: String) { }
fun main() {
  debug("d")
  info("i")
  warn("w")
  error("e")
}
"#,
        )
        .expect("parse std.log fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.log fixture");
        assert!(generated.contains("aura_log(0"));
        assert!(generated.contains("aura_log(3"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-log-levels-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.log fixture");
        let output = Command::new(&bin).output().expect("run std.log fixture");
        assert!(output.status.success(), "std.log failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "[DEBUG] d\n[INFO] i\n[WARN] w\n[ERROR] e\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_metrics_counter() {
        let file = aura_parser::parse_file(
            r#"package std.metrics
pub class Counter(private var value: Int) {
  pub fun add(amount: Int): Int { return 0 }
  pub fun increment(): Int { return 0 }
  pub fun get(): Int { return 0 }
  pub fun reset(): Unit { }
}
fun main() {
  val counter = Counter(2)
  println(counter.increment().toString())
  println(counter.add(4).toString())
  counter.reset()
  println(counter.get().toString())
}
"#,
        )
        .expect("parse std.metrics fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.metrics fixture");
        assert!(generated.contains("__atomic_add_fetch"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-metrics-counter-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.metrics fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.metrics fixture");
        assert!(output.status.success(), "std.metrics failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n7\n0\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_test_assertions() {
        let file = aura_parser::parse_file(
            r#"package std.test
pub fun assert(condition: Bool) { }
pub fun assertEqInt(left: Int, right: Int) { }
pub fun assertEqString(left: String, right: String) { }
pub fun assertEqBool(left: Bool, right: Bool) { }
@test
fun smoke() { }
fun main() {
  assert(true)
  assertEqInt(2 + 2, 4)
  assertEqString("ok", "ok")
  assertEqBool(true, true)
  println("tests-ok")
}
"#,
        )
        .expect("parse std.test fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.test fixture");
        assert!(generated.contains("aura_assert_eq_int"));
        assert!(generated.contains("aura_assert_eq_string"));
        let test_generated = emit_c_tests_from_ast(&file).expect("emit std.test runner fixture");
        assert!(test_generated.contains("ok (%lld ms)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-test-assertions-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.test fixture");
        let output = Command::new(&bin).output().expect("run std.test fixture");
        assert!(output.status.success(), "std.test failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "tests-ok\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_generic_class_static_constructor_body_with_concrete_substitution() {
        let file = aura_parser::parse_file(
            r#"package demo.generic_static
class Bag<T>(pub val first: T) {
  pub static fun make(value: T): Bag<T> {
    val result = Bag<T>(value)
    result.touch(value)
    return result
  }

  pub fun touch(value: T): Unit { }
}
fun main() {
  val bag = Bag.make("ok")
  println(bag.first)
}
"#,
        )
        .expect("parse generic static constructor fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-generic-static-constructor-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic static constructor fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic static constructor fixture");
        assert!(
            output.status.success(),
            "generic static constructor failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_companion_object_member() {
        let file = parse_file(
            r#"package demo.cobj
class Factory(val value: Int) {
  companion object {
    pub fun make(input: Int): Factory { return Factory(input) }
  }
}
fun main() { val result = Factory.make(42) println("companion") }
"#,
        )
        .expect("parse companion object fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-companion-object-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile companion object fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run companion object fixture");
        assert!(
            output.status.success(),
            "companion fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "companion\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_secondary_constructor() {
        let file = parse_file(
            r#"package demo.secondary_ctor
class User(var value: Int) {
  constructor(): this(41) { value = value + 1 }
}
fun main() { println(User().value.toString()) }
"#,
        )
        .expect("parse secondary constructor fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-secondary-constructor-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile secondary constructor fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run secondary constructor fixture");
        assert!(
            output.status.success(),
            "secondary constructor failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_secondary_constructor_interface_delegation() {
        let file = parse_file(
            r#"package demo.secondary_ctor_interface
interface Labelled { fun label(): String }
class Token() : Labelled {
  fun label(): String { return "token" }
}
class Box(val value: Labelled) {
  constructor(): this(Token()) {}
}
fun main() { println(Box().value.label()) }
"#,
        )
        .expect("parse secondary interface constructor fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-secondary-constructor-interface-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile secondary interface constructor fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run secondary interface constructor fixture");
        assert!(
            output.status.success(),
            "secondary interface constructor failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "token\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_default_argument_call() {
        let file = parse_file(
            r#"package demo.default_arg
fun greet(prefix: Int = 7): Int { return prefix }
fun main() { println(greet().toString()) }
"#,
        )
        .expect("parse default argument fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-default-argument-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile default argument fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run default argument fixture");
        assert!(
            output.status.success(),
            "default argument failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_string_default_argument_without_invalid_free() {
        let file = parse_file(
            r#"package demo.default_string
fun greet(prefix: String = "hello"): String { return prefix }
fun main() { println(greet()) }
"#,
        )
        .expect("parse string default argument fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-default-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile string default argument fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run string default argument fixture");
        assert!(
            output.status.success(),
            "string default argument failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "hello\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_top_level_overloads() {
        let file = parse_file(
            r#"package demo.overloads
fun pick(value: Int): Int { return 1 }
fun pick(value: String): Int { return 2 }
fun main() { println(pick(1).toString()) println(pick("x").toString()) }
"#,
        )
        .expect("parse overload fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-overloads-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile overload fixture");
        let output = Command::new(&bin).output().expect("run overload fixture");
        assert!(
            output.status.success(),
            "overload fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_class_method_overloads() {
        let file = parse_file(
            r#"package demo.method_overloads
class Picker() {
  fun pick(value: Int): Int { return 1 }
  fun pick(value: String): Int { return 2 }
}
fun main() { println(Picker().pick(1).toString()) println(Picker().pick("x").toString()) }
"#,
        )
        .expect("parse method overload fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-method-overloads-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile method overload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run method overload fixture");
        assert!(
            output.status.success(),
            "method overload fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_vararg_array_parameter() {
        let file = parse_file(
            r#"package demo.variadic
fun count(vararg values: Int): Int { return values.len }
fun main() { println(count(1, 2, 3).toString()) }
"#,
        )
        .expect("parse vararg fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-vararg-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile vararg fixture");
        let output = Command::new(&bin).output().expect("run vararg fixture");
        assert!(output.status.success(), "vararg fixture failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_string_vararg_array_parameter() {
        let file = parse_file(
            r#"package demo.variadic_string
fun count(vararg values: String): Int { return values.len }
fun main() { println(count("a", "b").toString()) }
"#,
        )
        .expect("parse string vararg fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-vararg-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile string vararg fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run string vararg fixture");
        assert!(
            output.status.success(),
            "string vararg fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_companion_method_overloads() {
        let file = parse_file(
            r#"package demo.static_overloads
class Picker() {
  companion object {
    fun pick(value: Int): Int { return 1 }
    fun pick(value: String): Int { return 2 }
  }
}
fun main() { println(Picker.pick(1).toString()) println(Picker.pick("x").toString()) }
"#,
        )
        .expect("parse static overload fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-static-overloads-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile static overload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run static overload fixture");
        assert!(
            output.status.success(),
            "static overload fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_interface_method_overloads() {
        let file = parse_file(
            r#"package demo.interface_overloads
interface Picker {
  fun pick(value: Int): Int
  fun pick(value: String): Int
}
class Impl() : Picker {
  fun pick(value: Int): Int { return 1 }
  fun pick(value: String): Int { return 2 }
}
fun main() {
  val picker: Picker = Impl()
  println(picker.pick(1).toString())
  println(picker.pick("x").toString())
}
"#,
        )
        .expect("parse interface overload fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-interface-overloads-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile interface overload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run interface overload fixture");
        assert!(
            output.status.success(),
            "interface overload fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_primary_constructor_default() {
        let file = parse_file(
            r#"package demo.primary_default
class User(val id: Int, val label: String = "user") {}
fun main() { println(User(1).label) }
"#,
        )
        .expect("parse primary constructor default fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-primary-default-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile primary constructor default fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run primary constructor default fixture");
        assert!(
            output.status.success(),
            "primary constructor default failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "user\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_interface_default_method_overloads() {
        let file = parse_file(
            r#"package demo.interface_default_overloads
interface Picker {
  fun pick(value: Int): Int { return 1 }
  fun pick(value: String): Int { return 2 }
}
class Impl() : Picker {}
fun main() {
  val picker: Picker = Impl()
  println(picker.pick(1).toString())
  println(picker.pick("x").toString())
}
"#,
        )
        .expect("parse interface default overload fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-interface-default-overloads-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile interface default overload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run interface default overload fixture");
        assert!(
            output.status.success(),
            "interface default overload fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_super_method_call_without_virtual_reentry() {
        let file = parse_file(
            r#"package demo.super_call
open class Base() { open fun label(): String { return "base" } }
class Child() : Base() { override fun label(): String { return super.label() + "-child" } }
fun main() { println(Child().label()) }
"#,
        )
        .expect("parse super call fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-super-call-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile super call fixture");
        let output = Command::new(&bin).output().expect("run super call fixture");
        assert!(
            output.status.success(),
            "super call fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "base-child\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_interface_default_method() {
        let file = parse_file(
            r#"package demo.iface_default
interface Named { fun name(): String { return "default" } }
class User() : Named {}
fun main() { val named: Named = User() println(named.name()) }
"#,
        )
        .expect("parse interface default fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-interface-default-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile interface default fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run interface default fixture");
        assert!(
            output.status.success(),
            "interface default fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "default\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_generic_interface_default_method() {
        let file = parse_file(
            r#"package demo.iface_default_generic
interface Echo<T> { fun echo(value: T): T { return value } }
class User() : Echo<String> {}
fun main() { val echo: Echo<String> = User() println(echo.echo("ok")) }
"#,
        )
        .expect("parse generic interface default fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-interface-default-generic-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic interface default fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic interface default fixture");
        assert!(
            output.status.success(),
            "generic interface default fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_json_validation_and_escape() {
        let file = aura_parser::parse_file(
            r#"package std.json
pub fun isValid(value: String): Bool { return false }
pub fun errorOffset(value: String): Int { return 0 }
pub fun escapeString(value: String): String? { return null }
fun main() {
  if (isValid("{\"ok\": [true, 3.5, null]}")) { println("valid") }
  if (!isValid("{bad")) { println("invalid") }
  println(errorOffset("{bad").toString())
  println(escapeString("a\"b\n")!!)
}
"#,
        )
        .expect("parse std.json fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.json fixture");
        assert!(generated.contains("aura_json_is_valid"));
        assert!(generated.contains("aura_json_error_offset"));
        assert!(generated.contains("aura_json_escape_string"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.json fixture");
        let output = Command::new(&bin).output().expect("run std.json fixture");
        assert!(output.status.success(), "std.json failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "valid\ninvalid\n2\n\"a\\\"b\\n\"\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_json_encode_with_json_field_name() {
        let file = aura_parser::parse_file(
            r#"package std.json
pub fun encode<T>(value: T): String? { throw "std.json encode intrinsic" }
pub fun stringify<T>(value: T): String? { throw "std.json stringify intrinsic" }
class User(@json(name = "user_id") val userId: Int, val name: String) {}
fun main() {
  val encoded = encode<User>(User(7, "a\"b"))
  if (encoded == null) { throw "encode returned null" }
  println(encoded!!)
  val alias = stringify<User>(User(8, "ok"))
  if (alias == null) { throw "stringify returned null" }
  println(alias!!)
  val numbers = Array<Int>(0)
  numbers.push(1)
  numbers.push(2)
  val encodedNumbers = encode<Array<Int>>(numbers)
  if (encodedNumbers == null) { throw "array encode returned null" }
  println(encodedNumbers!!)
}
"#,
        )
        .expect("parse std.json encode fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.json encode fixture");
        assert!(generated.contains("aura_json_encode_object"));
        assert!(generated.contains("\"user_id\""));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-encode-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.json encode fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.json encode fixture");
        assert!(
            output.status.success(),
            "std.json encode failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "{\"user_id\":7,\"name\":\"a\\\"b\"}\n{\"user_id\":8,\"name\":\"ok\"}\n[1,2]\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_json_encode_nested_aggregates() {
        let file = aura_parser::parse_file(
            r#"package std.json
pub fun encode<T>(value: T): String? { throw "std.json encode intrinsic" }
class Address(val city: String, val zip: Int) {}
class User(val name: String, val address: Address, val scores: Array<Int>) {}
fun main() {
  val scores = Array<Int>(0)
  scores.push(3)
  scores.push(5)
  val encoded = encode<User>(User("aura", Address("Hanoi", 10000), scores))
  if (encoded == null) { throw "nested encode returned null" }
  println(encoded!!)
}
"#,
        )
        .expect("parse nested std.json encode fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-nested-encode-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested std.json encode fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested std.json encode fixture");
        assert!(
            output.status.success(),
            "nested std.json failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "{\"name\":\"aura\",\"address\":{\"city\":\"Hanoi\",\"zip\":10000},\"scores\":[3,5]}\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_json_encode_generic_nested_aggregates() {
        let file = aura_parser::parse_file(
            r#"package std.json
pub fun encode<T>(value: T): String? { throw "std.json encode intrinsic" }
class Leaf<T>(val value: T) {}
class Envelope<T>(val leaf: Leaf<T>, val items: Array<T>) {}
fun main() {
  val items = Array<String>(0)
  items.push("b")
  val encoded = encode<Envelope<String>>(Envelope<String>(Leaf<String>("a"), items))
  if (encoded == null) { throw "generic nested encode returned null" }
  println(encoded!!)
}
"#,
        )
        .expect("parse generic nested std.json encode fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-generic-nested-encode-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic nested std.json encode fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic nested std.json encode fixture");
        assert!(
            output.status.success(),
            "generic nested std.json failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "{\"leaf\":{\"value\":\"a\"},\"items\":[\"b\"]}\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_json_encode_payload_enum() {
        let file = aura_parser::parse_file(
            r#"package std.json
pub fun encode<T>(value: T): String? { throw "std.json encode intrinsic" }
enum Result { case Ok(value: Int) case Err(message: String) }
fun main() {
  val ok = encode<Result>(Ok(7))
  if (ok == null) { throw "enum encode returned null" }
  println(ok!!)
  val err = encode<Result>(Err("nope"))
  if (err == null) { throw "enum error encode returned null" }
  println(err!!)
}
"#,
        )
        .expect("parse payload enum std.json encode fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-enum-encode-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile payload enum std.json encode fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run payload enum std.json encode fixture");
        assert!(
            output.status.success(),
            "payload enum std.json failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "{\"variant\":\"Ok\",\"value\":7}\n{\"variant\":\"Err\",\"message\":\"nope\"}\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_json_encode_string_map() {
        let file = aura_parser::parse_file(
            r#"package std.json
pub fun encode<T>(value: T): String? { throw "std.json encode intrinsic" }
class Map<K, V>(val keys: Array<K>, val vals: Array<V>) {}
fun main() {
  val keys = Array<String>(0)
  val vals = Array<Int>(0)
  keys.push("one")
  vals.push(1)
  val encoded = encode<Map<String, Int>>(Map<String, Int>(keys, vals))
  if (encoded == null) { throw "map encode returned null" }
  println(encoded!!)
}
"#,
        )
        .expect("parse map std.json encode fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-map-encode-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile map std.json encode fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run map std.json encode fixture");
        assert!(output.status.success(), "map std.json failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "{\"one\":1}\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_generic_method_that_encodes_its_type_parameter() {
        let file = aura_parser::parse_file(
            r#"package std.json
pub fun encode<T>(value: T): String? { throw "std.json encode intrinsic" }
class Payload(val id: Int) {}
class Context() {
  pub fun sendJson<T>(body: T): String {
    val encoded = encode<T>(body)
    if (encoded == null) { return "" }
    return encoded!!
  }
}
fun main() {
  println(Context().sendJson(Payload(9)))
}
"#,
        )
        .expect("parse generic JSON method fixture");
        let generated = emit_c_from_ast(&file).expect("emit generic JSON method fixture");
        assert!(generated.contains("aura_method_std_json_Context_sendJson_std_json_Payload"));
        assert!(generated.contains("aura_fn_std_json_encode_std_json_Payload"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-generic-method-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic JSON method fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic JSON method fixture");
        assert!(
            output.status.success(),
            "generic JSON method failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "{\"id\":9}\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_io_nullable_result_wrapper_with_owned_string() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
pub fun writeFile(path: String, content: String): Unit { }
pub fun tryReadFile(path: String): String? { return null }
pub fun readLine(): String? { return null }
pub fun readFileResult(path: String): Result<String, String> {
  val content = tryReadFile(path)
  if (content == null) { return Err("missing") }
  return Ok(content!!)
}
pub fun readLineResult(): Result<String?, String> {
  return Ok(readLine())
}
fun main() {
  writeFile("/tmp/aura-io-result-wrapper.txt", "owned")
  val result = readFileResult("/tmp/aura-io-result-wrapper.txt")
  match (result) {
    case Ok(value) => { if (value == "owned") { println("owned-ok") } }
    case Err(error) => { println(error) }
  }
  val line = readLineResult()
  match (line) {
    case Ok(value) => { if (value == null) { println("nullable-ok") } }
    case Err(error) => { println(error) }
  }
}
"#,
        )
        .expect("parse nullable std.io Result wrapper fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-io-result-wrapper-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nullable std.io Result wrapper fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nullable std.io Result wrapper fixture");
        assert!(
            output.status.success(),
            "std.io Result wrapper failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "owned-ok\nnullable-ok\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_json_recursive_generic_class_decode() {
        let file = aura_parser::parse_file(
            r#"package std.json
class Value(val text: String) {}
pub fun decode<T>(value: Value): T? { throw "std.json decode intrinsic" }
class Leaf<T>(val value: T) {}
class Middle<T>(val leaf: Leaf<T>) {}
class Root<T>(val middle: Middle<T>) {}
class OptionalRoot<T>(val child: Leaf<T>?) {}
class Batch<T>(val values: Array<T>) {}
class ClassBatch<T>(val values: Array<Leaf<T>>) {}
fun main() {
  val root = decode<Root<String>>(Value("{\"middle\":{\"leaf\":{\"value\":\"ok\"}}}"))
  if (root == null || root!!.middle.leaf.value != "ok") { throw "recursive generic decode failed" }
  val optional = decode<OptionalRoot<String>>(Value("{\"child\":null}"))
  if (optional == null || optional!!.child != null) { throw "nullable generic decode failed" }
  val batch = decode<Batch<String>>(Value("{\"values\":[\"a\",\"b\"]}"))
  if (batch == null || batch!!.values.len != 2 || batch!!.values.get(1) != "b") { throw "generic array decode failed" }
  val classBatch = decode<ClassBatch<String>>(Value("{\"values\":[{\"value\":\"x\"}]}"))
  if (classBatch == null || classBatch!!.values.len != 1 || classBatch!!.values.get(0).value != "x") { throw "generic class array decode failed" }
  println(root!!.middle.leaf.value)
}
"#,
        )
        .expect("parse recursive generic std.json fixture");
        let generated = emit_c_from_ast(&file).expect("emit recursive generic std.json fixture");
        assert!(generated.contains("aura_json_is_null"));
        assert!(generated.contains("aura_new_std_json_Root_String"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-generic-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile recursive generic std.json fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run recursive generic std.json fixture");
        assert!(
            output.status.success(),
            "recursive generic std.json failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_json_generic_primitive_array_decode() {
        let file = aura_parser::parse_file(
            r#"package std.json
class Value(val text: String) {}
pub fun decode<T>(value: Value): T? { throw "std.json decode intrinsic" }
fun main() {
  val ints = decode<Array<Int>>(Value("[3,5,8]"))
  if (ints!!.len != 3 || ints!!.get(2) != 8) { throw "generic int array decode failed" }
  val strings = decode<Array<String>>(Value("[\"a\",\"b\"]"))
  if (strings!!.len != 2 || strings!!.get(1) != "b") { throw "generic string array decode failed" }
  val nested = decode<Array<Array<Int>>>(Value("[[1,2],[3,4]]"))
  if (nested!!.len != 2 || nested!!.get(1).get(0) != 3) { throw "nested generic array decode failed" }
  val deeplyNested = decode<Array<Array<Array<String>>>>(Value("[[[\"a\"],[\"b\",\"c\"]]]"))
  if (deeplyNested!!.get(0).get(1).get(1) != "c") { throw "deeply nested generic array decode failed" }
  val malformed = decode<Array<Array<Int>>>(Value("[[1],[\"bad\"]]"))
  if (malformed!!.len != 0) { throw "nested generic array decode accepted malformed input" }
  println(ints!!.get(0).toString())
}
"#,
        )
        .expect("parse generic std.json array fixture");
        let generated = emit_c_from_ast(&file).expect("emit generic std.json array fixture");
        assert!(generated.contains("aura_json_array_count"));
        assert!(generated.contains("aura_new_Array_Int"));
        assert!(generated.contains("aura_new_Array_String"));
        assert!(generated.contains("aura_new_Array_Array_Int"));
        assert!(generated.contains("aura_new_Array_Array_Array_String"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-json-array-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic std.json array fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic std.json array fixture");
        assert!(
            output.status.success(),
            "generic std.json array failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_three_level_generic_class_layout() {
        let file = aura_parser::parse_file(
            r#"package demo.generic.three_level
class Box<T>(val value: T) {}
class Pair<A, B>(val left: A, val right: B) {}
class Shelf<T>(val payload: Array<Box<Pair<String, T>>>) {}
fun main() {
  val payload: Array<Box<Pair<String, Int>>> = Array(0)
  payload.push(Box(Pair("deep", 7)))
  val shelf: Shelf<Int> = Shelf(payload)
  val item = shelf.payload.get(0).value
  if (item.left != "deep" || item.right != 7) { throw "three-level generic failed" }
  println(item.left)
}
"#,
        )
        .expect("parse three-level generic fixture");
        let generated = emit_c_from_ast(&file).expect("emit three-level generic fixture");
        assert!(generated.contains("aura_new_demo_generic_three_level_Shelf_Int"));
        assert!(generated.contains(
            "aura_new_demo_generic_three_level_Box_demo_generic_three_level_Pair_String_Int"
        ));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-three-level-generic-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile three-level generic fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run three-level generic fixture");
        assert!(
            output.status.success(),
            "three-level generic failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "deep\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_signal_shutdown_state() {
        let file = aura_parser::parse_file(
            r#"package std.signal
pub fun installShutdown(): Bool { return false }
pub fun shutdownRequested(): Bool { return false }
pub fun clearShutdown(): Unit { }
fun main() {
  if (installShutdown() && !shutdownRequested()) { println("installed") }
  clearShutdown()
  if (!shutdownRequested()) { println("clear") }
}
"#,
        )
        .expect("parse std.signal fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.signal fixture");
        assert!(generated.contains("aura_signal_install_shutdown"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-signal-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.signal fixture");
        let output = Command::new(&bin).output().expect("run std.signal fixture");
        assert!(output.status.success(), "std.signal failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "installed\nclear\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_error_kind_mapping() {
        let file = aura_parser::parse_file(
            r#"package std.error
pub fun kindCode(code: Int): Int { return 11 }
fun main() {
  println(kindCode(22).toString())
  println(kindCode(2000000).toString())
}
"#,
        )
        .expect("parse std.error kind mapping fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.error kind mapping fixture");
        assert!(generated.contains("aura_error_kind_code"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-error-kind-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.error kind mapping fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.error kind mapping fixture");
        assert!(
            output.status.success(),
            "std.error kind mapping failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "0\n11\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_error_transport_classification() {
        let file = aura_parser::parse_file(
            r#"package std.error
enum ErrorKind {
  case TimedOut
  case Cancelled
  case Disconnected
  case ErrorNetwork
}
class Error(pub val kind: ErrorKind, pub val message: String, pub val code: Int) {
  pub fun isRetryable(): Bool {
    match (kind) {
      case TimedOut => { return true }
      case Cancelled => { return false }
      case Disconnected => { return true }
      case ErrorNetwork => { return true }
    }
  }
}
pub fun timedOut(message: String, code: Int): Error { return Error(TimedOut(), message, code) }
pub fun disconnected(message: String, code: Int): Error { return Error(Disconnected(), message, code) }
pub fun network(message: String, code: Int): Error { return Error(ErrorNetwork(), message, code) }
pub fun transport(message: String, code: Int): Error {
  if (message.contains("timeout") || message.contains("timed out")) { return timedOut(message, code) }
  if (message.contains("cancel")) { return Error(Cancelled(), message, code) }
  if (message.contains("closed") || message.contains("disconnect") || message.contains("EOF")) {
    return disconnected(message, code)
  }
  return network(message, code)
}
fun main() {
  if (transport("connection timed out", 1).isRetryable()) { println("timeout") }
  if (!transport("operation cancelled", 2).isRetryable()) { println("cancel") }
  if (transport("peer disconnected", 3).isRetryable()) { println("disconnect") }
  if (transport("unclassified network failure", 4).isRetryable()) { println("network") }
}
"#,
        )
        .expect("parse std.error transport classification fixture");
        emit_c_from_ast(&file).expect("emit std.error transport classification fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-error-transport-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.error transport classification fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.error transport classification fixture");
        assert!(
            output.status.success(),
            "std.error transport classification failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "timeout\ncancel\ndisconnect\nnetwork\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_shared_outcome_class_error_cleanup() {
        let file = aura_parser::parse_file(
            r#"package std.error
enum ErrorKind { case Protocol }
class Error(pub val kind: ErrorKind, pub val message: String, pub val code: Int) {}
enum Outcome<T, E> {
  case OutcomeOk(value: T)
  case OutcomeErr(error: E)
}
fun fail(): Outcome<String, Error> {
  return OutcomeErr(Error(Protocol(), "bad", 400))
}
async fun child(): Int { return 1 }
async fun asyncFail(): Outcome<String, Error> {
  val ignored: Int = await child()
  return OutcomeErr(Error(Protocol(), "async-bad", 401))
}
fun main() {
  val result: Outcome<String, Error> = fail()
  gc_collect()
  match (result) {
    case OutcomeOk(value) => { println("unexpected") }
    case OutcomeErr(error) => { println(error.message) }
  }
}
"#,
        )
        .expect("parse shared Outcome class-error fixture");
        let generated = emit_c_from_ast(&file).expect("emit shared Outcome class-error fixture");
        assert!(generated.contains("data.OutcomeErr.owned"));
        assert!(generated.contains("aura_gc_remove_root"));
        assert!(generated.contains("aura_async_data_drop_std_error_asyncFail"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-shared-outcome-error-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile shared Outcome class-error fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run shared Outcome class-error fixture");
        assert!(
            output.status.success(),
            "Outcome class-error fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "bad\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_join_of_shared_outcome_payload() {
        let mut file = aura_parser::parse_file(
            r#"package std.io
import std.error as Errors
enum ErrorKind { case Protocol }
class Error(pub val kind: ErrorKind, pub val message: String, pub val code: Int) {}
enum Outcome<T, E> {
  case OutcomeOk(value: T)
  case OutcomeErr(error: E)
}
fun protocol(message: String, code: Int): Error {
  return Error(Protocol(), message, code)
}
fun failure<T, E>(error: E): Outcome<T, E> {
  return OutcomeErr(error)
}
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val task = spawn {
    val value: Errors.Outcome<String, Errors.Error> = Errors.failure(Errors.protocol("joined-bad", 402))
    return value
  }
  val first: Result<Errors.Outcome<String, Errors.Error>, TaskError> = join(task)
  match (first) {
    case Ok(value) => {
      match (value) {
        case OutcomeOk(text) => { println(text) }
        case OutcomeErr(error) => { println(error.message) }
      }
    }
    case Err(error) => { println("task-failed") }
  }
  gc_collect()
  val second: Result<Errors.Outcome<String, Errors.Error>, TaskError> = join(task)
  match (second) {
    case Ok(value) => {
      match (value) {
        case OutcomeOk(text) => { println(text) }
        case OutcomeErr(error) => { println(error.message) }
      }
    }
    case Err(error) => { println("task-failed") }
  }
}
"#,
        )
        .expect("parse joined shared Outcome fixture");
        for enum_decl in &mut file.enums {
            if enum_decl.name.name == "TaskError" || enum_decl.name.name == "Result" {
                enum_decl.origin_package = "std.io".into();
                enum_decl.is_pub = true;
            } else if enum_decl.name.name == "ErrorKind" || enum_decl.name.name == "Outcome" {
                enum_decl.origin_package = "std.error".into();
                enum_decl.is_pub = true;
            }
        }
        for class_decl in &mut file.classes {
            if class_decl.name.name == "Error" {
                class_decl.origin_package = "std.error".into();
                class_decl.is_pub = true;
            }
        }
        for function in &mut file.functions {
            if function.name.name == "protocol" || function.name.name == "failure" {
                function.origin_package = "std.error".into();
                function.is_pub = true;
            }
        }
        let generated = emit_c_from_ast(&file).expect("emit joined shared Outcome fixture");
        assert!(generated.contains("data.Ok.owned"));
        assert!(generated.contains("OutcomeErr.error"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-joined-shared-outcome-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile joined shared Outcome fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run joined shared Outcome fixture");
        assert!(
            output.status.success(),
            "joined Outcome fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "joined-bad\njoined-bad\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_join_of_plain_enum_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Color { case Red case Blue(value: Int) }
fun main() {
  val task = spawn {
    return Blue(7)
  }
  val outcome: Result<Color, TaskError> = join(task)
  match (outcome) {
    case Ok(value) => {
      match (value) {
        case Red => { println("red") }
        case Blue(number) => { println(number.toString()) }
      }
    }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse plain enum task payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit plain enum task payload fixture");
        assert!(!generated.contains("unsupported owned task outcome payload type"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-joined-plain-enum-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile plain enum task payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run plain enum task payload fixture");
        assert!(
            output.status.success(),
            "plain enum task payload failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_join_of_string_enum_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Payload { case Text(value: String) }
fun main() {
  val payload: Payload = Text("owned-enum")
  val task = spawn {
    return payload
  }
  val first: Result<Payload, TaskError> = join(task)
  match (first) {
    case Ok(value) => {
      match (value) {
        case Text(text) => { println(text) }
      }
    }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Payload, TaskError> = join(task)
  match (second) {
    case Ok(value) => {
      match (value) {
        case Text(text) => { println(text) }
      }
    }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse String enum task payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit String enum task payload fixture");
        assert!(generated.contains("aura_enum_std_io_Payload_clone"));
        assert!(generated.contains("aura_enum_std_io_Payload_drop"));
        assert!(
            generated.contains("aura_var_std_io_Result_std_io_Payload_std_io_TaskError_OkOwned")
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-joined-string-enum-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile String enum task payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run String enum task payload fixture");
        assert!(
            output.status.success(),
            "String enum task payload failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "owned-enum\nowned-enum\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_repeated_join_of_scheduler_owned_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val outer = spawn { return spawn { return 7 } }
  val first: Result<TaskHandle<Int>, TaskError> = join(outer)
  match (first) {
    case Ok(inner) => {
      val nested: Result<Int, TaskError> = join(inner)
      match (nested) {
        case Ok(value) => { println(value.toString()) }
        case Err(error) => { println("nested-failed") }
      }
    }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<TaskHandle<Int>, TaskError> = join(outer)
  match (second) {
    case Ok(inner) => {
      val nested: Result<Int, TaskError> = join(inner)
      match (nested) {
        case Ok(value) => { println(value.toString()) }
        case Err(error) => { println("nested-failed") }
      }
    }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse scheduler-owned task payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit scheduler-owned task payload fixture");
        assert!(generated.contains("aura_task_executor_retain_payload"));
        assert!(generated.contains("aura_task_executor_release_payload"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-joined-scheduler-payload-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile scheduler-owned task payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run scheduler-owned task payload fixture");
        assert!(
            output.status.success(),
            "scheduler-owned task payload failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n7\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_repeated_join_of_channel_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val outer = spawn { return Channel<Int>(1) }
  val first: Result<Channel<Int>, TaskError> = join(outer)
  match (first) {
    case Ok(channel) => { channel.close() }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Channel<Int>, TaskError> = join(outer)
  match (second) {
    case Ok(channel) => { channel.close() }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse channel task payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit channel task payload fixture");
        assert!(generated.contains("aura_task_channel_retain"));
        assert!(generated.contains("aura_task_channel_destroy"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-joined-channel-payload-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile channel task payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run channel task payload fixture");
        assert!(
            output.status.success(),
            "channel task payload failed: {output:?}"
        );
        assert!(output.stdout.is_empty());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_unit_channel_send_receive() {
        let file = aura_parser::parse_file(
            r#"package demo
fun unit(): Unit { }
fun main() {
  val channel: Channel<Unit> = Channel<Unit>(1)
  channel.send(unit())
  channel.receive()
  channel.close()
}
"#,
        )
        .expect("parse Unit channel fixture");
        let generated = emit_c_from_ast(&file).expect("emit Unit channel fixture");
        assert!(generated.contains("AuraTaskChannelValue __v = { NULL, 0, NULL }"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-unit-channel-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile Unit channel fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run Unit channel fixture");
        assert!(output.status.success(), "Unit channel failed: {output:?}");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_nullable_class_channel_ownership() {
        let file = aura_parser::parse_file(
            r#"package demo
class Box(val value: Int) {}
fun main() {
  val channel: Channel<Box?> = Channel<Box?>(1)
  channel.send(Box(7))
  val received = channel.receive()
  channel.close()
}
"#,
        )
        .expect("parse nullable class channel fixture");
        let generated = emit_c_from_ast(&file).expect("emit nullable class channel fixture");
        assert!(generated.contains("aura_task_channel_value_destroy_class"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nullable-class-channel-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nullable class channel fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nullable class channel fixture");
        assert!(
            output.status.success(),
            "nullable class channel failed: {output:?}"
        );
        assert!(output.stdout.is_empty());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_typed_join_of_owned_array_enum_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Item { case Text(value: String) }
async fun produce(): Array<Item> {
  val values: Array<Item> = Array<Item>(0)
  values.push(Text("array-enum"))
  return values
}
fun main() {
  val task = spawn { val value: Array<Item> = await produce() return value }
  val result: Result<Array<Item>, TaskError> = join(task)
  match (result) {
    case Ok(value) => { match (value.get(0)) { case Text(text) => { println(text) } } }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val repeated: Result<Array<Item>, TaskError> = join(task)
  match (repeated) {
    case Ok(value) => { match (value.get(0)) { case Text(text) => { println(text) } } }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse Array<enum> ownership fixture");
        let generated = emit_c_from_ast(&file).expect("emit Array<enum> ownership fixture");
        assert!(generated.contains("aura_enum_std_io_Item_clone(&this->data[__i])"));
        assert!(generated.contains("aura_enum_std_io_Item_drop(&this->data[__i])"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-array-enum-ownership-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile Array<enum> ownership fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run Array<enum> ownership fixture");
        assert!(
            output.status.success(),
            "Array<enum> ownership fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "array-enum\narray-enum\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn emits_generic_enum_foreign_handle_join_ownership_hooks() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Boxed<T> { case Value(value: T) }
fun makeHandle(): ForeignHandle<Int> { throw "intrinsic" }
async fun produce(): Boxed<ForeignHandle<Int>> { return Value(makeHandle()) }
fun main() {
  val task = spawn { val value: Boxed<ForeignHandle<Int>> = await produce() return value }
  val first: Result<Boxed<ForeignHandle<Int>>, TaskError> = join(task)
  val second: Result<Boxed<ForeignHandle<Int>>, TaskError> = join(task)
}
"#,
        )
        .expect("parse generic foreign-handle enum fixture");
        let generated = emit_c_from_ast(&file).expect("emit generic foreign-handle enum fixture");
        assert!(generated.contains("aura_enum_std_io_Boxed_ForeignHandle_Int_clone"));
        assert!(generated.contains("aura_ffi_handle_retain"));
        assert!(generated.contains("aura_ffi_handle_drop"));
        assert!(generated.contains(
            "aura_var_std_io_Result_std_io_Boxed_ForeignHandle_Int_std_io_TaskError_OkOwned"
        ));
    }

    #[test]
    fn builds_and_runs_nested_array_enum_payload_without_shallow_copy() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Item { case Text(value: String) }
async fun produce(): Array<Array<Item>> {
  val inner: Array<Item> = Array<Item>(0)
  inner.push(Text("nested-array-enum"))
  val values: Array<Array<Item>> = Array<Array<Item>>(0)
  values.push(inner)
  return values
}
fun main() {
  val task = spawn { val value: Array<Array<Item>> = await produce() return value }
  val result: Result<Array<Array<Item>>, TaskError> = join(task)
  match (result) {
    case Ok(value) => { val inner = value.get(0)
      match (inner.get(0)) { case Text(text) => { println(text) } } }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val repeated: Result<Array<Array<Item>>, TaskError> = join(task)
  match (repeated) {
    case Ok(value) => { val inner = value.get(0)
      match (inner.get(0)) { case Text(text) => { println(text) } } }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse nested Array<enum> ownership fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested Array<enum> ownership fixture");
        assert!(generated.contains("_clone(&this->data[__i])"));
        assert!(generated.contains("_drop(&this->data[__i])"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-array-enum-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested Array<enum> ownership fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested Array<enum> ownership fixture");
        assert!(
            output.status.success(),
            "nested Array<enum> ownership fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "nested-array-enum\nnested-array-enum\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn async_cfg_marks_array_enum_across_await_and_gc() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Item { case Text(value: String) }
async fun produce(): Array<Item> {
  val values: Array<Item> = Array<Item>(0)
  values.push(Text("array-enum-await"))
  return values
}
async fun hold(): String {
  val values: Array<Item> = await produce()
  gc_collect()
  match (values.get(0)) {
    case Text(text) => { return text }
  }
}
fun main() {
  val task = spawn { val value: String = await hold() return value }
  val result: Result<String, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value!!) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse async Array<enum> GC fixture");
        let _generated = emit_c_from_ast(&file).expect("emit async Array<enum> GC fixture");

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-array-enum-await-gc-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async Array<enum> GC fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async Array<enum> GC fixture");
        assert!(
            output.status.success(),
            "async Array<enum> GC fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "array-enum-await\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn joins_nested_typed_class_failure_metadata() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Failure(val message: String) {}
pub fun taskErrorTypeName(error: TaskError): String? { return null }
pub fun taskErrorSourceId(error: TaskError): Int { return 0 }
pub fun taskErrorSpanStart(error: TaskError): Int { return 0 }
pub fun taskErrorSpanEnd(error: TaskError): Int { return 0 }
async fun leaf(): Unit { throw Failure("nested-class-failure") }
async fun middle(): Unit { await leaf() }
fun main() {
  val task = spawn { await middle() return "done" }
  val first: Result<String, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      val name: String? = taskErrorTypeName(error)
      if (name != null) { println(name) } else { println("missing-type") }
      if (taskErrorSourceId(error) > 0) { println("source") } else { println("missing-source") }
      if (taskErrorSpanEnd(error) > taskErrorSpanStart(error)) { println("span") } else { println("missing-span") }
    }
  }
  gc_collect()
  val second: Result<String, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      val name: String? = taskErrorTypeName(error)
      if (name != null) { println(name) } else { println("missing-type") }
      if (taskErrorSourceId(error) > 0) { println("source") } else { println("missing-source") }
      if (taskErrorSpanEnd(error) > taskErrorSpanStart(error)) { println("span") } else { println("missing-span") }
    }
  }
}
"#,
        )
        .expect("parse nested typed class failure join fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit nested typed class failure join fixture");
        assert!(generated.contains("aura_task_frame_error_type_name"));
        assert!(generated.contains("type_name_owned"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-typed-failure-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested typed class failure join fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested typed class failure join fixture");
        assert!(
            output.status.success(),
            "nested typed failure failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "Failure\nsource\nspan\nFailure\nsource\nspan\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_cfg_string_enum_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Payload { case Text(value: String) }
async fun produce(): Payload {
  return Text("cfg-enum")
}
async fun consume(): Payload {
  val value: Payload = await produce()
  return value
}
fun main() {
  val task = spawn {
    val value: Payload = await consume()
    return value
  }
  val first: Result<Payload, TaskError> = join(task)
  match (first) {
    case Ok(value) => {
      match (value) {
        case Text(text) => { println(text) }
      }
    }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Payload, TaskError> = join(task)
  match (second) {
    case Ok(value) => {
      match (value) {
        case Text(text) => { println(text) }
      }
    }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse CFG String enum payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit CFG String enum payload fixture");
        assert!(generated.contains("aura_enum_std_io_Payload_clone"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-cfg-string-enum-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile CFG String enum payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run CFG String enum payload fixture");
        assert!(
            output.status.success(),
            "CFG String enum payload failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "cfg-enum\ncfg-enum\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_sync_atomic_int() {
        let file = aura_parser::parse_file(
            r#"package std.sync
pub class AtomicInt(private var value: Int) {
  pub fun load(): Int { return this.value }
  pub fun store(nextValue: Int): Unit { }
  pub fun fetchAdd(amount: Int): Int { return this.value }
  pub fun compareExchange(expected: Int, desired: Int): Bool { return false }
}
pub class Mutex(private var state: Int) {
  pub fun tryLock(): Bool { return false }
  pub fun unlock(): Unit { }
  pub fun isLocked(): Bool { return false }
}
pub class RwLock(private var state: Int) {
  pub fun tryRead(): Bool { return false }
  pub fun tryWrite(): Bool { return false }
  pub fun unlockRead(): Unit { }
  pub fun unlockWrite(): Unit { }
  pub fun readerCount(): Int { return 0 }
  pub fun isWriteLocked(): Bool { return false }
}
pub class Once(private var state: Int) {
  pub fun tryEnter(): Bool { return false }
  pub fun isDone(): Bool { return false }
}
fun main() {
  val counter = AtomicInt(4)
  println(counter.load().toString())
  println(counter.fetchAdd(3).toString())
  counter.store(9)
  if (counter.compareExchange(9, 11)) { println(counter.load().toString()) }
  val mutex = Mutex(0)
  if (mutex.tryLock()) { println("locked") }
  if (mutex.isLocked()) { println("held") }
  mutex.unlock()
  if (!mutex.isLocked()) { println("unlocked") }
  val rw = RwLock(0)
  if (rw.tryRead()) { println("read") }
  println(rw.readerCount().toString())
  rw.unlockRead()
  if (rw.tryWrite()) { println("write") }
  if (rw.isWriteLocked()) { println("write-held") }
  rw.unlockWrite()
  val once = Once(0)
  if (once.tryEnter()) { println("once") }
  if (!once.tryEnter() && once.isDone()) { println("once-done") }
}
"#,
        )
        .expect("parse std.sync AtomicInt fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.sync AtomicInt fixture");
        assert!(generated.contains("__atomic_load_n"));
        assert!(generated.contains("__atomic_fetch_add"));
        assert!(generated.contains("__atomic_compare_exchange_n"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-sync-atomic-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.sync AtomicInt fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.sync AtomicInt fixture");
        assert!(
            output.status.success(),
            "std.sync AtomicInt failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "4\n4\n11\nlocked\nheld\nunlocked\nread\n1\nwrite\nwrite-held\nonce\nonce-done\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_try_finally_after_await() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun tick(): Unit {}
async fun worker(): Unit {
  try {
    await tick()
  } finally {
    println("finally")
  }
}
fun main() { worker() }
"#,
        )
        .expect("parse async try-finally fixture");
        let generated = emit_c_from_ast(&file).expect("emit async try-finally fixture");
        assert!(generated.contains("aura async general CFG Unit lowering"));
        assert!(generated.contains("kind=await-finally"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-try-finally-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async try-finally fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async try-finally fixture");
        assert!(
            output.status.success(),
            "async try-finally fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "finally\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_catch_after_await() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun fail(): Unit { throw "boom" }
async fun worker(): Unit {
  try {
    await fail()
  } catch (error: String) {
    println(error)
  }
}
fun main() { worker() }
"#,
        )
        .expect("parse async catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit async catch fixture");
        assert!(generated.contains("aura_task_frame_error(data->await_task)"));
        assert!(generated.contains("kind=await-catch"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async catch fixture");
        assert!(output.status.success(), "async catch failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "boom\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_nested_async_catch_across_branch_and_loop() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun maybeFail(shouldFail: Bool): String {
  if (shouldFail) { throw "boom" }
  return "ok"
}
async fun nested(shouldFail: Bool): String {
  try {
    if (shouldFail) {
      val failed: String = await maybeFail(true)
      return failed
    } else {
      var i: Int = 0
      var value: String = ""
      while (i < 2) {
        value = await maybeFail(false)
        println(value)
        i = i + 1
        gc_collect()
      }
      value = await maybeFail(false)
      return value
    }
  } catch (error: String) {
    return "caught:" + error
  }
}
fun main() {
  val success = spawn {
    val value: String = await nested(false)
    println(value)
    return
  }
  join(success)
  val failure = spawn {
    val value: String = await nested(true)
    println(value)
    return
  }
  join(failure)
  val cancelled = spawn {
    val value: String = await nested(false)
    println(value)
    return
  }
  cancel(cancelled)
  gc_collect()
}
"#,
        )
        .expect("parse nested async catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested async catch fixture");
        assert!(generated.contains("aura async general CFG String lowering"));
        assert!(generated.contains("kind=await-catch"));
        assert!(generated.matches("kind=await-catch").count() >= 2);

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-nested-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested async catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested async catch fixture");
        assert!(
            output.status.success(),
            "nested async catch failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "ok\nok\nok\ncaught:boom\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_non_unit_catch_after_await() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun fail(): String { throw "boom" }
async fun succeed(): String { return "ok" }
async fun successWorker(): Unit {
  try {
    val value: String = await succeed()
    println(value)
  } catch (error: String) {
    println("caught:" + error)
  }
}
async fun failureWorker(): Unit {
  try {
    val value: String = await fail()
    println(value)
  } catch (error: String) {
    println("caught:" + error)
  }
}
fun main() {
  successWorker()
  failureWorker()
}
"#,
        )
        .expect("parse async non-unit catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit async non-unit catch fixture");
        assert!(generated.contains("aura_task_frame_error(data->await_task)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-non-unit-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async non-unit catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async non-unit catch fixture");
        assert!(
            output.status.success(),
            "async non-unit catch failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\ncaught:boom\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_multi_await_catch_region() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun ok(): String { return "ok" }
async fun fail(): String { throw "boom" }
async fun worker(): Unit {
  try {
    val first: String = await ok()
    val second: String = await ok()
    println(first + second)
  } catch (error: String) {
    println("caught:" + error)
  }
}
async fun failingWorker(): Unit {
  try {
    val first: String = await ok()
    val second: String = await fail()
    println(first + second)
  } catch (error: String) {
    println("caught:" + error)
  }
}
fun main() {
  worker()
  failingWorker()
}
"#,
        )
        .expect("parse multi-await catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit multi-await catch fixture");
        assert!(generated.contains("aura_task_frame_error(data->await_task)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-multi-await-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile multi-await catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run multi-await catch fixture");
        assert!(
            output.status.success(),
            "multi-await catch failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "okok\ncaught:boom\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_catch_that_suspends_again() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun fail(): String { throw "boom" }
async fun recover(): String { return "recovered" }
async fun worker(): String {
  try {
    val value: String = await fail()
    return value
  } catch (error: String) {
    gc_collect()
    val retry: String = await recover()
    return error + ":" + retry
  }
}
async fun driver(): Unit {
  val task = worker()
  val first: String = await task
  println(first)
  gc_collect()
  val second: String = await task
  println(second)
}
fun main() { driver() }
"#,
        )
        .expect("parse async catch-resuspend fixture");
        let generated = emit_c_from_ast(&file).expect("emit async catch-resuspend fixture");
        assert!(generated.contains("kind=await-catch"));
        assert!(generated.matches("kind=await").count() >= 2);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-catch-resuspend-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async catch-resuspend fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async catch-resuspend fixture");
        assert!(
            output.status.success(),
            "async catch-resuspend failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "boom:recovered\nboom:recovered\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_statement_with_awaited_argument() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun value(): String { return "ready" }
async fun consume(text: String): Unit { println(text) }
async fun worker(): Unit { consume(await value()) }
fun main() { worker() }
"#,
        )
        .expect("parse async awaited-argument statement fixture");
        let generated = emit_c_from_ast(&file).expect("emit async awaited-argument fixture");
        assert!(generated.contains("aura async general CFG Unit lowering"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-awaited-argument-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async awaited-argument fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async awaited-argument fixture");
        assert!(
            output.status.success(),
            "async awaited-argument failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ready\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_class_catch_after_await() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Failure(var message: String) {}
async fun fail(): Unit { throw Failure("class-boom") }
async fun worker(): Unit {
  try {
    await fail()
  } catch (error: Failure) {
    gc_collect()
    println(error.message)
  }
}
fun main() { worker() }
"#,
        )
        .expect("parse async class catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit async class catch fixture");
        assert!(generated.contains("aura_task_frame_error_payload(data->await_task)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-class-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async class catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async class catch fixture");
        assert!(
            output.status.success(),
            "async class catch failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "class-boom\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_class_catch_with_nested_heap_field_after_gc() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Child(var value: Int) {}
class Failure(var message: String, var child: Child) {}
async fun fail(): Unit { throw Failure("class-boom", Child(97)) }
async fun worker(): Unit {
  try {
    await fail()
  } catch (error: Failure) {
    gc_collect()
    println(error.message)
    println(error.child.value.toString())
  }
}
fun main() { worker() }
"#,
        )
        .expect("parse nested-field async class catch fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit nested-field async class catch fixture");
        assert!(generated.contains("aura_gc_add_root((void **)&copy->child)"));
        assert!(generated.contains("aura_gc_remove_root((void **)&copy->child)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-class-catch-nested-field-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested-field async class catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested-field async class catch fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "class-boom\n97\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_class_catch_with_array_field_after_gc() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Failure(var message: String, var values: Array<String>) {}
async fun fail(): Unit { throw Failure("array-boom", Array<String>(2)) }
async fun worker(): Unit {
  try {
    await fail()
  } catch (error: Failure) {
    gc_collect()
    println(error.message)
    println(error.values.len.toString())
  }
}
fun main() { worker() }
"#,
        )
        .expect("parse array-field async class catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit array-field async class catch fixture");
        assert!(generated.contains("aura_async_class_error_clone_"));
        assert!(generated.contains("aura_method_Array_String_clone"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-class-catch-array-field-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile array-field async class catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run array-field async class catch fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "array-boom\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_class_catch_with_class_array_field_after_gc() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Child(var value: Int) {}
class Failure(var message: String, var values: Array<Child>) {}
async fun fail(): Unit {
  val values: Array<Child> = Array<Child>(0)
  values.push(Child(113))
  throw Failure("class-array-boom", values)
}
async fun worker(): Unit {
  try {
    await fail()
  } catch (error: Failure) {
    gc_collect()
    println(error.message)
    println(error.values.get(0).value.toString())
  }
}
fun main() { worker() }
"#,
        )
        .expect("parse class-array async catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit class-array async catch fixture");
        assert!(generated.contains("aura_gc_add_array_root"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-class-catch-class-array-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile class-array async catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run class-array async catch fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "class-array-boom\n113\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_primitive_catches_after_await() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun failInt(): Unit { throw 7 }
async fun failBool(): Unit { throw true }
async fun worker(): Unit {
  try { await failInt() } catch (errorInt: Int) { println(errorInt.toString()) }
  try { await failBool() } catch (errorBool: Bool) { if (errorBool) { println("bool") } }
}
fun main() { worker() }
"#,
        )
        .expect("parse primitive async catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit primitive async catch fixture");
        assert!(generated.contains("aura_task_frame_error_type_name"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-primitive-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile primitive async catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run primitive async catch fixture");
        assert!(
            output.status.success(),
            "primitive catch failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\nbool\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_same_name_async_catches_with_different_types() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun failInt(): Unit { await tick() throw 17 }
async fun failText(): Unit { await tick() throw "text-boom" }
async fun tick(): Unit {}
async fun worker(): Unit {
  try { await failInt() } catch (error: Int) { println(error.toString()) }
  try { await failText() } catch (error: String) { println(error) }
}
fun main() { worker() }
"#,
        )
        .expect("parse same-name async catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit same-name async catch fixture");
        assert!(generated.contains("__aura_async_catch_"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-same-name-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile same-name async catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run same-name async catch fixture");
        assert!(
            output.status.success(),
            "same-name catch failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "17\ntext-boom\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_enum_catch_after_await_with_forced_gc() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Failure { case Bad(message: String) }
struct Record(val message: String) {}
async fun tick(): Unit {}
async fun fail(): Unit {
  await tick()
  throw Bad("enum-boom")
}
async fun failRecord(): Unit {
  await tick()
  throw Record("struct-boom")
}
async fun worker(): Unit {
  try {
    await fail()
  } catch (enumError: Failure) {
    println("enum-caught")
  }
  try {
    await failRecord()
  } catch (recordError: Record) {
    println(recordError.message)
  }
}
fun main() {
  val task = spawn { await worker() }
  val first: Result<Unit, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println("done") }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Unit, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println("done") }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse async enum catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit async enum catch fixture");
        assert!(generated.contains("aura_async_cfg_aggregate_error_clone"));
        assert!(generated.contains("aura_task_frame_error_payload"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-enum-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async enum catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async enum catch fixture");
        assert!(
            output.status.success(),
            "async enum catch failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "enum-caught\nstruct-boom\ndone\ndone\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_class_method_catch_after_await() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun fail(): Unit { throw "class-boom" }
class Worker() {
  async fun run(): Unit {
    try {
      await fail()
    } catch (error: String) {
      println(error)
    }
  }
}
fun main() { Worker().run() }
"#,
        )
        .expect("parse async class catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit async class catch fixture");
        assert!(generated.contains("aura_task_frame_error(data->await_task)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-class-method-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async class catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async class catch fixture");
        assert!(
            output.status.success(),
            "async class catch failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "class-boom\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_catch_and_finally_after_await_failure() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun fail(): Int { throw 9 }
async fun worker(): Int {
  var result: Int = 0
  try {
    await fail()
  } catch (error: Int) {
    result = error
  } finally {
    result = result + 1
  }
  return result
}
fun main() {
  val task = spawn { val value: Int = await worker() return value }
  val outcome: Result<Int, TaskError> = join(task)
  match (outcome) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse async catch-finally fixture");
        let generated = emit_c_from_ast(&file).expect("emit async catch-finally fixture");
        assert!(!generated.contains("await lowering is deferred"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-catch-finally-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async catch-finally fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async catch-finally fixture");
        assert!(
            output.status.success(),
            "catch-finally fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "10\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_finally_after_await_failure() {
        let file = aura_parser::parse_file(
            r#"package std.io
async fun fail(): Unit { throw "finally-boom" }
async fun worker(): Unit {
  try {
    await fail()
  } finally {
    println("finally")
  }
}
fun main() {
  val task = spawn { worker() }
  join(task)
}
"#,
        )
        .expect("parse async finally failure fixture");
        let generated = emit_c_from_ast(&file).expect("emit async finally failure fixture");
        assert!(generated.contains("data->await_failed"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-finally-failure-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async finally failure fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async finally failure fixture");
        assert!(
            output.status.success(),
            "async finally failure failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "finally\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_nested_async_finally_cleanup() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun tick(): Unit {}
async fun worker(): Unit {
  try {
    try {
      await tick()
    } finally {
      println("inner")
    }
    await tick()
  } finally {
    println("outer")
  }
}
fun main() { worker() }
"#,
        )
        .expect("parse nested async finally fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested async finally fixture");
        assert!(generated.contains("aura async general CFG Unit lowering"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-async-finally-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested async finally fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested async finally fixture");
        assert!(output.status.success(), "nested finally failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "inner\nouter\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_nested_async_finally_cleanup_on_cancellation() {
        let file = aura_parser::parse_file(
            r#"package std.time
async fun sleep(milliseconds: Int): Unit { throw "time intrinsic" }
async fun worker(): Unit {
  try {
    try {
      await sleep(100)
    } finally {
      println("inner")
    }
    await sleep(100)
  } finally {
    println("outer")
  }
}
fun main() {
  val victim = spawn { await worker() }
  val canceller = spawn { await sleep(1) cancel(victim) }
  join(victim)
  join(canceller)
}
"#,
        )
        .expect("parse nested cancellation finally fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested cancellation finally fixture");
        assert!(generated.contains("data->await_cancelled"));
        assert!(generated.contains("aura_task_frame_set_cancel_handler"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-async-finally-cancel-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested cancellation finally fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested cancellation finally fixture");
        assert!(
            output.status.success(),
            "nested cancellation finally failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "inner\nouter\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_task_handle_local_across_async_cfg_await() {
        let file = aura_parser::parse_file(
            r#"package std.time
async fun sleep(milliseconds: Int): Unit { throw "time intrinsic" }
async fun child(): Unit { await sleep(100) }
async fun parent(): Unit {
  val handle = spawn { await child() }
  await sleep(1)
  cancel(handle)
}
fun main() {
  val task = spawn { await parent() }
  join(task)
}
"#,
        )
        .expect("parse task handle live-across-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit task handle live-across-await fixture");
        assert!(generated.contains("handle:TaskHandle_Unit"));
        assert!(generated
            .contains("aura_task_executor_release_payload(__aura_task_executor, &data->handle)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-task-handle-live-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile task handle live-across-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run task handle live-across-await fixture");
        assert!(
            output.status.success(),
            "task handle live-await failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_channel_local_across_async_cfg_await() {
        let file = aura_parser::parse_file(
            r#"package std.time
async fun sleep(milliseconds: Int): Unit { throw "time intrinsic" }
async fun parent(): Unit {
  val channel = Channel<Int>(1)
  channel.send(7)
  await sleep(1)
  channel.close()
}
fun main() {
  val task = spawn { await parent() }
  join(task)
}
"#,
        )
        .expect("parse channel live-across-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit channel live-across-await fixture");
        assert!(generated.contains("channel:Channel_Int"));
        assert!(generated.contains("aura_task_channel_destroy(data->channel)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-channel-live-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile channel live-across-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run channel live-across-await fixture");
        assert!(
            output.status.success(),
            "channel live-await failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_async_class_throw_after_await_with_owned_payload() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Failure(val message: String) {}
async fun tick(): Unit {}
async fun fail(): Unit {
  await tick()
  throw Failure("boom")
}
fun main() { fail() }
"#,
        )
        .expect("parse async class throw fixture");
        let generated = emit_c_from_ast(&file).expect("emit async class throw fixture");
        assert!(generated.contains("aura_task_frame_set_error_payload_with_clone"));
        assert!(generated.contains("aura_task_frame_set_error_type_name(frame, \"Failure\")"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-class-throw-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async class throw fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async class throw fixture");
        assert!(
            output.status.success(),
            "async class throw fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn emits_std_http_async_server_with_cancellation_bridge() {
        let file = aura_parser::parse_file(
            r#"package std.http
pub class Request(private val handle: ForeignHandle<Int>) {}
pub class Response(private val handle: ForeignHandle<Int>, private val connection: ForeignHandle<Int>) {}
async fun serveConnection(stream: ForeignHandle<Int>, handler: (Request, Response) -> Task<Unit>): Unit { throw "intrinsic" }
async fun serve(listener: ForeignHandle<Int>, handler: (Request, Response) -> Task<Unit>): Unit { throw "intrinsic" }
async fun tick(): Unit {}
async fun health(request: Request, response: Response): Unit {
  var attempts = 0
  while (attempts < 2) {
    if (attempts == 0) {
      await tick()
    } else {
      await tick()
    }
    attempts = attempts + 1
  }
  try {
    if (attempts > 1) {
      await tick()
    } else {
      await tick()
    }
  } catch (error: String) {
    if (attempts > 0) {
      await tick()
    } else {
      await tick()
    }
  }
}
fun makeHandler(): (Request, Response) -> Task<Unit> {
  return (request: Request, response: Response) => health(request, response)
}
fun main() { makeHandler() }
"#,
        )
        .expect("parse std.http async server fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.http async server fixture");
        assert!(generated.contains("aura_http_connection_poll_async_task_handle"));
        assert!(generated.contains("aura_http_connection_create_from_stream"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->child)"));
        assert!(generated.contains("aura_task_frame_set_cancel_handler(frame"));
        assert!(generated.contains("aura_task_executor_cancel(__aura_task_executor, data->child)"));
        assert!(generated.contains("size_t connection_count"));
        assert!(generated.contains("data->connection_count >= 64"));
        assert!(generated.contains("status == AURA_TCP_CLOSED"));
        assert!(generated.contains("aura_async_reap_"));
        assert!(generated.contains("aura_task_executor_release_terminal(__aura_task_executor"));
        assert!(generated.contains("aura_task_executor_release(__aura_task_executor, &connection)"));
        assert!(generated.contains("aura_gc_add_root((void **)&data->request)"));
        assert!(generated.contains("aura_task_frame_wait_tcp_listener"));
        assert!(generated.contains("aura async general CFG Unit lowering"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
    }

    #[test]
    fn builds_and_runs_std_time_sleep_timer() {
        let file = aura_parser::parse_file(
            r#"package std.time
pub class Duration(pub val milliseconds: Int) {}
pub fun milliseconds(value: Int): Duration { return Duration(value) }
pub fun nowMillis(): Int { throw "time intrinsic" }
pub class Deadline(pub val atMillis: Int) {
  pub fun remaining(): Duration {
    val value = this.atMillis - nowMillis()
    if (value <= 0) { return Duration(0) }
    return Duration(value)
  }
}
pub fun after(duration: Duration): Deadline { return Deadline(nowMillis() + duration.milliseconds) }
async fun sleep(milliseconds: Int): Unit { throw "intrinsic" }
async fun sleepFor(duration: Duration): Unit { await sleep(duration.milliseconds) }
async fun sleepUntil(deadline: Deadline): Unit {
  val remaining = deadline.remaining()
  if (remaining.milliseconds > 0) { await sleep(remaining.milliseconds) }
}
async fun waitAndReturn(): Int {
  await sleepUntil(after(milliseconds(1)))
  return 1
}
fun main() {
  val task = spawn { val result: Int = await waitAndReturn() println("timer-ok") return }
  join(task)
}
"#,
        )
        .expect("parse std.time sleep fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.time sleep fixture");
        assert!(generated.contains("aura_task_frame_wait_deadline"));
        assert!(generated.contains("aura_task_frame_take_fd_wait_timeout"));
        assert!(generated.contains("INT32_MAX"));
        assert!(generated.contains("aura_task_frame_cancel_requested"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-time-sleep-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.time sleep fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.time sleep fixture");
        assert!(output.status.success(), "std.time sleep failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "timer-ok\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_encoding_round_trips() {
        let file = aura_parser::parse_file(
            r#"package std.encoding
pub fun hexEncode(value: String): String { throw "encoding intrinsic" }
pub fun hexDecode(value: String): String? { throw "encoding intrinsic" }
pub fun base64Encode(value: String): String { throw "encoding intrinsic" }
pub fun base64Decode(value: String): String? { throw "encoding intrinsic" }
pub fun percentEncode(value: String): String { throw "encoding intrinsic" }
pub fun percentDecode(value: String): String? { throw "encoding intrinsic" }
pub fun isValidUtf8(value: String): Bool { throw "encoding intrinsic" }
fun main() {
  println(hexEncode("Hi"))
  println(hexDecode("4869")!!)
  println(base64Encode("hello"))
  println(base64Decode("aGVsbG8=")!!)
  println(percentEncode("a b/c"))
  println(percentDecode("a%20b%2Fc")!!)
  if (isValidUtf8("hé")) { println("true") } else { println("false") }
}
"#,
        )
        .expect("parse std.encoding fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.encoding fixture");
        assert!(generated.contains("aura_encoding_hex_encode"));
        assert!(generated.contains("aura_encoding_base64_decode"));
        assert!(generated.contains("aura_encoding_percent_decode"));
        assert!(generated.contains("aura_encoding_is_valid_utf8"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-encoding-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.encoding fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.encoding fixture");
        assert!(output.status.success(), "std.encoding failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "4869\nHi\naGVsbG8=\nhello\na%20b%2Fc\na b/c\ntrue\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_bytes_owned_operations() {
        let file = aura_parser::parse_file(
            r#"package std.bytes
pub fun copy(value: String): String { throw "bytes intrinsic" }
pub fun concat(left: String, right: String): String { throw "bytes intrinsic" }
pub fun slice(value: String, start: Int, length: Int): String? { throw "bytes intrinsic" }
pub fun equals(left: String, right: String): Bool { throw "bytes intrinsic" }
fun main() {
  val source = copy("hello")
  println(concat(source, " world"))
  println(slice(source, 1, 3)!!)
  if (equals(source, "hello")) { println("equal") }
  if (slice(source, 99, 1) == null) { println("bounds") }
}
"#,
        )
        .expect("parse std.bytes fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.bytes fixture");
        assert!(generated.contains("aura_bytes_copy"));
        assert!(generated.contains("aura_bytes_slice"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-bytes-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.bytes fixture");
        let output = Command::new(&bin).output().expect("run std.bytes fixture");
        assert!(output.status.success(), "std.bytes failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "hello world\nell\nequal\nbounds\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_bytes_owned_buffer_class() {
        let file = aura_parser::parse_file(
            r#"package std.bytes
class Buffer(private val values: Array<Int>) {
  pub fun length(): Int { return values.len }
  pub fun get(index: Int): Int? {
    if (index < 0 || index >= values.len) { return null }
    return values.get(index)
  }
  pub fun push(value: Int): Bool {
    if (value < 0 || value > 255) { return false }
    values.push(value)
    return true
  }
  pub fun clone(): Buffer { return Buffer(values.clone()) }
}
fun newBuffer(): Buffer { return Buffer(Array(0)) }
fun main() {
  val bytes = newBuffer()
  bytes.push(65)
  bytes.push(256)
  println(bytes.length().toString())
  println(bytes.get(0)!!.toString())
  if (bytes.get(9) == null) { println("bounds") }
  val copy = bytes.clone()
  copy.push(66)
  println(copy.length().toString())
}
"#,
        )
        .expect("parse std.bytes Buffer fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.bytes Buffer fixture");
        assert!(generated.contains("aura_method_Array_Int_get"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-bytes-buffer-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.bytes Buffer fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.bytes Buffer fixture");
        assert!(
            output.status.success(),
            "std.bytes Buffer failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "1\n65\nbounds\n2\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_fs_path_helpers() {
        let file = aura_parser::parse_file(
            r#"package std.fs
pub fun join(base: String, child: String): String { throw "fs intrinsic" }
pub fun basename(path: String): String { throw "fs intrinsic" }
pub fun dirname(path: String): String { throw "fs intrinsic" }
pub fun extension(path: String): String? { throw "fs intrinsic" }
pub fun isAbsolute(path: String): Bool { throw "fs intrinsic" }
pub fun isDirectory(path: String): Bool { throw "fs intrinsic" }
pub fun fileMode(path: String): Int { throw "fs intrinsic" }
pub fun permissions(path: String): Int { throw "fs intrinsic" }
pub fun modifiedMillis(path: String): Int { throw "fs intrinsic" }
pub fun listNames(path: String): String { throw "fs intrinsic" }
pub fun isSymlink(path: String): Bool { throw "fs intrinsic" }
fun main() {
  println(join("/tmp/", "/a.txt"))
  println(basename("/tmp/a.txt"))
  println(dirname("/tmp/a.txt"))
  println(extension("/tmp/a.txt")!!)
  if (isAbsolute("/tmp/a.txt")) { println("absolute") }
  if (isDirectory("/tmp")) { println("directory") }
  println(fileMode("/tmp").toString())
  if (permissions("/tmp") > 0) { println("permissions") }
  if (modifiedMillis("/tmp") > 0) { println("mtime") }
  if (listNames("/tmp").len > 0) { println("entries") }
  if (!isSymlink("/definitely/missing/aura-link")) { println("not-link") }
}
"#,
        )
        .expect("parse std.fs fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.fs fixture");
        assert!(generated.contains("aura_fs_join"));
        assert!(generated.contains("aura_fs_extension"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-fs-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.fs fixture");
        let output = Command::new(&bin).output().expect("run std.fs fixture");
        assert!(output.status.success(), "std.fs failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "/tmp/a.txt\na.txt\n/tmp\n.txt\nabsolute\ndirectory\n2\npermissions\nmtime\nentries\nnot-link\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_os_environment_helpers() {
        let file = aura_parser::parse_file(
            r#"package std.os
pub fun getEnv(name: String): String? { throw "os intrinsic" }
pub fun setEnv(name: String, value: String): Bool { throw "os intrinsic" }
pub fun unsetEnv(name: String): Bool { throw "os intrinsic" }
pub fun cwd(): String { throw "os intrinsic" }
pub fun pid(): Int { throw "os intrinsic" }
pub fun platform(): String { throw "os intrinsic" }
fun main() {
  if (setEnv("AURA_OS_TEST", "ok")) { println(getEnv("AURA_OS_TEST")!!) }
  if (unsetEnv("AURA_OS_TEST") && getEnv("AURA_OS_TEST") == null) { println("unset") }
  if (pid() > 0) { println("pid") }
  if (cwd().len > 0) { println("cwd") }
  println(platform())
}
"#,
        )
        .expect("parse std.os fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.os fixture");
        assert!(generated.contains("aura_os_get_env"));
        assert!(generated.contains("aura_os_pid"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-os-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.os fixture");
        let output = Command::new(&bin).output().expect("run std.os fixture");
        assert!(output.status.success(), "std.os failed: {output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.starts_with("ok\nunset\npid\ncwd\n"), "{stdout}");
        assert!(stdout.trim_end().ends_with("linux") || stdout.trim_end().ends_with("macos"));
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_url_and_mime_sanitizers() {
        let file = aura_parser::parse_file(
            r#"package std.url
pub fun isOriginForm(target: String): Bool { throw "url intrinsic" }
pub fun path(target: String): String? { throw "url intrinsic" }
pub fun query(target: String): String? { throw "url intrinsic" }
pub fun isAbsolute(target: String): Bool { throw "url intrinsic" }
pub fun authority(target: String): String? { throw "url intrinsic" }
pub fun authorityHost(target: String): String? { throw "url intrinsic" }
pub fun authorityPort(target: String): String? { throw "url intrinsic" }
pub fun queryValue(target: String, key: String): String? { throw "url intrinsic" }
fun main() {
  if (isOriginForm("/health?x=1")) { println("url-ok") }
  println(path("/health?x=1")!!)
  println(query("/health?x=1")!!)
  if (!isOriginForm("https://example.test/")) { println("url-reject") }
  if (isAbsolute("https://example.test/health")) { println(authority("https://example.test/health")!!) }
  println(authorityHost("https://user@[::1]:8443/health")!!)
  println(authorityPort("https://user@[::1]:8443/health")!!)
  println(queryValue("/health?a=1&name=alice", "name")!!)
}
"#,
        )
        .expect("parse std.url/std.mime fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.url/std.mime fixture");
        assert!(generated.contains("aura_url_is_origin_form"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-url-mime-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.url/std.mime fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run std.url/std.mime fixture");
        assert!(output.status.success(), "std.url failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "url-ok\n/health\nx=1\nurl-reject\nexample.test\n::1\n8443\nalice\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_mime_sanitizer() {
        let file = aura_parser::parse_file(
            r#"package std.mime
pub fun isValidType(value: String): Bool { throw "mime intrinsic" }
pub fun sanitizeFilename(value: String): String? { throw "mime intrinsic" }
pub fun dispositionFilename(value: String): String? { throw "mime intrinsic" }
fun main() {
  if (isValidType("text/plain; charset=utf-8")) { println("mime-ok") }
  println(sanitizeFilename("dir/file.txt")!!)
  println(dispositionFilename("form-data; name=upload; filename=\"../photo.txt\"")!!)
}
"#,
        )
        .expect("parse std.mime fixture");
        let generated = emit_c_from_ast(&file).expect("emit std.mime fixture");
        assert!(generated.contains("aura_mime_is_valid_type"));
        assert!(generated.contains("aura_mime_sanitize_filename"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-mime-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile std.mime fixture");
        let output = Command::new(&bin).output().expect("run std.mime fixture");
        assert!(output.status.success(), "std.mime failed: {output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "mime-ok\ndir_file.txt\n.._photo.txt\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_function_type_hidden_behind_type_alias() {
        let file = aura_parser::parse_file(
            r#"package demo
type Handler = (ForeignHandle<Int>) -> Task<Unit>
fun start(handler: Handler, value: ForeignHandle<Int>): Task<Unit> {
  return handler(value)
}
fun main() {}
"#,
        )
        .expect("parse aliased function type fixture");
        let generated = emit_c_from_ast(&file).expect("emit aliased function type fixture");
        assert!(generated.contains("typedef struct"));
        assert!(generated.contains("aura_fp_Fun_ForeignHandle_Int__Task_Unit"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-aliased-function-type-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile aliased function type fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_function_value_stored_in_class_field() {
        let file = aura_parser::parse_file(
            r#"package demo
class Route(val handler: (Int) -> Int) {
  fun invoke(value: Int): Int { return this.handler(value) }
}
fun main() {
  val route = Route((value: Int) => value + 1)
  println(route.invoke(41).toString())
}
"#,
        )
        .expect("parse class function-field fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-class-function-field-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile class function-field fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run class function-field fixture");
        assert!(
            output.status.success(),
            "class function-field failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "42\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_class_methods_named_channel_operations() {
        let file = aura_parser::parse_file(
            r#"package demo
class Service() {
  fun send(body: String): Unit { println("send:" + body) }
  fun receive(): String { return "received" }
  fun close(): Unit { println("closed") }
}
fun main() {
  val service = Service()
  service.send("ok")
  println(service.receive())
  service.close()
}
"#,
        )
        .expect("parse channel-named class method fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-class-channel-named-methods-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile channel-named class method fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run channel-named class method fixture");
        assert!(
            output.status.success(),
            "channel-named class method fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "send:ok\nreceived\nclosed\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_compiler_generated_async_write_fd() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun writeFd(fd: Int, content: String): Int { return 0 }
async fun readFd(fd: Int, capacity: Int): String { return "" }
fun main() {
  val task = spawn {
    val count: Int = await writeFd(2, "B")
    return count
  }
  gc_collect()
  val outcome: Result<Int, TaskError> = join(task)
  match (outcome) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val cancelled = spawn {
    val value: String = await readFd(0, 1)
    return value
  }
  cancel(cancelled)
  gc_collect()
}
"#,
        )
        .expect("parse generated async writeFd fixture");
        let generated = emit_c_from_ast(&file).expect("emit generated async writeFd fixture");
        assert!(generated.contains("compiler-generated std.io.writeFd"));
        assert!(generated.contains("aura_io_write_fd"));
        assert!(generated.contains("data->offset"));
        assert!(generated.contains("aura_task_frame_wait_fd(frame, (int)data->fd, 4)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-write-fd-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generated async writeFd fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run generated async writeFd fixture");
        assert!(
            output.status.success(),
            "writeFd fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "B");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_no_await_async_primitive_failure() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun fail(): Int { throw 7 }
fun main() { fail() }
"#,
        )
        .expect("parse async primitive failure fixture");
        let generated = emit_c_from_ast(&file).expect("emit async primitive failure fixture");
        assert!(generated.contains("aura_task_frame_set_error_span_with_clone"));
        assert!(generated.contains("aura_async_string_error_clone_demo_fail"));
        assert!(generated.contains("aura_ex_matches(\"Int\")"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-failure-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async primitive failure fixture");
        assert!(Command::new(&bin)
            .status()
            .expect("run async primitive failure fixture")
            .success());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiles_immediate_await_through_frame_polling() {
        let span = Span::new(0, 1);
        let ident = |name: &str| Ident {
            name: name.into(),
            span,
        };
        let int_ty = || TypeRef {
            qualifier: None,
            name: ident("Int"),
            type_args: vec![],
            nullable: false,
            reference: false,
            span,
            fun: None,
        };
        let worker = AsyncFunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: vec![],
            is_test: false,
            name: ident("worker"),
            type_params: vec![],
            params: vec![],
            return_type: Some(int_ty()),
            body: Block {
                stmts: vec![Stmt::Return(ReturnStmt {
                    value: Some(Expr::Int(IntLit { value: 7, span })),
                    span,
                })],
                span,
            },
            span,
        };
        let wrapper = AsyncFunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: vec![],
            is_test: false,
            name: ident("wrapper"),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Expr(Expr::Async(AsyncExpr::Await(AwaitExpr {
                    operand: Box::new(Expr::Call(CallExpr {
                        callee: Box::new(Expr::Ident(ident("worker"))),
                        type_args: vec![],
                        args: vec![],
                        span,
                    })),
                    span,
                })))],
                span,
            },
            span,
        };
        let main = FunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: vec![],
            modifiers: vec![],
            visibility: aura_ast::MemberVisibility::Package,
            is_test: false,
            name: ident("main"),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![],
                span,
            },
            span,
        };
        let file = File {
            package: Path {
                segments: vec![ident("demo")],
                span,
            },
            imports: vec![],
            interfaces: vec![],
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            consts: vec![],
            functions: vec![main],
            foreign_functions: vec![],
            async_functions: vec![worker, wrapper],
            span,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-await-{}", std::process::id()));
        let generated_c = dir.join(format!("aura-await-{}.aura.c", std::process::id()));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile immediate await");
        let generated = fs::read_to_string(&generated_c).expect("read generated await C");
        assert!(generated.contains("aura_task_frame_poll_once(__await)"));
        assert!(generated.contains("aura_task_frame_is_waiting(__await)"));
        assert!(generated.contains("aura_task_executor_run_one(__aura_task_executor)"));
        assert!(!generated.contains("await lowering is deferred"));
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_single_await_with_hoisted_int_and_string_locals() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun preserve(task: Task<Int>): Int {
  val before: Int = 40
  val label: String = "live" + "!"
  val observed: Int = await task
  return before + observed + label.len
}
fun main() {}
"#,
        )
        .expect("parse single-await live-local fixture");
        let generated = emit_c_from_ast(&file).expect("emit single-await live-local fixture");

        // The single-await straight-line slice stores locals in frame data and
        // resumes the parent only after the awaited child reaches a terminal
        // state. More complex control flow remains on the bounded fallback.
        assert!(generated.contains(
            "typedef struct aura_async_data_demo_preserve {\n  AuraTaskFrame * task;\n  int64_t before;\n  const char * label;\n  bool label__owned;\n  AuraTaskFrame *await_task;\n} aura_async_data_demo_preserve;\n"
        ) || generated.contains("aura async general CFG Int lowering"));
        assert!(
            generated.contains("static int64_t aura_async_resume_demo_preserve(")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("data->before = before;")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("data->label = label;")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
        assert!(
            generated.contains("aura_async_resume_demo_preserve(data, observed)")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(generated.contains("aura async suspension state=1 kind=await"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-await-hoist-{}", std::process::id()));
        let generated_c = dir.join(format!("aura-await-hoist-{}.aura.c", std::process::id()));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile hoisted single-await fixture");
        let status = Command::new(&bin).status().expect("run hoisted fixture");
        assert!(status.success());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_return_position_await_through_the_frame_lowering() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun worker(): Int { return 7 }
async fun wrapper(): Int { return await worker() }
fun main() {}
"#,
        )
        .expect("parse return-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit return-await fixture");
        assert!(generated.contains("__aura_await_return_"));
        assert!(
            generated.contains("aura_async_resume_demo_wrapper")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(!generated.contains("await lowering is deferred"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-return-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile return-await fixture");
        assert!(Command::new(&bin)
            .status()
            .expect("run return-await fixture")
            .success());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_scalar_await_inside_expression_with_continuation() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun one(): Int { return 7 }
async fun wrapper(): Int {
  val value: Int = (await one()) + 1
  return value
}
fun main() {
  val task = spawn { val value: Int = await wrapper() return value }
  val result: Result<Int, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse scalar await-expression fixture");
        let generated = emit_c_from_ast(&file).expect("emit scalar await-expression fixture");
        assert!(!generated.contains("await lowering is deferred"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-await-expression-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile scalar await-expression fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run scalar await-expression fixture");
        assert!(
            output.status.success(),
            "await-expression fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "8\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_await_in_expression_if_condition() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun flag(): Bool { return true }
async fun wrapper(): Int {
  val value: Int = if (await flag()) { 11 } else { 22 }
  return value
}
fun main() {
  val task = spawn { val value: Int = await wrapper() return value }
  val result: Result<Int, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse expression-if await fixture");
        let generated = emit_c_from_ast(&file).expect("emit expression-if await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-await-expression-if-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile expression-if await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run expression-if await fixture");
        assert!(
            output.status.success(),
            "expression-if await failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "11\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_await_in_expression_if_branches() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun left(): Int { return 11 }
async fun right(): Int { return 22 }
async fun wrapper(flag: Bool): Int {
  val value: Int = if (flag) { await left() } else { await right() }
  return value
}
fun main() {
  val first = spawn { val value: Int = await wrapper(true) return value }
  val second = spawn { val value: Int = await wrapper(false) return value }
  val a: Result<Int, TaskError> = join(first)
  val b: Result<Int, TaskError> = join(second)
  match (a) { case Ok(firstValue) => { println(firstValue.toString()) } case Err(firstError) => { println("failed") } }
  match (b) { case Ok(secondValue) => { println(secondValue.toString()) } case Err(secondError) => { println("failed") } }
}
"#,
        )
        .expect("parse expression-if branch await fixture");
        let generated = emit_c_from_ast(&file).expect("emit expression-if branch await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-await-expression-if-branches-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile expression-if branch await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run expression-if branch await fixture");
        assert!(
            output.status.success(),
            "expression-if branch await failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "11\n22\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_aggregate_await_inside_call_argument_with_continuation() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun produce(): String { return "aggregate-await" }
fun identity<T>(value: T): T { return value }
async fun wrapper(): String {
  val value: String = identity(await produce())
  return value
}
fun main() {
  val task = spawn { val value: String = await wrapper() return value }
  val result: Result<String, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse aggregate await-expression fixture");
        let generated = emit_c_from_ast(&file).expect("emit aggregate await-expression fixture");
        assert!(!generated.contains("await lowering is deferred"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-aggregate-await-expression-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile aggregate await-expression fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run aggregate await-expression fixture");
        assert!(
            output.status.success(),
            "aggregate await-expression fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "aggregate-await\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_nested_await_operand_with_continuation() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun leaf(): Int { return 9 }
async fun make(): Task<Int> { return leaf() }
async fun wrapper(): Int {
  return await await make()
}
fun main() {
  val task = spawn { val value: Int = await wrapper() return value }
  val result: Result<Int, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse nested await operand fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested await operand fixture");
        assert!(!generated.contains("await lowering is deferred"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-await-operand-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested await operand fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested await operand fixture");
        assert!(
            output.status.success(),
            "nested await operand fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "9\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_multiple_awaits_inside_one_expression_with_continuations() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun one(): Int { return 3 }
async fun two(): Int { return 5 }
fun identity<T>(value: T): T { return value }
async fun wrapper(): Int {
  val value: Int = identity(await one()) + await two()
  return value
}
fun main() {
  val task = spawn { val value: Int = await wrapper() return value }
  val result: Result<Int, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse multiple-await expression fixture");
        let generated = emit_c_from_ast(&file).expect("emit multiple-await expression fixture");
        assert!(!generated.contains("await lowering is deferred"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-multiple-await-expression-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile multiple-await expression fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run multiple-await expression fixture");
        assert!(
            output.status.success(),
            "multiple-await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "8\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_control_flow_await_with_false_path() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun choose(flag: Bool, task: Task<Int>): Int {
  if (flag) {
    val value: Int = await task
    return value
  }
  return 0
}
fun main() {}
"#,
        )
        .expect("parse control-flow await fixture");
        let generated = emit_c_from_ast(&file).expect("emit control-flow await fixture");
        assert!(
            generated.contains("aura async control-flow suspension")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(generated.contains("flag"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
    }

    #[test]
    fn builds_if_await_with_post_suspend_continuation() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun choose(flag: Bool, task: Task<Int>): Int {
  if (flag) {
    val value: Int = await task
    println(value.toString())
  }
  return 0
}
fun main() {}
"#,
        )
        .expect("parse post-await continuation fixture");
        let generated = emit_c_from_ast(&file).expect("emit post-await continuation fixture");
        assert!(
            generated.contains("aura async if-await continuation")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("data->awaited_value")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("aura_task_frame_propagate_error(frame, data->await_task)")
                || generated.contains("aura async general CFG Int lowering")
        );
    }

    #[test]
    fn builds_if_await_assignment_with_post_branch_continuation() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun worker(): Int { return 7 }
async fun choose(flag: Bool, task: Task<Int>): Int {
  var result: Int = 0
  if (flag) {
    result = await task
  }
  return result
}
fun main() {}
"#,
        )
        .expect("parse if-await assignment fixture");
        let generated = emit_c_from_ast(&file).expect("emit if-await assignment fixture");
        assert!(
            generated.contains("aura async if-assign suspension")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("data->result = result;"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
        assert!(generated.contains("aura_async_destroy_demo_choose"));
        assert!(
            generated.contains("data->result = *((int64_t *)child_result.data)")
                || generated.contains("aura async general CFG")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-await-if-assign-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile if-await assignment fixture");
        assert!(Command::new(&bin)
            .status()
            .expect("run if-await assignment fixture")
            .success());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_two_awaits_with_distinct_resume_states_and_intermediate_cleanup() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun twice(first: Task<Int>, second: Task<Int>): Int {
  val base: Int = 1
  val left: Int = await first
  val label: String = "x" + "!"
  val right: Int = await second
  return base + left + right + label.len
}
fun main() {}
"#,
        )
        .expect("parse two-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit two-await fixture");
        assert!(generated.contains("aura async suspension state=1 kind=await"));
        assert!(generated.contains("aura async suspension state=2 kind=await"));
        assert!(
            generated.contains("AuraTaskFrame *await_task_0;")
                || generated.contains("AuraTaskFrame *await_task;")
        );
        assert!(
            generated.contains("AuraTaskFrame *await_task_1;")
                || generated.contains("AuraTaskFrame *await_task;")
        );
        assert!(
            generated.contains("aura_task_frame_wait_on(frame, data->await_task_0)")
                || generated.contains("aura_task_frame_wait_on(frame, data->await_task)")
        );
        assert!(
            generated.contains("aura_task_frame_wait_on(frame, data->await_task_1)")
                || generated.contains("aura_task_frame_wait_on(frame, data->await_task)")
        );
        assert!(
            generated.contains("aura_task_frame_propagate_error(frame, data->await_task_0)")
                || generated.contains("aura_task_frame_propagate_error(frame, data->await_task)")
        );
        assert!(generated.contains("return AURA_TASK_CANCELLED;"));
        assert!(
            generated.contains("data->label__owned = true;")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("if (data->label__owned) free((void *)data->label);")
                || generated.contains("aura async general CFG Int lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-await-two-{}", std::process::id()));
        let generated_c = dir.join(format!("aura-await-two-{}.aura.c", std::process::id()));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile two-await fixture");
        let status = Command::new(&bin).status().expect("run two-await fixture");
        assert!(status.success());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_three_awaits_with_distinct_resume_states_and_child_edges() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun thrice(first: Task<Int>, second: Task<Int>, third: Task<Int>): Int {
  val base: Int = 1
  val left: Int = await first
  val middle: String = "x" + "!"
  val center: Int = await second
  val right: Int = await third
  return base + left + center + right + middle.len
}
fun main() {}
"#,
        )
        .expect("parse three-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit three-await fixture");
        assert!(generated.contains("aura async suspension state=1 kind=await"));
        assert!(generated.contains("aura async suspension state=2 kind=await"));
        assert!(generated.contains("aura async suspension state=3 kind=await"));
        assert!(
            generated.contains("AuraTaskFrame *await_task_0;")
                || generated.contains("AuraTaskFrame *await_task;")
        );
        assert!(
            generated.contains("AuraTaskFrame *await_task_1;")
                || generated.contains("AuraTaskFrame *await_task;")
        );
        assert!(
            generated.contains("AuraTaskFrame *await_task_2;")
                || generated.contains("AuraTaskFrame *await_task;")
        );
        assert!(
            generated.contains("aura_task_frame_wait_on(frame, data->await_task_2)")
                || generated.contains("aura_task_frame_wait_on(frame, data->await_task)")
        );
        assert!(
            generated.contains("aura_task_frame_propagate_error(frame, data->await_task_2)")
                || generated.contains("aura_task_frame_propagate_error(frame, data->await_task)")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 3)"));
        assert!(
            generated.contains("if (data->middle__owned) free((void *)data->middle);")
                || generated.contains("aura async general CFG Int lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-await-three-{}", std::process::id()));
        let generated_c = dir.join(format!("aura-await-three-{}.aura.c", std::process::id()));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile three-await fixture");
        let status = Command::new(&bin)
            .status()
            .expect("run three-await fixture");
        assert!(status.success());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_four_await_state_machine() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun worker(value: Int): Int { return value }
async fun four(): Int {
  val base: Int = 1
  val first: Int = await worker(2)
  val label: String = "four" + "!"
  val second: Int = await worker(3)
  val third: Int = await worker(4)
  val fourth: Int = await worker(5)
  println(label)
  return base + first + second + third + fourth
}
fun main() {
  val marker: String = "marker"
  val task = spawn { val result: Int = await four()
    println(marker)
    println("four-await-ok")
    return }
  join(task)
}
"#,
        )
        .expect("parse general four-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit general four-await fixture");
        let generated_again = emit_c_from_ast(&file).expect("re-emit general four-await fixture");
        assert_eq!(
            generated, generated_again,
            "async model dump must be deterministic"
        );
        assert!(generated.contains("aura async model version=1"));
        assert!(generated.contains("aura async frame fields:"));
        assert!(generated.contains("base:Int"));
        assert!(generated.contains("label:String"));
        assert!(generated.contains("aura async state=0 kind="));
        assert!(generated.contains("kind=await next="));
        assert!(generated.contains("kind=return"));
        for state in 1..=4 {
            assert!(generated.contains(&format!(
                "aura async general suspension state={state} kind=await"
            )));
        }
        assert!(
            generated.contains("AuraTaskFrame *await_task_3;")
                || generated.contains("AuraTaskFrame *await_task;")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 4)"));
        assert!(
            generated
                .contains("aura_task_executor_release(__aura_task_executor, &data->await_task_0)")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated
                .contains("aura_task_executor_release(__aura_task_executor, &data->await_task_3)")
                || generated.contains("aura async general CFG Int lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-await-general-four-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general four-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general four-await fixture");
        assert!(
            output.status.success(),
            "four-await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "four!\nmarker\nfour-await-ok\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_eight_await_state_machine() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun worker(value: Int): Int { return value }
async fun eight(): Int {
  val a: Int = await worker(1)
  val b: Int = await worker(2)
  val c: Int = await worker(3)
  val d: Int = await worker(4)
  val e: Int = await worker(5)
  val f: Int = await worker(6)
  val g: Int = await worker(7)
  val h: Int = await worker(8)
  gc_collect()
  return a + b + c + d + e + f + g + h
}
fun main() {
  val task = spawn {
    val value: Int = await eight()
    println(value.toString())
    return
  }
  join(task)
}
"#,
        )
        .expect("parse general eight-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit general eight-await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("aura async model version=1"));
        assert!(generated.matches("kind=await next=").count() >= 8);
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 8)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-general-eight-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general eight-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general eight-await fixture");
        assert!(
            output.status.success(),
            "eight-await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "36\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_for_in_nested_array_await() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun leaf(): Int { return 1 }
async fun sum(rows: Array<Array<Int>>): Int {
  var total: Int = 0
  for (row in rows) {
    val value: Int = await leaf()
    total = total + value
    gc_collect()
  }
  return total
}
fun main() {
  val rows: Array<Array<Int>> = Array<Array<Int>>(3)
  val task = spawn {
    val value: Int = await sum(rows)
    println(value.toString())
    return
  }
  join(task)
}
"#,
        )
        .expect("parse nested-array for-in await fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested-array for-in await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("aura_method_Array_Array_Int_clone"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-for-in-nested-array-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested-array for-in await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested-array for-in await fixture");
        assert!(
            output.status.success(),
            "nested-array for-in fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_four_await_string_state_machine() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun worker(): String { return "general-string" }
async fun four_string(): String {
  val first: String = await worker()
  val second: String = await worker()
  val third: String = await worker()
  val fourth: String = await worker()
  return fourth
}
fun main() {
  val task = spawn {
    val result: String = await four_string()
    println(result)
    return
  }
  join(task)
}
"#,
        )
        .expect("parse general four-await String fixture");
        let generated = emit_c_from_ast(&file).expect("emit general four-await String fixture");
        assert!(generated.contains("aura async general suspension state=4 kind=await"));
        assert!(generated.contains("const char * fourth;"));
        assert!(
            generated.contains("strlen(__returned)")
                || generated.contains("aura async general CFG String lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-general-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general four-await String fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general four-await String fixture");
        assert!(
            output.status.success(),
            "general four-await String fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "general-string\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn general_cfg_owns_string_concat_across_await() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun worker(): String { return "done" }
async fun request(): String {
  val prefix = "pre" + "fix-"
  val value: String = await worker()
  return prefix + value
}
fun main() { request() }
"#,
        )
        .expect("parse owned async String fixture");
        let generated = emit_c_from_ast(&file).expect("emit owned async String fixture");
        assert!(generated.contains("prefix__owned = true"));
        assert!(generated.contains("if (prefix__owned && prefix != NULL) free((void *)prefix)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-owned-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile owned async String fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run owned async String fixture");
        assert!(
            output.status.success(),
            "owned async String fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn general_cfg_string_accumulation_does_not_free_rhs_source() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun chunk(index: Int): String { return index.toString() }
async fun collect(): String {
  var body: String = ""
  var index: Int = 0
  while (index < 2) {
    val part: String = await chunk(index)
    body = body + part
    index = index + 1
  }
  return body
}
fun main() {
  val task = spawn { return await collect() }
  val result = join(task)
  match (result) {
    case Ok(value) => { println(value) }
    case Err(error) => { throw "string accumulation failed" }
  }
}
"#,
        )
        .expect("parse async string accumulation fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-string-accumulation-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async string accumulation fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async string accumulation fixture");
        assert!(
            output.status.success(),
            "async string accumulation fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "01\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn general_cfg_supports_awaited_return_expression() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(): String { return "returned" }
async fun wrapper(flag: Bool): String {
  if (flag) {
    return await worker()
  }
  return "fallback"
}
fun main() {
  val task = spawn { val value: String = await wrapper(true) return value }
  val result = join(task)
  match (result) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse awaited return expression fixture");
        let generated = emit_c_from_ast(&file).expect("emit awaited return expression fixture");
        assert!(generated.contains("__aura_async_return_"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-return-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile awaited return expression fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run awaited return expression fixture");
        assert!(
            output.status.success(),
            "awaited return expression fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "returned\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn general_cfg_accepts_nullable_local_before_awaited_class_return() {
        let file = aura_parser::parse_file(
            r#"package demo
class Reply(val code: Int) {}
async fun raw(): String { return "204" }
async fun parse(): Reply {
  val text: String = await raw()
  val code = text.toInt()
  if (code == null) { return Reply(0) }
  return Reply(code!!)
}
fun main() { parse() }
"#,
        )
        .expect("parse nullable async CFG fixture");
        let generated = emit_c_from_ast(&file).expect("emit nullable async CFG fixture");
        assert!(generated.contains("aura_opt_i64 code"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-nullable-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nullable async CFG fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nullable async CFG fixture");
        assert!(
            output.status.success(),
            "nullable async CFG fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_top_level_while_await_int_without_string_temporary() {
        let source = r#"package demo
async fun worker(value: Int): Int { return value }
async fun sum(limit: Int): Int {
  var i: Int = 0
  var total: Int = 0
  while (i < limit) {
    val value: Int = await worker(i)
    total = total + value
    i = i + 1
  }
  if (total == 10) { println("10") }
  return total
}
fun main() {
  val task = spawn { val result: Int = await sum(5) return }
  join(task)
}
"#;
        let file = parse_file(source).expect("parse while-await Int fixture");
        let generated = emit_c_from_ast(&file).expect("emit while-await Int fixture");
        assert!(
            generated.contains("/* aura async top-level while-await Int lowering */")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
        assert!(
            !generated.contains("aura_task_executor_wake(__aura_task_executor, data->await_task)")
        );
        assert!(!generated.contains("aura_task_executor_run_one(__aura_task_executor)"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-while-await-int-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile while-await Int fixture with ASAN/UBSAN");
        let output = Command::new(&bin)
            .output()
            .expect("run while-await Int fixture");
        assert!(
            output.status.success(),
            "while-await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "10\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_loop_await_with_break_continue_and_gc() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int, shouldFail: Bool): Int {
  if (shouldFail) { throw "break-continue-failure" }
  return value
}
async fun guarded(limit: Int, shouldFail: Bool): Int {
  var i: Int = 0
  while (i < limit) {
    if (i == 1) {
      i = i + 1
      continue
    }
    if (i == 4) { break }
    val value: Int = await worker(i, shouldFail && i == 3)
    i = i + 1
    gc_collect()
  }
  return i
}
fun main() {
  val first = spawn { val value: Int = await guarded(6, false) println(value.toString()) return value }
  val first_outcome: Result<Int, TaskError> = join(first)
  gc_collect()
  val second = spawn { val value: Int = await guarded(2, false) println(value.toString()) return value }
  val second_outcome: Result<Int, TaskError> = join(second)
  val failed = spawn { val value: Int = await guarded(6, true) return value }
  val failed_result: Result<Int, TaskError> = join(failed)
  match (failed_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("unexpected-cancel") }
      }
    }
  }
  val cancelled = spawn { val value: Int = await guarded(6, false) println("unexpected-cancel") return value }
  cancel(cancelled)
  val cancelled_outcome: Result<Int, TaskError> = join(cancelled)
}
"#;
        let file = parse_file(source).expect("parse guarded loop-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit guarded loop-await fixture");
        assert!(
            generated.contains("aura async general CFG Int lowering")
                || generated.contains("aura async loop CFG suspension states=1")
        );
        assert!(
            generated.contains("aura_async_loop_cfg_head")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 2)"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-loop-guarded-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile guarded loop-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run guarded loop-await fixture");
        assert!(
            output.status.success(),
            "guarded loop-await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "4\n2\nbreak-continue-failure\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_nested_loop_await_with_persisted_state() {
        let source = r#"package demo
async fun worker(value: Int): Int { return value }
async fun nested(limit: Int): Int {
  var outer: Int = 0
  var total: Int = 0
  while (outer < limit) {
    var inner: Int = 0
    while (inner < 2) {
      val value: Int = await worker(inner)
      total = total + value
      inner = inner + 1
      gc_collect()
    }
    outer = outer + 1
  }
  return total
}
fun main() {
  val task = spawn { val result: Int = await nested(3) println(result.toString()) return }
  join(task)
}
"#;
        let file = parse_file(source).expect("parse nested loop-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested loop-await fixture");
        assert!(
            generated.contains("/* aura async nested while-await Int lowering */")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("aura_async_nested_outer_head")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("aura_async_nested_inner_head")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("aura_task_frame_set_resume_state(frame, 2)")
                || generated.contains("aura async general CFG Int lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-loop-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested loop-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested loop-await fixture");
        assert!(
            output.status.success(),
            "nested loop-await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_loop_with_multiple_await_states() {
        let source = r#"package demo
async fun worker(value: Int): Int { return value }
async fun sum(limit: Int): Int {
  var i: Int = 0
  var total: Int = 0
  while (i < limit) {
    val first: Int = await worker(i)
    val second: Int = await worker(first + 1)
    total = total + first + second
    gc_collect()
    i = i + 1
  }
  return total
}
fun main() {
  val task = spawn { val result: Int = await sum(3) println(result.toString()) return }
  join(task)
  val cancelled = spawn { val result: Int = await sum(3) println("unexpected-cancel") return }
  cancel(cancelled)
  join(cancelled)
}
"#;
        let file = parse_file(source).expect("parse multi-await loop fixture");
        let generated = emit_c_from_ast(&file).expect("emit multi-await loop fixture");
        assert!(
            generated.contains("aura async general CFG Int lowering")
                || generated.contains("aura async loop multi-await suspension states=2")
        );
        assert!(
            generated
                .contains("aura_task_executor_release(__aura_task_executor, &data->await_task_0)")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("aura_task_frame_set_resume_state(frame, 2)")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("aura_task_frame_wait_on(frame, data->await_task_1)")
                || generated.contains("aura async general CFG Int lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-loop-multi-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile multi-await loop fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run multi-await loop fixture");
        assert!(
            output.status.success(),
            "multi-await loop fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "9\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn lowers_while_await_for_checked_task_parameter() {
        let source = r#"package demo
async fun sum(task: Task<Int>): Int {
  var i: Int = 0
  var total: Int = 0
  while (i < 1) {
    val value: Int = await task
    total = total + value
    i = i + 1
  }
  return total
}
"#;
        let file = parse_file(source).expect("parse task-parameter while-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit task-parameter while-await fixture");
        assert!(
            generated.contains("/* aura async top-level while-await Int lowering */")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(generated.contains("data->await_task = task;"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
    }

    #[test]
    fn builds_and_runs_conditional_await_inside_loop() {
        let source = r#"package demo
async fun worker(value: Int): Int { println("hit") return value }
async fun count(flag: Bool, limit: Int): Int {
  var i: Int = 0
  while (i < limit) {
    if (flag) {
      val value: Int = await worker(i)
    }
    i = i + 1
  }
  return i
}
fun main() {
  val first = spawn { val result: Int = await count(true, 3) return }
  join(first)
  val second = spawn { val result: Int = await count(false, 3) return }
  join(second)
}
"#;
        let file = parse_file(source).expect("parse conditional loop-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit conditional loop-await fixture");
        assert!(
            generated.contains("aura async general CFG Int lowering")
                || generated.contains("/* aura async conditional loop suspension state=1")
        );
        assert!(
            generated.contains("data->await_task = aura_fn_demo_worker(i);")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("if (flag)")
                || generated.contains("aura async general CFG Int lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-conditional-loop-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile conditional loop-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run conditional loop-await fixture");
        assert!(
            output.status.success(),
            "conditional loop-await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "hit\nhit\nhit\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_branch_containing_loop_await() {
        let source = r#"package demo
async fun worker(value: Int): Int { return value }
async fun nested(flag: Bool): Int {
  var total: Int = 0
  if (flag) {
    var i: Int = 0
    while (i < 2) {
      val value: Int = await worker(i)
      total = total + value
      i = i + 1
      gc_collect()
    }
  } else {
    total = 7
  }
  return total
}
fun main() {
  val first = spawn { val value: Int = await nested(true) println(value.toString()) return }
  join(first)
  val second = spawn { val value: Int = await nested(false) println(value.toString()) return }
  join(second)
  val cancelled = spawn { val value: Int = await nested(true) println("unexpected-cancel") return }
  cancel(cancelled)
  join(cancelled)
}
"#;
        let file = parse_file(source).expect("parse general CFG await fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG await fixture");
        assert!(generated.contains("/* aura async general CFG Int lowering"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated.contains("data->await_task_owned"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG await fixture");
        assert!(
            output.status.success(),
            "general CFG await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n7\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_array_branch_loop_with_owned_outcome() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int, fail: Bool): Array<Int> {
  if (fail) { throw "aggregate-failure" }
  return Array<Int>(value)
}
async fun nested(flag: Bool): Array<Int> {
  if (flag) {
    var i: Int = 0
    while (i < 1) {
      val value: Array<Int> = await worker(3, false)
      gc_collect()
      return value
    }

  }
  return Array<Int>(0)
}
async fun broken(): Array<Int> {
  if (true) {
    var i: Int = 0
    while (i < 1) {
      val value: Array<Int> = await worker(3, true)
      return value
    }
  }
  return Array<Int>(0)
}
fun main() {
  val task = spawn { val value: Array<Int> = await nested(true) return value }
  val first: Result<Array<Int>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Array<Int>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val failed = spawn { val value: Array<Int> = await broken() return value }
  val failed_result: Result<Array<Int>, TaskError> = join(failed)
  match (failed_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("unexpected-cancel") }
      }
    }
  }
  val cancelled = spawn { val value: Array<Int> = await nested(true) return value }
  cancel(cancelled)
  val cancelled_result: Result<Array<Int>, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#;
        let file = parse_file(source).expect("parse aggregate CFG await fixture");
        let generated = emit_c_from_ast(&file).expect("emit aggregate CFG await fixture");
        assert!(generated.contains("aura async general CFG Array lowering"));
        assert!(generated.contains("aura_method_Array_Int_clone"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated.contains("aura_async_result_destroy_"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-array-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile aggregate CFG await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run aggregate CFG await fixture");
        assert!(
            output.status.success(),
            "aggregate CFG await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "3\n3\naggregate-failure\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_struct_across_branch_loop_await() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
struct Packet(val text: String, val code: Int) {}
async fun worker(code: Int): Packet { return Packet("packet", code) }
async fun choose(flag: Bool): Packet {
  var result: Packet = Packet("initial", 0)
  var index: Int = 0
  while (index < 2) {
    if (flag) {
      val next: Packet = await worker(index + 1)
      result = next
    } else {
      val alternate: Packet = await worker(9)
      result = alternate
    }
    gc_collect()
    index = index + 1
  }
  return result
}
fun main() {
  val task = spawn { val value: Packet = await choose(true) return value }
  val first: Result<Packet, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.text) println(value.code.toString()) }
    case Err(error) => { println("struct-error") }
  }
  gc_collect()
  val second: Result<Packet, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.text) println(value.code.toString()) }
    case Err(error) => { println("struct-repeat-error") }
  }
}
"#;
        let file = aura_parser::parse_file(source).expect("parse general CFG struct fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG struct fixture");
        assert!(generated.contains("aura async general CFG Struct lowering"));
        assert!(generated.contains("aura_async_result_destroy_"));
        assert!(generated.contains("_Packet_clone"));
        assert!(generated.contains("_Packet_mark"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-struct-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG struct fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG struct fixture");
        assert!(
            output.status.success(),
            "general CFG struct fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "packet\n2\npacket\n2\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_string_array_branch_loop_with_owned_outcome() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int, fail: Bool): Array<String> {
  if (fail) { throw "aggregate-string-failure" }
  return Array<String>(value)
}
async fun nested(flag: Bool): Array<String> {
  if (flag) {
    var i: Int = 0
    while (i < 1) {
      val value: Array<String> = await worker(3, false)
      gc_collect()
      return value
    }
  }
  return Array<String>(0)
}
async fun broken(): Array<String> {
  if (true) {
    var i: Int = 0
    while (i < 1) {
      val value: Array<String> = await worker(3, true)
      return value
    }
  }
  return Array<String>(0)
}
fun main() {
  val task = spawn { val value: Array<String> = await nested(true) return value }
  val first: Result<Array<String>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Array<String>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val failed = spawn { val value: Array<String> = await broken() return value }
  val failed_result: Result<Array<String>, TaskError> = join(failed)
  match (failed_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("unexpected-cancel") }
      }
    }
  }
  val cancelled = spawn { val value: Array<String> = await nested(true) return value }
  cancel(cancelled)
  val cancelled_result: Result<Array<String>, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#;
        let file = parse_file(source).expect("parse String array CFG await fixture");
        let generated = emit_c_from_ast(&file).expect("emit String array CFG await fixture");
        assert!(generated.contains("aura async general CFG Array lowering"));
        assert!(generated.contains("aura_method_Array_String_clone"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-string-array-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile String array CFG await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run String array CFG await fixture");
        assert!(
            output.status.success(),
            "String array CFG await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "3\n3\naggregate-string-failure\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_string_branch_loop_with_owned_outcome() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(fail: Bool): String {
  if (fail) { throw "string-failure" }
  return "cfg-string"
}
async fun nested(flag: Bool): String {
  if (flag) {
    var i: Int = 0
    while (i < 1) {
      val value: String = await worker(false)
      gc_collect()
      return value
    }
  }
  return "empty"
}
async fun broken(): String {
  if (true) {
    var i: Int = 0
    while (i < 1) {
      val value: String = await worker(true)
      return value
    }
  }
  return "empty"
}
fun main() {
  val task = spawn { val value: String = await nested(true) return value }
  val first: Result<String, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<String, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("unexpected-error") }
  }
  val failed = spawn { val value: String = await broken() return value }
  val failed_result: Result<String, TaskError> = join(failed)
  match (failed_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("unexpected-cancel") }
      }
    }
  }
  val cancelled = spawn { val value: String = await nested(true) return value }
  cancel(cancelled)
  val cancelled_result: Result<String, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#;
        let file = parse_file(source).expect("parse String CFG await fixture");
        let generated = emit_c_from_ast(&file).expect("emit String CFG await fixture");
        assert!(generated.contains("aura async general CFG String lowering"));
        assert!(generated.contains("aura_async_result_destroy_"));
        assert!(generated.contains("data->value__owned"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile String CFG await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run String CFG await fixture");
        assert!(
            output.status.success(),
            "String CFG await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "cfg-string\ncfg-string\nstring-failure\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn lowers_general_cfg_caller_owned_task_parameter_without_static_release() {
        let source = r#"package demo
async fun nested(flag: Bool, task: Task<Int>): Int {
  var total: Int = 0
  if (flag) {
    var i: Int = 0
    while (i < 1) {
      val value: Int = await task
      total = total + value
      i = i + 1
    }
  }
  return total
}
fun main() {}
"#;
        let file = parse_file(source).expect("parse caller-owned CFG fixture");
        let generated = emit_c_from_ast(&file).expect("emit caller-owned CFG fixture");
        assert!(generated.contains("/* aura async general CFG Int lowering"));
        assert!(generated.contains("data->await_task_owned = false"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-owned-task-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile caller-owned CFG fixture");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_foreign_handle_parameter_across_multiple_awaits() {
        let path = format!("/tmp/aura-general-cfg-handle-{}-data", std::process::id());
        let source = format!(
            r#"package std.io
enum TaskError {{ case Failed(error: String) case Cancelled }}
enum Result<T, E> {{ case Ok(value: T) case Err(error: E) }}
fun openFile(path: String, mode: Int): ForeignHandle<Int> {{ throw "intrinsic" }}
async fun writeFile(file: ForeignHandle<Int>, content: String): Int {{ return 0 }}
async fun writeTwice(file: ForeignHandle<Int>): Int {{
  var total: Int = 0
  if (true) {{
    var i: Int = 0
    while (i < 2) {{
      val count: Int = await writeFile(file, "x")
      total = total + count
      i = i + 1
    }}
  }}
  return total
}}
fun main() {{
  val output: ForeignHandle<Int> = openFile("{path}", 1)
  val task = spawn {{
    val count: Int = await writeTwice(output)
    return count
  }}
  gc_collect()
  val first: Result<Int, TaskError> = join(task)
  match (first) {{
    case Ok(value) => {{ println(value.toString()) }}
    case Err(error) => {{ println("failed") }}
  }}
  val second: Result<Int, TaskError> = join(task)
  match (second) {{
    case Ok(value) => {{ println(value.toString()) }}
    case Err(error) => {{ println("failed") }}
  }}
}}
"#
        );
        let file = parse_file(&source).expect("parse general CFG handle fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG handle fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("aura_ffi_handle_retain(data->file)"));
        assert!(generated.contains("aura_ffi_handle_drop(&data->file)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-handle-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG handle fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG handle fixture");
        assert!(
            output.status.success(),
            "general CFG handle fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_and_runs_async_foreign_handle_throw_catch_with_ownership() {
        let path = format!("/tmp/aura-async-handle-catch-{}-data", std::process::id());
        let source = format!(
            r#"package std.io
enum TaskError {{ case Failed(error: String) case Cancelled }}
enum Result<T, E> {{ case Ok(value: T) case Err(error: E) }}
fun openFile(path: String, mode: Int): ForeignHandle<Int> {{ throw "intrinsic" }}
async fun tick(): Unit {{ }}
async fun fail(file: ForeignHandle<Int>): Int {{ await tick() throw file }}
async fun recover(file: ForeignHandle<Int>): Int {{
  try {{
    val ignored: Int = await fail(file)
    return 0
  }} catch (error: ForeignHandle<Int>) {{
    return 1
  }}
}}
fun main() {{
  val file: ForeignHandle<Int> = openFile("{path}", 1)
  val task = spawn {{ val result: Int = await recover(file) return result }}
  val outcome: Result<Int, TaskError> = join(task)
  match (outcome) {{
    case Ok(value) => {{ println(value.toString()) }}
    case Err(error) => {{ println("failed") }}
  }}
}}
"#
        );
        let file = parse_file(&source).expect("parse foreign handle throw/catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit foreign handle throw/catch fixture");
        assert!(generated.contains("aura_async_cfg_foreign_error_clone"));
        assert!(generated.contains("aura_ffi_handle_retain(__throw_handle)"));
        assert!(generated.contains("aura_task_frame_error_payload(data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-handle-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile foreign handle throw/catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run foreign handle throw/catch fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_and_runs_sync_foreign_handle_throw_catch_with_ownership() {
        let path = format!("/tmp/aura-sync-handle-catch-{}-data", std::process::id());
        let source = format!(
            r#"package std.io
fun openFile(path: String, mode: Int): ForeignHandle<Int> {{ throw "intrinsic" }}
fun recover(file: ForeignHandle<Int>): Int {{
  try {{ throw file }} catch (error: ForeignHandle<Int>) {{ return 1 }}
}}
fun main() {{
  val file: ForeignHandle<Int> = openFile("{path}", 1)
  println(recover(file).toString())
}}
"#
        );
        let file = parse_file(&source).expect("parse sync foreign handle throw/catch fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit sync foreign handle throw/catch fixture");
        assert!(generated.contains("aura_destroy_foreign_handle_payload"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-sync-handle-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile sync foreign handle throw/catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run sync foreign handle throw/catch fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_and_runs_general_cfg_caller_owned_foreign_handle_task_outcomes() {
        let path = format!(
            "/tmp/aura-general-cfg-owned-task-handle-{}-data",
            std::process::id()
        );
        let source = format!(
            r#"package std.io
enum TaskError {{ case Failed(error: String) case Cancelled }}
enum Result<T, E> {{ case Ok(value: T) case Err(error: E) }}
fun openFile(path: String, mode: Int): ForeignHandle<Int> {{ throw "intrinsic" }}
fun accept(outcome: Result<ForeignHandle<Int>, TaskError>) {{ }}
async fun leaf(file: ForeignHandle<Int>): ForeignHandle<Int> {{ return file }}
async fun fail(): ForeignHandle<Int> {{ throw "handle-failure" }}
async fun nested(seed: ForeignHandle<Int>, task: Task<ForeignHandle<Int>>): Int {{
  var result: ForeignHandle<Int> = seed
  var i: Int = 0
  while (i < 2) {{
    val value: ForeignHandle<Int> = await task
    result = value
    gc_collect()
    i = i + 1
  }}
  return i
}}
fun main() {{
  val success = spawn {{ val value: Int = await nested(openFile("{path}", 1), leaf(openFile("{path}", 1))) return value }}
  val first: Result<Int, TaskError> = join(success)
  match (first) {{
    case Ok(value) => {{ println("ok") }}
    case Err(error) => {{ println("success-failed") }}
  }}
  val second: Result<Int, TaskError> = join(success)
  match (second) {{
    case Ok(value) => {{ println("ok") }}
    case Err(error) => {{ println("success-failed") }}
  }}
  val failed = spawn {{ val value: Int = await nested(openFile("{path}", 1), fail()) return value }}
  val failed_result: Result<Int, TaskError> = join(failed)
  match (failed_result) {{
    case Ok(value) => {{ println("unexpected-success") }}
    case Err(error) => {{
      match (error) {{
        case Failed(message) => {{ println(message) }}
        case Cancelled => {{ println("unexpected-cancel") }}
      }}
    }}
  }}
  val cancelled = spawn {{ val value: Int = await nested(openFile("{path}", 1), leaf(openFile("{path}", 1))) return value }}
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {{
    case Ok(value) => {{ println("unexpected-success") }}
    case Err(error) => {{
      match (error) {{
        case Failed(message) => {{ println("unexpected-failure") }}
        case Cancelled => {{ println("cancelled") }}
      }}
    }}
  }}
}}
"#
        );
        let file = parse_file(&source).expect("parse caller-owned handle CFG fixture");
        let generated = emit_c_from_ast(&file).expect("emit caller-owned handle CFG fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("data->await_task_owned = false"));
        assert!(generated.contains("aura_ffi_handle_drop(&result)"));
        assert!(
            generated.contains("aura_var_std_io_Result_ForeignHandle_Int_std_io_TaskError_OkOwned")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-owned-task-handle-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile caller-owned handle CFG fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run caller-owned handle CFG fixture");
        assert!(
            output.status.success(),
            "caller-owned handle CFG fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "ok\nok\nhandle-failure\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_and_runs_general_cfg_caller_owned_string_task_outcomes() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun leaf(value: String): String { return value }
async fun fail(): String { throw "caller-owned-failure" }
async fun nested(flag: Bool, task: Task<String>): String {
  var result: String = "seed"
  if (flag) {
    var i: Int = 0
    while (i < 2) {
      val value: String = await task
      result = value
      i = i + 1
    }
  }
  gc_collect()
  return result
}
fun main() {
  val success = spawn {
    val value: String = await nested(true, leaf("caller-owned"))
    return value
  }
  val first: Result<String, TaskError> = join(success)
  match (first) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("success-failed") }
  }
  val second: Result<String, TaskError> = join(success)
  match (second) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("success-failed") }
  }
  val failed = spawn {
    val value: String = await nested(true, fail())
    return value
  }
  val failed_result: Result<String, TaskError> = join(failed)
  match (failed_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("unexpected-cancel") }
      }
    }
  }
  val cancelled = spawn {
    val value: String = await nested(true, leaf("cancelled"))
    return value
  }
  cancel(cancelled)
  val cancelled_result: Result<String, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#;
        let file = parse_file(source).expect("parse caller-owned String CFG fixture");
        let generated = emit_c_from_ast(&file).expect("emit caller-owned String CFG fixture");
        assert!(generated.contains("aura async general CFG String lowering"));
        assert!(generated.contains("data->await_task_owned = false"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated.contains("aura_task_frame_cancel_requested(frame)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-owned-string-task-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile caller-owned String CFG fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run caller-owned String CFG fixture");
        assert!(
            output.status.success(),
            "caller-owned String CFG fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "caller-owned\ncaller-owned\ncaller-owned-failure\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_repeated_caller_owned_array_task_outcomes() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun leaf(): Array<String> {
  val result: Array<String> = Array<String>(0)
  result.push("caller-owned-array")
  return result
}
async fun fail(): Array<String> { throw "caller-owned-array-failure" }
async fun nested(task: Task<Array<String>>): Array<String> {
  var result: Array<String> = Array<String>(0)
  var i: Int = 0
  while (i < 2) {
    val value: Array<String> = await task
    result = value
    gc_collect()
    i = i + 1
  }
  return result
}
fun main() {
  val success = spawn { val value: Array<String> = await nested(leaf()) return value }
  val first: Result<Array<String>, TaskError> = join(success)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("success-failed") }
  }
  val second: Result<Array<String>, TaskError> = join(success)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("success-failed") }
  }
  val failed = spawn { val value: Array<String> = await nested(fail()) return value }
  val failed_result: Result<Array<String>, TaskError> = join(failed)
  match (failed_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("unexpected-cancel") }
      }
    }
  }
  val cancelled = spawn { val value: Array<String> = await nested(leaf()) return value }
  cancel(cancelled)
  val cancelled_result: Result<Array<String>, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#;
        let file = parse_file(source).expect("parse repeated caller-owned Array CFG fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit repeated caller-owned Array CFG fixture");
        assert!(generated.contains("aura async general CFG Array lowering"));
        assert!(generated.contains("data->await_task_owned = false"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-owned-array-task-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile repeated caller-owned Array CFG fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run repeated caller-owned Array CFG fixture");
        assert!(
            output.status.success(),
            "repeated caller-owned Array CFG fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "1\n1\ncaller-owned-array-failure\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_for_range_await_with_gc_repeated_join_and_cancel() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int): Int { return value }
async fun sum(): Int {
  var total: Int = 0
  for (i in 1..4) {
    val value: Int = await worker(i)
    total = total + value
    gc_collect()
  }
  return total
}
fun main() {
  val task = spawn { val value: Int = await sum() return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val cancelled = spawn { val value: Int = await sum() return value }
  cancel(cancelled)
  join(cancelled)
}
"#;
        let file = parse_file(source).expect("parse for-range await fixture");
        let generated = emit_c_from_ast(&file).expect("emit for-range await fixture");
        assert!(
            generated.contains("/* aura async for-range-await Int lowering */")
                || generated.contains("/* aura async general CFG Int lowering")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 1)"));
        assert!(generated.contains("data->await_task = aura_fn_std_io_worker(i);"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-for-range-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile for-range await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run for-range await fixture");
        assert!(
            output.status.success(),
            "for-range await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "6\n6\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_loop_branch_join_with_gc_and_cancellation() {
        let source = r#"package demo
async fun worker(value: Int): Int { return value }
async fun sum(flag: Bool, limit: Int): Int {
  var i: Int = 0
  var total: Int = 0
  var value: Int = 0
  while (i < limit) {
    if (flag) { value = await worker(i) }
    else { value = await worker(i + 1) }
    gc_collect()
    total = total + value
    i = i + 1
  }
  return total
}
fun main() {
  val first = spawn { val value: Int = await sum(true, 3) println(value.toString()) return }
  join(first)
  val second = spawn { val value: Int = await sum(false, 3) println(value.toString()) return }
  join(second)
  val cancelled = spawn { val value: Int = await sum(true, 3) println("unexpected-cancel") return }
  cancel(cancelled)
  join(cancelled)
}
"#;
        let file = parse_file(source).expect("parse loop branch-join fixture");
        let generated = emit_c_from_ast(&file).expect("emit loop branch-join fixture");
        assert!(
            generated.contains("aura async loop branch-join suspension states=2")
                || generated.contains("aura async general CFG")
        );
        assert!(
            generated.contains("aura_async_loop_branch_head")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 2)"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated.contains("aura_gc_collect"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-loop-branch-join-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile loop branch-join fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run loop branch-join fixture");
        assert!(
            output.status.success(),
            "loop branch-join fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n6\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_loop_branch_array_payload_with_repeated_join_and_cancel() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun left(size: Int): Array<Int> { return Array<Int>(size) }
async fun right(size: Int): Array<Int> { return Array<Int>(size + 1) }
async fun collect(flag: Bool, limit: Int): Array<Int> {
  var i: Int = 0
  var value: Array<Int> = Array<Int>(0)
  while (i < limit) {
    if (flag) { value = await left(i + 1) }
    else { value = await right(i + 1) }
    gc_collect()
    i = i + 1
  }
  return value
}
fun main() {
  val task = spawn { val value: Array<Int> = await collect(true, 3) return value }
  val first: Result<Array<Int>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Array<Int>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val cancelled = spawn { val value: Array<Int> = await collect(false, 3) return value }
  cancel(cancelled)
  val outcome: Result<Array<Int>, TaskError> = join(cancelled)
  match (outcome) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => { println("cancelled") }
  }
}
"#,
        )
        .expect("parse loop Array payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit loop Array payload fixture");
        assert!(
            generated.contains("aura async loop branch-join Array suspension states=2")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_method_Array_Int_clone"));
        assert!(generated.contains("aura_async_destroy_std_io_collect"));
        assert!(generated.contains("aura_async_data_drop_std_io_collect"));
        assert!(generated.contains("aura_async_gc_mark_std_io_collect"));
        assert!(generated.contains("aura_task_frame_set_data_drop(frame"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 2)"));
        assert!(generated.contains("aura_gc_collect"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-loop-array-branch-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile loop Array payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run loop Array payload fixture");
        assert!(
            output.status.success(),
            "loop Array payload fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n3\ncancelled\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_loop_branch_array_enum_payload_with_gc() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Item { case Text(value: String) }
async fun left(size: Int): Array<Item> {
  val values: Array<Item> = Array<Item>(size)
  if (size > 0) { values.set(0, Text("left")) }
  return values
}
async fun right(size: Int): Array<Item> {
  val values: Array<Item> = Array<Item>(size + 1)
  if (size > 0) { values.set(0, Text("right")) }
  return values
}
async fun collect(flag: Bool, limit: Int): Array<Item> {
  var i: Int = 0
  var value: Array<Item> = Array<Item>(0)
  while (i < limit) {
    if (flag) { value = await left(i + 1) }
    else { value = await right(i + 1) }
    gc_collect()
    i = i + 1
  }
  return value
}
fun main() {
  val task = spawn { val value: Array<Item> = await collect(true, 3) return value }
  val first: Result<Array<Item>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Array<Item>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
}
"#,
        )
        .expect("parse loop Array<enum> payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit loop Array<enum> payload fixture");
        assert!(generated.contains("aura_async_gc_mark_std_io_collect"));
        assert!(generated.contains("aura_cls_Array_std_io_Item_mark"));
        assert!(generated.contains("aura_enum_std_io_Item_mark"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-loop-array-enum-branch-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile loop Array<enum> payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run loop Array<enum> payload fixture");
        assert!(
            output.status.success(),
            "loop Array<enum> fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_loop_with_two_conditional_await_states() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun first_worker(value: Int): Int { return value }
async fun second_worker(value: Int): Int { return value + 10 }
async fun sum(first_flag: Bool, second_flag: Bool, limit: Int): Int {
  var i: Int = 0
  var total: Int = 0
  var first: Int = 0
  var second: Int = 0
  while (i < limit) {
    if (first_flag) { first = await first_worker(i) }
    if (second_flag) { second = await second_worker(i) }
    total = total + first + second
    gc_collect()
    i = i + 1
  }
  return total
}
fun main() {
  val both = spawn { val value: Int = await sum(true, true, 2) return value }
  val both_first: Result<Int, TaskError> = join(both)
  match (both_first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val both_second: Result<Int, TaskError> = join(both)
  match (both_second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val first_only = spawn { val value: Int = await sum(true, false, 2) return value }
  val first_result: Result<Int, TaskError> = join(first_only)
  match (first_result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val second_only = spawn { val value: Int = await sum(false, true, 2) return value }
  val second_result: Result<Int, TaskError> = join(second_only)
  match (second_result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val neither = spawn { val value: Int = await sum(false, false, 2) return value }
  val neither_result: Result<Int, TaskError> = join(neither)
  match (neither_result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val cancelled = spawn { val value: Int = await sum(true, true, 5) return value }
  cancel(cancelled)
  join(cancelled)
}
"#,
        )
        .expect("parse two conditional await fixture");
        let generated = emit_c_from_ast(&file).expect("emit two conditional await fixture");
        assert!(
            generated.contains("aura async loop two-conditional suspension states=3")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 1)"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 2)"));
        assert!(
            generated.contains("data->await_task_1")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_gc_collect"));
        assert!(
            generated.contains("aura_task_frame_propagate_error(frame, data->await_task_0)")
                || generated.contains("aura async general CFG")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-loop-two-conditional-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile two conditional await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run two conditional await fixture");
        assert!(
            output.status.success(),
            "two conditional await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "22\n22\n1\n21\n0\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_for_range_await_with_gc_repeated_join_and_cancel() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int): Int { return value }
async fun sum(limit: Int): Int {
  var total: Int = 0
  for (i in 0..limit) {
    val value: Int = await worker(i)
    total = total + value
    gc_collect()
  }
  return total
}
fun main() {
  val task = spawn { val value: Int = await sum(5) return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
  val cancelled = spawn { val value: Int = await sum(100) return value }
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Cancelled => { println("cancelled") }
        case Failed(message) => { println(message) }
      }
    }
  }
}
"#,
        )
        .expect("parse general CFG for-range await fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG for-range await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("__aura_range_end_0"));
        assert!(generated.contains("INT64_C(1)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-for-range-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG for-range await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG for-range await fixture");
        assert!(
            output.status.success(),
            "general CFG for-range await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "10\n10\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_for_in_array_await_with_gc_repeated_join_and_cancel() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int, shouldFail: Bool): Int {
  if (shouldFail) { throw "for-in-failure" }
  return 1
}
async fun sum(values: Array<Int>, shouldFail: Bool): Int {
  var total: Int = 0
  for (item in values) {
    val value: Int = await worker(item, shouldFail)
    total = total + value
    gc_collect()
  }
  return total
}
fun main() {
  val task = spawn { val value: Int = await sum(Array<Int>(4), false) return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
  val failed = spawn { val value: Int = await sum(Array<Int>(2), true) return value }
  val failed_result: Result<Int, TaskError> = join(failed)
  match (failed_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("unexpected-cancel") }
      }
    }
  }
  val cancelled = spawn { val value: Int = await sum(Array<Int>(100), false) return value }
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Cancelled => { println("cancelled") }
        case Failed(message) => { println(message) }
      }
    }
  }
}
"#,
        )
        .expect("parse general CFG for-in await fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG for-in await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("__aura_for_iter_0"));
        assert!(generated.contains("__aura_for_index_0"));
        assert!(generated.contains("aura_method_Array_Int_get"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-for-in-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG for-in await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG for-in await fixture");
        assert!(
            output.status.success(),
            "general CFG for-in await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "4\n4\nfor-in-failure\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_for_in_string_await_with_repeated_join_and_cancel() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int): Int { return value }
async fun sum(values: String): Int {
  var total: Int = 0
  for (item in values) {
    val value: Int = await worker(item)
    total = total + value
  }
  return total
}
fun main() {
  val task = spawn { val value: Int = await sum("Aura") return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
  val cancelled = spawn { val value: Int = await sum("long-string") return value }
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Cancelled => { println("cancelled") }
        case Failed(message) => { println(message) }
      }
    }
  }
}
"#,
        )
        .expect("parse general CFG String for-in await fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit general CFG String for-in await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("strlen(__aura_for_iter_0)"));
        assert!(generated.contains("(unsigned char)__aura_for_iter_0[__aura_for_index_0]"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-for-in-string-await-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG String for-in await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG String for-in await fixture");
        assert!(
            output.status.success(),
            "general CFG String for-in await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "393\n393\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_for_in_interface_await_with_repeated_join_and_cancel() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
interface Iterable {
  fun len(): Int
  fun get(i: Int): Int
}
class Range(val n: Int) : Iterable {
  fun len(): Int { return this.n }
  fun get(i: Int): Int { return i }
}
async fun worker(value: Int): Int { return value }
async fun sum(values: Iterable): Int {
  var total: Int = 0
  for (item in values) {
    val value: Int = await worker(item)
    total = total + value
  }
  return total
}
fun main() {
  val task = spawn { val value: Int = await sum(Range(4)) return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
  val cancelled = spawn { val value: Int = await sum(Range(100)) return value }
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Cancelled => { println("cancelled") }
        case Failed(message) => { println(message) }
      }
    }
  }
}
"#,
        )
        .expect("parse general CFG interface for-in await fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit general CFG interface for-in await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("aura_iface_std_io_Iterable_len"));
        assert!(generated.contains("aura_iface_std_io_Iterable_get"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-for-in-interface-await-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG interface for-in await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG interface for-in await fixture");
        assert!(
            output.status.success(),
            "general CFG interface for-in await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "6\n6\ncancelled\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_owned_interface_task_result_across_repeated_join_and_gc() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
interface Named { fun value(): Int }
class Box(val n: Int) : Named { fun value(): Int { return this.n } }
async fun produce(): Named { return Box(41) }
fun main() {
  val task = spawn { val item: Named = Box(41) return item }
  val first: Result<Named, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.value().toString()) }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Named, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.value().toString()) }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse owned interface task result fixture");
        let generated = emit_c_from_ast(&file).expect("emit owned interface task result fixture");
        assert!(generated.contains("aura_iface_std_io_Named_clone"));
        assert!(generated.contains("aura_var_std_io_Result_std_io_Named_std_io_TaskError_OkOwned"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-owned-interface-task-result-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile owned interface task result fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run owned interface task result fixture");
        assert!(
            output.status.success(),
            "owned interface task result failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "41\n41\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_for_in_string_array_await_with_repeated_join_and_cancel() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: String): Int { return value.len }
async fun sum(values: Array<String>): Int {
  var total: Int = 0
  for (item in values) {
    val value: Int = await worker(item)
    total = total + value
  }
  return total
}
fun main() {
  val values: Array<String> = Array<String>(0)
  values.push("a")
  values.push("bb")
  values.push("ccc")
  val task = spawn { val value: Int = await sum(values) return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
  val cancelled = spawn { val value: Int = await sum(values) return value }
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Cancelled => { println("cancelled") }
        case Failed(message) => { println(message) }
      }
    }
  }
}
"#,
        )
        .expect("parse general CFG String array for-in await fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit general CFG String array for-in await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-for-in-string-array-await-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG String array for-in await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG String array for-in await fixture");
        assert!(
            output.status.success(),
            "general CFG String array for-in await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "6\n6\ncancelled\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_for_in_enum_array_await_with_gc() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Item { case Text(value: String) }
async fun size(item: Item): Int {
  match (item) {
    case Text(text) => { return text.len }
  }
}
async fun sum(values: Array<Item>): Int {
  var total: Int = 0
  for (item in values) {
    val value: Int = await size(item)
    total = total + value
    gc_collect()
  }
  return total
}
fun main() {
  val values: Array<Item> = Array<Item>(0)
  values.push(Text("a"))
  values.push(Text("bb"))
  val task = spawn { val value: Int = await sum(values) return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  gc_collect()
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse enum-array for-in await fixture");
        let generated = emit_c_from_ast(&file).expect("emit enum-array for-in await fixture");
        assert!(generated.contains("Item_clone"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-enum-array-for-in-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile enum-array for-in await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run enum-array for-in await fixture");
        assert!(
            output.status.success(),
            "enum-array for-in await failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_loop_with_three_conditional_await_states() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int): Int { return value }
async fun sum(): Int {
  var i: Int = 0
  var total: Int = 0
  var first: Int = 0
  var second: Int = 0
  var third: Int = 0
  while (i < 3) {
    if (i < 1) { first = await worker(10) }
    if (i < 2) { second = await worker(20) }
    if (i < 3) { third = await worker(30) }
    total = total + first + second + third
    gc_collect()
    i = i + 1
  }
  return total
}
fun main() {
  val task = spawn { val value: Int = await sum() return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val cancelled = spawn { val value: Int = await sum() return value }
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse three conditional await fixture");
        let generated = emit_c_from_ast(&file).expect("emit three conditional await fixture");
        assert!(
            generated.contains("aura async loop multi-conditional suspension states=4 branches=3")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 3)"));
        assert!(
            generated.contains("data->await_task_2")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_gc_collect"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-loop-three-conditional-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile three conditional await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run three conditional await fixture");
        assert!(
            output.status.success(),
            "three conditional await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "180\n180\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_loop_with_four_conditional_await_states() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int): Int { return value }
async fun sum(): Int {
  var i: Int = 0
  var total: Int = 0
  var first: Int = 0
  var second: Int = 0
  var third: Int = 0
  var fourth: Int = 0
  while (i < 4) {
    if (i < 1) { first = await worker(10) }
    if (i < 2) { second = await worker(20) }
    if (i < 3) { third = await worker(30) }
    if (i < 4) { fourth = await worker(40) }
    total = total + first + second + third + fourth
    gc_collect()
    i = i + 1
  }
  return total
}
fun main() {
  val task = spawn { val value: Int = await sum() return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val cancelled = spawn { val value: Int = await sum() return value }
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse four conditional await fixture");
        let generated = emit_c_from_ast(&file).expect("emit four conditional await fixture");
        assert!(
            generated.contains("aura async loop multi-conditional suspension states=5 branches=4")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 4)"));
        assert!(
            generated.contains("data->await_task_3")
                || generated.contains("aura async general CFG")
        );
        assert!(
            generated
                .contains("aura_task_executor_release(__aura_task_executor, &data->await_task_3)")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_gc_collect"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-loop-four-conditional-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile four conditional await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run four conditional await fixture");
        assert!(
            output.status.success(),
            "four conditional await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "400\n400\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_branch_then_second_await_state_machine() {
        let source = r#"package demo
async fun worker(value: Int): Int {
  println(value.toString())
  return value
}
async fun combine(flag: Bool, first: Task<Int>, second: Task<Int>): Int {
  if (flag) {
    val left: Int = await first
  }
  gc_collect()
  val right: Int = await second
  return right
}
fun main() {
  val task = spawn {
    val result: Int = await combine(true, worker(1), worker(2))
    println(result.toString())
    return
  }
  join(task)
  val skipped = spawn {
    val result: Int = await combine(false, worker(3), worker(4))
    println(result.toString())
    return
  }
  join(skipped)
  val cancelled = spawn {
    val result: Int = await combine(true, worker(5), worker(6))
    println("unexpected-cancel")
    return
  }
  cancel(cancelled)
  join(cancelled)
}
"#;
        let file = parse_file(source).expect("parse branch-then-second-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit branch-then-second-await fixture");
        assert!(
            generated.contains("aura async general CFG Int lowering")
                || generated.contains("aura async branch-then-multi suspension states=2")
        );
        assert!(
            generated.contains("AuraTaskFrame *await_task_0;")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(
            generated.contains("AuraTaskFrame *await_task_1;")
                || generated.contains("aura async general CFG Int lowering")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 2)"));
        assert!(
            generated.contains("aura_task_frame_propagate_error(frame, data->await_task_0)")
                || generated.contains("aura async general CFG Int lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-branch-then-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile branch-then-second-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run branch-then-second-await fixture");
        assert!(
            output.status.success(),
            "branch-then-second-await fixture failed: {output:?}"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("1\n"));
        assert!(stdout.contains("2\n2\n"));
        assert!(stdout.contains("4\n4\n"));
        assert!(stdout.contains("3\n"));
        assert!(!stdout.contains("5\n"));
        assert!(!stdout.contains("6\n"));
        assert!(!stdout.contains("unexpected-cancel"));
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_branch_join_await_state_machine() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun choose(flag: Bool): Int {
  if (flag) {
    val value: Int = await yes()
    return value
  } else {
    val value: Int = await no()
    return value
  }
}
async fun yes(): Int { println("yes") return 7 }
async fun no(): Int { println("no") return 9 }
async fun driver(flag: Bool): Int {
  val value: Int = await choose(flag)
  return value
}
fun main() {
  val first = spawn { val value: Int = await driver(true) return }
  join(first)
  val second = spawn { val value: Int = await driver(false) return }
  join(second)
}
"#,
        )
        .expect("parse branch-join await fixture");
        let generated = emit_c_from_ast(&file).expect("emit branch-join await fixture");
        assert!(generated.contains("aura async branch-join suspension state=1"));
        assert!(generated.contains("bool selected_then;"));
        assert!(generated.contains("data->selected_then ?"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated
            .contains("aura_task_executor_release(__aura_task_executor, &data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-await-branch-join-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile branch-join await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run branch-join await fixture");
        assert!(
            output.status.success(),
            "branch-join fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "yes\nno\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_branch_join_array_payload_with_repeated_join_and_cancel() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun left(): Array<Int> { return Array<Int>(2) }
async fun right(): Array<Int> { return Array<Int>(3) }
async fun choose(flag: Bool): Array<Int> {
  if (flag) {
    val value: Array<Int> = await left()
    return value
  } else {
    val value: Array<Int> = await right()
    return value
  }
}
fun main() {
  val task = spawn { val value: Array<Int> = await choose(true) return value }
  val first: Result<Array<Int>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Array<Int>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val cancelled = spawn { val value: Array<Int> = await choose(false) return value }
  cancel(cancelled)
  val outcome: Result<Array<Int>, TaskError> = join(cancelled)
  match (outcome) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => { println("cancelled") }
  }
}
"#,
        )
        .expect("parse branch Array payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit branch Array payload fixture");
        assert!(generated.contains("aura async branch-join suspension state=1"));
        assert!(generated.contains("aura_method_Array_Int_clone"));
        assert!(generated.contains("aura_async_result_destroy_"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-branch-array-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile branch Array payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run branch Array payload fixture");
        assert!(
            output.status.success(),
            "branch Array payload fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n2\ncancelled\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_branch_join_with_common_continuation() {
        let source = r#"package demo
async fun worker(value: Int): Int { return value }
async fun combine(flag: Bool, first: Task<Int>, second: Task<Int>): Int {
  var value: Int = 0
  if (flag) { value = await first }
  else { value = await second }
  gc_collect()
  return value + 1
}
fun main() {
  val first = spawn { val value: Int = await combine(true, worker(1), worker(2)) println(value.toString()) return }
  join(first)
  join(first)
  val second = spawn { val value: Int = await combine(false, worker(3), worker(4)) println(value.toString()) return }
  join(second)
  val cancelled = spawn { val value: Int = await combine(true, worker(5), worker(6)) println("unexpected-cancel") return }
  cancel(cancelled)
  join(cancelled)
}
"#;
        let file = parse_file(source).expect("parse branch-join continuation fixture");
        let generated = emit_c_from_ast(&file).expect("emit branch-join continuation fixture");
        assert!(
            generated.contains("aura async branch-join continuation states=1")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 1)"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated.contains("aura_task_frame_cancel_requested(frame)"));
        assert!(generated.contains("aura_gc_collect"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-branch-join-continuation-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile branch-join continuation fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run branch-join continuation fixture");
        assert!(
            output.status.success(),
            "branch-join continuation fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n5\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_string_branch_join_with_owned_continuation() {
        let source = r#"package demo
async fun worker(value: String): String { return value }
async fun combine(flag: Bool, first: Task<String>, second: Task<String>): String {
  var value: String = "initial"
  if (flag) { value = await first }
  else { value = await second }
  gc_collect()
  return value
}
fun main() {
  val first = spawn { val value: String = await combine(true, worker("one"), worker("two")) println(value) return }
  join(first)
  join(first)
  val second = spawn { val value: String = await combine(false, worker("three"), worker("four")) println(value) return }
  join(second)
}
"#;
        let file = parse_file(source).expect("parse String branch-join fixture");
        let generated = emit_c_from_ast(&file).expect("emit String branch-join fixture");
        assert!(
            generated.contains("aura async branch-join continuation states=1")
                || generated.contains("aura async general CFG")
        );
        assert!(generated.contains("bool value__owned;"));
        assert!(
            generated.contains("strlen(__returned)")
                || generated.contains("aura async general CFG String lowering")
        );
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-string-branch-join-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile String branch-join fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run String branch-join fixture");
        assert!(
            output.status.success(),
            "String branch-join fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "one\nfour\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_match_await_state_machine_with_typed_outcomes() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Choice { case First case Second }
async fun leaf(value: Int): Int { return value }
async fun fail(): Int { throw "match-failure" }
async fun choose(choice: Choice, first: Task<Int>, second: Task<Int>): Int {
  var value: Int = 0
  match (choice) {
    case First => { val first_value: Int = await first value = first_value }
    case Second => { val second_value: Int = await second value = second_value }
  }
  gc_collect()
  return value
}
fun main() {
  val first = spawn { val value: Int = await choose(First(), leaf(11), leaf(22)) return value }
  val first_result: Result<Int, TaskError> = join(first)
  match (first_result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("first-failed") }
  }
  val first_again: Result<Int, TaskError> = join(first)
  match (first_again) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("first-repeat-failed") }
  }
  val failed = spawn { val value: Int = await choose(Second(), leaf(33), fail()) return value }
  val failed_result: Result<Int, TaskError> = join(failed)
  match (failed_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("unexpected-cancel") }
      }
    }
  }
  val cancelled = spawn { val value: Int = await choose(First(), leaf(44), leaf(55)) return value }
  cancel(cancelled)
  val cancelled_result: Result<Int, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse match-await state machine fixture");
        let generated = emit_c_from_ast(&file).expect("emit match-await state machine fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains(".tag == 0"));
        assert!(generated.contains(".tag == 1"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-match-await-state-machine-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile match-await state machine fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run match-await state machine fixture");
        assert!(
            output.status.success(),
            "match-await state machine fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "11\n11\nmatch-failure\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_match_await_state_machine_with_int_bindings() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Choice { case First(value: Int) case Second(value: Int) }
async fun leaf(value: Int): Int { return value }
async fun choose(choice: Choice, task: Task<Int>): Int {
  var answer: Int = 0
  match (choice) {
    case First(seed) => { val first_value: Int = await task answer = seed + first_value }
    case Second(seed) => { val second_value: Int = await task answer = seed - second_value }
  }
  gc_collect()
  return answer
}
fun main() {
  val task = spawn { val value: Int = await choose(First(10), leaf(3)) return value }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse match-await binding fixture");
        let generated = emit_c_from_ast(&file).expect("emit match-await binding fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("data->seed"));
        assert!(generated.contains("data->first_value"));
        assert!(generated.contains(".data.First.value"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-match-await-bindings-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile match-await binding fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run match-await binding fixture");
        assert!(
            output.status.success(),
            "match-await binding fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "13\n13\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_match_await_state_machine_with_string_bindings() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Choice { case First(value: String) case Second(value: String) }
async fun leaf(value: String): String { return value }
async fun choose(choice: Choice, task: Task<String>): String {
  match (choice) {
    case First(seed) => { val first_value: String = await task gc_collect() return seed + first_value }
    case Second(seed) => { val second_value: String = await task gc_collect() return seed + second_value }
  }
  return "unreachable"
}
fun main() {
  val task = spawn { val value: String = await choose(First("seed-"), leaf("value")) return value }
  val first: Result<String, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("failed") }
  }
  val second: Result<String, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse string match-await binding fixture");
        let generated = emit_c_from_ast(&file).expect("emit string match-await binding fixture");
        assert!(generated.contains("aura async general CFG String lowering"));
        assert!(generated.contains("data->seed__owned"));
        assert!(generated.contains("malloc(__match_len + 1)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-match-await-string-bindings-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile string match-await binding fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run string match-await binding fixture");
        assert!(
            output.status.success(),
            "string match-await binding fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "seed-value\nseed-value\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_match_await_state_machine_with_array_bindings() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Choice { case First(value: Array<Int>) case Second(value: Array<Int>) }
async fun leaf(value: Array<Int>): Array<Int> { return value }
async fun choose(choice: Choice, task: Task<Array<Int>>): Array<Int> {
  match (choice) {
    case First(seed) => { val first_value: Array<Int> = await task gc_collect() return seed }
    case Second(seed) => { val second_value: Array<Int> = await task gc_collect() return seed }
  }
  return Array<Int>(0)
}
fun main() {
  val task = spawn { val value: Array<Int> = await choose(First(Array<Int>(3)), leaf(Array<Int>(1))) return value }
  val first: Result<Array<Int>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Array<Int>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse array match-await binding fixture");
        let generated = emit_c_from_ast(&file).expect("emit array match-await binding fixture");
        assert!(generated.contains("aura async general CFG Array lowering"));
        assert!(generated.contains("__match_value = choice.data.First.value"));
        assert!(generated.contains("data->seed.data"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-match-await-array-bindings-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile array match-await binding fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run array match-await binding fixture");
        assert!(
            output.status.success(),
            "array match-await binding fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_match_await_state_machine_with_string_array_bindings() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
enum Choice { case First(value: Array<String>) case Second(value: Array<String>) }
fun words(value: String): Array<String> {
  val result: Array<String> = Array<String>(0)
  result.push(value)
  return result
}
async fun leaf(value: Array<String>): Array<String> { return value }
async fun choose(choice: Choice, task: Task<Array<String>>): Array<String> {
  match (choice) {
    case First(seed) => { val first_value: Array<String> = await task gc_collect() return seed }
    case Second(seed) => { val second_value: Array<String> = await task gc_collect() return seed }
  }
  return Array<String>(0)
}
fun main() {
  val task = spawn { val value: Array<String> = await choose(First(words("seed")), leaf(words("value"))) return value }
  val first: Result<Array<String>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.get(0)) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Array<String>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.get(0)) }
    case Err(error) => { println("failed-repeat") }
  }
}
"#,
        )
        .expect("parse string array match-await binding fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit string array match-await binding fixture");
        assert!(generated.contains("aura async general CFG Array lowering"));
        assert!(generated.contains("__match_value = choice.data.First.value"));
        assert!(generated.contains("data->seed.data"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-match-await-string-array-bindings-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile string array match-await binding fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run string array match-await binding fixture");
        assert!(
            output.status.success(),
            "string array match-await binding fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "seed\nseed\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_match_await_state_machine_with_struct_bindings() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
struct Packet(val code: Int, val text: String) {}
enum Choice { case First(value: Packet) case Second(value: Packet) }
async fun leaf(value: Packet): Packet { return value }
async fun choose(choice: Choice, task: Task<Packet>): Packet {
  match (choice) {
    case First(seed) => { val next: Packet = await task gc_collect() return seed }
    case Second(seed) => { val next: Packet = await task gc_collect() return seed }
  }
  return Packet(0, "")
}
fun main() {
  val task = spawn { val value: Packet = await choose(First(Packet(37, "packet")), leaf(Packet(11, "child"))) return value }
  val first: Result<Packet, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.code.toString()) }
    case Err(error) => { println("failed") }
  }
  val second: Result<Packet, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.code.toString()) }
    case Err(error) => { println("failed-repeat") }
  }
  join(task)
}
"#,
        )
        .expect("parse struct match-await binding fixture");
        let generated = emit_c_from_ast(&file).expect("emit struct match-await binding fixture");
        assert!(generated.contains("aura_cls_std_io_Packet"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-match-await-struct-bindings-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile struct match-await binding fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run struct match-await binding fixture");
        assert!(
            output.status.success(),
            "struct match-await binding fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "37\n37\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_with_same_typed_branch_locals() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
struct Packet(val code: Int, val text: String) {}
async fun pick(flag: Bool, left: Task<Packet>, right: Task<Packet>): Packet {
  if (flag) {
    val value: Packet = await left
    return value
  } else {
    val value: Packet = await right
    return value
  }
  return Packet(0, "")
}
async fun leaf(value: Packet): Packet { return value }
fun main() {
  val left: Task<Packet> = leaf(Packet(41, "left"))
  val right: Task<Packet> = leaf(Packet(42, "right"))
  val task = spawn { val value: Packet = await pick(true, left, right) return value }
  val result: Result<Packet, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value.code.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse merged branch-local CFG fixture");
        let generated = emit_c_from_ast(&file).expect("emit merged branch-local CFG fixture");
        assert!(generated.contains("aura async general CFG Struct lowering"));
        assert!(generated.contains("value"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-merged-locals-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile merged branch-local CFG fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run merged branch-local CFG fixture");
        assert!(
            output.status.success(),
            "merged branch-local CFG fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "41\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_nested_branch_await_state_machine() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun leaf(value: Int): Int { return value }
async fun nested(outer: Bool, inner: Bool): Int {
  if (outer) {
    if (inner) { val value: Int = await leaf(1) return value }
    else { val value: Int = await leaf(2) return value }
  } else {
    if (inner) { val value: Int = await leaf(3) return value }
    else { val value: Int = await leaf(4) return value }
  }
}
fun main() {
  val first = spawn { val value: Int = await nested(true, true) println(value.toString()) return }
  join(first)
  val second = spawn { val value: Int = await nested(true, false) println(value.toString()) return }
  join(second)
  val third = spawn { val value: Int = await nested(false, true) println(value.toString()) return }
  join(third)
  val fourth = spawn { val value: Int = await nested(false, false) println(value.toString()) return }
  join(fourth)
}
"#,
        )
        .expect("parse nested branch-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested branch-await fixture");
        assert!(generated.contains("aura async nested-branch suspension states=1 leaves=4"));
        assert!(generated.contains("uint8_t selected_path;"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 1)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-branch-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested branch-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested branch-await fixture");
        assert!(
            output.status.success(),
            "nested branch-await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n2\n3\n4\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_single_await_string_payload() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun answer(): String { return "hello" }
async fun relay(): String {
  val value: String = await answer()
  println(value)
  return value
}
fun main() {
  relay()
}
"#,
        )
        .expect("parse single-await String fixture");
        let generated = emit_c_from_ast(&file).expect("emit single-await String fixture");
        assert!(
            generated.contains("strlen(__returned)")
                || generated.contains("aura async general CFG String lowering")
        );
        assert!(
            generated.contains("free((void *)*((const char **)data))")
                || generated.contains("aura async general CFG String lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-string-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile single-await String fixture");
        assert!(Command::new(&bin)
            .status()
            .expect("run single-await String fixture")
            .success());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_spawn_string_await_with_repeated_join() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun answer(): String { return "spawn-string" }
fun main() {
  val task = spawn {
    val value: String = await answer()
    println(value)
    return
  }
  join(task)
  join(task)
}
"#,
        )
        .expect("parse spawned String-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit spawned String-await fixture");
        assert!(generated.contains("const char * await_value;"));
        assert!(generated.contains("aura_task_executor_join_outcome"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-spawn-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile spawned String-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run spawned String-await fixture");
        assert!(
            output.status.success(),
            "spawned String-await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "spawn-string\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_typed_spawn_string_after_await_with_repeated_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun answer(): String { return "suspended-ready" }
fun main() {
  val task = spawn {
    val value: String = await answer()
    return value
  }
  val first: Result<String, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("unexpected-error") }
  }
  val second: Result<String, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("unexpected-error") }
  }
  val cancelled = spawn {
    val value: String = await answer()
    return value
  }
  cancel(cancelled)
  val cancelled_outcome: Result<String, TaskError> = join(cancelled)
  match (cancelled_outcome) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse suspended typed String spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit suspended typed String spawn fixture");
        assert!(generated.contains("typed suspended String result"));
        assert!(generated.contains("aura_spawn_result_destroy_"));
        assert!(generated.contains("aura_var_std_io_Result_String_std_io_TaskError_OkOwned"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-typed-spawn-await-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile suspended typed String spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run suspended typed String spawn fixture");
        assert!(
            output.status.success(),
            "suspended typed String spawn fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "suspended-ready\nsuspended-ready\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_typed_spawn_int_after_await_with_repeated_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun answer(): Int { return 9 }
fun main() {
  val task = spawn {
    val value: Int = await answer()
    return value
  }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
}
"#,
        )
        .expect("parse suspended typed Int spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit suspended typed Int spawn fixture");
        assert!(generated.contains("typed suspended Int result"));
        assert!(generated.contains("aura_var_std_io_Result_Int_std_io_TaskError_Ok"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-typed-spawn-await-int-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile suspended typed Int spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run suspended typed Int spawn fixture");
        assert!(
            output.status.success(),
            "suspended typed Int spawn fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "9\n9\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_typed_spawn_bool_after_await_with_repeated_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun answer(): Bool { return true }
fun main() {
  val task = spawn {
    val value: Bool = await answer()
    return value
  }
  val first: Result<Bool, TaskError> = join(task)
  match (first) {
    case Ok(value) => { if (value) { println("true") } }
    case Err(error) => { println("unexpected-error") }
  }
  val second: Result<Bool, TaskError> = join(task)
  match (second) {
    case Ok(value) => { if (value) { println("true") } }
    case Err(error) => { println("unexpected-error") }
  }
}
"#,
        )
        .expect("parse suspended typed Bool spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit suspended typed Bool spawn fixture");
        assert!(generated.contains("typed suspended Bool result"));
        assert!(generated.contains("aura_var_std_io_Result_Bool_std_io_TaskError_Ok"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-typed-spawn-await-bool-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile suspended typed Bool spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run suspended typed Bool spawn fixture");
        assert!(
            output.status.success(),
            "suspended typed Bool spawn fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "true\ntrue\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_string_branch_join_await_state_machine() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun answer(value: String): String { return value }
async fun choose(flag: Bool): String {
  if (flag) {
    val value: String = await answer("then")
    return value
  } else {
    val value: String = await answer("else")
    return value
  }
}
fun main() {
  val first = spawn { val value: String = await choose(true) println(value) return }
  join(first)
  val second = spawn { val value: String = await choose(false) println(value) return }
  join(second)
}
"#,
        )
        .expect("parse String branch-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit String branch-await fixture");
        assert!(generated.contains("branch-join suspension state=1"));
        assert!(generated.contains("strlen(__value)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-branch-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile String branch-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run String branch-await fixture");
        assert!(
            output.status.success(),
            "String branch-await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "then\nelse\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_two_await_string_state_machine() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun answer(): String { return "multi-string" }
async fun twice(): String {
  val first: String = await answer()
  val second: String = await answer()
  return second
}
fun main() {
  val task = spawn {
    val value: String = await twice()
    println(value)
    return
  }
  join(task)
}
"#,
        )
        .expect("parse two-await String fixture");
        let generated = emit_c_from_ast(&file).expect("emit two-await String fixture");
        assert!(generated.contains("aura async suspension state=2 kind=await"));
        assert!(
            generated.contains("AuraTaskFrame *await_task_1;")
                || generated.contains("AuraTaskFrame *await_task;")
        );
        assert!(
            generated.contains("strlen(__s)")
                || generated.contains("aura async general CFG String lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-two-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile two-await String fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run two-await String fixture");
        assert!(
            output.status.success(),
            "two-await String fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "multi-string\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_four_await_array_state_machine() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int): Array<Int> { return Array<Int>(value) }
async fun collect(): Array<Int> {
  val first: Array<Int> = await worker(1)
  gc_collect()
  val second: Array<Int> = await worker(2)
  gc_collect()
  val third: Array<Int> = await worker(3)
  gc_collect()
  val fourth: Array<Int> = await worker(4)
  return fourth
}
fun main() {
  val task = spawn { val value: Array<Int> = await collect() return value }
  val first: Result<Array<Int>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Array<Int>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  val cancelled = spawn { val value: Array<Int> = await collect() return value }
  cancel(cancelled)
  val cancelled_result: Result<Array<Int>, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse general four-await Array fixture");
        let generated = emit_c_from_ast(&file).expect("emit general four-await Array fixture");
        assert!(generated.contains("aura async general suspension state=4"));
        assert!(generated.contains("aura_method_Array_Int_clone"));
        assert!(generated.contains("aura_async_result_destroy_"));
        assert!(
            generated.contains("aura_task_frame_wait_on(frame, data->await_task_3)")
                || generated.contains("aura_task_frame_wait_on(frame, data->await_task)")
        );
        assert!(generated.contains("aura_gc_collect"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-await-general-four-array-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general four-await Array fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general four-await Array fixture");
        assert!(
            output.status.success(),
            "general four-await Array fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "4\n4\ncancelled\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_four_await_string_array_state_machine() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun worker(value: Int): Array<String> {
  val result: Array<String> = Array<String>(0)
  result.push("array-string")
  return result
}
async fun collect(): Array<String> {
  val first: Array<String> = await worker(1)
  gc_collect()
  val second: Array<String> = await worker(2)
  gc_collect()
  val third: Array<String> = await worker(3)
  gc_collect()
  val fourth: Array<String> = await worker(4)
  return fourth
}
fun main() {
  val task = spawn { val value: Array<String> = await collect() return value }
  val first: Result<Array<String>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.get(0)) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Array<String>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.get(0)) }
    case Err(error) => { println("unexpected-error") }
  }
  val cancelled = spawn { val value: Array<String> = await collect() return value }
  cancel(cancelled)
  val cancelled_result: Result<Array<String>, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("unexpected-success") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("unexpected-failure") }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse general four-await String array fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit general four-await String array fixture");
        assert!(generated.contains("aura async general suspension state=4"));
        assert!(generated.contains("aura_method_Array_String_clone"));
        assert!(generated.contains("aura_gc_collect"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-await-general-four-string-array-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general four-await String array fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general four-await String array fixture");
        assert!(
            output.status.success(),
            "general four-await String array fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "array-string\narray-string\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_corpus_four_await_fixture() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let source = fs::read_to_string(root.join("corpus/async/multi_await_four.aura"))
            .expect("read four-await corpus fixture");
        let file = aura_parser::parse_file(&source).expect("parse four-await corpus fixture");
        let generated = emit_c_from_ast(&file).expect("emit four-await corpus fixture");
        assert!(generated.contains("aura async general CFG Int lowering states=6"));
        assert!(!generated.contains("= (void)"));

        let dir = std::env::temp_dir();
        let stem = format!("aura-corpus-four-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile four-await corpus fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run four-await corpus fixture");
        assert!(
            output.status.success(),
            "four-await corpus failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "four-await-ok\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_bool_branch_loop_fixture() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun leaf(value: Bool): Bool { return value }
async fun choose(flag: Bool, first: Task<Bool>, second: Task<Bool>): Bool {
  var index: Int = 0
  var value: Bool = false
  while (index < 2) {
    if (flag) {
      val next: Bool = await first
      value = next
    } else {
      val alternate: Bool = await second
      value = alternate
    }
    gc_collect()
    index = index + 1
  }
  return value
}
fun main() {
  val task = spawn {
    val value: Bool = await choose(true, leaf(true), leaf(false))
    return value
  }
  val first: Result<Bool, TaskError> = join(task)
  match (first) {
    case Ok(value) => { if (value) { println("bool-ok") } else { println("bool-bad") } }
    case Err(error) => { println("bool-error") }
  }
  gc_collect()
  val second: Result<Bool, TaskError> = join(task)
  match (second) {
    case Ok(value) => { if (value) { println("bool-repeat-ok") } else { println("bool-repeat-bad") } }
    case Err(error) => { println("bool-repeat-error") }
  }
  val cancelled = spawn {
    val value: Bool = await choose(false, leaf(true), leaf(false))
    return value
  }
  cancel(cancelled)
  val cancelled_result: Result<Bool, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("bool-cancel-bad") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("bool-failure") }
        case Cancelled => { println("bool-cancelled") }
      }
    }
  }
}
"#;
        let file = aura_parser::parse_file(source).expect("parse general CFG Bool fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG Bool fixture");
        assert!(generated.contains("aura async general CFG Bool lowering"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-general-bool-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG Bool fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG Bool fixture");
        assert!(
            output.status.success(),
            "general CFG Bool fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "bool-ok\nbool-repeat-ok\nbool-cancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_heap_class_branch_loop_fixture() {
        let source = r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Box(val value: Int) {}
async fun leaf(value: Int): Box { return Box(value) }
async fun choose(flag: Bool, first: Task<Box>, second: Task<Box>): Box {
  var index: Int = 0
  var value: Box = Box(0)
  if (flag) {
    while (index < 2) {
      val next: Box = await first
      value = next
      gc_collect()
      index = index + 1
    }
  } else {
    while (index < 2) {
      val alternate: Box = await second
      value = alternate
      gc_collect()
      index = index + 1
    }
  }
  return value
}
fun main() {
  val task = spawn {
    val value: Box = await choose(true, leaf(73), leaf(11))
    return value
  }
  val first: Result<Box, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.value.toString()) }
    case Err(error) => { println("class-error") }
  }
  gc_collect()
  val second: Result<Box, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.value.toString()) }
    case Err(error) => { println("class-repeat-error") }
  }
  val cancelled = spawn {
    val value: Box = await choose(false, leaf(1), leaf(2))
    return value
  }
  cancel(cancelled)
  val cancelled_result: Result<Box, TaskError> = join(cancelled)
  match (cancelled_result) {
    case Ok(value) => { println("class-cancel-bad") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println("class-failure") }
        case Cancelled => { println("class-cancelled") }
      }
    }
  }
}
"#;
        let file = aura_parser::parse_file(source).expect("parse general CFG class fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG class fixture");
        assert!(generated.contains("aura async general CFG Class lowering"));
        assert!(generated.contains("aura_gc_add_root((void **)result)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-general-class-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG class fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG class fixture");
        assert!(
            output.status.success(),
            "general CFG class fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "73\n73\nclass-cancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_spawn_join_cancel() {
        let span = Span::new(0, 1);
        let ident = |name: &str| Ident {
            name: name.into(),
            span,
        };
        let unit_ty = || TypeRef {
            qualifier: None,
            name: ident("Unit"),
            type_args: vec![],
            nullable: false,
            reference: false,
            span,
            fun: None,
        };
        let handle_ty = || TypeRef {
            qualifier: None,
            name: ident("TaskHandle"),
            type_args: vec![unit_ty()],
            nullable: false,
            reference: false,
            span,
            fun: None,
        };
        let spawn = || {
            Expr::Async(AsyncExpr::Spawn(SpawnExpr {
                body: Block {
                    stmts: vec![],
                    span,
                },
                span,
            }))
        };
        let h1 = ident("h1");
        let h2 = ident("h2");
        let main_fun = FunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: vec![],
            modifiers: vec![],
            visibility: aura_ast::MemberVisibility::Package,
            is_test: false,
            name: ident("main"),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![
                    Stmt::Var(aura_ast::VarStmt {
                        mutable: false,
                        name: h1.clone(),
                        ty: Some(handle_ty()),
                        init: spawn(),
                        span,
                    }),
                    Stmt::Expr(Expr::Async(AsyncExpr::Join(JoinExpr {
                        handle: Box::new(Expr::Ident(h1)),
                        span,
                    }))),
                    Stmt::Var(aura_ast::VarStmt {
                        mutable: false,
                        name: h2.clone(),
                        ty: Some(handle_ty()),
                        init: spawn(),
                        span,
                    }),
                    Stmt::Expr(Expr::Async(AsyncExpr::Cancel(CancelExpr {
                        handle: Box::new(Expr::Ident(h2.clone())),
                        span,
                    }))),
                    Stmt::Expr(Expr::Async(AsyncExpr::Join(JoinExpr {
                        handle: Box::new(Expr::Ident(h2)),
                        span,
                    }))),
                ],
                span,
            },
            span,
        };
        let file = File {
            package: Path {
                segments: vec![ident("demo")],
                span,
            },
            imports: vec![],
            interfaces: vec![],
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            consts: vec![],
            functions: vec![main_fun],
            foreign_functions: vec![],
            async_functions: vec![],
            span,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-c22m-{}", std::process::id()));
        let runtime = root.join("runtime/runtime.c");
        build_from_file(&file, &bin, &runtime).expect("compile generated C22m");
        let generated =
            std::fs::read_to_string(dir.join(format!("aura-c22m-{}.aura.c", std::process::id())))
                .expect("read generated join C");
        assert!(generated.contains("aura_task_executor_join_outcome(__aura_task_executor"));
        assert!(generated.contains("AuraTaskOutcome __join_outcome"));
        assert!(generated.contains("aura_var_std_io_Result_Unit_std_io_TaskError_Err"));
        assert!(generated.contains("aura_var_std_io_TaskError_Failed"));
        assert!(generated.contains("AURA_TASK_CANCELLED"));
        let status = Command::new(&bin).status().expect("run generated binary");
        assert!(status.success());
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(dir.join(format!("aura-c22m-{}.aura.c", std::process::id())));
    }

    #[test]
    fn joined_task_failure_produces_structured_result() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun fail(): Int { throw 7 }
fun main() {
  val task = spawn { val value: Int = await fail() return }
  join(task)
}
"#,
        )
        .expect("parse catchable join failure fixture");
        let generated = emit_c_from_ast(&file).expect("emit catchable join failure fixture");
        assert!(generated.contains("aura_var_std_io_Result_Unit_std_io_TaskError_Err"));
        assert!(generated.contains("aura_var_std_io_TaskError_Failed"));
        assert!(generated.contains("aura_var_std_io_TaskError_Cancelled"));
        assert!(generated.contains("aura_ex_set_source_span"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-join-failure-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile catchable join failure fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run catchable join failure fixture");
        assert!(
            output.status.success(),
            "catchable join failure fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn joined_string_failure_preserves_error_detail() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun fail(): String { throw "detail-preserved" }
fun main() {
  val task = spawn { val value: String = await fail() println("unexpected") return }
  val outcome = join(task)
  match (outcome) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("cancelled") }
      }
    }
  }
  val repeated = join(task)
  match (repeated) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse String join-failure fixture");
        let generated = emit_c_from_ast(&file).expect("emit String join-failure fixture");
        assert!(generated.contains("const char *__join_error"));
        assert!(generated.contains("aura_var_std_io_TaskError_FailedOwned"));
        assert!(generated.contains(".data.Err.error.data.Failed.owned"));
        assert!(generated.contains("aura_ex_matches(\"String\")"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-join-string-failure-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile String join-failure fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run String join-failure fixture");
        assert!(
            output.status.success(),
            "String join-failure fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "detail-preserved\ndetail-preserved\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn general_cfg_throw_after_await_preserves_error_across_repeated_joins() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun ready(): Int { return 1 }
async fun worker(detail: String): Int {
  val value: Int = await ready()
  if (value == 1) { throw detail }
  return value
}
fun printOutcome(outcome: Result<Int, TaskError>): Unit {
  match (outcome) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
fun main() {
  val task = spawn { val value: Int = await worker("after-await") return value }
  val first: Result<Int, TaskError> = join(task)
  printOutcome(first)
  gc_collect()
  val second: Result<Int, TaskError> = join(task)
  printOutcome(second)
}
"#,
        )
        .expect("parse CFG throw-after-await fixture");
        let generated = emit_c_from_ast(&file).expect("emit CFG throw-after-await fixture");
        assert!(generated.contains("aura async general CFG Int lowering"));
        assert!(generated.contains("aura_task_frame_set_error_span_with_clone"));
        assert!(generated.contains("aura_task_frame_set_race_source_id"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-cfg-throw-after-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile CFG throw-after-await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run CFG throw-after-await fixture");
        assert!(
            output.status.success(),
            "CFG throw-after-await fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "after-await\nafter-await\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn nested_suspended_failure_preserves_owned_detail_across_repeated_joins() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun leaf(): String { throw "nested-detail" }
async fun middle(): String {
  val value: String = await leaf()
  return value
}
fun main() {
  val task = spawn {
    val value: String = await middle()
    return value
  }
  val first: Result<String, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("cancelled") }
      }
    }
  }
  gc_collect()
  val second: Result<String, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse nested suspended failure fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested suspended failure fixture");
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated.contains("aura_task_frame_set_error_span_with_clone"));
        assert!(generated.contains("aura_var_std_io_TaskError_FailedOwned"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-suspended-failure-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested suspended failure fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested suspended failure fixture");
        assert!(
            output.status.success(),
            "nested suspended failure fixture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "nested-detail\nnested-detail\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn nested_class_failure_normalizes_owned_type_across_repeated_joins() {
        let file = aura_parser::parse_file(
            r#"package std.io
class Failure(var message: String) {}
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun leaf(): Int { throw Failure("class-detail" + "") }
async fun middle(): Int {
  val value: Int = await leaf()
  return value
}
fun main() {
  val task = spawn {
    val value: Int = await middle()
    return value
  }
  val first: Result<Int, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("cancelled") }
      }
    }
  }
  gc_collect()
  val second: Result<Int, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println("unexpected") }
    case Err(error) => {
      match (error) {
        case Failed(message) => { println(message) }
        case Cancelled => { println("cancelled") }
      }
    }
  }
}
"#,
        )
        .expect("parse nested class failure fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested class failure fixture");
        assert!(generated.contains("aura_ex_take_obj"));
        assert!(generated.contains("aura_async_class_error_clone_"));
        assert!(generated.contains("aura_task_frame_set_error_payload_with_clone"));
        assert!(generated.contains("aura_throw_obj_with_destructor"));
        assert!(generated.contains("aura_task_frame_set_error_span_with_clone"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-class-failure-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested class failure fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested class failure fixture");
        assert!(
            output.status.success(),
            "nested class failure fixture failed: {output:?}"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines, ["class-detail", "class-detail"]);
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn emits_owned_string_success_for_local_task_result_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun observe(handle: TaskHandle<String>): Result<String, TaskError> {
  val outcome: Result<String, TaskError> = join(handle)
  return outcome
}
fun main() { }
"#,
        )
        .expect("parse String join-success codegen fixture");
        let generated = emit_c_from_ast(&file).expect("emit String join-success codegen fixture");
        assert!(generated.contains("aura_var_std_io_Result_String_std_io_TaskError_OkOwned"));
        assert!(generated.contains("data.Ok.owned"));
        assert!(generated.contains("__join_success_owned"));
    }

    #[test]
    fn builds_and_runs_typed_spawn_string_success_with_repeated_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val task = spawn { return "ready" }
  val outcome: Result<String, TaskError> = join(task)
  match (outcome) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("unexpected-error") }
  }
  val repeated: Result<String, TaskError> = join(task)
  match (repeated) {
    case Ok(value) => { println(value) }
    case Err(error) => { println("unexpected-error") }
  }
}
"#,
        )
        .expect("parse typed String spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit typed String spawn fixture");
        assert!(generated.contains("aura_spawn_result_destroy_"));
        assert!(generated.contains("aura_var_std_io_Result_String_std_io_TaskError_OkOwned"));
        assert!(generated.contains("__join_success_owned"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-typed-spawn-string-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile typed String spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run typed String spawn fixture");
        assert!(
            output.status.success(),
            "typed String spawn fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ready\nready\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_typed_spawn_array_success_with_repeated_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val task = spawn { return Array<Int>(2) }
  val first: Result<Array<Int>, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Array<Int>, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
}
"#,
        )
        .expect("parse typed Array spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit typed Array spawn fixture");
        assert!(generated.contains("aura_method_Array_Int_clone"));
        assert!(generated.contains("aura_spawn_result_destroy_"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-typed-spawn-array-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile typed Array spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run typed Array spawn fixture");
        assert!(
            output.status.success(),
            "typed Array spawn fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_typed_spawn_local_aggregate_success_with_repeated_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val words: Array<String> = Array<String>(1)
  words.set(0, "local")
  val task = spawn { return words }
  val first: Result<Array<String>, TaskError> = join(task)
  val second: Result<Array<String>, TaskError> = join(task)
  gc_collect()
  match (first) { case Ok(value) => { println(value.get(0)) } case Err(error) => { println("first-error") } }
  match (second) { case Ok(value) => { println(value.get(0)) } case Err(error) => { println("second-error") } }
  gc_collect()
}
"#,
        )
        .expect("parse local aggregate spawn fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-local-aggregate-{}", std::process::id());
        let bin = dir.join(&stem);
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile local aggregate spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run local aggregate spawn fixture");
        assert!(
            output.status.success(),
            "local aggregate spawn fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "local\nlocal\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(dir.join(format!("{stem}.aura.c")));
    }

    #[test]
    fn builds_and_runs_typed_spawn_class_success_with_repeated_join_and_gc() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Box(val value: Int) {}
fun main() {
  val task = spawn { return Box(73) }
  val first: Result<Box, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Box, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
}
"#,
        )
        .expect("parse typed class spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit typed class spawn fixture");
        assert!(generated.contains("aura_gc_add_root((void **)result)"));
        assert!(generated.contains("aura_gc_remove_root((void **)result)"));
        assert!(generated.contains("aura_gc_add_root((void **)&first.data.Ok.value)"));
        assert!(generated.contains("aura_var_std_io_Result_") && generated.contains("_OkOwned"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-typed-spawn-class-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile typed class spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run typed class spawn fixture");
        assert!(
            output.status.success(),
            "typed class spawn fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_typed_spawn_class_after_await_with_repeated_join_and_gc() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Box(val value: Int) {}
async fun leaf(): Box { return Box(91) }
async fun answer(): Box { return await leaf() }
fun main() {
  val task = spawn {
    val value: Box = await answer()
    return value
  }
  val first: Result<Box, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
  val second: Result<Box, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.value.toString()) }
    case Err(error) => { println("unexpected-error") }
  }
  gc_collect()
}
"#,
        )
        .expect("parse suspended typed class spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit suspended typed class spawn fixture");
        assert!(generated.contains("aura_gc_add_root((void **)result)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-typed-spawn-await-class-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile suspended typed class spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run suspended typed class spawn fixture");
        assert!(
            output.status.success(),
            "suspended typed class spawn fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "91\n91\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_non_empty_spawn_once() {
        let file = aura_parser::parse_file(
            "package demo\nfun main() { val task = spawn { println(\"bounded spawn\") } join(task) }\n",
        )
        .expect("parse bounded spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-spawn-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile bounded non-empty spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generated bounded spawn C");
        assert!(generated.contains("aura_spawn_poll_"));
        assert!(generated.contains("aura_task_executor_release(__aura_task_executor, &task)"));
        let output = Command::new(&bin).output().expect("run bounded spawn");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "bounded spawn\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_spawn_capture_across_await() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun worker(): Int { return 7 }\nfun main() { val captured: String = \"after await\"\nval task = spawn { val result: Int = await worker()\nprintln(captured)\nreturn } join(task) }\n",
        )
        .expect("parse spawn capture across await");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-spawn-await-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile spawn capture across await");
        let generated = fs::read_to_string(&generated_c).expect("read spawn await capture C");
        assert!(generated.contains("AuraTaskFrame *await_task;"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
        assert!(generated.contains("aura_box_str_new(captured)"));
        let output = Command::new(&bin)
            .output()
            .expect("run spawn capture across await");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "after await\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_all_bounded_capture_kinds_across_await_and_gc() {
        let file = aura_parser::parse_file(
            r#"package demo
class Box(val value: Int) {}
async fun ready(): Int { return 7 }
fun report(number: Int, flag: Bool, text: String, values: Array<Int>, box: Box, f: (Int) -> Int) {
  println(number.toString())
  if (flag) { println(text) }
  println(values.len.toString())
  println(box.value.toString())
  println(f(2).toString())
}
fun main() {
  var number: Int = 1
  var flag: Bool = false
  var text: String = "before"
  var values: Array<Int> = Array<Int>(1)
  var box: Box = Box(3)
  val captured: (Int) -> Int = (n: Int) => n + 1
  val task = spawn {
    val readyValue: Int = await ready()
    gc_collect()
    report(number, flag, text, values, box, captured)
    println(readyValue.toString())
    return
  }
  number = 2
  flag = true
  text = "after"
  values.push(2)
  box = Box(4)
  gc_collect()
  join(task)
  gc_collect()
  join(task)
}
"#,
        )
        .expect("parse all bounded capture kinds across await");
        let generated =
            emit_c_from_ast(&file).expect("emit all bounded capture kinds across await");
        assert!(generated.contains("aura_box_i64 * number;"));
        assert!(generated.contains("aura_box_bool * flag;"));
        assert!(generated.contains("aura_box_str * text;"));
        assert!(generated.contains("aura_box_ptr * values;"));
        assert!(generated.contains("aura_box_ptr * box;"));
        assert!(generated.contains("aura_fun_env_retain(__spawn_data->captured.env)"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-all-captures-await-gc-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile all bounded capture kinds across await");
        let output = Command::new(&bin)
            .output()
            .expect("run all bounded capture kinds across await");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "2\nafter\n2\n4\n3\n7\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_int_parameter_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(value: Int) { if (value == 41) { println(\"captured\") } }\nfun launch(value: Int) { val task = spawn { report(value) } join(task) }\nfun main() { launch(41) }\n",
        )
        .expect("parse Int capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-int-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile Int capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generated capture C");
        assert!(generated.contains("typedef struct aura_spawn_data_"));
        assert!(generated.contains("int64_t value;"));
        assert!(generated.contains("__spawn_data->value = value;"));
        let output = Command::new(&bin).output().expect("run Int capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "captured\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_int_local_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(value: Int) { if (value == 41) { println(\"local captured\") } }\nfun main() { val captured = 41\nval task = spawn { report(captured) } join(task) }\n",
        )
        .expect("parse local Int capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-local-int-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile local Int capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read local capture C");
        assert!(generated.contains("__spawn_data->captured = captured;"));
        let output = Command::new(&bin)
            .output()
            .expect("run local Int capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "local captured\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_expression_local_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(value: Int) { if (value == 41) { println(\"expression captured\") } }\nfun main() { val captured = 40 + 1\nval task = spawn { report(captured) } join(task) }\n",
        )
        .expect("parse expression local capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-bounded-expression-local-capture-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile expression local capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read expression capture C");
        assert!(generated.contains("__spawn_data->captured = captured;"));
        let output = Command::new(&bin)
            .output()
            .expect("run expression local capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "expression captured\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_generic_local_capture_without_annotation() {
        let file = aura_parser::parse_file(
            "package demo\nfun identity<T>(value: T): T { return value }\nfun report(value: Int) { if (value == 41) { println(\"generic captured\") } }\nfun main() { val captured = identity(41)\nval task = spawn { report(captured) } join(task) }\n",
        )
        .expect("parse generic local capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-generic-local-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic local capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generic capture C");
        assert!(generated.contains("__spawn_data->captured = captured;"));
        let output = Command::new(&bin)
            .output()
            .expect("run generic local capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "generic captured\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_generic_spawn_capture_from_parameter() {
        let file = aura_parser::parse_file(
            r#"package demo
fun launch<T>(value: T) { val task = spawn { val copied = value println("generic spawn capture") } join(task) }
fun main() { launch(41) launch("payload") }
"#,
        )
        .expect("parse generic spawn parameter capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-generic-spawn-parameter-capture-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic spawn parameter capture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic spawn parameter capture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "generic spawn capture\ngeneric spawn capture\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_generic_call_inside_spawn_capture() {
        let file = aura_parser::parse_file(
            r#"package demo
fun consume<T>(value: T) { println("generic call") }
fun launch<T>(value: T) { val task = spawn { consume(value) } join(task) }
fun main() { launch(41) launch("payload") }
"#,
        )
        .expect("parse generic call inside spawn capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-generic-call-spawn-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic call inside spawn capture");
        let output = Command::new(&bin)
            .output()
            .expect("run generic call inside spawn capture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "generic call\ngeneric call\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_generic_aggregate_capture_without_annotation() {
        let file = aura_parser::parse_file(
            r#"package demo
class Box(val value: Int) {}
fun identity<T>(value: T): T { return value }
fun report(values: Array<Box>) {
  if (values.get(0).value == 73) { println("generic aggregate captured") }
}
fun main() {
  val values = identity(Array<Box>(1))
  values.set(0, Box(73))
  val task = spawn { report(values) }
  join(task)
}
"#,
        )
        .expect("parse generic aggregate capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-bounded-generic-aggregate-capture-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile generic aggregate capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generic aggregate capture C");
        assert!(generated.contains("aura_method_Array_demo_Box_clone(&values)"));
        let output = Command::new(&bin)
            .output()
            .expect("run generic aggregate capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "generic aggregate captured\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_generic_nested_aggregate_capture_without_annotation() {
        let file = aura_parser::parse_file(
            r#"package demo
class Box(val value: Int) {}
fun identity<T>(value: T): T { return value }
fun report(values: Array<Array<Box>>) {
  if (values.get(0).get(0).value == 91) { println("nested generic aggregate captured") }
}
fun main() {
  val inner = Array<Box>(1)
  inner.set(0, Box(91))
  val values = identity(Array<Array<Box>>(1))
  values.set(0, inner)
  gc_collect()
  val task = spawn { report(values) }
  join(task)
}
"#,
        )
        .expect("parse nested generic aggregate capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-bounded-generic-nested-aggregate-capture-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested generic aggregate capture spawn");
        let generated =
            fs::read_to_string(&generated_c).expect("read nested generic aggregate capture C");
        assert!(generated.contains("aura_method_Array_Array_demo_Box_clone(&values)"));
        let output = Command::new(&bin)
            .output()
            .expect("run nested generic aggregate capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "nested generic aggregate captured\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_string_parameter_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(value: String) { println(value) }\nfun launch(value: String) { val task = spawn { report(value) } join(task) }\nfun main() { launch(\"captured string\") }\n",
        )
        .expect("parse String capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-string-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile String capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generated String capture C");
        assert!(generated.contains("aura_box_str * value;"));
        assert!(generated.contains("__spawn_data->value = aura_box_str_new(value);"));
        assert!(generated.contains("aura_box_str_release(data->value);"));
        let output = Command::new(&bin)
            .output()
            .expect("run String capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "captured string\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_string_local_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(value: String) { println(value) }\nfun main() { val captured: String = \"local string\"\nval task = spawn { report(captured) } join(task) }\n",
        )
        .expect("parse local String capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-local-string-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile local String capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read local String capture C");
        assert!(generated.contains("__spawn_data->captured = aura_box_str_new(captured);"));
        let output = Command::new(&bin)
            .output()
            .expect("run local String capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "local string\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_mutable_capture_shared_with_scheduler() {
        let file = aura_parser::parse_file(
            "package demo\nclass Box(var value: Int) {}\nfun report(number: Int, text: String, box: Box) { println(number.toString()) println(text) println(box.value.toString()) }\nfun main() { var number: Int = 1\nvar text: String = \"before\"\nvar box: Box = Box(1)\nval task = spawn { report(number, text, box) return }\nnumber = 2\ntext = \"after\"\nbox = Box(2)\njoin(task) }\n",
        )
        .expect("parse mutable spawn capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-mutable-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mutable spawn capture");
        let generated = fs::read_to_string(&generated_c).expect("read mutable capture C");
        assert!(generated.contains("aura_box_i64 * number;"));
        assert!(generated.contains("aura_box_str * text;"));
        assert!(generated.contains("aura_box_ptr * box;"));
        assert!(generated.contains("aura_box_i64_retain(__spawn_data->number)"));
        let output = Command::new(&bin)
            .output()
            .expect("run mutable spawn capture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\nafter\n2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_spawn_control_flow_with_mutable_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun main() { var number: Int = 1\nval task = spawn { if (number == 1) { println(\"before\") } else { println(\"after\") } return }\nnumber = 2\njoin(task) }\n",
        )
        .expect("parse spawn control-flow capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-control-flow-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile spawn control-flow capture");
        let output = Command::new(&bin)
            .output()
            .expect("run spawn control-flow capture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "after\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_spawn_control_flow_with_mutable_bool_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun main() { var flag: Bool = false\nval task = spawn { if (flag) { println(\"bad\") } else { println(\"ok\") } } join(task) }\n",
        )
        .expect("parse mutable Bool spawn capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-bool-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mutable Bool spawn capture");
        let output = Command::new(&bin)
            .output()
            .expect("run mutable Bool spawn capture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_spawn_await_bool_result() {
        let file = aura_parser::parse_file(
            "package demo\nasync fun ready(): Bool { return true }\nfun main() { val task = spawn { val value: Bool = await ready() if (value) { println(\"ready\") } } join(task) }\n",
        )
        .expect("parse Bool spawn await");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-await-bool-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile Bool spawn await");
        let output = Command::new(&bin).output().expect("run Bool spawn await");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ready\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_single_await_bool_result() {
        let file = aura_parser::parse_file(
            r#"package demo
async fun ready(): Bool { return true }
async fun wrapper(): Bool {
  val value: Bool = await ready()
  return value
}
fun main() {
  val task = spawn {
    val value: Bool = await wrapper()
    if (value) { println("bool-await-ok") }
    return
  }
  join(task)
}
"#,
        )
        .expect("parse single-await Bool result");
        let generated = emit_c_from_ast(&file).expect("emit single-await Bool result");
        assert!(
            generated.contains("static bool aura_async_resume_demo_wrapper")
                || generated.contains("aura async general CFG Bool lowering")
        );
        assert!(
            generated.contains("bool observed = 0;")
                || generated.contains("aura async general CFG Bool lowering")
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-single-await-bool-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile single-await Bool result");
        let output = Command::new(&bin)
            .output()
            .expect("run single-await Bool result");
        assert!(
            output.status.success(),
            "single-await Bool fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "bool-await-ok\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_spawn_capture_through_forced_gc() {
        let file = aura_parser::parse_file(
            "package demo\nclass Box(var value: Int) {}\nfun main() { var box: Box = Box(7)\nval task = spawn { gc_collect() println(box.value.toString()) return }\ngc_collect()\njoin(task) }\n",
        )
        .expect("parse spawn forced-GC capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-forced-gc-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile spawn forced-GC capture");
        let output = Command::new(&bin)
            .output()
            .expect("run spawn forced-GC capture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_spawn_capture_cancellation_cleanup() {
        let file = aura_parser::parse_file(
            "package demo\nfun main() { var text: String = \"pending\"\nval task = spawn { println(text) return }\ncancel(task)\njoin(task) }\n",
        )
        .expect("parse spawn cancellation capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-cancel-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile spawn cancellation capture");
        let output = Command::new(&bin)
            .output()
            .expect("run spawn cancellation capture");
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).is_empty());
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn compiler_nested_rethrow_preserves_cause_chain_and_spans() {
        let file = aura_parser::parse_file(
            "package demo\nfun main() { try { try { throw \"nested\" } catch (e: Int) { println(\"wrong\") } } catch (e: Bool) { println(\"wrong\") } }\n",
        )
        .expect("parse nested exception cause fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-exception-cause-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested exception cause fixture");
        let generated = fs::read_to_string(&generated_c).expect("read nested cause C");
        assert!(generated.contains("aura_ex_add_cause(\"Int\""));
        assert!(generated.contains("aura_ex_add_cause(\"Bool\""));
        let output = Command::new(&bin)
            .output()
            .expect("run nested exception cause fixture");
        assert!(
            !output.status.success(),
            "uncaught nested exception must fail"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("caused by (Int)"), "{stderr}");
        assert!(stderr.contains("caused by (Bool)"), "{stderr}");
        assert!(stderr.contains("source span"), "{stderr}");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_exception_cause_api() {
        let file = aura_parser::parse_file(
            "package demo\nfun main() { try { try { throw \"nested\" } catch (e: Int) { } } catch (e: String) { println(exception_cause_count().toString()) println(exception_cause_type(0)) println(exception_cause_span_start(0).toString()) exception_add_cause(\"manual\", 7, 9) println(exception_cause_count().toString()) println(exception_cause_type(1)) println(exception_cause_span_end(1).toString()) } }\n",
        )
        .expect("parse exception cause API fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-exception-cause-api-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile exception cause API fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run exception cause API fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "1\nInt\n32\n2\nmanual\n9\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_mutable_array_capture_shared_with_scheduler() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(values: Array<Int>) { println(values.len.toString()) }\nfun main() { var values: Array<Int> = Array(0)\nvalues.push(1)\nval task = spawn { report(values) return }\nvalues.push(2)\njoin(task) }\n",
        )
        .expect("parse mutable Array spawn capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-mutable-array-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mutable Array spawn capture");
        let generated = fs::read_to_string(&generated_c).expect("read mutable Array capture C");
        assert!(generated.contains("aura_box_ptr * values;"));
        assert!(generated.contains("aura_capture_drop_Array_Int"));
        assert!(generated.contains("aura_method_Array_Int_clone"));
        let output = Command::new(&bin)
            .output()
            .expect("run mutable Array spawn capture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_class_parameter_capture() {
        let file = aura_parser::parse_file(
            "package demo\nclass Box(val value: Int) {}\nfun report(box: Box) { if (box.value == 73) { println(\"captured class\") } }\nfun launch(box: Box) { val task = spawn { report(box) } join(task) }\nfun main() { launch(Box(73)) }\n",
        )
        .expect("parse class capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-class-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile class capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generated class capture C");
        assert!(generated.contains("aura_gc_add_root((void **)&__spawn_data->box);"));
        assert!(generated.contains("aura_gc_remove_root((void **)&data->box);"));
        let output = Command::new(&bin)
            .output()
            .expect("run class capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "captured class\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_class_string_constructor_ownership() {
        let file = parse_file(
            "package demo\nclass Note(val text: String) {}\nopen class Parent(val label: String) {}\nclass Child() : Parent(\"inherited literal\") {}\nfun main() { if (true) { val literal = Note(\"literal\") } gc_collect() if (true) { val source = \"heap\" + \"\" val owned = Note(source) } gc_collect() if (true) { val child = Child() } gc_collect() println(\"string constructor ownership ok\") }\n",
        )
        .expect("parse class String ownership");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-class-string-ownership-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile class String ownership");
        let generated = fs::read_to_string(&generated_c).expect("read generated class String C");
        assert!(generated.contains("class String field allocation failed"));
        let output = Command::new(&bin)
            .output()
            .expect("run class String ownership");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "string constructor ownership ok\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_this_as_heap_class_constructor_argument() {
        let file = parse_file(
            "package demo\nclass Handle(val owner: Node) {}\nclass Node(val id: Int) { fun handle(): Handle { return Handle(this) } }\nfun main() { val node = Node(42) val handle = node.handle() if (handle.owner.id == 42) { println(\"heap this constructor ok\") } }\n",
        )
        .expect("parse heap this constructor argument");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-heap-this-constructor-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile heap this constructor argument");
        let generated = fs::read_to_string(&generated_c).expect("read generated heap this C");
        assert!(generated.contains("aura_new_demo_Handle(this)"));
        let output = Command::new(&bin)
            .output()
            .expect("run heap this constructor argument");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "heap this constructor ok\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_class_local_capture() {
        let file = aura_parser::parse_file(
            "package demo\nclass Box(val value: Int) {}\nfun report(box: Box) { if (box.value == 73) { println(\"local class\") } }\nfun main() { val captured: Box = Box(73)\nval task = spawn { report(captured) } join(task) }\n",
        )
        .expect("parse local class capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-local-class-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile local class capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read local class capture C");
        assert!(generated.contains("aura_gc_add_root((void **)&__spawn_data->captured);"));
        let output = Command::new(&bin)
            .output()
            .expect("run local class capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "local class\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_array_parameter_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(values: Array<Int>) { if (values.len == 3) { println(\"captured array\") } }\nfun launch(values: Array<Int>) { val task = spawn { report(values) } join(task) }\nfun main() { launch(Array<Int>(3)) }\n",
        )
        .expect("parse Array capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-array-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile Array capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generated Array capture C");
        assert!(generated.contains("aura_method_Array_Int_clone(&values)"));
        assert!(generated.contains("aura_method_Array_Int_clone(&data->values)"));
        let output = Command::new(&bin)
            .output()
            .expect("run Array capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "captured array\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_array_local_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(values: Array<Int>) { if (values.len == 3) { println(\"local array\") } }\nfun main() { val captured = Array<Int>(3)\nval task = spawn { report(captured) } join(task) }\n",
        )
        .expect("parse local Array capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-local-array-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile local Array capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read local Array capture C");
        assert!(
            generated.contains("__spawn_data->captured = aura_method_Array_Int_clone(&captured);")
        );
        let output = Command::new(&bin)
            .output()
            .expect("run local Array capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "local array\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn spawn_capture_respects_inner_local_shadowing() {
        let file = aura_parser::parse_file(
            r#"package demo
fun main() {
  val captured = 41
  val task = spawn {
    val captured = 7
    println(captured.toString())
    return
  }
  join(task)
}
"#,
        )
        .expect("parse shadowed spawn capture");
        let generated = emit_c_from_ast(&file).expect("emit shadowed spawn capture");
        assert!(!generated.contains("__spawn_data->captured"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-shadowed-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile shadowed spawn capture");
        let output = Command::new(&bin)
            .output()
            .expect("run shadowed spawn capture");
        assert!(output.status.success(), "shadowed spawn failed: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_optional_local_capture() {
        let file = aura_parser::parse_file(
            r#"package demo
fun report(value: Int?): Unit {
  if (value != null) { println("captured optional") }
}
fun main() {
  val captured: Int? = 73
  val task = spawn { report(captured) }
  join(task)
}
"#,
        )
        .expect("parse optional spawn capture");
        let generated = emit_c_from_ast(&file).expect("emit optional spawn capture");
        assert!(generated.contains("aura_spawn_data_"));
        assert!(generated.contains("captured"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-optional-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile optional spawn capture");
        let output = Command::new(&bin)
            .output()
            .expect("run optional spawn capture");
        assert!(
            output.status.success(),
            "optional spawn capture failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "captured optional\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_string_array_parameter_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun report(values: Array<String>) { if (values.len == 1) { println(\"captured string array\") } }\nfun launch(values: Array<String>) { val task = spawn { report(values) } join(task) }\nfun main() { launch(Array<String>(1)) }\n",
        )
        .expect("parse String Array capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-string-array-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile String Array capture spawn");
        let generated =
            fs::read_to_string(&generated_c).expect("read generated String Array capture C");
        assert!(generated.contains("aura_method_Array_String_clone(&values)"));
        let output = Command::new(&bin)
            .output()
            .expect("run String Array capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "captured string array\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_class_array_capture_through_gc_and_await() {
        let file = aura_parser::parse_file(
            "package demo\nclass Box(var value: Int) {}\nasync fun pause(): Int { gc_collect() return 0 }\nfun main() { var task = spawn { return } if (true) { val values: Array<Box> = Array(1) values.set(0, Box(73)) task = spawn { val ignored: Int = await pause() gc_collect() println(values.get(0).value.toString()) return } } gc_collect() join(task) }\n",
        )
        .expect("parse Array<class> spawn capture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-class-array-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile Array<class> spawn capture");
        let generated = fs::read_to_string(&generated_c).expect("read Array<class> capture C");
        assert!(generated.contains("aura_gc_add_array_root((void **)&__spawn_data->values.data"));
        assert!(generated.contains("aura_gc_remove_array_root((void **)&data->values.data"));
        let output = Command::new(&bin)
            .output()
            .expect("run Array<class> spawn capture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_fun_parameter_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun apply(f: (Int) -> Int) { if (f(2) == 3) { println(\"captured fun\") } }\nfun launch(f: (Int) -> Int) { val task = spawn { apply(f) } join(task) }\nfun main() { launch((n: Int) => n + 1) }\n",
        )
        .expect("parse Fun capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-fun-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile Fun capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generated Fun capture C");
        assert!(generated.contains("aura_fun_env_retain(__spawn_data->f.env)"));
        assert!(generated.contains("aura_fun_env_free(data->f.env)"));
        let output = Command::new(&bin).output().expect("run Fun capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "captured fun\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_bounded_fun_local_capture() {
        let file = aura_parser::parse_file(
            "package demo\nfun apply(f: (Int) -> Int) { if (f(2) == 3) { println(\"local fun\") } }\nfun main() { val captured: (Int) -> Int = (n: Int) => n + 1\nval task = spawn { apply(captured) } join(task) }\n",
        )
        .expect("parse local Fun capture spawn");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-bounded-local-fun-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile local Fun capture spawn");
        let generated = fs::read_to_string(&generated_c).expect("read local Fun capture C");
        assert!(generated.contains("aura_fun_env_retain(__spawn_data->captured.env)"));
        let output = Command::new(&bin)
            .output()
            .expect("run local Fun capture spawn");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "local fun\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn moves_string_ownership_across_nested_assignment() {
        let file = aura_parser::parse_file(
            r#"package demo
fun main() {
  var path = ""
  if (true) {
    val arg = "owned string move"
    path = arg
  }
  println(path)
}
"#,
        )
        .expect("parse string ownership fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-string-move-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile string ownership fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run string ownership fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "owned string move\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn evaluates_owned_rhs_before_string_and_array_drop() {
        let file = aura_parser::parse_file(
            r#"package demo
fun appendMark(text: String): String {
  return text + "!"
}
fun makeWords(seed: String): Array<String> {
  val words: Array<String> = Array(0)
  words.push(seed)
  return words
}
fun main() {
  var text = "a"
  var i = 0
  while (i < 3) {
    text = appendMark(text)
    i = i + 1
  }
  var rows: Array<String> = Array(0)
  var j = 0
  while (j < 2) {
    val next = makeWords(text)
    rows = next.clone()
    j = j + 1
  }
  val copy = rows.clone()
  for (k in 0..copy.len) {
    println(copy.get(k))
  }
  println(text)
}
"#,
        )
        .expect("parse ownership evaluation-order fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-owned-rhs-order-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile ownership evaluation-order fixture");
        let generated = fs::read_to_string(&generated_c).expect("read generated ownership C");
        assert!(generated.contains("__aura_string_rhs_"));
        assert!(generated.contains("__aura_array_rhs_"));
        let output = Command::new(&bin)
            .output()
            .expect("run ownership evaluation-order fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "a!!!\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn non_empty_spawn_local_body_uses_bounded_poller() {
        let file = aura_parser::parse_file(
            "package demo\nfun main() { val task = spawn { val later = 1 } cancel(task) }\n",
        )
        .expect("parse bounded spawn");
        let generated = emit_c_from_ast(&file).expect("emit bounded spawn path");
        assert!(!generated.contains("non-empty spawn body requires C22l state-machine lowering"));
        assert!(generated.contains("AURA_TASK_COMPLETE"));
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_branch_and_loop_awaits() {
        let file = parse_file(
            r#"package demo
async fun ready(): Int { return 2 }
fun main() {
  val increment: Int = 2
  val task = spawn {
    var total: Int = 0
    var i: Int = 0
    while (i < 2) {
      if (i == 0) {
        val value: Int = await ready()
        total = total + increment
      } else {
        val value: Int = await ready()
        total = total + increment
      }
      i = i + 1
    }
    println(total.toString())
    return
  }
  join(task)
}
"#,
        )
        .expect("parse general CFG spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit general CFG spawn fixture");
        assert!(generated.contains("aura_fn_demo___spawn_cfg_"));
        assert!(generated.contains("aura async general CFG Unit lowering"));
        assert!(!generated.contains("non-empty spawn body requires C22l state-machine lowering"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-spawn-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG spawn fixture");
        assert!(
            output.status.success(),
            "general CFG spawn failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "4\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_aggregate_capture_and_try() {
        let file = parse_file(
            r#"package demo
async fun ready(value: Int): Int {
  await tick()
  return value
}
async fun tick(): Unit { }
fun main() {
  val values = Array<Int>(2)
  values.push(2)
  values.push(3)
  val use_first: Bool = true
  val task = spawn {
    var total: Int = 0
    var index: Int = 0
    while (index < values.len) {
      if (use_first) {
        try {
          val value: Int = await ready(values.get(index)!!)
          total = total + value
        } catch (error: String) {
          total = total + 100
        }
      } else {
        val value: Int = await ready(1)
        total = total + value
      }
      index = index + 1
    }
    println(total.toString())
    return
  }
  gc_collect()
  join(task)
}
"#,
        )
        .expect("parse aggregate general CFG spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit aggregate general CFG spawn fixture");
        assert!(generated.contains("aura_fn_demo___spawn_cfg_"));
        assert!(generated.contains("aura_method_Array_Int_clone"));
        assert!(generated.contains("aura async general CFG Unit lowering"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-spawn-aggregate-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile aggregate general CFG spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run aggregate general CFG spawn fixture");
        assert!(
            output.status.success(),
            "aggregate general CFG spawn failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "5\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_owned_array_result() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun ready(): Unit { }
fun main() {
  val values = Array<Int>(2)
  values.push(4)
  values.push(5)
  val task = spawn {
    var index: Int = 0
    while (index < values.len) {
      await ready()
      index = index + 1
    }
    return values
  }
  gc_collect()
  val outcome: Result<Array<Int>, TaskError> = join(task)
  match (outcome) {
    case Ok(value) => { println(value.len.toString()) }
    case Err(error) => { println("failed") }
  }
}
"#,
        )
        .expect("parse general CFG spawn owned array result fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit general CFG spawn owned array result fixture");
        assert!(generated.contains("aura_fn_std_io___spawn_cfg_"));
        assert!(generated.contains("aura_method_Array_Int_clone"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-general-cfg-spawn-owned-array-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile general CFG spawn owned array result fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run general CFG spawn owned array result fixture");
        assert!(
            output.status.success(),
            "general CFG spawn owned array result failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "4\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_inferred_array_capture() {
        let file = parse_file(
            r#"package demo
async fun ready(): Int { return 1 }
fun main() {
  val values = Array<Int>(3)
  val task = spawn {
    var index: Int = 0
    while (index < values.len) {
      val tick: Int = await ready()
      index = index + tick
    }
    println(values.len.toString())
    return
  }
  gc_collect()
  join(task)
}
"#,
        )
        .expect("parse inferred Array capture CFG spawn fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit inferred Array capture CFG spawn fixture");
        assert!(generated.contains("aura_fn_demo___spawn_cfg_"));
        assert!(generated.contains("aura_method_Array_Int_clone"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-spawn-array-capture-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile inferred Array capture CFG spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run inferred Array capture CFG spawn fixture");
        assert!(
            output.status.success(),
            "inferred Array capture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_inferred_class_capture_after_gc() {
        let file = parse_file(
            r#"package demo
class Box(val value: Int) {}
async fun ready(): Int { return 1 }
fun main() {
  val box = Box(73)
  val task = spawn {
    if (true) {
      val step: Int = await ready()
      println(step.toString())
    } else {
      val step: Int = await ready()
      println(step.toString())
    }
    println(box.value.toString())
    return
  }
  gc_collect()
  join(task)
}
"#,
        )
        .expect("parse inferred class capture CFG spawn fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit inferred class capture CFG spawn fixture");
        assert!(generated.contains("aura_fn_demo___spawn_cfg_"));
        assert!(
            generated.contains("aura_gc_mark_ptr"),
            "general CFG class capture must emit a typed GC mark hook"
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-spawn-class-capture-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile inferred class capture CFG spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run inferred class capture CFG spawn fixture");
        assert!(
            output.status.success(),
            "inferred class capture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_mutable_box_capture() {
        let file = parse_file(
            r#"package demo
async fun ready(): Int { return 2 }
fun main() {
  var total: Int = 1
  val task = spawn {
    if (true) {
      val step: Int = await ready()
      total = total + step
    } else {
      val step: Int = await ready()
      total = total + step
    }
    println(total.toString())
    return
  }
  join(task)
}
"#,
        )
        .expect("parse mutable general CFG spawn fixture");
        let generated = emit_c_from_ast(&file).expect("emit mutable general CFG spawn fixture");
        assert!(generated.contains("aura_fn_demo___spawn_cfg_"));
        assert!(generated.contains("aura_box_i64_retain"));
        assert!(generated.contains("aura_box_i64_release"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-spawn-mutable-capture-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mutable general CFG spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run mutable general CFG spawn fixture");
        assert!(
            output.status.success(),
            "mutable capture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_mutable_string_capture() {
        let file = parse_file(
            r#"package demo
async fun ready(): Int { return 1 }
fun main() {
  var text: String = "before"
  val task = spawn {
    if (true) {
      val step: Int = await ready()
      text = "after"
      println(step.toString())
    } else {
      val step: Int = await ready()
      text = "after"
      println(step.toString())
    }
    println(text)
    return
  }
  join(task)
}
"#,
        )
        .expect("parse mutable String general CFG spawn fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit mutable String general CFG spawn fixture");
        assert!(generated.contains("aura_fn_demo___spawn_cfg_"));
        assert!(generated.contains("aura_box_str_retain"));
        assert!(generated.contains("aura_box_str_release"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-spawn-mutable-string-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mutable String general CFG spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run mutable String general CFG spawn fixture");
        assert!(
            output.status.success(),
            "mutable String capture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1\nafter\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_mutable_array_capture() {
        let file = parse_file(
            r#"package demo
async fun ready(): Int { return 2 }
fun main() {
  var values: Array<Int> = Array<Int>(1)
  values.set(0, 1)
  val task = spawn {
    if (true) {
      val step: Int = await ready()
      values.set(0, values.get(0) + step)
    } else {
      val step: Int = await ready()
      values.set(0, values.get(0) + step)
    }
    println(values.get(0).toString())
    return
  }
  join(task)
}
"#,
        )
        .expect("parse mutable Array general CFG spawn fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit mutable Array general CFG spawn fixture");
        assert!(generated.contains("aura_fn_demo___spawn_cfg_"));
        assert!(generated.contains("aura_box_ptr_retain"));
        assert!(generated.contains("aura_box_ptr_release"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-spawn-mutable-array-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mutable Array general CFG spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run mutable Array general CFG spawn fixture");
        assert!(
            output.status.success(),
            "mutable Array capture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_general_cfg_spawn_with_mutable_class_capture() {
        let file = parse_file(
            r#"package demo
class Box(var value: Int) {
  fun add(amount: Int): Unit { value = value + amount }
}
async fun ready(): Int { return 2 }
fun main() {
  var box: Box = Box(1)
  val task = spawn {
    if (true) {
      val step: Int = await ready()
      box.add(step)
    } else {
      val step: Int = await ready()
      box.add(step)
    }

    println(box.value.toString())
    return
  }
  join(task)
}
"#,
        )
        .expect("parse mutable class general CFG spawn fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit mutable class general CFG spawn fixture");
        assert!(generated.contains("aura_fn_demo___spawn_cfg_"));
        assert!(generated.contains("aura_box_ptr_retain"));
        assert!(generated.contains("aura_box_ptr_release"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-general-cfg-spawn-mutable-class-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mutable class general CFG spawn fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run mutable class general CFG spawn fixture");
        assert!(
            output.status.success(),
            "mutable class capture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_nullable_class_task_payload_with_repeated_owned_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Box(val value: Int) {}
async fun produce(): Box? { return Box(9) }
fun main() {
  val task = spawn { val value: Box? = await produce() return value }
  val first: Result<Box?, TaskError> = join(task)
  match (first) {
    case Ok(value) => { if (value != null) { println("ok") } }
    case Err(error) => { println("first-error") }
  }
  gc_collect()
  val second: Result<Box?, TaskError> = join(task)
  match (second) {
    case Ok(value) => { if (value != null) { println("ok") } }
    case Err(error) => { println("second-error") }
  }
  val cancelled = spawn { val value: Box? = await produce() return value }
  cancel(cancelled)
  val cancelledResult: Result<Box?, TaskError> = join(cancelled)
  match (cancelledResult) {
    case Ok(value) => { println("cancelled-unexpected-success") }
    case Err(error) => { match (error) { case Cancelled => { println("cancelled") } case Failed(message) => { println("cancelled-failed") } } }
  }
}
"#,
        )
        .expect("parse nullable class task payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit nullable class task payload fixture");
        assert!(
            generated.contains("aura_var_std_io_Result_Opt_std_io_Box_std_io_TaskError_OkOwned")
        );
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nullable-class-task-payload-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nullable class task payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nullable class task payload fixture");
        assert!(
            output.status.success(),
            "nullable class task payload failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "ok\nok\ncancelled\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_nullable_string_task_payload_with_repeated_owned_join() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun produce(): String? { return "nullable-string" }
fun main() {
  val task = spawn { val value: String? = await produce() return value }
  val first: Result<String?, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value!!) }
    case Err(error) => { println("first-error") }
  }
  gc_collect()
  val second: Result<String?, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value!!) }
    case Err(error) => { println("second-error") }
  }
}
"#,
        )
        .expect("parse nullable String task payload fixture");
        let generated = emit_c_from_ast(&file).expect("emit nullable String task payload fixture");
        assert!(generated.contains("aura_var_std_io_Result_Opt_String_std_io_TaskError_OkOwned"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nullable-string-task-payload-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nullable String task payload fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nullable String task payload fixture");
        assert!(
            output.status.success(),
            "nullable String task payload failed: {output:?}"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "nullable-string\nnullable-string\n"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_typed_int_channel_fifo_capacity_and_close() {
        let span = Span::new(0, 1);
        let ident = |name: &str| Ident {
            name: name.into(),
            span,
        };
        let int_ty = || TypeRef {
            qualifier: None,
            name: ident("Int"),
            type_args: vec![],
            nullable: false,
            reference: false,
            span,
            fun: None,
        };
        let opt_int_ty = || TypeRef {
            nullable: true,
            ..int_ty()
        };
        let channel_ty = || TypeRef {
            qualifier: None,
            name: ident("Channel"),
            type_args: vec![int_ty()],
            nullable: false,
            reference: false,
            span,
            fun: None,
        };
        let ch = ident("ch");
        let recv = || {
            Expr::Async(AsyncExpr::ChannelReceive(ChannelReceiveExpr {
                channel: Box::new(Expr::Ident(ch.clone())),
                span,
            }))
        };
        let send = |n| {
            Expr::Async(AsyncExpr::ChannelSend(ChannelSendExpr {
                channel: Box::new(Expr::Ident(ch.clone())),
                value: Box::new(Expr::Int(IntLit { value: n, span })),
                span,
            }))
        };
        let assert_eq = |left, right| {
            Expr::Call(CallExpr {
                callee: Box::new(Expr::Ident(ident("assert_eq"))),
                type_args: vec![],
                args: vec![left, Expr::Int(IntLit { value: right, span })],
                span,
            })
        };
        let main_fun = FunDecl {
            is_pub: false,
            origin_package: String::new(),
            attributes: vec![],
            modifiers: vec![],
            visibility: aura_ast::MemberVisibility::Package,
            is_test: false,
            name: ident("main"),
            type_params: vec![],
            params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![
                    Stmt::Var(aura_ast::VarStmt {
                        mutable: false,
                        name: ch.clone(),
                        ty: Some(channel_ty()),
                        init: Expr::Async(AsyncExpr::ChannelCreate(ChannelCreateExpr {
                            element_type: int_ty(),
                            capacity: Box::new(Expr::Int(IntLit { value: 1, span })),
                            span,
                        })),
                        span,
                    }),
                    Stmt::Expr(send(10)),
                    // Receive before the second send: with capacity one this proves FIFO
                    // and makes the second send exercise the freed slot.
                    Stmt::Var(aura_ast::VarStmt {
                        mutable: false,
                        name: ident("first"),
                        ty: Some(opt_int_ty()),
                        init: recv(),
                        span,
                    }),
                    Stmt::Expr(assert_eq(
                        Expr::ForceUnwrap(aura_ast::ForceUnwrapExpr {
                            expr: Box::new(Expr::Ident(ident("first"))),
                            span,
                        }),
                        10,
                    )),
                    Stmt::Expr(send(20)),
                    Stmt::Var(aura_ast::VarStmt {
                        mutable: false,
                        name: ident("second"),
                        ty: Some(opt_int_ty()),
                        init: recv(),
                        span,
                    }),
                    Stmt::Expr(assert_eq(
                        Expr::ForceUnwrap(aura_ast::ForceUnwrapExpr {
                            expr: Box::new(Expr::Ident(ident("second"))),
                            span,
                        }),
                        20,
                    )),
                    Stmt::Expr(Expr::Async(AsyncExpr::ChannelClose(ChannelCloseExpr {
                        channel: Box::new(Expr::Ident(ch.clone())),
                        span,
                    }))),
                    Stmt::Var(aura_ast::VarStmt {
                        mutable: false,
                        name: ident("closed"),
                        ty: Some(opt_int_ty()),
                        init: recv(),
                        span,
                    }),
                    Stmt::Expr(assert_eq(
                        Expr::Binary(aura_ast::BinaryExpr {
                            op: aura_ast::BinOp::Coalesce,
                            left: Box::new(Expr::Ident(ident("closed"))),
                            right: Box::new(Expr::Int(IntLit { value: 0, span })),
                            span,
                        }),
                        0,
                    )),
                ],
                span,
            },
            span,
        };
        let file = File {
            package: Path {
                segments: vec![ident("demo")],
                span,
            },
            imports: vec![],
            interfaces: vec![],
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            consts: vec![],
            functions: vec![main_fun],
            foreign_functions: vec![],
            async_functions: vec![],
            span,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-c22o-{}", std::process::id()));
        let runtime = root.join("runtime/runtime.c");
        build_from_file(&file, &bin, &runtime).expect("compile generated C22o");
        let status = Command::new(&bin)
            .status()
            .expect("run generated C22o binary");
        assert!(status.success());
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(dir.join(format!("aura-c22o-{}.aura.c", std::process::id())));
    }

    #[test]
    fn builds_and_runs_owned_foreign_handle_channel_transfer() {
        let path = format!(
            "/tmp/aura-channel-foreign-handle-{}-data",
            std::process::id()
        );
        let source = format!(
            r#"package std.io
fun openFile(path: String, mode: Int): ForeignHandle<Int> {{ throw "intrinsic" }}
fun main() {{
  val file: ForeignHandle<Int> = openFile("{path}", 1)
  val channel: Channel<ForeignHandle<Int>> = Channel<ForeignHandle<Int>>(1)
  channel.send(file)
  val received: ForeignHandle<Int>? = channel.receive()
  gc_collect()
}}
"#
        );
        let file = parse_file(&source).expect("parse foreign-handle channel fixture");
        let generated = emit_c_from_ast(&file).expect("emit foreign-handle channel fixture");
        assert!(generated.contains("aura_task_channel_value_destroy_foreign_handle"));
        assert!(generated.contains("aura_ffi_handle_retain(__handle)"));
        assert!(generated.contains("aura_ffi_handle_drop(handle)"));
        assert!(generated.contains("*__payload = NULL"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-channel-foreign-handle-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile foreign-handle channel fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run foreign-handle channel fixture");
        assert!(
            output.status.success(),
            "foreign-handle channel fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn builds_and_runs_typed_enum_channel_payload_transfer() {
        let file = parse_file(
            r#"package std.io
enum Message { case Text(value: String) }
fun main() {
  val channel: Channel<Message> = Channel<Message>(1)
  channel.send(Text("channel"))
  val received: Message? = channel.receive()
  gc_collect()
  channel.close()
  gc_collect()
}
"#,
        )
        .expect("parse enum channel fixture");
        let generated = emit_c_from_ast(&file).expect("emit enum channel fixture");
        assert!(generated.contains("aura_task_channel_value_destroy_typed_"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-channel-enum-{}", std::process::id());
        let bin = dir.join(&stem);
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile enum channel fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run enum channel fixture");
        assert!(
            output.status.success(),
            "enum channel fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(dir.join(format!("{stem}.aura.c")));
    }

    #[test]
    fn builds_and_runs_typed_array_channel_payload_transfer() {
        let file = parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val channel: Channel<Array<String>> = Channel<Array<String>>(1)
  val values: Array<String> = Array<String>(1)
  values.set(0, "array-channel")
  channel.send(values)
  val received: Array<String>? = channel.receive()
  gc_collect()
  channel.close()
  gc_collect()
}
"#,
        )
        .expect("parse array channel fixture");
        let generated = emit_c_from_ast(&file).expect("emit array channel fixture");
        assert!(generated.contains("aura_task_channel_value_destroy_typed_"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-channel-array-{}", std::process::id());
        let bin = dir.join(&stem);
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile array channel fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run array channel fixture");
        assert!(
            output.status.success(),
            "array channel fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(dir.join(format!("{stem}.aura.c")));
    }

    #[test]
    fn builds_and_runs_bool_channel_payload_transfer() {
        let file = parse_file(
            r#"package std.io
fun main() {
  val channel: Channel<Bool> = Channel<Bool>(1)
  channel.send(true)
  val received: Bool? = channel.receive()
  if (received != null && received!!) { println("bool-channel") }
  channel.close()
  gc_collect()
}
"#,
        )
        .expect("parse bool channel fixture");
        let generated = emit_c_from_ast(&file).expect("emit bool channel fixture");
        assert!(generated.contains("aura_opt_bool"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-channel-bool-{}", std::process::id());
        let bin = dir.join(&stem);
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile bool channel fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run bool channel fixture");
        assert!(
            output.status.success(),
            "bool channel fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "bool-channel\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(dir.join(format!("{stem}.aura.c")));
    }

    #[test]
    fn builds_and_runs_task_handle_channel_payload_transfer() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val task: TaskHandle<Int> = spawn { return 9 }
  val channel: Channel<TaskHandle<Int>> = Channel<TaskHandle<Int>>(1)
  channel.send(task)
  val received: TaskHandle<Int>? = channel.receive()
  val outcome: Result<Int, TaskError> = join(received!!)
  channel.close()
  gc_collect()
}
"#,
        )
        .expect("parse task-handle channel fixture");
        let generated = emit_c_from_ast(&file).expect("emit task-handle channel fixture");
        assert!(generated.contains("aura_task_channel_value_from_task"));
        assert!(generated.contains("aura_task_executor_retain_payload"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-channel-task-handle-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile task-handle channel fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run task-handle channel fixture");
        assert!(
            output.status.success(),
            "task-handle channel fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_channel_capture_with_spawn_ownership() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val channel: Channel<Int> = Channel<Int>(1)
  val task = spawn { channel.send(3) return }
  val outcome: Result<Unit, TaskError> = join(task)
  channel.close()
  gc_collect()
}
"#,
        )
        .expect("parse channel capture fixture");
        let generated = emit_c_from_ast(&file).expect("emit channel capture fixture");
        assert!(generated.contains("aura_task_channel_retain"));
        assert!(generated.contains("aura_task_channel_destroy(data->channel)"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-spawn-channel-capture-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile channel capture fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run channel capture fixture");
        assert!(
            output.status.success(),
            "channel capture fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_channel_channel_payload_transfer() {
        let file = aura_parser::parse_file(
            r#"package std.io
fun main() {
  val inner: Channel<Int> = Channel<Int>(1)
  val outer: Channel<Channel<Int>> = Channel<Channel<Int>>(1)
  outer.send(inner)
  val received: Channel<Int>? = outer.receive()
  val receivedChannel: Channel<Int> = received!!
  receivedChannel.send(7)
  val value: Int? = receivedChannel.receive()
  outer.close()
  inner.close()
  receivedChannel.close()
  gc_collect()
}
"#,
        )
        .expect("parse nested channel fixture");
        let generated = emit_c_from_ast(&file).expect("emit nested channel fixture");
        assert!(generated.contains("aura_task_channel_value_from_channel"));
        assert!(generated.contains("aura_task_channel_value_take_channel"));
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-channel-channel-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested channel fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested channel fixture");
        assert!(
            output.status.success(),
            "nested channel fixture failed: {output:?}"
        );
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_std_task_select_constructor_and_add() {
        let file = parse_file(
            r#"package std.task
pub class Box(private val value: Int) {}
pub class Select<T>(private val runtimeHandle: Int) {
  pub fun add(channel: Channel<T>): Select<T> { throw "intrinsic" }
  pub async fun next(): T? { throw "intrinsic" }
}
pub fun select<T>(): Select<T> { throw "intrinsic" }
async fun main() {
  val channel: Channel<Int> = Channel<Int>(1)
  val selector: Select<Int> = select<Int>()
  selector.add(channel)
  channel.send(9)
  val value: Int? = await selector.next()
  if (value != null) { println(value!!.toString()) }
  val boolChannel: Channel<Bool> = Channel<Bool>(1)
  val boolSelector: Select<Bool> = select<Bool>()
  boolSelector.add(boolChannel)
  boolChannel.send(true)
  val boolValue: Bool? = await boolSelector.next()
  val stringChannel: Channel<String> = Channel<String>(1)
  val stringSelector: Select<String> = select<String>()
  stringSelector.add(stringChannel)
  stringChannel.send("ok")
  val stringValue: String? = await stringSelector.next()
  val boxChannel: Channel<Box> = Channel<Box>(1)
  val boxSelector: Select<Box> = select<Box>()
  boxSelector.add(boxChannel)
  boxChannel.send(Box(3))
  val boxValue: Box? = await boxSelector.next()
}
"#,
        )
        .expect("parse select fixture");
        let generated = emit_c_from_ast(&file).expect("emit select fixture");
        assert!(generated.contains("aura_task_select_new"));
        assert!(generated.contains("aura_task_select_add"));
        assert!(generated.contains("aura_task_select_next"));
        assert!(generated.contains("aura_select_next_std_task_Select_Bool_result_destroy"));
        assert!(generated.contains("aura_select_next_std_task_Select_String_result_destroy"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-task-select-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile select fixture");
        let output = Command::new(&bin).output().expect("run select fixture");
        assert!(output.status.success(), "select fixture failed: {output:?}");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn task_scope_rethrows_after_cleanup() {
        let file = parse_file(
            r#"package std.task
pub fun taskScope(body: () -> Unit): Unit { throw "intrinsic" }
fun main() {
  try {
    taskScope(() => { throw "scope-failure" })
  } catch (error: String) {
    println(error)
  }
}
"#,
        )
        .expect("parse task scope exception fixture");
        let generated = emit_c_from_ast(&file).expect("emit task scope exception fixture");
        assert!(generated.contains("aura_task_scope_end(__scope)"));
        assert!(generated.contains("aura_ex_rethrow()"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-std-task-scope-exception-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile task scope exception fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run task scope exception fixture");
        assert!(
            output.status.success(),
            "task scope exception fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "scope-failure\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn immutable_array_lambda_capture_owns_snapshot_after_outer_scope() {
        let file = parse_file(
            r#"package demo
fun make(): () -> Int {
  val values: Array<Int> = Array<Int>(1)
  values.set(0, 73)
  return () => values.get(0)
}
fun main() {
  val read: () -> Int = make()
  println(read().toString())
}
"#,
        )
        .expect("parse immutable Array lambda capture fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit immutable Array lambda capture fixture");
        assert!(generated.contains("aura_method_Array_Int_clone(&values)"));
        assert!(generated.contains("aura_lenv_"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-array-lambda-snapshot-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile immutable Array lambda capture fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run immutable Array lambda capture fixture");
        assert!(
            output.status.success(),
            "Array lambda fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn immutable_class_array_lambda_capture_roots_snapshot_after_gc() {
        let file = parse_file(
            r#"package demo
class Box(var value: Int) {}
fun make(): () -> Int {
  val values: Array<Box> = Array<Box>(1)
  values.set(0, Box(73))
  return () => values.get(0).value
}
fun main() {
  val read: () -> Int = make()
  gc_collect()
  println(read().toString())
}
"#,
        )
        .expect("parse immutable class Array lambda capture fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit immutable class Array lambda capture fixture");
        assert!(generated.contains("aura_gc_add_array_root"));
        assert!(generated.contains("aura_gc_remove_array_root"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-array-class-lambda-snapshot-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile immutable class Array lambda capture fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run immutable class Array lambda capture fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn immutable_interface_array_lambda_capture_roots_snapshot_after_gc() {
        let file = parse_file(
            r#"package demo
interface Named { fun value(): Int }
class Box(var n: Int) : Named { fun value(): Int { return this.n } }
fun make(): () -> Int {
  val values: Array<Named> = Array<Named>(1)
  values.set(0, Box(73))
  return () => values.get(0).value()
}
fun main() {
  val read: () -> Int = make()
  gc_collect()
  println(read().toString())
}
"#,
        )
        .expect("parse immutable interface Array lambda capture fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit immutable interface Array lambda capture fixture");
        assert!(generated.contains("aura_iface_demo_Named_clone"));
        assert!(generated.contains("aura_gc_add_array_root"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!(
            "aura-array-interface-lambda-snapshot-{}",
            std::process::id()
        );
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile immutable interface Array lambda capture fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run immutable interface Array lambda capture fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn nested_interface_array_lambda_capture_roots_snapshot_after_gc() {
        let file = parse_file(
            r#"package demo
interface Named { fun value(): Int }
class Box(var n: Int) : Named { fun value(): Int { return this.n } }
fun make(): () -> Int {
  val inner: Array<Named> = Array<Named>(1)
  inner.set(0, Box(91))
  val values: Array<Array<Named>> = Array<Array<Named>>(1)
  values.set(0, inner)
  return () => values.get(0).get(0).value()
}
fun main() {
  val read: () -> Int = make()
  gc_collect()
  println(read().toString())
}
"#,
        )
        .expect("parse nested interface Array lambda capture fixture");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-nested-array-interface-lambda-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile nested interface Array lambda capture fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run nested interface Array lambda capture fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "91\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn mutable_array_lambda_capture_shares_owned_box_after_outer_scope() {
        let file = parse_file(
            r#"package demo
fun make(): () -> Int {
  var values: Array<Int> = Array<Int>(0)
  values.push(1)
  val read = () => values.len
  values.push(2)
  return read
}
fun main() {
  val read: () -> Int = make()
  println(read().toString())
}
"#,
        )
        .expect("parse mutable Array lambda capture fixture");
        let generated = emit_c_from_ast(&file).expect("emit mutable Array lambda capture fixture");
        assert!(generated.contains("aura_box_ptr_new"));
        assert!(generated.contains("aura_box_ptr_retain"));
        assert!(generated.contains("aura_capture_drop_Array_Int"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-array-lambda-mutable-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile mutable Array lambda capture fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run mutable Array lambda capture fixture");
        assert!(
            output.status.success(),
            "mutable Array lambda fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn mutable_array_lambda_capture_shares_one_owner_across_multiple_closures() {
        let file = parse_file(
            r#"package demo
fun make(): Int {
  var values: Array<Int> = Array<Int>(0)
  val first = () => values.len
  val second = () => values.len
  values.push(1)
  values.push(2)
  return first() + second()
}
fun main() {
  println(make().toString())
}
"#,
        )
        .expect("parse multiple escaping Array closures");
        let generated = emit_c_from_ast(&file).expect("emit multiple Array closures");
        assert!(generated.contains("aura_box_ptr_retain"));
        assert!(generated.contains("aura_capture_drop_Array_Int"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-array-lambda-two-owners-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile multiple Array closures");
        let output = Command::new(&bin)
            .output()
            .expect("run multiple Array closures");
        assert!(
            output.status.success(),
            "multiple closure fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "4\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn mutable_array_lambda_capture_survives_owner_rebinding_and_gc() {
        let file = parse_file(
            r#"package demo
fun make(): Int {
  var values: Array<Int> = Array<Int>(0)
  values.push(10)
  val append = (value: Int) => {
    values.push(value)
    return values.len
  }
  val before = append(20)
  values = Array<Int>(0)
  values.push(30)
  gc_collect()
  val after = append(40)
  return before + after + values.get(0)
}
fun main() { println(make().toString()) }
"#,
        )
        .expect("parse rebinding Array lambda capture fixture");
        let generated =
            emit_c_from_ast(&file).expect("emit rebinding Array lambda capture fixture");
        assert!(generated.contains("aura_box_ptr_retain"));
        assert!(generated.contains("aura_box_ptr_release"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-array-lambda-rebinding-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile rebinding Array lambda capture fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run rebinding Array lambda capture fixture");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "34\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_enum_heap_payload_across_await_gc() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
class Box(val value: Int) {}
async fun leaf(value: Int): Box { return Box(value) }
async fun choose(choice: Result<Box, TaskError>, task: Task<Box>): Box {
  gc_collect()
  val next: Box = await task
  gc_collect()
  match (choice) {
    case Ok(value) => { return value }
    case Err(error) => { return next }
  }
  return next
}
fun main() {
  val task = spawn { val value: Box = await choose(Ok(Box(73)), leaf(11)) return value }
  val first: Result<Box, TaskError> = join(task)
  match (first) {
    case Ok(value) => { println(value.value.toString()) }
    case Err(error) => { println("error") }
  }
  gc_collect()
  val second: Result<Box, TaskError> = join(task)
  match (second) {
    case Ok(value) => { println(value.value.toString()) }
    case Err(error) => { println("repeat-error") }
  }
}
"#,
        )
        .expect("parse enum heap payload await fixture");
        let generated = emit_c_from_ast(&file).expect("emit enum heap payload await fixture");
        assert!(generated.contains("_mark(const aura_enum_"));
        assert!(generated.contains("aura_gc_mark_ptr((void *)value->data.Ok.value)"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-enum-heap-payload-await-gc-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile enum heap payload await fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run enum heap payload await fixture");
        assert!(
            output.status.success(),
            "enum heap payload await fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "73\n73\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }

    #[test]
    fn builds_and_runs_async_array_throw_catch_across_await() {
        let file = aura_parser::parse_file(
            r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
async fun fail(): Array<Int> {
  val marker: Int = await ready()
  val values: Array<Int> = Array<Int>(2)
  throw values
}
async fun ready(): Int { return 1 }
async fun recover(): Int {
  try {
    val ignored: Array<Int> = await fail()
    return 0
  } catch (error: Array<Int>) {
    return error.len
  }
}
fun main() {
  val task = spawn { val value: Int = await recover() return value }
  val result: Result<Int, TaskError> = join(task)
  match (result) {
    case Ok(value) => { println(value.toString()) }
    case Err(error) => { println("error") }
  }
}
"#,
        )
        .expect("parse async array throw/catch fixture");
        let generated = emit_c_from_ast(&file).expect("emit async array throw/catch fixture");
        assert!(generated.contains("aura_async_cfg_array_error_clone"));
        assert!(generated.contains("aura_task_frame_set_error_payload_with_clone"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-async-array-throw-catch-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/runtime.c"))
            .expect("compile async array throw/catch fixture");
        let output = Command::new(&bin)
            .output()
            .expect("run async array throw/catch fixture");
        assert!(
            output.status.success(),
            "async array throw/catch fixture failed: {output:?}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
        let _ = fs::remove_file(bin);
        let _ = fs::remove_file(generated_c);
    }
}
