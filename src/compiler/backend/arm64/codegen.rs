use crate::compiler::ast::{Expr, Program, Span, Statement, TemplatePart};
use crate::compiler::backend::arm64::asm::{Emitter, Register};
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

pub struct Codegen {
    emitter: Emitter,
    variables: HashMap<String, (usize, Type)>, // name -> (stack offset, type)
    global_variables: HashMap<String, (String, Type)>, // name -> (label, type)
    classes: HashMap<String, (Vec<String>, Vec<String>)>, // name -> (fields, methods)
    string_constants: HashMap<String, String>, // value -> label
    node_types: HashMap<Span, Type>,
    stack_offset: usize,
    label_count: usize,
    current_fn_end: Option<String>,
    current_class: Option<String>,
    is_global_scope: bool,
    loaded_files: std::collections::HashSet<String>,
    current_dir: Option<String>,
    pub stdlib_path: Option<String>,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            emitter: Emitter::new(),
            variables: HashMap::new(),
            global_variables: HashMap::new(),
            classes: HashMap::new(),
            string_constants: HashMap::new(),
            node_types: HashMap::new(),
            stack_offset: 0,
            label_count: 0,
            current_fn_end: None,
            current_class: None,
            is_global_scope: true,
            loaded_files: std::collections::HashSet::new(),
            current_dir: None,
            stdlib_path: None,
        }
    }

    pub fn set_current_dir(&mut self, dir: String) {
        self.current_dir = Some(dir);
    }

    pub fn set_node_types(&mut self, node_types: HashMap<Span, Type>) {
        self.node_types = node_types;
    }

    pub fn generate(mut self, program: Program) -> String {
        // Register built-in classes
        self.classes.insert(
            "Promise".to_string(),
            (vec![], vec!["all".to_string(), "then".to_string()]),
        );
        // Register built-in Promise class (still needed as it's not in stdlib yet)
        self.classes.insert(
            "Promise".to_string(),
            (vec![], vec!["all".to_string(), "then".to_string(), "any".to_string(), "race".to_string(), "allSettled".to_string()]),
        );

        let mut classes = Vec::new();
        let mut fns = Vec::new();
        let mut global_stmts = Vec::new();

        self.collect_all_definitions(program, &mut classes, &mut fns, &mut global_stmts);

        for c in classes {
            self.generate_statement(c);
        }

        for f in fns {
            self.generate_statement(f);
        }

        self.emitter.emit_header();
        self.is_global_scope = true;
        for stmt in global_stmts {
            self.generate_statement(stmt);
        }
        self.emitter.emit_footer();

        // Emit global variables in .data section
        if !self.global_variables.is_empty() {
            self.emitter.output.push_str("\n.data\n");
            self.emitter.output.push_str(".align 8\n");
            for (_name, (label, _ty)) in &self.global_variables {
                self.emitter.output.push_str(&format!("{}:\n", label));
                self.emitter.output.push_str("    .quad 0\n");
            }
        }

        // Define aura_string_table for linker
        self.emitter.output.push_str("\n.data\n");
        self.emitter.output.push_str(".global _aura_string_table\n");
        self.emitter.output.push_str("_aura_string_table:\n");
        self.emitter.output.push_str("    .quad 0\n"); // Empty table

        // Emit string constants
        for (value, label) in &self.string_constants {
            self.emitter.output.push_str(&format!("{}:\n", label));
            self.emitter
                .output
                .push_str(&format!("    .asciz \"{}\"\n", value.replace("\"", "\\\"")));
        }

        self.emitter.finalize()
    }

    pub fn new_label(&mut self, prefix: &str) -> String {
        let l = self.label_count;
        self.label_count += 1;
        format!("_{}_{}", prefix, l)
    }

    fn collect_all_definitions(
        &mut self,
        program: Program,
        classes: &mut Vec<Statement>,
        fns: &mut Vec<Statement>,
        global_stmts: &mut Vec<Statement>,
    ) {
        for stmt in program.statements {
            let actual_stmt = match stmt {
                Statement::Export { decl, .. } => *decl,
                _ => stmt,
            };

            match actual_stmt {
                Statement::ClassDeclaration { .. } => classes.push(actual_stmt),
                Statement::FunctionDeclaration { .. } => fns.push(actual_stmt),
                Statement::Import { path, .. } => {
                    let absolute_path = if path.starts_with("std/") {
                        if let Some(ref std_path) = self.stdlib_path {
                            let sub_path = &path[4..]; // remove "std/"
                            let aura_path = format!("{}.aura", sub_path);
                            std::path::Path::new(std_path).join(aura_path).canonicalize()
                        } else {
                            continue;
                        }
                    } else if path.starts_with(".") {
                        if let Some(ref dir) = self.current_dir {
                            std::path::Path::new(dir).join(path).canonicalize()
                        } else {
                            std::path::Path::new(&path).canonicalize()
                        }
                    } else {
                        std::path::Path::new(&path).canonicalize()
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
                            let mut parser = crate::compiler::frontend::parser::Parser::new(tokens);
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
                Statement::VarDeclaration {
                    ref name,
                    ref value,
                    ..
                } => {
                    let var_ty = self
                        .node_types
                        .get(&value.span())
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    let label = format!("_g_{}", name);
                    self.global_variables.insert(name.clone(), (label, var_ty));
                    global_stmts.push(actual_stmt);
                }
                _ => global_stmts.push(actual_stmt),
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

    fn generate_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::VarDeclaration {
                name, ty: _, value, ..
            } => {
                let mut var_ty = self
                    .node_types
                    .get(&value.span())
                    .cloned()
                    .unwrap_or(Type::Unknown);
                if matches!(var_ty, Type::Unknown | Type::Int64) {
                    match value {
                        Expr::StringLiteral(_, _) => var_ty = Type::String,
                        Expr::Variable(ref n, _) if n == "true" || n == "false" => {
                            var_ty = Type::Boolean
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
                        .push_str(&format!("    adrp x1, {}@PAGE\n", label));
                    self.emitter
                        .output
                        .push_str(&format!("    str x0, [x1, {}@PAGEOFF]\n", label));
                } else {
                    self.stack_offset += 16;
                    self.variables
                        .insert(name.clone(), (self.stack_offset, var_ty));
                    self.emitter
                        .output
                        .push_str(&format!("    str x0, [x29, -{}]\n", self.stack_offset));
                }
            }
            Statement::FunctionDeclaration {
                name,
                name_span: _,
                params,
                return_ty: _,
                body,
                is_async: _,
                span: _,
                doc: _,
            } => {
                let saved_vars = self.variables.clone();
                let saved_offset = self.stack_offset;
                let saved_global_scope = self.is_global_scope;
                self.variables.clear();
                self.stack_offset = 0;
                self.is_global_scope = false;

                let is_method = self.current_class.is_some() && !name.contains("main");

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

                let mut current_arg_reg = 0;
                if is_method {
                    self.stack_offset += 16;
                    let class_ty = Type::Class(self.current_class.clone().unwrap());
                    self.variables
                        .insert("this".to_string(), (self.stack_offset, class_ty));
                    self.emitter
                        .output
                        .push_str(&format!("    str x0, [x29, -{}]\n", self.stack_offset));
                    current_arg_reg += 1;
                }

                // Map params to stack
                for (pname, ty_expr) in params {
                    let pty = self
                        .node_types
                        .get(&ty_expr.span())
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    self.stack_offset += 16;
                    self.variables
                        .insert(pname.clone(), (self.stack_offset, pty));
                    if current_arg_reg < 8 {
                        self.emitter.output.push_str(&format!(
                            "    str x{}, [x29, -{}]\n",
                            current_arg_reg, self.stack_offset
                        ));
                        current_arg_reg += 1;
                    }
                }

                self.generate_statement(*body);

                self.emitter.output.push_str(&format!("{}:\n", end_label));
                self.emitter.output.push_str("    add sp, sp, #256\n");
                self.emitter.output.push_str("    ldp x29, x30, [sp], 16\n");
                self.emitter.output.push_str("    ret\n");

                self.variables = saved_vars;
                self.stack_offset = saved_offset;
                self.is_global_scope = saved_global_scope;
                self.current_fn_end = old_fn_end;
            }
            Statement::Return(expr, _) => {
                self.generate_expr(expr);
                if let Some(ref end) = self.current_fn_end {
                    self.emitter.output.push_str(&format!("    b {}\n", end));
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
                            _ => {}
                        }
                    } else if let Some((_, ty)) = self.global_variables.get(name) {
                        match ty {
                            Type::String => is_str = true,
                            Type::Boolean => is_bool = true,
                            Type::Array(_) => is_array = true,
                            Type::Generic(ref name, _) if name == "Promise" => is_promise = true,
                            Type::Null => is_null = true,
                            _ => {}
                        }
                    }
                }
                if let Some(ty) = self.node_types.get(&expr.span()) {
                    match ty {
                        Type::String => is_str = true,
                        Type::Boolean => is_bool = true,
                        Type::Array(_) => is_array = true,
                        Type::Generic(ref name, _) if name == "Promise" => is_promise = true,
                        Type::Null => is_null = true,
                        _ => {}
                    }
                }

                self.generate_expr(expr);
                if is_str {
                    self.emitter.call("_print_str");
                } else if is_bool {
                    self.emitter.call("_print_bool");
                } else if is_array {
                    self.emitter.call("_print_array");
                } else if is_promise {
                    self.emitter.call("_print_promise");
                } else if is_null {
                    // No print_null yet, maybe use print_str with "null"?
                    // For now skip or handle as needed
                } else {
                    self.emitter.call("_print_num");
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
                name_span: _,
                fields,
                methods,
                constructor,
                span,
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
                    // Default constructor
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
                        name: if method.is_static {
                            format!("{}_{}", name, method.name)
                        } else {
                            format!("{}_{}", name, method.name) // Currently same mangling
                        },
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
                // For now, just generate the try block to avoid panics
                self.generate_statement(*try_block);
            }
            Statement::Import { .. } => {
                // Handled in collect_all_definitions
            }
            Statement::Export { decl, .. } => {
                self.generate_statement(*decl);
            }
        }
    }

    fn generate_expr(&mut self, expr: Expr) {
        match expr {
            Expr::Number(val, _) => {
                self.emitter.mov_imm(Register::X0, val);
            }
            Expr::Null(_) => {
                self.emitter.mov_imm(Register::X0, 0);
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
                    .push_str(&format!("    adrp x0, {}@PAGE\n", label));
                self.emitter
                    .output
                    .push_str(&format!("    add x0, x0, {}@PAGEOFF\n", label));
            }
            Expr::Variable(name, _) => {
                if let Some((offset, _)) = self.variables.get(&name) {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x0, [x29, -{}]\n", offset));
                } else if let Some((label, _)) = self.global_variables.get(&name) {
                    self.emitter
                        .output
                        .push_str(&format!("    adrp x1, {}@PAGE\n", label));
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x0, [x1, {}@PAGEOFF]\n", label));
                } else if self.classes.contains_key(&name) {
                    self.emitter.mov_imm(Register::X0, 0); // Class reference is null for now
                } else {
                    panic!("Undefined variable {}", name);
                }
            }
            Expr::BinaryOp(left, op, right, _) => {
                let left_ty = self.node_types.get(&left.span()).cloned();
                let right_ty = self.node_types.get(&right.span()).cloned();

                self.generate_expr(*right);
                self.emitter.push(Register::X0);
                self.generate_expr(*left);
                self.emitter.pop(Register::X1);

                // String concatenation
                if let (Some(lty), Some(rty)) = (left_ty, right_ty) {
                    if (matches!(lty, Type::String) || matches!(rty, Type::String)) && op == "+" {
                        self.emitter.call("_aura_str_concat");
                        return;
                    }
                }

                match op.as_str() {
                    "+" => self.emitter.add(Register::X0, Register::X0, Register::X1),
                    "-" => self.emitter.sub(Register::X0, Register::X0, Register::X1),
                    "*" => self.emitter.mul(Register::X0, Register::X0, Register::X1),
                    "/" => self.emitter.sdiv(Register::X0, Register::X0, Register::X1),
                    "%" => {
                        self.emitter.sdiv(Register::X2, Register::X0, Register::X1); // X2 = a / b
                        self.emitter.mul(Register::X2, Register::X2, Register::X1); // X2 = (a / b) * b
                        self.emitter.sub(Register::X0, Register::X0, Register::X2);
                        // X0 = a - X2
                    }
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
                if let Some((offset, _)) = self.variables.get(&name) {
                    self.emitter
                        .output
                        .push_str(&format!("    str x0, [x29, -{}]\n", offset));
                } else if let Some((label, _)) = self.global_variables.get(&name) {
                    self.emitter.push(Register::X0); // Save value
                    self.emitter
                        .output
                        .push_str(&format!("    adrp x1, {}@PAGE\n", label));
                    self.emitter.pop(Register::X0); // Restore value
                    self.emitter
                        .output
                        .push_str(&format!("    str x0, [x1, {}@PAGEOFF]\n", label));
                } else {
                    panic!("Undefined variable {}", name);
                }
            }
            Expr::This(_) => {
                let (offset, _) = self
                    .variables
                    .get("this")
                    .expect("'this' used outside of method");
                self.emitter
                    .output
                    .push_str(&format!("    ldr x0, [x29, -{}]\n", offset));
            }
            Expr::New(class_name, _, args, _) => {
                let (fields, _) = self
                    .classes
                    .get(&class_name)
                    .expect(&format!("Undefined class {}", class_name));
                let size = fields.len() * 8;
                self.emitter.mov_imm(Register::X0, size as i64);
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

                self.emitter.call(&format!("_{}_ctor", class_name));

                self.emitter.output.push_str("    ldr x0, [sp], 16\n");
            }
            Expr::MemberAccess(obj, member, _, _span) => {
                let obj_span = obj.span();
                self.generate_expr(*obj);
                let mut offset = 0;
                if let Some(Type::Class(ref class_name)) = self.node_types.get(&obj_span) {
                    if let Some((fields, _)) = self.classes.get(class_name) {
                        if let Some(idx) = fields.iter().position(|f| f == &member) {
                            offset = idx * 8;
                        }
                    }
                } else if let Some(ref class_name) = self.current_class {
                    // Fallback for 'this' if not in node_types for some reason
                    if let Some((fields, _)) = self.classes.get(class_name) {
                        if let Some(idx) = fields.iter().position(|f| f == &member) {
                            offset = idx * 8;
                        }
                    }
                }
                self.emitter
                    .output
                    .push_str(&format!("    ldr x0, [x0, #{}]\n", offset));
            }
            Expr::MemberAssign(obj, member, value, _, _span) => {
                let obj_span = obj.span();
                self.generate_expr(*value);
                self.emitter.push(Register::X0);
                self.generate_expr(*obj);
                let mut offset = 0;
                if let Some(Type::Class(ref class_name)) = self.node_types.get(&obj_span) {
                    if let Some((fields, _)) = self.classes.get(class_name) {
                        if let Some(idx) = fields.iter().position(|f| f == &member) {
                            offset = idx * 8;
                        }
                    }
                } else if let Some(ref class_name) = self.current_class {
                    if let Some((fields, _)) = self.classes.get(class_name) {
                        if let Some(idx) = fields.iter().position(|f| f == &member) {
                            offset = idx * 8;
                        }
                    }
                }
                self.emitter.pop(Register::X1);
                self.emitter
                    .output
                    .push_str(&format!("    str x1, [x0, #{}]\n", offset));
                self.emitter.mov_reg(Register::X0, Register::X1); // Assignment result
            }
            Expr::MethodCall(obj, member, _, args, _span) => {
                let obj_span = obj.span();
                let mut is_static = false;
                let mut class_name_found = None;
                let mut is_primitive = false;

                if let Expr::Variable(ref name, _) = *obj {
                    if self.classes.contains_key(name) {
                        is_static = true;
                        class_name_found = Some(name.clone());
                    }
                }

                if !is_static {
                    let ty = self
                        .node_types
                        .get(&obj_span)
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    if matches!(ty, Type::String | Type::Array(_)) {
                        is_primitive = true;
                    }
                }

                if is_static || is_primitive {
                    self.emitter.mov_imm(Register::X0, 0); // dummy this
                } else {
                    self.generate_expr((*obj).clone());
                }
                self.emitter.push(Register::X0);

                if is_primitive {
                    // Prepend object as first explicit argument
                    self.generate_expr((*obj).clone());
                    self.emitter.push(Register::X0);
                }

                for arg in &args {
                    self.generate_expr(arg.clone());
                    self.emitter.push(Register::X0);
                }

                let num_args = args.len() + 1 + (if is_primitive { 1 } else { 0 });
                for i in (0..num_args.min(8)).rev() {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x{}, [sp], 16\n", i));
                }

                let mut method_label = format!("_METHOD_{}", member);

                if let Some(cname) = class_name_found {
                    method_label = format!("_{}_{}", cname, member);
                } else if let Some(Type::Class(ref class_name)) = self.node_types.get(&obj_span) {
                    method_label = format!("_{}_{}", class_name, member);
                } else if is_primitive {
                    let ty = self
                        .node_types
                        .get(&obj_span)
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    if matches!(ty, Type::String) {
                        method_label = format!("_String_{}", member);
                    } else {
                        method_label = format!("_Array_{}", member);
                    }
                } else {
                    // Fallback search
                    for (class_name, (_, methods)) in &self.classes {
                        if methods.contains(&member) {
                            method_label = format!("_{}_{}", class_name, member);
                            break;
                        }
                    }
                }

                self.emitter.call(&method_label);
            }
            Expr::Call(name, _, args, _) => {
                for arg in &args {
                    self.generate_expr(arg.clone());
                    self.emitter.push(Register::X0);
                }
                for i in (0..args.len().min(8)).rev() {
                    self.emitter
                        .output
                        .push_str(&format!("    ldr x{}, [sp], 16\n", i));
                }
                let call_label = if name == "main" {
                    "_main_aura".to_string()
                } else {
                    format!("_{}", name)
                };
                self.emitter.call(&call_label);
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
            Expr::Template(parts, _) => {
                for (i, part) in parts.into_iter().enumerate() {
                    match part {
                        TemplatePart::Str(s) => {
                            let label = if let Some(l) = self.string_constants.get(&s) {
                                l.clone()
                            } else {
                                let l = self.new_label("str");
                                self.string_constants.insert(s.clone(), l.clone());
                                l
                            };
                            self.emitter
                                .output
                                .push_str(&format!("    adrp x0, {}@PAGE\n", label));
                            self.emitter
                                .output
                                .push_str(&format!("    add x0, x0, {}@PAGEOFF\n", label));
                        }
                        TemplatePart::Expr(expr) => {
                            let span = expr.span();
                            let mut ty =
                                self.node_types.get(&span).cloned().unwrap_or(Type::Unknown);
                            if matches!(ty, Type::Unknown | Type::Int64) {
                                if let Expr::Variable(ref name, _) = *expr {
                                    if name == "true" || name == "false" {
                                        ty = Type::Boolean;
                                    } else if let Some((_, var_ty)) = self.variables.get(name) {
                                        ty = var_ty.clone();
                                    }
                                }
                            }
                            self.generate_expr((*expr).clone());
                            match ty {
                                Type::Int32 | Type::Int64 | Type::Unknown => {
                                    self.emitter.call("_aura_num_to_str");
                                }
                                Type::Boolean => {
                                    self.emitter.call("_aura_bool_to_str");
                                }
                                Type::String => {}
                                _ => {
                                    self.emitter.call("_aura_num_to_str");
                                }
                            }
                        }
                    }
                    if i > 0 {
                        self.emitter.pop(Register::X1); // Previous result
                        self.emitter.mov_reg(Register::X2, Register::X0); // current
                        self.emitter.mov_reg(Register::X0, Register::X1); // previous
                        self.emitter.mov_reg(Register::X1, Register::X2); // current
                        self.emitter.call("_aura_str_concat");
                    }
                    self.emitter.push(Register::X0);
                }
                self.emitter.pop(Register::X0);
            }
            Expr::ArrayLiteral(elements, _) => {
                self.emitter.mov_imm(Register::X0, elements.len() as i64);
                self.emitter.call("_aura_array_new");
                self.emitter.push(Register::X0);
                for el in elements {
                    self.generate_expr(el);
                    self.emitter.mov_reg(Register::X1, Register::X0);
                    self.emitter.pop(Register::X0);
                    self.emitter.push(Register::X0);
                    self.emitter.call("_aura_array_push");
                }
                self.emitter.pop(Register::X0);
            }
            Expr::Await(expr, _) => {
                self.generate_expr(*expr);
            }
            Expr::Throw(expr, _) => {
                self.generate_expr(*expr);
                self.emitter.call("_aura_throw");
            }
            Expr::Index(obj, index, _) => {
                self.generate_expr(*obj);
                self.emitter.push(Register::X0);
                self.generate_expr(*index);
                self.emitter.mov_reg(Register::X1, Register::X0);
                self.emitter.pop(Register::X0);
                self.emitter.call("__arr_get");
            }
            Expr::Error(_) => panic!("Compiler bug: reaching error node in codegen"),
        }
    }
}
