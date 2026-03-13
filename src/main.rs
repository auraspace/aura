use aura::compiler::frontend::lexer::Lexer;
use aura::compiler::frontend::parser::Parser;
use aura::compiler::interp::Interpreter;
use aura::compiler::intrinsic::{register_analyzer_intrinsics, register_interpreter_intrinsics};
use aura::compiler::ir::lower::Lowerer;
use aura::compiler::ir::opt::Optimizer;
use aura::compiler::sema::checker::SemanticAnalyzer;

// Import backends
use aura::compiler::backend::aarch64_apple_darwin;
use aura::compiler::backend::x86_64_pc_windows_msvc;
use aura::compiler::backend::x86_64_unknown_linux_gnu;

fn print_help() {
    println!("Aura Compiler");
    println!("");
    println!("Usage: aura <command> [options] <input_file>");
    println!("");
    println!("Commands:");
    println!("  build      Compile the source file into a binary");
    println!("  run        Compile and execute the source file (default)");
    println!("  lsp        Start the Language Server Protocol (LSP) server");
    println!("  help       Show this help message");
    println!("");
    println!("Options:");
    println!("  --ir       Use the Intermediate Representation (IR) backend");
    println!("  --interp   Use the interpreter for execution");
    println!("  --emit-ir  Print the generated IR and exit");
    println!(
        "  --target   Specify the target architecture (default: {})",
        get_default_target()
    );
    println!("             Supported targets: aarch64-apple-darwin, x86_64-unknown-linux-gnu, x86_64-pc-windows-msvc");
    std::process::exit(0);
}

