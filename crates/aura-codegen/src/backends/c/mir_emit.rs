//! Small, deliberately total C renderer for the backend-neutral MIR subset.
//!
//! This renderer is a C backend concern; the operations and control-flow it
//! consumes are defined in aura-ir. Unsupported shapes return `false` so the
//! alpha compatibility emitter remains the explicit fallback.

use std::fmt::Write as _;

use aura_ir::{mir, FunctionIr};
use aura_sema::Ty;

use crate::names::{c_fun_name, mangle_ident};

pub(crate) fn emit_function(out: &mut String, ir: &FunctionIr) -> bool {
    // std.* declarations may have intentionally different runtime lowering
    // than their source body (for example signal/error intrinsics). Preserve
    // those backend-specific ABI substitutions until they have their own IR
    // operation, rather than treating a literal body as the final semantics.
    if ir.package.starts_with("std.") {
        return false;
    }
    let Some(body) = ir.body.as_ref() else {
        return false;
    };
    if !emit_body_from_mir(&mut String::new(), body, &ir.package, ir.params.len(), 1) {
        return false;
    }

    let params = if ir.params.is_empty() {
        "void".to_string()
    } else {
        ir.params
            .iter()
            .zip(body.locals.iter())
            .map(|(param, local)| format!("{} {}", c_ty(&param.ty), mangle_ident(&local.name)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let signature = format!(
        "{} {}({})",
        c_ty(&ir.ret.ty),
        c_fun_name(&ir.package, &ir.name, &[]),
        params
    );
    let _ = writeln!(out, "{signature} {{");
    let _ = emit_body_from_mir(out, body, &ir.package, ir.params.len(), 1);
    out.push_str("}\n");
    true
}

pub(crate) fn emit_body_from_mir(
    out: &mut String,
    body: &mir::MirBody,
    package: &str,
    param_count: usize,
    indent: usize,
) -> bool {
    if body.entry >= body.blocks.len()
        || body.validate().is_err()
        || !body
            .blocks
            .iter()
            .all(|block| block.statements.iter().all(statement_supported))
        || !body.locals.iter().all(|local| primitive(&local.ty))
        || !body.blocks.iter().all(|block| {
            matches!(
                block.terminator,
                mir::Terminator::Goto { .. }
                    | mir::Terminator::SwitchInt { .. }
                    | mir::Terminator::SwitchTag { .. }
                    | mir::Terminator::Return { .. }
                    | mir::Terminator::Unreachable
            )
        })
    {
        return false;
    }
    let prefix = "  ".repeat(indent);
    for local in body.locals.iter().skip(param_count) {
        let _ = writeln!(
            out,
            "{prefix}{} {};",
            c_ty(&local.ty),
            mangle_ident(&local.name)
        );
    }
    for (index, block) in body.blocks.iter().enumerate() {
        let _ = writeln!(out, "{prefix}bb_{index}:");
        for statement in &block.statements {
            emit_statement(out, statement, body, package, indent + 1);
        }
        let inner = "  ".repeat(indent + 1);
        match &block.terminator {
            mir::Terminator::Goto { target } => {
                let _ = writeln!(out, "{inner}goto bb_{target};");
            }
            mir::Terminator::SwitchInt {
                condition,
                then_target,
                else_target,
            } => {
                let _ = writeln!(
                    out,
                    "{inner}if ({}) goto bb_{then_target}; else goto bb_{else_target};",
                    place(condition, body)
                );
            }
            mir::Terminator::SwitchTag { .. } => return false,
            mir::Terminator::Return { value } => {
                if let Some(value) = value {
                    let _ = writeln!(out, "{inner}return {};", place(value, body));
                } else if matches!(body.return_ty, Ty::Unit) {
                    let _ = writeln!(out, "{inner}return;");
                }
            }
            mir::Terminator::Unreachable => {
                let _ = writeln!(out, "{inner}abort();");
            }
            mir::Terminator::Await { .. }
            | mir::Terminator::Throw { .. }
            | mir::Terminator::Cancel => return false,
        }
    }
    true
}

fn primitive(ty: &Ty) -> bool {
    matches!(ty, Ty::Unit | Ty::Int | Ty::Bool)
}

fn c_ty(ty: &Ty) -> &'static str {
    match ty {
        Ty::Unit => "void",
        Ty::Bool => "_Bool",
        Ty::Int => "int64_t",
        _ => unreachable!("primitive gate must reject non-primitive types"),
    }
}

fn statement_supported(statement: &mir::Statement) -> bool {
    matches!(statement, mir::Statement::Assign { value, .. } if rvalue_supported(value))
        || matches!(statement, mir::Statement::Evaluate(value) if rvalue_supported(value))
        || matches!(
            statement,
            mir::Statement::Move { .. }
                | mir::Statement::Clone { .. }
                | mir::Statement::Retain { .. }
        )
        || matches!(statement, mir::Statement::Drop(_))
}

fn rvalue_supported(value: &mir::Rvalue) -> bool {
    match value {
        mir::Rvalue::ConstInt(_)
        | mir::Rvalue::ConstFloat(_)
        | mir::Rvalue::ConstBool(_)
        | mir::Rvalue::Use(_) => true,
        mir::Rvalue::Unary { .. } => true,
        mir::Rvalue::Binary { op, .. } => !matches!(op, mir::BinaryOp::Coalesce),
        mir::Rvalue::Select { .. } => true,
        mir::Rvalue::Unwrap { .. } | mir::Rvalue::TypeTest { .. } => false,
        mir::Rvalue::VariantTag { .. } => false,
        mir::Rvalue::Length(_) | mir::Rvalue::Index { .. } | mir::Rvalue::Field { .. } => false,
        mir::Rvalue::Call { target, .. } => {
            !target.package.starts_with("std.")
                && target.variant.as_deref() != Some("__iterable_protocol")
                && !matches!(target.name.as_str(), "gc_collect" | "gc_mark")
        }
        // Runtime intrinsics need a backend capability before they can be
        // rendered as C; never guess a free-function ABI here.
        mir::Rvalue::Intrinsic(_) => false,
        mir::Rvalue::AsyncOp(_) => false,
        mir::Rvalue::ConstString(_) | mir::Rvalue::ConstNull => false,
    }
}

fn emit_statement(
    out: &mut String,
    statement: &mir::Statement,
    body: &mir::MirBody,
    package: &str,
    indent: usize,
) {
    let prefix = "  ".repeat(indent);
    match statement {
        mir::Statement::Assign {
            place: destination,
            value,
        } => {
            let _ = writeln!(
                out,
                "{prefix}{} = {};",
                place(destination, body),
                rvalue(value, body, package)
            );
        }
        mir::Statement::Evaluate(value) => {
            let _ = writeln!(out, "{prefix}{};", rvalue(value, body, package));
        }
        mir::Statement::Move { from, to }
        | mir::Statement::Clone { from, to }
        | mir::Statement::Retain { from, to } => {
            let _ = writeln!(out, "{prefix}{} = {};", place(to, body), place(from, body));
        }
        mir::Statement::Drop(_) => {}
        _ => {}
    }
}

fn rvalue(value: &mir::Rvalue, body: &mir::MirBody, package: &str) -> String {
    match value {
        mir::Rvalue::Use(value) => place(value, body),
        mir::Rvalue::ConstInt(value) => value.to_string(),
        mir::Rvalue::ConstFloat(value) => f64::from_bits(*value).to_string(),
        mir::Rvalue::ConstBool(value) => if *value { "1" } else { "0" }.into(),
        mir::Rvalue::Unary { op, operand } => {
            let op = match op {
                mir::UnaryOp::Neg => "-",
                mir::UnaryOp::Not => "!",
            };
            format!("({op}{})", place(operand, body))
        }
        mir::Rvalue::Binary { op, left, right } => {
            let op = match op {
                mir::BinaryOp::Add => "+",
                mir::BinaryOp::Sub => "-",
                mir::BinaryOp::Mul => "*",
                mir::BinaryOp::Div => "/",
                mir::BinaryOp::Rem => "%",
                mir::BinaryOp::Eq => "==",
                mir::BinaryOp::Ne => "!=",
                mir::BinaryOp::Lt => "<",
                mir::BinaryOp::Le => "<=",
                mir::BinaryOp::Gt => ">",
                mir::BinaryOp::Ge => ">=",
                mir::BinaryOp::And => "&&",
                mir::BinaryOp::Or => "||",
                mir::BinaryOp::Coalesce => return "0".into(),
            };
            format!("({} {op} {})", place(left, body), place(right, body))
        }
        mir::Rvalue::Select {
            condition,
            then_value,
            else_value,
        } => format!(
            "({} ? {} : {})",
            place(condition, body),
            place(then_value, body),
            place(else_value, body)
        ),
        mir::Rvalue::Unwrap { .. } | mir::Rvalue::TypeTest { .. } => "0".into(),
        mir::Rvalue::VariantTag { .. } => "0".into(),
        mir::Rvalue::Length(_) | mir::Rvalue::Index { .. } | mir::Rvalue::Field { .. } => {
            "0".into()
        }
        mir::Rvalue::Call { target, args } => {
            let rendered_args = args.iter().map(|arg| place(arg, body)).collect::<Vec<_>>();
            match (
                target.package.is_empty(),
                target.name.as_str(),
                args.as_slice(),
            ) {
                // These source-level builtins are lowered to runtime calls by
                // the AST emitter; keep the MIR path consistent for primitive tests.
                (true, "assert", [condition]) if body.locals[condition.local].ty == Ty::Bool => {
                    format!("aura_assert({})", rendered_args[0])
                }
                (true, "assert_eq", [left, right])
                    if body.locals[left.local].ty == Ty::Int
                        && body.locals[right.local].ty == Ty::Int =>
                {
                    format!(
                        "aura_assert_eq_int({}, {})",
                        rendered_args[0], rendered_args[1]
                    )
                }
                (true, "assert_eq", [left, right])
                    if body.locals[left.local].ty == Ty::Bool
                        && body.locals[right.local].ty == Ty::Bool =>
                {
                    format!(
                        "aura_assert_eq_bool({}, {})",
                        rendered_args[0], rendered_args[1]
                    )
                }
                _ => format!(
                    "{}({})",
                    c_fun_name(
                        if target.package.is_empty() {
                            package
                        } else {
                            &target.package
                        },
                        &target.name,
                        &target.type_args,
                    ),
                    rendered_args.join(", ")
                ),
            }
        }
        mir::Rvalue::ConstString(_) | mir::Rvalue::ConstNull => "0".into(),
        mir::Rvalue::Intrinsic(_) => "0".into(),
        mir::Rvalue::AsyncOp(_) => "0".into(),
    }
}

fn place(place: &mir::Place, body: &mir::MirBody) -> String {
    mangle_ident(&body.locals[place.local].name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_ir::LoweredProgram;

    #[test]
    fn renders_supported_function_body_from_mir() {
        let file = aura_parser::parse_file("package demo\nfun answer(): Int { return 7 }\n")
            .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let ir = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "answer")
            .expect("function IR");
        let mut output = String::new();
        assert!(emit_function(&mut output, ir));
        assert!(output.contains("__return_0 = 7;"));
        assert!(output.contains("return __return_0;"));
        let translation_unit = crate::emit::emit_c_with_program(&program, Default::default());
        assert!(translation_unit.contains("__return_0 = 7;"));
    }

    #[test]
    fn renders_no_await_async_body_from_mir_when_the_shape_is_supported() {
        let file = aura_parser::parse_file("package demo\nasync fun answer(): Int { return 7 }\n")
            .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let body = program.async_mir().first().expect("async MIR");
        let mut output = String::new();
        assert!(emit_body_from_mir(&mut output, body, "demo", 0, 1));
        assert!(output.contains("__return_0 = 7;"));
        assert!(output.contains("return __return_0;"));
    }

    #[test]
    fn renders_primitive_branch_cfg_from_mir_without_source_lowering() {
        let file = aura_parser::parse_file(
            "package demo\nfun choose(flag: Bool): Int { if (flag) { val one: Int = 1 return one } else { val two: Int = 2 return two } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let ir = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "choose")
            .expect("function IR");
        assert!(ir.body.as_ref().is_some_and(|body| body.blocks.len() > 1));
        let mut output = String::new();
        assert!(emit_function(&mut output, ir));
        assert!(output.contains("goto bb_"));
        assert!(output.matches("return ").count() >= 2);
    }

    #[test]
    fn renders_user_call_rvalue_from_mir() {
        let file = aura_parser::parse_file(
            "package demo\nfun one(value: Int): Int { return value }\nfun two(value: Int): Int { return one(value) }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let ir = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "two")
            .expect("function IR");
        let mut output = String::new();
        assert!(emit_function(&mut output, ir));
        assert!(output.contains("aura_fn_demo_one("));
    }

    #[test]
    fn renders_primitive_assertions_as_runtime_calls_from_mir() {
        let file = aura_parser::parse_file(
            "package demo\nfun checks() { assert_eq(1, 1) assert(true) }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let ir = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "checks")
            .expect("function IR");
        let mut output = String::new();
        assert!(emit_function(&mut output, ir));
        assert!(output.contains("aura_assert_eq_int("));
        assert!(output.contains("aura_assert("));
        assert!(!output.contains("aura_fn_demo_assert_eq("));
        assert!(!output.contains("aura_fn_demo_assert("));
    }

    #[test]
    fn renders_loop_body_from_mir_without_ast_lowering() {
        let file = aura_parser::parse_file(
            "package demo\nfun touch() { }\nfun spin(flag: Bool) { while (flag) { touch() } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let ir = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "spin")
            .expect("function IR");
        let mut output = String::new();
        assert!(emit_function(&mut output, ir));
        assert!(output.contains("goto bb_"));
        assert!(output.contains("aura_fn_demo_touch();"));
    }

    #[test]
    fn renders_for_range_cfg_from_mir() {
        let file = aura_parser::parse_file(
            "package demo\nfun touch(value: Int) { }\nfun count() { for (i in 0..3) { touch(i) } }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let ir = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "count")
            .expect("function IR");
        let mut output = String::new();
        assert!(emit_function(&mut output, ir));
        assert!(output.contains("goto bb_"));
        assert!(output.contains("aura_fn_demo_touch("));
    }

    #[test]
    fn rejects_protocol_dispatch_until_alpha_receiver_abi_is_available() {
        let file = aura_parser::parse_file(
            "package demo\ninterface Iterable { fun len(): Int fun get(i: Int): Int }\nfun sum(it: Iterable): Int { for (x in it) { return x } return 0 }\n",
        )
        .expect("parse");
        let checked = aura_sema::check_file(&file).expect("check");
        let program = LoweredProgram::from_checked(checked);
        let ir = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "sum")
            .expect("function IR");
        let mut output = String::new();
        assert!(!emit_function(&mut output, ir));
        assert!(output.is_empty());
    }
}
