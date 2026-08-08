use std::collections::HashMap;
use std::fmt::Write as _;

use aura_ir::mir::{BinaryOp, MirBody, Place, Rvalue, Statement, Terminator, UnaryOp};
use aura_ir::{FunctionIr, LoweredProgram};
use aura_sema::Ty;

use crate::error::CodegenError;

type Signatures = HashMap<(String, String), (Ty, Vec<Ty>)>;

pub fn emit_module(program: &LoweredProgram) -> Result<String, CodegenError> {
    validate_program(program)?;
    let mut module = String::from("; ModuleID = 'aura'\nsource_filename = \"aura\"\n\n");
    let signatures = signatures(program);
    for function in program
        .checked()
        .functions
        .iter()
        .chain(program.checked().generic_functions.iter())
    {
        let Some(body) = &function.body else {
            continue;
        };
        emit_function(&mut module, function, body, &signatures)?;
    }
    if let Some(function) = program
        .checked()
        .functions
        .iter()
        .find(|function| function.name == "main" && function.params.is_empty())
    {
        let symbol = symbol_name(&function.package, &function.name);
        module.push_str(&format!(
            "define i32 @main() {{\nentry:\n  call void @{symbol}()\n  ret i32 0\n}}\n"
        ));
    }
    Ok(module)
}

fn validate_program(program: &LoweredProgram) -> Result<(), CodegenError> {
    let checked = program.checked();
    if !checked.async_mir.is_empty()
        || !checked.open_generic_async_mir.is_empty()
        || !checked.generic_async_mir.is_empty()
        || !checked.generic_async_method_mir.is_empty()
    {
        return Err(unsupported("async MIR"));
    }
    for function in checked
        .functions
        .iter()
        .chain(checked.generic_functions.iter())
    {
        if let Some(body) = &function.body {
            body.validate()
                .map_err(|error| unsupported(&format!("invalid MIR: {error:?}")))?;
        }
    }
    if checked
        .functions
        .iter()
        .any(|function| function.name == "main" && function.ret.ty != Ty::Unit)
    {
        return Err(unsupported("non-unit main"));
    }
    Ok(())
}

fn emit_function(
    out: &mut String,
    function: &FunctionIr,
    body: &MirBody,
    signatures: &Signatures,
) -> Result<(), CodegenError> {
    let ret = llvm_type(&function.ret.ty)?;
    let params = function
        .params
        .iter()
        .enumerate()
        .map(|(index, value)| Ok(format!("{} %arg{index}", llvm_type(&value.ty)?)))
        .collect::<Result<Vec<_>, CodegenError>>()?
        .join(", ");
    writeln!(
        out,
        "define {ret} @{}({params}) {{",
        symbol_name(&function.package, &function.name)
    )
    .unwrap();
    out.push_str("entry:\n");
    for (index, local) in body.locals.iter().enumerate() {
        if local.ty != Ty::Unit {
            writeln!(out, "  %slot{index} = alloca {}", llvm_type(&local.ty)?).unwrap();
        }
    }
    for (index, local) in body.locals.iter().take(function.params.len()).enumerate() {
        if local.ty != Ty::Unit {
            writeln!(
                out,
                "  store {} %arg{index}, ptr %slot{index}",
                llvm_type(&local.ty)?
            )
            .unwrap();
        }
    }
    writeln!(out, "  br label %bb{}", body.entry).unwrap();
    for (index, block) in body.blocks.iter().enumerate() {
        writeln!(out, "bb{index}:").unwrap();
        for statement in &block.statements {
            emit_statement(out, statement, body, signatures, &function.package)?;
        }
        emit_terminator(out, &block.terminator, body, ret)?;
    }
    out.push_str("}\n\n");
    Ok(())
}

fn emit_statement(
    out: &mut String,
    statement: &Statement,
    body: &MirBody,
    signatures: &Signatures,
    package: &str,
) -> Result<(), CodegenError> {
    match statement {
        Statement::Assign { place, value } => {
            if body.locals[place.local].ty == Ty::Unit {
                return Ok(());
            }
            let value = emit_rvalue(
                out,
                value,
                body,
                Some(&body.locals[place.local].ty),
                signatures,
                package,
            )?;
            let ty = llvm_type(&body.locals[place.local].ty)?;
            writeln!(out, "  store {ty} {value}, ptr %slot{}", place.local).unwrap();
        }
        Statement::Move { from, to }
        | Statement::Clone { from, to }
        | Statement::Retain { from, to } => {
            copy_place(out, *from, *to, body)?;
        }
        Statement::Evaluate(value) => {
            let _ = emit_rvalue(out, value, body, None, signatures, package)?;
        }
        Statement::Drop(_) | Statement::EnterTry { .. } | Statement::LeaveTry => {}
        Statement::ExtractVariantField { .. } | Statement::LoadIndex { .. } => {
            return Err(unsupported("aggregate extraction/indexing"));
        }
    }
    Ok(())
}

