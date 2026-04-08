use crate::*;
use aura_ast::{Block, Expr, Ident, Program, Stmt, TopLevel};
use aura_span::Span;
use aura_typeck::{Ty, TypedProgram};
use std::collections::{BTreeSet, HashMap};

pub fn lower_program(source: &str, program: &Program, typed_program: &TypedProgram) -> MirProgram {
    let mut lowerer = Lowerer::new(source, typed_program);
    lowerer.lower(program)
}

struct Lowerer<'a> {
    source: &'a str,
    typed_program: &'a TypedProgram,
    functions: Vec<MirFunction>,
    classes: HashMap<String, MirClass>,
    interfaces: HashMap<String, MirInterface>,
    method_slots: Vec<String>,
}

impl<'a> Lowerer<'a> {
    fn new(source: &'a str, typed_program: &'a TypedProgram) -> Self {
        Self {
            source,
            typed_program,
            functions: Vec::new(),
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            method_slots: Vec::new(),
        }
    }

    fn lower(&mut self, program: &Program) -> MirProgram {
        let mut slot_names = BTreeSet::new();
        for class in self.typed_program.classes.values() {
            for name in class.methods.keys() {
                slot_names.insert(name.clone());
            }
        }
        for iface in self.typed_program.interfaces.values() {
            for name in iface.methods.keys() {
                slot_names.insert(name.clone());
            }
        }
        self.method_slots = slot_names.into_iter().collect();

        self.interfaces = self
            .typed_program
            .interfaces
            .iter()
            .map(|(name, info)| {
                (
                    name.clone(),
                    MirInterface {
                        methods: info.methods.clone(),
                    },
                )
            })
            .collect();

        for item in &program.items {
            match item {
                TopLevel::Function(func) => {
                    let fname = self.ident_text(&func.name);
                    let sig = self.typed_program.functions.get(fname).cloned().unwrap_or(
                        aura_typeck::MethodSig {
                            params: vec![],
                            return_ty: Ty::Void,
                        },
                    );
                    let mir_func = self.lower_function(
                        fname.to_string(),
                        func.name.span,
                        &func.body,
                        &func.params,
                        None,
                        sig.return_ty,
                    );
                    self.functions.push(mir_func);
                }
                TopLevel::Class(class_decl) => {
                    let class_name = self.ident_text(&class_decl.name).to_string();
                    let mut methods = HashMap::new();

                    if let Some(cinfo) = self.typed_program.classes.get(&class_name) {
                        for method in &class_decl.methods {
                            let mname = self.ident_text(&method.name).to_string();
                            let sig = cinfo.methods.get(&mname).cloned().unwrap_or(
                                aura_typeck::MethodSig {
                                    params: vec![],
                                    return_ty: Ty::Void,
                                },
                            );
                            let mir_method = self.lower_function(
                                MirBuilder::qualified_method_name(&class_name, &mname),
                                method.name.span,
                                &method.body,
                                &method.params,
                                Some(Ty::Class(class_name.clone())),
                                sig.return_ty,
                            );
                            methods.insert(mname, mir_method);
                        }
                    }

                    self.classes.insert(
                        class_name.clone(),
                        MirClass {
                            name: class_name.clone(),
                            extends: class_decl
                                .extends
                                .as_ref()
                                .map(|ty| self.ident_text(&ty.name).to_string()),
                            fields: self
                                .typed_program
                                .classes
                                .get(&class_name)
                                .map(|c| c.fields.clone())
                                .unwrap_or_default(),
                            field_order: self
                                .typed_program
                                .classes
                                .get(&class_name)
                                .map(|c| c.field_order.clone())
                                .unwrap_or_default(),
                            methods,
                        },
                    );
                }
                _ => {}
            }
        }

        MirProgram {
            functions: self.functions.split_off(0),
            classes: self.classes.drain().collect(),
            interfaces: self.interfaces.drain().collect(),
            method_slots: self.method_slots.clone(),
        }
    }

