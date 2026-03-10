use crate::compiler::ast::{Expr, Program, Statement, TypeExpr};
use crate::compiler::ir::builder::IrBuilder;
use crate::compiler::ir::instr::{IrFunction, IrModule, IrType, Operand};
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

struct ClassLayout {
    field_offsets: HashMap<String, u32>,
    size: u32,
}

pub struct Lowerer {
    builder: IrBuilder,
    // name -> (reg_id of pointer (alloca), optional class name, optional semantic type)
    mem_vars: HashMap<String, (u32, Option<String>, Option<Type>)>,
    class_layouts: HashMap<String, ClassLayout>,
    function_tys: HashMap<String, (Vec<Type>, Type)>, // (params, return)
    class_structures: HashMap<String, HashMap<String, Type>>, // field_name -> Type
    static_methods: HashMap<String, std::collections::HashSet<String>>,
    current_class: Option<String>,
    globals: Vec<(String, String)>, // (name, content) for strings
    last_expr_ty: Type,
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
            last_expr_ty: Type::Unknown,
        }
    }

    fn get_field_offset(&self, class_name: &str, field_name: &str) -> u32 {
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
                        let mut parser = crate::compiler::frontend::parser::Parser::new(tokens);
                        let program = parser.parse_program();

                        // Pass 1: Layouts
                        for stmt in &program.statements {
                            if let Statement::ClassDeclaration {
                                name,
                                fields,
                                methods,
                                ..
                            } = stmt
                            {
                                let mut field_offsets = HashMap::new();
                                let mut field_tys = HashMap::new();
                                let mut current_offset = 0;
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

        // Pass 1: Collect class layouts, structures and function types
        for stmt in &program.statements {
            match stmt {
                Statement::ClassDeclaration {
                    name,
                    fields,
                    methods,
                    ..
                } => {
                    let mut field_offsets = HashMap::new();
                    let mut field_tys = HashMap::new();
                    let mut current_offset = 0;
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
                Statement::FunctionDeclaration {
                    name,
                    params,
                    return_ty,
                    ..
                } => {
                    let p_tys = params
                        .iter()
                        .map(|(_, t)| self.resolve_type(t.clone()))
                        .collect();
                    let r_ty = self.resolve_type(return_ty.clone());
                    self.function_tys.insert(name.clone(), (p_tys, r_ty));
                }
                _ => {}
            }
        }

        // Pass 2: Lower functions and methods
        for stmt in program.statements {
            match stmt {
                Statement::FunctionDeclaration {
                    name,
                    params,
                    return_ty: _,
                    body,
                    is_async: _is_async,
                    span: _,
                    doc: _,
                } => {
                    let pnames = params.clone();
                    functions.push(self.lower_function(name, pnames, *body, None));
                }
                Statement::ClassDeclaration {
                    name,
                    fields: _,
                    methods,
                    constructor,
                    span: _,
                    doc: _,
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
                    }
                }
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
            let ptr_reg = self.builder.new_reg();
            self.builder
                .emit(crate::compiler::ir::instr::Instruction::Alloc(ptr_reg, 8));
            self.builder
                .emit(crate::compiler::ir::instr::Instruction::Store(
                    Operand::Parameter(i as u32),
                    Operand::Value(ptr_reg),
                    0,
                ));
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

    fn lower_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::VarDeclaration {
                name,
                ty,
                value,
                span: _,
                doc: _,
            } => {
                let mut class_name = if let Expr::New(ref cls, _, _) = value {
                    Some(cls.clone())
                } else {
                    None
                };
                let mut sem_ty = ty.as_ref().map(|t| self.resolve_type(t.clone()));

                if sem_ty.is_none() {
                    match &value {
                        Expr::StringLiteral(_, _) | Expr::Template(_, _) => {
                            sem_ty = Some(Type::String);
                        }
                        Expr::Call(name, _, _) => {
                            if let Some((_, ret_ty)) = self.function_tys.get(name) {
                                sem_ty = Some(ret_ty.clone());
                                if let Type::Class(c) = ret_ty {
                                    class_name = Some(c.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                } else if class_name.is_none() {
                    if let Some(Type::Class(c)) = &sem_ty {
                        class_name = Some(c.clone());
                    }
                }
                let size = if let Some(Type::Union(_)) = sem_ty {
                    16
                } else {
                    8
                };

                let val_op = self.lower_expr(value);
                let ptr_reg = self.builder.new_reg();
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Alloc(
                        ptr_reg, size,
                    ));

                if size == 16 {
                    // Tagged Union: [tag (8 bytes), value (8 bytes)]
                    // For now, assume it's i32 | string and the value is i32
                    // Tag 1 = i32
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Store(
                            Operand::Constant(1),
                            Operand::Value(ptr_reg),
                            0,
                        ));
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Store(
                            val_op,
                            Operand::Value(ptr_reg),
                            8,
                        ));
                } else {
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Store(
                            val_op,
                            Operand::Value(ptr_reg),
                            0,
                        ));
                }
                self.mem_vars.insert(name, (ptr_reg, class_name, sem_ty));
            }
            Statement::Expression(expr, _) => {
                self.lower_expr(expr);
            }
            Statement::Print(expr, _) => {
                let val = self.lower_expr(expr);
                if self.last_expr_ty == Type::String {
                    self.builder.call("print_str".to_string(), vec![val]);
                } else {
                    self.builder.call("print_num".to_string(), vec![val]);
                }
            }
            Statement::Block(stmts, _) => {
                for s in stmts {
                    self.lower_statement(s);
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
                span: _,
            } => {
                let cond_op = self.lower_expr(condition);
                let then_label = self.builder.new_label("then");
                let else_label = self.builder.new_label("else");
                let merge_label = self.builder.new_label("merge");

                self.builder
                    .branch(cond_op, then_label.clone(), else_label.clone());

                self.builder.set_block(then_label);
                self.lower_statement(*then_branch);
                self.builder.jump(merge_label.clone());

                self.builder.set_block(else_label);
                if let Some(eb) = else_branch {
                    self.lower_statement(*eb);
                }
                self.builder.jump(merge_label.clone());

                self.builder.set_block(merge_label);
            }
            Statement::While {
                condition,
                body,
                span: _,
            } => {
                let cond_label = self.builder.new_label("loop_cond");
                let body_label = self.builder.new_label("loop_body");
                let end_label = self.builder.new_label("loop_end");

                self.builder.jump(cond_label.clone());
                self.builder.set_block(cond_label.clone());
                let cond_op = self.lower_expr(condition);
                self.builder
                    .branch(cond_op, body_label.clone(), end_label.clone());

                self.builder.set_block(body_label);
                self.lower_statement(*body);
                self.builder.jump(cond_label);

                self.builder.set_block(end_label);
            }
            Statement::Return(expr, _) => {
                let val = self.lower_expr(expr);
                self.builder.ret(Some(val));
            }
            Statement::FunctionDeclaration { .. } | Statement::ClassDeclaration { .. } => {
                // These are handled in lower_program
            }
            Statement::TryCatch { .. } => {
                todo!("Try-catch lowering to IR is not implemented yet")
            }
            Statement::Error => {}
            Statement::Import { .. } | Statement::Export { .. } => {
                todo!("Imports/exports lowering to IR is not implemented yet")
            }
        }
    }

    fn lower_expr(&mut self, expr: Expr) -> Operand {
        match expr {
            Expr::Number(n, _) => {
                self.last_expr_ty = Type::Int32;
                Operand::Constant(n as i64)
            }
            Expr::Null(_) => {
                self.last_expr_ty = Type::Null;
                Operand::Constant(0)
            }
            Expr::Template(_, _) => {
                todo!("Template lowering")
            }
            Expr::Await(_, _) => todo!("Await in IR lower"),
            Expr::ArrayLiteral(_, _) => todo!("Array literals in IR lower"),
            Expr::Error(_) => panic!("Compiler bug: error node in IR lower"),
            Expr::StringLiteral(s, _) => {
                let name = format!("str_{}", self.globals.len());
                self.globals.push((name.clone(), s));
                self.last_expr_ty = Type::String;
                // Treat strings as pointers in IR for now
                self.builder.call(
                    "aura_get_string".to_string(),
                    vec![Operand::Constant(self.globals.len() as i64 - 1)],
                )
            }
            Expr::Variable(name, _) => {
                if let Some((ptr_reg, cls_name, ty)) = self.mem_vars.get(&name).cloned() {
                    self.last_expr_ty = ty.unwrap_or_else(|| {
                        if let Some(cls) = cls_name {
                            Type::Class(cls)
                        } else {
                            Type::Unknown
                        }
                    });
                    let dest = self.builder.new_reg();
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Load(
                            dest,
                            Operand::Value(ptr_reg),
                            0,
                        ));
                    Operand::Value(dest)
                } else if self.class_structures.contains_key(&name) {
                    self.last_expr_ty = Type::Class(name);
                    Operand::Constant(0) // Class constant is 0
                } else {
                    panic!("Undefined variable {}", name);
                }
            }
            Expr::BinaryOp(left, op, right, _) => {
                let lhs = self.lower_expr(*left);
                let lhs_ty = self.last_expr_ty.clone();
                let rhs = self.lower_expr(*right);
                // Binary ops currently assume numeric for simplicity
                self.last_expr_ty = if op == "=="
                    || op == "!="
                    || op == "<"
                    || op == "<="
                    || op == ">"
                    || op == ">="
                {
                    Type::Boolean
                } else {
                    lhs_ty
                };
                match op.as_str() {
                    "+" => self.builder.add(lhs, rhs),
                    "-" => self.builder.sub(lhs, rhs),
                    "*" => self.builder.mul(lhs, rhs),
                    "/" => self.builder.div(lhs, rhs),
                    "%" => self.builder.rem(lhs, rhs),
                    "==" => self.builder.eq(lhs, rhs),
                    "!=" => self.builder.ne(lhs, rhs),
                    "<" => self.builder.lt(lhs, rhs),
                    "<=" => self.builder.le(lhs, rhs),
                    ">" => self.builder.gt(lhs, rhs),
                    ">=" => self.builder.ge(lhs, rhs),
                    _ => panic!("Unsupported operator {}", op),
                }
            }
            Expr::Call(name, args, _) => {
                let (_, r_ty) = self
                    .function_tys
                    .get(&name)
                    .cloned()
                    .unwrap_or((vec![], Type::Unknown));
                let ir_args = args.into_iter().map(|a| self.lower_expr(a)).collect();
                self.last_expr_ty = r_ty;
                self.builder.call(name, ir_args)
            }
            Expr::Assign(name, value, _) => {
                let val_op = self.lower_expr(*value);
                let (ptr_reg, _, _) = self
                    .mem_vars
                    .get(&name)
                    .cloned()
                    .expect("Undefined variable");
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Store(
                        val_op.clone(),
                        Operand::Value(ptr_reg),
                        0,
                    ));
                val_op
            }
            Expr::New(class_name, args, _) => {
                let size = self
                    .class_layouts
                    .get(&class_name)
                    .map(|l| l.size)
                    .unwrap_or(0);
                let obj_reg = self.builder.call(
                    "aura_alloc".to_string(),
                    vec![Operand::Constant(size as i64)],
                );

                // Call constructor if exists
                let ctor_name = format!("{}_ctor", class_name);
                let mut ctor_args = vec![obj_reg.clone()];
                ctor_args.extend(args.into_iter().map(|a| self.lower_expr(a)));
                self.builder.call(ctor_name, ctor_args);

                self.last_expr_ty = Type::Class(class_name);
                obj_reg
            }
            Expr::This(_) => {
                let (ptr_reg, _, _) = self
                    .mem_vars
                    .get("this")
                    .cloned()
                    .expect("this not in scope");
                self.last_expr_ty = self
                    .current_class
                    .as_ref()
                    .map(|c| Type::Class(c.clone()))
                    .unwrap_or(Type::Unknown);
                let dest = self.builder.new_reg();
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Load(
                        dest,
                        Operand::Value(ptr_reg),
                        0,
                    ));
                Operand::Value(dest)
            }
            Expr::MemberAccess(obj, field, _) => {
                let obj_op = self.lower_expr(*obj);
                let obj_ty = self.last_expr_ty.clone();
                if let Type::Class(cls_name) = obj_ty {
                    let offset = self.get_field_offset(&cls_name, &field);
                    let field_ty = self
                        .class_structures
                        .get(&cls_name)
                        .and_then(|s| s.get(&field))
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    let dest = self.builder.new_reg();
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Load(
                            dest, obj_op, offset,
                        ));
                    self.last_expr_ty = field_ty;
                    Operand::Value(dest)
                } else {
                    panic!("Member access on non-class type {:?}", obj_ty);
                }
            }
            Expr::MemberAssign(obj, field, value, _) => {
                let obj_op = self.lower_expr(*obj);
                let obj_ty = self.last_expr_ty.clone();
                let val_op = self.lower_expr(*value);
                if let Type::Class(cls_name) = obj_ty {
                    let offset = self.get_field_offset(&cls_name, &field);
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Store(
                            val_op.clone(),
                            obj_op.clone(),
                            offset,
                        ));
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::WriteBarrier(
                            obj_op,
                            val_op.clone(),
                        ));
                    val_op
                } else {
                    panic!("Member assign on non-class type {:?}", obj_ty);
                }
            }
            Expr::MethodCall(obj, method, args, _) => {
                let obj_op = self.lower_expr(*obj);
                let obj_ty = self.last_expr_ty.clone();
                let mut is_static = false;
                let mangled_name = if let Type::Class(cls_name) = obj_ty {
                    if let Some(statics) = self.static_methods.get(&cls_name) {
                        if statics.contains(&method) {
                            is_static = true;
                        }
                    }
                    format!("{}_{}", cls_name, method)
                } else {
                    method
                };

                let mut ir_args = Vec::new();
                if !is_static {
                    ir_args.push(obj_op);
                }
                ir_args.extend(args.into_iter().map(|a| self.lower_expr(a)));
                self.last_expr_ty = Type::Unknown; // TODO: Lookup method return type
                self.builder.call(mangled_name, ir_args)
            }
            Expr::UnaryOp(op, expr, _) => {
                let val = self.lower_expr(*expr);
                if op == "-" {
                    self.builder.sub(Operand::Constant(0), val)
                } else {
                    val
                }
            }
            Expr::TypeTest(expr, ty_expr, _) => {
                let _target_ty = self.resolve_type(ty_expr);
                let val_op = self.lower_expr(*expr);
                self.last_expr_ty = Type::Boolean;

                // Let's just emit a call to aura_check_tag for now
                self.builder.call(
                    "aura_check_tag".to_string(),
                    vec![val_op, Operand::Constant(1)],
                )
            }
            Expr::Throw(expr, _) => {
                let val = self.lower_expr(*expr);
                self.builder.call("aura_throw".to_string(), vec![val]);
                Operand::Constant(0)
            }
            Expr::Index(obj, index, _) => {
                let obj_op = self.lower_expr(*obj);
                let index_op = self.lower_expr(*index);
                // Lower to intrinsic call for now
                self.builder
                    .call("__arr_get".to_string(), vec![obj_op, index_op])
            }
        }
    }

    fn resolve_type(&self, te: TypeExpr) -> Type {
        match te {
            TypeExpr::Name(n, _) => match n.as_str() {
                "i32" | "Int32" => Type::Int32,
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
            statements: vec![
                Statement::ClassDeclaration {
                    name: "Node".to_string(),
                    fields: vec![crate::compiler::ast::Field {
                        name: "next".to_string(),
                        ty: TypeExpr::Name("Node".to_string(), span),
                        value: None,
                        is_static: false,
                        span,
                        doc: None,
                    }],
                    methods: vec![],
                    constructor: None,
                    span,
                    doc: None,
                },
                Statement::FunctionDeclaration {
                    name: "set_next".to_string(),
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