fn get_default_target() -> String {
    if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin".to_string()
    } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        "x86_64-unknown-linux-gnu".to_string()
    } else if cfg!(all(target_arch = "x86_64", target_os = "windows")) {
        "x86_64-pc-windows-msvc".to_string()
    } else {
        // Fallback for other systems
        "aarch64-apple-darwin".to_string()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() <= 1 || args.contains(&"help".to_string()) || args.contains(&"--help".to_string())
    {
        print_help();
    }

    let mut command = "run";
    let mut input_path = None;
    let mut use_ir = false;
    let mut use_interp = false;
    let mut emit_ir = false;
    let mut is_lsp = false;
    let mut target = get_default_target();

    let mut skip_next = false;
    for (i, arg) in args.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }

        match arg.as_str() {
            "run" if i == 1 => command = "run",
            "build" if i == 1 => command = "build",
            "lsp" if i == 1 => {
                command = "lsp";
                is_lsp = true;
            }
            "help" if i == 1 => print_help(),
            "--ir" => use_ir = true,
            "--interp" => use_interp = true,
            "--emit-ir" => emit_ir = true,
            "--lsp" => {
                is_lsp = true;
                command = "lsp";
            }
            "--target" => {
                if i + 1 < args.len() {
                    target = args[i + 1].clone();
                    skip_next = true;
                }
            }
            _ if !arg.starts_with("--") => {
                input_path = Some(arg);
            }
            _ => {}
        }
    }

    // stdlib and runtime resolution
    let (stdlib_path, runtime_path) = std::env::var("AURA_STDLIB")
        .map(|s| {
            (
                s,
                std::env::var("AURA_RUNTIME")
                    .unwrap_or_else(|_| "src/runtime/runtime.c".to_string()),
            )
        })
        .unwrap_or_else(|_| {
            let mut s_path = "stdlib/std".to_string();
            let mut r_path = "src/runtime/runtime.c".to_string();

            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(exe_dir) = exe_path.parent() {
                    // Try relative to exe
                    let p1_s = exe_dir.join("stdlib/std");
                    let p1_r = exe_dir.join("src/runtime/runtime.c");
                    if p1_s.exists() && p1_r.exists() {
                        s_path = p1_s.to_string_lossy().to_string();
                        r_path = p1_r.to_string_lossy().to_string();
                    } else {
                        // Try dev environment (target/debug)
                        let p2_s = exe_dir.join("../../stdlib/std");
                        let p2_r = exe_dir.join("../../src/runtime/runtime.c");
                        if p2_s.exists() && p2_r.exists() {
                            s_path = p2_s.to_string_lossy().to_string();
                            r_path = p2_r.to_string_lossy().to_string();
                        }
                    }
                }
            }
            (s_path, r_path)
        });

    if is_lsp {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(aura::lsp::server::run_server(stdlib_path));
        return;
    }

    let (source, input_name) = if let Some(path) = input_path {
        let abs_p = std::path::Path::new(path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(path));
        let content = std::fs::read_to_string(&abs_p).unwrap_or_else(|e| {
            eprintln!("Error: Unable to read file '{}': {}", path, e);
            std::process::exit(1);
        });
        (content, abs_p.to_string_lossy().to_string())
    } else {
        print_help();
        return;
    };

    if use_interp {
        println!("Interpreting: {}", input_name);
    } else {
        println!(
            "{}: {} (IR: {})",
            if command == "build" {
                "Building"
            } else {
                "Compiling"
            },
            input_name,
            use_ir
        );
    }

    let mut lexer = Lexer::new(&source);
    let tokens = lexer.lex_all();

    let mut parser = Parser::new(tokens, input_name.clone());
    let program = parser.parse_program();

    let mut has_errors = false;
    if lexer.diagnostics.has_errors() {
        lexer.diagnostics.report();
        has_errors = true;
    }
    if parser.diagnostics.has_errors() {
        parser.diagnostics.report();
        has_errors = true;
    }

    if has_errors {
        std::process::exit(1);
    }

    // Semantic Analysis
    let mut analyzer = SemanticAnalyzer::new();
    register_analyzer_intrinsics(&mut analyzer);
    analyzer.load_stdlib(&stdlib_path);
    let input_dir = std::path::Path::new(&input_name)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    analyzer.set_current_dir(input_dir.clone());
    analyzer.analyze(program.clone());
    if analyzer.diagnostics.has_errors() {
        analyzer.diagnostics.report();
        std::process::exit(1);
    }

    if use_interp {
        println!("--- Starting Interpreter ---");
        let mut interpreter = Interpreter::new();
        register_interpreter_intrinsics(&mut |name, val| {
            interpreter.env.insert(name, val);
        });
        interpreter.load_stdlib(&stdlib_path);
        interpreter.interpret(program);
        return;
    }

    let asm = if use_ir || emit_ir {
        let mut lowerer = Lowerer::new();
        let module = lowerer.lower_program(program);
        if emit_ir {
            println!("{}", module);
            return;
        }
        let mut opt = Optimizer::new();
        let module = opt.optimize(module);

        if target == "x86_64-unknown-linux-gnu" {
            let mut cg = x86_64_unknown_linux_gnu::ir_codegen::IrCodegen::new();
            cg.generate(module)
        } else if target == "x86_64-pc-windows-msvc" {
            let mut cg = x86_64_pc_windows_msvc::ir_codegen::IrCodegen::new();
            cg.generate(module)
        } else {
            let mut cg = aarch64_apple_darwin::ir_codegen::IrCodegen::new();
            cg.generate(module)
        }
    } else {
        if target == "x86_64-unknown-linux-gnu" {
            let mut cg = x86_64_unknown_linux_gnu::codegen::Codegen::new();
            cg.set_node_types(analyzer.node_types);
            cg.load_stdlib(&stdlib_path);
            cg.set_current_dir(input_dir);
            cg.generate(program)
        } else if target == "x86_64-pc-windows-msvc" {
            let mut cg = x86_64_pc_windows_msvc::codegen::Codegen::new();
            // Assuming Windows backend has similar methods
            cg.generate(program)
        } else {
            let mut cg = aarch64_apple_darwin::codegen::Codegen::new();
            cg.set_node_types(analyzer.node_types);
            cg.load_stdlib(&stdlib_path);
            cg.set_current_dir(input_dir);
            cg.generate(program)
        }
    };

    let input_stem = std::path::Path::new(&input_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let asm_file = format!("{}.s", input_stem);
    let binary_file = format!("{}_bin", input_stem);

    std::fs::write(&asm_file, asm).expect("Unable to write assembly file");

    let build_result = if target == "x86_64-unknown-linux-gnu" {
        x86_64_unknown_linux_gnu::driver::Driver::build(&asm_file, &binary_file, &runtime_path)
    } else if target == "x86_64-pc-windows-msvc" {
        x86_64_pc_windows_msvc::driver::Driver::build(&asm_file, &binary_file, &runtime_path)
    } else {
        aarch64_apple_darwin::driver::Driver::build(&asm_file, &binary_file, &runtime_path)
    };

    if let Err(e) = build_result {
        eprintln!("Build failed: {}", e);
        // Cleanup on failure
        let _ = std::fs::remove_file(&asm_file);
        std::process::exit(1);
    }

    // Assembly file is always temporary unless we specifically want it (but we don't here)
    let _ = std::fs::remove_file(&asm_file);

    if command == "run" {
        println!("--- Running Aura Program ---");
        let output = std::process::Command::new(format!("./{}", binary_file))
            .output()
            .expect("Failed to execute program");
        println!("{}", String::from_utf8_lossy(&output.stdout));

        // Cleanup temporary binary
        let _ = std::fs::remove_file(&binary_file);
    } else {
        println!("Build successful: {}", binary_file);
    }
}