    fn lower_function(
        &self,
        name: String,
        name_span: Span,
        body: &Block,
        params: &[aura_ast::Param],
        this_ty: Option<Ty>,
        return_ty: Ty,
    ) -> MirFunction {
        let mut builder = MirBuilder::new(name);

        // Declare return slot (id 0)
        builder.declare_local(
            return_ty,
            Some("return".to_string()),
            name_span,
            LocalKind::Return,
        );

        // Declare 'this' if applicable (id 1)
        if let Some(ty) = this_ty {
            builder.this_local_id = Some(builder.declare_local(
                ty,
                Some("this".to_string()),
                name_span,
                LocalKind::Arg,
            ));
        }

        // Declare parameters
        for param in params {
            let pname = self.ident_text(&param.name).to_string();
            let ty = self
                .typed_program
                .expression_types
                .get(&param.name.span)
                .cloned()
                .unwrap_or(Ty::Unknown);
            builder.declare_local(ty, Some(pname), param.span, LocalKind::Arg);
        }

        builder.lower_block(self, body);

        // Ensure the last block has a terminator
        builder.ensure_terminated();

        builder.build()
    }

    fn ident_text(&self, ident: &Ident) -> &str {
        self.source_at(ident.span)
    }

    fn source_at(&self, span: Span) -> &str {
        &self.source[span.start.raw() as usize..span.end.raw() as usize]
    }
}

struct MirBuilder {
    name: String,
    locals: Vec<LocalDecl>,
    blocks: Vec<BasicBlock>,
    cleanup_regions: Vec<CleanupRegion>,
    cleanup_stack: Vec<CleanupContext>,
    current_block: usize,
    this_local_id: Option<usize>,
}

#[derive(Clone)]
struct CleanupContext {
    catch_block: Option<usize>,
    finally_block: Option<usize>,
    exit_mode_local: Option<usize>,
    exception_local: Option<usize>,
}

impl MirBuilder {
    fn new(name: String) -> Self {
        let mut builder = Self {
            name,
            locals: Vec::new(),
            blocks: Vec::new(),
            cleanup_regions: Vec::new(),
            cleanup_stack: Vec::new(),
            current_block: 0,
            this_local_id: None,
        };
        builder.new_block();
        builder
    }

    fn new_block(&mut self) -> usize {
        let id = self.blocks.len();
        self.blocks.push(BasicBlock {
            id,
            statements: Vec::new(),
            terminator: None,
        });
        id
    }

    fn declare_local(
        &mut self,
        ty: Ty,
        name: Option<String>,
        span: Span,
        kind: LocalKind,
    ) -> usize {
        let id = self.locals.len();
        self.locals.push(LocalDecl {
            ty,
            name,
            span,
            kind,
        });
        id
    }

    fn push_cleanup_context(
        &mut self,
        catch_block: Option<usize>,
        finally_block: Option<usize>,
        exit_mode_local: Option<usize>,
        exception_local: Option<usize>,
    ) {
        self.cleanup_stack.push(CleanupContext {
            catch_block,
            finally_block,
            exit_mode_local,
            exception_local,
        });
    }

    fn pop_cleanup_context(&mut self) {
        let _ = self.cleanup_stack.pop();
    }

    fn cleanup_context_for_return(&self) -> Option<&CleanupContext> {
        self.cleanup_stack
            .iter()
            .rev()
            .find(|ctx| ctx.finally_block.is_some())
    }

    fn emit_return(&mut self, value: Option<Operand>) {
        if let Some(ctx) = self.cleanup_context_for_return().cloned() {
            if let Some(val) = value {
                self.push_stmt(Statement::Assign(
                    Lvalue::Local(0),
                    Rvalue::Use(val),
                ));
            }
            if let Some(exit_mode_local) = ctx.exit_mode_local {
                self.push_stmt(Statement::Assign(
                    Lvalue::Local(exit_mode_local),
                    Rvalue::Use(Operand::Constant(Constant::Int(1))),
                ));
            }
            if let Some(finally_block) = ctx.finally_block {
                self.terminate(Terminator::Goto(finally_block));
            } else {
                self.terminate(Terminator::Return(Some(Operand::Copy(Lvalue::Local(0)))));
            }
            return;
        }

        self.terminate(Terminator::Return(value));
    }

