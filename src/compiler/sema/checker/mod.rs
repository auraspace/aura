use crate::compiler::ast::{AccessModifier, Program, Span, TypeExpr};
use crate::compiler::frontend::error::{Diagnostic, DiagnosticList};
use crate::compiler::sema::scope::Scope;
use crate::compiler::sema::ty::Type;
use std::collections::{HashMap, HashSet};

pub mod decl;
pub mod expr;
pub mod stmt;

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub ty: Type,
    pub is_static: bool,
    pub is_readonly: bool,
    pub defined_in_class: String,
    pub access: AccessModifier,
    pub span: Span,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub params: Vec<Type>,
    pub ret_ty: Type,
    pub is_static: bool,
    pub is_async: bool,
    pub is_override: bool,
    pub is_abstract: bool,
    pub defined_in_class: String,
    pub access: AccessModifier,
    pub span: Span,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub parent: Option<String>,
    pub implements: Vec<String>,
    pub fields: HashMap<String, FieldInfo>,
    pub methods: HashMap<String, MethodInfo>,
    pub constructor: Option<(Vec<Type>, AccessModifier)>,
    pub is_exported: bool,
    pub is_abstract: bool,
    pub defined_in: String,
    pub span: Span,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub fields: HashMap<String, FieldInfo>,
    pub methods: HashMap<String, MethodInfo>,
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
    StaticMethodAsync(String),
    AccessDenied(String, String, String), // member, class, required_access
    ReadonlyViolation(String),
    ArgumentCountMismatch(usize, usize), // expected, found
    CircularInheritance(String),
    MissingOverride(String, String),      // class, method
    UnexpectedOverride(String, String),   // class, method
    IncompatibleOverride(String, String), // class, method
    SuperOutsideClass,
    SuperInStaticMethod,
    NoParentClass(String),
    CannotInstantiateAbstractClass(String),
    AbstractMethodInConcreteClass(String, String), // class, method
    AbstractMethodWithBody(String, String),        // class, method
    ConcreteClassMissingImplementation(String, String), // class, method
}

#[derive(Debug, Clone)]
pub struct SemanticError {
    pub kind: SemanticErrorKind,
    pub span: Span,
}

pub struct SemanticAnalyzer {
    pub scope: Box<Scope>,
    pub classes: HashMap<String, ClassInfo>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub current_class: Option<String>,
    pub current_method: Option<String>,
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
            interfaces: HashMap::new(),
            current_class: None,
            current_method: None,
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
        let mut promise_methods = HashMap::new();
        let span = Span::new(0, 0);

        // Promise.all<T>(values: Array<Promise<T>>): Promise<Array<T>>
        promise_methods.insert(
            "all".to_string(),
            MethodInfo {
                params: vec![Type::Array(Box::new(Type::Unknown))],
                ret_ty: Type::Generic(
                    "Promise".to_string(),
                    vec![Type::Array(Box::new(Type::Unknown))],
                ),
                is_static: true,
                is_async: false,
                is_override: false,
                is_abstract: false,
                defined_in_class: "Promise".to_string(),
                access: AccessModifier::Public,
                span,
                doc: Some("Waits for all promises to be resolved".to_string()),
            },
        );

        promise_methods.insert(
            "allSettled".to_string(),
            MethodInfo {
                params: vec![Type::Array(Box::new(Type::Unknown))],
                ret_ty: Type::Generic(
                    "Promise".to_string(),
                    vec![Type::Array(Box::new(Type::Unknown))],
                ),
                is_static: true,
                is_async: false,
                is_override: false,
                is_abstract: false,
                defined_in_class: "Promise".to_string(),
                access: AccessModifier::Public,
                span,
                doc: Some("Waits for all promises to be settled".to_string()),
            },
        );

        promise_methods.insert(
            "any".to_string(),
            MethodInfo {
                params: vec![Type::Array(Box::new(Type::Unknown))],
                ret_ty: Type::Generic("Promise".to_string(), vec![Type::Unknown]),
                is_static: true,
                is_async: false,
                is_override: false,
                is_abstract: false,
                defined_in_class: "Promise".to_string(),
                access: AccessModifier::Public,
                span,
                doc: Some("Waits for any promise to be resolved".to_string()),
            },
        );

