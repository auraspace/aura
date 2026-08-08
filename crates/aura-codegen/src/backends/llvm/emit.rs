use std::collections::HashMap;
use std::fmt::Write as _;

use aura_ir::mir::{BinaryOp, MirBody, Place, Rvalue, Statement, Terminator, UnaryOp};
use aura_ir::{FunctionIr, LoweredProgram};
use aura_sema::Ty;

use crate::error::CodegenError;

type Signatures = HashMap<(String, String), (Ty, Vec<Ty>)>;

const STRING_RUNTIME: &str = r#"
%AuraLlvmString = type { i64, i64, [0 x i8] }
declare ptr @malloc(i64)
declare void @free(ptr)
declare i64 @strlen(ptr)
declare ptr @memcpy(ptr, ptr, i64)
declare i32 @strcmp(ptr, ptr)
declare i32 @puts(ptr)

define ptr @aura_llvm_str_alloc(i64 %len) {
entry:
  %size = add i64 %len, 17
  %value = call ptr @malloc(i64 %size)
  %refs = getelementptr %AuraLlvmString, ptr %value, i32 0, i32 0
  store i64 1, ptr %refs
  %length = getelementptr %AuraLlvmString, ptr %value, i32 0, i32 1
  store i64 %len, ptr %length
  %data = getelementptr %AuraLlvmString, ptr %value, i32 0, i32 2, i64 0
  %last = add i64 %len, 0
  %terminator = getelementptr i8, ptr %data, i64 %last
  store i8 0, ptr %terminator
  ret ptr %value
}

define ptr @aura_llvm_str_new(ptr %source) {
entry:
  %is_null = icmp eq ptr %source, null
  br i1 %is_null, label %empty, label %copy
empty:
  %empty_value = call ptr @aura_llvm_str_alloc(i64 0)
  ret ptr %empty_value
copy:
  %len = call i64 @strlen(ptr %source)
  %value = call ptr @aura_llvm_str_alloc(i64 %len)
  %data = getelementptr %AuraLlvmString, ptr %value, i32 0, i32 2, i64 0
  %copy_len = add i64 %len, 1
  %ignored = call ptr @memcpy(ptr %data, ptr %source, i64 %copy_len)
  ret ptr %value
}

define void @aura_llvm_str_retain(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %retain
retain:
  %refs = getelementptr %AuraLlvmString, ptr %value, i32 0, i32 0
  %current = load i64, ptr %refs
  %next = add i64 %current, 1
  store i64 %next, ptr %refs
  br label %done
done:
  ret void
}

define void @aura_llvm_str_release(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %release
release:
  %refs = getelementptr %AuraLlvmString, ptr %value, i32 0, i32 0
  %current = load i64, ptr %refs
  %next = sub i64 %current, 1
  store i64 %next, ptr %refs
  %last = icmp eq i64 %next, 0
  br i1 %last, label %destroy, label %done
destroy:
  call void @free(ptr %value)
  br label %done
done:
  ret void
}

define ptr @aura_llvm_str_data(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %empty, label %data
empty:
  ret ptr null
data:
  %result = getelementptr %AuraLlvmString, ptr %value, i32 0, i32 2, i64 0
  ret ptr %result
}

define i64 @aura_llvm_str_len(ptr %value) {
entry:
  %data = call ptr @aura_llvm_str_data(ptr %value)
  %is_null = icmp eq ptr %data, null
  br i1 %is_null, label %empty, label %measure
empty:
  ret i64 0
measure:
  %len = call i64 @strlen(ptr %data)
  ret i64 %len
}

define ptr @aura_llvm_str_concat(ptr %left, ptr %right) {
entry:
  %left_data = call ptr @aura_llvm_str_data(ptr %left)
  %right_data = call ptr @aura_llvm_str_data(ptr %right)
  %left_len = call i64 @aura_llvm_str_len(ptr %left)
  %right_len = call i64 @aura_llvm_str_len(ptr %right)
  %total = add i64 %left_len, %right_len
  %value = call ptr @aura_llvm_str_alloc(i64 %total)
  %data = getelementptr %AuraLlvmString, ptr %value, i32 0, i32 2, i64 0
  %left_end = getelementptr i8, ptr %data, i64 %left_len
  %left_copy = call ptr @memcpy(ptr %data, ptr %left_data, i64 %left_len)
  %right_copy = call ptr @memcpy(ptr %left_end, ptr %right_data, i64 %right_len)
  ret ptr %value
}