    fn emit_throw(&mut self, value: Operand) {
        let current_block = self.current_block;
        let cleanup_stack = self.cleanup_stack.clone();
        for ctx in cleanup_stack.into_iter().rev() {
            if ctx.catch_block == Some(current_block) {
                if let Some(finally_block) = ctx.finally_block {
                    if let Some(exception_local) = ctx.exception_local {
                        self.push_stmt(Statement::Assign(
                            Lvalue::Local(exception_local),
                            Rvalue::Use(value),
                        ));
                    }
                    if let Some(exit_mode_local) = ctx.exit_mode_local {
                        self.push_stmt(Statement::Assign(
                            Lvalue::Local(exit_mode_local),
                            Rvalue::Use(Operand::Constant(Constant::Int(2))),
                        ));
                    }
                    self.terminate(Terminator::Goto(finally_block));
                    return;
                }
                continue;
            }

            if let Some(exception_local) = ctx.exception_local {
                self.push_stmt(Statement::Assign(
                    Lvalue::Local(exception_local),
                    Rvalue::Use(value.clone()),
                ));
            }
            if let Some(catch_block) = ctx.catch_block {
                self.terminate(Terminator::Goto(catch_block));
                return;
            }
            if let Some(finally_block) = ctx.finally_block {
                if let Some(exit_mode_local) = ctx.exit_mode_local {
                    self.push_stmt(Statement::Assign(
                        Lvalue::Local(exit_mode_local),
                        Rvalue::Use(Operand::Constant(Constant::Int(2))),
                    ));
                }
                self.terminate(Terminator::Goto(finally_block));
                return;
            }
        }

        self.terminate(Terminator::Unreachable);
    }

    fn lower_block(&mut self, lowerer: &Lowerer, block: &Block) {
        for stmt in &block.stmts {
            self.lower_stmt(lowerer, stmt);
        }
    }

    fn lower_stmt(&mut self, lowerer: &Lowerer, stmt: &Stmt) {
        match stmt {
            Stmt::Let(s) | Stmt::Const(s) => {
                let name = lowerer.ident_text(&s.name).to_string();
                let ty = lowerer
                    .typed_program
                    .expression_types
                    .get(&s.name.span)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let local_id = self.declare_local(ty, Some(name), s.span, LocalKind::Var);
                if let Some(init) = &s.init {
                    let rvalue = self.lower_expr(lowerer, init);
                    self.push_stmt(Statement::Assign(Lvalue::Local(local_id), rvalue));
                }
            }
            Stmt::Expr(s) => {
                let _ = self.lower_expr(lowerer, &s.expr);
            }
            Stmt::Return(s) => {
                let return_ty = self.locals[0].ty.clone();
                let op = s
                    .value
                    .as_ref()
                    .map(|v| self.lower_expr_to_operand(lowerer, v, Some(&return_ty)));
                self.emit_return(op);
            }
            Stmt::Throw(s) => {
                let value = self.lower_expr_to_operand(lowerer, &s.value, None);
                self.emit_throw(value);
            }
            Stmt::If(s) => {
                let cond = self.lower_expr_to_operand(lowerer, &s.cond, Some(&Ty::Bool));
                let then_id = self.new_block();
                let join_id = self.new_block();
                let else_id = s
                    .else_block
                    .as_ref()
                    .map(|_| self.new_block())
                    .unwrap_or(join_id);

                self.terminate(Terminator::SwitchInt {
                    discr: cond,
                    targets: vec![(1, then_id)], // 1 is true
                    otherwise: else_id,
                });

                // Then block
                self.current_block = then_id;
                self.lower_block(lowerer, &s.then_block);
                self.terminate(Terminator::Goto(join_id));

                // Else block
                if let Some(else_block) = &s.else_block {
                    self.current_block = else_id;
                    self.lower_block(lowerer, else_block);
                    self.terminate(Terminator::Goto(join_id));
                }

                self.current_block = join_id;
            }
            Stmt::While(s) => {
                let cond_id = self.new_block();
                let body_id = self.new_block();
                let exit_id = self.new_block();
                let _cond_span = self.span_of_expr(&s.cond);

                self.terminate(Terminator::Goto(cond_id));

                // Cond block
                self.current_block = cond_id;
                let cond = self.lower_expr_to_operand(lowerer, &s.cond, Some(&Ty::Bool));
                self.terminate(Terminator::SwitchInt {
                    discr: cond,
                    targets: vec![(1, body_id)],
                    otherwise: exit_id,
                });

                // Body block
                self.current_block = body_id;
                self.lower_block(lowerer, &s.body);
                self.terminate(Terminator::Goto(cond_id));

                self.current_block = exit_id;
            }
            Stmt::Try(s) => {
                self.lower_try_stmt(lowerer, s);
            }
            Stmt::Block(b) => {
                self.lower_block(lowerer, b);
            }
            Stmt::Empty(_) => {}
        }
    }

