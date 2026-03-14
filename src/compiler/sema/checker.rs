use crate::compiler::ast::{Expr, ImportItem, Program, Span, Statement, TypeExpr};
use crate::compiler::frontend::error::{Diagnostic, DiagnosticList};
use crate::compiler::sema::scope::Scope;
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

pub struct ClassInfo {
    pub name: String,
    pub fields: HashMap<String, (Type, Span, Option<String>)>, // Type, Span, Doc
    pub static_fields: HashMap<String, (Type, Span, Option<String>)>,
    pub methods: HashMap<String, (Vec<Type>, Type, Option<String>, Span)>, // params, ret, doc, span
    pub static_methods: HashMap<String, (Vec<Type>, Type, Option<String>, Span)>,
    pub is_exported: bool,
    pub defined_in: String,
    pub span: Span,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SemanticErrorKind {
    UndefinedVariable(String),
    UndefinedClass(String),
    UndefinedEnum(String),
    UndefinedMethod(String, String),
    UndefinedField(String, String),
    TypeMismatch(String, String), // expected, found
    IncompatibleBinaryOperators(String, String, String), // left_ty, op, right_ty
    DuplicateDeclaration(String),
    WrongArgumentCount(String, usize, usize), // name, expected, found
    NotAClass(String),
    UndefinedFunction(String),
    CannotAssignToConstant(String),
    UndefinedImport(String, String), // symbol, module
    ExportRequired(String),          // symbol
}

#[derive(Debug, Clone)]
pub struct SemanticError {
    pub kind: SemanticErrorKind,
    pub span: Span,
}

pub struct SemanticAnalyzer {
    pub scope: Box<Scope>,
    pub classes: HashMap<String, ClassInfo>,
    pub current_class: Option<String>,
    pub diagnostics: DiagnosticList,
    pub node_types: HashMap<String, HashMap<Span, Type>>,
    pub node_definitions: HashMap<String, HashMap<Span, (String, Span)>>,
    pub node_docs: HashMap<String, HashMap<Span, String>>,
    pub record_node_info: bool,
    pub current_file: String,
    pub loaded_files: std::collections::HashSet<String>,
    pub current_dir: Option<String>,
    pub stdlib_path: Option<String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut analyzer = Self {
            scope: Box::new(Scope::new(None)),
            classes: HashMap::new(),
            current_class: None,
            diagnostics: DiagnosticList::new(),
            node_types: HashMap::new(),
            node_definitions: HashMap::new(),
            node_docs: HashMap::new(),
            record_node_info: false,
            current_file: String::new(),
            loaded_files: std::collections::HashSet::new(),
            current_dir: None,
            stdlib_path: None,
        };

        // Register built-in Promise class
        let mut static_methods = HashMap::new();
        // Promise.all<T>(values: Array<Promise<T>>): Promise<Array<T>>
        // For simplicity, we use Unknown for now as we don't have generics in methods yet
        static_methods.insert(
            "all".to_string(),
            (
                vec![Type::Array(Box::new(Type::Unknown))],
                Type::Generic(
                    "Promise".to_string(),
                    vec![Type::Array(Box::new(Type::Unknown))],
                ),
                Some("Waits for all promises to be resolved".to_string()),
                Span::new(0, 0),
            ),
        );
        static_methods.insert(
            "allSettled".to_string(),
            (
                vec![Type::Array(Box::new(Type::Unknown))],
                Type::Generic(
                    "Promise".to_string(),
                    vec![Type::Array(Box::new(Type::Unknown))],
                ),
                Some("Waits for all promises to be settled".to_string()),
                Span::new(0, 0),
            ),
        );
        static_methods.insert(
            "any".to_string(),
            (
                vec![Type::Array(Box::new(Type::Unknown))],
                Type::Generic("Promise".to_string(), vec![Type::Unknown]),
                Some("Waits for any promise to be resolved".to_string()),
                Span::new(0, 0),
            ),
        );
        static_methods.insert(
            "race".to_string(),
            (
                vec![Type::Array(Box::new(Type::Unknown))],
                Type::Generic("Promise".to_string(), vec![Type::Unknown]),
                Some("Waits for the first promise to be settled".to_string()),
                Span::new(0, 0),
            ),
        );

        analyzer.classes.insert(
            "Promise".to_string(),
            ClassInfo {
                name: "Promise".to_string(),
                fields: HashMap::new(),
                static_fields: HashMap::new(),
                methods: HashMap::new(),
                static_methods,
                is_exported: true,
                defined_in: "".to_string(),
                span: Span::new(0, 0),
                doc: Some("Built-in Promise class".to_string()),
            },
        );

        analyzer.scope.insert(
            "true".to_string(),
            Type::Boolean,
            false,
            true, // true is a constant
            true, // exported
            Span::new(0, 0),
            "".to_string(),
            None,
        );
        analyzer.scope.insert(
            "false".to_string(),
            Type::Boolean,
            false,
            true, // false is a constant
            true, // exported
            Span::new(0, 0),
            "".to_string(),
            None,
        );
        analyzer.scope.insert(
            "null".to_string(),
            Type::Null,
            false,
            true, // null is a constant
            true, // exported
            Span::new(0, 0),
            "".to_string(),
            None,
        );

        analyzer.scope.insert(
            "O_RDONLY".to_string(),
            Type::Int32,
            false,
            true,
            true,
            Span::new(0, 0),
            "".to_string(),
            None,
        );
        analyzer.scope.insert(
            "O_WRONLY".to_string(),
            Type::Int32,
            false,
            true,
            true,
            Span::new(0, 0),
            "".to_string(),
            None,
        );
        analyzer.scope.insert(
            "O_RDWR".to_string(),
            Type::Int32,
            false,
            true,
            true,
            Span::new(0, 0),
            "".to_string(),
            None,
        );
        analyzer.scope.insert(
            "O_CREAT".to_string(),
            Type::Int32,
            false,
            true,
            true,
            Span::new(0, 0),
            "".to_string(),
            None,
        );
        analyzer.scope.insert(
            "O_TRUNC".to_string(),
            Type::Int32,
            false,
            true,
            true,
            Span::new(0, 0),
            "".to_string(),
            None,
        );
        analyzer.scope.insert(
            "O_APPEND".to_string(),
            Type::Int32,
            false,
            true,
            true,
            Span::new(0, 0),
            "".to_string(),
            None,
        );

        analyzer
    }

