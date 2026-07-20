//! Aura CLI — check / build / run / test / emit-c with pretty diagnostics.

mod package;

use aura_codegen::{build_from_file, build_tests_from_file, emit_c_from_ast};
use aura_diagnostics::format_error;
use aura_sema::{check_file, SemaError, SemaErrors};
use package::{load_package, load_package_default, LoadedPackage};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        eprint_usage();
        return ExitCode::from(2);
    }

    let cmd = args.remove(0);
    match cmd.as_str() {
        "check" => cmd_check(&args),
        "build" => cmd_build(&args),
        "run" => cmd_run(&args),
        "test" => cmd_test(&args),
        "emit-c" => cmd_emit_c(&args),
        "help" | "-h" | "--help" => {
            eprint_usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("error: unknown command `{other}`");
            eprint_usage();
            ExitCode::from(2)
        }
    }
}

fn eprint_usage() {
    eprintln!(
        "Aura toolchain (C0–C3f)\n\n\
         Usage:\n  \
           aura check [path]                 Parse + typecheck (.aura | dir | aura.toml)\n  \
           aura build [path] [-o <bin>]      Compile to native binary (C backend)\n  \
           aura run [path]                   Build to temp and execute\n  \
           aura test [path]                  Run @test functions (package-wide)\n  \
           aura emit-c [path]                Print generated C (debug)\n  \
           aura help\n\n\
         Path may be a `.aura` file, a package directory, or `aura.toml`.\n\
         With no path, commands look for `./aura.toml`.\n\n\
         See docs/roadmap.md and RFC-001 §6.0 / RFC-005 / RFC-008."
    );
}

fn resolve_package(args: &[String]) -> Result<LoadedPackage, String> {
    if args.is_empty() {
        load_package_default()
    } else {
        load_package(Path::new(&args[0]))
    }
}

fn diag_sema(pkg: &LoadedPackage, e: &SemaError) -> String {
    let (path, src, span) = pkg.locate(e.span);
    format_error(&path, src, &e.message, span)
}