    fn lower_try_stmt(&mut self, lowerer: &Lowerer, stmt: &aura_ast::TryStmt) {
        let try_entry = self.current_block;
        let after_block = self.new_block();
        let catch_block = stmt.catch.as_ref().map(|_| self.new_block());
        let finally_block = stmt.finally_block.as_ref().map(|_| self.new_block());
        let exit_mode_local = if finally_block.is_some() {
            Some(self.declare_local(
                Ty::I32,
                Some("__cleanup_mode".to_string()),
                stmt.span,
                LocalKind::Temp,
            ))
        } else {
            None
        };
        let exception_local = if stmt.catch.is_some() || finally_block.is_some() {
            Some(self.declare_local(
                Ty::Unknown,
                Some("__caught_exception".to_string()),
                stmt.span,
                LocalKind::Temp,
            ))
        } else {
            None
        };

        self.push_cleanup_context(
            catch_block,
            finally_block,
            exit_mode_local,
            exception_local,
        );

        let mut edges = vec![CleanupEdge {
            from_block: try_entry,
            to_block: finally_block.unwrap_or(after_block),
            reason: CleanupReason::Normal,
        }];
        if let Some(catch_block_id) = catch_block {
            edges.push(CleanupEdge {
                from_block: try_entry,
                to_block: catch_block_id,
                reason: CleanupReason::Throw,
            });
            edges.push(CleanupEdge {
                from_block: catch_block_id,
                to_block: finally_block.unwrap_or(after_block),
                reason: CleanupReason::Normal,
            });
        }
        if let Some(finally_block_id) = finally_block {
            edges.push(CleanupEdge {
                from_block: finally_block_id,
                to_block: after_block,
                reason: CleanupReason::Return,
            });
        }
        self.cleanup_regions.push(CleanupRegion {
            try_block: try_entry,
            catch_block,
            finally_block,
            after_block,
            edges,
        });

        self.lower_block(lowerer, &stmt.try_block);
        if self.blocks[self.current_block].terminator.is_none() {
            if let Some(finally_block_id) = finally_block {
                if let Some(exit_mode_local) = exit_mode_local {
                    self.push_stmt(Statement::Assign(
                        Lvalue::Local(exit_mode_local),
                        Rvalue::Use(Operand::Constant(Constant::Int(0))),
                    ));
                }
                self.terminate(Terminator::Goto(finally_block_id));
            } else {
                self.terminate(Terminator::Goto(after_block));
            }
        }

        if let Some(catch) = &stmt.catch {
            if let Some(catch_block_id) = catch_block {
                self.current_block = catch_block_id;
                let catch_binding = self.declare_local(
                    Ty::Unknown,
                    Some(lowerer.ident_text(&catch.binding).to_string()),
                    catch.binding.span,
                    LocalKind::Var,
                );
                if let Some(exception_local) = exception_local {
                    self.push_stmt(Statement::Assign(
                        Lvalue::Local(catch_binding),
                        Rvalue::Use(Operand::Copy(Lvalue::Local(exception_local))),
                    ));
                }
                self.lower_block(lowerer, &catch.block);
                if self.blocks[self.current_block].terminator.is_none() {
                    if let Some(finally_block_id) = finally_block {
                        self.terminate(Terminator::Goto(finally_block_id));
                    } else {
                        self.terminate(Terminator::Goto(after_block));
                    }
                }
            }
        }

        if let Some(finally_block_id) = finally_block {
            self.pop_cleanup_context();
            self.current_block = finally_block_id;
            if let Some(finally_stmt) = &stmt.finally_block {
                self.lower_block(lowerer, finally_stmt);
            }

            if self.blocks[self.current_block].terminator.is_none() {
                if let Some(exit_mode_local) = exit_mode_local {
                    let return_id = self.new_block();
                    let throw_id = self.new_block();
                    self.terminate(Terminator::SwitchInt {
                        discr: Operand::Copy(Lvalue::Local(exit_mode_local)),
                        targets: vec![(1, return_id), (2, throw_id)],
                        otherwise: after_block,
                    });

                    self.current_block = return_id;
                    self.emit_return(Some(Operand::Copy(Lvalue::Local(0))));

                    self.current_block = throw_id;
                    if let Some(exception_local) = exception_local {
                        self.emit_throw(Operand::Copy(Lvalue::Local(exception_local)));
                    } else {
                        self.terminate(Terminator::Unreachable);
                    }
                } else {
                    self.terminate(Terminator::Goto(after_block));
                }
            }
        } else {
            self.pop_cleanup_context();
        }

        self.current_block = after_block;
        let _ = try_entry;
    }