    fn error(&mut self, kind: SemanticErrorKind, span: Span) {
        let msg = match &kind {
            SemanticErrorKind::UndefinedVariable(n) => format!("Undefined variable: {}", n),
            SemanticErrorKind::UndefinedClass(n) => format!("Undefined class: {}", n),
            SemanticErrorKind::UndefinedEnum(n) => format!("Undefined enum: {}", n),
            SemanticErrorKind::UndefinedMethod(c, m) => {
                format!("Method {} not found in class {}", m, c)
            }
            SemanticErrorKind::UndefinedField(c, f) => {
                format!("Field {} not found in type {}", f, c)
            }
            SemanticErrorKind::TypeMismatch(e, f) => {
                format!("Type mismatch: expected {}, found {}", e, f)
            }
            SemanticErrorKind::IncompatibleBinaryOperators(l, op, r) => {
                format!("Incompatible types for operator {}: {} and {}", op, l, r)
            }
            SemanticErrorKind::DuplicateDeclaration(n) => format!("Duplicate declaration: {}", n),
            SemanticErrorKind::WrongArgumentCount(n, e, f) => {
                format!(
                    "Wrong argument count for {}: expected {}, found {}",
                    n, e, f
                )
            }
            SemanticErrorKind::NotAClass(t) => format!("Type {} is not a class", t),
            SemanticErrorKind::UndefinedFunction(n) => format!("Undefined function: {}", n),
            SemanticErrorKind::CannotAssignToConstant(n) => {
                format!("Cannot assign to constant: {}", n)
            }
            SemanticErrorKind::UndefinedImport(s, m) => {
                format!("Symbol {} not found in module {}", s, m)
            }
            SemanticErrorKind::ExportRequired(s) => {
                format!("Symbol {} is not exported", s)
            }
        };
        self.diagnostics
            .push(Diagnostic::error(msg, span.line, span.column));
    }

    pub fn set_current_dir(&mut self, dir: String) {
        self.current_dir = Some(dir);
    }

    pub fn analyze(&mut self, program: Program) {
        self.record_node_info = true;
        self.current_file = program.file_path.clone();

        // Pass 1: Collect declarations from current program
        self.collect_definitions(&program);

        // Pass 2: Check statements
        self.current_file = program.file_path.clone(); // Ensure it's correct after potentially recursive calls
        for stmt in program.statements {
            self.check_statement(stmt);
        }
    }

