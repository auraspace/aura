use aura::compiler::frontend::formatter::Formatter;
use aura::compiler::frontend::lexer::Lexer;
use aura::compiler::frontend::parser::Parser;
use aura::compiler::interp::Interpreter;
use aura::compiler::intrinsic::{register_analyzer_intrinsics, register_interpreter_intrinsics};
use aura::compiler::ir::lower::Lowerer;
use aura::compiler::ir::opt::Optimizer;
use aura::compiler::sema::checker::SemanticAnalyzer;

// Import backends
use aura::compiler::backend::aarch64_apple_darwin;

fn print_help() {
    println!("Aura Compiler");
    println!("");
    println!("Usage: aura <command> [options] <input_file>");
    println!("");
    println!("Commands:");
    println!("  build      Compile the source file into a binary");
    println!("  run        Compile and execute the source file (default)");
    println!("  lsp        Start the Language Server Protocol (LSP) server");
    println!("  fmt        Format .aura files");
    println!("  help       Show this help message");
    println!("");
    println!("Options:");
    println!("  -v, --version  Show version information and exit");
    println!("  --ir           Use the Intermediate Representation (IR) backend");
    println!("  --interp   Use the interpreter for execution");
    println!("  --emit-ir  Print the generated IR and exit");
    println!(
        "  --target   Specify the target architecture (default: {})",
        get_default_target()
    );
    println!("             Supported targets: aarch64-apple-darwin");
    std::process::exit(0);
}

fn get_default_target() -> String {
    "aarch64-apple-darwin".to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() <= 1 || args.contains(&"help".to_string()) || args.contains(&"--help".to_string())
    {
        print_help();
    }

    if args.contains(&"--version".to_string()) || args.contains(&"-v".to_string()) {
        println!("Aura version {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    let mut command = "run";
    let mut input_path = None;
    let mut use_ir = false;
    let mut use_interp = false;
    let mut emit_ir = false;
    let mut is_lsp = false;
    let mut is_fmt = false;
    #[allow(unused_assignments)]
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
            "fmt" if i == 1 => {
                command = "fmt";
                is_fmt = true;
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
                    if target != "aarch64-apple-darwin" {
                        eprintln!("Error: Target '{}' is not supported. Currently only 'aarch64-apple-darwin' is supported.", target);
                        std::process::exit(1);
                    }
                    skip_next = true;
                }
            }
            _ if !arg.starts_with("--") => {
                input_path = Some(arg);
            }
            _ => {}
        }
    }

    // stdlib resolution
    let stdlib_path = std::env::var("AURA_STDLIB").unwrap_or_else(|_| {
        let mut s_path = "stdlib/std".to_string();

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                // Try relative to exe (installed layout: bin/aura -> ../stdlib/std)
                let p1_s = exe_dir.join("../stdlib/std");
                // Try relative to exe (running from project root: ./stdlib/std)
                let p2_s = exe_dir.join("stdlib/std");
                // Try dev environment (target/debug/aura -> ../../stdlib/std)
                let p3_s = exe_dir.join("../../stdlib/std");

                if p1_s.exists() {
                    s_path = p1_s.to_string_lossy().to_string();
                } else if p2_s.exists() {
                    s_path = p2_s.to_string_lossy().to_string();
                } else if p3_s.exists() {
                    s_path = p3_s.to_string_lossy().to_string();
                }
            }
        }
        s_path
    });

    if is_lsp {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(aura::lsp::server::run_server(stdlib_path));
        return;
    }

    if is_fmt {
        let path = input_path
            .map(|p| p.to_string())
            .unwrap_or_else(|| ".".to_string());
        handle_fmt_command(&path);
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

        let mut cg = aarch64_apple_darwin::ir_codegen::IrCodegen::new();
        cg.generate(module)
    } else {
        let mut cg = aarch64_apple_darwin::codegen::Codegen::new();
        cg.set_node_types(analyzer.node_types);
        cg.load_stdlib(&stdlib_path);
        cg.set_current_dir(input_dir);
        cg.generate(program)
    };

    let input_stem = std::path::Path::new(&input_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let asm_file = format!("{}.s", input_stem);
    let binary_file = format!("{}_bin", input_stem);

    std::fs::write(&asm_file, asm).expect("Unable to write assembly file");

    let runtime_code = aura::runtime::embedded::RUNTIME_C;
    let build_result =
        aarch64_apple_darwin::driver::Driver::build(&asm_file, &binary_file, runtime_code);

    if let Err(e) = build_result {
        eprintln!("Build failed: {}", e);
        // Cleanup on failure
        let _ = std::fs::remove_file(&asm_file);
        std::process::exit(1);
    }

    // Assembly file is always temporary unless we specifically want it (but we don't here)
    // let _ = std::fs::remove_file(&asm_file);

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

fn handle_fmt_command(path_str: &str) {
    let path = std::path::Path::new(path_str);
    if path.is_file() {
        if path.extension().and_then(|s| s.to_str()) == Some("aura") {
            format_file(path);
        }
    } else if path.is_dir() {
        format_dir(path);
    } else if !path.exists() {
        eprintln!("Error: Path '{}' not found", path_str);
        std::process::exit(1);
    }
}

fn format_dir(dir: &std::path::Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Error reading directory '{}': {}", dir.display(), e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            format_dir(&path);
        } else if path.extension().and_then(|s| s.to_str()) == Some("aura") {
            format_file(&path);
        }
    }
}

fn format_file(path: &std::path::Path) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", path.display(), e);
            return;
        }
    };

    let mut lexer = Lexer::new(&content);
    let tokens = lexer.lex_all();
    if lexer.diagnostics.has_errors() {
        eprintln!(
            "Error: Syntax errors in '{}', skipping formatting",
            path.display()
        );
        lexer.diagnostics.report();
        return;
    }

    let mut parser = Parser::new(tokens, path.to_string_lossy().to_string());
    let program = parser.parse_program();
    if parser.diagnostics.has_errors() {
        eprintln!(
            "Error: Parsing errors in '{}', skipping formatting",
            path.display()
        );
        parser.diagnostics.report();
        return;
    }

    let formatter = Formatter::new().with_source(content.clone());
    let formatted = formatter.format_program(&program);

    if formatted != content {
        if let Err(e) = std::fs::write(path, formatted) {
            eprintln!("Error writing formatted file '{}': {}", path.display(), e);
        } else {
            println!("Formatted: {}", path.display());
        }
    }
}