    fn lower_expr(&mut self, lowerer: &Lowerer, expr: &Expr) -> Rvalue {
        match expr {
            Expr::IntLit(span) => {
                let val = lowerer.source_at(*span).parse::<i64>().unwrap_or(0);
                Rvalue::Use(Operand::Constant(Constant::Int(val)))
            }
            Expr::FloatLit(span) => {
                let val = lowerer.source_at(*span).parse::<f64>().unwrap_or(0.0);
                Rvalue::Use(Operand::Constant(Constant::Float(val)))
            }
            Expr::StringLit(span) => {
                let s = lowerer.source_at(*span);
                // Remove quotes
                let s = &s[1..s.len() - 1];
                Rvalue::Use(Operand::Constant(Constant::String(s.to_string())))
            }
            Expr::BoolLit(val, _) => Rvalue::Use(Operand::Constant(Constant::Bool(*val))),
            Expr::Ident(id) => {
                let name = lowerer.ident_text(id);
                // Find local by name (simple for now, should handle scoping better)
                let local_id = self
                    .locals
                    .iter()
                    .enumerate()
                    .rfind(|(_, l)| l.name.as_deref() == Some(name))
                    .map(|(i, _)| i)
                    .or_else(|| {
                        // If not found as a regular local/param, check if it's 'this' field?
                        // No, the parser/typeck should have handled that by making it a Member access.
                        None
                    });

                if let Some(id) = local_id {
                    Rvalue::Use(Operand::Copy(Lvalue::Local(id)))
                } else {
                    // Fallback (e.g. for global functions, though they shouldn't be lvalues)
                    Rvalue::Use(Operand::Constant(Constant::String(name.to_string())))
                }
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                match op {
                    BinaryOp::AndAnd => {
                        let result_ty = Ty::Bool;
                        let result_local = self.declare_local(
                            result_ty,
                            None,
                            self.span_of_expr(expr),
                            LocalKind::Temp,
                        );
                        let result_lval = Lvalue::Local(result_local);

                        let then_id = self.new_block();
                        let else_id = self.new_block();
                        let join_id = self.new_block();

                        // Evaluate left
                        let lop = self.lower_expr_to_operand(lowerer, left, Some(&Ty::Bool));
                        self.terminate(Terminator::SwitchInt {
                            discr: lop,
                            targets: vec![(1, then_id)],
                            otherwise: else_id,
                        });

                        // Left was true: evaluate right and assign to result
                        self.current_block = then_id;
                        let rop = self.lower_expr_to_operand(lowerer, right, Some(&Ty::Bool));
                        self.push_stmt(Statement::Assign(result_lval.clone(), Rvalue::Use(rop)));
                        self.terminate(Terminator::Goto(join_id));

                        // Left was false: result is false
                        self.current_block = else_id;
                        self.push_stmt(Statement::Assign(
                            result_lval.clone(),
                            Rvalue::Use(Operand::Constant(Constant::Bool(false))),
                        ));
                        self.terminate(Terminator::Goto(join_id));

                        self.current_block = join_id;
                        return Rvalue::Use(Operand::Move(result_lval));
                    }
                    BinaryOp::OrOr => {
                        let result_local = self.declare_local(
                            Ty::Bool,
                            None,
                            self.span_of_expr(expr),
                            LocalKind::Temp,
                        );
                        let result_lval = Lvalue::Local(result_local);

                        let then_id = self.new_block();
                        let else_id = self.new_block();
                        let join_id = self.new_block();

                        let lop = self.lower_expr_to_operand(lowerer, left, Some(&Ty::Bool));
                        self.terminate(Terminator::SwitchInt {
                            discr: lop,
                            targets: vec![(1, then_id)], // If true, result is true
                            otherwise: else_id,          // If false, evaluate right
                        });

                        // Left was true: result is true
                        self.current_block = then_id;
                        self.push_stmt(Statement::Assign(
                            result_lval.clone(),
                            Rvalue::Use(Operand::Constant(Constant::Bool(true))),
                        ));
                        self.terminate(Terminator::Goto(join_id));

                        // Left was false: evaluate right
                        self.current_block = else_id;
                        let rop = self.lower_expr_to_operand(lowerer, right, Some(&Ty::Bool));
                        self.push_stmt(Statement::Assign(result_lval.clone(), Rvalue::Use(rop)));
                        self.terminate(Terminator::Goto(join_id));

                        self.current_block = join_id;
                        return Rvalue::Use(Operand::Move(result_lval));
                    }
                    _ => {
                        let lop = self.lower_expr_to_operand(lowerer, left, None);
                        let rop = self.lower_expr_to_operand(lowerer, right, None);
                        Rvalue::BinaryOp(*op, lop, rop)
                    }
                }
            }
            Expr::Unary { op, expr, .. } => {
                let op_val = self.lower_expr_to_operand(lowerer, expr, None);
                Rvalue::UnaryOp(*op, op_val)
            }
            Expr::Assign { target, value, .. } => {
                let lval = self.lower_expr_to_lvalue(lowerer, target);
                let target_ty = lowerer
                    .typed_program
                    .expression_types
                    .get(&self.span_of_expr(target));
                let val_op = self.lower_expr_to_operand(lowerer, value, target_ty);
                self.push_stmt(Statement::Assign(lval.clone(), Rvalue::Use(val_op.clone())));
                Rvalue::Use(val_op)
            }
            Expr::Call { callee, args, span } => {
                let callee_span = self.span_of_expr(callee);
                let callee_ty = lowerer
                    .typed_program
                    .expression_types
                    .get(&callee_span)
                    .cloned();
                let params = if let Some(Ty::Function(sig)) = callee_ty.clone() {
                    sig.params.clone()
                } else {
                    vec![]
                };

                let mut arg_ops: Vec<Operand> = Vec::with_capacity(args.len() + 1);
                let callee_op = if let Expr::Member { object, field, .. } = &**callee {
                    let object_ty = lowerer
                        .typed_program
                        .expression_types
                        .get(&self.span_of_expr(object))
                        .cloned();
                    if matches!(object_ty, Some(Ty::Class(_)) | Some(Ty::Interface(_))) {
                        let field_name = lowerer.ident_text(field).to_string();
                        arg_ops.push(self.lower_expr_to_operand(
                            lowerer,
                            object,
                            object_ty.as_ref(),
                        ));
                        Operand::Constant(Constant::String(format!("method:{field_name}")))
                    } else {
                        self.lower_expr_to_operand(lowerer, callee, None)
                    }
                } else {
                    self.lower_expr_to_operand(lowerer, callee, None)
                };

                for (i, a) in args.iter().enumerate() {
                    arg_ops.push(self.lower_expr_to_operand(lowerer, a, params.get(i)));
                }

                let dest_ty = lowerer
                    .typed_program
                    .expression_types
                    .get(span)
                    .cloned()
                    .unwrap_or(Ty::Void);
                let destination = self.declare_local(dest_ty, None, *span, LocalKind::Temp);

                let next_id = self.new_block();
                self.terminate(Terminator::Call {
                    callee: callee_op,
                    args: arg_ops,
                    destination: Lvalue::Local(destination),
                    target: next_id,
                });

                self.current_block = next_id;
                Rvalue::Use(Operand::Move(Lvalue::Local(destination)))
            }
            Expr::This(span) => {
                if let Some(id) = self.this_local_id {
                    Rvalue::Use(Operand::Copy(Lvalue::Local(id)))
                } else {
                    // This shouldn't happen if typeck passed.
                    let ty = lowerer
                        .typed_program
                        .expression_types
                        .get(span)
                        .cloned()
                        .unwrap_or(Ty::Unknown);
                    let id =
                        self.declare_local(ty, Some("this".to_string()), *span, LocalKind::Arg);
                    self.this_local_id = Some(id);
                    Rvalue::Use(Operand::Copy(Lvalue::Local(id)))
                }
            }
            Expr::New { class, args, span } => {
                // 'new' is also a call to a constructor (or a special MIR op)
                // For now, let's treat it as a special Call.
                let arg_ops: Vec<_> = args
                    .iter()
                    .map(|a| self.lower_expr_to_operand(lowerer, a, None)) // TODO: Get constructor params
                    .collect();
                let dest_ty = Ty::Class(lowerer.ident_text(class).to_string());
                let destination = self.declare_local(dest_ty, None, *span, LocalKind::Temp);

                let next_id = self.new_block();
                // We'll use a placeholder for 'new' callee for now.
                self.terminate(Terminator::Call {
                    callee: Operand::Constant(Constant::String(format!(
                        "new:{}",
                        lowerer.ident_text(class)
                    ))),
                    args: arg_ops,
                    destination: Lvalue::Local(destination),
                    target: next_id,
                });

                self.current_block = next_id;
                Rvalue::Use(Operand::Move(Lvalue::Local(destination)))
            }
            Expr::Member { object, field, .. } => {
                let obj_lval = self.lower_expr_to_lvalue(lowerer, object);
                Rvalue::Use(Operand::Copy(Lvalue::Field(
                    Box::new(obj_lval),
                    lowerer.ident_text(field).to_string(),
                )))
            }
            Expr::Paren { expr, .. } => self.lower_expr(lowerer, expr),
        }
    }