        promise_methods.insert(
            "race".to_string(),
            MethodInfo {
                params: vec![Type::Array(Box::new(Type::Unknown))],
                ret_ty: Type::Generic("Promise".to_string(), vec![Type::Unknown]),
                is_static: true,
                is_async: false,
                is_override: false,
                is_abstract: false,
                defined_in_class: "Promise".to_string(),
                access: AccessModifier::Public,
                span,
                doc: Some("Waits for the first promise to be settled".to_string()),
            },
        );

        analyzer.classes.insert(
            "Promise".to_string(),
            ClassInfo {
                name: "Promise".to_string(),
                parent: None,
                implements: Vec::new(),
                fields: HashMap::new(),
                methods: promise_methods,
                constructor: None,
                is_exported: true,
                is_abstract: false,
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
            SemanticErrorKind::StaticMethodAsync(n) => {
                format!("Static method '{}' cannot be async", n)
            }
            SemanticErrorKind::AccessDenied(member, class, req) => {
                format!(
                    "Access to {} member '{}' of class '{}' is denied",
                    req, member, class
                )
            }
            SemanticErrorKind::ReadonlyViolation(n) => {
                format!("Cannot assign to readonly field '{}'", n)
            }
            SemanticErrorKind::ArgumentCountMismatch(e, f) => {
                format!("Argument count mismatch: expected {}, found {}", e, f)
            }
            SemanticErrorKind::CircularInheritance(c) => {
                format!("Circular inheritance detected for class '{}'", c)
            }
            SemanticErrorKind::MissingOverride(c, m) => {
                format!("Method '{}' in class '{}' overrides a base class method but is missing the 'override' keyword", m, c)
            }
            SemanticErrorKind::UnexpectedOverride(c, m) => {
                format!("Method '{}' in class '{}' has the 'override' keyword but doesn't override any base class method", m, c)
            }
            SemanticErrorKind::IncompatibleOverride(c, m) => {
                format!("Method '{}' in class '{}' has an incompatible signature with the base class method it overrides", m, c)
            }
            SemanticErrorKind::SuperOutsideClass => {
                "Keyword 'super' can only be used inside a class".to_string()
            }
            SemanticErrorKind::SuperInStaticMethod => {
                "Keyword 'super' cannot be used in static methods".to_string()
            }
            SemanticErrorKind::NoParentClass(c) => {
                format!(
                    "Class '{}' does not have a parent class to call super on",
                    c
                )
            }
            SemanticErrorKind::CannotInstantiateAbstractClass(c) => {
                format!("Cannot instantiate abstract class '{}'", c)
            }
            SemanticErrorKind::AbstractMethodInConcreteClass(c, m) => {
                format!(
                    "Abstract method '{}' cannot be declared in concrete class '{}'",
                    m, c
                )
            }
            SemanticErrorKind::AbstractMethodWithBody(c, m) => {
                format!(
                    "Abstract method '{}' in class '{}' cannot have a body",
                    m, c
                )
            }
            SemanticErrorKind::ConcreteClassMissingImplementation(c, m) => {
                format!(
                    "Concrete class '{}' must implement abstract method '{}'",
                    c, m
                )
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

        // Pass 1.2: Validate interfaces
        self.validate_interfaces();

        // Pass 1.5: Validate inheritance hierarchies
        self.validate_inheritance();

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

            // Interface structural typing
            (src_ty, Type::Class(tgt_name)) if self.interfaces.contains_key(tgt_name) => {
                let tgt_iface = self.interfaces.get(tgt_name).unwrap();

                // Get source structure (either class or interface)
                let (src_fields, src_methods) = if let Type::Class(src_name) = src_ty {
                    if let Some(src_class) = self.classes.get(src_name) {
                        (Some(&src_class.fields), Some(&src_class.methods))
                    } else if let Some(src_iface) = self.interfaces.get(src_name) {
                        (Some(&src_iface.fields), Some(&src_iface.methods))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };

                if let (Some(fields), Some(methods)) = (src_fields, src_methods) {
                    let fields: &HashMap<String, FieldInfo> = fields;
                    let methods: &HashMap<String, MethodInfo> = methods;
                    // Check fields
                    for (name, tgt_f) in &tgt_iface.fields {
                        if let Some(src_f) = fields.get(name) {
                            if !self.is_assignable_internal(&src_f.ty, &tgt_f.ty, history) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    // Check methods
                    for (name, tgt_m) in &tgt_iface.methods {
                        if let Some(src_m) = methods.get(name) {
                            if src_m.params.len() != tgt_m.params.len() {
                                return false;
                            }
                            for (p1, p2) in src_m.params.iter().zip(tgt_m.params.iter()) {
                                if !self.is_assignable_internal(p2, p1, history) {
                                    return false;
                                }
                            }
                            if !self.is_assignable_internal(&src_m.ret_ty, &tgt_m.ret_ty, history) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            }

            // Structural identity for classes
            (Type::Class(src_name), Type::Class(tgt_name)) => {
                if src_name == tgt_name {
                    return true;
                }

                // Nominal subtyping: check if tgt_name is in the inheritance chain of src_name
                let mut current = Some(src_name.clone());
                while let Some(curr_name) = current {
                    if curr_name == *tgt_name {
                        return true;
                    }
                    current = self.classes.get(&curr_name).and_then(|i| i.parent.clone());
                }

                false
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

    pub fn lookup_field(&self, class_name: &str, field: &str) -> Option<(FieldInfo, String, Span)> {
        let mut curr = Some(class_name.to_string());
        while let Some(name) = curr {
            if let Some(info) = self.classes.get(&name) {
                if let Some(f) = info.fields.get(field) {
                    return Some((f.clone(), info.defined_in.clone(), info.span));
                }
                curr = info.parent.clone();
            } else if let Some(info) = self.interfaces.get(&name) {
                if let Some(f) = info.fields.get(field) {
                    return Some((f.clone(), info.defined_in.clone(), info.span));
                }
                break;
            } else {
                break;
            }
        }
        None
    }

    pub fn lookup_method(
        &self,
        class_name: &str,
        method: &str,
    ) -> Option<(MethodInfo, String, Span)> {
        let mut curr = Some(class_name.to_string());
        while let Some(name) = curr {
            if let Some(info) = self.classes.get(&name) {
                if let Some(m) = info.methods.get(method) {
                    return Some((m.clone(), info.defined_in.clone(), info.span));
                }
                curr = info.parent.clone();
            } else if let Some(info) = self.interfaces.get(&name) {
                if let Some(m) = info.methods.get(method) {
                    return Some((m.clone(), info.defined_in.clone(), info.span));
                }
                break;
            } else {
                break;
            }
        }
        None
    }

    fn validate_inheritance(&mut self) {
        let class_names: Vec<String> = self.classes.keys().cloned().collect();
        for name in class_names {
            // Check for circular inheritance
            let mut visited = Vec::new();
            let mut current = Some(name.clone());
            while let Some(curr_name) = current {
                if visited.contains(&curr_name) {
                    let span = self.classes.get(&name).unwrap().span;
                    self.error(SemanticErrorKind::CircularInheritance(name.clone()), span);
                    break;
                }
                visited.push(curr_name.clone());
                current = self.classes.get(&curr_name).and_then(|i| i.parent.clone());
            }

            // Validate parent existence
            let info = self.classes.get(&name).unwrap().clone();
            if let Some(ref parent_name) = info.parent {
                if !self.classes.contains_key(parent_name) {
                    self.error(
                        SemanticErrorKind::UndefinedClass(parent_name.clone()),
                        info.span,
                    );
                    continue;
                }

                if !info.is_abstract {
                    let mut abstract_methods = HashSet::new();
                    let mut curr = info.parent.clone();
                    while let Some(pn) = curr {
                        if let Some(pinfo) = self.classes.get(&pn) {
                            for (mname, m_info) in &pinfo.methods {
                                if m_info.is_abstract {
                                    abstract_methods.insert(mname.clone());
                                }
                            }
                            curr = pinfo.parent.clone();
                        } else {
                            break;
                        }
                    }

                    for mname in abstract_methods {
                        if let Some((found_minfo, _, _)) = self.lookup_method(&name, &mname) {
                            if found_minfo.is_abstract {
                                self.error(
                                    SemanticErrorKind::ConcreteClassMissingImplementation(
                                        name.clone(),
                                        mname.clone(),
                                    ),
                                    info.span,
                                );
                            }
                        }
                    }
                }

                // Validate overrides
                for (mname, minfo) in &info.methods {
                    if minfo.is_static {
                        continue;
                    }

                    let mut overridden = None;
                    let mut curr_parent = Some(parent_name.clone());
                    while let Some(pn) = curr_parent {
                        if let Some(pinfo) = self.classes.get(&pn) {
                            if let Some(pmeta) = pinfo.methods.get(mname) {
                                if !pmeta.is_static {
                                    overridden = Some(pmeta.clone());
                                    break;
                                }
                            }
                            curr_parent = pinfo.parent.clone();
                        } else {
                            break;
                        }
                    }

                    if let Some(pmeta) = overridden {
                        if !minfo.is_override {
                            self.error(
                                SemanticErrorKind::MissingOverride(name.clone(), mname.clone()),
                                minfo.span,
                            );
                        } else {
                            // Check compatibility
                            if minfo.params.len() != pmeta.params.len() {
                                self.error(
                                    SemanticErrorKind::IncompatibleOverride(
                                        name.clone(),
                                        mname.clone(),
                                    ),
                                    minfo.span,
                                );
                            } else {
                                for (p1, p2) in minfo.params.iter().zip(pmeta.params.iter()) {
                                    if p1 != p2 {
                                        self.error(
                                            SemanticErrorKind::IncompatibleOverride(
                                                name.clone(),
                                                mname.clone(),
                                            ),
                                            minfo.span,
                                        );
                                        break;
                                    }
                                }
                                if minfo.ret_ty != pmeta.ret_ty {
                                    self.error(
                                        SemanticErrorKind::IncompatibleOverride(
                                            name.clone(),
                                            mname.clone(),
                                        ),
                                        minfo.span,
                                    );
                                }
                            }
                        }
                    } else if minfo.is_override {
                        self.error(
                            SemanticErrorKind::UnexpectedOverride(name.clone(), mname.clone()),
                            minfo.span,
                        );
                    }
                }
            }
        }
    }

    fn validate_interfaces(&mut self) {
        let class_names: Vec<String> = self.classes.keys().cloned().collect();
        for name in class_names {
            let info = self.classes.get(&name).unwrap().clone();
            for iface_name in &info.implements {
                if let Some(iface_info) = self.interfaces.get(iface_name) {
                    // Check if class structurally implements the interface
                    for (fname, f_info) in &iface_info.fields {
                        if let Some(cf_info) = info.fields.get(fname) {
                            if !self.is_assignable(&cf_info.ty, &f_info.ty) {
                                let msg = format!("Class '{}' incorrectly implements interface '{}': field '{}' has incompatible type", name, iface_name, fname);
                                self.diagnostics.push(Diagnostic::error(
                                    msg,
                                    info.span.line,
                                    info.span.column,
                                ));
                            }
                        } else {
                            let msg = format!(
                                "Class '{}' does not implement interface '{}': missing field '{}'",
                                name, iface_name, fname
                            );
                            self.diagnostics.push(Diagnostic::error(
                                msg,
                                info.span.line,
                                info.span.column,
                            ));
                        }
                    }
                    for (mname, m_info) in &iface_info.methods {
                        if let Some(cm_info) = info.methods.get(mname) {
                            // Check signature compatibility
                            let mut compatible = cm_info.params.len() == m_info.params.len();
                            if compatible {
                                for (p1, p2) in cm_info.params.iter().zip(m_info.params.iter()) {
                                    if p1 != p2 {
                                        compatible = false;
                                        break;
                                    }
                                }
                            }
                            if compatible && cm_info.ret_ty != m_info.ret_ty {
                                compatible = false;
                            }

                            if !compatible {
                                let msg = format!("Class '{}' incorrectly implements interface '{}': method '{}' has incompatible signature", name, iface_name, mname);
                                self.diagnostics.push(Diagnostic::error(
                                    msg,
                                    info.span.line,
                                    info.span.column,
                                ));
                            }
                        } else {
                            let msg = format!(
                                "Class '{}' does not implement interface '{}': missing method '{}'",
                                name, iface_name, mname
                            );
                            self.diagnostics.push(Diagnostic::error(
                                msg,
                                info.span.line,
                                info.span.column,
                            ));
                        }
                    }
                } else {
                    self.error(
                        SemanticErrorKind::UndefinedClass(iface_name.clone()),
                        info.span,
                    );
                }
            }
        }
    }
}
