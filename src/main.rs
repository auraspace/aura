use aura::compiler::backend::arm64::codegen::Codegen;
use aura::compiler::backend::arm64::driver::Driver;
use aura::compiler::backend::arm64::ir_codegen::IrCodegen;
use aura::compiler::frontend::lexer::Lexer;
use aura::compiler::frontend::parser::Parser;
use aura::compiler::interp::Interpreter;
use aura::compiler::intrinsic::{
    register_analyzer_intrinsics, register_interpreter_intrinsics,
};
use aura::compiler::ir::lower::Lowerer;
use aura::compiler::ir::opt::Optimizer;
use aura::compiler::sema::checker::SemanticAnalyzer;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let use_ir = args.contains(&"--ir".to_string());
    let use_interp = args.contains(&"--interp".to_string());
    let emit_ir = args.contains(&"--emit-ir".to_string());
    let is_lsp = args.contains(&"--lsp".to_string());

    let mut target = "arm64".to_string();
    if let Some(idx) = args.iter().position(|a| a == "--target") {
        if idx + 1 < args.len() {
            target = args[idx + 1].clone();
        }
    }

    // stdlib and runtime resolution
    let (stdlib_path, runtime_path) = std::env::var("AURA_STDLIB").map(|s| (s, std::env::var("AURA_RUNTIME").unwrap_or_else(|_| "src/runtime/runtime.c".to_string()))).unwrap_or_else(|_| {
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

    let mut input_path = None;
    let mut skip_next = false;
    for (i, arg) in args.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "run" && i == 1 {
            continue;
        }
        if arg == "--target" {
            skip_next = true;
            continue;
        }
        if !arg.starts_with("--") {
            input_path = Some(arg);
            break;
        }
    }

    let (source, input_name) = if let Some(path) = input_path {
        let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Error: Unable to read file '{}': {}", path, e);
            std::process::exit(1);
        });
        (content, path.clone())
    } else {
        println!("Usage: aura [options] <input_file>");
        println!("Options:");
        println!("  --ir       Use IR backend");
        println!("  --interp   Use interpreter");
        println!("  --emit-ir  Print IR and exit");
        println!("  --lsp      Start LSP server");
        println!("  --target   Target architecture (arm64, x86_64)");
        std::process::exit(1);
    };

    if use_interp {
        println!("Interpreting: {}", input_name);
    } else {
        println!("Compiling: {} (IR: {})", input_name, use_ir);
    }

    let mut lexer = Lexer::new(&source);
    let tokens = lexer.lex_all();

    let mut parser = Parser::new(tokens);
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

    // stdlib and runtime resolution already done above

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

        if target == "x86_64" {
            let mut cg = aura::compiler::backend::x86_64::ir_codegen::IrCodegen::new();
            cg.generate(module)
        } else {
            let mut cg = IrCodegen::new();
            cg.generate(module)
        }
    } else {
        let mut cg = Codegen::new();
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

    if let Err(e) = Driver::build(&asm_file, &binary_file, &runtime_path) {
        eprintln!("Build failed: {}", e);
        // Cleanup on failure
        let _ = std::fs::remove_file(&asm_file);
        std::process::exit(1);
    }

    println!("--- Running Aura Program ---");
    let output = std::process::Command::new(format!("./{}", binary_file))
        .output()
        .expect("Failed to execute program");
    println!("{}", String::from_utf8_lossy(&output.stdout));

    // Cleanup temporary files
    let _ = std::fs::remove_file(&asm_file);
    let _ = std::fs::remove_file(&binary_file);
}
