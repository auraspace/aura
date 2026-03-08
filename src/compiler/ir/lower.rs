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

    pub fn lower_program(&mut self, program: Program) -> IrModule {
        let mut functions = Vec::new();
        let mut global_stmts = Vec::new();

        // Pass 1: Collect class layouts, structures and function types
        for stmt in &program.statements {
            match stmt {
                Statement::ClassDeclaration { name, fields, .. } => {
                    let mut field_offsets = HashMap::new();
                    let mut field_tys = HashMap::new();
                    let mut current_offset = 0;
                    for f in fields {
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
                } => {
                    let pnames = params.into_iter().map(|(n, _)| n).collect();
                    functions.push(self.lower_function(name, pnames, *body, None));
                }
                Statement::ClassDeclaration {
                    name,
                    fields: _,
                    methods,
                    constructor,
                } => {
                    for m in methods {
                        let mangled_name = format!("{}_{}", name, m.name);
                        let mut pnames = vec!["this".to_string()];
                        pnames.extend(m.params.into_iter().map(|(n, _)| n));
                        functions.push(self.lower_function(
                            mangled_name,
                            pnames,
                            *m.body,
                            Some(name.clone()),
                        ));
                    }
                    if let Some(ctor) = constructor {
                        let mangled_name = format!("{}_ctor", name);
                        let mut pnames = vec!["this".to_string()];
                        pnames.extend(ctor.params.into_iter().map(|(n, _)| n));
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
                Statement::Block(global_stmts),
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
        params: Vec<String>,
        body: Statement,
        class_name: Option<String>,
    ) -> IrFunction {
        self.mem_vars.clear();
        self.current_class = class_name.clone();

        let ir_params = vec![IrType::I64; params.len()];
        for (i, param_name) in params.iter().enumerate() {
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
                None
            };
            self.mem_vars
                .insert(param_name.clone(), (ptr_reg, cls, None));
        }

        self.lower_statement(body);

        self.builder.finish_function(name, ir_params, IrType::I64)
    }

    fn lower_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::VarDeclaration { name, ty, value } => {
                let class_name = if let Expr::New(ref cls, _) = value {
                    Some(cls.clone())
                } else {
                    None
                };
                let sem_ty = ty
                    .as_ref()
                    .map(|t| self.resolve_type(t.clone()))
                    .or_else(|| {
                        if let Expr::StringLiteral(_) = value {
                            Some(Type::String)
                        } else {
                            None
                        }
                    });
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
            Statement::Expression(expr) => {
                self.lower_expr(expr);
            }
            Statement::Print(expr) => {
                let val = self.lower_expr(expr);
                if self.last_expr_ty == Type::String {
                    self.builder.call("print_str".to_string(), vec![val]);
                } else {
                    self.builder.call("print_num".to_string(), vec![val]);
                }
            }
            Statement::Block(stmts) => {
                for s in stmts {
                    self.lower_statement(s);
                }
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
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
            Statement::While { condition, body } => {
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
            Statement::Return(expr) => {
                let val = self.lower_expr(expr);
                self.builder.ret(Some(val));
            }
            Statement::Error | _ => {}
        }
    }

    fn lower_expr(&mut self, expr: Expr) -> Operand {
        match expr {
            Expr::Number(n) => {
                self.last_expr_ty = Type::Int32;
                Operand::Constant(n as i64)
            }
            Expr::StringLiteral(s) => {
                let name = format!("str_{}", self.globals.len());
                self.globals.push((name.clone(), s));
                self.last_expr_ty = Type::String;
                // Treat strings as pointers in IR for now
                self.builder.call(
                    "aura_get_string".to_string(),
                    vec![Operand::Constant(self.globals.len() as i64 - 1)],
                )
            }
            Expr::Variable(name) => {
                let (ptr_reg, cls_name, ty) = self
                    .mem_vars
                    .get(&name)
                    .cloned()
                    .expect("Undefined variable");
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
            }
            Expr::BinaryOp(left, op, right) => {
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
            Expr::Call(name, args) => {
                let (_, r_ty) = self
                    .function_tys
                    .get(&name)
                    .cloned()
                    .unwrap_or((vec![], Type::Unknown));
                let ir_args = args.into_iter().map(|a| self.lower_expr(a)).collect();
                self.last_expr_ty = r_ty;
                self.builder.call(name, ir_args)
            }
            Expr::Assign(name, value) => {
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
            Expr::New(class_name, args) => {
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
            Expr::This => {
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
            Expr::MemberAccess(obj, field) => {
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
            Expr::MemberAssign(obj, field, value) => {
                let obj_op = self.lower_expr(*obj);
                let obj_ty = self.last_expr_ty.clone();
                let val_op = self.lower_expr(*value);
                if let Type::Class(cls_name) = obj_ty {
                    let offset = self.get_field_offset(&cls_name, &field);
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Store(
                            val_op.clone(),
                            obj_op,
                            offset,
                        ));
                    val_op
                } else {
                    panic!("Member assign on non-class type {:?}", obj_ty);
                }
            }
            Expr::MethodCall(obj, method, args) => {
                let obj_op = self.lower_expr(*obj);
                let obj_ty = self.last_expr_ty.clone();
                let mangled_name = if let Type::Class(cls_name) = obj_ty {
                    format!("{}_{}", cls_name, method)
                } else {
                    method
                };

                let mut ir_args = vec![obj_op];
                ir_args.extend(args.into_iter().map(|a| self.lower_expr(a)));
                self.last_expr_ty = Type::Unknown; // TODO: Lookup method return type
                self.builder.call(mangled_name, ir_args)
            }
            Expr::TypeTest(expr, ty_expr) => {
                let _target_ty = self.resolve_type(ty_expr);
                let val_op = self.lower_expr(*expr);
                self.last_expr_ty = Type::Boolean;

                // Let's just emit a call to aura_check_tag for now
                self.builder.call(
                    "aura_check_tag".to_string(),
                    vec![val_op, Operand::Constant(1)],
                )
            }
            Expr::Error => panic!("Compiler bug: reaching error node in lowerer"),
        }
    }

    fn resolve_type(&self, te: TypeExpr) -> Type {
        match te {
            TypeExpr::Name(n) => match n.as_str() {
                "i32" => Type::Int32,
                "i64" => Type::Int64,
                "f32" => Type::Float32,
                "f64" => Type::Float64,
                "string" => Type::String,
                "boolean" => Type::Boolean,
                "void" => Type::Void,
                _ => Type::Class(n),
            },
            TypeExpr::Union(tys) => {
                Type::Union(tys.into_iter().map(|t| self.resolve_type(t)).collect())
            }
            _ => Type::Unknown,
        }
    }
}
