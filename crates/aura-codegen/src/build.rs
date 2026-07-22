//! Shell out to a C compiler.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aura_ast::File;
use aura_sema::check_file;

use crate::ctx::EmitOptions;
use crate::emit::{emit_c, emit_c_with};
use crate::error::CodegenError;

pub fn emit_c_from_ast(file: &File) -> Result<String, CodegenError> {
    let checked = check_file(file)?;
    Ok(emit_c(&checked))
}

pub fn emit_c_tests_from_ast(file: &File) -> Result<String, CodegenError> {
    let checked = check_file(file)?;
    Ok(emit_c_with(&checked, EmitOptions { test: true }))
}

/// Typecheck + emit C + compile with the system C compiler (`CC` or `cc`).
pub fn build_from_file(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    build_from_file_with(file, out_bin, runtime_c, EmitOptions::default())
}

pub fn build_tests_from_file(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
) -> Result<PathBuf, CodegenError> {
    build_from_file_with(file, out_bin, runtime_c, EmitOptions { test: true })
}

pub(crate) fn build_from_file_with(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
    opts: EmitOptions,
) -> Result<PathBuf, CodegenError> {
    let checked = check_file(file)?;
    let c_src = emit_c_with(&checked, opts);

    let parent = out_bin
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent).map_err(|e| CodegenError::Io(e.to_string()))?;

    let c_path = parent.join(format!(
        "{}.aura.c",
        out_bin
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("out")
    ));
    fs::write(&c_path, c_src).map_err(|e| CodegenError::Io(e.to_string()))?;

    let compiler = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    let status = Command::new(&compiler)
        .arg("-O0")
        .arg("-std=c11")
        .arg(&c_path)
        .arg(runtime_c)
        .arg("-o")
        .arg(out_bin)
        .status()
        .map_err(|e| CodegenError::Compile(format!("failed to spawn {compiler}: {e}")))?;

    if !status.success() {
        return Err(CodegenError::Compile(format!(
            "{compiler} failed with status {status} (source {})",
            c_path.display()
        )));
    }

    Ok(out_bin.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use aura_ast::{
        AsyncExpr, AsyncFunDecl, Block, CallExpr, CancelExpr, ChannelCloseExpr, ChannelCreateExpr,
        ChannelReceiveExpr, ChannelSendExpr, Expr, File, FunDecl, Ident, IntLit, JoinExpr, Path,
        ReturnStmt, Span, SpawnExpr, Stmt, TypeRef,
    };

    use super::build_from_file;

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
            async_functions: vec![async_fun],
            span,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        let dir = std::env::temp_dir();
        let bin = dir.join(format!("aura-c22l-{}", std::process::id()));
        let runtime = root.join("runtime/aura_rt.c");
        build_from_file(&file, &bin, &runtime).expect("compile generated async C");
        let status = Command::new(&bin).status().expect("run generated binary");
        assert!(status.success());
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(dir.join(format!("aura-c22l-{}.aura.c", std::process::id())));
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
        let status = Command::new(&bin).status().expect("run generated binary");
        assert!(status.success());
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(dir.join(format!("aura-c22m-{}.aura.c", std::process::id())));
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