define i1 @aura_llvm_str_eq(ptr %left, ptr %right) {
entry:
  %left_data = call ptr @aura_llvm_str_data(ptr %left)
  %right_data = call ptr @aura_llvm_str_data(ptr %right)
  %left_null = icmp eq ptr %left_data, null
  %right_null = icmp eq ptr %right_data, null
  %both_null = and i1 %left_null, %right_null
  br i1 %both_null, label %equal, label %check_one
check_one:
  %one_null = xor i1 %left_null, %right_null
  br i1 %one_null, label %different, label %compare
compare:
  %result = call i32 @strcmp(ptr %left_data, ptr %right_data)
  %same = icmp eq i32 %result, 0
  br label %result_join
equal:
  br label %result_join
different:
  br label %result_join
result_join:
  %value = phi i1 [ true, %equal ], [ false, %different ], [ %same, %compare ]
  ret i1 %value
}

"#;

const ENUM_RUNTIME: &str = r#"
%AuraLlvmEnum = type { i64, i64, [0 x i64] }

define ptr @aura_llvm_enum_alloc(i64 %fields) {
entry:
  %field_bytes = mul i64 %fields, 8
  %size = add i64 %field_bytes, 24
  %value = call ptr @malloc(i64 %size)
  %refs = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 0
  store i64 1, ptr %refs
  ret ptr %value
}

define void @aura_llvm_enum_retain(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %retain
retain:
  %refs = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 0
  %current = load i64, ptr %refs
  %next = add i64 %current, 1
  store i64 %next, ptr %refs
  br label %done
done:
  ret void
}

define void @aura_llvm_enum_release(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %release
release:
  %refs = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 0
  %current = load i64, ptr %refs
  %next = sub i64 %current, 1
  store i64 %next, ptr %refs
  %last = icmp eq i64 %next, 0
  br i1 %last, label %destroy, label %done
destroy:
  call void @free(ptr %value)
  br label %done
done:
  ret void
}

"#;

struct EmitContext {
    signatures: Signatures,
    enum_variants: HashMap<String, EnumVariantInfo>,
    string_literals: Vec<String>,
}

#[derive(Clone)]
struct EnumVariantInfo {
    tag: i64,
    fields: Vec<(String, Ty)>,
}