    fn lower_expr_to_operand(
        &mut self,
        lowerer: &Lowerer,
        expr: &Expr,
        target_ty: Option<&Ty>,
    ) -> Operand {
        let span = self.span_of_expr(expr);
        let rvalue = self.lower_expr(lowerer, expr);

        let expr_ty = lowerer
            .typed_program
            .expression_types
            .get(&span)
            .cloned()
            .unwrap_or(Ty::Unknown);

        // Handle implicit string coercion
        if let Some(Ty::String) = target_ty {
            if expr_ty != Ty::String && expr_ty != Ty::Unknown {
                let op = match rvalue {
                    Rvalue::Use(op) => op,
                    _ => {
                        let temp = self.declare_local(expr_ty.clone(), None, span, LocalKind::Temp);
                        self.push_stmt(Statement::Assign(Lvalue::Local(temp), rvalue));
                        Operand::Move(Lvalue::Local(temp))
                    }
                };
                return self.coerce_to_string(lowerer, op, &expr_ty, span);
            }
        }

        match rvalue {
            Rvalue::Use(op) => op,
            _ => {
                // If it's a complex rvalue, evaluate it into a temporary.
                let temp = self.declare_local(expr_ty, None, span, LocalKind::Temp);
                let lval = Lvalue::Local(temp);
                self.push_stmt(Statement::Assign(lval.clone(), rvalue));
                Operand::Move(lval)
            }
        }
    }

