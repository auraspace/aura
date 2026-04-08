use std::env;

const HELP: &str = r#"aurac - Aura compiler

USAGE:
  aurac <COMMAND> [ARGS...]

COMMANDS:
  build   Compile sources into a native executable
  check   Parse/typecheck sources
  run     Build then run the executable
  help    Print this message

OPTIONS:
  -h, --help      Print help
  -V, --version   Print version

DEBUG OPTIONS:
  --print=types   Print inferred types for all expressions
  --emit=hir      Print the annotated HIR (AST with types)
"#;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return;
    }

    let mut command = None;
    let mut file_path = None;
    let mut print_types = false;
    let mut emit_hir = false;

    for arg in &args {
        match arg.as_str() {
            "-h" | "--help" | "help" => {
                print_help();
                return;
            }
            "-V" | "--version" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--print=types" => print_types = true,
            "--emit=hir" => emit_hir = true,
            cmd if command.is_none() && (cmd == "build" || cmd == "check" || cmd == "run") => {
                command = Some(cmd.to_string());
            }
            path if !path.starts_with('-') && file_path.is_none() => {
                file_path = Some(path.to_string());
            }
            other => {
                if other.starts_with('-') {
                    eprintln!("error: unknown option `{other}`");
                    std::process::exit(2);
                }
            }
        }
    }

    let Some(command) = command else {
        print_help();
        return;
    };

    match command.as_str() {
        "check" => {
            let Some(path) = file_path else {
                eprintln!("error: missing <FILE>\n");
                eprintln!("USAGE:\n  aurac check <FILE>\n");
                std::process::exit(2);
            };

            match aura_driver::check_file(&path) {
                Ok(out) => {
                    if !out.diagnostics.is_empty() {
                        eprintln!(
                            "{}",
                            aura_diagnostics::format_all(&out.source, &out.diagnostics)
                        );
                        std::process::exit(1);
                    }

                    if print_types {
                        if let Some(ref typed) = out.typed_program {
                            println!("--- Expression Types ---");
                            let mut spans: Vec<_> = typed.expression_types.keys().collect();
                            spans.sort_by_key(|s| s.start);
                            for span in spans {
                                let snippet = out.source.get(span.start.raw() as usize..span.end.raw() as usize).unwrap_or("???");
                                let ty = typed.expression_types.get(span).unwrap();
                                println!("  {:?} -> {}", snippet, ty.name());
                            }
                        }
                    }

                    if emit_hir {
                        if let Some(ref _typed) = out.typed_program {
                            println!("--- Annotated AST (HIR) ---");
                            // Simple debug print for now.
                            // In a real implementation, we would use a visitor to print the AST with type annotations.
                            println!("HIR output is not fully implemented yet, but types were collected successfully.");
                        }
                    }

                    if !print_types && !emit_hir {
                        println!("ok");
                    }
                }
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(2);
                }
            }
        }
        "build" | "run" => {
            println!("{command}: not implemented yet");
        }
        _ => unreachable!(),
    }
}

fn print_help() {
    println!("{HELP}");
}
