use crate::compiler::ast::{AccessModifier, Program, Span, TypeExpr, TypeParam};
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
    pub type_params: Vec<TypeParam>,
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
    pub parent: Option<TypeExpr>,
    pub implements: Vec<TypeExpr>,
    pub type_params: Vec<TypeParam>,
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
    pub type_params: Vec<TypeParam>,
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
    ThisInStaticMethod,
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
    pub enums: HashMap<String, HashSet<String>>,
    pub current_class: Option<String>,
    pub current_method: Option<String>,
    pub is_static_context: bool,
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
            enums: HashMap::new(),
            current_class: None,
            current_method: None,
            is_static_context: false,
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

        let t_param = TypeParam {
            name: "T".to_string(),
            constraint: None,
            span: Span::new(0, 0),
        };

        // Promise.all<T>(values: Array<T>): Promise<Array<T>>
        promise_methods.insert(
            "all".to_string(),
            MethodInfo {
                type_params: vec![t_param.clone()],
                params: vec![Type::Array(Box::new(Type::GenericParam("T".to_string())))],
                ret_ty: Type::Generic(
                    "Promise".to_string(),
                    vec![Type::Array(Box::new(Type::GenericParam("T".to_string())))],
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
                type_params: vec![t_param.clone()],
                params: vec![Type::Array(Box::new(Type::GenericParam("T".to_string())))],
                ret_ty: Type::Generic(
                    "Promise".to_string(),
                    vec![Type::Array(Box::new(Type::GenericParam("T".to_string())))],
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
                type_params: vec![t_param.clone()],
                params: vec![Type::Array(Box::new(Type::GenericParam("T".to_string())))],
                ret_ty: Type::Generic(
                    "Promise".to_string(),
                    vec![Type::GenericParam("T".to_string())],
                ),
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
                type_params: vec![t_param.clone()],
                params: vec![Type::Array(Box::new(Type::GenericParam("T".to_string())))],
                ret_ty: Type::Generic(
                    "Promise".to_string(),
                    vec![Type::GenericParam("T".to_string())],
                ),
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

        analyzer.classes.insert(
            "Promise".to_string(),
            ClassInfo {
                name: "Promise".to_string(),
                parent: None,
                implements: Vec::new(),
                type_params: Vec::new(), // TODO: Make Promise generic in stdlib
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
            SemanticErrorKind::ThisInStaticMethod => {
                "Keyword 'this' cannot be used in static methods".to_string()
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

    pub fn get_substitution_mapping(
        &self,
        type_params: &[TypeParam],
        args: &[Type],
    ) -> HashMap<String, Type> {
        let mut mapping = HashMap::new();
        for (param, arg) in type_params.iter().zip(args.iter()) {
            mapping.insert(param.name.clone(), arg.clone());
        }
        mapping
    }

    pub fn substitute(&self, ty: &Type, mapping: &HashMap<String, Type>) -> Type {
        match ty {
            Type::Class(name) => {
                if let Some(substituted) = mapping.get(name) {
                    substituted.clone()
                } else {
                    ty.clone()
                }
            }
            Type::GenericParam(name) => {
                if let Some(substituted) = mapping.get(name) {
                    substituted.clone()
                } else {
                    ty.clone()
                }
            }
            Type::Array(inner) => Type::Array(Box::new(self.substitute(inner, mapping))),
            Type::Generic(name, args) => Type::Generic(
                name.clone(),
                args.iter()
                    .map(|arg| self.substitute(arg, mapping))
                    .collect(),
            ),
            Type::Function(tparams, params, ret) => Type::Function(
                tparams.clone(),
                params.iter().map(|p| self.substitute(p, mapping)).collect(),
                Box::new(self.substitute(ret, mapping)),
            ),
            Type::Union(options) => Type::Union(
                options
                    .iter()
                    .map(|opt| self.substitute(opt, mapping))
                    .collect(),
            ),
            _ => ty.clone(),
        }
    }

    pub fn resolve_type(&mut self, te: TypeExpr) -> Type {
        match te {
            TypeExpr::Name(n, s) => match n.as_str() {
                "i32" | "Int32" | "number" | "Number" => Type::Int32,
                "i64" | "Int64" => Type::Int64,
                "f32" | "Float32" => Type::Float32,
                "f64" | "Float64" | "float" | "Float" => Type::Float64,
                "string" | "String" => Type::String,
                "boolean" | "Boolean" | "bool" => Type::Boolean,
                "void" | "Void" => Type::Void,
                "any" => {
                    self.error(SemanticErrorKind::UndefinedClass("any".to_string()), s);
                    Type::Error
                }
                "unknown" => {
                    self.error(SemanticErrorKind::UndefinedClass("unknown".to_string()), s);
                    Type::Error
                }
                _ => {
                    if let Some(sym) = self.scope.lookup(&n) {
                        if matches!(sym.ty, Type::Enum(_) | Type::GenericParam(_)) {
                            return sym.ty.clone();
                        }
                    }
                    Type::Class(n)
                }
            },
            TypeExpr::Union(tys, _) => {
                let mut resolved = Vec::new();
                for t in tys {
                    resolved.push(self.resolve_type(t));
                }
                Type::Union(resolved)
            }
            TypeExpr::Generic(name, args, _) => {
                let mut resolved_args = Vec::new();
                for t in args {
                    resolved_args.push(self.resolve_type(t));
                }
                Type::Generic(name, resolved_args)
            }
            TypeExpr::Array(base, _) => {
                let res = self.resolve_type(*base);
                Type::Array(Box::new(res))
            }
            TypeExpr::Function(tparams, params, ret, _) => {
                let mut resolved_params = Vec::new();
                for p in params {
                    resolved_params.push(self.resolve_type(p));
                }
                let resolved_ret = self.resolve_type(*ret);
                Type::Function(tparams, resolved_params, Box::new(resolved_ret))
            }
        }
    }

    pub fn is_assignable(&mut self, src: &Type, target: &Type) -> bool {
        self.is_assignable_internal(src, target, &mut Vec::new())
    }

    pub fn is_assignable_internal(
        &mut self,
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
            (Type::Error, _) | (_, Type::Error) => true,

            (s, Type::Union(options)) => options
                .iter()
                .any(|opt| self.is_assignable_internal(s, opt, history)),
            (Type::Union(options), t) => options
                .iter()
                .all(|opt| self.is_assignable_internal(opt, t, history)),

            (Type::Int32, Type::Int64) => true,
            (Type::Int32 | Type::Int64, Type::Float32 | Type::Float64) => true,
            (Type::Float32, Type::Float64) => true,

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
                let tgt_iface = self.interfaces.get(tgt_name).unwrap().clone();

                // Get source structure (either class or interface)
                let (src_fields, src_methods) =
                    if let Type::Class(src_name) | Type::Generic(src_name, _) = src_ty {
                        if let Some(src_class) = self.classes.get(src_name) {
                            (
                                Some(src_class.fields.clone()),
                                Some(src_class.methods.clone()),
                            )
                        } else if let Some(src_iface) = self.interfaces.get(src_name) {
                            (
                                Some(src_iface.fields.clone()),
                                Some(src_iface.methods.clone()),
                            )
                        } else {
                            (None, None)
                        }
                    } else {
                        (None, None)
                    };

                if let (Some(fields), Some(methods)) = (src_fields, src_methods) {
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

            // Nominal inheritance
            (src, Type::Class(tgt_name)) => {
                let mut current_ty = src.clone();
                while let Type::Class(ref name) | Type::Generic(ref name, _) = current_ty {
                    if name == tgt_name {
                        return true;
                    }

                    let class_info_opt = self.classes.get(name).cloned();
                    if let Some(class_info) = class_info_opt {
                        if let Some(parent_expr) = &class_info.parent {
                            let mut parent_ty = self.resolve_type(parent_expr.clone());
                            // If source is generic, substitute parent arguments
                            if let Type::Generic(_, args) = &current_ty {
                                let mut mapping = HashMap::new();
                                for (i, param) in class_info.type_params.iter().enumerate() {
                                    if i < args.len() {
                                        mapping.insert(param.name.clone(), args[i].clone());
                                    }
                                }
                                parent_ty = self.substitute(&parent_ty, &mapping);
                            }
                            current_ty = parent_ty;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
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

    pub fn lookup_field(
        &mut self,
        class_name: &str,
        field: &str,
    ) -> Option<(FieldInfo, String, Span)> {
        self.lookup_field_recursive(class_name, field, &HashMap::new())
    }

    pub fn lookup_field_with_mapping(
        &mut self,
        class_name: &str,
        field: &str,
        mapping: &HashMap<String, Type>,
    ) -> Option<(FieldInfo, String, Span)> {
        self.lookup_field_recursive(class_name, field, mapping)
    }

    fn lookup_field_recursive(
        &mut self,
        class_name: &str,
        field: &str,
        current_mapping: &HashMap<String, Type>,
    ) -> Option<(FieldInfo, String, Span)> {
        if let Some(info) = self.classes.get(class_name).cloned() {
            if let Some(f) = info.fields.get(field) {
                let mut substituted_f = f.clone();
                if !current_mapping.is_empty() {
                    substituted_f.ty = self.substitute(&substituted_f.ty, current_mapping);
                }
                return Some((substituted_f, info.defined_in.clone(), info.span));
            }
            if let Some(ref parent_expr) = info.parent {
                match parent_expr {
                    crate::compiler::ast::TypeExpr::Name(pn, _) => {
                        return self.lookup_field_recursive(pn, field, current_mapping);
                    }
                    crate::compiler::ast::TypeExpr::Generic(pn, args, _) => {
                        let mut resolved_args = Vec::new();
                        for a in args {
                            let t = self.resolve_type(a.clone());
                            if current_mapping.is_empty() {
                                resolved_args.push(t);
                            } else {
                                resolved_args.push(self.substitute(&t, current_mapping));
                            }
                        }
                        if let Some(pinfo) = self.classes.get(pn) {
                            let mut next_mapping = HashMap::new();
                            for (param, arg) in pinfo.type_params.iter().zip(resolved_args.iter()) {
                                next_mapping.insert(param.name.clone(), arg.clone());
                            }
                            return self.lookup_field_recursive(pn, field, &next_mapping);
                        }
                    }
                    _ => {}
                }
            }
        } else if let Some(info) = self.interfaces.get(class_name).cloned() {
            if let Some(f) = info.fields.get(field) {
                let mut substituted_f = f.clone();
                if !current_mapping.is_empty() {
                    substituted_f.ty = self.substitute(&substituted_f.ty, current_mapping);
                }
                return Some((substituted_f, info.defined_in.clone(), info.span));
            }
        }
        None
    }

    pub fn lookup_method(
        &mut self,
        class_name: &str,
        method: &str,
    ) -> Option<(MethodInfo, String, Span)> {
        self.lookup_method_recursive(class_name, method, &HashMap::new())
    }

    pub fn lookup_method_with_mapping(
        &mut self,
        class_name: &str,
        method: &str,
        mapping: &HashMap<String, Type>,
    ) -> Option<(MethodInfo, String, Span)> {
        self.lookup_method_recursive(class_name, method, mapping)
    }

    fn lookup_method_recursive(
        &mut self,
        class_name: &str,
        method: &str,
        current_mapping: &HashMap<String, Type>,
    ) -> Option<(MethodInfo, String, Span)> {
        if let Some(info) = self.classes.get(class_name).cloned() {
            if let Some(m) = info.methods.get(method) {
                let mut substituted_m = m.clone();
                if !current_mapping.is_empty() {
                    substituted_m.params = substituted_m
                        .params
                        .iter()
                        .map(|p| self.substitute(p, current_mapping))
                        .collect();
                    substituted_m.ret_ty = self.substitute(&substituted_m.ret_ty, current_mapping);
                }
                return Some((substituted_m, info.defined_in.clone(), info.span));
            }
            if let Some(ref parent_expr) = info.parent {
                match parent_expr {
                    crate::compiler::ast::TypeExpr::Name(pn, _) => {
                        return self.lookup_method_recursive(pn, method, current_mapping);
                    }
                    crate::compiler::ast::TypeExpr::Generic(pn, args, _) => {
                        let mut resolved_args = Vec::new();
                        for a in args {
                            let t = self.resolve_type(a.clone());
                            if current_mapping.is_empty() {
                                resolved_args.push(t);
                            } else {
                                resolved_args.push(self.substitute(&t, current_mapping));
                            }
                        }
                        if let Some(pinfo) = self.classes.get(pn) {
                            let mut next_mapping = HashMap::new();
                            for (param, arg) in pinfo.type_params.iter().zip(resolved_args.iter()) {
                                next_mapping.insert(param.name.clone(), arg.clone());
                            }
                            return self.lookup_method_recursive(pn, method, &next_mapping);
                        }
                    }
                    _ => {}
                }
            }
        } else if let Some(info) = self.interfaces.get(class_name).cloned() {
            if let Some(m) = info.methods.get(method) {
                let mut substituted_m = m.clone();
                if !current_mapping.is_empty() {
                    substituted_m.params = substituted_m
                        .params
                        .iter()
                        .map(|p| self.substitute(p, current_mapping))
                        .collect();
                    substituted_m.ret_ty = self.substitute(&substituted_m.ret_ty, current_mapping);
                }
                return Some((substituted_m, info.defined_in.clone(), info.span));
            }
        }
        None
    }

    pub fn lookup_method_for_type(
        &mut self,
        ty: &Type,
        method: &str,
    ) -> Option<(MethodInfo, String, Span)> {
        match ty {
            Type::Class(name) => self.lookup_method(name, method),
            Type::ClassType(name) => self.lookup_method(name, method),
            Type::Generic(name, args) => {
                if let Some(info) = self.classes.get(name).cloned() {
                    let mut mapping = HashMap::new();
                    for (param, arg) in info.type_params.iter().zip(args.iter()) {
                        mapping.insert(param.name.clone(), arg.clone());
                    }
                    self.lookup_method_with_mapping(name, method, &mapping)
                } else if let Some(info) = self.interfaces.get(name).cloned() {
                    let mut mapping = HashMap::new();
                    for (param, arg) in info.type_params.iter().zip(args.iter()) {
                        mapping.insert(param.name.clone(), arg.clone());
                    }
                    self.lookup_method_with_mapping(name, method, &mapping)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn lookup_field_for_type(
        &mut self,
        ty: &Type,
        field: &str,
    ) -> Option<(FieldInfo, String, Span)> {
        match ty {
            Type::Class(name) => self.lookup_field(name, field),
            Type::ClassType(name) => self.lookup_field(name, field),
            Type::Generic(name, args) => {
                if let Some(info) = self.classes.get(name).cloned() {
                    let mut mapping = HashMap::new();
                    for (param, arg) in info.type_params.iter().zip(args.iter()) {
                        mapping.insert(param.name.clone(), arg.clone());
                    }
                    self.lookup_field_with_mapping(name, field, &mapping)
                } else if let Some(info) = self.interfaces.get(name).cloned() {
                    let mut mapping = HashMap::new();
                    for (param, arg) in info.type_params.iter().zip(args.iter()) {
                        mapping.insert(param.name.clone(), arg.clone());
                    }
                    self.lookup_field_with_mapping(name, field, &mapping)
                } else {
                    None
                }
            }
            _ => None,
        }
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
                current = self.classes.get(&curr_name).and_then(|i| {
                    i.parent.as_ref().and_then(|p| match p {
                        crate::compiler::ast::TypeExpr::Name(n, _) => Some(n.clone()),
                        crate::compiler::ast::TypeExpr::Generic(n, _, _) => Some(n.clone()),
                        _ => None,
                    })
                });
            }

            // Validate parent existence
            let info = self.classes.get(&name).unwrap().clone();
            if let Some(ref parent_expr) = info.parent {
                let parent_name_opt = match parent_expr {
                    crate::compiler::ast::TypeExpr::Name(n, _) => Some(n.clone()),
                    crate::compiler::ast::TypeExpr::Generic(n, _, _) => Some(n.clone()),
                    _ => None,
                };

                if let Some(ref pn) = parent_name_opt {
                    if !self.classes.contains_key(pn) {
                        self.error(SemanticErrorKind::UndefinedClass(pn.clone()), info.span);
                        continue;
                    }
                } else {
                    continue;
                }

                if !info.is_abstract {
                    let mut abstract_methods = HashSet::new();
                    let mut curr = info.parent.as_ref().and_then(|p| match p {
                        crate::compiler::ast::TypeExpr::Name(n, _) => Some(n.clone()),
                        crate::compiler::ast::TypeExpr::Generic(n, _, _) => Some(n.clone()),
                        _ => None,
                    });
                    while let Some(pn) = curr {
                        if let Some(pinfo) = self.classes.get(&pn) {
                            for (mname, m_info) in &pinfo.methods {
                                if m_info.is_abstract {
                                    abstract_methods.insert(mname.clone());
                                }
                            }
                            curr = pinfo.parent.as_ref().and_then(|p| match p {
                                crate::compiler::ast::TypeExpr::Name(n, _) => Some(n.clone()),
                                crate::compiler::ast::TypeExpr::Generic(n, _, _) => Some(n.clone()),
                                _ => None,
                            });
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
                    let mut curr_parent = parent_name_opt.clone();
                    while let Some(pn) = curr_parent {
                        if let Some(pinfo) = self.classes.get(&pn) {
                            if let Some(pmeta) = pinfo.methods.get(mname) {
                                if !pmeta.is_static {
                                    overridden = Some(pmeta.clone());
                                    break;
                                }
                            }
                            curr_parent = pinfo.parent.as_ref().and_then(|p| match p {
                                crate::compiler::ast::TypeExpr::Name(n, _) => Some(n.clone()),
                                crate::compiler::ast::TypeExpr::Generic(n, _, _) => Some(n.clone()),
                                _ => None,
                            });
                        } else {
                            break;
                        }
                    }

                    if let Some(ref pmeta) = overridden {
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
                    }
                    if minfo.is_override && overridden.is_none() {
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
            for iface_expr in &info.implements {
                let iface_name = match iface_expr {
                    crate::compiler::ast::TypeExpr::Name(n, _) => n.clone(),
                    crate::compiler::ast::TypeExpr::Generic(n, _, _) => n.clone(),
                    _ => continue,
                };
                let iface_info_opt = self.interfaces.get(&iface_name).cloned();
                if let Some(iface_info) = iface_info_opt {
                    let mut type_args = Vec::new();
                    if let crate::compiler::ast::TypeExpr::Generic(_, args, _) = iface_expr {
                        for arg_expr in args {
                            type_args.push(self.resolve_type(arg_expr.clone()));
                        }
                    }
                    let mapping =
                        self.get_substitution_mapping(&iface_info.type_params, &type_args);

                    // Check if class structurally implements the interface
                    for (fname, f_info) in &iface_info.fields {
                        let expected_ty = self.substitute(&f_info.ty, &mapping);
                        if let Some(cf_info) = info.fields.get(fname) {
                            if !self.is_assignable(&cf_info.ty, &expected_ty) {
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
                                    let expected_p2 = self.substitute(p2, &mapping);
                                    if p1 != &expected_p2 {
                                        compatible = false;
                                        break;
                                    }
                                }
                            }
                            if compatible {
                                let expected_ret = self.substitute(&m_info.ret_ty, &mapping);
                                if cm_info.ret_ty != expected_ret {
                                    compatible = false;
                                }
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

    pub fn infer_type_args(
        &self,
        _type_params: &[TypeParam],
        param_tys: &[Type],
        arg_tys: &[Type],
    ) -> HashMap<String, Type> {
        let mut mapping = HashMap::new();
        for (param, arg) in param_tys.iter().zip(arg_tys.iter()) {
            self.infer_recursive(param, arg, &mut mapping);
        }
        mapping
    }

    fn infer_recursive(&self, param: &Type, arg: &Type, mapping: &mut HashMap<String, Type>) {
        match (param, arg) {
            (Type::GenericParam(name), _) => {
                mapping.insert(name.clone(), arg.clone());
            }
            (Type::Array(p_inner), Type::Array(a_inner)) => {
                self.infer_recursive(p_inner, a_inner, mapping);
            }
            (Type::Generic(p_name, p_args), Type::Generic(a_name, a_args)) if p_name == a_name => {
                for (p, a) in p_args.iter().zip(a_args.iter()) {
                    self.infer_recursive(p, a, mapping);
                }
            }
            _ => {}
        }
    }
}