    fn coerce_to_string(
        &mut self,
        _lowerer: &Lowerer,
        op: Operand,
        from_ty: &Ty,
        span: Span,
    ) -> Operand {
        match from_ty {
            Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64 | Ty::Bool => {
                let runtime_func = match from_ty {
                    Ty::I32 => "aura_i32_to_string",
                    Ty::I64 => "aura_i64_to_string",
                    Ty::F32 => "aura_f32_to_string",
                    Ty::F64 => "aura_f64_to_string",
                    Ty::Bool => "aura_bool_to_string",
                    _ => unreachable!(),
                };

                let temp = self.declare_local(Ty::String, None, span, LocalKind::Temp);
                let next_id = self.new_block();
                self.terminate(Terminator::Call {
                    callee: Operand::Constant(Constant::String(runtime_func.to_string())),
                    args: vec![op],
                    destination: Lvalue::Local(temp),
                    target: next_id,
                });
                self.current_block = next_id;
                Operand::Move(Lvalue::Local(temp))
            }
            Ty::Class(_) => {
                // Call .toString()
                let temp = self.declare_local(Ty::String, None, span, LocalKind::Temp);
                let next_id = self.new_block();

                self.terminate(Terminator::Call {
                    callee: Operand::Constant(Constant::String("method:toString".to_string())),
                    args: vec![op],
                    destination: Lvalue::Local(temp),
                    target: next_id,
                });
                self.current_block = next_id;
                Operand::Move(Lvalue::Local(temp))
            }
            _ => op,
        }
    }

