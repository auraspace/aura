use std::env;

const HELP: &str = r#"aurac - Aura compiler

USAGE:
  aurac <COMMAND>

COMMANDS:
  build   Compile sources into a native executable
  check   Typecheck/validate sources (no output)
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
            // TODO: wire into aura-driver once available.
            println!("check: not implemented yet");
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