    pub fn collect_definitions(&mut self, program: &Program) {
        let saved_file = self.current_file.clone();
        self.current_file = program.file_path.clone();
        for stmt in &program.statements {
            let (actual_stmt, is_exported) = match stmt {
                Statement::Export { decl, .. } => (&**decl, true),
                _ => (stmt, false),
            };

            if let Statement::ClassDeclaration {
                name,
                name_span,
                fields,
                methods,
                constructor: _,
                span,
                doc,
            } = actual_stmt
            {
                if let Some(existing) = self.classes.get(name) {
                    if existing.defined_in == self.current_file && existing.span == *span {
                        continue;
                    }
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                } else if let Some(existing) = self.scope.lookup_local(name) {
                    if existing.defined_in == self.current_file && existing.span == *name_span {
                        continue;
                    }
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                }
                let mut field_map = HashMap::new();
                let mut static_field_map = HashMap::new();
                for f in fields {
                    if field_map.contains_key(&f.name) || static_field_map.contains_key(&f.name) {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(f.name.clone()),
                            f.name_span,
                        );
                    }
                    let ty = self.resolve_type(f.ty.clone());
                    if f.is_static {
                        static_field_map.insert(
                            f.name.clone(),
                            (ty, f.name_span, f.doc.as_ref().map(|d| d.content())),
                        );
                    } else {
                        field_map.insert(
                            f.name.clone(),
                            (ty, f.name_span, f.doc.as_ref().map(|d| d.content())),
                        );
                    }
                }
                let mut method_map = HashMap::new();
                let mut static_method_map = HashMap::new();
                for m in methods {
                    if method_map.contains_key(&m.name)
                        || static_method_map.contains_key(&m.name)
                        || field_map.contains_key(&m.name)
                        || static_field_map.contains_key(&m.name)
                    {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(m.name.clone()),
                            m.name_span,
                        );
                    }
                    let param_tys = m
                        .params
                        .iter()
                        .map(|(_, ty)| self.resolve_type(ty.clone()))
                        .collect();
                    let ret_ty = self.resolve_type(m.return_ty.clone());
                    if m.is_static {
                        static_method_map.insert(
                            m.name.clone(),
                            (
                                param_tys,
                                ret_ty,
                                m.doc.as_ref().map(|d| d.content()),
                                m.name_span,
                            ),
                        );
                    } else {
                        method_map.insert(
                            m.name.clone(),
                            (
                                param_tys,
                                ret_ty,
                                m.doc.as_ref().map(|d| d.content()),
                                m.name_span,
                            ),
                        );
                    }
                }
                self.classes.insert(
                    name.clone(),
                    ClassInfo {
                        name: name.clone(),
                        fields: field_map,
                        static_fields: static_field_map,
                        methods: method_map,
                        static_methods: static_method_map,
                        is_exported,
                        defined_in: self.current_file.clone(),
                        span: *span,
                        doc: doc.as_ref().map(|d| d.content()),
                    },
                );
            } else if let Statement::FunctionDeclaration {
                name,
                name_span,
                params,
                return_ty,
                body: _,
                is_async: _,
                span: _,
                doc,
            } = actual_stmt
            {
                if let Some(existing) = self.classes.get(name) {
                    if existing.defined_in == self.current_file && existing.span == *name_span {
                        continue;
                    }
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                } else if let Some(existing) = self.scope.lookup_local(name) {
                    if existing.defined_in == self.current_file && existing.span == *name_span {
                        continue;
                    }
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                }
                let param_tys = params
                    .iter()
                    .map(|(_, ty)| self.resolve_type(ty.clone()))
                    .collect();
                let ret_ty = self.resolve_type(return_ty.clone());
                self.scope.insert(
                    name.clone(),
                    Type::Function(param_tys, Box::new(ret_ty)),
                    false,
                    true, // function declarations are constant
                    is_exported,
                    *name_span,
                    self.current_file.clone(),
                    doc.as_ref().map(|d| d.content()),
                );
            } else if let Statement::VarDeclaration {
                name,
                name_span,
                ty,
                value: _,
                is_const,
                span: _,
                doc,
            } = actual_stmt
            {
                if let Some(existing) = self.classes.get(name) {
                    if existing.defined_in == self.current_file && existing.span == *name_span {
                        continue;
                    }
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                } else if let Some(existing) = self.scope.lookup_local(name) {
                    if existing.defined_in == self.current_file && existing.span == *name_span {
                        continue;
                    }
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                }
                // In pass 1, we try to use the declared type if available.
                // Otherwise we use Unknown, and it will be properly inferred in pass 2.
                let var_ty = ty
                    .as_ref()
                    .map(|t| self.resolve_type(t.clone()))
                    .unwrap_or(Type::Unknown);
                self.scope.insert(
                    name.clone(),
                    var_ty,
                    false,
                    *is_const,
                    is_exported,
                    *name_span,
                    self.current_file.clone(),
                    doc.as_ref().map(|d| d.content()),
                );
            } else if let Statement::Enum(decl) = actual_stmt {
                if let Some(existing) = self.classes.get(&decl.name) {
                    if existing.defined_in == self.current_file && existing.span == decl.span {
                        continue;
                    }
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(decl.name.clone()),
                        decl.name_span,
                    );
                } else if let Some(existing) = self.scope.lookup_local(&decl.name) {
                    if existing.defined_in == self.current_file && existing.span == decl.name_span {
                        continue;
                    }
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(decl.name.clone()),
                        decl.name_span,
                    );
                }

                self.scope.insert(
                    decl.name.clone(),
                    Type::Enum(decl.name.clone()),
                    false,
                    true,
                    is_exported,
                    decl.name_span,
                    self.current_file.clone(),
                    decl.doc.as_ref().map(|d| d.content()),
                );
            } else if let Statement::Import {
                path,
                path_span,
                item,
                ..
            } = actual_stmt
            {
                self.load_import(path.clone(), *path_span);
                match item {
                    ImportItem::Named(names) => {
                        for (name, name_span) in names {
                            let sym_info = self
                                .scope
                                .lookup(name)
                                .map(|s| (s.is_exported, s.defined_in.clone(), s.span));
                            let class_info = self
                                .classes
                                .get(name)
                                .map(|c| (c.is_exported, self.current_file.clone(), c.span)); // simplified defined_in for classes

                            let export_check = if let Some(info) = sym_info {
                                Some(info)
                            } else {
                                class_info
                            };

                            if let Some((is_exported, def_file, def_span)) = export_check {
                                if !is_exported {
                                    self.error(
                                        SemanticErrorKind::ExportRequired(name.clone()),
                                        *name_span,
                                    );
                                }
                                self.record_definition(*name_span, def_file, def_span);

                                // Insert placeholder with correct is_exported flag if it was a symbol
                                // (Classes are already in self.classes)
                                if self.scope.lookup_local(name).is_none()
                                    && self.classes.get(name).is_none()
                                {
                                    // Placeholder insert for variables/functions
                                    self.scope.insert(
                                        name.clone(),
                                        Type::Unknown,
                                        false,
                                        false,
                                        is_exported,
                                        *name_span,
                                        self.current_file.clone(),
                                        None,
                                    );
                                }
                            } else {
                                self.error(
                                    SemanticErrorKind::UndefinedImport(name.clone(), path.clone()),
                                    *name_span,
                                );
                            }
                        }
                    }
                    ImportItem::Namespace((_ns, ns_span)) => {
                        if let Ok(abs_p) = self.resolve_import_path(path) {
                            self.record_definition(
                                *ns_span,
                                abs_p.to_string_lossy().to_string(),
                                Span::new(1, 1),
                            );
                        }
                    }
                }
            }
        }
        self.current_file = saved_file;
    }

    fn record_type(&mut self, span: Span, ty: Type) {
        if !self.record_node_info {
            return;
        }
        self.node_types
            .entry(self.current_file.clone())
            .or_insert_with(HashMap::new)
            .insert(span, ty);
    }

    fn record_definition(&mut self, span: Span, def_file: String, def_span: Span) {
        if !self.record_node_info {
            return;
        }
        self.node_definitions
            .entry(self.current_file.clone())
            .or_insert_with(HashMap::new)
            .insert(span, (def_file, def_span));
    }

    fn record_doc(&mut self, span: Span, doc: String) {
        if !self.record_node_info {
            return;
        }
        self.node_docs
            .entry(self.current_file.clone())
            .or_insert_with(HashMap::new)
            .insert(span, doc);
    }

    pub fn resolve_import_path(&self, path: &str) -> Result<std::path::PathBuf, std::io::Error> {
        if path.starts_with("std/") {
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
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Stdlib path not set",
                ))
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
        }
    }

    fn load_import(&mut self, path: String, span: Span) {
        let absolute_path = self.resolve_import_path(&path);

        if let Ok(abs_p) = absolute_path {
            let path_str = abs_p.to_string_lossy().to_string();
            if self.loaded_files.contains(&path_str) {
                return;
            }
            self.loaded_files.insert(path_str.clone());
            self.record_definition(span, path_str.clone(), Span::new(1, 1));

            if let Ok(source) = std::fs::read_to_string(&abs_p) {
                let mut lexer = crate::compiler::frontend::lexer::Lexer::new(&source);
                let mut parser = crate::compiler::frontend::parser::Parser::new(
                    lexer.lex_all(),
                    path_str.clone(),
                );
                let program = parser.parse_program();

                let saved_dir = self.current_dir.clone();
                if let Some(parent) = abs_p.parent() {
                    self.current_dir = Some(parent.to_string_lossy().to_string());
                }

                self.collect_definitions(&program);

                let saved_file = self.current_file.clone();
                self.current_file = path_str;
                for stmt in program.statements {
                    self.check_statement(stmt);
                }
                self.current_file = saved_file;

                self.current_dir = saved_dir;
            }
        }
    }

    pub fn load_stdlib(&mut self, stdlib_path: &str) {
        self.stdlib_path = Some(stdlib_path.to_string());
        let core_path = match std::path::Path::new(stdlib_path)
            .join("core.aura")
            .canonicalize()
        {
            Ok(p) => p,
            Err(_) => return,
        };
        if core_path.exists() {
            let path_str = core_path.to_string_lossy().to_string();
            if self.loaded_files.contains(&path_str) {
                return;
            }
            if let Ok(source) = std::fs::read_to_string(&core_path) {
                self.loaded_files.insert(path_str.clone());
                let mut lexer = crate::compiler::frontend::lexer::Lexer::new(&source);
                let mut parser = crate::compiler::frontend::parser::Parser::new(
                    lexer.lex_all(),
                    path_str.clone(),
                );
                let program = parser.parse_program();

                self.collect_definitions(&program);

                let saved_file = self.current_file.clone();
                self.current_file = path_str;
                for stmt in program.statements {
                    self.check_statement(stmt);
                }
                self.current_file = saved_file;
            }
        }
    }

    fn resolve_type(&self, te: TypeExpr) -> Type {
        match te {
            TypeExpr::Name(n, _) => match n.as_str() {
                "i32" | "Int32" | "number" | "Number" => Type::Int32,
                "i64" | "Int64" => Type::Int64,
                "f32" | "Float32" => Type::Float32,
                "f64" | "Float64" => Type::Float64,
                "string" | "String" => Type::String,
                "boolean" | "Boolean" => Type::Boolean,
                "void" | "Void" => Type::Void,
                "any" => Type::Unknown,
                _ => {
                    if let Some(sym) = self.scope.lookup(&n) {
                        if let Type::Enum(_) = sym.ty {
                            return sym.ty.clone();
                        }
                    }
                    Type::Class(n)
                }
            },
            TypeExpr::Union(tys, _) => {
                Type::Union(tys.into_iter().map(|t| self.resolve_type(t)).collect())
            }
            TypeExpr::Generic(name, args, _) => Type::Generic(
                name,
                args.into_iter().map(|t| self.resolve_type(t)).collect(),
            ),
            TypeExpr::Array(base, _) => Type::Array(Box::new(self.resolve_type(*base))),
            TypeExpr::Function(params, ret, _) => Type::Function(
                params.into_iter().map(|p| self.resolve_type(p)).collect(),
                Box::new(self.resolve_type(*ret)),
            ),
        }
    }

    fn is_assignable(&self, src: &Type, target: &Type) -> bool {
        self.is_assignable_internal(src, target, &mut Vec::new())
    }

    fn is_assignable_internal(
        &self,
        src: &Type,
        target: &Type,
        history: &mut Vec<(Type, Type)>,
    ) -> bool {
        if src == target {
            return true;
        }

        let pair = (src.clone(), target.clone());
        if history.contains(&pair) {
            return true;
        }
        history.push(pair);

        let result = match (src, target) {
            (Type::Unknown, _) | (_, Type::Unknown) => true,

            (s, Type::Union(options)) => options
                .iter()
                .any(|opt| self.is_assignable_internal(s, opt, history)),
            (Type::Union(options), t) => options
                .iter()
                .all(|opt| self.is_assignable_internal(opt, t, history)),

            (Type::Int32, Type::Int64) => true,

            // Array types
            (Type::Array(s), Type::Array(t)) => self.is_assignable_internal(s, t, history),

            // Generic types (Nominal for now, e.g. Box<i32> vs Box<i32>)
            (Type::Generic(src_name, src_args), Type::Generic(tgt_name, tgt_args)) => {
                if src_name != tgt_name || src_args.len() != tgt_args.len() {
                    return false;
                }
                for (s, t) in src_args.iter().zip(tgt_args.iter()) {
                    if !self.is_assignable_internal(s, t, history) {
                        return false;
                    }
                }
                true
            }

            // Structural identity for classes
            (Type::Class(src_name), Type::Class(tgt_name)) => {
                let src_info = self.classes.get(src_name);
                let tgt_info = self.classes.get(tgt_name);

                if let (Some(si), Some(ti)) = (src_info, tgt_info) {
                    let mut all_match = true;
                    for (name, (tgt_ty, _, _)) in &ti.fields {
                        if let Some((src_ty, _, _)) = si.fields.get(name) {
                            if !self.is_assignable_internal(src_ty, tgt_ty, history) {
                                all_match = false;
                                break;
                            }
                        } else {
                            all_match = false;
                            break;
                        }
                    }
                    all_match
                } else {
                    false
                }
            }

            _ => false,
        };

        history.pop();
        result
    }

    fn check_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::Enum(decl) => {
                let mut first_ty: Option<Type> = None;

                for member in &decl.members {
                    let member_ty = if let Some(ref expr) = member.value {
                        let ty = self.check_expr(expr.clone());
                        if !matches!(ty, Type::Int64 | Type::Int32 | Type::String) {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    "Int or String".to_string(),
                                    format!("{:?}", ty),
                                ),
                                member.name_span,
                            );
                        }

                        ty
                    } else {
                        // Implicit value
                        if let Some(Type::String) = first_ty {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    "Explicit String value required for all members".to_string(),
                                    "Implicit Enum value".to_string(),
                                ),
                                member.name_span,
                            );
                        }
                        // It's implicitly an integer
                        Type::Int64
                    };

                    if let Some(ref first) = first_ty {
                        // Check if all members have the same primitive type base
                        let base_first = if matches!(first, Type::Int64 | Type::Int32) {
                            Type::Int64
                        } else {
                            first.clone()
                        };
                        let base_current = if matches!(member_ty, Type::Int64 | Type::Int32) {
                            Type::Int64
                        } else {
                            member_ty.clone()
                        };

                        if base_first != base_current && base_current != Type::Unknown {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    format!("{:?}", first),
                                    format!("{:?}", member_ty),
                                ),
                                member.name_span,
                            );
                        }
                    } else {
                        first_ty = Some(member_ty.clone());
                    }

                    // Register enum member as a constant
                    // E.g., `Direction.Up` will be registered as `Direction.Up` in the scope
                    let fqn = format!("{}.{}", decl.name, member.name);
                    let is_exported = self
                        .scope
                        .lookup_local(&decl.name)
                        .map(|s| s.is_exported)
                        .unwrap_or(false);
                    self.scope.insert(
                        fqn,
                        Type::Enum(decl.name.clone()),
                        false,
                        true, // Enum members are constant
                        is_exported,
                        member.name_span,
                        self.current_file.clone(),
                        None,
                    );
                }

                // Register the Enum type itself
                let is_exported = self
                    .scope
                    .lookup_local(&decl.name)
                    .map(|s| s.is_exported)
                    .unwrap_or(false);
                self.scope.insert(
                    decl.name.clone(),
                    Type::Enum(decl.name.clone()),
                    false,
                    true, // Enum itself is a constant symbol
                    is_exported,
                    decl.name_span,
                    self.current_file.clone(),
                    decl.doc.as_ref().map(|d| d.content()),
                );
            }
            Statement::VarDeclaration {
                name,
                name_span,
                ty,
                value,
                is_const,
                span,
                doc,
            } => {
                if self.scope.parent.is_some() {
                    if self.scope.lookup_local(&name).is_some() {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(name.clone()),
                            name_span,
                        );
                    }
                }
                let val_ty = self.check_expr(value);
                let declared_ty = ty
                    .map(|t| self.resolve_type(t))
                    .unwrap_or_else(|| val_ty.clone());
                if !self.is_assignable(&val_ty, &declared_ty) {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            format!("{:?}", declared_ty),
                            format!("{:?}", val_ty),
                        ),
                        span,
                    );
                }
                if self.record_node_info {
                    if let Some(d) = &doc {
                        self.record_doc(name_span, d.content());
                    }
                    self.record_type(name_span, declared_ty.clone());
                }
                let is_exported_flag = self
                    .scope
                    .lookup_local(&name)
                    .map(|s| s.is_exported)
                    .unwrap_or(false);
                self.scope.insert(
                    name,
                    declared_ty,
                    false,
                    is_const,
                    is_exported_flag,
                    span,
                    self.current_file.clone(),
                    doc.as_ref().map(|d| d.content()),
                );
            }
            Statement::Expression(expr, _) => {
                self.check_expr(expr);
            }
            Statement::Print(expr, _) => {
                self.check_expr(expr);
            }
            Statement::Block(stmts, _) => {
                self.push_scope();
                for s in stmts {
                    self.check_statement(s);
                }
                self.pop_scope();
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => {
                let _cond_ty = self.check_expr(condition.clone());

                if let Expr::TypeTest(ref expr, ref ty_expr, _) = condition {
                    if let Expr::Variable(ref name, _) = **expr {
                        let narrowed_ty = self.resolve_type(ty_expr.clone());

                        self.push_scope();
                        self.scope.insert(
                            name.clone(),
                            narrowed_ty.clone(),
                            false,
                            false,
                            false, // narrowed type in block
                            span,
                            self.current_file.clone(),
                            None,
                        );
                        self.check_statement(*then_branch);
                        self.pop_scope();

                        if let Some(eb) = else_branch {
                            let original_ty = self
                                .scope
                                .lookup(name)
                                .map(|s| s.ty.clone())
                                .unwrap_or(Type::Unknown);
                            let excluded_ty = original_ty.exclude(&narrowed_ty);

                            self.push_scope();
                            self.scope.insert(
                                name.clone(),
                                excluded_ty,
                                false,
                                false,
                                false, // narrowed type in block
                                span,
                                self.current_file.clone(),
                                None,
                            );
                            self.check_statement(*eb);
                            self.pop_scope();
                        }
                        return;
                    }
                }

                self.check_statement(*then_branch);
                if let Some(eb) = else_branch {
                    self.check_statement(*eb);
                }
            }
            Statement::While {
                condition, body, ..
            } => {
                self.check_expr(condition);
                self.check_statement(*body);
            }
            Statement::Return(expr, _) => {
                self.check_expr(expr);
            }
            Statement::FunctionDeclaration {
                name,
                name_span,
                params,
                return_ty,
                body,
                is_async: _,
                span,
                doc,
            } => {
                if self.scope.parent.is_some() {
                    if self.scope.lookup_local(&name).is_some() {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(name.clone()),
                            name_span,
                        );
                    }
                }
                let param_tys: Vec<Type> = params
                    .iter()
                    .map(|(_, ty)| self.resolve_type(ty.clone()))
                    .collect();
                let ret_ty = self.resolve_type(return_ty);

                // Register function before checking body for recursion
                let func_ty = Type::Function(param_tys.clone(), Box::new(ret_ty.clone()));
                let is_exported_flag = self
                    .scope
                    .lookup_local(&name)
                    .map(|s| s.is_exported)
                    .unwrap_or(false);
                self.scope.insert(
                    name.clone(),
                    func_ty.clone(),
                    false,
                    true, // functions are constant
                    is_exported_flag,
                    span,
                    self.current_file.clone(),
                    doc.as_ref().map(|d| d.content()),
                );
                if self.record_node_info {
                    if let Some(d) = &doc {
                        self.record_doc(name_span, d.content());
                    }
                    self.record_type(name_span, func_ty);
                }

                self.push_scope();
                for (pname, pty) in params {
                    let ty = self.resolve_type(pty.clone());
                    self.scope.insert(
                        pname,
                        ty,
                        true,
                        false,
                        false,
                        pty.span(),
                        self.current_file.clone(),
                        None,
                    );
                }
                self.check_statement(*body);
                self.pop_scope();
            }
            Statement::ClassDeclaration {
                name,
                name_span: _,
                fields,
                methods,
                constructor,
                span: _,
                doc: _,
            } => {
                self.current_class = Some(name.clone());

                for f in fields {
                    if let Some(init) = &f.value {
                        let init_ty = self.check_expr(init.clone());
                        let expected_ty = self.resolve_type(f.ty.clone());
                        if init_ty != expected_ty {
                            self.diagnostics.push(Diagnostic::error(
                                format!(
                                    "Type mismatch in field initializer for {}: expected {}, found {}",
                                    f.name, expected_ty, init_ty
                                ),
                                f.span.line,
                                f.span.column,
                            ));
                        }
                    }
                }

                if let Some(ctor) = constructor {
                    self.push_scope();
                    self.scope.insert(
                        "this".to_string(),
                        Type::Class(name.clone()),
                        false,
                        true,  // this is constant
                        false, // this is not exported
                        ctor.span,
                        self.current_file.clone(),
                        None,
                    );
                    for (pname, pty) in ctor.params {
                        let ty = self.resolve_type(pty.clone());
                        self.scope.insert(
                            pname,
                            ty,
                            true,
                            false,
                            false, // param not exported
                            pty.span(),
                            self.current_file.clone(),
                            None,
                        );
                    }
                    self.check_statement(*ctor.body);
                    self.pop_scope();
                }

                for m in methods {
                    self.push_scope();
                    self.scope.insert(
                        "this".to_string(),
                        Type::Class(name.clone()),
                        false,
                        true,  // this is constant
                        false, // this is not exported
                        m.span,
                        self.current_file.clone(),
                        None,
                    );
                    for (pname, pty) in m.params {
                        let ty = self.resolve_type(pty.clone());
                        self.scope.insert(
                            pname,
                            ty,
                            true,
                            false,
                            false, // param not exported
                            pty.span(),
                            self.current_file.clone(),
                            None,
                        );
                    }
                    self.check_statement(*m.body);
                    self.pop_scope();
                }
                self.current_class = None;
            }
            Statement::Error => {}
            Statement::Import { .. } => {
                // Symbols should be loaded in Pass 1 (collect_definitions)
                // But we should also check the body of the imported file for errors
                // However, we need to be careful not to check it multiple times.
                // For now, Pass 1 loading is enough for symbols.
                // To support full error checking of dependencies, we'd need a more robust module system.
            }
            Statement::TryCatch {
                try_block,
                catch_param,
                catch_block,
                finally_block,
                ..
            } => {
                self.check_statement(*try_block);

                if let Some(cb) = catch_block {
                    self.push_scope();

                    if let Some((name, ty_expr)) = catch_param {
                        let ty = self.resolve_type(ty_expr);
                        // If type is Unknown (not provided), we might want to default to Error
                        let final_ty = if matches!(ty, Type::Unknown) {
                            Type::Class("Error".to_string())
                        } else {
                            ty
                        };
                        self.scope.insert(
                            name.clone(),
                            final_ty,
                            true,
                            false,
                            false, // catch param not exported
                            Span::new(0, 0),
                            self.current_file.clone(),
                            None,
                        );
                    }

                    self.check_statement(*cb);
                    self.pop_scope();
                }

                if let Some(fb) = finally_block {
                    self.check_statement(*fb);
                }
            }
            Statement::Export { decl, .. } => {
                self.check_statement(*decl);
            }
            Statement::Comment(_, _) | Statement::RegularBlockComment(_, _) => {}
        }
    }

    fn check_expr(&mut self, expr: Expr) -> Type {
        let span = expr.span();
        let ty = match expr {
            Expr::Number(_, _) => Type::Int32,
            Expr::StringLiteral(_, _) => Type::String,
            Expr::Template(parts, _s) => {
                for part in parts {
                    if let crate::compiler::ast::TemplatePart::Expr(e) = part {
                        self.check_expr(*e);
                    }
                }
                Type::String
            }
            Expr::Await(expr, _s) => {
                let ty = self.check_expr(*expr);
                // Basic validation: await target should be a Promise (or any for now)
                // For now, we just return the inner type if it's a Promise, or the type itself
                match ty {
                    Type::Generic(name, args) if name == "Promise" && args.len() == 1 => {
                        args[0].clone()
                    }
                    _ => ty, // Fallback
                }
            }
            Expr::ArrayLiteral(elements, _) => {
                let mut element_tys = Vec::new();
                for el in elements {
                    element_tys.push(self.check_expr(el));
                }
                let base_ty = if element_tys.is_empty() {
                    Type::Unknown
                } else {
                    let first = element_tys[0].clone();
                    if element_tys.iter().all(|t| *t == first) {
                        first
                    } else {
                        Type::Union(element_tys)
                    }
                };
                Type::Array(Box::new(base_ty))
            }
            Expr::Null(_) => Type::Null,
            Expr::Error(_s) => Type::Unknown,
            Expr::Variable(name, span) => {
                if let Some(sym) = self.scope.lookup(&name) {
                    let ty = sym.ty.clone();
                    if self.record_node_info {
                        let doc = sym.doc.clone();
                        let sym_span = sym.span;
                        let defined_in = sym.defined_in.clone();
                        if let Some(d) = doc {
                            self.record_doc(span, d);
                        }
                        self.record_definition(span, defined_in, sym_span);
                    }
                    ty
                } else if self.classes.contains_key(&name) {
                    Type::Class(name)
                } else {
                    self.error(SemanticErrorKind::UndefinedVariable(name), span);
                    Type::Unknown
                }
            }
            Expr::BinaryOp(left, op, right, span) => {
                let lhs = self.check_expr(*left);
                let rhs = self.check_expr(*right);
                match op.as_str() {
                    "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                        // Allow comparison between same types, or classes/unions/unknown and null
                        let is_nullable = |ty: &Type| {
                            matches!(
                                ty,
                                Type::Class(_) | Type::Union(_) | Type::Unknown | Type::Null
                            )
                        };
                        let ok = if lhs == rhs {
                            true
                        } else if is_nullable(&lhs) && is_nullable(&rhs) {
                            true
                        } else {
                            false
                        };

                        if !ok {
                            self.error(
                                SemanticErrorKind::IncompatibleBinaryOperators(
                                    lhs.to_string(),
                                    op,
                                    rhs.to_string(),
                                ),
                                span,
                            );
                        }
                        Type::Boolean
                    }
                    "&&" | "||" => {
                        if lhs.is_boolean() && rhs.is_boolean() {
                            Type::Boolean
                        } else {
                            self.error(
                                SemanticErrorKind::IncompatibleBinaryOperators(
                                    lhs.to_string(),
                                    op,
                                    rhs.to_string(),
                                ),
                                span,
                            );
                            Type::Boolean
                        }
                    }
                    "+" | "-" | "*" | "/" | "%" | "|" => {
                        if lhs.is_numeric() && rhs.is_numeric() {
                            lhs
                        } else if op == "+" && (lhs == Type::String || rhs == Type::String) {
                            Type::String
                        } else {
                            self.error(
                                SemanticErrorKind::IncompatibleBinaryOperators(
                                    lhs.to_string(),
                                    op,
                                    rhs.to_string(),
                                ),
                                span,
                            );
                            Type::Unknown
                        }
                    }
                    _ => Type::Unknown,
                }
            }
            Expr::Assign(name, value, span) => {
                let val_ty = self.check_expr(*value);
                let (sym_is_const, sym_ty) = if let Some(sym) = self.scope.lookup(&name) {
                    (sym.is_const, Some(sym.ty.clone()))
                } else {
                    (false, None)
                };

                if let Some(expected_ty) = sym_ty {
                    if sym_is_const {
                        self.error(
                            SemanticErrorKind::CannotAssignToConstant(name.clone()),
                            span,
                        );
                    }
                    if !self.is_assignable(&val_ty, &expected_ty) {
                        self.error(
                            SemanticErrorKind::TypeMismatch(
                                format!("{:?}", expected_ty),
                                format!("{:?}", val_ty),
                            ),
                            span,
                        );
                    }
                } else {
                    self.error(SemanticErrorKind::UndefinedVariable(name), span);
                }
                val_ty
            }
            Expr::Call(name, name_span, args, span) => {
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg));
                }

                let sym = self.scope.lookup(&name);
                let function_ty = sym.map(|s| s.ty.clone());

                if let Some(Type::Function(param_tys, ret_ty)) = function_ty {
                    if self.record_node_info {
                        if let Some(sym) = self.scope.lookup(&name) {
                            let doc = sym.doc.clone();
                            let sym_span = sym.span;
                            let defined_in = sym.defined_in.clone();
                            if let Some(d) = doc {
                                self.record_doc(name_span, d);
                            }
                            self.record_definition(name_span, defined_in, sym_span);
                            self.record_type(
                                name_span,
                                Type::Function(param_tys.clone(), ret_ty.clone()),
                            );
                        }
                        // Also record return type for the whole call span
                        self.record_type(span, (*ret_ty).clone());
                    }
                    if param_tys.len() != arg_tys.len() {
                        self.error(
                            SemanticErrorKind::WrongArgumentCount(
                                name,
                                param_tys.len(),
                                arg_tys.len(),
                            ),
                            span,
                        );
                        return (*ret_ty).clone();
                    }
                    for (i, arg_ty) in arg_tys.iter().enumerate() {
                        if !self.is_assignable(arg_ty, &param_tys[i]) {
                            self.error(
                                SemanticErrorKind::TypeMismatch(
                                    format!("{:?}", param_tys[i]),
                                    format!("{:?}", arg_ty),
                                ),
                                span,
                            );
                        }
                    }
                    *ret_ty
                } else {
                    self.error(SemanticErrorKind::UndefinedFunction(name), name_span);
                    Type::Unknown
                }
            }
            Expr::New(class_name, name_span, args, span) => {
                if let Some(class_info) = self.classes.get(&class_name) {
                    if self.record_node_info {
                        let doc_opt = class_info.doc.clone();
                        let class_span = class_info.span;
                        if let Some(doc) = doc_opt {
                            self.record_doc(name_span, doc);
                        }
                        self.record_definition(name_span, self.current_file.clone(), class_span); // Class defined in same file for now if simple
                        self.record_type(name_span, Type::Class(class_name.clone()));
                        // Also record for the whole expression
                        self.record_type(span, Type::Class(class_name.clone()));
                    }
                } else {
                    self.error(SemanticErrorKind::UndefinedClass(class_name.clone()), span);
                }
                for arg in args {
                    self.check_expr(arg);
                }
                Type::Class(class_name)
            }
            Expr::MemberAccess(obj, field, name_span, span) => {
                let obj_ty = self.check_expr(*obj);

                if let Type::Class(ref class_name) | Type::Enum(ref class_name) = obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        let field_info = class_info
                            .fields
                            .get(&field)
                            .or(class_info.static_fields.get(&field))
                            .cloned();

                        if let Some((field_ty, field_span, doc)) = field_info {
                            if self.record_node_info {
                                if let Some(d) = doc {
                                    self.record_doc(name_span, d);
                                }
                                self.record_definition(
                                    name_span,
                                    self.current_file.clone(),
                                    field_span,
                                ); // Same file for simplicity now
                                self.record_type(name_span, field_ty.clone());
                                self.record_type(span, field_ty.clone());
                            }
                            field_ty
                        } else {
                            if matches!(obj_ty, Type::Enum(_)) {
                                // For enums, members are registered directly in the scope as Name.Member
                                let fqn = format!("{}.{}", class_name, field);
                                if let Some(sym) = self.scope.lookup(&fqn) {
                                    if self.record_node_info {
                                        let sym_ty = sym.ty.clone();
                                        let sym_doc = sym.doc.clone();
                                        let sym_defined_in = sym.defined_in.clone();
                                        let sym_span = sym.span;

                                        if let Some(d) = sym_doc {
                                            self.record_doc(name_span, d);
                                        }
                                        self.record_definition(name_span, sym_defined_in, sym_span);
                                        self.record_type(name_span, sym_ty.clone());
                                        self.record_type(span, sym_ty.clone());
                                        return sym_ty;
                                    }
                                    return sym.ty.clone();
                                }
                                self.error(
                                    SemanticErrorKind::UndefinedField(class_name.clone(), field),
                                    name_span,
                                );
                                return Type::Unknown;
                            }
                            self.error(
                                SemanticErrorKind::UndefinedField(class_name.clone(), field),
                                name_span,
                            );
                            Type::Unknown
                        }
                    } else {
                        if matches!(obj_ty, Type::Enum(_)) {
                            // Enum itself is just a namespace for its members
                            let fqn = format!("{}.{}", class_name, field);
                            if let Some(sym) = self.scope.lookup(&fqn) {
                                if self.record_node_info {
                                    let sym_ty = sym.ty.clone();
                                    let sym_doc = sym.doc.clone();
                                    let sym_defined_in = sym.defined_in.clone();
                                    let sym_span = sym.span;

                                    if let Some(d) = sym_doc {
                                        self.record_doc(name_span, d);
                                    }
                                    self.record_definition(name_span, sym_defined_in, sym_span);
                                    self.record_type(name_span, sym_ty.clone());
                                    self.record_type(span, sym_ty.clone());
                                    return sym_ty;
                                }
                                return sym.ty.clone();
                            }
                            self.error(
                                SemanticErrorKind::UndefinedField(class_name.clone(), field),
                                name_span,
                            );
                        } else {
                            self.error(SemanticErrorKind::UndefinedClass(class_name.clone()), span);
                        }
                        Type::Unknown
                    }
                } else {
                    self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                    Type::Unknown
                }
            }
            Expr::MemberAssign(obj, field, value, name_span, span) => {
                let obj_ty = self.check_expr(*obj);
                let val_ty = self.check_expr(*value);

                let field_info = if let Type::Class(ref class_name) = obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        class_info.fields.get(&field).cloned()
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some((field_ty, field_span, doc)) = field_info {
                    if !self.is_assignable(&val_ty, &field_ty) {
                        self.error(
                            SemanticErrorKind::TypeMismatch(
                                format!("{:?}", field_ty),
                                format!("{:?}", val_ty),
                            ),
                            span,
                        );
                    }
                    if self.record_node_info {
                        if let Some(d) = doc {
                            self.record_doc(name_span, d);
                        }
                        self.record_definition(name_span, self.current_file.clone(), field_span);
                        self.record_type(name_span, field_ty.clone());
                        self.record_type(span, field_ty.clone());
                    }
                } else {
                    if let Type::Class(class_name) = obj_ty {
                        if self.classes.contains_key(&class_name) {
                            self.error(
                                SemanticErrorKind::UndefinedField(class_name, field),
                                name_span,
                            );
                        } else {
                            self.error(SemanticErrorKind::UndefinedClass(class_name), span);
                        }
                    } else {
                        self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                    }
                }
                val_ty
            }
            Expr::MethodCall(obj, method, name_span, args, span) => {
                let obj_ty = self.check_expr(*obj);
                let mut arg_tys = Vec::new();
                for arg in args {
                    arg_tys.push(self.check_expr(arg));
                }

                if let Type::Class(class_name) = obj_ty {
                    let method_info = if let Some(class_info) = self.classes.get(&class_name) {
                        if let Some(m) = class_info.methods.get(&method) {
                            Some(m.clone())
                        } else {
                            class_info.static_methods.get(&method).cloned()
                        }
                    } else {
                        None
                    };

                    if let Some((param_tys, ret_ty, doc, mspan)) = method_info {
                        if self.record_node_info {
                            if let Some(d) = doc {
                                self.record_doc(name_span, d.clone());
                            }
                            self.record_definition(name_span, self.current_file.clone(), mspan);
                            self.record_type(
                                name_span,
                                Type::Function(param_tys.clone(), Box::new(ret_ty.clone())),
                            );
                            self.record_type(span, ret_ty.clone());
                        }

                        if param_tys.len() != arg_tys.len() {
                            self.error(
                                SemanticErrorKind::WrongArgumentCount(
                                    method,
                                    param_tys.len(),
                                    arg_tys.len(),
                                ),
                                span,
                            );
                            return ret_ty;
                        }
                        for (i, arg_ty) in arg_tys.iter().enumerate() {
                            if !self.is_assignable(arg_ty, &param_tys[i]) {
                                self.error(
                                    SemanticErrorKind::TypeMismatch(
                                        format!("{:?}", param_tys[i]),
                                        format!("{:?}", arg_ty),
                                    ),
                                    span,
                                );
                            }
                        }
                        ret_ty
                    } else {
                        self.error(SemanticErrorKind::UndefinedMethod(class_name, method), span);
                        Type::Unknown
                    }
                } else if obj_ty == Type::String {
                    let ret_ty = match method.as_str() {
                        "len" => Type::Int32,
                        "charAt" => Type::String,
                        "substring" => Type::String,
                        "indexOf" => Type::Int32,
                        "toUpper" | "toLower" | "trim" => Type::String,
                        _ => {
                            self.error(
                                SemanticErrorKind::UndefinedMethod("string".to_string(), method),
                                span,
                            );
                            Type::Unknown
                        }
                    };
                    if self.record_node_info && !matches!(ret_ty, Type::Unknown) {
                        self.record_type(span, ret_ty.clone());
                    }
                    ret_ty
                } else if let Type::Array(inner) = obj_ty {
                    let ret_ty = match method.as_str() {
                        "len" => Type::Int32,
                        "push" => Type::Void,
                        "pop" => (*inner).clone(),
                        "join" => Type::String,
                        "get" => (*inner).clone(),
                        _ => {
                            self.error(
                                SemanticErrorKind::UndefinedMethod("array".to_string(), method),
                                span,
                            );
                            Type::Unknown
                        }
                    };
                    if self.record_node_info && !matches!(ret_ty, Type::Unknown) {
                        self.record_type(span, ret_ty.clone());
                    }
                    ret_ty
                } else {
                    self.error(SemanticErrorKind::NotAClass(format!("{:?}", obj_ty)), span);
                    Type::Unknown
                }
            }
            Expr::This(span) => {
                if let Some(class_name) = &self.current_class {
                    Type::Class(class_name.clone())
                } else {
                    self.error(
                        SemanticErrorKind::UndefinedVariable("this".to_string()),
                        span,
                    );
                    Type::Unknown
                }
            }
            Expr::TypeTest(expr, ty_expr, _) => {
                self.check_expr(*expr);
                self.resolve_type(ty_expr);
                Type::Boolean
            }
            Expr::Throw(expr, span) => {
                let expr_ty = self.check_expr(*expr);
                let error_ty = Type::Class("Error".to_string());
                if !self.is_assignable(&expr_ty, &error_ty) {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            "Error".to_string(),
                            format!("{:?}", expr_ty),
                        ),
                        span,
                    );
                }
                Type::Unknown // throw doesn't "return" a value to its context
            }
            Expr::Index(obj, index, span) => {
                let obj_ty = self.check_expr(*obj);
                let index_ty = self.check_expr(*index);
                if index_ty != Type::Int32 {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            "i32".to_string(),
                            format!("{:?}", index_ty),
                        ),
                        span,
                    );
                }
                if let Type::Array(inner) = obj_ty {
                    *inner
                } else if obj_ty == Type::String {
                    Type::String
                } else {
                    self.error(
                        SemanticErrorKind::TypeMismatch(
                            "array or string".to_string(),
                            format!("{:?}", obj_ty),
                        ),
                        span,
                    );
                    Type::Unknown
                }
            }
            Expr::UnaryOp(op, expr, span) => {
                let expr_ty = self.check_expr(*expr);
                if op == "-" && expr_ty.is_numeric() {
                    expr_ty
                } else {
                    self.error(
                        SemanticErrorKind::IncompatibleBinaryOperators(
                            format!("{:?}", expr_ty),
                            op,
                            "None".to_string(),
                        ),
                        span,
                    );
                    Type::Unknown
                }
            }
        };
        if self.record_node_info {
            self.record_type(span, ty.clone());
        }
        ty
    }

    fn push_scope(&mut self) {
        let current = std::mem::replace(&mut self.scope, Box::new(Scope::new(None)));
        self.scope = Box::new(Scope::new(Some(current)));
    }

    fn pop_scope(&mut self) {
        let mut child = std::mem::replace(&mut self.scope, Box::new(Scope::new(None)));
        if let Some(parent) = child.parent.take() {
            self.scope = parent;
        } else {
            panic!("Popped root scope");
        }
    }
}
