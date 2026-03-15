use crate::compiler::ast::{Expr, Program, Statement, TypeExpr};
use crate::compiler::ir::builder::IrBuilder;
use crate::compiler::ir::instr::{IrFunction, IrModule, IrType, Operand};
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

mod expr;
mod stmt;

pub(crate) struct ClassLayout {
    pub(crate) field_offsets: HashMap<String, u32>,
    pub(crate) size: u32,
    pub(crate) vtable_index: HashMap<String, u32>,
}

pub struct Lowerer {
    pub(crate) builder: IrBuilder,
    // name -> (reg_id of pointer (alloca), optional class name, optional semantic type)
    pub(crate) mem_vars: HashMap<String, (u32, Option<String>, Option<Type>)>,
    pub(crate) class_layouts: HashMap<String, ClassLayout>,
    pub(crate) function_tys: HashMap<String, (Vec<Type>, Type)>, // (params, return)
    pub(crate) class_structures: HashMap<String, HashMap<String, Type>>, // field_name -> Type
    pub(crate) static_methods: HashMap<String, std::collections::HashSet<String>>,
    pub(crate) current_class: Option<String>,
    pub(crate) globals: Vec<(String, String)>, // (name, content) for strings
    pub(crate) vtables: HashMap<String, Vec<String>>, // class_name -> list of mangled method names
    pub(crate) parent_classes: HashMap<String, String>, // subclass -> parent
    pub(crate) last_expr_ty: Type,
}

impl Lowerer {
    pub fn new() -> Self {
        Self {
            builder: IrBuilder::new(),
            mem_vars: HashMap::new(),
            class_layouts: HashMap::new(),
            function_tys: HashMap::new(),
            class_structures: HashMap::new(),
            static_methods: HashMap::new(),
            current_class: None,
            globals: Vec::new(),
            vtables: HashMap::new(),
            parent_classes: HashMap::new(),
            last_expr_ty: Type::Unknown,
        }
    }

    pub(crate) fn get_field_offset(&self, class_name: &str, field_name: &str) -> u32 {
        self.class_layouts
            .get(class_name)
            .and_then(|l| l.field_offsets.get(field_name))
            .cloned()
            .unwrap_or(0)
    }

