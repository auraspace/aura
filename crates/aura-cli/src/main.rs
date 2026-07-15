//! Minimal Aura CLI for milestone C0: `aura check <file.aura>`.

use aura_parser::parse_file;
use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        eprint_usage();
        return ExitCode::from(2);
    }

    let cmd = args.remove(0);
    match cmd.as_str() {
        "check" => {
            if args.is_empty() {
                eprintln!("error: missing path\n  usage: aura check <file.aura>");
                return ExitCode::from(2);
            }
            let path = Path::new(&args[0]);
            match check_file(path) {
                Ok(summary) => {
                    println!("{summary}");
                    ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprintln!("error: {msg}");
                    ExitCode::from(1)
                }
            }
        }
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
        "Aura toolchain (C0)\n\n\
         Usage:\n  \
           aura check <file.aura>   Parse file and print a short summary\n  \
           aura help\n\n\
         See docs/roadmap.md and RFC-001 §6.0 for the MVP surface."
    );
}

fn check_file(path: &Path) -> Result<String, String> {
    let src = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let file = parse_file(&src).map_err(|e| format!("{}: {e}", path.display()))?;

    let pkg = file
        .package
        .segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".");

    let mut lines = Vec::new();
    lines.push(format!("ok  {}", path.display()));
    lines.push(format!("package {pkg}"));
    lines.push(format!("{} function(s)", file.functions.len()));
    for f in &file.functions {
        let ret = f
            .return_type
            .as_ref()
            .map(|t| {
                if t.nullable {
                    format!("{}?", t.name.name)
                } else {
                    t.name.name.clone()
                }
            })
            .unwrap_or_else(|| "Unit".into());
        lines.push(format!(
            "  fun {}({}) -> {}  ({} stmt)",
            f.name.name,
            f.params
                .iter()
                .map(|p| format!(
                    "{}: {}{}",
                    p.name.name,
                    p.ty.name.name,
                    if p.ty.nullable { "?" } else { "" }
                ))
                .collect::<Vec<_>>()
                .join(", "),
            ret,
            f.body.stmts.len()
        ));
    }
    Ok(lines.join("\n"))
}
