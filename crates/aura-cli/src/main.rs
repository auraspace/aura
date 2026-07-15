//! Aura CLI — check / build / run / emit-c with pretty diagnostics.

use aura_codegen::{build_from_file, emit_c_from_ast};
use aura_diagnostics::format_error;
use aura_parser::{parse_file, ParseError};
use aura_sema::{check_file, SemaError};
use std::env;
use std::fs;
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
        "Aura toolchain (C0–C3c)\n\n\
         Usage:\n  \
           aura check <file.aura>              Parse + typecheck\n  \
           aura build <file.aura> [-o <bin>]   Compile to native binary (C backend)\n  \
           aura run <file.aura>                Build to temp and execute\n  \
           aura emit-c <file.aura>             Print generated C (debug)\n  \
           aura help\n\n\
         See docs/roadmap.md and RFC-001 §6.0."
    );
}

fn diag_parse(path: &Path, src: &str, e: ParseError) -> String {
    format_error(&path.display().to_string(), src, &e.message, e.span)
}

fn diag_sema(path: &Path, src: &str, e: SemaError) -> String {
    format_error(&path.display().to_string(), src, &e.message, e.span)
}

fn cmd_check(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("error: missing path\n  usage: aura check <file.aura>");
        return ExitCode::from(2);
    }
    let path = Path::new(&args[0]);
    match check_path(path) {
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

fn check_path(path: &Path) -> Result<String, String> {
    let src = fs::read_to_string(path).map_err(|e| format!("error: read {}: {e}", path.display()))?;
    let file = parse_file(&src).map_err(|e| diag_parse(path, &src, e))?;
    let checked = check_file(&file).map_err(|e| diag_sema(path, &src, e))?;

    let mut lines = Vec::new();
    lines.push(format!("ok  {}", path.display()));
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
                format!(" : {}", c.implements.join(", "))
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
    for f in &checked.functions {
        lines.push(format!(
            "  fun {}({}) -> {}",
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
    if args.is_empty() {
        eprintln!("error: missing path\n  usage: aura emit-c <file.aura>");
        return ExitCode::from(2);
    }
    let path = Path::new(&args[0]);
    match load_and_emit_c(path) {
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

fn load_and_emit_c(path: &Path) -> Result<String, String> {
    let src = fs::read_to_string(path).map_err(|e| format!("error: read {}: {e}", path.display()))?;
    let file = parse_file(&src).map_err(|e| diag_parse(path, &src, e))?;
    emit_c_from_ast(&file).map_err(|e| match e {
        aura_codegen::CodegenError::Sema(se) => diag_sema(path, &src, se),
        other => format!("error: {}: {other}", path.display()),
    })
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

    let Some(input) = input else {
        eprintln!("error: missing path\n  usage: aura build <file.aura> [-o <bin>]");
        return ExitCode::from(2);
    };

    let out = output.unwrap_or_else(|| default_out_path(&input));
    match build_path(&input, &out) {
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

fn default_out_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a.out");
    PathBuf::from(format!("target/aura/{stem}"))
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

fn build_path(input: &Path, out: &Path) -> Result<PathBuf, String> {
    let src = fs::read_to_string(input).map_err(|e| format!("error: read {}: {e}", input.display()))?;
    let file = parse_file(&src).map_err(|e| diag_parse(input, &src, e))?;
    let rt = runtime_c_path()?;
    build_from_file(&file, out, &rt).map_err(|e| match e {
        aura_codegen::CodegenError::Sema(se) => diag_sema(input, &src, se),
        other => format!("error: {}: {other}", input.display()),
    })
}

fn cmd_run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("error: missing path\n  usage: aura run <file.aura>");
        return ExitCode::from(2);
    }
    let input = Path::new(&args[0]);
    let out = PathBuf::from(format!(
        "target/aura/run-{}",
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("prog")
    ));
    match build_path(input, &out) {
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
