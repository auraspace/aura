//! Shell out to a C compiler.

use std::path::{Path, PathBuf};

use aura_ast::File;

use crate::ctx::EmitOptions;
use crate::driver::{Artifact, CBackend, Driver};
use crate::error::CodegenError;
use crate::options::CompileOptions;

pub fn emit_c_from_ast(file: &File) -> Result<String, CodegenError> {
    Driver::new(CBackend).emit(file, EmitOptions::default())
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
    use std::{fs, process::Command};

    use aura_ast::{
        AsyncExpr, AsyncFunDecl, AwaitExpr, Block, CallExpr, CancelExpr, ChannelCloseExpr,
        ChannelCreateExpr, ChannelReceiveExpr, ChannelSendExpr, Expr, File, FunDecl, Ident, IntLit,
        JoinExpr, Path, ReturnStmt, Span, SpawnExpr, Stmt, TypeRef,
    };

    use super::{build_from_file, build_from_file_with, emit_c_from_ast};
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
            &root.join("runtime/aura_rt.c"),
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
        let runtime = root.join("runtime/aura_rt.c");
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
            assert_eq!(
                first_bytes,
                fs::read(second.path()).expect("read second artifact")
            );
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
        let runtime = dir.join(format!("{stem}.runtime.c"));
        let generated_c = dir.join(format!("{stem}.aura.c"));
        let source = fs::read_to_string(root.join("runtime/aura_rt.c")).expect("read runtime");
        let mismatched = source.replace(
            "#define AURA_RT_ABI_VERSION 1u",
            "#define AURA_RT_ABI_VERSION 2u",
        );
        assert_ne!(source, mismatched, "test must change runtime ABI version");
        fs::write(&runtime, mismatched).expect("write mismatched runtime");

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
        let _ = fs::remove_file(runtime);
        let _ = fs::remove_file(generated_c);
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
        let runtime = dir.join(format!("{stem}.runtime.c"));
        let generated_c = dir.join(format!("{stem}.aura.c"));
        let source = fs::read_to_string(root.join("runtime/aura_rt.c")).expect("read runtime");
        let mismatched = source.replace(
            "aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1",
            "aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=2",
        );
        assert_ne!(source, mismatched, "test must change the FFI ABI identity");
        fs::write(&runtime, mismatched).expect("write mismatched runtime");

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
        let _ = fs::remove_file(runtime);
        let _ = fs::remove_file(generated_c);
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
        let runtime = root.join("runtime/aura_rt.c");
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
            &root.join("runtime/aura_rt.c"),
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
        let runtime = root.join("runtime/aura_rt.c");
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        ));
        assert!(generated.contains("static int64_t aura_async_resume_demo_preserve("));
        assert!(generated.contains("data->before = before;"));
        assert!(generated.contains("data->label = label;"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
        assert!(generated.contains("aura_async_resume_demo_preserve(data, observed)"));
        assert!(generated.contains("aura async suspension state=1 kind=await"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-await-hoist-{}", std::process::id()));
        let generated_c = dir.join(format!("aura-await-hoist-{}.aura.c", std::process::id()));
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        assert!(generated.contains("aura_async_resume_demo_wrapper"));
        assert!(!generated.contains("await lowering is deferred"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let stem = format!("aura-return-await-{}", std::process::id());
        let bin = dir.join(&stem);
        let generated_c = dir.join(format!("{stem}.aura.c"));
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
            .expect("compile return-await fixture");
        assert!(Command::new(&bin)
            .status()
            .expect("run return-await fixture")
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
        assert!(generated.contains("AuraTaskFrame *await_task_0;"));
        assert!(generated.contains("AuraTaskFrame *await_task_1;"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task_0)"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task_1)"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task_0)"));
        assert!(generated.contains("return AURA_TASK_CANCELLED;"));
        assert!(generated.contains("data->label__owned = true;"));
        assert!(generated.contains("if (data->label__owned) free((void *)data->label);"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-await-two-{}", std::process::id()));
        let generated_c = dir.join(format!("aura-await-two-{}.aura.c", std::process::id()));
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        assert!(generated.contains("AuraTaskFrame *await_task_0;"));
        assert!(generated.contains("AuraTaskFrame *await_task_1;"));
        assert!(generated.contains("AuraTaskFrame *await_task_2;"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task_2)"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task_2)"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 3)"));
        assert!(generated.contains("if (data->middle__owned) free((void *)data->middle);"));

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-await-three-{}", std::process::id()));
        let generated_c = dir.join(format!("aura-await-three-{}.aura.c", std::process::id()));
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
            .expect("compile three-await fixture");
        let status = Command::new(&bin)
            .status()
            .expect("run three-await fixture");
        assert!(status.success());
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
        let runtime = root.join("runtime/aura_rt.c");
        build_from_file(&file, &bin, &runtime).expect("compile generated C22m");
        let generated =
            std::fs::read_to_string(dir.join(format!("aura-c22m-{}.aura.c", std::process::id())))
                .expect("read generated join C");
        assert!(generated.contains("aura_task_executor_join(__aura_task_executor"));
        assert!(generated.contains("joined task failed"));
        assert!(generated
            .contains("__join_state != AURA_TASK_COMPLETE && __join_state != AURA_TASK_CANCELLED"));
        let status = Command::new(&bin).status().expect("run generated binary");
        assert!(status.success());
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(dir.join(format!("aura-c22m-{}.aura.c", std::process::id())));
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
            .expect("compile bounded non-empty spawn");
        let generated = fs::read_to_string(&generated_c).expect("read generated bounded spawn C");
        assert!(generated.contains("aura_spawn_poll_"));
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
            "package demo\nfun report(value: Int) { if (value == 41) { println(\"local captured\") } }\nfun main() { val captured: Int = 41\nval task = spawn { report(captured) } join(task) }\n",
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
            "package demo\nfun report(values: Array<Int>) { if (values.len == 3) { println(\"local array\") } }\nfun main() { val captured: Array<Int> = Array<Int>(3)\nval task = spawn { report(captured) } join(task) }\n",
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
        build_from_file(&file, &bin, &root.join("runtime/aura_rt.c"))
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
    fn unsupported_spawn_body_keeps_stable_failure_path() {
        let file = aura_parser::parse_file(
            "package demo\nfun main() { val task = spawn { val later = 1 } cancel(task) }\n",
        )
        .expect("parse unsupported spawn");
        let generated = emit_c_from_ast(&file).expect("emit unsupported spawn path");
        assert!(generated.contains("non-empty spawn body requires C22l state-machine lowering"));
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
        let runtime = root.join("runtime/aura_rt.c");
        build_from_file(&file, &bin, &runtime).expect("compile generated C22o");
        let status = Command::new(&bin)
            .status()
            .expect("run generated C22o binary");
        assert!(status.success());
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(dir.join(format!("aura-c22o-{}.aura.c", std::process::id())));
    }
}
