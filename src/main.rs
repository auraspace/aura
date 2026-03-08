use aura_rust::compiler::backend::arm64::codegen::Codegen;
use aura_rust::compiler::backend::arm64::driver::Driver;
use aura_rust::compiler::backend::arm64::ir_codegen::IrCodegen;
use aura_rust::compiler::frontend::lexer::Lexer;
use aura_rust::compiler::frontend::parser::Parser;
use aura_rust::compiler::interp::Interpreter;
use aura_rust::compiler::ir::lower::Lowerer;
use aura_rust::compiler::ir::opt::Optimizer;
use aura_rust::compiler::sema::checker::SemanticAnalyzer;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let use_ir = args.contains(&"--ir".to_string());
    let use_interp = args.contains(&"--interp".to_string());
    let emit_ir = args.contains(&"--emit-ir".to_string());
    let is_lsp = args.contains(&"--lsp".to_string());

    if is_lsp {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(aura_rust::lsp::server::run_server());
        return;
    }

    let mut input_path = None;
    for arg in args.iter().skip(1) {
        if !arg.starts_with("--") {
            input_path = Some(arg);
            break;
        }
    }

    let (source, input_name) = if let Some(path) = input_path {
        (
            std::fs::read_to_string(path).expect("Unable to read file"),
            path.clone(),
        )
    } else {
        ("function fib(n: i32): i32 { if (n <= 1) { return n; } return fib(n - 1) + fib(n - 2); } print(fib(10));".to_string(), "fib_example".to_string())
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

    // Semantic Analysis
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.load_stdlib();
    analyzer.analyze(program.clone());
    if analyzer.diagnostics.has_errors() {
        analyzer.diagnostics.report();
        std::process::exit(1);
    }

    if use_interp {
        println!("--- Starting Interpreter ---");
        let mut interpreter = Interpreter::new();
        interpreter.load_stdlib();
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
        let mut cg = IrCodegen::new();
        cg.generate(module)
    } else {
        let mut cg = Codegen::new();
        cg.load_stdlib();
        cg.generate(program)
    };

    let asm_file = "output.s";
    let binary_file = "aura_program";

    std::fs::write(asm_file, asm).expect("Unable to write assembly file");

    if let Err(e) = Driver::build(asm_file, binary_file) {
        eprintln!("Build failed: {}", e);
        return;
    }

    println!("--- Running Aura Program ---");
    let output = std::process::Command::new(format!("./{}", binary_file))
        .output()
        .expect("Failed to execute program");
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
