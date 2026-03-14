use crate::compiler::ast::{Program, Span, TypeExpr};
use crate::compiler::frontend::error::{Diagnostic, DiagnosticList};
use crate::compiler::sema::scope::Scope;
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

pub mod decl;
pub mod expr;
pub mod stmt;

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

    pub fn error(&mut self, kind: SemanticErrorKind, span: Span) {
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

    pub fn record_type(&mut self, span: Span, ty: Type) {
        if !self.record_node_info {
            return;
        }
        self.node_types
            .entry(self.current_file.clone())
            .or_insert_with(HashMap::new)
            .insert(span, ty);
    }

    pub fn record_definition(&mut self, span: Span, def_file: String, def_span: Span) {
        if !self.record_node_info {
            return;
        }
        self.node_definitions
            .entry(self.current_file.clone())
            .or_insert_with(HashMap::new)
            .insert(span, (def_file, def_span));
    }

    pub fn record_doc(&mut self, span: Span, doc: String) {
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

    pub fn load_import(&mut self, path: String, span: Span) {
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
        let core_path = std::path::Path::new(stdlib_path).join("core.aura");
        if core_path.exists() {
            if let Ok(source) = std::fs::read_to_string(&core_path) {
                let path_str = core_path.to_string_lossy().to_string();
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

    pub fn resolve_type(&self, te: TypeExpr) -> Type {
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

    pub fn is_assignable(&self, src: &Type, target: &Type) -> bool {
        self.is_assignable_internal(src, target, &mut Vec::new())
    }

    pub fn is_assignable_internal(
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

    pub fn push_scope(&mut self) {
        let current = std::mem::replace(&mut self.scope, Box::new(Scope::new(None)));
        self.scope = Box::new(Scope::new(Some(current)));
    }

    pub fn pop_scope(&mut self) {
        let mut child = std::mem::replace(&mut self.scope, Box::new(Scope::new(None)));
        if let Some(parent) = child.parent.take() {
            self.scope = parent;
        } else {
            panic!("Popped root scope");
        }
    }
}
