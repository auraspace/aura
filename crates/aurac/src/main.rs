use std::env;

const HELP: &str = r#"aurac - Aura compiler

USAGE:
  aurac <COMMAND> [ARGS...]

COMMANDS:
  build   Compile sources into a native executable
  check   Parse/typecheck sources (no output)
  run     Build then run the executable
  help    Print this message

OPTIONS:
  -h, --help      Print help
  -V, --version   Print version
"#;

fn main() {
    let mut args = env::args().skip(1);
    let Some(arg1) = args.next() else {
        print_help();
        return;
    };

    match arg1.as_str() {
        "-h" | "--help" | "help" => {
            print_help();
        }
        "-V" | "--version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        "build" => {
            // TODO: wire into aura-driver once available.
            println!("build: not implemented yet");
        }
        "check" => {
            let Some(path) = args.next() else {
                eprintln!("error: missing <FILE>\n");
                eprintln!("USAGE:\n  aurac check <FILE>\n");
                std::process::exit(2);
            };

            match aura_driver::check_file(&path) {
                Ok(out) => {
                    if out.diagnostics.is_empty() {
                        println!("ok");
                    } else {
                        eprintln!("{}", aura_diagnostics::format_all(&out.source, &out.diagnostics));
                        std::process::exit(1);
                    }
                }
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(2);
                }
            }
        }
        "run" => {
            // TODO: wire into aura-driver once available.
            println!("run: not implemented yet");
        }
        other => {
            eprintln!("error: unknown command `{other}`\n");
            print_help();
            std::process::exit(2);
        }
    }
}

fn print_help() {
    print!("{HELP}");
}