fn diag_sema_errors(pkg: &LoadedPackage, es: SemaErrors) -> String {
    es.errors
        .iter()
        .map(|e| diag_sema(pkg, e))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn cmd_check(args: &[String]) -> ExitCode {
    match resolve_package(args).and_then(check_package) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn check_package(pkg: LoadedPackage) -> Result<String, String> {
    let checked = check_file(&pkg.ast).map_err(|e| diag_sema_errors(&pkg, e))?;

    let mut lines = Vec::new();
    if pkg.sources.len() == 1 {
        lines.push(format!("ok  {}", pkg.sources[0].path.display()));
    } else {
        lines.push(format!(
            "ok  {} ({} files)",
            pkg.root.display(),
            pkg.sources.len()
        ));
        for s in &pkg.sources {
            lines.push(format!("  file {}", s.path.display()));
        }
    }
    lines.push(format!("package {}", checked.package));
    if !checked.interfaces.is_empty() {
        lines.push(format!("{} interface(s)", checked.interfaces.len()));
        for i in &checked.interfaces {
            lines.push(format!(
                "  interface {} ({} method(s))",
                i.name,
                i.methods.len()
            ));
        }
    }
    if !checked.enums.is_empty() {
        lines.push(format!("{} enum(s)", checked.enums.len()));
        for e in &checked.enums {
            lines.push(format!(
                "  enum {} ({} variant(s))",
                e.name,
                e.variants.len()
            ));
        }
    }
    if !checked.classes.is_empty() {
        let n_cls = checked.classes.iter().filter(|c| !c.is_struct).count();
        let n_st = checked.classes.iter().filter(|c| c.is_struct).count();
        if n_cls > 0 {
            lines.push(format!("{n_cls} class(es)"));
        }
        if n_st > 0 {
            lines.push(format!("{n_st} struct(s)"));
        }
        for c in &checked.classes {
            let kind = if c.is_struct { "struct" } else { "class" };
            let impls = if c.implements.is_empty() {
                String::new()
            } else {
                format!(
                    " : {}",
                    c.implements
                        .iter()
                        .map(|t| t.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            lines.push(format!(
                "  {kind} {}{} ({} field(s), {} method(s))",
                c.name,
                impls,
                c.fields.len(),
                c.methods.len()
            ));
        }
    }
    lines.push(format!(
        "{} function(s) typechecked",
        checked.functions.len()
    ));
    let n_tests = checked.functions.iter().filter(|f| f.is_test).count();
    if n_tests > 0 {
        lines.push(format!("{n_tests} @test function(s)"));
    }
    for f in &checked.functions {
        let mark = if f.is_test { " @test" } else { "" };
        lines.push(format!(
            "  fun{} {}({}) -> {}",
            mark,
            f.name,
            f.params
                .iter()
                .map(|t| t.display())
                .collect::<Vec<_>>()
                .join(", "),
            f.ret.display()
        ));
    }
    Ok(lines.join("\n"))
}

fn cmd_emit_c(args: &[String]) -> ExitCode {
    match resolve_package(args).and_then(|pkg| {
        emit_c_from_ast(&pkg.ast).map_err(|e| match e {
            aura_codegen::CodegenError::Sema(se) => diag_sema_errors(&pkg, se),
            other => format!("error: {other}"),
        })
    }) {
        Ok(c) => {
            print!("{c}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_build(args: &[String]) -> ExitCode {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: -o requires a path");
                    return ExitCode::from(2);
                }
                output = Some(PathBuf::from(&args[i]));
            }
            s if s.starts_with('-') => {
                eprintln!("error: unknown option `{s}`");
                return ExitCode::from(2);
            }
            s => {
                if input.is_some() {
                    eprintln!("error: unexpected argument `{s}`");
                    return ExitCode::from(2);
                }
                input = Some(PathBuf::from(s));
            }
        }
        i += 1;
    }

    let pkg = match input {
        Some(p) => load_package(&p),
        None => load_package_default(),
    };
    let pkg = match pkg {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };

    let out = output.unwrap_or_else(|| PathBuf::from(format!("target/aura/{}", pkg.bin_name)));
    match build_package(&pkg, &out) {
        Ok(bin) => {
            println!("ok  {}", bin.display());
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn runtime_c_path() -> Result<PathBuf, String> {
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/aura_rt.c"),
        PathBuf::from("runtime/aura_rt.c"),
        PathBuf::from("../runtime/aura_rt.c"),
    ];
    for c in candidates {
        if c.is_file() {
            return Ok(c.canonicalize().unwrap_or(c));
        }
    }
    Err(
        "error: cannot find runtime/aura_rt.c (run from repo root or via cargo run -p aura-cli)"
            .into(),
    )
}

fn build_package(pkg: &LoadedPackage, out: &Path) -> Result<PathBuf, String> {
    let rt = runtime_c_path()?;
    build_from_file(&pkg.ast, out, &rt).map_err(|e| match e {
        aura_codegen::CodegenError::Sema(se) => diag_sema_errors(pkg, se),
        other => format!("error: {other}"),
    })
}

fn cmd_run(args: &[String]) -> ExitCode {
    let pkg = match resolve_package(args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let out = PathBuf::from(format!("target/aura/run-{}", pkg.bin_name));
    match build_package(&pkg, &out) {
        Ok(bin) => {
            let status = Command::new(&bin).status();
            match status {
                Ok(s) if s.success() => ExitCode::SUCCESS,
                Ok(s) => {
                    eprintln!("error: program exited with {s}");
                    ExitCode::from(s.code().unwrap_or(1) as u8)
                }
                Err(e) => {
                    eprintln!("error: failed to execute {}: {e}", bin.display());
                    ExitCode::from(1)
                }
            }
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_test(args: &[String]) -> ExitCode {
    let pkg = match resolve_package(args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let n_tests = pkg.ast.functions.iter().filter(|f| f.is_test).count();
    if n_tests == 0 {
        eprintln!(
            "error: no @test functions found in package `{}` ({} file(s))",
            pkg.package,
            pkg.sources.len()
        );
        return ExitCode::from(1);
    }
    let out = PathBuf::from(format!("target/aura/test-{}", pkg.bin_name));
    match build_test_package(&pkg, &out) {
        Ok(bin) => {
            let status = Command::new(&bin).status();
            match status {
                Ok(s) if s.success() => ExitCode::SUCCESS,
                Ok(s) => ExitCode::from(s.code().unwrap_or(1) as u8),
                Err(e) => {
                    eprintln!("error: failed to execute {}: {e}", bin.display());
                    ExitCode::from(1)
                }
            }
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn build_test_package(pkg: &LoadedPackage, out: &Path) -> Result<PathBuf, String> {
    let rt = runtime_c_path()?;
    build_tests_from_file(&pkg.ast, out, &rt).map_err(|e| match e {
        aura_codegen::CodegenError::Sema(se) => diag_sema_errors(pkg, se),
        other => format!("error: {other}"),
    })
}