pub fn emit_module(program: &LoweredProgram) -> Result<String, CodegenError> {
    validate_program(program)?;
    let mut module = String::from("; ModuleID = 'aura'\nsource_filename = \"aura\"\n\n");
    let mut context = EmitContext {
        signatures: signatures(program),
        enum_variants: enum_variants(program),
        string_literals: Vec::new(),
    };
    module.push_str(STRING_RUNTIME);
    module.push_str(ENUM_RUNTIME);
    for function in program
        .checked()
        .functions
        .iter()
        .chain(program.checked().generic_functions.iter())
    {
        let Some(body) = &function.body else {
            continue;
        };
        emit_function(&mut module, function, body, &mut context)?;
    }
    if let Some(function) = program
        .checked()
        .functions
        .iter()
        .find(|function| function.name == "main" && function.params.is_empty())
    {
        let symbol = symbol_name(&function.package, &function.name);
        match &function.ret.ty {
            Ty::Unit => module.push_str(&format!(
                "define i32 @main() {{\nentry:\n  call void @{symbol}()\n  ret i32 0\n}}\n"
            )),
            Ty::Int => module.push_str(&format!(
                "define i32 @main() {{\nentry:\n  %result = call i64 @{symbol}()\n  %status = trunc i64 %result to i32\n  ret i32 %status\n}}\n"
            )),
            _ => return Err(unsupported("main return type")),
        }
    }
    for (index, literal) in context.string_literals.iter().enumerate() {
        let bytes = literal.as_bytes();
        writeln!(
            module,
            "@.aura_str{index} = private unnamed_addr constant [{} x i8] c\"{}\", align 1",
            bytes.len() + 1,
            escape_llvm_bytes(bytes)
        )
        .unwrap();
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
    Ok(())
}

fn emit_function(
    out: &mut String,
    function: &FunctionIr,
    body: &MirBody,
    context: &mut EmitContext,
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
            let ty = llvm_type(&local.ty)?;
            writeln!(out, "  %slot{index} = alloca {ty}").unwrap();
            writeln!(
                out,
                "  store {ty} {}, ptr %slot{index}",
                llvm_zero(&local.ty)?
            )
            .unwrap();
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
            emit_statement(out, statement, body, context, &function.package)?;
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
    context: &mut EmitContext,
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
                package,
                context,
            )?;
            let ty = llvm_type(&body.locals[place.local].ty)?;
            writeln!(out, "  store {ty} {value}, ptr %slot{}", place.local).unwrap();
        }
        Statement::Move { from, to } => {
            copy_place(out, *from, *to, body, false)?;
        }
        Statement::Clone { from, to } | Statement::Retain { from, to } => {
            copy_place(out, *from, *to, body, true)?;
        }
        Statement::Evaluate(value) => {
            let _ = emit_rvalue(out, value, body, None, package, context)?;
        }
        Statement::Drop(place) => {
            if is_string_type(&body.locals[place.local].ty) {
                let value = load_place(out, *place, body)?;
                writeln!(out, "  call void @aura_llvm_str_release(ptr {value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            } else if is_enum_type(&body.locals[place.local].ty) {
                let value = load_place(out, *place, body)?;
                writeln!(out, "  call void @aura_llvm_enum_release(ptr {value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            }
        }
        Statement::EnterTry { .. } | Statement::LeaveTry => {}
        Statement::ExtractVariantField {
            operand,
            variant,
            field,
            to,
            ..
        } => {
            let object = load_place(out, *operand, body)?;
            let info = context
                .enum_variants
                .get(variant)
                .ok_or_else(|| unsupported(&format!("enum variant {variant}")))?;
            let (field_index, (_, field_ty)) = info
                .fields
                .iter()
                .enumerate()
                .find(|(_, (name, _))| name == field)
                .ok_or_else(|| unsupported(&format!("enum field {variant}.{field}")))?;
            if !matches!(field_ty, Ty::Int | Ty::Bool | Ty::Float)
                || body.locals[to.local].ty != *field_ty
            {
                return Err(unsupported("non-primitive enum payload"));
            }
            let address = next_temp(out);
            writeln!(
                out,
                "  {address} = getelementptr %AuraLlvmEnum, ptr {object}, i32 0, i32 2, i64 {field_index}"
            )
            .unwrap();
            let raw = next_temp(out);
            writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
            let value = match field_ty {
                Ty::Int => raw,
                Ty::Bool => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = trunc i64 {raw} to i1").unwrap();
                    value
                }
                Ty::Float => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = bitcast i64 {raw} to double").unwrap();
                    value
                }
                _ => unreachable!("validated primitive payload"),
            };
            let ty = llvm_type(field_ty)?;
            writeln!(out, "  store {ty} {value}, ptr %slot{}", to.local).unwrap();
        }
        Statement::LoadIndex {
            collection,
            index,
            to,
            ..
        } => {
            let collection_ty = &body.locals[collection.local].ty;
            if !is_string_type(collection_ty) || body.locals[to.local].ty != Ty::Int {
                return Err(unsupported("indexing non-String values"));
            }
            let value = load_string_byte(out, *collection, *index, body)?;
            writeln!(out, "  store i64 {value}, ptr %slot{}", to.local).unwrap();
        }
    }
    Ok(())
}

