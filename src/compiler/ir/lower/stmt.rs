use super::Lowerer;
use crate::compiler::ast::{Expr, Statement};
use crate::compiler::ir::instr::Operand;
use crate::compiler::sema::ty::Type;

impl Lowerer {
    pub fn lower_statement(&mut self, stmt: Statement) {
        match stmt {
            Statement::Enum(_) => {} // Enums are lowered into constants (in the environment/checker), IR doesn't need to do anything here except maybe register constants, but we'll do that in SEMA.
            Statement::VarDeclaration {
                name,
                name_span: _,
                ty,
                value,
                is_const: _,
                span: _,
                doc: _,
            } => {
                let mut class_name = if let Expr::New(ref cls, _, _, _, _) = value {
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
                        Expr::Call(name, _, _, _, _) => {
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

                let mut val_op = self.lower_expr(value);
                if let Some(target_ty) = &sem_ty {
                    if target_ty.is_float() && self.last_expr_ty.is_integer() {
                        val_op = self.builder.itof(val_op);
                    }
                }
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
                let ty = self.last_expr_ty.clone();
                if ty.is_float() {
                    self.builder.fcall("print_float".to_string(), vec![val]);
                } else if ty == Type::String {
                    self.builder.call("print_str".to_string(), vec![val]);
                } else if ty == Type::Boolean {
                    self.builder.call("print_bool".to_string(), vec![val]);
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
            Statement::Import { .. } | Statement::Export { .. } => {}
            Statement::Comment(_, _) | Statement::RegularBlockComment(_, _) => {}
            Statement::Interface(_) => {}
        }
    }
}
