use crate::compiler::ast::{Expr, Program, Statement};
use crate::compiler::backend::arm64::asm::{Emitter, Register};
use std::collections::HashMap;

pub struct Codegen {
    emitter: Emitter,
    variables: HashMap<String, usize>, // name -> stack offset
    classes: HashMap<String, (Vec<String>, Vec<String>)>, // name -> (fields, methods)
    stack_offset: usize,
    label_count: usize,
    current_fn_end: Option<String>,
    current_class: Option<String>,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            emitter: Emitter::new(),
            variables: HashMap::new(),
            classes: HashMap::new(),
            stack_offset: 0,
            label_count: 0,
            current_fn_end: None,
            current_class: None,
        }
    }

    pub fn generate(mut self, program: Program) -> String {
        let mut classes = Vec::new();
        let mut fns = Vec::new();
        let mut global_stmts = Vec::new();

        for stmt in program.statements {
            match stmt {
                Statement::ClassDeclaration { .. } => classes.push(stmt),
                Statement::FunctionDeclaration { .. } => fns.push(stmt),
                _ => global_stmts.push(stmt),
            }
        }

        for c in classes {
            self.generate_statement(c);
        }

        for f in fns {
            self.generate_statement(f);
        }

        self.emitter.emit_header();
        for stmt in global_stmts {
            self.generate_statement(stmt);
        }
        self.emitter.emit_footer();

        self.emitter.finalize()
    }

    pub fn new_label(&mut self, prefix: &str) -> String {
        let l = self.label_count;
        self.label_count += 1;
        format!("_{}_{}", prefix, l)
    }

    pub fn load_stdlib(&mut self) {
        let stdlib_path = "stdlib/std";
        if let Ok(entries) = std::fs::read_dir(stdlib_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("aura") {
                    if let Ok(source) = std::fs::read_to_string(&path) {
                        let mut lexer = crate::compiler::frontend::lexer::Lexer::new(&source);
                        let tokens = lexer.lex_all();
                        let mut parser = crate::compiler::frontend::parser::Parser::new(tokens);
                        let program = parser.parse_program();
                        for stmt in program.statements {
                            if let Statement::ClassDeclaration {
                                name,
                                fields,
                                methods,
                                ..
                            } = stmt
                            {
                                let fnames = fields.into_iter().map(|f| f.name).collect();
                                let mnames = methods.into_iter().map(|m| m.name).collect();
                                self.classes.insert(name, (fnames, mnames));
                            }
                        }
                    }
                }
            }
        }
    }

    fn generate_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::VarDeclaration {
                name,
                ty: _,
                value,
                span: _,
                doc: _,
            } => {
                self.generate_expr(value);
                if !self.variables.contains_key(&name) {
                    self.stack_offset += 16;
                    self.variables.insert(name.clone(), self.stack_offset);
                }
                let offset = self.variables.get(&name).unwrap();
                self.emitter
                    .output
                    .push_str(&format!("    str x0, [x29, -{}]\n", offset));
            }
            Statement::FunctionDeclaration {
                name,
                params,
                return_ty: _,
                body,
                span: _,
                doc: _,
            } => {
                let saved_vars = self.variables.clone();
                let saved_offset = self.stack_offset;
                self.variables.clear();
                self.stack_offset = 0;

                let fn_label = if name == "main" {
                    "_main_aura".to_string()
                } else {
                    format!("_{}", name)
                };
                let end_label = self.new_label("fn_end");
                let old_fn_end = self.current_fn_end.replace(end_label.clone());

                self.emitter
                    .output
                    .push_str(&format!(".global {}\n{}:\n", fn_label, fn_label));
                self.emitter
                    .output
                    .push_str("    stp x29, x30, [sp, -16]!\n");
                self.emitter.output.push_str("    mov x29, sp\n");
                self.emitter.output.push_str("    sub sp, sp, #256\n");

                // Map params to stack
                for (i, (pname, _)) in params.iter().enumerate() {
                    self.stack_offset += 16;
                    self.variables.insert(pname.clone(), self.stack_offset);
                    if i < 8 {
                        self.emitter
                            .output
                            .push_str(&format!("    str x{}, [x29, -{}]\n", i, self.stack_offset));
                    }
                }

                self.generate_statement(*body);

                self.emitter.output.push_str(&format!("{}:\n", end_label));
                self.emitter.output.push_str("    add sp, sp, #256\n");
                self.emitter.output.push_str("    ldp x29, x30, [sp], 16\n");
                self.emitter.output.push_str("    ret\n");

                self.variables = saved_vars;
                self.stack_offset = saved_offset;
                self.current_fn_end = old_fn_end;
            }
            Statement::Return(expr, _) => {
                self.generate_expr(expr);
                if let Some(ref end) = self.current_fn_end {
                    self.emitter.output.push_str(&format!("    b {}\n", end));
                }
            }
            Statement::Print(expr, _) => {
                self.generate_expr(expr);
                self.emitter.call("_print_num");
            }
            Statement::Expression(expr, _) => {
                self.generate_expr(expr);
            }
            Statement::Block(stmts, _) => {
                for s in stmts {
                    self.generate_statement(s);
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                span: _,
            } => {
                let else_label = self.new_label("else");
                let end_label = self.new_label("end");
                self.generate_expr(condition);
                self.emitter
                    .output
                    .push_str(&format!("    cbz x0, {}\n", else_label));
                self.generate_statement(*then_branch);
                self.emitter
                    .output
                    .push_str(&format!("    b {}\n", end_label));
                self.emitter.output.push_str(&format!("{}:\n", else_label));
                if let Some(eb) = else_branch {
                    self.generate_statement(*eb);
                }
                self.emitter.output.push_str(&format!("{}:\n", end_label));
            }
            Statement::While {
                condition,
                body,
                span: _,
            } => {
                let start_label = self.new_label("while_start");
                let end_label = self.new_label("while_end");
                self.emitter.output.push_str(&format!("{}:\n", start_label));
                self.generate_expr(condition);
                self.emitter
                    .output
                    .push_str(&format!("    cbz x0, {}\n", end_label));
                self.generate_statement(*body);
                self.emitter
                    .output
                    .push_str(&format!("    b {}\n", start_label));
                self.emitter.output.push_str(&format!("{}:\n", end_label));
            }
            Statement::ClassDeclaration {
                name,
                fields,
                methods,
                constructor,
                span: _,
                doc: _,
            } => {
                let field_names: Vec<String> = fields
                    .iter()
                    .filter(|f| !f.is_static)
                    .map(|f| f.name.clone())
                    .collect();
                let method_names: Vec<String> = methods
                    .iter()
                    .filter(|m| !m.is_static)
                    .map(|m| m.name.clone())
                    .collect();
                self.classes
                    .insert(name.clone(), (field_names, method_names));

                let old_class = self.current_class.replace(name.clone());

                if let Some(cons) = constructor {
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_constructor", name),
                        params: cons.params,
                        return_ty: cons.return_ty,
                        body: cons.body,
                        span: cons.span,
                        doc: None,
                    });
                }

                for method in methods {
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: if method.is_static {
                            format!("{}_{}", name, method.name)
                        } else {
                            format!("{}_{}", name, method.name) // Currently same mangling
                        },
                        params: method.params,
                        return_ty: method.return_ty,
                        body: method.body,
                        span: method.span,
                        doc: None,
                    });
                }
                self.current_class = old_class;
            }
            Statement::Error => panic!("Compiler bug: reaching error node in codegen"),
        }
    }

    fn generate_expr(&mut self, expr: Expr) {
        match expr {
            Expr::Number(val, _) => {
                self.emitter.mov_imm(Register::X0, val);
            }
            Expr::StringLiteral(_, _) => {
                self.emitter.mov_imm(Register::X0, 0); // TODO: String support
            }
            Expr::Variable(name, _) => {
                if let Some(offset) = self.variables.get(&name) {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x0, [x29, -{}]\n", offset));
                } else if self.classes.contains_key(&name) {
                    self.emitter.mov_imm(Register::X0, 0); // Class reference is null for now
                } else {
                    panic!("Undefined variable {}", name);
                }
            }
            Expr::BinaryOp(left, op, right, _) => {
                self.generate_expr(*left);
                self.emitter.push(Register::X0);
                self.generate_expr(*right);
                self.emitter.mov_reg(Register::X1, Register::X0);
                self.emitter.pop(Register::X0);
                match op.as_str() {
                    "+" => self.emitter.add(Register::X0, Register::X0, Register::X1),
                    "-" => self.emitter.sub(Register::X0, Register::X0, Register::X1),
                    "==" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, eq\n");
                    }
                    "!=" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, ne\n");
                    }
                    "<" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, lt\n");
                    }
                    "<=" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, le\n");
                    }
                    ">" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, gt\n");
                    }
                    ">=" => {
                        self.emitter
                            .output
                            .push_str("    cmp x0, x1\n    cset x0, ge\n");
                    }
                    _ => panic!("Unsupported operator {}", op),
                }
            }
            Expr::Assign(name, value, _) => {
                self.generate_expr(*value);
                let offset = self.variables.get(&name).expect("Undefined variable");
                self.emitter
                    .output
                    .push_str(&format!("    str x0, [x29, -{}]\n", offset));
            }
            Expr::This(_) => {
                let offset = self
                    .variables
                    .get("this")
                    .expect("'this' used outside of method");
                self.emitter
                    .output
                    .push_str(&format!("    ldr x0, [x29, -{}]\n", offset));
            }
            Expr::New(class_name, args, _) => {
                let (fields, _) = self
                    .classes
                    .get(&class_name)
                    .expect(&format!("Undefined class {}", class_name));
                let size = fields.len() * 8;
                self.emitter.mov_imm(Register::X0, size as i32);
                self.emitter.call("_aura_alloc");

                // Push result (instance) to save it while evaluating args
                self.emitter.push(Register::X0);

                // Now push instance as the first argument ('this')
                self.emitter.push(Register::X0);

                for arg in &args {
                    self.generate_expr(arg.clone());
                    self.emitter.push(Register::X0);
                }

                // Pop args into x0-x7
                let num_args = args.len() + 1; // +1 for 'this'
                for i in (0..num_args.min(8)).rev() {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x{}, [sp], 16\n", i));
                }

                self.emitter.call(&format!("_{}_constructor", class_name));

                self.emitter.output.push_str("    ldr x0, [sp], 16\n");
            }
            Expr::MemberAccess(obj, member, _) => {
                self.generate_expr(*obj);
                let mut offset = 0;
                if let Some(ref class_name) = self.current_class {
                    let (fields, _) = self.classes.get(class_name).unwrap();
                    if let Some(idx) = fields.iter().position(|f| f == &member) {
                        offset = idx * 8;
                    }
                }
                self.emitter
                    .output
                    .push_str(&format!("    ldr x0, [x0, #{}]\n", offset));
            }
            Expr::MemberAssign(obj, member, value, _) => {
                self.generate_expr(*value);
                self.emitter.push(Register::X0);
                self.generate_expr(*obj);
                let mut offset = 0;
                if let Some(ref class_name) = self.current_class {
                    let (fields, _) = self.classes.get(class_name).unwrap();
                    if let Some(idx) = fields.iter().position(|f| f == &member) {
                        offset = idx * 8;
                    }
                }
                self.emitter.pop(Register::X1);
                self.emitter
                    .output
                    .push_str(&format!("    str x1, [x0, #{}]\n", offset));
                self.emitter.mov_reg(Register::X0, Register::X1); // Assignment result
            }
            Expr::MethodCall(obj, member, args, _) => {
                self.generate_expr(*obj);
                self.emitter.push(Register::X0);

                for arg in &args {
                    self.generate_expr(arg.clone());
                    self.emitter.push(Register::X0);
                }

                let num_args = args.len() + 1;
                for i in (0..num_args.min(8)).rev() {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x{}, [sp], 16\n", i));
                }

                let mut method_label = format!("_METHOD_{}", member);
                for (class_name, (_, methods)) in &self.classes {
                    if methods.contains(&member) {
                        method_label = format!("_{}_{}", class_name, member);
                        break;
                    }
                }

                self.emitter.call(&method_label);
            }
            Expr::Call(name, args, _) => {
                for arg in &args {
                    self.generate_expr(arg.clone());
                    self.emitter.push(Register::X0);
                }
                for i in (0..args.len().min(8)).rev() {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x{}, [sp], 16\n", i));
                }
                self.emitter.call(&format!("_{}", name));
            }
            Expr::UnaryOp(op, expr, _) => {
                self.generate_expr(*expr);
                if op == "-" {
                    self.emitter.sub(Register::X0, Register::XZR, Register::X0);
                }
            }
            Expr::TypeTest(_, _, _) => {
                // TODO: Implement type test codegen in Phase 4/5
                self.emitter.mov_imm(Register::X0, 0);
            }
            Expr::Error(_) => panic!("Compiler bug: reaching error node in codegen"),
        }
    }
}
