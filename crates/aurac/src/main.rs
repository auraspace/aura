use aura_codegen::Backend;
use aura_driver::modules::{build_module_graph, build_symbol_table};
use std::env;
use std::path::Path;

const HELP: &str = r#"aurac - Aura compiler

USAGE:
  aurac <COMMAND> [ARGS...]

COMMANDS:
  build                 Compile sources into a native executable
  check                 Parse/typecheck sources
  run                   Build then run the executable
  help                  Print this help message

OPTIONS:
  -v, --version         Print version information
  -h, --help            Print this help message

DEBUG OPTIONS:
  --emit=ast            Print the parsed AST
  --print=types         Print inferred types for all expressions
  --print=symbols       Print top-level symbols for the entry module
  --print=imports       Print resolved imports for the entry module
  --emit=hir            Print the annotated HIR (AST with types)
  --emit=mir            Print the generated Mid-level IR (MIR)
  --emit=llvm           Emit LLVM IR (.ll)
  --emit=asm            Emit assembly (.s)
  --emit=obj            Emit object file (.o)
  --backend=llvm|clif   Select codegen backend (default: llvm)
"#;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return;
    }

    let mut command = None;
    let mut file_path = None;
    let mut emit_ast = false;
    let mut print_types = false;
    let mut print_symbols = false;
    let mut print_imports = false;
    let mut emit_hir = false;
    let mut emit_mir = false;
    let mut emit_llvm = false;
    let mut emit_asm = false;
    let mut emit_obj = false;
    let mut backend_kind = aura_codegen::BackendKind::Llvm;

    for arg in &args {
        match arg.as_str() {
            "-h" | "--help" | "help" => {
                print_help();
                return;
            }
            "-v" | "--version" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return;
            }
            "--emit=ast" => emit_ast = true,
            "--print=types" => print_types = true,
            "--print=symbols" => print_symbols = true,
            "--print=imports" => print_imports = true,
            "--emit=hir" => emit_hir = true,
            "--emit=mir" => emit_mir = true,
            "--emit=llvm" => emit_llvm = true,
            "--emit=asm" => emit_asm = true,
            "--emit=obj" => emit_obj = true,
            value if value.starts_with("--backend=") => {
                let backend_name = value.trim_start_matches("--backend=");
                backend_kind = match aura_codegen::BackendKind::parse(backend_name) {
                    Ok(kind) => kind,
                    Err(err) => {
                        eprintln!("error: {err}");
                        std::process::exit(2);
                    }
                };
            }
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

                    if let Err(err) = print_debug_views(
                        &path,
                        emit_ast,
                        print_types,
                        print_symbols,
                        print_imports,
                        emit_hir,
                        emit_mir,
                    ) {
                        eprintln!("error: {err}");
                        std::process::exit(2);
                    }

                    if !emit_ast
                        && !print_types
                        && !print_symbols
                        && !print_imports
                        && !emit_hir
                        && !emit_mir
                    {
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
            let Some(path) = file_path else {
                eprintln!("error: missing <FILE>\n");
                eprintln!("USAGE:\n  aurac {} <FILE>\n", command);
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

                    if let Err(err) = print_debug_views(
                        &path,
                        emit_ast,
                        print_types,
                        print_symbols,
                        print_imports,
                        emit_hir,
                        emit_mir,
                    ) {
                        eprintln!("error: {err}");
                        std::process::exit(2);
                    }

                    let mir = out.mir.as_ref().expect("MIR should exist if no errors");

                    let backend_capabilities = backend_kind.capabilities();
                    if emit_llvm && !backend_capabilities.supports_emit_llvm {
                        eprintln!(
                            "error: backend `{}` does not support `--emit=llvm` yet",
                            backend_kind.name()
                        );
                        std::process::exit(1);
                    }
                    if emit_asm && !backend_capabilities.supports_emit_asm {
                        eprintln!(
                            "error: backend `{}` does not support `--emit=asm` yet",
                            backend_kind.name()
                        );
                        std::process::exit(1);
                    }

                    let target = aura_codegen::Target::default();
                    let build_dir = std::path::Path::new(".");

                    let (obj_path, backend_name) = match backend_kind {
                        aura_codegen::BackendKind::Llvm => {
                            let context = inkwell::context::Context::create();
                            let backend = aura_codegen_llvm::LlvmBackend::new(
                                &context,
                                "aura_module",
                                &target,
                            )
                            .expect("Failed to create LLVM backend");

                            let obj_path = match backend.compile(mir, build_dir) {
                                Ok(path) => path,
                                Err(err) => {
                                    eprintln!("error: failed to compile with llvm backend: {err}");
                                    std::process::exit(1);
                                }
                            };

                            if emit_llvm {
                                if let Err(err) =
                                    backend.emit_llvm(mir, std::path::Path::new("main.ll"))
                                {
                                    eprintln!("error: failed to emit LLVM: {err}");
                                    std::process::exit(1);
                                }
                            }
                            if emit_asm {
                                if let Err(err) =
                                    backend.emit_asm(mir, std::path::Path::new("main.s"))
                                {
                                    eprintln!("error: failed to emit ASM: {err}");
                                    std::process::exit(1);
                                }
                            }

                            if emit_obj {
                                // obj is already at obj_path (main.o)
                            }

                            (obj_path, "llvm")
                        }
                        aura_codegen::BackendKind::Clif => {
                            let backend = aura_codegen_clif::ClifBackend::new();
                            let obj_path = match backend.compile(mir, build_dir) {
                                Ok(path) => path,
                                Err(err) => {
                                    eprintln!("error: failed to compile with clif backend: {err}");
                                    std::process::exit(1);
                                }
                            };
                            (obj_path, "clif")
                        }
                    };

                    let exe_path = Path::new("a.out");
                    let run_path = Path::new("./a.out");
                    if let Err(err) = target.ensure_linking_supported() {
                        eprintln!("error: {err}");
                        std::process::exit(1);
                    }
                    let linker = aura_link::Linker::new(target.linker_triple().to_string());
                    // For MVP, look for the runtime in the target directory
                    let mut runtime_path = build_dir.join("target/debug/libaura_rt.a");
                    if !runtime_path.exists() {
                        // Try workspace root if not in current dir
                        if let Ok(root) = std::env::var("AURA_WORKSPACE_ROOT") {
                            runtime_path =
                                std::path::PathBuf::from(root).join("target/debug/libaura_rt.a");
                        }
                    }

                    linker
                        .link(&[obj_path.as_path()], &runtime_path, exe_path)
                        .expect("Failed to link");

                    if command == "run" {
                        let status = std::process::Command::new(run_path)
                            .status()
                            .expect("Failed to run executable");
                        if let Some(code) = status.code() {
                            std::process::exit(code);
                        }

                        eprintln!("aura panic: uncaught exception");
                        let _ = std::io::Write::flush(&mut std::io::stderr());
                        std::process::exit(1);
                    } else {
                        println!(
                            "Build successful with backend {}: {}",
                            backend_name,
                            exe_path.display()
                        );
                    }
                }
                Err(err) => {
                    eprintln!("error: {err}");
                    std::process::exit(2);
                }
            }
        }
        _ => unreachable!(),
    }
}