    fn qualified_method_name(class_name: &str, method_name: &str) -> String {
        format!("{class_name}::{method_name}")
    }

    fn lower_expr_to_lvalue(&mut self, lowerer: &Lowerer, expr: &Expr) -> Lvalue {
        match expr {
            Expr::Ident(id) => {
                let name = lowerer.ident_text(id);
                let local_id = self
                    .locals
                    .iter()
                    .enumerate()
                    .rfind(|(_, l)| l.name.as_deref() == Some(name))
                    .map(|(i, _)| i)
                    .unwrap_or_else(|| {
                        // Fallback/Error case (e.g. for globals handled by typeck)
                        0
                    });
                Lvalue::Local(local_id)
            }
            Expr::Member { object, field, .. } => {
                let obj_lval = self.lower_expr_to_lvalue(lowerer, object);
                Lvalue::Field(Box::new(obj_lval), lowerer.ident_text(field).to_string())
            }
            Expr::This(_) => {
                if let Some(id) = self.this_local_id {
                    Lvalue::Local(id)
                } else {
                    Lvalue::Local(0)
                }
            }
            _ => {
                // For complex expressions (like calls), evaluate into a temporary then use as Lvalue
                let op = self.lower_expr_to_operand(lowerer, expr, None);
                match op {
                    Operand::Copy(lval) | Operand::Move(lval) => lval,
                    Operand::Constant(_) => {
                        // Constants shouldn't really be used as Lvalue bases directy in source
                        // but if they are (like "literal".field), evaluation into a temp is needed.
                        let span = self.span_of_expr(expr);
                        let ty = lowerer
                            .typed_program
                            .expression_types
                            .get(&span)
                            .cloned()
                            .unwrap_or(Ty::Unknown);
                        let temp = self.declare_local(ty, None, span, LocalKind::Temp);
                        self.push_stmt(Statement::Assign(Lvalue::Local(temp), Rvalue::Use(op)));
                        Lvalue::Local(temp)
                    }
                }
            }
        }
    }

    fn push_stmt(&mut self, stmt: Statement) {
        self.blocks[self.current_block].statements.push(stmt);
    }

    fn terminate(&mut self, term: Terminator) {
        if self.blocks[self.current_block].terminator.is_none() {
            self.blocks[self.current_block].terminator = Some(term);
        }
    }

    fn ensure_terminated(&mut self) {
        if self.blocks[self.current_block].terminator.is_none() {
            self.blocks[self.current_block].terminator = Some(Terminator::Return(None));
        }
    }

    fn build(self) -> MirFunction {
        MirFunction {
            name: self.name,
            locals: self.locals,
            blocks: self.blocks,
            cleanup_regions: self.cleanup_regions,
        }
    }

    fn span_of_expr(&self, expr: &Expr) -> Span {
        match expr {
            Expr::Ident(id) => id.span,
            Expr::This(s) => *s,
            Expr::IntLit(s) => *s,
            Expr::FloatLit(s) => *s,
            Expr::StringLit(s) => *s,
            Expr::BoolLit(_, s) => *s,
            Expr::Unary { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Assign { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::New { span, .. } => *span,
            Expr::Member { span, .. } => *span,
            Expr::Paren { span, .. } => *span,
        }
    }
}