    pub fn load_stdlib(&mut self) -> Vec<IrFunction> {
        let mut functions = Vec::new();
        let stdlib_path = "stdlib/std";
        if let Ok(entries) = std::fs::read_dir(stdlib_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("aura") {
                    if let Ok(source) = std::fs::read_to_string(&path) {
                        let mut lexer = crate::compiler::frontend::lexer::Lexer::new(&source);
                        let tokens = lexer.lex_all();
                        let mut parser = crate::compiler::frontend::parser::Parser::new(
                            tokens,
                            "ir_lowering".to_string(),
                        );
                        let program = parser.parse_program();

                        // Pass 1: Layouts
                        for stmt in &program.statements {
                            if let Statement::ClassDeclaration {
                                name,
                                fields,
                                methods,
                                extends: _,
                                implements: _,
                                ..
                            } = stmt
                            {
                                let mut field_offsets = HashMap::new();
                                let mut field_tys = HashMap::new();
                                let mut current_offset = 8;
                                for f in fields.iter() {
                                    field_offsets.insert(f.name.clone(), current_offset);
                                    let ty = self.resolve_type(f.ty.clone());
                                    field_tys.insert(f.name.clone(), ty);
                                    current_offset += 8;
                                }
                                self.class_layouts.insert(
                                    name.clone(),
                                    ClassLayout {
                                        field_offsets,
                                        size: current_offset,
                                        vtable_index: HashMap::new(), // Pass 1 doesn't have vtable yet
                                    },
                                );
                                self.class_structures.insert(name.clone(), field_tys);

                                let mut statics = std::collections::HashSet::new();
                                for m in methods.iter() {
                                    if m.is_static {
                                        statics.insert(m.name.clone());
                                    }
                                }
                                self.static_methods.insert(name.clone(), statics);
                            }
                        }

                        // Pass 2: Methods
                        for stmt in program.statements {
                            if let Statement::ClassDeclaration {
                                name,
                                methods,
                                constructor,
                                extends: _,
                                implements: _,
                                ..
                            } = stmt
                            {
                                for m in methods {
                                    let mangled_name = format!("{}_{}", name, m.name);
                                    let mut pnames = Vec::new();
                                    if !m.is_static {
                                        let span =
                                            crate::compiler::ast::Span { line: 0, column: 0 };
                                        pnames.push((
                                            "this".to_string(),
                                            TypeExpr::Name(name.clone(), span),
                                        ));
                                    }
                                    pnames.extend(m.params.clone().into_iter());
                                    functions.push(self.lower_function(
                                        mangled_name,
                                        pnames,
                                        *m.body,
                                        if m.is_static {
                                            None
                                        } else {
                                            Some(name.clone())
                                        },
                                    ));
                                }
                                if let Some(ctor) = constructor {
                                    let mangled_name = format!("{}_ctor", name);
                                    let span = crate::compiler::ast::Span { line: 0, column: 0 };
                                    let mut pnames = vec![(
                                        "this".to_string(),
                                        TypeExpr::Name(name.clone(), span),
                                    )];
                                    pnames.extend(ctor.params.clone().into_iter());
                                    functions.push(self.lower_function(
                                        mangled_name,
                                        pnames,
                                        *ctor.body,
                                        Some(name.clone()),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        functions
    }

    pub fn lower_program(&mut self, program: Program) -> IrModule {
        let mut functions = self.load_stdlib();
        let mut global_stmts = Vec::new();

        // Pass 0: Collect all class names and parents
        for stmt in &program.statements {
            if let Statement::ClassDeclaration { name, extends, .. } = stmt {
                if let Some(parent) = extends {
                    self.parent_classes.insert(name.clone(), parent.clone());
                }
            }
            if let Statement::Interface(_decl) = stmt {
                // Collect interface info if needed for IR lowering (currently not used)
            }
        }

        // Pass 1: Collect class layouts, structures and function types
        // Need to process parents before children for layouts
        let mut pending_classes: Vec<&Statement> = program.statements.iter().collect();
        let mut processed_classes = std::collections::HashSet::new();

        while !pending_classes.is_empty() {
            let mut made_progress = false;
            let mut i = 0;
            while i < pending_classes.len() {
                if let Statement::ClassDeclaration {
                    name,
                    fields,
                    methods,
                    extends,
                    implements: _,
                    ..
                } = pending_classes[i]
                {
                    let can_process = match extends {
                        Some(p) => {
                            processed_classes.contains(p) || self.class_layouts.contains_key(p)
                        }
                        None => true,
                    };

                    if can_process {
                        let mut field_offsets = HashMap::new();
                        let mut field_tys = HashMap::new();
                        let mut current_offset = 8; // Offset 0 is VTable pointer
                        let mut vtable_index = HashMap::new();
                        let mut vtable_methods = Vec::new();

                        if let Some(parent_name) = extends {
                            if let Some(parent_layout) = self.class_layouts.get(parent_name) {
                                field_offsets = parent_layout.field_offsets.clone();
                                current_offset = parent_layout.size;
                                vtable_index = parent_layout.vtable_index.clone();
                                vtable_methods =
                                    self.vtables.get(parent_name).cloned().unwrap_or_default();
                            }
                        }

                        for f in fields.iter() {
                            if !field_offsets.contains_key(&f.name) {
                                field_offsets.insert(f.name.clone(), current_offset);
                                current_offset += 8;
                            }
                            let ty = self.resolve_type(f.ty.clone());
                            field_tys.insert(f.name.clone(), ty);
                        }

                        let mut statics = std::collections::HashSet::new();
                        for m in methods.iter() {
                            let m_name = if m.is_static {
                                format!("{}_{}", name, m.name)
                            } else {
                                format!("{}_{}", name, m.name)
                            };
                            let mut p_tys: Vec<Type> = Vec::new();
                            if !m.is_static {
                                p_tys.push(Type::Class(name.clone()));
                            }
                            p_tys
                                .extend(m.params.iter().map(|(_, t)| self.resolve_type(t.clone())));
                            let r_ty = self.resolve_type(m.return_ty.clone());
                            self.function_tys.insert(m_name, (p_tys, r_ty));

                            if !m.is_static {
                                if let Some(idx) = vtable_index.get(&m.name) {
                                    // Override
                                    vtable_methods[*idx as usize] = format!("{}_{}", name, m.name);
                                } else {
                                    // New virtual method
                                    vtable_index
                                        .insert(m.name.clone(), vtable_methods.len() as u32);
                                    vtable_methods.push(format!("{}_{}", name, m.name));
                                }
                            } else {
                                statics.insert(m.name.clone());
                            }
                        }

                        self.class_layouts.insert(
                            name.clone(),
                            ClassLayout {
                                field_offsets,
                                size: current_offset,
                                vtable_index,
                            },
                        );
                        self.class_structures.insert(name.clone(), field_tys);
                        self.vtables.insert(name.clone(), vtable_methods);
                        self.static_methods.insert(name.clone(), statics);

                        processed_classes.insert(name.clone());
                        pending_classes.remove(i);
                        made_progress = true;
                    } else {
                        i += 1;
                    }
                } else {
                    if let Statement::FunctionDeclaration {
                        name,
                        params,
                        return_ty,
                        ..
                    } = pending_classes[i]
                    {
                        let p_tys = params
                            .iter()
                            .map(|(_, t)| self.resolve_type(t.clone()))
                            .collect();
                        let r_ty = self.resolve_type(return_ty.clone());
                        self.function_tys.insert(name.clone(), (p_tys, r_ty));
                    }
                    if !matches!(pending_classes[i], Statement::ClassDeclaration { .. }) {
                        pending_classes.remove(i);
                    } else {
                        i += 1;
                    }
                }
            }
            if !made_progress && !pending_classes.is_empty() {
                // Break circular dependency if any (should be caught by sema)
                break;
            }
        }

        // Pass 2: Lower functions and methods
        for stmt in program.statements {
            match stmt {
                Statement::FunctionDeclaration {
                    name, params, body, ..
                } => {
                    let pnames = params.clone();
                    functions.push(self.lower_function(name, pnames, *body, None));
                }
                Statement::ClassDeclaration {
                    name,
                    methods,
                    constructor,
                    extends,
                    ..
                } => {
                    for m in methods {
                        let mangled_name = format!("{}_{}", name, m.name);
                        let mut pnames = Vec::new();
                        if !m.is_static {
                            let span = crate::compiler::ast::Span { line: 0, column: 0 };
                            pnames.push(("this".to_string(), TypeExpr::Name(name.clone(), span)));
                        }
                        pnames.extend(m.params.clone().into_iter());
                        functions.push(self.lower_function(
                            mangled_name,
                            pnames,
                            *m.body,
                            if m.is_static {
                                None
                            } else {
                                Some(name.clone())
                            },
                        ));
                    }
                    if let Some(ctor) = constructor {
                        let mangled_name = format!("{}_ctor", name);
                        let span = crate::compiler::ast::Span { line: 0, column: 0 };
                        let mut pnames =
                            vec![("this".to_string(), TypeExpr::Name(name.clone(), span))];
                        pnames.extend(ctor.params.clone().into_iter());
                        functions.push(self.lower_function(
                            mangled_name,
                            pnames,
                            *ctor.body,
                            Some(name.clone()),
                        ));
                    } else {
                        // Generate default constructor
                        let mangled_name = format!("{}_ctor", name);
                        let span = crate::compiler::ast::Span { line: 0, column: 0 };
                        let pnames = vec![("this".to_string(), TypeExpr::Name(name.clone(), span))];

                        // Default body: call super() if it exists
                        let mut body_stmts = Vec::new();
                        if let Some(_parent) = extends {
                            body_stmts
                                .push(Statement::Expression(Expr::SuperCall(vec![], span), span));
                        }

                        functions.push(self.lower_function(
                            mangled_name,
                            pnames,
                            Statement::Block(body_stmts, span),
                            Some(name.clone()),
                        ));
                    }
                }
                Statement::Interface(_) => {}
                _ => global_stmts.push(stmt),
            }
        }

        // Handle global statements
        if !global_stmts.is_empty() {
            functions.push(self.lower_function(
                "main_aura".to_string(),
                vec![],
                Statement::Block(
                    global_stmts,
                    crate::compiler::ast::Span { line: 0, column: 0 },
                ),
                None,
            ));
        }

        IrModule {
            functions,
            globals: self.globals.clone(),
            vtables: self.vtables.clone(),
        }
    }

    fn lower_function(
        &mut self,
        name: String,
        params: Vec<(String, TypeExpr)>,
        body: Statement,
        class_name: Option<String>,
    ) -> IrFunction {
        self.mem_vars.clear();
        self.current_class = class_name.clone();

        let ir_params = vec![IrType::I64; params.len()];
        for (i, (param_name, ty_expr)) in params.iter().enumerate() {
            let ptr_reg_op = self.builder.salloc(8);
            let ptr_reg = match ptr_reg_op {
                Operand::Value(v) => v,
                _ => unreachable!(),
            };
            self.builder
                .store(Operand::Parameter(i as u32), ptr_reg_op, 0);
            let cls = if param_name == "this" {
                class_name.clone()
            } else {
                if let Type::Class(c) = self.resolve_type(ty_expr.clone()) {
                    Some(c)
                } else {
                    None
                }
            };
            let sem_ty = self.resolve_type(ty_expr.clone());
            self.mem_vars
                .insert(param_name.clone(), (ptr_reg, cls, Some(sem_ty)));
        }

        self.lower_statement(body);

        self.builder.finish_function(name, ir_params, IrType::I64)
    }

    pub(crate) fn resolve_type(&self, te: TypeExpr) -> Type {
        match te {
            TypeExpr::Name(n, _) => match n.as_str() {
                "i32" | "Int32" | "number" | "Number" => Type::Int32,
                "i64" | "Int64" => Type::Int64,
                "f32" | "Float32" => Type::Float32,
                "f64" | "Float64" => Type::Float64,
                "string" | "String" => Type::String,
                "boolean" | "Boolean" => Type::Boolean,
                "void" | "Void" => Type::Void,
                _ => Type::Class(n),
            },
            TypeExpr::Union(tys, _) => {
                Type::Union(tys.into_iter().map(|t| self.resolve_type(t)).collect())
            }
            _ => Type::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::ast::{Expr, Span, Statement, TypeExpr};

    #[test]
    fn test_lower_member_assign_write_barrier() {
        let span = Span { line: 1, column: 1 };
        let mut lowerer = Lowerer::new();

        let program = Program {
            file_path: "test".to_string(),
            statements: vec![
                Statement::ClassDeclaration {
                    name: "Node".to_string(),
                    name_span: span,
                    fields: vec![crate::compiler::ast::Field {
                        name: "next".to_string(),
                        name_span: span,
                        ty: TypeExpr::Name("Node".to_string(), span),
                        value: None,
                        is_static: false,
                        is_readonly: false,
                        access: crate::compiler::ast::AccessModifier::Public,
                        span,
                        doc: None,
                    }],
                    methods: vec![],
                    constructor: None,
                    extends: None,
                    implements: vec![],
                    span,
                    doc: None,
                },
                Statement::FunctionDeclaration {
                    name: "set_next".to_string(),
                    name_span: span,
                    params: vec![
                        ("n1".to_string(), TypeExpr::Name("Node".to_string(), span)),
                        ("n2".to_string(), TypeExpr::Name("Node".to_string(), span)),
                    ],
                    return_ty: TypeExpr::Name("void".to_string(), span),
                    body: Box::new(Statement::Block(
                        vec![Statement::Expression(
                            Expr::MemberAssign(
                                Box::new(Expr::Variable("n1".to_string(), span)),
                                "next".to_string(),
                                Box::new(Expr::Variable("n2".to_string(), span)),
                                span,
                                span,
                            ),
                            span,
                        )],
                        span,
                    )),
                    is_async: false,
                    span,
                    doc: None,
                },
            ],
        };

        let module = lowerer.lower_program(program);

        // Find set_next function
        let func = module
            .functions
            .iter()
            .find(|f| f.name == "set_next")
            .expect("Function set_next not found");

        let mut found_store = false;
        let mut found_barrier = false;

        for block in &func.blocks {
            for instr in &block.instructions {
                match instr {
                    crate::compiler::ir::instr::Instruction::Store(_, _, _) => {
                        found_store = true;
                    }
                    crate::compiler::ir::instr::Instruction::WriteBarrier(_, _) => {
                        found_barrier = true;
                    }
                    _ => {}
                }
            }
        }

        assert!(
            found_store,
            "Should emit Store instruction for MemberAssign"
        );
        assert!(
            found_barrier,
            "Should emit WriteBarrier instruction right after Store for GC tracking"
        );
    }
}