fn copy_place(
    out: &mut String,
    from: Place,
    to: Place,
    body: &MirBody,
) -> Result<(), CodegenError> {
    let ty = llvm_type(&body.locals[from.local].ty)?;
    let value = load_place(out, from, body)?;
    writeln!(out, "  store {ty} {value}, ptr %slot{}", to.local).unwrap();
    Ok(())
}

fn emit_terminator(
    out: &mut String,
    term: &Terminator,
    body: &MirBody,
    ret: &str,
) -> Result<(), CodegenError> {
    match term {
        Terminator::Goto { target } => writeln!(out, "  br label %bb{target}").unwrap(),
        Terminator::SwitchInt {
            condition,
            then_target,
            else_target,
        } => {
            let value = load_place(out, *condition, body)?;
            writeln!(
                out,
                "  br i1 {value}, label %bb{then_target}, label %bb{else_target}"
            )
            .unwrap();
        }
        Terminator::Return { value } => {
            if ret == "void" {
                out.push_str("  ret void\n");
            } else {
                let value = value.ok_or_else(|| unsupported("missing return value"))?;
                let loaded = load_place(out, value, body)?;
                writeln!(out, "  ret {ret} {loaded}").unwrap();
            }
        }
        Terminator::Unreachable => out.push_str("  unreachable\n"),
        Terminator::SwitchTag { .. }
        | Terminator::Await { .. }
        | Terminator::Throw { .. }
        | Terminator::Cancel => {
            return Err(unsupported(
                "tag switch, async, exception, or cancellation control flow",
            ));
        }
    }
    Ok(())
}

fn emit_rvalue(
    out: &mut String,
    value: &Rvalue,
    body: &MirBody,
    result_ty: Option<&Ty>,
    signatures: &Signatures,
    package: &str,
) -> Result<String, CodegenError> {
    match value {
        Rvalue::Use(place) => load_place(out, *place, body),
        Rvalue::ConstInt(value) => Ok(value.to_string()),
        Rvalue::ConstFloat(value) => Ok(format_float_constant(f64::from_bits(*value))),
        Rvalue::ConstBool(value) => Ok(if *value { "true" } else { "false" }.into()),
        Rvalue::ConstNull => Ok("null".into()),
        Rvalue::ConstString(_) => Err(unsupported("String constants")),
        Rvalue::Unary { op, operand } => {
            let value = load_place(out, *operand, body)?;
            let operand_ty = &body.locals[operand.local].ty;
            match (op, operand_ty) {
                (UnaryOp::Neg, Ty::Float) => Ok(format!("fneg double {value}")),
                (UnaryOp::Neg, _) => Ok(format!("sub i64 0, {value}")),
                (UnaryOp::Not, Ty::Bool) => Ok(format!("xor i1 {value}, true")),
                (UnaryOp::Not, _) => Err(unsupported("logical not on non-bool")),
            }
        }
        Rvalue::Binary { op, left, right } => {
            let left_ty = &body.locals[left.local].ty;
            let operand_ty = llvm_type(left_ty)?;
            let left = load_place(out, *left, body)?;
            let right = load_place(out, *right, body)?;
            let instruction = match (op, left_ty) {
                (BinaryOp::Add, Ty::Float) => "fadd",
                (BinaryOp::Sub, Ty::Float) => "fsub",
                (BinaryOp::Mul, Ty::Float) => "fmul",
                (BinaryOp::Div, Ty::Float) => "fdiv",
                (BinaryOp::Rem, Ty::Float) => "frem",
                (BinaryOp::Eq, Ty::Float) => "fcmp oeq",
                (BinaryOp::Ne, Ty::Float) => "fcmp one",
                (BinaryOp::Lt, Ty::Float) => "fcmp olt",
                (BinaryOp::Le, Ty::Float) => "fcmp ole",
                (BinaryOp::Gt, Ty::Float) => "fcmp ogt",
                (BinaryOp::Ge, Ty::Float) => "fcmp oge",
                (BinaryOp::Add, _) => "add",
                (BinaryOp::Sub, _) => "sub",
                (BinaryOp::Mul, _) => "mul",
                (BinaryOp::Div, _) => "sdiv",
                (BinaryOp::Rem, _) => "srem",
                (BinaryOp::And, _) => "and",
                (BinaryOp::Or, _) => "or",
                (BinaryOp::Eq, _) => "icmp eq",
                (BinaryOp::Ne, _) => "icmp ne",
                (BinaryOp::Lt, _) => "icmp slt",
                (BinaryOp::Le, _) => "icmp sle",
                (BinaryOp::Gt, _) => "icmp sgt",
                (BinaryOp::Ge, _) => "icmp sge",
                (BinaryOp::Coalesce, _) => return Err(unsupported("coalesce")),
            };
            let temp = next_temp(out);
            writeln!(out, "  {temp} = {instruction} {operand_ty} {left}, {right}").unwrap();
            Ok(temp)
        }
        Rvalue::Select {
            condition,
            then_value,
            else_value,
        } => {
            let condition = load_place(out, *condition, body)?;
            let then_value = load_place(out, *then_value, body)?;
            let else_value = load_place(out, *else_value, body)?;
            let selected_ty = llvm_type(result_ty.unwrap_or(&Ty::Int))?;
            let temp = next_temp(out);
            writeln!(
                out,
                "  {temp} = select i1 {condition}, {selected_ty} {then_value}, {selected_ty} {else_value}"
            )
            .unwrap();
            Ok(temp)
        }
        Rvalue::Call { target, args } => {
            let values = args
                .iter()
                .map(|place| load_place(out, *place, body))
                .collect::<Result<Vec<_>, _>>()?;
            let name = symbol_name(&target.package, &target.name);
            let (return_ty, parameter_tys) = signature_for(signatures, package, target)
                .ok_or_else(|| unsupported(&format!("call target {}", target.name)))?;
            if parameter_tys.len() != values.len() {
                return Err(unsupported(&format!("call arity for {}", target.name)));
            }
            let arguments = values
                .iter()
                .zip(parameter_tys)
                .map(|(value, ty)| Ok(format!("{} {value}", llvm_type(ty)?)))
                .collect::<Result<Vec<_>, CodegenError>>()?
                .join(", ");
            if *return_ty == Ty::Unit {
                writeln!(out, "  call void @{name}({arguments})").unwrap();
                return Ok(String::new());
            }
            let temp = next_temp(out);
            writeln!(
                out,
                "  {temp} = call {} @{name}({arguments})",
                llvm_type(return_ty)?
            )
            .unwrap();
            Ok(temp)
        }
        Rvalue::Intrinsic(_)
        | Rvalue::Unwrap { .. }
        | Rvalue::TypeTest { .. }
        | Rvalue::VariantTag { .. }
        | Rvalue::Length(_)
        | Rvalue::Index { .. }
        | Rvalue::Field { .. }
        | Rvalue::AsyncOp(_) => Err(unsupported("non-scalar MIR operation")),
    }
}

