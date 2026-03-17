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
    is_static_context: bool,
    method_to_idx: HashMap<String, u32>,
    next_method_idx: u32,
    vtables: HashMap<String, Vec<String>>, // class_name -> list of mangled method names
    loaded_files: std::collections::HashSet<String>,
    current_dir: Option<String>,
    pub stdlib_path: Option<String>,
    core_program: Option<Program>,
    has_aura_main: bool,
    generic_specializations: HashMap<String, Vec<Vec<Type>>>, // class_name -> list of concrete type argument vectors
    current_specialization: Option<(String, Vec<Type>)>,      // (class_name, type_args)
    generic_params: Vec<String>,                              // current class's T, U, etc.
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
            is_static_context: false,
            method_to_idx: HashMap::new(),
            next_method_idx: 0,
            vtables: HashMap::new(),
            loaded_files: std::collections::HashSet::new(),
            current_dir: None,
            stdlib_path: None,
            core_program: None,
            has_aura_main: false,
            generic_specializations: HashMap::new(),
            current_specialization: None,
            generic_params: Vec::new(),
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
        // 1. If it's a variable, check if it's a class name first (static access)
        if let Expr::Variable(name, _) = obj {
            if self.classes.contains_key(name) {
                return Type::ClassType(name.clone());
            }
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

        // 2. First try node_types from semantic analysis
        if let Some(ty) = self.get_node_type(&obj.span()) {
            if *ty != Type::Unknown {
                return ty.clone();
            }
        }

        // Fallback for variables and simple expressions
        match obj {
            Expr::Variable(_name, _) => {
                // Fallback handled above
            }
            Expr::This(_) => {
                if let Some(ref class_name) = self.current_class {
                    return Type::Class(class_name.clone());
                }
            }
            Expr::Super(_) => {
                if let Some(ref class_name) = self.current_class {
                    return Type::Class(class_name.clone());
                }
            }
            Expr::MemberAccess(inner_obj, _, _, _) => {
                let inner_ty = self.resolve_obj_type(inner_obj);
                // Recursive lookup if needed, but usually node_types should have it
                return inner_ty;
            }
            _ => {}
        }

        Type::Unknown
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

    pub fn mangle_name(&self, name: &str, args: &[Type]) -> String {
        if args.is_empty() {
            return name.to_string();
        }
        let mut mangled = name.to_string();
        for arg in args {
            let arg_str = format!("{}", arg)
                .replace("<", "_")
                .replace(">", "_")
                .replace(", ", "_")
                .replace("[]", "_array");
            mangled.push('_');
            mangled.push_str(&arg_str);
        }
        mangled
    }

    fn collect_specializations(&mut self) {
        let mut specs: HashMap<String, Vec<Vec<Type>>> = HashMap::new();
        for map in self.node_types.values() {
            for ty in map.values() {
                self.extract_generics(ty, &mut specs);
            }
        }
        self.generic_specializations = specs;
    }

    fn extract_generics(&self, ty: &Type, specs: &mut HashMap<String, Vec<Vec<Type>>>) {
        match ty {
            Type::Generic(name, args) => {
                let entry = specs.entry(name.clone()).or_default();
                if !entry.contains(args) {
                    entry.push(args.clone());
                }
                for arg in args {
                    self.extract_generics(arg, specs);
                }
            }
            Type::Array(inner) => self.extract_generics(inner, specs),
            _ => {}
        }
    }

    pub fn get_specialized_type(&self, ty: &Type) -> Type {
        if let Type::GenericParam(ref name) = ty {
            if let Some((_, args)) = &self.current_specialization {
                if let Some(pos) = self.generic_params.iter().position(|p| p == name) {
                    if let Some(concrete_ty) = args.get(pos) {
                        return concrete_ty.clone();
                    }
                }
            }
        }
        ty.clone()
    }

    pub fn emit_string_conversion(&mut self, ty: &Option<Type>, expr_ast: &Expr) {
        let mut actual_ty = ty.clone().unwrap_or(Type::Unknown);
        actual_ty = self.get_specialized_type(&actual_ty);

        if matches!(
            actual_ty,
            Type::Unknown | Type::Int64 | Type::GenericParam(_)
        ) {
            if let Expr::Variable(ref name, _) = expr_ast {
                if name == "true" || name == "false" {
                    actual_ty = Type::Boolean;
                } else if let Some((_, var_ty)) = self.variables.get(name) {
                    actual_ty = var_ty.clone();
                }
            }
        }
        match actual_ty {
            Type::Int32 | Type::Int64 | Type::Unknown | Type::GenericParam(_) => {
                self.emitter.call("_aura_num_to_str");
            }
            Type::Boolean => {
                self.emitter.call("_aura_bool_to_str");
            }
            Type::String => {}
            Type::Float32 | Type::Float64 => {
                self.emitter.output.push_str("    fmov d0, x0\n");
                self.emitter.call("_aura_float_to_str");
            }
            _ => {
                self.emitter.call("_aura_num_to_str");
            }
        }
    }

    pub fn generate(mut self, program: Program) -> String {
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
        // Register built-in Error class
        self.classes
            .insert("Error".to_string(), (vec!["message".to_string()], vec![]));

        let mut class_decls = Vec::new();
        let mut fn_decls = Vec::new();
        let mut top_level = Vec::new();

        if let Some(core_p) = self.core_program.take() {
            self.collect_all_definitions(core_p, &mut class_decls, &mut fn_decls, &mut top_level);
        }
        self.collect_all_definitions(program, &mut class_decls, &mut fn_decls, &mut top_level);

        // Monomorphization Pass
        self.collect_specializations();

        // 0. Emit built-in class stubs (Error, etc.)
        self.emitter
            .output
            .push_str("\n; ---- built-in Error class ----\n");
        self.emitter
            .output
            .push_str(".text\n.global _Error_ctor\n.align 4\n");
        self.emitter.output.push_str("_Error_ctor:\n");
        self.emitter
            .output
            .push_str("    stp x29, x30, [sp, -16]!\n");
        self.emitter.output.push_str("    mov x29, sp\n");
        // x0 = this, x1 = message
        self.emitter.output.push_str("    str x1, [x0, #8]\n"); // store message at offset 8 (field 0)
        self.emitter.output.push_str("    ldp x29, x30, [sp], 16\n");
        self.emitter.output.push_str("    ret\n");

        // 1. Emit all class definitions (methods, constructors)
        for (path, stmt) in class_decls {
            self.current_file = path;

            if let Statement::ClassDeclaration {
                ref name,
                ref type_params,
                ..
            } = stmt
            {
                if !type_params.is_empty() {
                    // Generate specialized versions
                    if let Some(specs) = self.generic_specializations.get(name).cloned() {
                        for args in specs {
                            self.current_specialization = Some((name.clone(), args.clone()));
                            self.generic_params =
                                type_params.iter().map(|tp| tp.name.clone()).collect();
                            self.generate_statement(stmt.clone());
                            self.current_specialization = None;
                            self.generic_params.clear();
                        }
                    }
                    continue; // Original generic class is not emitted directly
                }
            }
            self.generate_statement(stmt);
        }

        // 2. Emit all function definitions
        for (path, stmt) in fn_decls {
            self.current_file = path;
            self.generate_statement(stmt);
        }

        // 3. Emit main entry point and top-level code
        self.emitter.emit_header();
        self.is_global_scope = true;

        if !self.has_aura_main {
            for (path, stmt) in top_level {
                self.current_file = path;
                self.generate_statement(stmt);
            }
        } else {
            // Only emit static initializers if any, but in aura world we don't have static initializers yet
            // except for globals which are handled in collect_all_definitions
            for (path, stmt) in top_level {
                if let Statement::Expression(Expr::MemberAssign(ref obj, _, _, _, _), _) = stmt {
                    if let Expr::Variable(ref class_name, _) = **obj {
                        if self.classes.contains_key(class_name) {
                            // This is a static initializer, we must keep it
                            self.current_file = path;
                            self.generate_statement(stmt);
                            continue;
                        }
                    }
                }
                // Skip other top level code if we have explicit main
            }
        }

        // Call main_aura if it was defined
        if self.has_aura_main {
            self.emitter.call("_main_aura");
        }

        self.emitter.emit_footer();

        // 4. Data sections
        // Always emit built-in vtables
        self.emitter.output.push_str("\n.data\n.align 8\n");
        self.emitter.output.push_str("vtable_Error:\n"); // empty vtable for built-in Error class

        if !self.vtables.is_empty() {
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

        if !self.global_variables.is_empty() {
            self.emitter.output.push_str("\n.data\n");
            self.emitter.output.push_str(".align 8\n");
            for (_name, (label, _ty)) in &self.global_variables {
                self.emitter.output.push_str(&format!("{}:\n", label));
                self.emitter.output.push_str("    .quad 0\n");
            }
        }

        // String table
        self.emitter.output.push_str("\n.data\n");
        self.emitter.output.push_str(".global _aura_string_table\n");
        self.emitter.output.push_str("_aura_string_table:\n");
        self.emitter.output.push_str("    .quad 0\n");

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
        let file_path = program.file_path.clone();
        for stmt in program.statements {
            let actual_stmt = match stmt {
                Statement::Export { decl, .. } => *decl,
                _ => stmt,
            };

            match actual_stmt {
                Statement::ClassDeclaration {
                    ref name,
                    is_abstract,
                    ref fields,
                    ref methods,
                    ..
                } => {
                    if is_abstract {
                        self.abstract_classes.insert(name.clone());
                    }

                    // Register static fields as global variables
                    for field in fields {
                        if field.is_static {
                            let label = format!("_static_{}_{}", name, field.name);
                            let ty = self
                                .get_node_type(&field.name_span)
                                .cloned()
                                .unwrap_or(Type::Unknown);
                            self.global_variables
                                .insert(format!("{}.{}", name, field.name), (label, ty.clone()));

                            // Extract initializer into global_stmts if it exists
                            if let Some(ref value) = field.value {
                                global_stmts.push((
                                    file_path.clone(),
                                    Statement::Expression(
                                        Expr::MemberAssign(
                                            Box::new(Expr::Variable(name.clone(), field.name_span)),
                                            field.name.clone(),
                                            Box::new(value.clone()),
                                            field.name_span,
                                            field.span,
                                        ),
                                        field.span,
                                    ),
                                ));
                            }
                        }
                    }

                    // Register class schema (for member offsets)
                    let field_names: Vec<String> = fields
                        .iter()
                        .filter(|f| !f.is_static)
                        .map(|f| f.name.clone())
                        .collect();
                    let method_names: Vec<String> = methods
                        .iter()
                        .filter(|m| !m.is_static && !m.is_abstract)
                        .map(|m| m.name.clone())
                        .collect();
                    self.classes
                        .insert(name.clone(), (field_names, method_names));

                    classes.push((file_path.clone(), actual_stmt));
                }
                Statement::Interface(ref decl) => {
                    self.interfaces.insert(decl.name.clone());
                    for m in &decl.methods {
                        if !self.method_to_idx.contains_key(&m.name) {
                            self.method_to_idx
                                .insert(m.name.clone(), self.next_method_idx);
                            self.next_method_idx += 1;
                        }
                    }
                    // Interfaces don't generate code
                }
                Statement::FunctionDeclaration { ref name, .. } => {
                    if name == "main" && self.current_class.is_none() {
                        self.has_aura_main = true;
                    }
                    fns.push((file_path.clone(), actual_stmt));
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
                        .node_types
                        .get(&file_path)
                        .and_then(|m| m.get(&value.span()))
                        .cloned()
                        .unwrap_or(Type::Unknown);

                    if matches!(var_ty, Type::Unknown) {
                        if let Expr::New(ref class_name, _, _, _, _) = value {
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
                    global_stmts.push((file_path.clone(), actual_stmt));
                }
                _ => global_stmts.push((file_path.clone(), actual_stmt)),
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
                let _program = parser.parse_program();
            }
        }
    }
}
