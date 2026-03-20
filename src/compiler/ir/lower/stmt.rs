use super::Lowerer;
use crate::compiler::ast::{Expr, Statement};
use crate::compiler::ir::instr::Operand;
use crate::compiler::sema::ty::Type;

impl Lowerer {
    pub fn lower_statement(&mut self, stmt: Statement) {
        match stmt {
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
                    let tag = self.last_expr_ty.tag();
                    self.builder
                        .emit(crate::compiler::ir::instr::Instruction::Store(
                            Operand::Constant(tag),
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
                let val = self.lower_expr(expr.clone());
                let ty = self.last_expr_ty.clone();
                if ty.is_float() {
                    self.builder.fcall("print_float".to_string(), vec![val]);
                } else if ty == Type::String {
                    self.builder.call("print_str".to_string(), vec![val]);
                } else if ty == Type::Boolean {
                    self.builder.call("print_bool".to_string(), vec![val]);
                } else if let Type::Union(_) = ty {
                    // Call a generic print_union that takes a pointer to the 16-byte slot
                    // We need a way to pass the address of the variable if it's a variable,
                    // or allocate a temp slot for expressions.
                    if let Operand::Value(_) = val {
                        // If it's already a value (loaded), we might need to repack it?
                        // Actually, print for unions should probably take the tag and value separately
                        // but for simplicity, let's assume we can pass the tag and value as two args.
                        // But IR Instruction::Call only takes Vec<Operand>.
                        // Let's implement aura_print_union(int64_t tag, int64_t val)

                        // We need to load the tag and value.
                        // If 'val' is just the value (from lower_expr), we don't have the tag here!
                        // This is why lower_expr for a Union should probably return the tag.
                        // But it can only return one Operand.

                        // Re-thinking: Statement::Print should handle Union by loading tag/val.
                        if let Expr::Variable(name, _) = &expr {
                            if let Some((ptr_reg, _, _)) = self.mem_vars.get(name) {
                                let tag_op = self.builder.load(Operand::Value(*ptr_reg), 0);
                                let val_op = self.builder.load(Operand::Value(*ptr_reg), 8);
                                self.builder
                                    .call("aura_print_union".to_string(), vec![tag_op, val_op]);
                            }
                        } else {
                            // For non-variable unions (e.g. from a function call),
                            // lower_expr should have returned the value, but where is the tag?
                            // This is a deeper problem. For now, let's fix variables.
                            self.builder.call("print_num".to_string(), vec![val]);
                        }
                    } else {
                        self.builder.call("print_num".to_string(), vec![val]);
                    }
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
            Statement::For {
                initializer,
                condition,
                increment,
                body,
                span: _,
            } => {
                if let Some(init) = initializer {
                    self.lower_statement(*init);
                }
                let cond_label = self.builder.new_label("for_cond");
                let body_label = self.builder.new_label("for_body");
                let inc_label = self.builder.new_label("for_inc");
                let end_label = self.builder.new_label("for_end");

                self.builder.jump(cond_label.clone());
                self.builder.set_block(cond_label.clone());

                let cond_op = if let Some(cond) = condition {
                    self.lower_expr(cond)
                } else {
                    Operand::Constant(1) // Infinite loop if no condition
                };

                self.builder
                    .branch(cond_op, body_label.clone(), end_label.clone());

                self.builder.set_block(body_label);
                self.lower_statement(*body);
                self.builder.jump(inc_label.clone());

                self.builder.set_block(inc_label);
                if let Some(inc) = increment {
                    self.lower_expr(inc);
                }
                self.builder.jump(cond_label);

                self.builder.set_block(end_label);
            }
            Statement::ForOf {
                variable,
                variable_span: _,
                is_const: _,
                iterable,
                body,
                span: _,
            } => {
                let iterable_op = self.lower_expr(iterable);
                let iterable_ty = self.last_expr_ty.clone(); // Type from checker
                let element_ty = match iterable_ty {
                    Type::Array(inner) => *inner,
                    _ => Type::Error,
                };
                let class_name = if let Type::Class(c) = &element_ty {
                    Some(c.clone())
                } else {
                    None
                };

                // Hidden index variable
                let i_reg = self.builder.new_reg();
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Alloc(i_reg, 8));
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Store(
                        Operand::Constant(0),
                        Operand::Value(i_reg),
                        0,
                    ));

                // Get length
                let len_op = self
                    .builder
                    .call("aura_array_len".to_string(), vec![iterable_op.clone()]);

                let cond_label = self.builder.new_label("for_of_cond");
                let body_label = self.builder.new_label("for_of_body");
                let end_label = self.builder.new_label("for_of_end");

                self.builder.jump(cond_label.clone());
                self.builder.set_block(cond_label.clone());

                let i_val = self.builder.load(Operand::Value(i_reg), 0);
                let cond_op = self.builder.lt(i_val.clone(), len_op);

                self.builder
                    .branch(cond_op, body_label.clone(), end_label.clone());

                self.builder.set_block(body_label);

                // Get element: aura_array_get(iterable, i)
                let element_op = self
                    .builder
                    .call("aura_array_get".to_string(), vec![iterable_op, i_val]);

                // Loop variable allocation
                let var_ptr = self.builder.new_reg();
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Alloc(var_ptr, 8));
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Store(
                        element_op,
                        Operand::Value(var_ptr),
                        0,
                    ));

                // Register in mem_vars
                self.mem_vars
                    .insert(variable, (var_ptr, class_name, Some(element_ty)));

                self.lower_statement(*body);

                // Increment i
                let i_val2 = self.builder.load(Operand::Value(i_reg), 0);
                let i_next = self.builder.add(i_val2, Operand::Constant(1));
                self.builder
                    .emit(crate::compiler::ir::instr::Instruction::Store(
                        i_next,
                        Operand::Value(i_reg),
                        0,
                    ));

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
            Statement::Error
            | Statement::Import { .. }
            | Statement::Export { .. }
            | Statement::Enum(_)
            | Statement::TypeAlias(_)
            | Statement::Interface(_)
            | Statement::Comment(_, _)
            | Statement::RegularBlockComment(_, _)
            | Statement::Empty(_) => {}
        }
    }
}