fn load_place(out: &mut String, place: Place, body: &MirBody) -> Result<String, CodegenError> {
    let ty = llvm_type(&body.locals[place.local].ty)?;
    if ty == "void" {
        return Err(unsupported("unit place"));
    }
    let temp = next_temp(out);
    writeln!(out, "  {temp} = load {ty}, ptr %slot{}", place.local).unwrap();
    Ok(temp)
}

fn next_temp(out: &str) -> String {
    format!(
        "%t{}",
        out.lines().filter(|line| line.contains(" = ")).count()
    )
}

pub(super) fn llvm_type(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Unit => Ok("void"),
        Ty::Int => Ok("i64"),
        Ty::Bool => Ok("i1"),
        Ty::Float => Ok("double"),
        Ty::Null => Ok("ptr"),
        _ => Err(unsupported(&format!("type {}", ty.display()))),
    }
}

fn signatures(program: &LoweredProgram) -> Signatures {
    program
        .checked()
        .functions
        .iter()
        .chain(program.checked().generic_functions.iter())
        .map(|function| {
            (
                (function.package.clone(), function.name.clone()),
                (
                    function.ret.ty.clone(),
                    function
                        .params
                        .iter()
                        .map(|param| param.ty.clone())
                        .collect(),
                ),
            )
        })
        .collect()
}

fn signature_for<'a>(
    signatures: &'a Signatures,
    package: &str,
    target: &aura_ir::mir::CallTarget,
) -> Option<&'a (Ty, Vec<Ty>)> {
    signatures
        .get(&(target.package.clone(), target.name.clone()))
        .or_else(|| signatures.get(&(package.to_owned(), target.name.clone())))
        .or_else(|| {
            signatures
                .iter()
                .find(|((_, name), _)| name == &target.name)
                .map(|(_, signature)| signature)
        })
}

pub(super) fn format_float_constant(value: f64) -> String {
    if value == 0.0 {
        "0.0".into()
    } else {
        format!("{value:.17}")
    }
}

fn symbol_name(package: &str, name: &str) -> String {
    let package = package
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    format!("aura_{}_{}", package, name)
}

fn unsupported(feature: &str) -> CodegenError {
    CodegenError::Configuration(format!(
        "LLVM backend does not support {feature} in the current MIR contract"
    ))
}
