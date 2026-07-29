//! Aura language server entrypoint.

fn main() {
    if let Err(error) = aura_lsp::run_stdio() {
        eprintln!("auralsp: {error}");
        std::process::exit(1);
    }
}