fn print_help() {
    println!("{HELP}");
}

fn print_debug_views(
    path: &str,
    emit_ast: bool,
    print_types: bool,
    print_symbols: bool,
    print_imports: bool,
    emit_hir: bool,
    emit_mir: bool,
) -> Result<(), String> {
    if !(emit_ast || print_types || print_symbols || print_imports || emit_hir || emit_mir) {
        return Ok(());
    }

    let graph = build_module_graph(&[path]).map_err(|err| err.to_string())?;
    let entry = graph
        .modules
        .iter()
        .find(|module| module.path == Path::new(path))
        .or_else(|| graph.modules.first())
        .ok_or_else(|| "no modules were loaded".to_string())?;

    let typed = if print_types || emit_hir || emit_mir {
        let (diags, typed) = aura_typeck::typeck_program(&entry.source, &entry.ast);
        if !diags.is_empty() {
            return Err(diags[0].message.clone());
        }
        Some(typed)
    } else {
        None
    };

    if emit_ast {
        println!("--- AST ---");
        println!("{:#?}", entry.ast);
    }

    if print_symbols {
        println!("--- Symbols ---");
        let table = build_symbol_table(entry);
        let mut symbols: Vec<_> = table.bindings.iter().collect();
        symbols.sort_by(|(left_name, _), (right_name, _)| left_name.cmp(right_name));
        for (name, symbol) in symbols {
            let kind = match symbol.kind {
                aura_driver::modules::SymbolKind::Function => "function",
                aura_driver::modules::SymbolKind::Class => "class",
                aura_driver::modules::SymbolKind::Interface => "interface",
                aura_driver::modules::SymbolKind::Let => "let",
                aura_driver::modules::SymbolKind::Const => "const",
                aura_driver::modules::SymbolKind::Import => "import",
            };
            let start = symbol.span.start.raw() as usize;
            let end = symbol.span.end.raw() as usize;
            let snippet = entry.source.get(start..end).unwrap_or("???");
            println!("  {name}: {kind} = {snippet}");
        }
    }

    if print_imports {
        println!("--- Imports ---");
        let mut imports: Vec<_> = graph
            .edges
            .iter()
            .filter(|edge| edge.from == entry.path)
            .collect();
        imports.sort_by_key(|edge| edge.specifier_span.start);
        for edge in imports {
            let resolved = edge
                .resolved_to
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<unresolved>".to_string());
            println!("  {} -> {}", edge.specifier, resolved);
        }
    }

    if let Some(typed) = typed {
        if print_types {
            println!("--- Expression Types ---");
            let mut spans: Vec<_> = typed.expression_types.keys().collect();
            spans.sort_by_key(|span| span.start);
            for span in spans {
                let start = span.start.raw() as usize;
                let end = span.end.raw() as usize;
                let snippet = entry.source.get(start..end).unwrap_or("???");
                let ty = typed.expression_types.get(span).unwrap();
                println!("  {:?} -> {}", snippet, ty.name());
            }
        }

        if emit_hir {
            println!("--- Annotated AST (HIR) ---");
            aura_driver::dump_hir::dump_hir(&entry.source, &entry.ast, &typed);
        }

        if emit_mir {
            println!("--- Mid-level IR (MIR) ---");
            let mir = aura_mir::lower_program(&entry.source, &entry.ast, &typed);
            println!("{}", aura_mir::dump_mir(&mir));
        }
    }

    Ok(())
}
