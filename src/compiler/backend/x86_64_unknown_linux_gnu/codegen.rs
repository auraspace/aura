use crate::compiler::ast::{Expr, Program, Span, Statement};
use crate::compiler::backend::x86_64_unknown_linux_gnu::asm::Emitter;
use crate::compiler::backend::x86_64_unknown_linux_gnu::reg::Register;
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

pub struct Codegen {
    emitter: Emitter,
    variables: HashMap<String, (usize, Type)>, // name -> (stack offset, type)
    global_variables: HashMap<String, (String, Type)>, // name -> (label, type)
    classes: HashMap<String, (Vec<String>, Vec<String>)>, // name -> (fields, methods)
    enums: HashMap<String, HashMap<String, Expr>>, // name -> (member -> value)
    string_constants: HashMap<String, String>, // value -> label
    node_types: HashMap<String, HashMap<Span, Type>>,
    stack_offset: usize,
    label_count: usize,
    current_file: String,
    current_fn_end: Option<String>,
    current_class: Option<String>,
    is_global_scope: bool,
    loaded_files: std::collections::HashSet<String>,
    current_dir: Option<String>,
    pub stdlib_path: Option<String>,
    core_program: Option<Program>,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            emitter: Emitter::new(),
            variables: HashMap::new(),
            global_variables: HashMap::new(),
            classes: HashMap::new(),
            enums: HashMap::new(),
            string_constants: HashMap::new(),
            node_types: HashMap::new(),
            stack_offset: 0,
            label_count: 0,
            current_file: String::new(),
            current_fn_end: None,
            current_class: None,
            is_global_scope: true,
            loaded_files: std::collections::HashSet::new(),
            current_dir: None,
            stdlib_path: None,
            core_program: None,
        }
    }

    pub fn set_current_dir(&mut self, dir: String) {
        self.current_dir = Some(dir);
    }

    pub fn set_node_types(&mut self, node_types: HashMap<String, HashMap<Span, Type>>) {
        self.node_types = node_types;
    }

    fn get_node_type(&self, span: &Span) -> Option<&Type> {
        self.node_types
            .get(&self.current_file)
            .and_then(|m| m.get(span))
    }

    fn is_string_enum(&self, enum_name: &str) -> bool {
        if let Some(members) = self.enums.get(enum_name) {
            members
                .values()
                .any(|v| matches!(v, Expr::StringLiteral(_, _)))
        } else {
            false
        }
    }

    fn store_local(&mut self, reg: Register, offset: usize) {
        self.emitter
            .output
            .push_str(&format!("    mov %{}, -{}(%rbp)\n", reg.name(), offset));
    }

    fn load_local(&mut self, reg: Register, offset: usize) {
        self.emitter
            .output
            .push_str(&format!("    mov -{}(%rbp), %{}\n", offset, reg.name()));
    }

    pub fn generate(mut self, program: Program) -> String {
        self.current_file = program.file_path.clone();

        // Register built-in classes (same as aarch64)
        self.classes.insert(
            "Promise".to_string(),
            (vec![], vec!["all".to_string(), "then".to_string()]),
        );
        self.classes.insert(
            "Promise".to_string(),
            (
                vec![],
                vec![
                    "all".to_string(),
                    "then".to_string(),
                    "any".to_string(),
                    "race".to_string(),
                    "allSettled".to_string(),
                ],
            ),
        );

        let mut classes: Vec<(String, Statement)> = Vec::new();
        let mut fns: Vec<(String, Statement)> = Vec::new();
        let mut global_stmts: Vec<(String, Statement)> = Vec::new();

        if let Some(core_prog) = self.core_program.take() {
            self.collect_all_definitions(core_prog, &mut classes, &mut fns, &mut global_stmts);
        }

        self.collect_all_definitions(program, &mut classes, &mut fns, &mut global_stmts);

        let has_main = fns.iter().any(|(_, stmt)| {
            if let Statement::FunctionDeclaration { name, .. } = stmt {
                name == "main"
            } else {
                false
            }
        });

        for (path, c) in classes {
            self.current_file = path;
            self.generate_statement(c);
        }

        for (path, f) in fns {
            self.current_file = path;
            self.generate_statement(f);
        }

        let has_explicit_main_call = global_stmts.iter().any(|(_, stmt)| {
            if let Statement::Expression(Expr::Call(ref name, _, _, _), _) = stmt {
                name == "main"
            } else {
                false
            }
        });

        self.emitter.emit_header();
        self.is_global_scope = true;
        for (path, stmt) in global_stmts {
            self.current_file = path;
            self.generate_statement(stmt);
        }

        if has_main && !has_explicit_main_call {
            self.emitter.call("main_aura");
        }

        self.emitter.emit_footer();

        // Emit global variables
        if !self.global_variables.is_empty() {
            self.emitter.output.push_str("\n.data\n");
            self.emitter.output.push_str(".align 8\n");
            for (_name, (label, _ty)) in &self.global_variables {
                self.emitter.output.push_str(&format!("{}:\n", label));
                self.emitter.output.push_str("    .quad 0\n");
            }
        }

        self.emitter.output.push_str("\n.data\n");
        self.emitter.output.push_str(".global aura_string_table\n");
        self.emitter.output.push_str("aura_string_table:\n");
        self.emitter.output.push_str("    .quad 0\n");

        for (value, label) in &self.string_constants {
            self.emitter.output.push_str(&format!("{}:\n", label));
            self.emitter
                .output
                .push_str(&format!("    .asciz \"{}\"\n", value.replace("\"", "\\\"")));
        }

        self.emitter.finalize()
    }

    // Reuse helper methods from aarch64 (modulo target differences)
    pub fn new_label(&mut self, prefix: &str) -> String {
        let l = self.label_count;
        self.label_count += 1;
        format!("{}_{}", prefix, l)
    }

    fn collect_all_definitions(
        &mut self,
        program: Program,
        classes: &mut Vec<(String, Statement)>,
        fns: &mut Vec<(String, Statement)>,
        global_stmts: &mut Vec<(String, Statement)>,
    ) {
        // This is mostly architecture independent logic, I'll keep it same as aarch64
        self.current_file = program.file_path.clone();
        for stmt in program.statements {
            let actual_stmt = match stmt {
                Statement::Export { decl, .. } => *decl,
                _ => stmt,
            };

            match actual_stmt {
                Statement::ClassDeclaration { .. } => {
                    classes.push((program.file_path.clone(), actual_stmt))
                }
                Statement::FunctionDeclaration { .. } => {
                    fns.push((program.file_path.clone(), actual_stmt))
                }
                Statement::Import { path, .. } => {
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
                            continue;
                        }
                    } else {
                        let actual_path = if path.ends_with(".aura") {
                            path.to_string()
                        } else {
                            format!("{}.aura", path)
                        };

                        if path.starts_with(".") {
                            if let Some(ref dir) = self.current_dir {
                                std::path::Path::new(dir).join(actual_path).canonicalize()
                            } else {
                                std::path::Path::new(&actual_path).canonicalize()
                            }
                        } else {
                            std::path::Path::new(&actual_path).canonicalize()
                        }
                    };

                    if let Ok(abs_p) = absolute_path {
                        let path_str = abs_p.to_string_lossy().to_string();
                        if self.loaded_files.contains(&path_str) {
                            continue;
                        }
                        self.loaded_files.insert(path_str.clone());

                        if let Ok(source) = std::fs::read_to_string(&abs_p) {
                            let mut lexer = crate::compiler::frontend::lexer::Lexer::new(&source);
                            let tokens = lexer.lex_all();
                            let mut parser =
                                crate::compiler::frontend::parser::Parser::new(tokens, path_str);
                            let program = parser.parse_program();

                            let saved_dir = self.current_dir.clone();
                            if let Some(parent) = abs_p.parent() {
                                self.current_dir = Some(parent.to_string_lossy().to_string());
                            }

                            self.collect_all_definitions(program, classes, fns, global_stmts);

                            self.current_dir = saved_dir;
                        }
                    }
                }
                Statement::Enum(ref decl) => {
                    let mut members = HashMap::new();
                    let mut next_int: i64 = 0;
                    for member in &decl.members {
                        if let Some(ref expr) = member.value {
                            if let Expr::Number(val, _) = expr {
                                next_int = *val + 1;
                            }
                            members.insert(member.name.clone(), expr.clone());
                        } else {
                            members.insert(
                                member.name.clone(),
                                Expr::Number(next_int, member.name_span),
                            );
                            next_int += 1;
                        }
                    }
                    self.enums.insert(decl.name.clone(), members);
                }
                Statement::VarDeclaration {
                    ref name,
                    ref value,
                    ..
                } => {
                    let mut var_ty = self
                        .get_node_type(&value.span())
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    if matches!(var_ty, Type::Unknown) {
                        if let Expr::MemberAccess(ref obj, _, _, _) = value {
                            if let Expr::Variable(ref enum_name, _) = **obj {
                                if self.enums.contains_key(enum_name.as_str()) {
                                    var_ty = Type::Enum(enum_name.clone());
                                }
                            }
                        }
                    }
                    let label = format!("g_{}", name);
                    self.global_variables.insert(name.clone(), (label, var_ty));
                    global_stmts.push((program.file_path.clone(), actual_stmt));
                }
                _ => global_stmts.push((program.file_path.clone(), actual_stmt)),
            }
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
                self.core_program = Some(program.clone());
                for stmt in program.statements {
                    if let Statement::ClassDeclaration {
                        ref name,
                        ref fields,
                        ref methods,
                        ..
                    } = stmt
                    {
                        let fnames = fields.iter().map(|f| f.name.clone()).collect();
                        let mnames = methods.iter().map(|m| m.name.clone()).collect();
                        self.classes.insert(name.clone(), (fnames, mnames));
                    }
                }
            }
        }
    }

    fn generate_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::Enum(_) => {}
            Statement::Comment(_, _) | Statement::RegularBlockComment(_, _) => {}
            Statement::VarDeclaration {
                name, ty: _, value, ..
            } => {
                let mut var_ty = self
                    .get_node_type(&value.span())
                    .cloned()
                    .unwrap_or(Type::Unknown);
                // Type inference logic same as aarch64
                if matches!(var_ty, Type::Unknown | Type::Int64) {
                    match value {
                        Expr::StringLiteral(_, _) => var_ty = Type::String,
                        Expr::Variable(ref n, _) if n == "true" || n == "false" => {
                            var_ty = Type::Boolean
                        }
                        Expr::ArrayLiteral(_, _) => var_ty = Type::Array(Box::new(Type::Unknown)),
                        Expr::MethodCall(_, ref member, _, _, _) => {
                            if matches!(
                                member.as_str(),
                                "trim"
                                    | "len"
                                    | "toUpper"
                                    | "toLower"
                                    | "charAt"
                                    | "substring"
                                    | "join"
                                    | "read"
                            ) {
                                var_ty = Type::String;
                            }
                        }
                        _ => {}
                    }
                }
                self.generate_expr(value);
                if self.is_global_scope {
                    let (label, _) = self
                        .global_variables
                        .get(&name)
                        .expect("Global variable should be predefined");
                    self.emitter
                        .output
                        .push_str(&format!("    mov %rax, {}(%rip)\n", label));
                } else {
                    self.stack_offset += 8;
                    self.variables
                        .insert(name.clone(), (self.stack_offset, var_ty));
                    self.store_local(Register::RAX, self.stack_offset);
                }
            }
            Statement::FunctionDeclaration {
                name, params, body, ..
            } => {
                let saved_vars = self.variables.clone();
                let saved_offset = self.stack_offset;
                let saved_global_scope = self.is_global_scope;
                self.variables.clear();
                self.stack_offset = 0;
                self.is_global_scope = false;

                let is_method = self.current_class.is_some() && !name.contains("main");

                let fn_label = if name == "main" {
                    "main_aura".to_string()
                } else {
                    name.clone()
                };
                let end_label = self.new_label("fn_end");
                let old_fn_end = self.current_fn_end.replace(end_label.clone());

                self.emitter
                    .output
                    .push_str(&format!(".global {}\n{}:\n", fn_label, fn_label));
                self.emitter.output.push_str("    push %rbp\n");
                self.emitter.output.push_str("    mov %rsp, %rbp\n");
                self.emitter.output.push_str("    sub $256, %rsp\n");

                let mut current_arg_reg = 0;
                let arg_regs = [
                    Register::RDI,
                    Register::RSI,
                    Register::RDX,
                    Register::RCX,
                    Register::R8,
                    Register::R9,
                ];

                if is_method {
                    self.stack_offset += 8;
                    let class_ty = Type::Class(self.current_class.clone().unwrap());
                    self.variables
                        .insert("this".to_string(), (self.stack_offset, class_ty));
                    self.store_local(Register::RDI, self.stack_offset);
                    current_arg_reg += 1;
                }

                for (pname, ty_expr) in params {
                    let pty = self
                        .get_node_type(&ty_expr.span())
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    self.stack_offset += 8;
                    self.variables
                        .insert(pname.clone(), (self.stack_offset, pty));
                    if current_arg_reg < arg_regs.len() {
                        self.store_local(arg_regs[current_arg_reg], self.stack_offset);
                        current_arg_reg += 1;
                    }
                }

                self.generate_statement(*body);

                self.emitter.output.push_str(&format!("{}:\n", end_label));
                self.emitter.output.push_str("    leave\n");
                self.emitter.output.push_str("    ret\n");

                self.variables = saved_vars;
                self.stack_offset = saved_offset;
                self.is_global_scope = saved_global_scope;
                self.current_fn_end = old_fn_end;
            }
            Statement::Return(expr, _) => {
                self.generate_expr(expr);
                if let Some(ref end) = self.current_fn_end {
                    self.emitter.output.push_str(&format!("    jmp {}\n", end));
                }
            }
            Statement::Print(expr, _) => {
                let mut is_str = matches!(expr, Expr::StringLiteral(_, _) | Expr::Template(_, _));
                let mut is_bool = false;
                let mut is_array = false;
                let mut is_promise = false;
                let mut is_null = matches!(expr, Expr::Null(_));

                if let Expr::Variable(ref name, _) = expr {
                    if let Some((_, ty)) = self.variables.get(name) {
                        match ty {
                            Type::String => is_str = true,
                            Type::Boolean => is_bool = true,
                            Type::Array(_) => is_array = true,
                            Type::Generic(ref name, _) if name == "Promise" => is_promise = true,
                            Type::Null => is_null = true,
                            Type::Enum(ref enum_name) => {
                                if self.is_string_enum(enum_name) {
                                    is_str = true;
                                }
                            }
                            _ => {}
                        }
                    } else if let Some((_, ty)) = self.global_variables.get(name) {
                        match ty {
                            Type::String => is_str = true,
                            Type::Boolean => is_bool = true,
                            Type::Array(_) => is_array = true,
                            Type::Generic(ref name, _) if name == "Promise" => is_promise = true,
                            Type::Null => is_null = true,
                            Type::Enum(ref enum_name) => {
                                if self.is_string_enum(enum_name) {
                                    is_str = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // ... same type deduction logic as aarch64 ...
                if let Some(ty) = self.get_node_type(&expr.span()) {
                    match ty {
                        Type::String => is_str = true,
                        Type::Boolean => is_bool = true,
                        Type::Array(_) => is_array = true,
                        Type::Generic(ref name, _) if name == "Promise" => is_promise = true,
                        Type::Null => is_null = true,
                        Type::Enum(ref enum_name) => {
                            if self.is_string_enum(enum_name) {
                                is_str = true;
                            }
                        }
                        _ => {}
                    }
                }

                self.generate_expr(expr);

                // x86_64 uses %rdi for the first argument to print functions
                self.emitter.mov_reg(Register::RDI, Register::RAX);

                if is_str {
                    self.emitter.call("print_str");
                } else if is_bool {
                    self.emitter.call("print_bool");
                } else if is_array {
                    self.emitter.call("print_array");
                } else if is_promise {
                    self.emitter.call("print_promise");
                } else if is_null {
                } else {
                    self.emitter.call("print_num");
                }
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
                ..
            } => {
                let else_label = self.new_label("else");
                let end_label = self.new_label("end");
                self.generate_expr(condition);
                self.emitter.output.push_str("    cmp $0, %rax\n");
                self.emitter
                    .output
                    .push_str(&format!("    je {}\n", else_label));
                self.generate_statement(*then_branch);
                self.emitter
                    .output
                    .push_str(&format!("    jmp {}\n", end_label));
                self.emitter.output.push_str(&format!("{}:\n", else_label));
                if let Some(eb) = else_branch {
                    self.generate_statement(*eb);
                }
                self.emitter.output.push_str(&format!("{}:\n", end_label));
            }
            Statement::While {
                condition, body, ..
            } => {
                let start_label = self.new_label("while_start");
                let end_label = self.new_label("while_end");
                self.emitter.output.push_str(&format!("{}:\n", start_label));
                self.generate_expr(condition);
                self.emitter.output.push_str("    cmp $0, %rax\n");
                self.emitter
                    .output
                    .push_str(&format!("    je {}\n", end_label));
                self.generate_statement(*body);
                self.emitter
                    .output
                    .push_str(&format!("    jmp {}\n", start_label));
                self.emitter.output.push_str(&format!("{}:\n", end_label));
            }
            Statement::ClassDeclaration {
                name,
                fields,
                methods,
                constructor,
                extends: _,
                span,
                ..
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
                let saved_global_scope = self.is_global_scope;
                self.is_global_scope = false;

                if let Some(cons) = constructor {
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_ctor", name),
                        name_span: cons.name_span,
                        params: cons.params,
                        return_ty: cons.return_ty,
                        body: cons.body,
                        is_async: cons.is_async,
                        span: cons.span,
                        doc: None,
                    });
                } else {
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_ctor", name),
                        name_span: span,
                        params: vec![],
                        return_ty: crate::compiler::ast::TypeExpr::Name("void".to_string(), span),
                        body: Box::new(Statement::Block(vec![], span)),
                        is_async: false,
                        span,
                        doc: None,
                    });
                }

                for method in methods {
                    self.generate_statement(Statement::FunctionDeclaration {
                        name: format!("{}_{}", name, method.name),
                        name_span: method.name_span,
                        params: method.params,
                        return_ty: method.return_ty,
                        body: method.body,
                        is_async: method.is_async,
                        span: method.span,
                        doc: None,
                    });
                }
                self.current_class = old_class;
                self.is_global_scope = saved_global_scope;
            }
            Statement::Error => panic!("Compiler bug: reaching error node in codegen"),
            Statement::TryCatch { try_block, .. } => {
                self.generate_statement(*try_block);
            }
            Statement::Import { .. } => {}
            Statement::Interface(_) => {}
            Statement::Export { decl, .. } => {
                self.generate_statement(*decl);
            }
        }
    }

    fn generate_expr(&mut self, expr: Expr) {
        match expr {
            Expr::Number(val, _) => {
                self.emitter.mov_imm(Register::RAX, val);
            }
            Expr::Null(_) => {
                self.emitter.mov_imm(Register::RAX, 0);
            }
            Expr::StringLiteral(val, _) => {
                let label = if let Some(l) = self.string_constants.get(&val) {
                    l.clone()
                } else {
                    let l = self.new_label("str");
                    self.string_constants.insert(val, l.clone());
                    l
                };
                self.emitter
                    .output
                    .push_str(&format!("    lea {}(%rip), %rax\n", label));
            }
            Expr::Variable(name, _) => {
                if let Some((offset, _)) = self.variables.get(&name) {
                    self.load_local(Register::RAX, *offset);
                } else if let Some((label, _)) = self.global_variables.get(&name) {
                    self.emitter
                        .output
                        .push_str(&format!("    mov {}(%rip), %rax\n", label));
                } else if self.classes.contains_key(&name) {
                    self.emitter.mov_imm(Register::RAX, 0);
                } else {
                    match name.as_str() {
                        "true" => self.emitter.mov_imm(Register::RAX, 1),
                        "false" => self.emitter.mov_imm(Register::RAX, 0),
                        "null" => self.emitter.mov_imm(Register::RAX, 0),
                        _ => panic!("Undefined variable {}", name),
                    }
                }
            }
            Expr::BinaryOp(left, op, right, _) => {
                self.generate_expr(*left);
                self.emitter.push(Register::RAX);
                self.generate_expr(*right);
                self.emitter.mov_reg(Register::RBX, Register::RAX);
                self.emitter.pop(Register::RAX);
                match op.as_str() {
                    "+" => self
                        .emitter
                        .add(Register::RAX, Register::RAX, Register::RBX),
                    "-" => self
                        .emitter
                        .sub(Register::RAX, Register::RAX, Register::RBX),
                    "*" => self
                        .emitter
                        .mul(Register::RAX, Register::RAX, Register::RBX),
                    "/" => self
                        .emitter
                        .sdiv(Register::RAX, Register::RAX, Register::RBX),
                    "==" => {
                        self.emitter.output.push_str("    cmp %rbx, %rax\n");
                        self.emitter.output.push_str("    sete %al\n");
                        self.emitter.output.push_str("    movzbq %al, %rax\n");
                    }
                    "!=" => {
                        self.emitter.output.push_str("    cmp %rbx, %rax\n");
                        self.emitter.output.push_str("    setne %al\n");
                        self.emitter.output.push_str("    movzbq %al, %rax\n");
                    }
                    "<" => {
                        self.emitter.output.push_str("    cmp %rbx, %rax\n");
                        self.emitter.output.push_str("    setl %al\n");
                        self.emitter.output.push_str("    movzbq %al, %rax\n");
                    }
                    "<=" => {
                        self.emitter.output.push_str("    cmp %rbx, %rax\n");
                        self.emitter.output.push_str("    setle %al\n");
                        self.emitter.output.push_str("    movzbq %al, %rax\n");
                    }
                    ">" => {
                        self.emitter.output.push_str("    cmp %rbx, %rax\n");
                        self.emitter.output.push_str("    setg %al\n");
                        self.emitter.output.push_str("    movzbq %al, %rax\n");
                    }
                    ">=" => {
                        self.emitter.output.push_str("    cmp %rbx, %rax\n");
                        self.emitter.output.push_str("    setge %al\n");
                        self.emitter.output.push_str("    movzbq %al, %rax\n");
                    }
                    _ => panic!("Unsupported binary operator {}", op),
                }
            }
            Expr::Call(name, _, args, _) => {
                let arg_regs = [
                    Register::RDI,
                    Register::RSI,
                    Register::RDX,
                    Register::RCX,
                    Register::R8,
                    Register::R9,
                ];
                for (i, arg) in args.into_iter().enumerate() {
                    self.generate_expr(arg);
                    if i < arg_regs.len() {
                        self.emitter.mov_reg(arg_regs[i], Register::RAX);
                    } else {
                        self.emitter.push(Register::RAX);
                    }
                }
                self.emitter.call(&name);
            }
            Expr::MemberAccess(_obj, _member, _, _) => {
                // Placeholder for member access
            }
            Expr::MethodCall(_obj, _member, _args, _, _) => {
                // Placeholder for method call
            }
            Expr::Super(_) | Expr::SuperCall(_, _) => {
                // Not supported in x86_64 yet
            }
            _ => {
                // Implement other expressions as needed
            }
        }
    }
}