fn copy_place(
    out: &mut String,
    from: Place,
    to: Place,
    body: &MirBody,
    retain: bool,
) -> Result<(), CodegenError> {
    let ty = llvm_type(&body.locals[from.local].ty)?;
    let value = load_place(out, from, body)?;
    if retain && is_string_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_str_retain(ptr {value})").unwrap();
    } else if retain && is_enum_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_enum_retain(ptr {value})").unwrap();
    }
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
                let Some(value) = value else {
                    out.push_str("  unreachable\n");
                    return Ok(());
                };
                let loaded = load_place(out, *value, body)?;
                writeln!(out, "  ret {ret} {loaded}").unwrap();
            }
        }
        Terminator::Unreachable => out.push_str("  unreachable\n"),
        Terminator::SwitchTag {
            discriminant,
            targets,
            otherwise,
        } => {
            let value = load_place(out, *discriminant, body)?;
            writeln!(out, "  switch i64 {value}, label %bb{otherwise} [").unwrap();
            for (tag, target) in targets {
                writeln!(out, "    i64 {tag}, label %bb{target}").unwrap();
            }
            out.push_str("  ]\n");
        }
        Terminator::Await { .. } | Terminator::Throw { .. } | Terminator::Cancel => {
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
    package: &str,
    context: &mut EmitContext,
) -> Result<String, CodegenError> {
    match value {
        Rvalue::Use(place) => load_place(out, *place, body),
        Rvalue::ConstInt(value) => Ok(value.to_string()),
        Rvalue::ConstFloat(value) => Ok(format_float_constant(f64::from_bits(*value))),
        Rvalue::ConstBool(value) => Ok(if *value { "true" } else { "false" }.into()),
        Rvalue::ConstNull => Ok("null".into()),
        Rvalue::ConstString(value) => {
            let index = context
                .string_literals
                .iter()
                .position(|literal| literal == value)
                .unwrap_or_else(|| {
                    context.string_literals.push(value.clone());
                    context.string_literals.len() - 1
                });
            let length = context.string_literals[index].len() + 1;
            let address = next_temp(out);
            writeln!(
                out,
                "  {address} = getelementptr [{length} x i8], ptr @.aura_str{index}, i64 0, i64 0"
            )
            .unwrap();
            let value = next_temp(out);
            writeln!(
                out,
                "  {value} = call ptr @aura_llvm_str_new(ptr {address})"
            )
            .unwrap();
            Ok(value)
        }
        Rvalue::Unary { op, operand } => {
            let value = load_place(out, *operand, body)?;
            let operand_ty = &body.locals[operand.local].ty;
            match (op, operand_ty) {
                (UnaryOp::Neg, Ty::Float) => {
                    let temp = next_temp(out);
                    writeln!(out, "  {temp} = fneg double {value}").unwrap();
                    Ok(temp)
                }
                (UnaryOp::Neg, _) => {
                    let temp = next_temp(out);
                    writeln!(out, "  {temp} = sub i64 0, {value}").unwrap();
                    Ok(temp)
                }
                (UnaryOp::Not, Ty::Bool) => {
                    let temp = next_temp(out);
                    writeln!(out, "  {temp} = xor i1 {value}, true").unwrap();
                    Ok(temp)
                }
                (UnaryOp::Not, _) => Err(unsupported("logical not on non-bool")),
            }
        }
        Rvalue::Binary { op, left, right } => {
            let left_ty = &body.locals[left.local].ty;
            let left = load_place(out, *left, body)?;
            let right = load_place(out, *right, body)?;
            if is_string_type(left_ty) {
                let temp = next_temp(out);
                match op {
                    BinaryOp::Add => writeln!(
                        out,
                        "  {temp} = call ptr @aura_llvm_str_concat(ptr {left}, ptr {right})"
                    )
                    .unwrap(),
                    BinaryOp::Eq | BinaryOp::Ne => {
                        writeln!(
                            out,
                            "  {temp} = call i1 @aura_llvm_str_eq(ptr {left}, ptr {right})"
                        )
                        .unwrap();
                        if matches!(op, BinaryOp::Ne) {
                            let inverted = next_temp(out);
                            writeln!(out, "  {inverted} = xor i1 {temp}, true").unwrap();
                            return Ok(inverted);
                        }
                    }
                    BinaryOp::Coalesce => {
                        let present = next_temp(out);
                        writeln!(out, "  {present} = icmp ne ptr {left}, null").unwrap();
                        writeln!(
                            out,
                            "  {temp} = select i1 {present}, ptr {left}, ptr {right}"
                        )
                        .unwrap();
                    }
                    _ => return Err(unsupported("String binary operation")),
                }
                return Ok(temp);
            }
            let operand_ty = llvm_type(left_ty)?;
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
            if let Some(variant) = &target.variant {
                let info = context
                    .enum_variants
                    .get(variant)
                    .ok_or_else(|| unsupported(&format!("enum variant {variant}")))?;
                if info.fields.len() != args.len() {
                    return Err(unsupported(&format!("enum constructor {variant} arity")));
                }
                if info
                    .fields
                    .iter()
                    .any(|(_, ty)| !matches!(ty, Ty::Int | Ty::Bool | Ty::Float))
                {
                    return Err(unsupported("non-primitive enum payload"));
                }
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_enum_alloc(i64 {})",
                    args.len()
                )
                .unwrap();
                let tag_address = next_temp(out);
                writeln!(
                    out,
                    "  {tag_address} = getelementptr %AuraLlvmEnum, ptr {value}, i32 0, i32 1"
                )
                .unwrap();
                writeln!(out, "  store i64 {}, ptr {tag_address}", info.tag).unwrap();
                for (index, ((_, ty), argument)) in
                    info.fields.iter().zip(values.iter()).enumerate()
                {
                    let field_address = next_temp(out);
                    writeln!(
                        out,
                        "  {field_address} = getelementptr %AuraLlvmEnum, ptr {value}, i32 0, i32 2, i64 {index}"
                    )
                    .unwrap();
                    let raw = match ty {
                        Ty::Int => argument.clone(),
                        Ty::Bool => {
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = zext i1 {argument} to i64").unwrap();
                            raw
                        }
                        Ty::Float => {
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = bitcast double {argument} to i64").unwrap();
                            raw
                        }
                        _ => unreachable!("validated primitive payload"),
                    };
                    writeln!(out, "  store i64 {raw}, ptr {field_address}").unwrap();
                }
                return Ok(value);
            }
            if is_print_call(target) {
                if args.len() != 1 || !is_string_type(&body.locals[args[0].local].ty) {
                    return Err(unsupported(&format!("{} argument shape", target.name)));
                }
                let data = next_temp(out);
                writeln!(
                    out,
                    "  {data} = call ptr @aura_llvm_str_data(ptr {})",
                    values[0]
                )
                .unwrap();
                let call = next_temp(out);
                writeln!(out, "  {call} = call i32 @puts(ptr {data})").unwrap();
                return Ok(String::new());
            }
            let (return_ty, parameter_tys) = signature_for(&context.signatures, package, target)
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
        Rvalue::Unwrap { operand } => {
            let value = load_place(out, *operand, body)?;
            if is_string_type(&body.locals[operand.local].ty) {
                writeln!(out, "  call void @aura_llvm_str_retain(ptr {value})").unwrap();
            }
            Ok(value)
        }
        Rvalue::TypeTest { operand, .. } => {
            let value = load_place(out, *operand, body)?;
            if !is_string_type(&body.locals[operand.local].ty) {
                return Err(unsupported("type tests outside nullable String"));
            }
            let temp = next_temp(out);
            writeln!(out, "  {temp} = icmp ne ptr {value}, null").unwrap();
            Ok(temp)
        }
        Rvalue::Length(place) => {
            if !is_string_type(&body.locals[place.local].ty) {
                return Err(unsupported("length outside String"));
            }
            let value = load_place(out, *place, body)?;
            let temp = next_temp(out);
            writeln!(out, "  {temp} = call i64 @aura_llvm_str_len(ptr {value})").unwrap();
            Ok(temp)
        }
        Rvalue::Index { collection, index } => {
            if !is_string_type(&body.locals[collection.local].ty) {
                return Err(unsupported("indexing non-String values"));
            }
            load_string_byte(out, *collection, *index, body)
        }
        Rvalue::VariantTag { operand } => {
            let value = load_place(out, *operand, body)?;
            if !is_enum_type(&body.locals[operand.local].ty) {
                return Err(unsupported("variant tags outside unit enums"));
            }
            let address = next_temp(out);
            writeln!(
                out,
                "  {address} = getelementptr %AuraLlvmEnum, ptr {value}, i32 0, i32 1"
            )
            .unwrap();
            let tag = next_temp(out);
            writeln!(out, "  {tag} = load i64, ptr {address}").unwrap();
            Ok(tag)
        }
        Rvalue::Intrinsic(_) | Rvalue::Field { .. } | Rvalue::AsyncOp(_) => {
            Err(unsupported("non-scalar MIR operation"))
        }
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

fn load_string_byte(
    out: &mut String,
    collection: Place,
    index: Place,
    body: &MirBody,
) -> Result<String, CodegenError> {
    if body.locals[index.local].ty != Ty::Int {
        return Err(unsupported("String index is not Int"));
    }
    let value = load_place(out, collection, body)?;
    let offset = load_place(out, index, body)?;
    let data = next_temp(out);
    writeln!(out, "  {data} = call ptr @aura_llvm_str_data(ptr {value})").unwrap();
    let address = next_temp(out);
    writeln!(
        out,
        "  {address} = getelementptr i8, ptr {data}, i64 {offset}"
    )
    .unwrap();
    let byte = next_temp(out);
    writeln!(out, "  {byte} = load i8, ptr {address}").unwrap();
    let result = next_temp(out);
    writeln!(out, "  {result} = zext i8 {byte} to i64").unwrap();
    Ok(result)
}

fn llvm_zero(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Unit => Err(unsupported("unit local")),
        Ty::Bool => Ok("false"),
        Ty::Float => Ok("0.0"),
        Ty::Int => Ok("0"),
        Ty::String | Ty::Null => Ok("null"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::String) => Ok("null"),
        Ty::Enum(_) | Ty::EnumApp { .. } => Ok("null"),
        _ => Err(unsupported(&format!("type {}", ty.display()))),
    }
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
        Ty::String | Ty::Null => Ok("ptr"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::String) => Ok("ptr"),
        Ty::Enum(_) | Ty::EnumApp { .. } => Ok("ptr"),
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

fn is_string_type(ty: &Ty) -> bool {
    match ty {
        Ty::String => true,
        Ty::Nullable(inner) => matches!(inner.as_ref(), Ty::String),
        _ => false,
    }
}

fn is_enum_type(ty: &Ty) -> bool {
    matches!(ty, Ty::Enum(_) | Ty::EnumApp { .. })
}

fn enum_variants(program: &LoweredProgram) -> HashMap<String, EnumVariantInfo> {
    program
        .source()
        .enums
        .iter()
        .flat_map(|enum_decl| {
            enum_decl.variants.iter().map(|variant| {
                (
                    variant.name.clone(),
                    EnumVariantInfo {
                        tag: variant.tag as i64,
                        fields: variant.fields.clone(),
                    },
                )
            })
        })
        .collect()
}

fn is_print_call(target: &aura_ir::mir::CallTarget) -> bool {
    matches!(
        target.name.as_str(),
        "print" | "println" | "eprint" | "eprintln"
    ) && (target.package.is_empty() || target.package.starts_with("std."))
}

pub(super) fn format_float_constant(value: f64) -> String {
    if value == 0.0 {
        "0.0".into()
    } else {
        format!("{value:.17}")
    }
}

fn escape_llvm_bytes(bytes: &[u8]) -> String {
    let mut escaped = String::new();
    for &byte in bytes {
        match byte {
            b'\\' => escaped.push_str("\\5C"),
            b'"' => escaped.push_str("\\22"),
            0x20..=0x7e => escaped.push(byte as char),
            _ => write!(escaped, "\\{byte:02X}").unwrap(),
        }
    }
    escaped.push_str("\\00");
    escaped
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
