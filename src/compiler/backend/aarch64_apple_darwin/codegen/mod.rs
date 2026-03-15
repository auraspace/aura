use crate::compiler::ast::{Expr, Program, Span, Statement};
use crate::compiler::backend::aarch64_apple_darwin::asm::Emitter;
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

mod emit_expr;
mod emit_stmt;

pub struct Codegen {
    emitter: Emitter,
    variables: HashMap<String, (usize, Type)>, // name -> (stack offset, type)
    global_variables: HashMap<String, (String, Type)>, // name -> (label, type)
    classes: HashMap<String, (Vec<String>, Vec<String>)>, // name -> (fields, methods)
    interfaces: std::collections::HashSet<String>, // name
    abstract_classes: std::collections::HashSet<String>, // name
    enums: HashMap<String, HashMap<String, Expr>>, // name -> (member -> value)
    string_constants: HashMap<String, String>, // value -> label
    node_types: HashMap<String, HashMap<Span, Type>>,
    stack_offset: usize,
    label_count: usize,
    current_file: String,
    current_fn_end: Option<String>,
    current_class: Option<String>,
    is_global_scope: bool,
    method_to_idx: HashMap<String, u32>,
    next_method_idx: u32,
    vtables: HashMap<String, Vec<String>>, // class_name -> list of mangled method names
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
            interfaces: std::collections::HashSet::new(),
            abstract_classes: std::collections::HashSet::new(),
            enums: HashMap::new(),
            string_constants: HashMap::new(),
            node_types: HashMap::new(),
            stack_offset: 0,
            label_count: 0,
            current_file: String::new(),
            current_fn_end: None,
            current_class: None,
            is_global_scope: true,
            method_to_idx: HashMap::new(),
            next_method_idx: 0,
            vtables: HashMap::new(),
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
        if let Some(m) = self.node_types.get(&self.current_file) {
            m.get(span)
        } else {
            // Try fallback without path if exact match fails
            for map in self.node_types.values() {
                if let Some(ty) = map.get(span) {
                    return Some(ty);
                }
            }
            None
        }
    }

    fn resolve_obj_type(&self, obj: &Expr) -> Type {
        match obj {
            Expr::Variable(name, _) => {
                if let Some((_, ty)) = self.variables.get(name) {
                    if *ty != Type::Unknown {
                        return ty.clone();
                    }
                }
                if let Some((_, ty)) = self.global_variables.get(name) {
                    if *ty != Type::Unknown {
                        return ty.clone();
                    }
                }
            }
            Expr::This(_) => {
                if let Some(ref class_name) = self.current_class {
                    return Type::Class(class_name.clone());
                }
            }
            _ => {}
        }

        let span = obj.span();
        if let Some(ty) = self.get_node_type(&span) {
            if *ty != Type::Unknown {
                return ty.clone();
            }
        }

        match obj {
            Expr::MemberAccess(inner_obj, _member, _, _) => {
                let inner_ty = self.resolve_obj_type(inner_obj);
                if let Type::Class(ref _class_name) = inner_ty {
                    // Note: We don't have field types here in Godegen classes Map,
                    // but we should eventually add them or rely on node_types.
                    // For now, let's just return Unknown and rely on span-based lookup
                    // if it was a nested access.
                }
                Type::Unknown
            }
            _ => Type::Unknown,
        }
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

    fn store_local(&mut self, reg: &str, offset: usize) {
        if offset <= 255 {
            self.emitter
                .output
                .push_str(&format!("    str {}, [x29, -{}]\n", reg, offset));
        } else {
            self.emitter.output.push_str(&format!(
                "    mov x16, {}\n    sub x16, x29, x16\n    str {}, [x16]\n",
                offset, reg
            ));
        }
    }

    fn load_local(&mut self, reg: &str, offset: usize) {
        if offset <= 255 {
            self.emitter
                .output
                .push_str(&format!("    ldr {}, [x29, -{}]\n", reg, offset));
        } else {
            self.emitter.output.push_str(&format!(
                "    mov x16, {}\n    sub x16, x29, x16\n    ldr {}, [x16]\n",
                offset, reg
            ));
        }
    }

    pub fn generate(mut self, program: Program) -> String {
        self.current_file = program.file_path.clone();
        // Register built-in classes
        self.classes.insert(
            "Promise".to_string(),
            (vec![], vec!["all".to_string(), "then".to_string()]),
        );
        // Register built-in Promise class (still needed as it's not in stdlib yet)
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

        // Check for explicit main() call before consuming global_stmts
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

        // Call main_aura if it exists and wasn't already called explicitly
        if has_main && !has_explicit_main_call {
            self.emitter.call("_main_aura");
        }

        self.emitter.emit_footer();

        // Emit vtables in .data section
        if !self.vtables.is_empty() {
            self.emitter.output.push_str("\n.data\n");
            self.emitter.output.push_str(".align 8\n");
            for (class, methods) in &self.vtables {
                self.emitter
                    .output
                    .push_str(&format!("vtable_{}:\n", class));
                for method in methods {
                    if method == "aura_null" {
                        self.emitter.output.push_str("    .quad 0\n");
                    } else {
                        self.emitter
                            .output
                            .push_str(&format!("    .quad _{}\n", method));
                    }
                }
            }
        }

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
            self.emitter.output.push_str(&format!(
                "    .asciz \"{}\"\n",
                value.replace("\\", "\\\\").replace("\"", "\\\"")
            ));
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
        classes: &mut Vec<(String, Statement)>,
        fns: &mut Vec<(String, Statement)>,
        global_stmts: &mut Vec<(String, Statement)>,
    ) {
        self.current_file = program.file_path.clone();
        for stmt in program.statements {
            let actual_stmt = match stmt {
                Statement::Export { decl, .. } => *decl,
                _ => stmt,
            };

            match actual_stmt {
                Statement::ClassDeclaration {
                    ref name,
                    is_abstract,
                    ..
                } => {
                    if is_abstract {
                        self.abstract_classes.insert(name.clone());
                    }
                    classes.push((program.file_path.clone(), actual_stmt))
                }
                Statement::Interface(decl) => {
                    self.interfaces.insert(decl.name.clone());
                    for m in &decl.methods {
                        if !self.method_to_idx.contains_key(&m.name) {
                            self.method_to_idx
                                .insert(m.name.clone(), self.next_method_idx);
                            self.next_method_idx += 1;
                        }
                    }
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
                        // Infer type from value expression when node_types aren't available
                        if let Expr::New(ref class_name, _, _, _) = value {
                            var_ty = Type::Class(class_name.clone());
                        } else if let Expr::MemberAccess(ref obj, _, _, _) = value {
                            if let Expr::Variable(ref enum_name, _) = **obj {
                                if self.enums.contains_key(enum_name.as_str()) {
                                    var_ty = Type::Enum(enum_name.clone());
                                }
                            }
                        }
                    }
                    let label = format!("_g_{}", name);
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
                        is_abstract,
                        ..
                    } = stmt
                    {
                        if is_abstract {
                            self.abstract_classes.insert(name.clone());
                        }
                        let fnames = fields.iter().map(|f| f.name.clone()).collect();
                        let mnames = methods
                            .iter()
                            .filter(|m| !m.is_abstract)
                            .map(|m| m.name.clone())
                            .collect();
                        self.classes.insert(name.clone(), (fnames, mnames));
                    }
                }
            }
        }
    }
}
