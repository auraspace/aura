pub mod env;
pub mod eval;

pub use env::{Environment, StatementResult, Value};

use crate::compiler::ast::{Expr, Field, Program, Statement};
use crate::compiler::sema::ty::Type;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub struct Interpreter {
    pub env: Box<Environment>,
    // Store class definitions separately (fields metadata and methods)
    pub classes: HashMap<
        String,
        (
            Vec<Field>,
            HashMap<String, crate::compiler::ast::ClassMethod>,
        ),
    >,
    // Static fields: class_name -> fields
    pub static_fields: HashMap<String, Rc<RefCell<HashMap<String, Value>>>>,
    pub pending_exception: Option<Value>,
    pub env_stack: Vec<Rc<RefCell<HashMap<String, Value>>>>,
    pub stdlib_path: Option<String>,
    pub loaded_files: HashSet<String>,
    /// Optional handler for `print` statements. If None, uses `println!`.
    pub print_handler: Option<Rc<dyn Fn(&str)>>,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut interp = Self {
            env: Box::new(Environment::new(None)),
            classes: HashMap::new(),
            static_fields: HashMap::new(),
            pending_exception: None,
            env_stack: vec![Rc::new(RefCell::new(HashMap::new()))],
            stdlib_path: None,
            loaded_files: HashSet::new(),
            print_handler: None,
        };
        // Register built-in Promise class (methods are handled as special cases in eval_expr)
        interp
            .classes
            .insert("Promise".to_string(), (Vec::new(), HashMap::new()));

        // Register core constants that are lexed as identifiers
        interp.env.insert("true".to_string(), Value::Boolean(true));
        interp
            .env
            .insert("false".to_string(), Value::Boolean(false));
        interp.env.insert("null".to_string(), Value::Null);

        interp
    }

    pub fn interpret(&mut self, program: Program) {
        let mut has_explicit_main_call = false;
        for stmt in &program.statements {
            if let Statement::Expression(Expr::Call(ref name, _, _, _, _), _) = stmt {
                if name == "main" {
                    has_explicit_main_call = true;
                }
            }
        }

        for stmt in program.statements {
            self.execute_statement(stmt);
        }

        // Call main if it exists and wasn't already called explicitly
        if !has_explicit_main_call {
            if let Some(main_val) = self.env.lookup("main") {
                if matches!(main_val, Value::Function { .. } | Value::NativeFunction(_)) {
                    self.call_value(&main_val, vec![]);
                }
            }
        }
    }

    pub fn call_value(&mut self, func: &Value, args: Vec<Value>) -> Value {
        match func {
            Value::Function {
                name: _,
                params,
                return_ty: _,
                body,
                captured_env,
                is_async: _,
            } => {
                // Push captured environment onto the stack
                let original_env_stack =
                    std::mem::replace(&mut self.env_stack, captured_env.clone());
                self.push_scope();
                for (i, (pname, pty)) in params.iter().enumerate() {
                    let mut val = args.get(i).cloned().unwrap_or(Value::Null);
                    if *pty == Type::Float64 || *pty == Type::Float32 {
                        if let Value::Int(i) = val {
                            val = Value::Float(i as f64);
                        } else if let Value::Int64(i) = val {
                            val = Value::Float(i as f64);
                        }
                    }
                    self.env.insert(pname.clone(), val);
                }
                let res = self.execute_statement(body.clone());
                self.pop_scope();
                // Restore original environment stack
                self.env_stack = original_env_stack;
                if let StatementResult::Return(v) = res {
                    v
                } else if let StatementResult::Throw(e) = res {
                    self.pending_exception = Some(e);
                    Value::Void
                } else {
                    Value::Void
                }
            }
            Value::NativeFunction(f) => f(args),
            _ => panic!("Not a callable value: {:?}", func),
        }
    }

    pub fn load_stdlib(&mut self, stdlib_path: &str) {
        self.stdlib_path = Some(stdlib_path.to_string());
        let core_path = std::path::Path::new(stdlib_path).join("core.aura");
        if core_path.exists() {
            if let Ok(source) = std::fs::read_to_string(&core_path) {
                let mut lexer = crate::compiler::frontend::lexer::Lexer::new(&source);
                let tokens = lexer.lex_all();
                let mut parser = crate::compiler::frontend::parser::Parser::new(
                    tokens,
                    core_path.to_string_lossy().to_string(),
                );
                let program = parser.parse_program();
                self.interpret(program);
            }
        }
    }

    fn load_import(&mut self, path: String) {
        let absolute_path = if path.starts_with("std/") {
            if let Some(ref std_path) = self.stdlib_path {
                let sub_path = &path[4..]; // remove "std/"
                let aura_path = if sub_path.ends_with(".aura") {
                    sub_path.to_string()
                } else {
                    format!("{}.aura", sub_path)
                };
                std::path::Path::new(std_path)
                    .join(aura_path)
                    .canonicalize()
            } else {
                return;
            }
        } else if path.starts_with(".") {
            // How to get current file's directory in Interpreter?
            // For now, assume relative to current working directory or add current_dir to Interpreter
            std::path::Path::new(&path).canonicalize()
        } else {
            std::path::Path::new(&path).canonicalize()
        };

        if let Ok(abs_p) = absolute_path {
            let path_str = abs_p.to_string_lossy().to_string();
            if self.loaded_files.contains(&path_str) {
                return;
            }
            self.loaded_files.insert(path_str.clone());

            if let Ok(source) = std::fs::read_to_string(&abs_p) {
                let mut lexer = crate::compiler::frontend::lexer::Lexer::new(&source);
                let tokens = lexer.lex_all();
                let mut parser = crate::compiler::frontend::parser::Parser::new(tokens, path_str);
                let program = parser.parse_program();
                self.interpret(program);
            }
        }
    }

    pub(crate) fn push_scope(&mut self) {
        let parent = std::mem::replace(&mut self.env, Box::new(Environment::new(None)));
        self.env = Box::new(Environment::new(Some(parent)));
        self.env_stack.push(Rc::new(RefCell::new(HashMap::new()))); // Add a new scope to the stack
    }

    pub(crate) fn pop_scope(&mut self) {
        let parent = self.env.parent.take();
        if let Some(parent) = parent {
            self.env = parent;
            self.env_stack.pop();
        }
    }
}
