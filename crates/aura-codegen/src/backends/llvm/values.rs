use super::*;

use aura_ir::intrinsic_registry::{lookup as lookup_std_intrinsic, Intrinsic as StdIntrinsic};

pub(super) fn optional_literal(result_ty: Option<&Ty>, llvm_value_ty: &str, value: &str) -> String {
    match result_ty {
        Some(Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Int | Ty::Bool | Ty::Float) => {
            format!("{{ i1 true, {llvm_value_ty} {value} }}")
        }
        _ => value.to_owned(),
    }
}

pub(super) fn nullable_zero_value(result_ty: Option<&Ty>) -> Option<&'static str> {
    match result_ty {
        Some(Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Int) => {
            Some("{ i1 false, i64 0 }")
        }
        Some(Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Bool) => {
            Some("{ i1 false, i1 false }")
        }
        Some(Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Float) => {
            Some("{ i1 false, double 0.0 }")
        }
        _ => None,
    }
}

pub(super) fn build_optional_value(
    out: &mut String,
    llvm_ty: &str,
    value_ty: &str,
    value: &str,
) -> String {
    let present = next_temp(out);
    writeln!(out, "  {present} = insertvalue {llvm_ty} undef, i1 true, 0").unwrap();
    let result = next_temp(out);
    writeln!(
        out,
        "  {result} = insertvalue {llvm_ty} {present}, {value_ty} {value}, 1"
    )
    .unwrap();
    result
}

pub(super) fn extract_optional_payload(
    out: &mut String,
    value: &str,
    ty: &Ty,
) -> Result<String, CodegenError> {
    if !is_tagged_nullable(ty) {
        return Ok(value.to_owned());
    }
    let payload = next_temp(out);
    writeln!(
        out,
        "  {payload} = extractvalue {} {value}, 1",
        llvm_type(ty)?
    )
    .unwrap();
    Ok(payload)
}

pub(super) fn emit_use_value(
    out: &mut String,
    place: Place,
    body: &MirBody,
    result_ty: Option<&Ty>,
) -> Result<String, CodegenError> {
    let source_ty = &body.locals[place.local].ty;
    let value = load_place(out, place, body)?;
    match (source_ty, result_ty) {
        (Ty::Int, Some(Ty::Nullable(inner))) if **inner == Ty::Int => {
            Ok(build_optional_value(out, "%AuraLlvmOptInt", "i64", &value))
        }
        (Ty::Bool, Some(Ty::Nullable(inner))) if **inner == Ty::Bool => {
            Ok(build_optional_value(out, "%AuraLlvmOptBool", "i1", &value))
        }
        (Ty::Float, Some(Ty::Nullable(inner))) if **inner == Ty::Float => Ok(build_optional_value(
            out,
            "%AuraLlvmOptFloat",
            "double",
            &value,
        )),
        (Ty::Nullable(inner), Some(destination))
            if inner.as_ref() == destination && is_tagged_nullable(source_ty) =>
        {
            let value_slot = next_temp(out);
            writeln!(
                out,
                "  {value_slot} = extractvalue {} {value}, 1",
                llvm_type(source_ty)?
            )
            .unwrap();
            Ok(value_slot)
        }
        _ => Ok(value),
    }
}

pub(super) fn coerce_llvm_argument(
    out: &mut String,
    value: &str,
    source_ty: &Ty,
    expected_ty: &Ty,
) -> Result<String, CodegenError> {
    if source_ty == expected_ty {
        return Ok(value.to_owned());
    }
    if types_compatible(source_ty, expected_ty) {
        return Ok(value.to_owned());
    }
    if is_pointer_abi_type(source_ty) && is_pointer_abi_type(expected_ty) {
        // Nominal class conversions have already been checked by sema; all
        // heap values cross the LLVM ABI as opaque pointers.
        return Ok(value.to_owned());
    }
    match (source_ty, expected_ty) {
        (Ty::Null, Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Int) => {
            Ok("{ i1 false, i64 0 }".into())
        }
        (Ty::Null, Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Bool) => {
            Ok("{ i1 false, i1 false }".into())
        }
        (Ty::Null, Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Float) => {
            Ok("{ i1 false, double 0.0 }".into())
        }
        (Ty::Null, Ty::Nullable(_)) => Ok("null".into()),
        (
            Ty::Null,
            Ty::Class(_)
            | Ty::ClassApp { .. }
            | Ty::Enum(_)
            | Ty::EnumApp { .. }
            | Ty::Interface(_)
            | Ty::InterfaceApp { .. }
            | Ty::ForeignHandle(_)
            | Ty::Channel(_),
        ) => Ok("null".into()),
        (Ty::Int, Ty::Nullable(inner)) if **inner == Ty::Int => {
            Ok(build_optional_value(out, "%AuraLlvmOptInt", "i64", value))
        }
        (Ty::Bool, Ty::Nullable(inner)) if **inner == Ty::Bool => {
            Ok(build_optional_value(out, "%AuraLlvmOptBool", "i1", value))
        }
        (Ty::Float, Ty::Nullable(inner)) if **inner == Ty::Float => Ok(build_optional_value(
            out,
            "%AuraLlvmOptFloat",
            "double",
            value,
        )),
        (Ty::Nullable(source), Ty::Nullable(expected))
            if types_compatible(source.as_ref(), expected.as_ref()) =>
        {
            Ok(value.to_owned())
        }
        (source, Ty::Nullable(inner)) if types_compatible(source, inner.as_ref()) => {
            Ok(value.to_owned())
        }
        (Ty::Nullable(inner), destination)
            if types_compatible(inner.as_ref(), destination) && !is_tagged_nullable(source_ty) =>
        {
            Ok(value.to_owned())
        }
        (Ty::Nullable(inner), destination) if types_compatible(inner.as_ref(), destination) => {
            let payload = next_temp(out);
            writeln!(
                out,
                "  {payload} = extractvalue {} {value}, 1",
                llvm_type(source_ty)?
            )
            .unwrap();
            Ok(payload)
        }
        _ => Err(unsupported(&format!(
            "argument conversion from {} to {}",
            source_ty.display(),
            expected_ty.display()
        ))),
    }
}

pub(super) fn emit_rvalue(
    out: &mut String,
    value: &Rvalue,
    body: &MirBody,
    result_ty: Option<&Ty>,
    package: &str,
    context: &mut EmitContext,
) -> Result<String, CodegenError> {
    match value {
        Rvalue::Use(place) => emit_use_value(out, *place, body, result_ty),
        Rvalue::ConstInt(value) => Ok(optional_literal(result_ty, "i64", &value.to_string())),
        Rvalue::ConstFloat(value) => Ok(optional_literal(
            result_ty,
            "double",
            &format_float_constant(f64::from_bits(*value)),
        )),
        Rvalue::ConstBool(value) => Ok(optional_literal(
            result_ty,
            "i1",
            if *value { "true" } else { "false" },
        )),
        Rvalue::ConstNull => Ok(nullable_zero_value(result_ty).unwrap_or("null").into()),
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
        Rvalue::Function { name, captures } => {
            let env = if captures.is_empty() {
                "null".to_string()
            } else {
                let env_ty = closure_env_name(name);
                let size_ptr = next_temp(out);
                writeln!(
                    out,
                    "  {size_ptr} = getelementptr %{env_ty}, ptr null, i32 1"
                )
                .unwrap();
                let size = next_temp(out);
                writeln!(out, "  {size} = ptrtoint ptr {size_ptr} to i64").unwrap();
                let env = next_temp(out);
                writeln!(out, "  {env} = call ptr @malloc(i64 {size})").unwrap();
                let drop_slot = next_temp(out);
                writeln!(
                    out,
                    "  {drop_slot} = getelementptr %{env_ty}, ptr {env}, i32 0, i32 0"
                )
                .unwrap();
                let drop = if captures.iter().any(|capture| {
                    capture.by_ref
                        || capture.ty == Ty::String
                        || is_array_type(&capture.ty)
                        || matches!(
                            capture.ty,
                            Ty::Class(_)
                                | Ty::ClassApp { .. }
                                | Ty::Enum(_)
                                | Ty::EnumApp { .. }
                                | Ty::Task(_)
                                | Ty::TaskHandle(_)
                                | Ty::Channel(_)
                        )
                }) {
                    format!("@{}", closure_drop_name(name))
                } else {
                    "null".to_string()
                };
                writeln!(out, "  store ptr {drop}, ptr {drop_slot}").unwrap();
                let refs_slot = next_temp(out);
                writeln!(
                    out,
                    "  {refs_slot} = getelementptr %{env_ty}, ptr {env}, i32 0, i32 1"
                )
                .unwrap();
                writeln!(out, "  store i32 1, ptr {refs_slot}").unwrap();
                for (index, capture) in captures.iter().enumerate() {
                    let value = if capture.by_ref {
                        let value = next_temp(out);
                        writeln!(
                            out,
                            "  {value} = load ptr, ptr %slot{}",
                            capture.source.local
                        )
                        .unwrap();
                        value
                    } else {
                        load_place(out, capture.source, body)?
                    };
                    let stored = if capture.by_ref {
                        value.clone()
                    } else if is_array_type(&capture.ty) {
                        let cloned = next_temp(out);
                        writeln!(
                            out,
                            "  {cloned} = call ptr @aura_llvm_array_clone(ptr {value})"
                        )
                        .unwrap();
                        cloned
                    } else {
                        value.clone()
                    };
                    if capture.by_ref {
                        let helper = match capture.ty {
                            Ty::Int => "aura_box_i64_retain",
                            Ty::Bool => "aura_box_bool_retain",
                            Ty::Float => "aura_box_f64_retain",
                            Ty::String => "aura_box_str_retain",
                            Ty::Fun { .. }
                            | Ty::Class(_)
                            | Ty::ClassApp { .. }
                            | Ty::Interface(_)
                            | Ty::InterfaceApp { .. }
                            | Ty::Enum(_)
                            | Ty::EnumApp { .. }
                            | Ty::Task(_)
                            | Ty::TaskHandle(_)
                            | Ty::Channel(_) => "aura_box_ptr_retain",
                            _ => return Err(unsupported("mutable capture type")),
                        };
                        writeln!(out, "  call void @{helper}(ptr {value})").unwrap();
                    } else if capture.ty == Ty::String {
                        writeln!(out, "  call void @aura_llvm_str_retain(ptr {value})").unwrap();
                    } else if matches!(capture.ty, Ty::Fun { .. }) {
                        writeln!(
                            out,
                            "  call void @aura_llvm_fun_retain(%AuraLlvmFun {value})"
                        )
                        .unwrap();
                    } else if !is_array_type(&capture.ty)
                        && matches!(
                            capture.ty,
                            Ty::Class(_)
                                | Ty::ClassApp { .. }
                                | Ty::Interface(_)
                                | Ty::InterfaceApp { .. }
                        )
                    {
                        writeln!(out, "  call void @aura_llvm_class_retain(ptr {value})").unwrap();
                    } else if matches!(capture.ty, Ty::Enum(_) | Ty::EnumApp { .. }) {
                        writeln!(out, "  call void @aura_llvm_enum_retain(ptr {value})").unwrap();
                    } else if matches!(capture.ty, Ty::Task(_) | Ty::TaskHandle(_)) {
                        let executor = next_temp(out);
                        writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                        writeln!(out, "  call i32 @aura_task_executor_retain_payload(ptr {executor}, ptr {value})").unwrap();
                    } else if matches!(capture.ty, Ty::Channel(_)) {
                        writeln!(out, "  call i32 @aura_task_channel_retain(ptr {value})").unwrap();
                    }
                    let address = next_temp(out);
                    writeln!(
                        out,
                        "  {address} = getelementptr %{env_ty}, ptr {env}, i32 0, i32 {}",
                        index + 2
                    )
                    .unwrap();
                    writeln!(
                        out,
                        "  store {field_ty} {stored}, ptr {address}",
                        field_ty = if capture.by_ref {
                            "ptr"
                        } else {
                            llvm_type(&capture.ty)?
                        }
                    )
                    .unwrap();
                }
                env
            };
            let function = format!("@{}", symbol_name(package, name));
            let with_env = next_temp(out);
            writeln!(
                out,
                "  {with_env} = insertvalue %AuraLlvmFun undef, ptr {env}, 0"
            )
            .unwrap();
            let result = next_temp(out);
            writeln!(
                out,
                "  {result} = insertvalue %AuraLlvmFun {with_env}, ptr {function}, 1"
            )
            .unwrap();
            Ok(result)
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
            let right_ty = &body.locals[right.local].ty;
            let left = load_place(out, *left, body)?;
            let right = load_place(out, *right, body)?;
            if is_tagged_nullable(left_ty) {
                let Ty::Nullable(inner) = left_ty else {
                    unreachable!("tagged nullable type checked above")
                };
                let present = next_temp(out);
                writeln!(
                    out,
                    "  {present} = extractvalue {} {left}, 0",
                    llvm_type(left_ty)?
                )
                .unwrap();
                if matches!(op, BinaryOp::Coalesce) {
                    let payload = next_temp(out);
                    writeln!(
                        out,
                        "  {payload} = extractvalue {} {left}, 1",
                        llvm_type(left_ty)?
                    )
                    .unwrap();
                    let result_ty = llvm_type(result_ty.unwrap_or(inner))?;
                    let fallback = if *right_ty == **inner {
                        right
                    } else {
                        return Err(unsupported("nullable coalesce operand type"));
                    };
                    let selected = next_temp(out);
                    writeln!(
                        out,
                        "  {selected} = select i1 {present}, {result_ty} {payload}, {result_ty} {fallback}"
                    )
                    .unwrap();
                    return Ok(selected);
                }
                if matches!(right_ty, Ty::Null) {
                    let value = next_temp(out);
                    let instruction = if matches!(op, BinaryOp::Eq) {
                        "icmp eq"
                    } else {
                        "icmp ne"
                    };
                    writeln!(out, "  {value} = {instruction} i1 {present}, false").unwrap();
                    return Ok(value);
                }
                let payload = next_temp(out);
                writeln!(
                    out,
                    "  {payload} = extractvalue {} {left}, 1",
                    llvm_type(left_ty)?
                )
                .unwrap();
                let compare_ty = if *right_ty == **inner {
                    inner.as_ref()
                } else {
                    return Err(unsupported("nullable binary operand type"));
                };
                let operand_ty = llvm_type(compare_ty)?;
                let instruction = match (op, compare_ty) {
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
                    (BinaryOp::Eq, _) => "icmp eq",
                    (BinaryOp::Ne, _) => "icmp ne",
                    (BinaryOp::Lt, _) => "icmp slt",
                    (BinaryOp::Le, _) => "icmp sle",
                    (BinaryOp::Gt, _) => "icmp sgt",
                    (BinaryOp::Ge, _) => "icmp sge",
                    _ => return Err(unsupported("nullable binary operation")),
                };
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = {instruction} {operand_ty} {payload}, {right}"
                )
                .unwrap();
                return Ok(value);
            }
            if is_string_type(left_ty) {
                match op {
                    BinaryOp::Add => {
                        let right = if is_string_type(right_ty) {
                            right
                        } else {
                            emit_to_string(out, &right, right_ty)?
                        };
                        let temp = next_temp(out);
                        writeln!(
                            out,
                            "  {temp} = call ptr @aura_llvm_str_concat(ptr {left}, ptr {right})"
                        )
                        .unwrap();
                        return Ok(temp);
                    }
                    BinaryOp::Eq | BinaryOp::Ne => {
                        let temp = next_temp(out);
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
                        return Ok(temp);
                    }
                    BinaryOp::Coalesce => {
                        let temp = next_temp(out);
                        let present = next_temp(out);
                        writeln!(out, "  {present} = icmp ne ptr {left}, null").unwrap();
                        writeln!(
                            out,
                            "  {temp} = select i1 {present}, ptr {left}, ptr {right}"
                        )
                        .unwrap();
                        return Ok(temp);
                    }
                    _ => return Err(unsupported("String binary operation")),
                }
            }
            if matches!(op, BinaryOp::Add) && is_string_type(right_ty) {
                let left = if is_string_type(left_ty) {
                    left
                } else {
                    emit_to_string(out, &left, left_ty)?
                };
                let temp = next_temp(out);
                writeln!(
                    out,
                    "  {temp} = call ptr @aura_llvm_str_concat(ptr {left}, ptr {right})"
                )
                .unwrap();
                return Ok(temp);
            }
            if is_class_type(left_ty) && matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                let temp = next_temp(out);
                let instruction = if matches!(op, BinaryOp::Eq) {
                    "eq"
                } else {
                    "ne"
                };
                writeln!(out, "  {temp} = icmp {instruction} ptr {left}, {right}").unwrap();
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
        Rvalue::CallIndirect { callee, args } => {
            let Ty::Fun { params, ret } = &body.locals[callee.local].ty else {
                return Err(unsupported("indirect call through non-function value"));
            };
            if params.len() != args.len() {
                return Err(unsupported("indirect call arity"));
            }
            let callee = load_place(out, *callee, body)?;
            let env = next_temp(out);
            writeln!(out, "  {env} = extractvalue %AuraLlvmFun {callee}, 0").unwrap();
            let function = next_temp(out);
            writeln!(out, "  {function} = extractvalue %AuraLlvmFun {callee}, 1").unwrap();
            let values = args
                .iter()
                .map(|place| load_place(out, *place, body))
                .collect::<Result<Vec<_>, _>>()?;
            for (value, ty) in values.iter().zip(params) {
                if is_pointer_value_type(ty) {
                    retain_pointer_value(out, value, ty)?;
                } else if matches!(ty, Ty::Fun { .. }) {
                    writeln!(
                        out,
                        "  call void @aura_llvm_fun_retain(%AuraLlvmFun {value})"
                    )
                    .unwrap();
                }
            }
            let mut arguments = values
                .iter()
                .zip(params)
                .map(|(value, ty)| Ok(format!("{} {value}", llvm_type(ty)?)))
                .collect::<Result<Vec<_>, CodegenError>>()?
                .join(", ");
            if arguments.is_empty() {
                arguments = format!("ptr {env}");
            } else {
                arguments = format!("ptr {env}, {arguments}");
            }
            let return_ty = result_ty.unwrap_or(ret.as_ref());
            let llvm_return_ty = llvm_type(return_ty)?;
            if *return_ty == Ty::Unit {
                writeln!(out, "  call void {function}({arguments})").unwrap();
                Ok(String::new())
            } else {
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call {llvm_return_ty} {function}({arguments})"
                )
                .unwrap();
                Ok(value)
            }
        }
        Rvalue::Call { target, args } => {
            let values = args
                .iter()
                .map(|place| load_place(out, *place, body))
                .collect::<Result<Vec<_>, _>>()?;
            let std_intrinsic =
                lookup_std_intrinsic(&target.package, &target.name).map(|spec| spec.intrinsic);
            if target.name == "assert" && values.len() == 1 {
                if body.locals[args[0].local].ty != Ty::Bool {
                    return Err(unsupported("assert condition type"));
                }
                writeln!(out, "  call void @aura_llvm_assert(i1 {})", values[0]).unwrap();
                return Ok(String::new());
            }
            if target.name == "assert_eq" && values.len() == 2 {
                let left_ty = &body.locals[args[0].local].ty;
                let right_ty = &body.locals[args[1].local].ty;
                let (left, right, compare_ty) = match (left_ty, right_ty) {
                    (Ty::Nullable(inner), ty) if inner.as_ref() == ty => {
                        let payload = extract_optional_payload(out, values[0].as_str(), left_ty)?;
                        (payload, values[1].clone(), inner.as_ref())
                    }
                    (ty, Ty::Nullable(inner)) if inner.as_ref() == ty => {
                        let payload = extract_optional_payload(out, values[1].as_str(), right_ty)?;
                        (values[0].clone(), payload, left_ty)
                    }
                    (left, right) if left == right => (values[0].clone(), values[1].clone(), left),
                    _ => return Err(unsupported("assert_eq operand types")),
                };
                let equal = emit_equality(out, &left, &right, compare_ty)?;
                writeln!(out, "  call void @aura_llvm_assert(i1 {equal})").unwrap();
                return Ok(String::new());
            }
            if !args.is_empty()
                && !values.is_empty()
                && matches!(
                    body.locals[args[0].local].ty,
                    Ty::Class(_) | Ty::ClassApp { .. }
                )
            {
                let receiver_ty = &body.locals[args[0].local].ty;
                let sync_name = class_type_name(receiver_ty)
                    .map(|name| name.split('@').next().unwrap_or(name).to_owned());
                let is_atomic = sync_name
                    .as_deref()
                    .is_some_and(|name| name.ends_with("AtomicInt"));
                let is_mutex = sync_name
                    .as_deref()
                    .is_some_and(|name| name.ends_with("Mutex"));
                let is_rwlock = sync_name
                    .as_deref()
                    .is_some_and(|name| name.ends_with("RwLock"));
                let is_once = sync_name
                    .as_deref()
                    .is_some_and(|name| name.ends_with("Once"));
                if is_atomic || is_mutex || is_rwlock || is_once {
                    let name = class_type_name(receiver_ty).unwrap();
                    let fields = class_fields(context, name, class_type_args(receiver_ty))
                        .ok_or_else(|| unsupported("std.sync class layout"))?;
                    let field_name = if is_atomic { "value" } else { "state" };
                    let field_index = fields
                        .iter()
                        .position(|(field, ty)| field == field_name && *ty == Ty::Int)
                        .ok_or_else(|| unsupported("std.sync state field"))?;
                    let address = next_temp(out);
                    writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {field_index}", values[0]).unwrap();
                    let call = |out: &mut String, helper: &str, arguments: &str| -> String {
                        let value = next_temp(out);
                        writeln!(
                            out,
                            "  {value} = call i64 @{helper}(ptr {address}{arguments})"
                        )
                        .unwrap();
                        value
                    };
                    if is_atomic {
                        match target.name.as_str() {
                            "load" if values.len() == 1 => {
                                return Ok(call(out, "aura_llvm_sync_load", ""))
                            }
                            "store" if values.len() == 2 => {
                                writeln!(
                                    out,
                                    "  call void @aura_llvm_sync_store(ptr {address}, i64 {})",
                                    values[1]
                                )
                                .unwrap();
                                return Ok(String::new());
                            }
                            "fetchAdd" if values.len() == 2 => {
                                return Ok(call(
                                    out,
                                    "aura_llvm_sync_fetch_add",
                                    &format!(", i64 {}", values[1]),
                                ))
                            }
                            "compareExchange" if values.len() == 3 => {
                                let raw = next_temp(out);
                                writeln!(out, "  {raw} = call i32 @aura_llvm_sync_compare_exchange(ptr {address}, i64 {}, i64 {})", values[1], values[2]).unwrap();
                                let result = next_temp(out);
                                writeln!(out, "  {result} = icmp ne i32 {raw}, 0").unwrap();
                                return Ok(result);
                            }
                            _ => {}
                        }
                    } else if is_mutex {
                        match target.name.as_str() {
                            "tryLock" if values.len() == 1 => {
                                let raw = next_temp(out);
                                writeln!(
                                    out,
                                    "  {raw} = call i32 @aura_llvm_sync_try_lock(ptr {address})"
                                )
                                .unwrap();
                                let result = next_temp(out);
                                writeln!(out, "  {result} = icmp ne i32 {raw}, 0").unwrap();
                                return Ok(result);
                            }
                            "unlock" if values.len() == 1 => {
                                writeln!(out, "  call void @aura_llvm_sync_unlock(ptr {address})")
                                    .unwrap();
                                return Ok(String::new());
                            }
                            "isLocked" if values.len() == 1 => {
                                let raw = next_temp(out);
                                writeln!(
                                    out,
                                    "  {raw} = call i32 @aura_llvm_sync_is_locked(ptr {address})"
                                )
                                .unwrap();
                                let result = next_temp(out);
                                writeln!(out, "  {result} = icmp ne i32 {raw}, 0").unwrap();
                                return Ok(result);
                            }
                            _ => {}
                        }
                    } else if is_once {
                        match target.name.as_str() {
                            "tryEnter" if values.len() == 1 => {
                                let raw = next_temp(out);
                                writeln!(
                                    out,
                                    "  {raw} = call i32 @aura_llvm_sync_try_lock(ptr {address})"
                                )
                                .unwrap();
                                let result = next_temp(out);
                                writeln!(out, "  {result} = icmp ne i32 {raw}, 0").unwrap();
                                return Ok(result);
                            }
                            "isDone" if values.len() == 1 => {
                                let raw = next_temp(out);
                                writeln!(
                                    out,
                                    "  {raw} = call i32 @aura_llvm_sync_is_locked(ptr {address})"
                                )
                                .unwrap();
                                let result = next_temp(out);
                                writeln!(out, "  {result} = icmp ne i32 {raw}, 0").unwrap();
                                return Ok(result);
                            }
                            _ => {}
                        }
                    } else if is_rwlock {
                        match target.name.as_str() {
                            "tryRead" => {
                                let raw = next_temp(out);
                                writeln!(
                                    out,
                                    "  {raw} = call i32 @aura_llvm_sync_try_read(ptr {address})"
                                )
                                .unwrap();
                                let result = next_temp(out);
                                writeln!(out, "  {result} = icmp ne i32 {raw}, 0").unwrap();
                                return Ok(result);
                            }
                            "tryWrite" => {
                                let raw = next_temp(out);
                                writeln!(
                                    out,
                                    "  {raw} = call i32 @aura_llvm_sync_try_write(ptr {address})"
                                )
                                .unwrap();
                                let result = next_temp(out);
                                writeln!(out, "  {result} = icmp ne i32 {raw}, 0").unwrap();
                                return Ok(result);
                            }
                            "unlockRead" => {
                                writeln!(
                                    out,
                                    "  call void @aura_llvm_sync_unlock_read(ptr {address})"
                                )
                                .unwrap();
                                return Ok(String::new());
                            }
                            "unlockWrite" => {
                                writeln!(
                                    out,
                                    "  call void @aura_llvm_sync_unlock_write(ptr {address})"
                                )
                                .unwrap();
                                return Ok(String::new());
                            }
                            "readerCount" => {
                                return Ok(call(out, "aura_llvm_sync_reader_count", ""));
                            }
                            "isWriteLocked" => {
                                let raw = next_temp(out);
                                writeln!(out, "  {raw} = call i32 @aura_llvm_sync_is_write_locked(ptr {address})").unwrap();
                                let result = next_temp(out);
                                writeln!(out, "  {result} = icmp ne i32 {raw}, 0").unwrap();
                                return Ok(result);
                            }
                            _ => {}
                        }
                    }
                }
            }
            if target.name == "lazy"
                && std_intrinsic == Some(StdIntrinsic::SyncLazy)
                && values.len() == 1
                && matches!(&body.locals[args[0].local].ty, Ty::Fun { params, ret } if params.is_empty() && ret.as_ref() == &Ty::Int)
                && matches!(result_ty, Some(Ty::Class(_) | Ty::ClassApp { .. }))
            {
                let lazy_ty = result_ty.ok_or_else(|| unsupported("std.sync Lazy result type"))?;
                let lazy_name = class_type_name(lazy_ty)
                    .ok_or_else(|| unsupported("std.sync Lazy class type"))?;
                let fields = class_fields(context, lazy_name, class_type_args(lazy_ty))
                    .ok_or_else(|| unsupported("std.sync Lazy class layout"))?;
                let handle_index = fields
                    .iter()
                    .position(|(name, ty)| name == "runtimeHandle" && *ty == Ty::Int)
                    .ok_or_else(|| unsupported("std.sync Lazy.runtimeHandle field"))?;
                let layout_name = context
                    .classes
                    .keys()
                    .find(|name| name.split('@').next() == Some(lazy_name))
                    .cloned()
                    .ok_or_else(|| unsupported("std.sync Lazy class type id"))?;
                let type_id = context
                    .class_type_ids
                    .get(&layout_name)
                    .copied()
                    .ok_or_else(|| unsupported("std.sync Lazy class type id"))?;
                let env = next_temp(out);
                writeln!(out, "  {env} = extractvalue %AuraLlvmFun {}, 0", values[0]).unwrap();
                let function = next_temp(out);
                writeln!(
                    out,
                    "  {function} = extractvalue %AuraLlvmFun {}, 1",
                    values[0]
                )
                .unwrap();
                let cell = next_temp(out);
                writeln!(
                    out,
                    "  {cell} = call ptr @aura_llvm_lazy_int_new(ptr {env}, ptr {function})"
                )
                .unwrap();
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_class_alloc(i64 {}, i64 {type_id})",
                    fields.len()
                )
                .unwrap();
                let address = next_temp(out);
                writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {value}, i32 0, i32 1, i64 {handle_index}").unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = ptrtoint ptr {cell} to i64").unwrap();
                writeln!(out, "  store i64 {raw}, ptr {address}").unwrap();
                return Ok(value);
            }
            if std_intrinsic == Some(StdIntrinsic::TaskScope) && values.len() == 1 {
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let scope = next_temp(out);
                writeln!(
                    out,
                    "  {scope} = call ptr @aura_task_scope_begin(ptr {executor})"
                )
                .unwrap();
                let exception_id = out.lines().count();
                writeln!(
                    out,
                    "  %scope_ex_buf{exception_id} = alloca [256 x i8], align 16"
                )
                .unwrap();
                writeln!(
                    out,
                    "  call void @aura_try_enter(ptr %scope_ex_buf{exception_id})"
                )
                .unwrap();
                writeln!(out, "  %scope_ex_jump{exception_id} = call i32 @_setjmp(ptr %scope_ex_buf{exception_id})").unwrap();
                writeln!(out, "  %scope_ex_thrown{exception_id} = icmp ne i32 %scope_ex_jump{exception_id}, 0").unwrap();
                writeln!(out, "  br i1 %scope_ex_thrown{exception_id}, label %scope_catch{exception_id}, label %scope_try{exception_id}").unwrap();
                writeln!(out, "scope_try{exception_id}:").unwrap();
                let env = next_temp(out);
                writeln!(out, "  {env} = extractvalue %AuraLlvmFun {}, 0", values[0]).unwrap();
                let function = next_temp(out);
                writeln!(
                    out,
                    "  {function} = extractvalue %AuraLlvmFun {}, 1",
                    values[0]
                )
                .unwrap();
                writeln!(out, "  call void {function}(ptr {env})").unwrap();
                out.push_str("  call void @aura_try_leave()\n");
                let status = next_temp(out);
                writeln!(
                    out,
                    "  {status} = call i32 @aura_task_scope_end(ptr {scope})"
                )
                .unwrap();
                let failed = next_temp(out);
                writeln!(out, "  {failed} = icmp eq i32 {status}, 1").unwrap();
                let cancelled = next_temp(out);
                writeln!(out, "  {cancelled} = icmp eq i32 {status}, 2").unwrap();
                let id = exception_id;
                writeln!(
                    out,
                    "  br i1 {failed}, label %scope_fail{id}, label %scope_cancel_check{id}"
                )
                .unwrap();
                writeln!(out, "scope_fail{id}:").unwrap();
                writeln!(out, "  call void @aura_throw_string(ptr getelementptr ([29 x i8], ptr @.aura_scope_failed, i64 0, i64 0))").unwrap();
                out.push_str("  unreachable\n");
                writeln!(out, "scope_cancel_check{id}:").unwrap();
                writeln!(
                    out,
                    "  br i1 {cancelled}, label %scope_cancel{id}, label %scope_done{id}"
                )
                .unwrap();
                writeln!(out, "scope_cancel{id}:").unwrap();
                writeln!(out, "  call void @aura_throw_string(ptr getelementptr ([32 x i8], ptr @.aura_scope_cancelled, i64 0, i64 0))").unwrap();
                out.push_str("  unreachable\n");
                writeln!(out, "scope_done{id}:").unwrap();
                writeln!(out, "  br label %scope_continue{id}").unwrap();
                writeln!(out, "scope_catch{id}:").unwrap();
                writeln!(out, "  call i32 @aura_task_scope_end(ptr {scope})").unwrap();
                out.push_str("  call void @aura_ex_rethrow()\n  unreachable\n");
                writeln!(out, "scope_continue{id}:").unwrap();
                return Ok(String::new());
            }
            if matches!(target.name.as_str(), "get" | "isInitialized")
                && !args.is_empty()
                && values.len() == 1
                && matches!(
                    body.locals[args[0].local].ty,
                    Ty::Class(_) | Ty::ClassApp { .. }
                )
                && class_type_name(&body.locals[args[0].local].ty).is_some_and(|name| {
                    name.split('@')
                        .next()
                        .is_some_and(|base| base.ends_with("Lazy"))
                })
            {
                let receiver_ty = &body.locals[args[0].local].ty;
                let lazy_name = class_type_name(receiver_ty).unwrap();
                let fields = class_fields(context, lazy_name, class_type_args(receiver_ty))
                    .ok_or_else(|| unsupported("std.sync Lazy class layout"))?;
                let handle_index = fields
                    .iter()
                    .position(|(name, ty)| name == "runtimeHandle" && *ty == Ty::Int)
                    .ok_or_else(|| unsupported("std.sync Lazy.runtimeHandle field"))?;
                let address = next_temp(out);
                writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {handle_index}", values[0]).unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                let cell = next_temp(out);
                writeln!(out, "  {cell} = inttoptr i64 {raw} to ptr").unwrap();
                if target.name == "get" && result_ty == Some(&Ty::Int) {
                    let value = next_temp(out);
                    writeln!(
                        out,
                        "  {value} = call i64 @aura_llvm_lazy_int_get(ptr {cell})"
                    )
                    .unwrap();
                    return Ok(value);
                }
                if target.name == "isInitialized" && result_ty == Some(&Ty::Bool) {
                    let initialized = next_temp(out);
                    writeln!(
                        out,
                        "  {initialized} = call i32 @aura_llvm_lazy_is_initialized(ptr {cell})"
                    )
                    .unwrap();
                    let value = next_temp(out);
                    writeln!(out, "  {value} = icmp ne i32 {initialized}, 0").unwrap();
                    return Ok(value);
                }
            }
            if target.name == "spawnBlocking"
                && std_intrinsic == Some(StdIntrinsic::TaskSpawnBlocking)
                && values.len() == 1
                && matches!(result_ty, Some(Ty::TaskHandle(inner) | Ty::Task(inner)) if inner.as_ref() == &Ty::Int)
                && matches!(body.locals[args[0].local].ty, Ty::Fun { .. })
            {
                let frame = next_temp(out);
                writeln!(
                    out,
                    "  {frame} = call ptr @aura_llvm_spawn_blocking_i64(%AuraLlvmFun {})",
                    values[0]
                )
                .unwrap();
                return Ok(frame);
            }
            if target.name == "connect"
                && std_intrinsic == Some(StdIntrinsic::Websocket)
                && values.len() == 1
                && matches!(result_ty, Some(Ty::Task(inner) | Ty::TaskHandle(inner)) if matches!(inner.as_ref(), Ty::Class(_) | Ty::ClassApp { .. }))
            {
                let payload = match result_ty {
                    Some(Ty::Task(inner) | Ty::TaskHandle(inner)) => inner.as_ref(),
                    _ => unreachable!("websocket task result checked above"),
                };
                let class_name = class_type_name(payload)
                    .ok_or_else(|| unsupported("websocket connection result type"))?;
                let layout_name = context
                    .classes
                    .keys()
                    .find(|name| name.split('@').next() == Some(class_name))
                    .cloned()
                    .ok_or_else(|| unsupported("websocket Connection class layout"))?;
                let fields = class_fields(context, &layout_name, class_type_args(payload))
                    .ok_or_else(|| unsupported("websocket Connection class layout"))?;
                let endpoint_index = fields
                    .iter()
                    .position(|(name, ty)| name == "endpoint" && is_string_type(ty))
                    .ok_or_else(|| unsupported("websocket Connection.endpoint field"))?;
                let connected = next_temp(out);
                let endpoint_data = next_temp(out);
                writeln!(
                    out,
                    "  {endpoint_data} = call ptr @aura_llvm_str_data(ptr {})",
                    values[0]
                )
                .unwrap();
                writeln!(
                    out,
                    "  {connected} = call i32 @aura_ws_connect(ptr {endpoint_data})"
                )
                .unwrap();
                let connected_ok = next_temp(out);
                writeln!(out, "  {connected_ok} = icmp ne i32 {connected}, 0").unwrap();
                let label_id = out.len();
                let connected_label = format!("ws_connected_{label_id}");
                let failed_label = format!("ws_connect_failed_{label_id}");
                writeln!(
                    out,
                    "  br i1 {connected_ok}, label %{connected_label}, label %{failed_label}"
                )
                .unwrap();
                writeln!(out, "{failed_label}:").unwrap();
                writeln!(out, "  call void @aura_throw_string(ptr {endpoint_data})").unwrap();
                writeln!(out, "  unreachable").unwrap();
                writeln!(out, "{connected_label}:").unwrap();
                let value = next_temp(out);
                let type_id = context
                    .class_type_ids
                    .get(&layout_name)
                    .copied()
                    .ok_or_else(|| unsupported("websocket Connection type id"))?;
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_class_alloc(i64 {}, i64 {type_id})",
                    fields.len()
                )
                .unwrap();
                writeln!(out, "  call void @aura_llvm_str_retain(ptr {})", values[0]).unwrap();
                let field = next_temp(out);
                writeln!(out, "  {field} = getelementptr %AuraLlvmClass, ptr {value}, i32 0, i32 1, i64 {endpoint_index}").unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = ptrtoint ptr {} to i64", values[0]).unwrap();
                writeln!(out, "  store i64 {raw}, ptr {field}").unwrap();
                let frame = next_temp(out);
                writeln!(
                    out,
                    "  {frame} = call ptr @aura_llvm_task_immediate_ptr(ptr {value})"
                )
                .unwrap();
                return Ok(frame);
            }
            if matches!(target.name.as_str(), "send" | "ping" | "receive" | "close")
                && !args.is_empty()
                && class_type_name(&body.locals[args[0].local].ty).is_some_and(|name| {
                    name.split('@')
                        .next()
                        .is_some_and(|base| base.ends_with("Connection"))
                })
            {
                let connection_name = class_type_name(&body.locals[args[0].local].ty)
                    .ok_or_else(|| unsupported("websocket Connection receiver"))?;
                let connection_fields = class_fields(
                    context,
                    connection_name,
                    class_type_args(&body.locals[args[0].local].ty),
                )
                .ok_or_else(|| unsupported("websocket Connection layout"))?;
                let endpoint_index = connection_fields
                    .iter()
                    .position(|(name, ty)| name == "endpoint" && is_string_type(ty))
                    .ok_or_else(|| unsupported("websocket Connection.endpoint field"))?;
                let endpoint_address = next_temp(out);
                writeln!(out, "  {endpoint_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {endpoint_index}", values[0]).unwrap();
                let endpoint_raw = next_temp(out);
                writeln!(out, "  {endpoint_raw} = load i64, ptr {endpoint_address}").unwrap();
                let endpoint = next_temp(out);
                writeln!(out, "  {endpoint} = inttoptr i64 {endpoint_raw} to ptr").unwrap();
                let endpoint_data = next_temp(out);
                writeln!(
                    out,
                    "  {endpoint_data} = call ptr @aura_llvm_str_data(ptr {endpoint})"
                )
                .unwrap();

                if target.name == "receive" {
                    let kind_slot = next_temp(out);
                    writeln!(out, "  {kind_slot} = alloca i64").unwrap();
                    let payload_data = next_temp(out);
                    writeln!(out, "  {payload_data} = call ptr @aura_ws_receive(ptr {endpoint_data}, ptr {kind_slot})").unwrap();
                    let payload_ok = next_temp(out);
                    writeln!(out, "  {payload_ok} = icmp ne ptr {payload_data}, null").unwrap();
                    let ok_label = format!("ws_receive_ok_{}", out.len());
                    let fail_label = format!("ws_receive_fail_{}", out.len());
                    writeln!(
                        out,
                        "  br i1 {payload_ok}, label %{ok_label}, label %{fail_label}"
                    )
                    .unwrap();
                    writeln!(out, "{fail_label}:").unwrap();
                    writeln!(out, "  call void @aura_throw_string(ptr {endpoint_data})").unwrap();
                    writeln!(out, "  unreachable").unwrap();
                    writeln!(out, "{ok_label}:").unwrap();
                    let payload = next_temp(out);
                    writeln!(
                        out,
                        "  {payload} = call ptr @aura_llvm_str_new(ptr {payload_data})"
                    )
                    .unwrap();
                    writeln!(out, "  call void @free(ptr {payload_data})").unwrap();
                    let kind = next_temp(out);
                    writeln!(out, "  {kind} = load i64, ptr {kind_slot}").unwrap();
                    let is_binary = next_temp(out);
                    writeln!(out, "  {is_binary} = icmp eq i64 {kind}, 2").unwrap();
                    let binary_tag = next_temp(out);
                    writeln!(out, "  {binary_tag} = select i1 {is_binary}, i64 1, i64 0").unwrap();
                    let is_ping = next_temp(out);
                    writeln!(out, "  {is_ping} = icmp eq i64 {kind}, 9").unwrap();
                    let ping_tag = next_temp(out);
                    writeln!(
                        out,
                        "  {ping_tag} = select i1 {is_ping}, i64 2, i64 {binary_tag}"
                    )
                    .unwrap();
                    let is_pong = next_temp(out);
                    writeln!(out, "  {is_pong} = icmp eq i64 {kind}, 10").unwrap();
                    let pong_tag = next_temp(out);
                    writeln!(
                        out,
                        "  {pong_tag} = select i1 {is_pong}, i64 3, i64 {ping_tag}"
                    )
                    .unwrap();
                    let is_close = next_temp(out);
                    writeln!(out, "  {is_close} = icmp eq i64 {kind}, 8").unwrap();
                    let message_tag = next_temp(out);
                    writeln!(
                        out,
                        "  {message_tag} = select i1 {is_close}, i64 4, i64 {pong_tag}"
                    )
                    .unwrap();
                    let message_kind = next_temp(out);
                    writeln!(
                        out,
                        "  {message_kind} = call ptr @aura_llvm_enum_alloc(i64 0, ptr null)"
                    )
                    .unwrap();
                    let kind_tag = next_temp(out);
                    writeln!(out, "  {kind_tag} = getelementptr %AuraLlvmEnum, ptr {message_kind}, i32 0, i32 1").unwrap();
                    writeln!(out, "  store i64 {message_tag}, ptr {kind_tag}").unwrap();
                    let message_name = context
                        .classes
                        .keys()
                        .find(|name| {
                            name.split('@')
                                .next()
                                .is_some_and(|base| base.ends_with("Message"))
                        })
                        .cloned()
                        .ok_or_else(|| unsupported("websocket Message layout"))?;
                    let message_fields = class_fields(context, &message_name, &[])
                        .ok_or_else(|| unsupported("websocket Message layout"))?;
                    let message_type_id = context
                        .class_type_ids
                        .get(&message_name)
                        .copied()
                        .ok_or_else(|| unsupported("websocket Message type id"))?;
                    let kind_index = message_fields
                        .iter()
                        .position(|(name, _)| name == "kind")
                        .ok_or_else(|| unsupported("websocket Message.kind field"))?;
                    let payload_index = message_fields
                        .iter()
                        .position(|(name, _)| name == "payload")
                        .ok_or_else(|| unsupported("websocket Message.payload field"))?;
                    let message = next_temp(out);
                    writeln!(out, "  {message} = call ptr @aura_llvm_class_alloc(i64 {}, i64 {message_type_id})", message_fields.len()).unwrap();
                    let kind_field = next_temp(out);
                    writeln!(out, "  {kind_field} = getelementptr %AuraLlvmClass, ptr {message}, i32 0, i32 1, i64 {kind_index}").unwrap();
                    let kind_raw = next_temp(out);
                    writeln!(out, "  {kind_raw} = ptrtoint ptr {message_kind} to i64").unwrap();
                    writeln!(out, "  store i64 {kind_raw}, ptr {kind_field}").unwrap();
                    let payload_field = next_temp(out);
                    writeln!(out, "  {payload_field} = getelementptr %AuraLlvmClass, ptr {message}, i32 0, i32 1, i64 {payload_index}").unwrap();
                    let payload_raw = next_temp(out);
                    writeln!(out, "  {payload_raw} = ptrtoint ptr {payload} to i64").unwrap();
                    writeln!(out, "  store i64 {payload_raw}, ptr {payload_field}").unwrap();
                    let frame = next_temp(out);
                    writeln!(
                        out,
                        "  {frame} = call ptr @aura_llvm_task_immediate_ptr(ptr {message})"
                    )
                    .unwrap();
                    return Ok(frame);
                }

                let kind = if target.name == "ping" { "2" } else { "0" };
                let payload = if target.name == "ping" {
                    values
                        .get(1)
                        .cloned()
                        .ok_or_else(|| unsupported("websocket ping payload"))?
                } else if target.name == "send" {
                    let message_name = class_type_name(&body.locals[args[1].local].ty)
                        .ok_or_else(|| unsupported("websocket Message receiver"))?;
                    let message_fields = class_fields(
                        context,
                        message_name,
                        class_type_args(&body.locals[args[1].local].ty),
                    )
                    .ok_or_else(|| unsupported("websocket Message layout"))?;
                    let payload_index = message_fields
                        .iter()
                        .position(|(name, _)| name == "payload")
                        .ok_or_else(|| unsupported("websocket Message.payload field"))?;
                    let address = next_temp(out);
                    writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {payload_index}", values[1]).unwrap();
                    let raw = next_temp(out);
                    writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                    let payload = next_temp(out);
                    writeln!(out, "  {payload} = inttoptr i64 {raw} to ptr").unwrap();
                    payload
                } else {
                    String::new()
                };
                let actual_kind = if target.name == "send" {
                    let message_name = class_type_name(&body.locals[args[1].local].ty)
                        .ok_or_else(|| unsupported("websocket Message receiver"))?;
                    let message_fields = class_fields(
                        context,
                        message_name,
                        class_type_args(&body.locals[args[1].local].ty),
                    )
                    .ok_or_else(|| unsupported("websocket Message layout"))?;
                    let kind_index = message_fields
                        .iter()
                        .position(|(name, _)| name == "kind")
                        .ok_or_else(|| unsupported("websocket Message.kind field"))?;
                    let address = next_temp(out);
                    writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {kind_index}", values[1]).unwrap();
                    let raw = next_temp(out);
                    writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                    let enum_value = next_temp(out);
                    writeln!(out, "  {enum_value} = inttoptr i64 {raw} to ptr").unwrap();
                    let tag_address = next_temp(out);
                    writeln!(out, "  {tag_address} = getelementptr %AuraLlvmEnum, ptr {enum_value}, i32 0, i32 1").unwrap();
                    let tag = next_temp(out);
                    writeln!(out, "  {tag} = load i64, ptr {tag_address}").unwrap();
                    tag
                } else {
                    kind.to_owned()
                };
                if target.name == "close" {
                    let result = next_temp(out);
                    writeln!(
                        out,
                        "  {result} = call i32 @aura_ws_close(ptr {endpoint_data})"
                    )
                    .unwrap();
                    let ok = next_temp(out);
                    writeln!(out, "  {ok} = icmp ne i32 {result}, 0").unwrap();
                    let close_ok = format!("ws_close_ok_{}", out.len());
                    let close_fail = format!("ws_close_fail_{}", out.len());
                    writeln!(out, "  br i1 {ok}, label %{close_ok}, label %{close_fail}").unwrap();
                    writeln!(out, "{close_fail}:").unwrap();
                    writeln!(out, "  call void @aura_throw_string(ptr {endpoint_data})").unwrap();
                    writeln!(out, "  unreachable").unwrap();
                    writeln!(out, "{close_ok}:").unwrap();
                    return Ok(String::new());
                }
                let payload_data = next_temp(out);
                writeln!(
                    out,
                    "  {payload_data} = call ptr @aura_llvm_str_data(ptr {payload})"
                )
                .unwrap();
                let sent = next_temp(out);
                writeln!(out, "  {sent} = call i64 @aura_ws_send(ptr {endpoint_data}, i64 {actual_kind}, ptr {payload_data})").unwrap();
                let sent_ok = next_temp(out);
                writeln!(out, "  {sent_ok} = icmp sge i64 {sent}, 0").unwrap();
                let send_ok = format!("ws_send_ok_{}", out.len());
                let send_fail = format!("ws_send_fail_{}", out.len());
                writeln!(
                    out,
                    "  br i1 {sent_ok}, label %{send_ok}, label %{send_fail}"
                )
                .unwrap();
                writeln!(out, "{send_fail}:").unwrap();
                writeln!(out, "  call void @aura_throw_string(ptr {endpoint_data})").unwrap();
                writeln!(out, "  unreachable").unwrap();
                writeln!(out, "{send_ok}:").unwrap();
                if result_ty.is_some_and(|ty| matches!(ty, Ty::Task(_) | Ty::TaskHandle(_))) {
                    let frame = next_temp(out);
                    writeln!(
                        out,
                        "  {frame} = call ptr @aura_llvm_task_immediate_i64(i64 0)"
                    )
                    .unwrap();
                    return Ok(frame);
                }
                return Ok(String::new());
            }
            if std_intrinsic == Some(StdIntrinsic::HttpAccessor)
                && matches!(
                    target.name.as_str(),
                    "requestMethod"
                        | "requestTarget"
                        | "requestVersion"
                        | "requestHeaderCount"
                        | "requestHeaderName"
                        | "requestHeaderValue"
                        | "requestBody"
                        | "responseStatus"
                        | "responseKeepAlive"
                        | "responseSetStatus"
                        | "responseSetKeepAlive"
                        | "responseSetBody"
                        | "responseAddHeader"
                )
            {
                let receiver = values
                    .first()
                    .ok_or_else(|| unsupported("HTTP accessor receiver"))?;
                let call = |out: &mut String, ty: &str, name: &str, args: &str| -> String {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = call {ty} @{name}({args})").unwrap();
                    value
                };
                let result = match target.name.as_str() {
                    "requestMethod" => call(
                        out,
                        "ptr",
                        "aura_http_request_method",
                        &format!("ptr {receiver}"),
                    ),
                    "requestTarget" => call(
                        out,
                        "ptr",
                        "aura_http_request_target",
                        &format!("ptr {receiver}"),
                    ),
                    "requestVersion" => call(
                        out,
                        "ptr",
                        "aura_http_request_version",
                        &format!("ptr {receiver}"),
                    ),
                    "requestHeaderCount" => call(
                        out,
                        "i64",
                        "aura_http_request_header_count",
                        &format!("ptr {receiver}"),
                    ),
                    "requestHeaderName" => call(
                        out,
                        "ptr",
                        "aura_http_request_header_name",
                        &format!("ptr {receiver}, i64 {}", values[1]),
                    ),
                    "requestHeaderValue" => call(
                        out,
                        "ptr",
                        "aura_http_request_header_value",
                        &format!("ptr {receiver}, i64 {}", values[1]),
                    ),
                    "requestBody" => call(
                        out,
                        "ptr",
                        "aura_http_request_body",
                        &format!("ptr {receiver}"),
                    ),
                    "responseStatus" => {
                        let raw = call(
                            out,
                            "i32",
                            "aura_http_response_status",
                            &format!("ptr {receiver}"),
                        );
                        let value = next_temp(out);
                        writeln!(out, "  {value} = sext i32 {raw} to i64").unwrap();
                        value
                    }
                    "responseKeepAlive" => {
                        let raw = call(
                            out,
                            "i32",
                            "aura_http_response_keep_alive",
                            &format!("ptr {receiver}"),
                        );
                        let value = next_temp(out);
                        writeln!(out, "  {value} = icmp ne i32 {raw}, 0").unwrap();
                        return Ok(value);
                    }
                    "responseSetStatus" => {
                        let status = next_temp(out);
                        writeln!(out, "  {status} = trunc i64 {} to i32", values[1]).unwrap();
                        let raw = call(
                            out,
                            "i32",
                            "aura_http_response_set_status",
                            &format!("ptr {receiver}, i32 {status}"),
                        );
                        let value = next_temp(out);
                        writeln!(out, "  {value} = icmp eq i32 {raw}, 0").unwrap();
                        return Ok(value);
                    }
                    "responseSetKeepAlive" => {
                        let connection = next_temp(out);
                        writeln!(
                            out,
                            "  {connection} = select i1 {}, i32 1, i32 0",
                            values[1]
                        )
                        .unwrap();
                        let raw = call(
                            out,
                            "i32",
                            "aura_http_response_set_connection",
                            &format!("ptr {receiver}, i32 {connection}"),
                        );
                        let value = next_temp(out);
                        writeln!(out, "  {value} = icmp eq i32 {raw}, 0").unwrap();
                        return Ok(value);
                    }
                    "responseSetBody" => {
                        let body = next_temp(out);
                        writeln!(
                            out,
                            "  {body} = call ptr @aura_llvm_str_data(ptr {})",
                            values[1]
                        )
                        .unwrap();
                        let length = next_temp(out);
                        writeln!(out, "  {length} = call i64 @strlen(ptr {body})").unwrap();
                        let raw = call(
                            out,
                            "i32",
                            "aura_http_response_set_body",
                            &format!("ptr {receiver}, ptr {body}, i64 {length}"),
                        );
                        let value = next_temp(out);
                        writeln!(out, "  {value} = icmp eq i32 {raw}, 0").unwrap();
                        return Ok(value);
                    }
                    "responseAddHeader" => {
                        let name = next_temp(out);
                        writeln!(
                            out,
                            "  {name} = call ptr @aura_llvm_str_data(ptr {})",
                            values[1]
                        )
                        .unwrap();
                        let value_data = next_temp(out);
                        writeln!(
                            out,
                            "  {value_data} = call ptr @aura_llvm_str_data(ptr {})",
                            values[2]
                        )
                        .unwrap();
                        let raw = call(
                            out,
                            "i32",
                            "aura_http_response_add_header",
                            &format!("ptr {receiver}, ptr {name}, ptr {value_data}"),
                        );
                        let value = next_temp(out);
                        writeln!(out, "  {value} = icmp eq i32 {raw}, 0").unwrap();
                        return Ok(value);
                    }
                    "responseBody" => return Err(unsupported("HTTP accessor")),
                    _ => return Err(unsupported("HTTP accessor")),
                };
                if matches!(
                    target.name.as_str(),
                    "requestMethod"
                        | "requestTarget"
                        | "requestVersion"
                        | "requestHeaderName"
                        | "requestHeaderValue"
                        | "requestBody"
                ) {
                    return Ok(call(
                        out,
                        "ptr",
                        "aura_llvm_str_new",
                        &format!("ptr {result}"),
                    ));
                }
                return Ok(result);
            }
            if target.name == "bind"
                && std_intrinsic == Some(StdIntrinsic::Udp)
                && values.len() == 1
                && matches!(result_ty, Some(Ty::Class(_) | Ty::ClassApp { .. }))
            {
                let socket_ty = result_ty.ok_or_else(|| unsupported("UDP socket result type"))?;
                let socket_name = class_type_name(socket_ty)
                    .ok_or_else(|| unsupported("UDP socket result type"))?;
                let endpoint_fields = class_fields(context, "std_udp_Endpoint", &[])
                    .or_else(|| class_fields(context, "Endpoint", &[]))
                    .ok_or_else(|| unsupported("UDP endpoint class layout"))?;
                let host_index = endpoint_fields
                    .iter()
                    .position(|(name, ty)| name == "host" && is_string_type(ty))
                    .ok_or_else(|| unsupported("UDP endpoint host field"))?;
                let port_index = endpoint_fields
                    .iter()
                    .position(|(name, ty)| name == "port" && *ty == Ty::Int)
                    .ok_or_else(|| unsupported("UDP endpoint port field"))?;
                let endpoint_host_address = next_temp(out);
                writeln!(out, "  {endpoint_host_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {host_index}", values[0]).unwrap();
                let endpoint_host_raw = next_temp(out);
                writeln!(
                    out,
                    "  {endpoint_host_raw} = load i64, ptr {endpoint_host_address}"
                )
                .unwrap();
                let endpoint_host = next_temp(out);
                writeln!(
                    out,
                    "  {endpoint_host} = inttoptr i64 {endpoint_host_raw} to ptr"
                )
                .unwrap();
                let endpoint_port_address = next_temp(out);
                writeln!(out, "  {endpoint_port_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {port_index}", values[0]).unwrap();
                let endpoint_port = next_temp(out);
                writeln!(
                    out,
                    "  {endpoint_port} = load i64, ptr {endpoint_port_address}"
                )
                .unwrap();
                let host_data = next_temp(out);
                writeln!(
                    out,
                    "  {host_data} = call ptr @aura_llvm_str_data(ptr {endpoint_host})"
                )
                .unwrap();
                let bound = next_temp(out);
                writeln!(
                    out,
                    "  {bound} = call i32 @aura_udp_bind(ptr {host_data}, i64 {endpoint_port})"
                )
                .unwrap();
                let socket_layout_name = context
                    .classes
                    .keys()
                    .find(|name| name.split('@').next() == Some(socket_name))
                    .cloned()
                    .ok_or_else(|| unsupported("UDP Socket class layout"))?;
                let socket_fields =
                    class_fields(context, &socket_layout_name, class_type_args(socket_ty))
                        .ok_or_else(|| unsupported("UDP Socket class layout"))?;
                let socket_endpoint_index = socket_fields
                    .iter()
                    .position(|(name, ty)| name == "endpoint" && is_pointer_value_type(ty))
                    .ok_or_else(|| unsupported("UDP Socket.endpoint field"))?;
                let socket_type_id = context
                    .class_type_ids
                    .get(&socket_layout_name)
                    .copied()
                    .ok_or_else(|| unsupported("UDP Socket type id"))?;
                let socket = next_temp(out);
                writeln!(
                    out,
                    "  {socket} = call ptr @aura_llvm_class_alloc(i64 {}, i64 {socket_type_id})",
                    socket_fields.len()
                )
                .unwrap();
                writeln!(
                    out,
                    "  call void @aura_llvm_class_retain(ptr {})",
                    values[0]
                )
                .unwrap();
                let socket_field = next_temp(out);
                writeln!(out, "  {socket_field} = getelementptr %AuraLlvmClass, ptr {socket}, i32 0, i32 1, i64 {socket_endpoint_index}").unwrap();
                let socket_raw = next_temp(out);
                writeln!(out, "  {socket_raw} = ptrtoint ptr {} to i64", values[0]).unwrap();
                writeln!(out, "  store i64 {socket_raw}, ptr {socket_field}").unwrap();
                let _ = bound;
                return Ok(socket);
            }
            if target.name == "close"
                && values.len() == 1
                && matches!(
                    body.locals[args[0].local].ty,
                    Ty::Class(_) | Ty::ClassApp { .. }
                )
            {
                let class_name =
                    class_type_name(&body.locals[args[0].local].ty).unwrap_or_default();
                if class_name == "std_udp_Socket" || class_name == "Socket" {
                    let socket_fields = class_fields(
                        context,
                        class_name,
                        class_type_args(&body.locals[args[0].local].ty),
                    )
                    .ok_or_else(|| unsupported("UDP Socket class layout"))?;
                    let endpoint_index = socket_fields
                        .iter()
                        .position(|(name, ty)| name == "endpoint" && is_pointer_value_type(ty))
                        .ok_or_else(|| unsupported("UDP Socket.endpoint field"))?;
                    let endpoint = next_temp(out);
                    let endpoint_address = next_temp(out);
                    writeln!(out, "  {endpoint_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {endpoint_index}", values[0]).unwrap();
                    let endpoint_raw = next_temp(out);
                    writeln!(out, "  {endpoint_raw} = load i64, ptr {endpoint_address}").unwrap();
                    writeln!(out, "  {endpoint} = inttoptr i64 {endpoint_raw} to ptr").unwrap();
                    let endpoint_fields = class_fields(context, "std_udp_Endpoint", &[])
                        .or_else(|| class_fields(context, "Endpoint", &[]))
                        .ok_or_else(|| unsupported("UDP endpoint class layout"))?;
                    let host_index = endpoint_fields
                        .iter()
                        .position(|(name, _)| name == "host")
                        .ok_or_else(|| unsupported("UDP endpoint host field"))?;
                    let port_index = endpoint_fields
                        .iter()
                        .position(|(name, _)| name == "port")
                        .ok_or_else(|| unsupported("UDP endpoint port field"))?;
                    let host_address = next_temp(out);
                    writeln!(out, "  {host_address} = getelementptr %AuraLlvmClass, ptr {endpoint}, i32 0, i32 1, i64 {host_index}").unwrap();
                    let host_raw = next_temp(out);
                    writeln!(out, "  {host_raw} = load i64, ptr {host_address}").unwrap();
                    let host = next_temp(out);
                    writeln!(out, "  {host} = inttoptr i64 {host_raw} to ptr").unwrap();
                    let host_data = next_temp(out);
                    writeln!(
                        out,
                        "  {host_data} = call ptr @aura_llvm_str_data(ptr {host})"
                    )
                    .unwrap();
                    let port_address = next_temp(out);
                    writeln!(out, "  {port_address} = getelementptr %AuraLlvmClass, ptr {endpoint}, i32 0, i32 1, i64 {port_index}").unwrap();
                    let port = next_temp(out);
                    writeln!(out, "  {port} = load i64, ptr {port_address}").unwrap();
                    writeln!(
                        out,
                        "  call i32 @aura_udp_close(ptr {host_data}, i64 {port})"
                    )
                    .unwrap();
                    return Ok(String::new());
                }
            }
            let receiver_is_stream_adapter = !args.is_empty()
                && matches!(&body.locals[args[0].local].ty, Ty::Class(name) | Ty::ClassApp { name, .. }
                    if matches!(name.split('@').next(), Some("std_stream_Reader" | "std_stream_Writer" | "Reader" | "Writer")));
            let receiver_is_tls_connection = !args.is_empty()
                && matches!(&body.locals[args[0].local].ty, Ty::Class(name) | Ty::ClassApp { name, .. }
                    if name.split('@').next().is_some_and(|base| base == "std_tls_Connection" || base == "Connection"));
            if receiver_is_tls_connection
                && matches!(
                    target.name.as_str(),
                    "readBytes" | "readBytesWithTimeout" | "writeBytes" | "writeBytesWithTimeout"
                )
            {
                let class_name = class_type_name(&body.locals[args[0].local].ty)
                    .ok_or_else(|| unsupported("TLS Connection class layout"))?;
                let fields = class_fields(
                    context,
                    class_name,
                    class_type_args(&body.locals[args[0].local].ty),
                )
                .ok_or_else(|| unsupported("TLS Connection class layout"))?;
                let endpoint_index = fields
                    .iter()
                    .position(|(name, ty)| name == "endpoint" && is_pointer_value_type(ty))
                    .ok_or_else(|| unsupported("TLS Connection.endpoint field"))?;
                let endpoint_address = next_temp(out);
                writeln!(out, "  {endpoint_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {endpoint_index}", values[0]).unwrap();
                let endpoint_raw = next_temp(out);
                writeln!(out, "  {endpoint_raw} = load i64, ptr {endpoint_address}").unwrap();
                let endpoint = next_temp(out);
                writeln!(out, "  {endpoint} = inttoptr i64 {endpoint_raw} to ptr").unwrap();
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let timeout = if target.name.ends_with("WithTimeout") {
                    values[2].clone()
                } else {
                    "0".to_string()
                };
                let frame = next_temp(out);
                if target.name.starts_with("read") {
                    let payload = task_payload_type(
                        result_ty.ok_or_else(|| unsupported("TLS read result type"))?,
                    )
                    .ok_or_else(|| unsupported("TLS read result type"))?;
                    let buffer_name = class_type_name(payload)
                        .ok_or_else(|| unsupported("TLS Buffer result type"))?;
                    let buffer_layout = context
                        .classes
                        .keys()
                        .find(|name| {
                            name == &buffer_name
                                || name.split('@').next() == buffer_name.split('@').next()
                        })
                        .cloned()
                        .unwrap_or_else(|| buffer_name.to_owned());
                    let buffer_type_id = context
                        .class_type_ids
                        .get(&buffer_layout)
                        .copied()
                        .ok_or_else(|| unsupported("TLS Buffer type id"))?;
                    writeln!(out, "  {frame} = call ptr @aura_llvm_tls_read_task(ptr {executor}, ptr {endpoint}, i64 {}, i64 {timeout}, i64 {buffer_type_id})", values[1]).unwrap();
                } else {
                    writeln!(out, "  {frame} = call ptr @aura_llvm_tls_write_task(ptr {executor}, ptr {endpoint}, ptr {}, i64 {timeout})", values[1]).unwrap();
                }
                return Ok(frame);
            }
            if receiver_is_stream_adapter
                && matches!(
                    target.name.as_str(),
                    "readExactly" | "readExactlyWithTimeout"
                )
                && ((target.name == "readExactly" && values.len() == 2)
                    || (target.name == "readExactlyWithTimeout" && values.len() == 3))
            {
                let class_name = class_type_name(&body.locals[args[0].local].ty)
                    .ok_or_else(|| unsupported("stream adapter class layout"))?;
                let fields = class_fields(
                    context,
                    class_name,
                    class_type_args(&body.locals[args[0].local].ty),
                )
                .ok_or_else(|| unsupported("stream adapter class layout"))?;
                let stream_index = fields
                    .iter()
                    .position(|(name, ty)| name == "stream" && is_pointer_value_type(ty))
                    .ok_or_else(|| unsupported("stream adapter stream field"))?;
                let address = next_temp(out);
                writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {stream_index}", values[0]).unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                let stream = next_temp(out);
                writeln!(out, "  {stream} = inttoptr i64 {raw} to ptr").unwrap();
                let payload = task_payload_type(
                    result_ty.ok_or_else(|| unsupported("exact read result type"))?,
                )
                .ok_or_else(|| unsupported("exact read result type"))?;
                let buffer_name = class_type_name(payload)
                    .ok_or_else(|| unsupported("std.bytes.Buffer result type"))?;
                let buffer_layout = context
                    .classes
                    .keys()
                    .find(|name| {
                        name == &buffer_name
                            || name.split('@').next() == buffer_name.split('@').next()
                    })
                    .cloned()
                    .unwrap_or_else(|| buffer_name.to_owned());
                let buffer_type_id = context
                    .class_type_ids
                    .get(&buffer_layout)
                    .copied()
                    .ok_or_else(|| unsupported("std.bytes.Buffer type id"))?;
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let timeout = if target.name == "readExactlyWithTimeout" {
                    values[2].clone()
                } else {
                    "0".to_string()
                };
                let frame = next_temp(out);
                writeln!(out, "  {frame} = call ptr @aura_llvm_net_read_exact_task(ptr {executor}, ptr {stream}, i64 {}, i64 {timeout}, i64 0, i64 {buffer_type_id})", values[1]).unwrap();
                return Ok(frame);
            }
            if receiver_is_stream_adapter
                && matches!(target.name.as_str(), "writeAll" | "writeAllWithTimeout")
                && ((target.name == "writeAll" && values.len() == 2)
                    || (target.name == "writeAllWithTimeout" && values.len() == 3))
            {
                let class_name = class_type_name(&body.locals[args[0].local].ty)
                    .ok_or_else(|| unsupported("stream adapter class layout"))?;
                let fields = class_fields(
                    context,
                    class_name,
                    class_type_args(&body.locals[args[0].local].ty),
                )
                .ok_or_else(|| unsupported("stream adapter class layout"))?;
                let stream_index = fields
                    .iter()
                    .position(|(name, ty)| name == "stream" && is_pointer_value_type(ty))
                    .ok_or_else(|| unsupported("stream adapter stream field"))?;
                let address = next_temp(out);
                writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {stream_index}", values[0]).unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                let stream = next_temp(out);
                writeln!(out, "  {stream} = inttoptr i64 {raw} to ptr").unwrap();
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                writeln!(out, "  {frame} = call ptr @aura_llvm_net_write_all_task(ptr {executor}, ptr {stream}, ptr {}, i64 0)", values[1]).unwrap();
                return Ok(frame);
            }
            if receiver_is_stream_adapter
                && matches!(
                    target.name.as_str(),
                    "read" | "write" | "readResult" | "writeResult"
                )
                && values.len() == 2
            {
                let class_name = class_type_name(&body.locals[args[0].local].ty)
                    .ok_or_else(|| unsupported("stream adapter class layout"))?;
                let fields = class_fields(
                    context,
                    class_name,
                    class_type_args(&body.locals[args[0].local].ty),
                )
                .ok_or_else(|| unsupported("stream adapter class layout"))?;
                let stream_index = fields
                    .iter()
                    .position(|(name, ty)| name == "stream" && is_pointer_value_type(ty))
                    .ok_or_else(|| unsupported("stream adapter stream field"))?;
                let address = next_temp(out);
                writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {stream_index}", values[0]).unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                let stream = next_temp(out);
                writeln!(out, "  {stream} = inttoptr i64 {raw} to ptr").unwrap();
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                if matches!(target.name.as_str(), "read" | "write") {
                    if target.name == "read" {
                        writeln!(out, "  {frame} = call ptr @aura_llvm_net_read_task(ptr {executor}, ptr {stream}, i64 {}, i64 0)", values[1]).unwrap();
                    } else {
                        writeln!(out, "  {frame} = call ptr @aura_llvm_net_write_task(ptr {executor}, ptr {stream}, ptr {}, i64 0)", values[1]).unwrap();
                    }
                } else {
                    if target.name == "readResult" {
                        writeln!(out, "  {frame} = call ptr @aura_llvm_net_read_task(ptr {executor}, ptr {stream}, i64 {}, i64 0)", values[1]).unwrap();
                    } else {
                        writeln!(out, "  {frame} = call ptr @aura_llvm_net_write_task(ptr {executor}, ptr {stream}, ptr {}, i64 0)", values[1]).unwrap();
                    }
                }
                return Ok(frame);
            }
            if !args.is_empty()
                && matches!(target.name.as_str(), "readChunk" | "readChunkResult")
                && matches!(&body.locals[args[0].local].ty, Ty::Class(name) | Ty::ClassApp { name, .. }
                    if name.split('@').next().is_some_and(|base| base.ends_with("RequestBody")))
                && values.len() == 2
            {
                let class_name = class_type_name(&body.locals[args[0].local].ty)
                    .ok_or_else(|| unsupported("HTTP RequestBody class layout"))?;
                let fields = class_fields(
                    context,
                    class_name,
                    class_type_args(&body.locals[args[0].local].ty),
                )
                .ok_or_else(|| unsupported("HTTP RequestBody class layout"))?;
                let handle_index = fields
                    .iter()
                    .position(|(name, ty)| name == "handle" && is_pointer_value_type(ty))
                    .ok_or_else(|| unsupported("HTTP RequestBody.handle field"))?;
                let address = next_temp(out);
                writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {handle_index}", values[0]).unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                let handle = next_temp(out);
                writeln!(out, "  {handle} = inttoptr i64 {raw} to ptr").unwrap();
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                if target.name == "readChunk" {
                    writeln!(out, "  {frame} = call ptr @aura_llvm_http_read_chunk_task(ptr {executor}, ptr {handle}, i64 {})", values[1]).unwrap();
                } else {
                    writeln!(out, "  {frame} = call ptr @aura_llvm_http_read_chunk_task(ptr {executor}, ptr {handle}, i64 {})", values[1]).unwrap();
                }
                return Ok(frame);
            }
            if !args.is_empty()
                && matches!(target.name.as_str(), "writeChunk" | "writeChunkResult")
                && matches!(&body.locals[args[0].local].ty, Ty::Class(name) | Ty::ClassApp { name, .. }
                    if name.split('@').next().is_some_and(|base| base.ends_with("Response")))
                && values.len() == 2
            {
                let class_name = class_type_name(&body.locals[args[0].local].ty)
                    .ok_or_else(|| unsupported("HTTP Response class layout"))?;
                let fields = class_fields(
                    context,
                    class_name,
                    class_type_args(&body.locals[args[0].local].ty),
                )
                .ok_or_else(|| unsupported("HTTP Response class layout"))?;
                let response_index = fields
                    .iter()
                    .position(|(name, ty)| name == "handle" && is_pointer_value_type(ty))
                    .ok_or_else(|| unsupported("HTTP Response.handle field"))?;
                let connection_index = fields
                    .iter()
                    .position(|(name, ty)| name == "connection" && is_pointer_value_type(ty))
                    .ok_or_else(|| unsupported("HTTP Response.connection field"))?;
                let load_handle = |out: &mut String, index: usize, receiver: &str| -> String {
                    let address = next_temp(out);
                    writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {receiver}, i32 0, i32 1, i64 {index}").unwrap();
                    let raw = next_temp(out);
                    writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                    let handle = next_temp(out);
                    writeln!(out, "  {handle} = inttoptr i64 {raw} to ptr").unwrap();
                    handle
                };
                let response = load_handle(out, response_index, &values[0]);
                let connection = load_handle(out, connection_index, &values[0]);
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                if target.name == "writeChunk" {
                    writeln!(out, "  {frame} = call ptr @aura_llvm_http_write_chunk_task(ptr {executor}, ptr {response}, ptr {connection}, ptr {})", values[1]).unwrap();
                } else {
                    writeln!(out, "  {frame} = call ptr @aura_llvm_http_write_chunk_task(ptr {executor}, ptr {response}, ptr {connection}, ptr {})", values[1]).unwrap();
                }
                return Ok(frame);
            }
            if (matches!(target.name.as_str(), "listenResult" | "closeStreamResult")
                || (target.name == "closeResult" && receiver_is_stream_adapter))
                && result_ty.is_some_and(|ty| matches!(ty, Ty::EnumApp { name, .. } if matches!(name.split('@').next(), Some("std_error_Outcome" | "Outcome"))))
            {
                let result_ty = result_ty.ok_or_else(|| unsupported("network outcome result type"))?;
                let Ty::EnumApp { args: outcome_args, .. } = result_ty else {
                    return Err(unsupported("network outcome result type"));
                };
                let info = context
                    .enum_variants
                    .get("OutcomeOk")
                    .cloned()
                    .ok_or_else(|| unsupported("OutcomeOk variant"))?;
                let fields = resolved_variant_fields(&info, result_ty, outcome_args);
                if fields.len() != 1 || args.len() != 1 {
                    return Err(unsupported("network outcome success shape"));
                }
                let value = next_temp(out);
                let operation_value = if target.name == "closeResult" {
                    let class_name = match &body.locals[args[0].local].ty {
                        Ty::Class(name) | Ty::ClassApp { name, .. } => name,
                        _ => unreachable!("stream adapter receiver checked above"),
                    };
                    let fields = class_fields(context, class_name.split('@').next().unwrap_or(class_name), &[])
                        .ok_or_else(|| unsupported("stream adapter class layout"))?;
                    let index = fields
                        .iter()
                        .position(|(name, ty)| name == "stream" && is_pointer_value_type(ty))
                        .ok_or_else(|| unsupported("stream adapter stream field"))?;
                    let address = next_temp(out);
                    writeln!(out, "  {address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {index}", values[0]).unwrap();
                    let raw = next_temp(out);
                    writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                    let stream = next_temp(out);
                    writeln!(out, "  {stream} = inttoptr i64 {raw} to ptr").unwrap();
                    stream
                } else {
                    values[0].clone()
                };
                    if target.name == "listenResult" {
                    writeln!(out, "  {value} = call ptr @aura_llvm_net_listen(ptr {})", values[0]).unwrap();
                } else {
                    writeln!(out, "  {value} = call i1 @aura_std_net_closeStream(ptr {operation_value})").unwrap();
                }
                let destructor = enum_destructor_symbol(Some("OutcomeOk"), &fields, context);
                let outcome = next_temp(out);
                writeln!(out, "  {outcome} = call ptr @aura_llvm_enum_alloc(i64 1, ptr {destructor})").unwrap();
                let tag = next_temp(out);
                writeln!(out, "  {tag} = getelementptr %AuraLlvmEnum, ptr {outcome}, i32 0, i32 1").unwrap();
                writeln!(out, "  store i64 {}, ptr {tag}", info.tag).unwrap();
                let field = next_temp(out);
                writeln!(out, "  {field} = getelementptr %AuraLlvmEnum, ptr {outcome}, i32 0, i32 3, i64 0").unwrap();
                let raw = match &fields[0].1 {
                    Ty::Int => value,
                    Ty::Bool => {
                        let converted = next_temp(out);
                        writeln!(out, "  {converted} = zext i1 {value} to i64").unwrap();
                        converted
                    }
                    ty if is_pointer_value_type(ty) => {
                        let converted = next_temp(out);
                        writeln!(out, "  {converted} = ptrtoint ptr {value} to i64").unwrap();
                        converted
                    }
                    _ => return Err(unsupported("network outcome payload type")),
                };
                writeln!(out, "  store i64 {raw}, ptr {field}").unwrap();
                return Ok(outcome);
            }
            if std_intrinsic == Some(StdIntrinsic::Net)
                && target.name == "listen"
                && values.len() == 1
            {
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_net_listen(ptr {})",
                    values[0]
                )
                .unwrap();
                return Ok(value);
            }
            if std_intrinsic == Some(StdIntrinsic::Net)
                && target.name == "connect"
                && values.len() == 2
            {
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_net_connect(ptr {}, i64 {})",
                    values[0], values[1]
                )
                .unwrap();
                return Ok(value);
            }
            if std_intrinsic == Some(StdIntrinsic::Net)
                && target.name == "closeListener"
                && values.len() == 1
            {
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call i32 @aura_llvm_net_close_listener(ptr {})",
                    values[0]
                )
                .unwrap();
                let result = next_temp(out);
                writeln!(out, "  {result} = icmp ne i32 {value}, 0").unwrap();
                return Ok(result);
            }
            if std_intrinsic == Some(StdIntrinsic::Net)
                && target.name == "closeStream"
                && values.len() == 1
            {
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call i32 @aura_llvm_net_close_stream(ptr {})",
                    values[0]
                )
                .unwrap();
                let result = next_temp(out);
                writeln!(out, "  {result} = icmp ne i32 {value}, 0").unwrap();
                return Ok(result);
            }
            if std_intrinsic == Some(StdIntrinsic::Net)
                && target.name == "readStream"
                && values.len() == 2
                && result_ty.is_some_and(|ty| matches!(ty, Ty::Task(_) | Ty::TaskHandle(_)))
            {
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                writeln!(out, "  {frame} = call ptr @aura_llvm_net_read_task(ptr {executor}, ptr {}, i64 {}, i64 0)", values[0], values[1]).unwrap();
                return Ok(frame);
            }
            if std_intrinsic == Some(StdIntrinsic::Net)
                && target.name == "writeStream"
                && values.len() == 2
                && result_ty.is_some_and(|ty| matches!(ty, Ty::Task(_) | Ty::TaskHandle(_)))
            {
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                writeln!(out, "  {frame} = call ptr @aura_llvm_net_write_task(ptr {executor}, ptr {}, ptr {}, i64 0)", values[0], values[1]).unwrap();
                return Ok(frame);
            }
            if std_intrinsic == Some(StdIntrinsic::Net)
                && matches!(
                    target.name.as_str(),
                    "readStreamResult" | "writeStreamResult"
                )
                && values.len() == 2
            {
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                if target.name == "readStreamResult" {
                    writeln!(out, "  {frame} = call ptr @aura_llvm_net_read_task(ptr {executor}, ptr {}, i64 {}, i64 0)", values[0], values[1]).unwrap();
                } else {
                    writeln!(out, "  {frame} = call ptr @aura_llvm_net_write_task(ptr {executor}, ptr {}, ptr {}, i64 0)", values[0], values[1]).unwrap();
                }
                return Ok(frame);
            }
            match (target.name.as_str(), values.len()) {
                ("exception_cause_count", 0) => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = call i64 @aura_ex_cause_count()").unwrap();
                    return Ok(value);
                }
                ("exception_source_span_start", 0) | ("exception_source_span_end", 0) => {
                    let helper = if target.name == "exception_source_span_start" {
                        "aura_ex_source_span_start"
                    } else {
                        "aura_ex_source_span_end"
                    };
                    let raw = next_temp(out);
                    writeln!(out, "  {raw} = call i32 @{helper}()").unwrap();
                    let value = next_temp(out);
                    writeln!(out, "  {value} = zext i32 {raw} to i64").unwrap();
                    return Ok(value);
                }
                ("exception_cause_type", 1) => {
                    let raw = next_temp(out);
                    writeln!(
                        out,
                        "  {raw} = call ptr @aura_ex_cause_type_copy(i64 {})",
                        values[0]
                    )
                    .unwrap();
                    let value = next_temp(out);
                    writeln!(out, "  {value} = call ptr @aura_llvm_str_new(ptr {raw})").unwrap();
                    return Ok(value);
                }
                ("exception_cause_span_start", 1) | ("exception_cause_span_end", 1) => {
                    let helper = if target.name == "exception_cause_span_start" {
                        "aura_ex_cause_span_start"
                    } else {
                        "aura_ex_cause_span_end"
                    };
                    let raw = next_temp(out);
                    writeln!(out, "  {raw} = call i32 @{helper}(i64 {})", values[0]).unwrap();
                    let value = next_temp(out);
                    writeln!(out, "  {value} = zext i32 {raw} to i64").unwrap();
                    return Ok(value);
                }
                ("exception_add_cause", 3) => {
                    let data = next_temp(out);
                    writeln!(
                        out,
                        "  {data} = call ptr @aura_llvm_str_data(ptr {})",
                        values[0]
                    )
                    .unwrap();
                    let start = next_temp(out);
                    writeln!(out, "  {start} = trunc i64 {} to i32", values[1]).unwrap();
                    let end = next_temp(out);
                    writeln!(out, "  {end} = trunc i64 {} to i32", values[2]).unwrap();
                    writeln!(
                        out,
                        "  call i32 @aura_ex_add_cause(ptr {data}, i32 {start}, i32 {end})"
                    )
                    .unwrap();
                    return Ok(String::new());
                }
                _ => {}
            }
            if (target.name == "toString" || target.name == "to_string") && values.len() == 1 {
                let operand_ty = &body.locals[args[0].local].ty;
                let value = next_temp(out);
                match operand_ty {
                    Ty::Int => writeln!(
                        out,
                        "  {value} = call ptr @aura_llvm_int_to_string(i64 {})",
                        values[0]
                    )
                    .unwrap(),
                    Ty::Float => writeln!(
                        out,
                        "  {value} = call ptr @aura_llvm_float_to_string(double {})",
                        values[0]
                    )
                    .unwrap(),
                    Ty::Bool => writeln!(
                        out,
                        "  {value} = call ptr @aura_llvm_bool_to_string(i1 {})",
                        values[0]
                    )
                    .unwrap(),
                    _ => return Err(unsupported("toString operand type")),
                }
                return Ok(value);
            }
            if target.name == "toFloat" && values.len() == 1 {
                return match body.locals[args[0].local].ty {
                    Ty::Int => {
                        let value = next_temp(out);
                        writeln!(out, "  {value} = sitofp i64 {} to double", values[0]).unwrap();
                        Ok(value)
                    }
                    Ty::Float => Ok(values[0].clone()),
                    _ => Err(unsupported("toFloat operand type")),
                };
            }
            if target.name == "toInt" && values.len() == 1 {
                return match body.locals[args[0].local].ty {
                    Ty::Float => {
                        let value = next_temp(out);
                        writeln!(out, "  {value} = fptosi double {} to i64", values[0]).unwrap();
                        Ok(value)
                    }
                    Ty::Int => Ok(values[0].clone()),
                    Ty::String => {
                        let value = next_temp(out);
                        writeln!(
                            out,
                            "  {value} = call %AuraLlvmOptInt @aura_llvm_str_to_int(ptr {})",
                            values[0]
                        )
                        .unwrap();
                        Ok(value)
                    }
                    _ => Err(unsupported("toInt operand type")),
                };
            }
            if matches!(
                target.name.as_str(),
                "__typeOf" | "typeOf" | "__typeIdOf" | "typeIdOf"
            ) && args.is_empty()
                && !target.type_args.is_empty()
            {
                let reflected = target.type_args[0].mono_suffix();
                let class_name = if target.name.contains("typeId") {
                    "TypeId"
                } else {
                    "Type"
                };
                let type_id = context
                    .class_type_ids
                    .get(class_name)
                    .copied()
                    .ok_or_else(|| unsupported("reflection class type id"))?;
                let value = next_temp(out);
                let field_count = if class_name == "Type" { 2 } else { 1 };
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_class_alloc(i64 {field_count}, i64 {type_id})"
                )
                .unwrap();
                let name_index = context
                    .string_literals
                    .iter()
                    .position(|literal| literal == &reflected)
                    .unwrap_or_else(|| {
                        context.string_literals.push(reflected.clone());
                        context.string_literals.len() - 1
                    });
                let name_address = next_temp(out);
                writeln!(
                    out,
                    "  {name_address} = getelementptr [{} x i8], ptr @.aura_str{}, i64 0, i64 0",
                    reflected.len() + 1,
                    name_index
                )
                .unwrap();
                let name_value = next_temp(out);
                writeln!(
                    out,
                    "  {name_value} = call ptr @aura_llvm_str_new(ptr {name_address})"
                )
                .unwrap();
                let field = next_temp(out);
                let field_index = if class_name == "Type" { 1 } else { 0 };
                writeln!(
                    out,
                    "  {field} = getelementptr %AuraLlvmClass, ptr {value}, i32 0, i32 1, i64 {field_index}"
                )
                .unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = ptrtoint ptr {name_value} to i64").unwrap();
                writeln!(out, "  store i64 {raw}, ptr {field}").unwrap();
                return Ok(value);
            }
            if let Some(value) =
                emit_builtin_method(out, target, args, &values, body, result_ty, context)?
            {
                return Ok(value);
            }
            if let Some(value) = emit_std_io_intrinsic(out, target, &values, body, result_ty)? {
                return Ok(value);
            }
            if let Some(variant) = target
                .variant
                .as_deref()
                .filter(|variant| *variant != "__iterable_protocol")
            {
                let info = context
                    .enum_variants
                    .get(variant)
                    .cloned()
                    .ok_or_else(|| unsupported(&format!("enum variant {variant}")))?;
                let fields = resolved_variant_fields(
                    &info,
                    &Ty::EnumApp {
                        name: target.name.clone(),
                        args: target.type_args.clone(),
                    },
                    &target.type_args,
                );
                if fields.len() != args.len() {
                    return Err(unsupported(&format!("enum constructor {variant} arity")));
                }
                if fields.iter().any(|(_, ty)| {
                    !(matches!(ty, Ty::Int | Ty::Bool | Ty::Float) || is_pointer_value_type(ty))
                }) {
                    return Err(unsupported("non-primitive enum payload"));
                }
                let destructor =
                    enum_destructor_symbol(target.variant.as_deref(), &fields, context);
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_enum_alloc(i64 {}, ptr {})",
                    args.len(),
                    destructor
                )
                .unwrap();
                let tag_address = next_temp(out);
                writeln!(
                    out,
                    "  {tag_address} = getelementptr %AuraLlvmEnum, ptr {value}, i32 0, i32 1"
                )
                .unwrap();
                writeln!(out, "  store i64 {}, ptr {tag_address}", info.tag).unwrap();
                for (index, ((_, ty), argument)) in fields.iter().zip(values.iter()).enumerate() {
                    let field_address = next_temp(out);
                    writeln!(
                        out,
                        "  {field_address} = getelementptr %AuraLlvmEnum, ptr {value}, i32 0, i32 3, i64 {index}"
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
                        Ty::String
                        | Ty::Class(_)
                        | Ty::ClassApp { .. }
                        | Ty::Interface(_)
                        | Ty::InterfaceApp { .. }
                        | Ty::Enum(_)
                        | Ty::EnumApp { .. } => {
                            retain_pointer_value(out, argument, ty)?;
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
                            raw
                        }
                        Ty::ForeignHandle(_) => {
                            retain_pointer_value(out, argument, ty)?;
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
                            raw
                        }
                        ty if is_pointer_value_type(ty) => {
                            retain_pointer_value(out, argument, ty)?;
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
                            raw
                        }
                        Ty::Nullable(inner) if is_pointer_value_type(inner) => {
                            retain_pointer_value(out, argument, ty)?;
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
                            raw
                        }
                        _ if is_pointer_value_type(ty) => {
                            retain_pointer_value(out, argument, ty)?;
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
                            raw
                        }
                        _ => {
                            return Err(unsupported(&format!("enum payload type {}", ty.display())))
                        }
                    };
                    writeln!(out, "  store i64 {raw}, ptr {field_address}").unwrap();
                }
                return Ok(value);
            }
            let is_constructor = target.is_constructor
                || (!target.type_args.is_empty() && context.classes.contains_key(&target.name));
            if is_constructor {
                if target.name == "Array" {
                    let Some(Ty::Int) = args.first().map(|place| &body.locals[place.local].ty)
                    else {
                        return Err(unsupported("Array constructor length"));
                    };
                    let element_ty = target
                        .type_args
                        .first()
                        .ok_or_else(|| unsupported("Array element type"))?;
                    let kind = array_kind(element_ty)?;
                    let value = next_temp(out);
                    writeln!(
                        out,
                        "  {value} = call ptr @aura_llvm_array_alloc(i64 {}, i64 {kind})",
                        values[0]
                    )
                    .unwrap();
                    return Ok(value);
                }
                let own_fields = class_own_fields(context, &target.name, &target.type_args)
                    .ok_or_else(|| unsupported(&format!("class {}", target.name)))?;
                if own_fields.len() != args.len()
                    || own_fields.iter().any(|(_, ty)| {
                        !(matches!(ty, Ty::Int | Ty::Bool | Ty::Float) || is_pointer_value_type(ty))
                    })
                {
                    return Err(unsupported(&format!(
                        "class constructor field type {}",
                        own_fields
                            .iter()
                            .find_map(|(_, ty)| (!matches!(ty, Ty::Int | Ty::Bool | Ty::Float)
                                && !is_pointer_value_type(ty))
                            .then(|| ty.display()))
                            .unwrap_or_default()
                    )));
                }
                let layout_fields = class_fields(context, &target.name, &target.type_args)
                    .ok_or_else(|| unsupported(&format!("class {}", target.name)))?;
                let inherited_count = layout_fields.len() - own_fields.len();
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_class_alloc(i64 {}, i64 {})",
                    layout_fields.len(),
                    context
                        .class_type_ids
                        .get(&target.name)
                        .copied()
                        .ok_or_else(|| unsupported("class type id"))?
                )
                .unwrap();
                if let Some(super_args) = context.class_superclass_args.get(&target.name).cloned() {
                    let Some(parent) = context.class_superclasses.get(&target.name) else {
                        return Err(unsupported(&format!(
                            "superclass constructor for {}",
                            target.name
                        )));
                    };
                    let parent_fields = class_fields(context, parent, &target.type_args)
                        .ok_or_else(|| {
                            unsupported(&format!(
                                "superclass constructor fields for {} (parent {})",
                                target.name, parent
                            ))
                        })?;
                    if super_args.len() != parent_fields.len() {
                        return Err(unsupported("superclass constructor arity"));
                    }
                    for (index, (expr, (_, field_ty))) in
                        super_args.iter().zip(parent_fields.iter()).enumerate()
                    {
                        let (argument, argument_ty) = emit_superclass_arg(
                            out,
                            expr,
                            &own_fields,
                            &values,
                            body,
                            package,
                            context,
                        )?;
                        if !types_compatible(&argument_ty, field_ty) {
                            return Err(unsupported("superclass constructor argument type"));
                        }
                        let address = next_temp(out);
                        writeln!(
                            out,
                            "  {address} = getelementptr %AuraLlvmClass, ptr {value}, i32 0, i32 1, i64 {index}"
                        )
                        .unwrap();
                        let raw = raw_class_field_value(
                            out,
                            &argument,
                            field_ty,
                            !is_moving_call_arg(&argument_ty, field_ty),
                        )?;
                        writeln!(out, "  store i64 {raw}, ptr {address}").unwrap();
                    }
                }
                for (own_index, ((_, ty), argument)) in
                    own_fields.iter().zip(values.iter()).enumerate()
                {
                    let index = inherited_count + own_index;
                    let address = next_temp(out);
                    writeln!(
                        out,
                        "  {address} = getelementptr %AuraLlvmClass, ptr {value}, i32 0, i32 1, i64 {index}"
                    )
                    .unwrap();
                    let source_ty = &body.locals[args[own_index].local].ty;
                    let raw = raw_class_field_value(
                        out,
                        argument,
                        ty,
                        !is_moving_call_arg(source_ty, ty),
                    )?;
                    writeln!(out, "  store i64 {raw}, ptr {address}").unwrap();
                }
                let field_types = own_fields
                    .iter()
                    .map(|(_, ty)| ty.clone())
                    .collect::<Vec<_>>();
                consume_owned_call_args(out, args, &field_types, body)?;
                return Ok(value);
            }
            if target.name == "send" && (args.len() == 2 || args.len() == 3) {
                let udp_receiver =
                    class_type_name(&body.locals[args[0].local].ty).is_some_and(|name| {
                        matches!(name.split('@').next(), Some("std_udp_Socket" | "Socket"))
                    });
                if std_intrinsic == Some(StdIntrinsic::Udp) || udp_receiver {
                    let socket_fields = class_fields(
                        context,
                        class_type_name(&body.locals[args[0].local].ty).unwrap_or("Socket"),
                        class_type_args(&body.locals[args[0].local].ty),
                    )
                    .ok_or_else(|| unsupported("UDP Socket class layout"))?;
                    let endpoint_index = socket_fields
                        .iter()
                        .position(|(name, _)| name == "endpoint")
                        .ok_or_else(|| unsupported("UDP Socket.endpoint field"))?;
                    let endpoint = next_temp(out);
                    let endpoint_address = next_temp(out);
                    writeln!(out, "  {endpoint_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {endpoint_index}", values[0]).unwrap();
                    let endpoint_raw = next_temp(out);
                    writeln!(out, "  {endpoint_raw} = load i64, ptr {endpoint_address}").unwrap();
                    writeln!(out, "  {endpoint} = inttoptr i64 {endpoint_raw} to ptr").unwrap();
                    let endpoint_fields = class_fields(context, "Endpoint", &[])
                        .or_else(|| class_fields(context, "std_udp_Endpoint", &[]))
                        .ok_or_else(|| unsupported("UDP Endpoint class layout"))?;
                    let host_index = endpoint_fields
                        .iter()
                        .position(|(name, _)| name == "host")
                        .ok_or_else(|| unsupported("UDP Endpoint.host field"))?;
                    let port_index = endpoint_fields
                        .iter()
                        .position(|(name, _)| name == "port")
                        .ok_or_else(|| unsupported("UDP Endpoint.port field"))?;
                    let host_address = next_temp(out);
                    writeln!(out, "  {host_address} = getelementptr %AuraLlvmClass, ptr {endpoint}, i32 0, i32 1, i64 {host_index}").unwrap();
                    let host_raw = next_temp(out);
                    writeln!(out, "  {host_raw} = load i64, ptr {host_address}").unwrap();
                    let host = next_temp(out);
                    writeln!(out, "  {host} = inttoptr i64 {host_raw} to ptr").unwrap();
                    let host_data = next_temp(out);
                    writeln!(
                        out,
                        "  {host_data} = call ptr @aura_llvm_str_data(ptr {host})"
                    )
                    .unwrap();
                    let port_address = next_temp(out);
                    writeln!(out, "  {port_address} = getelementptr %AuraLlvmClass, ptr {endpoint}, i32 0, i32 1, i64 {port_index}").unwrap();
                    let port = next_temp(out);
                    writeln!(out, "  {port} = load i64, ptr {port_address}").unwrap();
                    let target_endpoint = load_place(out, args[1], body)?;
                    let target_host_address = next_temp(out);
                    writeln!(out, "  {target_host_address} = getelementptr %AuraLlvmClass, ptr {target_endpoint}, i32 0, i32 1, i64 {host_index}").unwrap();
                    let target_host_raw = next_temp(out);
                    writeln!(
                        out,
                        "  {target_host_raw} = load i64, ptr {target_host_address}"
                    )
                    .unwrap();
                    let target_host = next_temp(out);
                    writeln!(
                        out,
                        "  {target_host} = inttoptr i64 {target_host_raw} to ptr"
                    )
                    .unwrap();
                    let target_host_data = next_temp(out);
                    writeln!(
                        out,
                        "  {target_host_data} = call ptr @aura_llvm_str_data(ptr {target_host})"
                    )
                    .unwrap();
                    let target_port_address = next_temp(out);
                    writeln!(out, "  {target_port_address} = getelementptr %AuraLlvmClass, ptr {target_endpoint}, i32 0, i32 1, i64 {port_index}").unwrap();
                    let target_port = next_temp(out);
                    writeln!(out, "  {target_port} = load i64, ptr {target_port_address}").unwrap();
                    let payload = load_place(out, args[2], body)?;
                    let payload_data = next_temp(out);
                    writeln!(
                        out,
                        "  {payload_data} = call ptr @aura_llvm_str_data(ptr {payload})"
                    )
                    .unwrap();
                    let bound = next_temp(out);
                    writeln!(
                        out,
                        "  {bound} = call i32 @aura_udp_bind(ptr {host_data}, i64 {port})"
                    )
                    .unwrap();
                    let sent = next_temp(out);
                    writeln!(out, "  {sent} = call i64 @aura_udp_send(ptr {host_data}, i64 {port}, ptr {target_host_data}, i64 {target_port}, ptr {payload_data})").unwrap();
                    let _ = bound;
                    let frame = next_temp(out);
                    writeln!(
                        out,
                        "  {frame} = call ptr @aura_llvm_task_immediate_i64(i64 {sent})"
                    )
                    .unwrap();
                    return Ok(frame);
                }
                let Ty::Channel(element_ty) = &body.locals[args[0].local].ty else {
                    return Err(unsupported("send target outside Channel"));
                };
                let source_ty = &body.locals[args[1].local].ty;
                let interface_coercion =
                    matches!(&**element_ty, Ty::Interface(_) | Ty::InterfaceApp { .. })
                        && is_class_type(source_ty);
                if !types_compatible(source_ty, element_ty) && !interface_coercion {
                    return Err(unsupported("channel send value type"));
                }
                let value = &values[1];
                if is_pointer_value_type(element_ty) {
                    retain_pointer_value(out, value, element_ty)?;
                }
                let raw = array_raw_value(out, value, element_ty)?;
                let sent = next_temp(out);
                writeln!(
                    out,
                    "  {sent} = call i1 @aura_llvm_channel_send(ptr {}, i64 {raw})",
                    values[0]
                )
                .unwrap();
                return Ok(String::new());
            }
            if target.name == "receive"
                && args.len() == 2
                && (std_intrinsic == Some(StdIntrinsic::Udp)
                    || class_type_name(&body.locals[args[0].local].ty).is_some_and(|name| {
                        matches!(name.split('@').next(), Some("std_udp_Socket" | "Socket"))
                    }))
            {
                let socket_fields = class_fields(
                    context,
                    class_type_name(&body.locals[args[0].local].ty).unwrap_or("Socket"),
                    class_type_args(&body.locals[args[0].local].ty),
                );
                let socket_fields = socket_fields
                    .or_else(|| class_fields(context, "Socket", &[]))
                    .ok_or_else(|| unsupported("UDP Socket class layout"))?;
                let endpoint_index = socket_fields
                    .iter()
                    .position(|(name, _)| name == "endpoint")
                    .ok_or_else(|| unsupported("UDP Socket.endpoint field"))?;
                let endpoint = next_temp(out);
                let endpoint_address = next_temp(out);
                writeln!(out, "  {endpoint_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {endpoint_index}", values[0]).unwrap();
                let endpoint_raw = next_temp(out);
                writeln!(out, "  {endpoint_raw} = load i64, ptr {endpoint_address}").unwrap();
                writeln!(out, "  {endpoint} = inttoptr i64 {endpoint_raw} to ptr").unwrap();
                let endpoint_fields = class_fields(context, "Endpoint", &[])
                    .or_else(|| class_fields(context, "std_udp_Endpoint", &[]))
                    .ok_or_else(|| unsupported("UDP Endpoint class layout"))?;
                let host_index = endpoint_fields
                    .iter()
                    .position(|(name, _)| name == "host")
                    .ok_or_else(|| unsupported("UDP Endpoint.host field"))?;
                let port_index = endpoint_fields
                    .iter()
                    .position(|(name, _)| name == "port")
                    .ok_or_else(|| unsupported("UDP Endpoint.port field"))?;
                let host_address = next_temp(out);
                writeln!(out, "  {host_address} = getelementptr %AuraLlvmClass, ptr {endpoint}, i32 0, i32 1, i64 {host_index}").unwrap();
                let host_raw = next_temp(out);
                writeln!(out, "  {host_raw} = load i64, ptr {host_address}").unwrap();
                let host = next_temp(out);
                writeln!(out, "  {host} = inttoptr i64 {host_raw} to ptr").unwrap();
                let host_data = next_temp(out);
                writeln!(
                    out,
                    "  {host_data} = call ptr @aura_llvm_str_data(ptr {host})"
                )
                .unwrap();
                let port_address = next_temp(out);
                writeln!(out, "  {port_address} = getelementptr %AuraLlvmClass, ptr {endpoint}, i32 0, i32 1, i64 {port_index}").unwrap();
                let port = next_temp(out);
                writeln!(out, "  {port} = load i64, ptr {port_address}").unwrap();
                let result_ty = result_ty.ok_or_else(|| unsupported("UDP receive result type"))?;
                let datagram_ty = task_payload_type(result_ty).unwrap_or(result_ty);
                let datagram_name = class_type_name(datagram_ty)
                    .ok_or_else(|| unsupported("UDP Datagram result type"))?;
                let datagram_type = context
                    .class_type_ids
                    .get(datagram_name)
                    .copied()
                    .ok_or_else(|| unsupported("UDP Datagram type id"))?;
                let endpoint_type = context
                    .class_type_ids
                    .get("std_udp_Endpoint")
                    .or_else(|| context.class_type_ids.get("Endpoint"))
                    .copied()
                    .ok_or_else(|| unsupported("UDP Endpoint type id"))?;
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let capacity = load_place(out, args[1], body)?;
                let frame = next_temp(out);
                writeln!(out, "  {frame} = call ptr @aura_llvm_udp_receive_task(ptr {executor}, ptr {host_data}, i64 {port}, i64 {capacity}, i64 {endpoint_type}, i64 {datagram_type})").unwrap();
                return Ok(frame);
            }
            if target.name == "close" && args.len() == 1 {
                if !matches!(body.locals[args[0].local].ty, Ty::Channel(_)) {
                    return Err(unsupported("close target outside Channel"));
                }
                writeln!(
                    out,
                    "  call void @aura_llvm_channel_close(ptr {})",
                    values[0]
                )
                .unwrap();
                return Ok(String::new());
            }
            if target.name == "receive" && args.len() == 1 {
                let Ty::Channel(element_ty) = &body.locals[args[0].local].ty else {
                    return Err(unsupported("receive target outside Channel"));
                };
                let result_ty =
                    result_ty.ok_or_else(|| unsupported("channel receive result type"))?;
                let nullable = matches!(result_ty, Ty::Nullable(inner) if inner.as_ref() == element_ty.as_ref());
                if *result_ty != **element_ty && !nullable {
                    return Err(unsupported(&format!(
                        "channel receive result type (channel={}, result={})",
                        body.locals[args[0].local].ty.display(),
                        result_ty.display()
                    )));
                }
                let raw_slot = next_temp(out);
                writeln!(out, "  {raw_slot} = alloca i64").unwrap();
                let received = next_temp(out);
                writeln!(
                    out,
                    "  {received} = call i1 @aura_llvm_channel_receive(ptr {}, ptr {raw_slot})",
                    values[0]
                )
                .unwrap();
                let raw = next_temp(out);
                writeln!(out, "  {raw} = load i64, ptr {raw_slot}").unwrap();
                let value = array_value_from_raw(out, raw, element_ty)?;
                if !nullable {
                    return Ok(value);
                }
                let value_ty = llvm_type(element_ty)?;
                let present = if matches!(element_ty.as_ref(), Ty::Int | Ty::Bool | Ty::Float) {
                    build_optional_value(out, llvm_type(result_ty)?, value_ty, &value)
                } else {
                    value
                };
                let zero = nullable_zero_value(Some(result_ty)).unwrap_or("null");
                let selected = next_temp(out);
                writeln!(
                    out,
                    "  {selected} = select i1 {received}, {} {present}, {} {zero}",
                    llvm_type(result_ty)?,
                    llvm_type(result_ty)?
                )
                .unwrap();
                return Ok(selected);
            }
            if target.name == "get"
                && args.len() == 2
                && array_element_type(&body.locals[args[0].local].ty).is_some()
            {
                let Some(element_ty) = array_element_type(&body.locals[args[0].local].ty) else {
                    return Err(unsupported("get target outside Array"));
                };
                if body.locals[args[1].local].ty != Ty::Int {
                    return Err(unsupported("Array get index type"));
                }
                return load_array_element(out, args[0], args[1], element_ty, body);
            }
            if matches!(target.name.as_str(), "get" | "aura__get")
                && args.len() == 2
                && body.locals[args[0].local].ty == Ty::String
                && body.locals[args[1].local].ty == Ty::Int
            {
                if result_ty == Some(&Ty::String) {
                    let byte = load_string_byte(out, args[0], args[1], body)?;
                    let string = next_temp(out);
                    writeln!(
                        out,
                        "  {string} = call ptr @aura_llvm_string_single_byte(i64 {byte})"
                    )
                    .unwrap();
                    return Ok(string);
                }
                return load_string_byte(out, args[0], args[1], body);
            }
            if target.name == "push"
                && args.len() == 2
                && array_element_type(&body.locals[args[0].local].ty).is_some()
            {
                let Some(element_ty) = array_element_type(&body.locals[args[0].local].ty) else {
                    return Err(unsupported("push target outside Array"));
                };
                let source_ty = &body.locals[args[1].local].ty;
                let interface_coercion =
                    matches!(element_ty, Ty::Interface(_) | Ty::InterfaceApp { .. })
                        && is_class_type(source_ty);
                if !types_compatible(source_ty, element_ty) && !interface_coercion {
                    return Err(unsupported("Array push value type"));
                }
                let value = &values[1];
                if is_pointer_value_type(element_ty) {
                    retain_pointer_value(out, value, element_ty)?;
                }
                let raw = array_raw_value(out, value, element_ty)?;
                writeln!(
                    out,
                    "  call void @aura_llvm_array_push(ptr {}, i64 {raw})",
                    values[0]
                )
                .unwrap();
                return Ok(String::new());
            }
            if target.name == "pop"
                && args.len() == 1
                && array_element_type(&body.locals[args[0].local].ty).is_some()
            {
                let Some(element_ty) = array_element_type(&body.locals[args[0].local].ty) else {
                    return Err(unsupported("pop target outside Array"));
                };
                let raw = next_temp(out);
                writeln!(
                    out,
                    "  {raw} = call i64 @aura_llvm_array_pop(ptr {})",
                    values[0]
                )
                .unwrap();
                return array_value_from_raw(out, raw, element_ty);
            }
            if target.name == "set"
                && args.len() == 3
                && array_element_type(&body.locals[args[0].local].ty).is_some()
            {
                let Some(element_ty) = array_element_type(&body.locals[args[0].local].ty) else {
                    return Err(unsupported("set target outside Array"));
                };
                let raw = array_raw_value(out, &values[2], element_ty)?;
                writeln!(
                    out,
                    "  call void @aura_llvm_array_set(ptr {}, i64 {}, i64 {raw})",
                    values[0], values[1]
                )
                .unwrap();
                return Ok(String::new());
            }
            let method_name =
                method_symbol_for(&context.signatures, target, args, body, package, result_ty);
            let argument_tys = args
                .iter()
                .map(|arg| body.locals[arg.local].ty.clone())
                .collect::<Vec<_>>();
            let generic_name =
                monomorphized_symbol_for(&context.signatures, target, package, &argument_tys);
            let name = if context.foreign_names.contains(&target.name) {
                target.name.clone()
            } else if !target.is_static && !args.is_empty() {
                method_name
                    .clone()
                    .or(generic_name.clone())
                    .unwrap_or_else(|| symbol_name(&target.package, &target.name))
            } else {
                generic_name
                    .clone()
                    .or(method_name.clone())
                    .clone()
                    .unwrap_or_else(|| symbol_name(&target.package, &target.name))
            };
            if name == "aura__get" {
                return Err(unsupported(&format!(
                    "unresolved iterable get dispatch for {} on {}",
                    target.name,
                    body.locals[args[0].local].ty.display()
                )));
            }
            if std_intrinsic == Some(StdIntrinsic::HttpServe)
                && target.name == "serveConnection"
                && values.len() == 2
            {
                let request_layout = context
                    .classes
                    .keys()
                    .find(|name| {
                        name.split('@')
                            .next()
                            .is_some_and(|base| base == "Request" || base.ends_with("_Request"))
                    })
                    .cloned()
                    .ok_or_else(|| unsupported("HTTP Request class layout"))?;
                let response_layout = context
                    .classes
                    .keys()
                    .find(|name| {
                        name.split('@')
                            .next()
                            .is_some_and(|base| base == "Response" || base.ends_with("_Response"))
                    })
                    .cloned()
                    .ok_or_else(|| unsupported("HTTP Response class layout"))?;
                let request_fields = class_fields(context, &request_layout, &[])
                    .ok_or_else(|| unsupported("HTTP Request class layout"))?;
                let response_fields = class_fields(context, &response_layout, &[])
                    .ok_or_else(|| unsupported("HTTP Response class layout"))?;
                let request_type_id = context
                    .class_type_ids
                    .get(&request_layout)
                    .copied()
                    .ok_or_else(|| unsupported("HTTP Request type id"))?;
                let response_type_id = context
                    .class_type_ids
                    .get(&response_layout)
                    .copied()
                    .ok_or_else(|| unsupported("HTTP Response type id"))?;
                let environment = next_temp(out);
                let function = next_temp(out);
                writeln!(
                    out,
                    "  {environment} = extractvalue %AuraLlvmFun {}, 0",
                    values[1]
                )
                .unwrap();
                writeln!(
                    out,
                    "  {function} = extractvalue %AuraLlvmFun {}, 1",
                    values[1]
                )
                .unwrap();
                writeln!(out, "  call void @aura_fun_env_retain(ptr {environment})").unwrap();
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                writeln!(out, "  {frame} = call ptr @aura_llvm_http_serve_connection_task(ptr {executor}, ptr {}, ptr {environment}, ptr {function}, i64 {}, i64 {}, i64 {}, i64 {})", values[0], request_fields.len(), request_type_id, response_fields.len(), response_type_id).unwrap();
                return Ok(frame);
            }
            if std_intrinsic == Some(StdIntrinsic::HttpServe)
                && target.name == "serve"
                && values.len() == 2
            {
                let request_layout = context
                    .classes
                    .keys()
                    .find(|name| {
                        name.split('@')
                            .next()
                            .is_some_and(|base| base == "Request" || base.ends_with("_Request"))
                    })
                    .cloned()
                    .ok_or_else(|| unsupported("HTTP Request class layout"))?;
                let response_layout = context
                    .classes
                    .keys()
                    .find(|name| {
                        name.split('@')
                            .next()
                            .is_some_and(|base| base == "Response" || base.ends_with("_Response"))
                    })
                    .cloned()
                    .ok_or_else(|| unsupported("HTTP Response class layout"))?;
                let request_fields = class_fields(context, &request_layout, &[])
                    .ok_or_else(|| unsupported("HTTP Request class layout"))?;
                let response_fields = class_fields(context, &response_layout, &[])
                    .ok_or_else(|| unsupported("HTTP Response class layout"))?;
                let request_type_id = context
                    .class_type_ids
                    .get(&request_layout)
                    .copied()
                    .ok_or_else(|| unsupported("HTTP Request type id"))?;
                let response_type_id = context
                    .class_type_ids
                    .get(&response_layout)
                    .copied()
                    .ok_or_else(|| unsupported("HTTP Response type id"))?;
                let environment = next_temp(out);
                let function = next_temp(out);
                writeln!(
                    out,
                    "  {environment} = extractvalue %AuraLlvmFun {}, 0",
                    values[1]
                )
                .unwrap();
                writeln!(
                    out,
                    "  {function} = extractvalue %AuraLlvmFun {}, 1",
                    values[1]
                )
                .unwrap();
                writeln!(out, "  call void @aura_fun_env_retain(ptr {environment})").unwrap();
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                let frame = next_temp(out);
                writeln!(out, "  {frame} = call ptr @aura_llvm_http_serve_task(ptr {executor}, ptr {}, ptr {environment}, ptr {function}, i64 {}, i64 {}, i64 {}, i64 {})", values[0], request_fields.len(), request_type_id, response_fields.len(), response_type_id).unwrap();
                return Ok(frame);
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
            let (return_ty, declared_parameter_tys) = method_name
                .as_deref()
                .and_then(|symbol| signature_for_symbol(&context.signatures, symbol))
                .or_else(|| {
                    generic_name
                        .as_deref()
                        .and_then(|symbol| signature_for_symbol(&context.signatures, symbol))
                })
                .or_else(|| signature_for(&context.signatures, package, target))
                .ok_or_else(|| {
                    unsupported(&format!(
                        "call target {}.{} (args {}, result {})",
                        target.package,
                        target.name,
                        values.len(),
                        result_ty.map_or_else(|| "<none>".to_owned(), Ty::display)
                    ))
                })?;
            let parameter_tys = declared_parameter_tys.clone();
            if parameter_tys.len() != values.len() {
                return Err(unsupported(&format!(
                    "call arity for {} (symbol {}, expected {}, got {})",
                    target.name,
                    name,
                    parameter_tys.len(),
                    values.len()
                )));
            }
            if target.is_safe && !target.is_static && !values.is_empty() {
                let receiver_ty = &body.locals[args[0].local].ty;
                if !is_pointer_value_type(receiver_ty) {
                    return Err(unsupported("safe call receiver type"));
                }
                let dispatch_targets = dynamic_method_targets(context, receiver_ty, target);
                if !dispatch_targets.is_empty() {
                    return Err(unsupported("safe dynamic method dispatch"));
                }
                let call_id = out.lines().count();
                let nonnull = next_temp(out);
                let null_label = format!("safe_null{call_id}");
                let call_label = format!("safe_call{call_id}");
                let join_label = format!("safe_join{call_id}");
                writeln!(out, "  {nonnull} = icmp ne ptr {}, null", values[0]).unwrap();
                writeln!(
                    out,
                    "  br i1 {nonnull}, label %{call_label}, label %{null_label}"
                )
                .unwrap();
                writeln!(out, "{null_label}:").unwrap();
                writeln!(out, "  br label %{join_label}").unwrap();
                writeln!(out, "{call_label}:").unwrap();
                if !context.foreign_names.contains(&target.name) {
                    for (index, value) in values.iter().enumerate() {
                        let source_ty = &body.locals[args[index].local].ty;
                        if is_pointer_value_type(source_ty)
                            && !is_moving_call_arg(source_ty, &parameter_tys[index])
                        {
                            retain_pointer_value(out, value, source_ty)?;
                        } else if matches!(source_ty, Ty::Fun { .. })
                            && !is_moving_call_arg(source_ty, &parameter_tys[index])
                        {
                            writeln!(
                                out,
                                "  call void @aura_llvm_fun_retain(%AuraLlvmFun {value})"
                            )
                            .unwrap();
                        }
                    }
                }
                let arguments = values
                    .iter()
                    .zip(&parameter_tys)
                    .enumerate()
                    .map(|(index, (value, ty))| {
                        let source_ty = &body.locals[args[index].local].ty;
                        let value = if target.name == "get"
                            && *source_ty == Ty::Int
                            && is_pointer_value_type(ty)
                        {
                            return Err(unsupported(
                                "generic get dispatch has no concrete element ABI",
                            ));
                        } else {
                            coerce_llvm_argument(out, value, source_ty, ty).map_err(|_| {
                                unsupported(&format!(
                                    "argument conversion in {} ({}) in {} from {} to {}",
                                    target.name,
                                    name,
                                    body.name,
                                    source_ty.display(),
                                    ty.display()
                                ))
                            })?
                        };
                        Ok(format!("{} {value}", llvm_type(ty)?))
                    })
                    .collect::<Result<Vec<_>, CodegenError>>()?
                    .join(", ");
                let llvm_return_ty = llvm_type(return_ty)?;
                if *return_ty == Ty::Unit {
                    writeln!(out, "  call void @{name}({arguments})").unwrap();
                    writeln!(out, "  br label %{join_label}").unwrap();
                    writeln!(out, "{join_label}:").unwrap();
                    return Ok(String::new());
                }
                let call_result = next_temp(out);
                writeln!(
                    out,
                    "  {call_result} = call {llvm_return_ty} @{name}({arguments})"
                )
                .unwrap();
                consume_owned_call_args(out, args, &parameter_tys, body)?;
                writeln!(out, "  br label %{join_label}").unwrap();
                writeln!(out, "{join_label}:").unwrap();
                let result = next_temp(out);
                let safe_result_ty = result_ty.unwrap_or(return_ty);
                let zero = match safe_result_ty {
                    Ty::Nullable(inner) if is_pointer_value_type(inner) => "null",
                    _ => nullable_zero_value(Some(safe_result_ty))
                        .ok_or_else(|| unsupported("safe call result type"))?,
                };
                writeln!(out, "  {result} = phi {llvm_return_ty} [{zero}, %{null_label}], [{call_result}, %{call_label}]").unwrap();
                return Ok(result);
            }
            if !context.foreign_names.contains(&target.name) {
                for (index, value) in values.iter().enumerate() {
                    let source_ty = &body.locals[args[index].local].ty;
                    if is_pointer_value_type(source_ty)
                        && !is_moving_call_arg(source_ty, &parameter_tys[index])
                    {
                        retain_pointer_value(out, value, source_ty)?;
                    } else if matches!(source_ty, Ty::Fun { .. })
                        && !is_moving_call_arg(source_ty, &parameter_tys[index])
                    {
                        writeln!(
                            out,
                            "  call void @aura_llvm_fun_retain(%AuraLlvmFun {value})"
                        )
                        .unwrap();
                    }
                }
            }
            let arguments = values
                .iter()
                .zip(&parameter_tys)
                .enumerate()
                .map(|(index, (value, ty))| {
                    let source_ty = &body.locals[args[index].local].ty;
                    let value = if target.name == "get"
                        && *source_ty == Ty::Int
                        && is_pointer_value_type(ty)
                    {
                        "null".to_owned()
                    } else {
                        coerce_llvm_argument(out, value, source_ty, ty).map_err(|_| {
                            unsupported(&format!(
                                "argument conversion in {} ({}) in {} from {} to {}",
                                target.name,
                                name,
                                body.name,
                                source_ty.display(),
                                ty.display()
                            ))
                        })?
                    };
                    Ok(format!("{} {value}", llvm_type(ty)?))
                })
                .collect::<Result<Vec<_>, CodegenError>>()?
                .join(", ");
            let dispatch_targets = if !target.is_static && !args.is_empty() {
                dynamic_method_targets(context, &body.locals[args[0].local].ty, target)
            } else {
                Vec::new()
            };
            if !dispatch_targets.is_empty() {
                let dispatch_id = out.lines().count();
                let tag = next_temp(out);
                writeln!(
                    out,
                    "  {tag} = call i64 @aura_llvm_class_type(ptr {})",
                    values[0]
                )
                .unwrap();
                let join = format!("dispatch_join{dispatch_id}");
                let mut incoming = Vec::new();
                for (index, (type_id, symbol)) in dispatch_targets.iter().enumerate() {
                    let case = format!("dispatch_case{dispatch_id}_{index}");
                    let next = format!("dispatch_next{dispatch_id}_{index}");
                    let expected = next_temp(out);
                    writeln!(out, "  {expected} = icmp eq i64 {tag}, {type_id}").unwrap();
                    writeln!(out, "  br i1 {expected}, label %{case}, label %{next}").unwrap();
                    writeln!(out, "{case}:").unwrap();
                    let Some((candidate_ret, _)) =
                        signature_for_symbol(&context.signatures, symbol)
                    else {
                        return Err(unsupported("dynamic method signature"));
                    };
                    if *candidate_ret == Ty::Unit {
                        writeln!(out, "  call void @{symbol}({arguments})").unwrap();
                    } else {
                        let result = next_temp(out);
                        writeln!(
                            out,
                            "  {result} = call {} @{symbol}({arguments})",
                            llvm_type(candidate_ret)?
                        )
                        .unwrap();
                        incoming.push(format!("[{result}, %{case}]"));
                    }
                    writeln!(out, "  br label %{join}").unwrap();
                    writeln!(out, "{next}:").unwrap();
                }
                if *return_ty == Ty::Unit {
                    writeln!(out, "  call void @{name}({arguments})").unwrap();
                    writeln!(out, "  br label %{join}").unwrap();
                    writeln!(out, "{join}:").unwrap();
                    return Ok(String::new());
                }
                let fallback_symbol = if target.variant.as_deref() == Some("__iterable_protocol") {
                    dispatch_targets
                        .first()
                        .map(|(_, symbol)| symbol.as_str())
                        .unwrap_or(&name)
                } else {
                    &name
                };
                let fallback_return_ty = signature_for_symbol(&context.signatures, fallback_symbol)
                    .map(|(ret, _)| ret)
                    .unwrap_or(return_ty);
                let fallback = next_temp(out);
                writeln!(
                    out,
                    "  {fallback} = call {} @{fallback_symbol}({arguments})",
                    llvm_type(fallback_return_ty)?
                )
                .unwrap();
                incoming.push(format!(
                    "[{fallback}, %dispatch_next{dispatch_id}_{}]",
                    dispatch_targets.len() - 1
                ));
                writeln!(out, "  br label %{join}").unwrap();
                writeln!(out, "{join}:").unwrap();
                let result = next_temp(out);
                writeln!(
                    out,
                    "  {result} = phi {} {}",
                    llvm_type(fallback_return_ty)?,
                    incoming.join(", ")
                )
                .unwrap();
                return Ok(result);
            }
            if *return_ty == Ty::Unit {
                writeln!(out, "  call void @{name}({arguments})").unwrap();
                consume_owned_call_args(out, args, &parameter_tys, body)?;
                return Ok(String::new());
            }
            let temp = next_temp(out);
            writeln!(
                out,
                "  {temp} = call {} @{name}({arguments})",
                llvm_type(return_ty)?
            )
            .unwrap();
            consume_owned_call_args(out, args, &parameter_tys, body)?;
            Ok(temp)
        }
        Rvalue::Unwrap { operand } => {
            let value = load_place(out, *operand, body)?;
            if matches!(
                &body.locals[operand.local].ty,
                Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Int | Ty::Bool | Ty::Float)
            ) {
                let payload = next_temp(out);
                writeln!(
                    out,
                    "  {payload} = extractvalue {} {value}, 1",
                    llvm_type(&body.locals[operand.local].ty)?
                )
                .unwrap();
                return Ok(payload);
            }
            if is_string_type(&body.locals[operand.local].ty) {
                writeln!(out, "  call void @aura_llvm_str_retain(ptr {value})").unwrap();
            }
            Ok(value)
        }
        Rvalue::TypeTest { operand, ty } => {
            let value = load_place(out, *operand, body)?;
            if matches!(
                &body.locals[operand.local].ty,
                Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Int | Ty::Bool | Ty::Float)
            ) {
                let present = next_temp(out);
                writeln!(
                    out,
                    "  {present} = extractvalue {} {value}, 0",
                    llvm_type(&body.locals[operand.local].ty)?
                )
                .unwrap();
                return Ok(present);
            }
            if is_string_type(&body.locals[operand.local].ty) {
                let temp = next_temp(out);
                writeln!(out, "  {temp} = icmp ne ptr {value}, null").unwrap();
                return Ok(temp);
            }
            let (Ty::Class(name) | Ty::ClassApp { name, .. }) = ty else {
                return Err(unsupported("type tests outside nominal heap values"));
            };
            let class = name.split('@').next().unwrap_or(name);
            let type_id = context
                .class_type_ids
                .get(class)
                .copied()
                .ok_or_else(|| unsupported("type test class"))?;
            let runtime_type = next_temp(out);
            writeln!(
                out,
                "  {runtime_type} = call i64 @aura_llvm_class_type(ptr {value})"
            )
            .unwrap();
            let result = next_temp(out);
            writeln!(out, "  {result} = icmp eq i64 {runtime_type}, {type_id}").unwrap();
            Ok(result)
        }
        Rvalue::Length(place) => {
            let value = load_place(out, *place, body)?;
            let temp = next_temp(out);
            if is_string_type(&body.locals[place.local].ty) {
                writeln!(out, "  {temp} = call i64 @aura_llvm_str_len(ptr {value})").unwrap();
            } else if is_array_type(&body.locals[place.local].ty) {
                writeln!(out, "  {temp} = call i64 @aura_llvm_array_len(ptr {value})").unwrap();
            } else {
                return Err(unsupported("length outside String/Array"));
            }
            Ok(temp)
        }
        Rvalue::Index { collection, index } => {
            if is_string_type(&body.locals[collection.local].ty) {
                load_string_byte(out, *collection, *index, body)
            } else if let Some(element_ty) = array_element_type(&body.locals[collection.local].ty) {
                load_array_element(out, *collection, *index, element_ty, body)
            } else {
                Err(unsupported("indexing non-String/Array values"))
            }
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
        Rvalue::Field { object, field } => {
            let object_ty = &body.locals[object.local].ty;
            if field == "len" && is_string_type(object_ty) {
                let object = load_place(out, *object, body)?;
                let value = next_temp(out);
                writeln!(out, "  {value} = call i64 @aura_llvm_str_len(ptr {object})").unwrap();
                return Ok(value);
            }
            if field == "len" && is_array_type(object_ty) {
                let object = load_place(out, *object, body)?;
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call i64 @aura_llvm_array_len(ptr {object})"
                )
                .unwrap();
                return Ok(value);
            }
            let name =
                class_type_name(object_ty).ok_or_else(|| unsupported("fields outside classes"))?;
            let type_args = class_type_args(object_ty);
            let fields = class_fields(context, name, type_args)
                .ok_or_else(|| unsupported(&format!("class {name}")))?;
            let (index, field_ty) = fields
                .iter()
                .enumerate()
                .find(|(_, (candidate, _))| candidate == field)
                .map(|(index, (_, ty))| (index, ty))
                .ok_or_else(|| unsupported(&format!("class field {name}.{field}")))?;
            let object = load_place(out, *object, body)?;
            let address = next_temp(out);
            writeln!(
                out,
                "  {address} = getelementptr %AuraLlvmClass, ptr {object}, i32 0, i32 1, i64 {index}"
            )
            .unwrap();
            let raw = next_temp(out);
            writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
            match field_ty {
                Ty::Int => Ok(raw),
                Ty::Bool => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = trunc i64 {raw} to i1").unwrap();
                    Ok(value)
                }
                Ty::Float => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = bitcast i64 {raw} to double").unwrap();
                    Ok(value)
                }
                ty if is_pointer_value_type(ty) => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                    retain_pointer_value(out, &value, field_ty)?;
                    Ok(value)
                }
                _ => Err(unsupported(&format!(
                    "class field load type {} in {} on {}",
                    field_ty.display(),
                    body.name,
                    object_ty.display()
                ))),
            }
        }
        Rvalue::Intrinsic(intrinsic) => match intrinsic {
            Intrinsic::GcCollect => {
                writeln!(out, "  call void @aura_llvm_gc_collect()").unwrap();
                Ok(String::new())
            }
            Intrinsic::ExceptionString => {
                let raw = next_temp(out);
                writeln!(out, "  {raw} = call ptr @aura_ex_as_string()").unwrap();
                let value = next_temp(out);
                writeln!(out, "  {value} = call ptr @aura_llvm_str_new(ptr {raw})").unwrap();
                Ok(value)
            }
            Intrinsic::ExceptionInt => {
                let value = next_temp(out);
                writeln!(out, "  {value} = call i64 @aura_ex_as_int()").unwrap();
                Ok(value)
            }
            Intrinsic::ExceptionBool => {
                let value = next_temp(out);
                writeln!(out, "  {value} = call i1 @aura_ex_as_bool()").unwrap();
                Ok(value)
            }
            Intrinsic::ExceptionObject => {
                let value = next_temp(out);
                writeln!(out, "  {value} = call ptr @aura_ex_take_obj()").unwrap();
                Ok(value)
            }
        },
        Rvalue::AsyncOp(operation) => {
            emit_async_op(out, operation, body, result_ty, package, context)
        }
    }
}

fn emit_builtin_method(
    out: &mut String,
    target: &aura_mir::mir::CallTarget,
    args: &[Place],
    values: &[String],
    body: &MirBody,
    result_ty: Option<&Ty>,
    context: &EmitContext,
) -> Result<Option<String>, CodegenError> {
    let Some(receiver) = args.first() else {
        return Ok(None);
    };
    let receiver_ty = &body.locals[receiver.local].ty;
    let is_json_value = matches!(receiver_ty, Ty::Class(name) | Ty::ClassApp { name, .. }
        if matches!(name.split('@').next(), Some("std_json_Value" | "Value")));
    if matches!(target.name.as_str(), "get" | "at" | "aura__get")
        && is_json_value
        && matches!(result_ty, Some(Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Class(_) | Ty::ClassApp { .. }))
        && values.len() == 2
    {
        let object_lookup = target.name != "at" && body.locals[args[1].local].ty == Ty::String;
        let result_name = match result_ty {
            Some(Ty::Nullable(inner)) => match inner.as_ref() {
                Ty::Class(name) | Ty::ClassApp { name, .. } => name,
                _ => unreachable!("JSON lookup result checked above"),
            },
            _ => unreachable!("JSON lookup result checked above"),
        };
        let value_layout_name = context
            .classes
            .keys()
            .find(|name| matches!(name.split('@').next(), Some("std_json_Value" | "Value")))
            .cloned()
            .ok_or_else(|| unsupported("std.json.Value class layout"))?;
        let value_fields = class_fields(context, &value_layout_name, &[])
            .ok_or_else(|| unsupported("std.json.Value class layout"))?;
        let text_index = value_fields
            .iter()
            .position(|(name, ty)| name == "text" && is_string_type(ty))
            .ok_or_else(|| unsupported("std.json.Value.text field"))?;
        let result_layout_name = context
            .classes
            .keys()
            .find(|name| {
                name == &result_name || name.split('@').next() == result_name.split('@').next()
            })
            .cloned()
            .unwrap_or_else(|| result_name.clone());
        let result_type_id = context
            .class_type_ids
            .get(&result_layout_name)
            .copied()
            .ok_or_else(|| unsupported("std.json.Value type id"))?;
        let text_address = next_temp(out);
        writeln!(out, "  {text_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {text_index}", values[0]).unwrap();
        let text_raw = next_temp(out);
        writeln!(out, "  {text_raw} = load i64, ptr {text_address}").unwrap();
        let text = next_temp(out);
        writeln!(out, "  {text} = inttoptr i64 {text_raw} to ptr").unwrap();
        let data = next_temp(out);
        writeln!(out, "  {data} = call ptr @aura_llvm_str_data(ptr {text})").unwrap();
        let raw = next_temp(out);
        if object_lookup {
            let key = next_temp(out);
            writeln!(
                out,
                "  {key} = call ptr @aura_llvm_str_data(ptr {})",
                values[1]
            )
            .unwrap();
            writeln!(
                out,
                "  {raw} = call ptr @aura_json_object_get(ptr {data}, ptr {key})"
            )
            .unwrap();
        } else {
            writeln!(
                out,
                "  {raw} = call ptr @aura_json_array_at(ptr {data}, i64 {})",
                values[1]
            )
            .unwrap();
        }
        let present = next_temp(out);
        writeln!(out, "  {present} = icmp ne ptr {raw}, null").unwrap();
        let label_id = out.len();
        let good = format!("json_lookup_good_{label_id}");
        let bad = format!("json_lookup_bad_{label_id}");
        let merge = format!("json_lookup_merge_{label_id}");
        writeln!(out, "  br i1 {present}, label %{good}, label %{bad}").unwrap();
        writeln!(out, "{good}:").unwrap();
        let owned_text = next_temp(out);
        writeln!(
            out,
            "  {owned_text} = call ptr @aura_llvm_str_new(ptr {raw})"
        )
        .unwrap();
        let result = next_temp(out);
        writeln!(
            out,
            "  {result} = call ptr @aura_llvm_class_alloc(i64 1, i64 {result_type_id})"
        )
        .unwrap();
        let field = next_temp(out);
        writeln!(
            out,
            "  {field} = getelementptr %AuraLlvmClass, ptr {result}, i32 0, i32 1, i64 0"
        )
        .unwrap();
        let field_raw = next_temp(out);
        writeln!(out, "  {field_raw} = ptrtoint ptr {owned_text} to i64").unwrap();
        writeln!(out, "  store i64 {field_raw}, ptr {field}").unwrap();
        writeln!(out, "  br label %{merge}").unwrap();
        writeln!(out, "{bad}:").unwrap();
        writeln!(out, "  br label %{merge}").unwrap();
        writeln!(out, "{merge}:").unwrap();
        let merged = next_temp(out);
        writeln!(
            out,
            "  {merged} = phi ptr [{result}, %{good}], [null, %{bad}]"
        )
        .unwrap();
        return Ok(Some(merged));
    }
    let is_bytes_buffer = matches!(receiver_ty, Ty::Class(name) | Ty::ClassApp { name, .. }
        if matches!(name.split('@').next(), Some("std_bytes_Buffer" | "Buffer")));
    if matches!(target.name.as_str(), "slice" | "__slice")
        && values.len() == 3
        && is_bytes_buffer
        && matches!(result_ty, Some(Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Class(_) | Ty::ClassApp { .. }))
    {
        let result_name = match result_ty {
            Some(Ty::Nullable(inner)) => match inner.as_ref() {
                Ty::Class(name) | Ty::ClassApp { name, .. } => name,
                _ => unreachable!("byte slice result checked above"),
            },
            _ => unreachable!("byte slice result checked above"),
        };
        let buffer_layout_name = context
            .classes
            .keys()
            .find(|name| matches!(name.split('@').next(), Some("std_bytes_Buffer" | "Buffer")))
            .cloned()
            .ok_or_else(|| unsupported("std.bytes.Buffer class layout"))?;
        let buffer_fields = class_fields(context, &buffer_layout_name, &[])
            .ok_or_else(|| unsupported("std.bytes.Buffer class layout"))?;
        let values_index = buffer_fields
            .iter()
            .position(|(name, ty)| name == "values" && is_array_type(ty))
            .ok_or_else(|| unsupported("std.bytes.Buffer.values field"))?;
        let result_layout_name = context
            .classes
            .keys()
            .find(|name| {
                name == &result_name || name.split('@').next() == result_name.split('@').next()
            })
            .cloned()
            .unwrap_or_else(|| result_name.clone());
        let result_type_id = context
            .class_type_ids
            .get(&result_layout_name)
            .copied()
            .ok_or_else(|| unsupported("std.bytes.Buffer type id"))?;
        let source_address = next_temp(out);
        writeln!(out, "  {source_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {values_index}", values[0]).unwrap();
        let source_raw = next_temp(out);
        writeln!(out, "  {source_raw} = load i64, ptr {source_address}").unwrap();
        let source_array = next_temp(out);
        writeln!(out, "  {source_array} = inttoptr i64 {source_raw} to ptr").unwrap();
        let source_len = next_temp(out);
        writeln!(
            out,
            "  {source_len} = call i64 @aura_llvm_array_len(ptr {source_array})"
        )
        .unwrap();
        let valid_start = next_temp(out);
        writeln!(
            out,
            "  {valid_start} = icmp ule i64 {}, {source_len}",
            values[1]
        )
        .unwrap();
        let remaining = next_temp(out);
        writeln!(out, "  {remaining} = sub i64 {source_len}, {}", values[1]).unwrap();
        let valid_length = next_temp(out);
        writeln!(
            out,
            "  {valid_length} = icmp ule i64 {}, {remaining}",
            values[2]
        )
        .unwrap();
        let valid = next_temp(out);
        writeln!(out, "  {valid} = and i1 {valid_start}, {valid_length}").unwrap();
        let label_id = out.len();
        let good = format!("byte_slice_good_{label_id}");
        let bad = format!("byte_slice_bad_{label_id}");
        let merge = format!("byte_slice_merge_{label_id}");
        writeln!(out, "  br i1 {valid}, label %{good}, label %{bad}").unwrap();
        writeln!(out, "{good}:").unwrap();
        let result_array = next_temp(out);
        writeln!(
            out,
            "  {result_array} = call ptr @aura_llvm_array_alloc(i64 {}, i64 0)",
            values[2]
        )
        .unwrap();
        let counter = next_temp(out);
        writeln!(out, "  {counter} = alloca i64").unwrap();
        writeln!(out, "  store i64 0, ptr {counter}").unwrap();
        let loop_label = format!("byte_slice_loop_{label_id}");
        let done_label = format!("byte_slice_done_{label_id}");
        writeln!(out, "  br label %{loop_label}").unwrap();
        writeln!(out, "{loop_label}:").unwrap();
        let index = next_temp(out);
        writeln!(out, "  {index} = load i64, ptr {counter}").unwrap();
        let done = next_temp(out);
        writeln!(out, "  {done} = icmp uge i64 {index}, {}", values[2]).unwrap();
        writeln!(
            out,
            "  br i1 {done}, label %{done_label}, label %{loop_label}_body"
        )
        .unwrap();
        writeln!(out, "{loop_label}_body:").unwrap();
        let source_index = next_temp(out);
        writeln!(out, "  {source_index} = add i64 {}, {index}", values[1]).unwrap();
        let element = next_temp(out);
        writeln!(
            out,
            "  {element} = call i64 @aura_llvm_array_get(ptr {source_array}, i64 {source_index})"
        )
        .unwrap();
        writeln!(
            out,
            "  call void @aura_llvm_array_set(ptr {result_array}, i64 {index}, i64 {element})"
        )
        .unwrap();
        let next = next_temp(out);
        writeln!(out, "  {next} = add i64 {index}, 1").unwrap();
        writeln!(out, "  store i64 {next}, ptr {counter}").unwrap();
        writeln!(out, "  br label %{loop_label}").unwrap();
        writeln!(out, "{done_label}:").unwrap();
        let result = next_temp(out);
        writeln!(
            out,
            "  {result} = call ptr @aura_llvm_class_alloc(i64 1, i64 {result_type_id})"
        )
        .unwrap();
        let result_field = next_temp(out);
        writeln!(
            out,
            "  {result_field} = getelementptr %AuraLlvmClass, ptr {result}, i32 0, i32 1, i64 0"
        )
        .unwrap();
        let result_raw = next_temp(out);
        writeln!(out, "  {result_raw} = ptrtoint ptr {result_array} to i64").unwrap();
        writeln!(out, "  store i64 {result_raw}, ptr {result_field}").unwrap();
        writeln!(out, "  br label %{merge}").unwrap();
        writeln!(out, "{bad}:").unwrap();
        writeln!(out, "  br label %{merge}").unwrap();
        writeln!(out, "{merge}:").unwrap();
        let merged = next_temp(out);
        writeln!(
            out,
            "  {merged} = phi ptr [{result}, %{done_label}], [null, %{bad}]"
        )
        .unwrap();
        return Ok(Some(merged));
    }
    if matches!(target.name.as_str(), "readByte" | "__readByte")
        && values.len() == 2
        && is_bytes_buffer
        && matches!(result_ty, Some(Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Class(_) | Ty::ClassApp { .. }))
    {
        let byte_name = match result_ty {
            Some(Ty::Nullable(inner)) => match inner.as_ref() {
                Ty::Class(name) | Ty::ClassApp { name, .. } => name,
                _ => unreachable!("readByte result checked above"),
            },
            _ => unreachable!("readByte result checked above"),
        };
        let byte_layout_name = context
            .classes
            .keys()
            .find(|name| {
                name == &byte_name || name.split('@').next() == byte_name.split('@').next()
            })
            .cloned()
            .unwrap_or_else(|| byte_name.clone());
        let byte_fields = class_fields(context, &byte_layout_name, &[])
            .ok_or_else(|| unsupported("std.bytes.Byte class layout"))?;
        if byte_fields.len() != 1 || byte_fields[0].1 != Ty::Int {
            return Err(unsupported("std.bytes.Byte class layout"));
        }
        let buffer_layout_name = context
            .classes
            .keys()
            .find(|name| matches!(name.split('@').next(), Some("std_bytes_Buffer" | "Buffer")))
            .cloned()
            .ok_or_else(|| unsupported("std.bytes.Buffer class layout"))?;
        let values_field = class_fields(context, &buffer_layout_name, &[])
            .ok_or_else(|| unsupported("std.bytes.Buffer class layout"))?;
        let values_index = values_field
            .iter()
            .position(|(name, ty)| name == "values" && is_array_type(ty))
            .ok_or_else(|| unsupported("std.bytes.Buffer.values field"))?;
        let values_address = next_temp(out);
        writeln!(
            out,
            "  {values_address} = getelementptr %AuraLlvmClass, ptr {}, i32 0, i32 1, i64 {values_index}",
            values[0]
        )
        .unwrap();
        let values_raw = next_temp(out);
        writeln!(out, "  {values_raw} = load i64, ptr {values_address}").unwrap();
        let array = next_temp(out);
        writeln!(out, "  {array} = inttoptr i64 {values_raw} to ptr").unwrap();
        let length = next_temp(out);
        writeln!(
            out,
            "  {length} = call i64 @aura_llvm_array_len(ptr {array})"
        )
        .unwrap();
        let in_bounds = next_temp(out);
        writeln!(out, "  {in_bounds} = icmp ult i64 {}, {length}", values[1]).unwrap();
        let label_id = out.len();
        let present_label = format!("read_byte_present_{label_id}");
        let absent_label = format!("read_byte_absent_{label_id}");
        let merge_label = format!("read_byte_merge_{label_id}");
        writeln!(
            out,
            "  br i1 {in_bounds}, label %{present_label}, label %{absent_label}"
        )
        .unwrap();
        writeln!(out, "{present_label}:").unwrap();
        let raw = next_temp(out);
        writeln!(
            out,
            "  {raw} = call i64 @aura_llvm_array_get(ptr {array}, i64 {})",
            values[1]
        )
        .unwrap();
        let byte = next_temp(out);
        let byte_type_id = context
            .class_type_ids
            .get(&byte_layout_name)
            .copied()
            .ok_or_else(|| unsupported("std.bytes.Byte type id"))?;
        writeln!(
            out,
            "  {byte} = call ptr @aura_llvm_class_alloc(i64 1, i64 {byte_type_id})"
        )
        .unwrap();
        let byte_field = next_temp(out);
        writeln!(
            out,
            "  {byte_field} = getelementptr %AuraLlvmClass, ptr {byte}, i32 0, i32 1, i64 0"
        )
        .unwrap();
        writeln!(out, "  store i64 {raw}, ptr {byte_field}").unwrap();
        writeln!(out, "  br label %{merge_label}").unwrap();
        writeln!(out, "{absent_label}:").unwrap();
        writeln!(out, "  br label %{merge_label}").unwrap();
        writeln!(out, "{merge_label}:").unwrap();
        let result = next_temp(out);
        writeln!(
            out,
            "  {result} = phi ptr [{byte}, %{present_label}], [null, %{absent_label}]"
        )
        .unwrap();
        return Ok(Some(result));
    }
    if target.name == "hash" && values.len() == 1 {
        let result = next_temp(out);
        match receiver_ty {
            Ty::String => {
                writeln!(
                    out,
                    "  {result} = call i64 @aura_hash_string(ptr {})",
                    values[0]
                )
                .unwrap();
            }
            Ty::Int => {
                writeln!(out, "  {result} = add i64 {}, 0", values[0]).unwrap();
            }
            _ => return Ok(None),
        }
        return Ok(Some(result));
    }
    if is_string_type(receiver_ty) {
        let helper = match (target.name.as_str(), values.len()) {
            ("isEmpty", 1) => "aura_llvm_str_is_empty",
            ("startsWith", 2) => "aura_llvm_str_starts_with",
            ("contains", 2) => "aura_llvm_str_contains",
            ("endsWith", 2) => "aura_llvm_str_ends_with",
            ("charAt", 2) => "aura_llvm_str_char_at",
            ("indexOf", 2) => "aura_llvm_str_index_of",
            ("substring", 3) => "aura_llvm_str_substring",
            ("trim", 1) => "aura_llvm_str_trim",
            ("trimStart", 1) => "aura_llvm_str_trim",
            ("trimEnd", 1) => "aura_llvm_str_trim",
            ("toLower", 1) => "aura_llvm_str_case",
            ("toUpper", 1) => "aura_llvm_str_case",
            ("split", 2) => "aura_llvm_str_split",
            _ => return Ok(None),
        };
        let result = next_temp(out);
        match target.name.as_str() {
            "isEmpty" | "startsWith" | "contains" | "endsWith" => {
                let arguments = values.join(", ");
                writeln!(
                    out,
                    "  {result} = call i1 @{helper}(ptr {})",
                    arguments.replace(", ", ", ptr ")
                )
                .unwrap();
            }
            "charAt" => {
                writeln!(
                    out,
                    "  {result} = call i64 @{helper}(ptr {}, i64 {})",
                    values[0], values[1]
                )
                .unwrap();
            }
            "indexOf" => {
                writeln!(
                    out,
                    "  {result} = call i64 @{helper}(ptr {}, ptr {})",
                    values[0], values[1]
                )
                .unwrap();
            }
            "substring" => {
                writeln!(
                    out,
                    "  {result} = call ptr @{helper}(ptr {}, i64 {}, i64 {})",
                    values[0], values[1], values[2]
                )
                .unwrap();
            }
            "trim" | "trimStart" | "trimEnd" => {
                let mode = match target.name.as_str() {
                    "trimStart" => 1,
                    "trimEnd" => 2,
                    _ => 0,
                };
                writeln!(
                    out,
                    "  {result} = call ptr @{helper}(ptr {}, i64 {mode})",
                    values[0]
                )
                .unwrap();
            }
            "toLower" | "toUpper" => {
                let upper = i32::from(target.name == "toUpper");
                writeln!(
                    out,
                    "  {result} = call ptr @{helper}(ptr {}, i1 {})",
                    values[0], upper
                )
                .unwrap();
            }
            "split" => {
                writeln!(
                    out,
                    "  {result} = call ptr @{helper}(ptr {}, ptr {})",
                    values[0], values[1]
                )
                .unwrap();
            }
            _ => unreachable!("builtin string helper shape checked above"),
        }
        return Ok(Some(result));
    }
    if let Some(element_ty) = array_element_type(receiver_ty) {
        let helper = match (target.name.as_str(), values.len()) {
            ("clone", 1) => "clone",
            ("clear", 1) => "clear",
            ("reserve", 2) => "reserve",
            ("isEmpty", 1) => "is_empty",
            _ => return Ok(None),
        };
        let _ = array_kind(element_ty)?;
        match helper {
            "clone" => {
                let result = next_temp(out);
                writeln!(
                    out,
                    "  {result} = call ptr @aura_llvm_array_clone(ptr {})",
                    values[0]
                )
                .unwrap();
                return Ok(Some(result));
            }
            "clear" => {
                writeln!(out, "  call void @aura_llvm_array_clear(ptr {})", values[0]).unwrap();
                return Ok(Some(String::new()));
            }
            "reserve" => {
                writeln!(
                    out,
                    "  call void @aura_llvm_array_reserve(ptr {}, i64 {})",
                    values[0], values[1]
                )
                .unwrap();
                return Ok(Some(String::new()));
            }
            "is_empty" => {
                let result = next_temp(out);
                writeln!(
                    out,
                    "  {result} = call i1 @aura_llvm_array_is_empty(ptr {})",
                    values[0]
                )
                .unwrap();
                return Ok(Some(result));
            }
            _ => unreachable!("builtin array helper shape checked above"),
        }
    }
    Ok(None)
}

fn emit_std_io_intrinsic(
    out: &mut String,
    target: &aura_mir::mir::CallTarget,
    values: &[String],
    body: &MirBody,
    result_ty: Option<&Ty>,
) -> Result<Option<String>, CodegenError> {
    if lookup_std_intrinsic(&target.package, &target.name)
        .is_none_or(|spec| spec.intrinsic != StdIntrinsic::IoFd)
    {
        return Ok(None);
    }
    if target.name == "readFd"
        && values.len() == 2
        && result_ty.is_some_and(|ty| matches!(ty, Ty::Task(_) | Ty::TaskHandle(_)))
    {
        let executor = next_temp(out);
        writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
        let frame = next_temp(out);
        writeln!(
            out,
            "  {frame} = call ptr @aura_llvm_io_read_fd_task(ptr {executor}, i64 {}, i64 {})",
            values[0], values[1]
        )
        .unwrap();
        return Ok(Some(frame));
    }
    if target.name == "readFdResult"
        && values.len() == 2
        && result_ty.is_some_and(|ty| matches!(ty, Ty::Task(_) | Ty::TaskHandle(_)))
    {
        let executor = next_temp(out);
        writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
        let frame = next_temp(out);
        writeln!(out, "  {frame} = call ptr @aura_llvm_io_read_fd_result_task(ptr {executor}, i64 {}, i64 {}, i64 0, i64 1, ptr @aura_llvm_enum_drop_string, ptr @aura_llvm_enum_drop_string)", values[0], values[1]).unwrap();
        return Ok(Some(frame));
    }
    if target.name == "writeFd"
        && values.len() == 2
        && result_ty.is_some_and(|ty| matches!(ty, Ty::Task(_) | Ty::TaskHandle(_)))
    {
        let executor = next_temp(out);
        writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
        let frame = next_temp(out);
        writeln!(
            out,
            "  {frame} = call ptr @aura_llvm_io_write_fd_task(ptr {executor}, i64 {}, ptr {})",
            values[0], values[1]
        )
        .unwrap();
        return Ok(Some(frame));
    }
    if target.name == "writeFdResult"
        && values.len() == 2
        && result_ty.is_some_and(|ty| matches!(ty, Ty::Task(_) | Ty::TaskHandle(_)))
    {
        let executor = next_temp(out);
        writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
        let frame = next_temp(out);
        writeln!(out, "  {frame} = call ptr @aura_llvm_io_write_fd_result_task(ptr {executor}, i64 {}, ptr {}, i64 0, i64 1, ptr null, ptr @aura_llvm_enum_drop_string)", values[0], values[1]).unwrap();
        return Ok(Some(frame));
    }
    let mut string_data = |value: &str| {
        let data = next_temp(out);
        writeln!(out, "  {data} = call ptr @aura_llvm_str_data(ptr {value})").unwrap();
        data
    };
    let value = match (target.name.as_str(), values.len()) {
        ("print", 1) | ("println", 1) | ("eprint", 1) | ("eprintln", 1) => {
            let data = string_data(&values[0]);
            let helper = match target.name.as_str() {
                "print" => "aura_print",
                "println" => "aura_println",
                "eprint" => "aura_eprint",
                _ => "aura_eprintln",
            };
            writeln!(out, "  call void @{helper}(ptr {data})").unwrap();
            String::new()
        }
        ("readFile", 1) | ("tryReadFile", 1) => {
            let path = string_data(&values[0]);
            let raw = next_temp(out);
            let helper = if target.name == "readFile" {
                "aura_read_file"
            } else {
                "aura_try_read_file"
            };
            writeln!(out, "  {raw} = call ptr @{helper}(ptr {path})").unwrap();
            let result = next_temp(out);
            let constructor = if target.name == "readFile" {
                "aura_llvm_str_new"
            } else {
                "aura_llvm_str_new_nullable"
            };
            writeln!(out, "  {result} = call ptr @{constructor}(ptr {raw})").unwrap();
            writeln!(out, "  call void @free(ptr {raw})").unwrap();
            result
        }
        ("writeFile", 2) | ("appendFile", 2) | ("tryWriteFile", 2) => {
            let path = string_data(&values[0]);
            let content = string_data(&values[1]);
            let result = next_temp(out);
            match target.name.as_str() {
                "writeFile" => {
                    writeln!(
                        out,
                        "  call void @aura_write_file(ptr {path}, ptr {content})"
                    )
                    .unwrap();
                    return Ok(Some(String::new()));
                }
                "appendFile" => {
                    writeln!(
                        out,
                        "  call void @aura_append_file(ptr {path}, ptr {content})"
                    )
                    .unwrap();
                    return Ok(Some(String::new()));
                }
                _ => writeln!(
                    out,
                    "  {result} = call i1 @aura_try_write_file(ptr {path}, ptr {content})"
                )
                .unwrap(),
            }
            result
        }
        ("fileExists", 1) => {
            let path = string_data(&values[0]);
            let result = next_temp(out);
            writeln!(out, "  {result} = call i1 @aura_file_exists(ptr {path})").unwrap();
            result
        }
        ("fileSize", 1) => {
            let path = string_data(&values[0]);
            let result = next_temp(out);
            writeln!(out, "  {result} = call i64 @aura_file_size(ptr {path})").unwrap();
            result
        }
        ("readLine", 0) | ("readAllStdin", 0) => {
            let raw = next_temp(out);
            let helper = if target.name == "readLine" {
                "aura_read_line"
            } else {
                "aura_read_all_stdin"
            };
            writeln!(out, "  {raw} = call ptr @{helper}()").unwrap();
            let result = next_temp(out);
            let constructor = if target.name == "readLine" {
                "aura_llvm_str_new_nullable"
            } else {
                "aura_llvm_str_new"
            };
            writeln!(out, "  {result} = call ptr @{constructor}(ptr {raw})").unwrap();
            writeln!(out, "  call void @free(ptr {raw})").unwrap();
            result
        }
        ("args", 0) => {
            let result = next_temp(out);
            writeln!(out, "  {result} = call ptr @aura_llvm_args()").unwrap();
            result
        }
        ("exit", 1) => {
            writeln!(out, "  call void @aura_exit(i64 {})", values[0]).unwrap();
            String::new()
        }
        ("readLineResult", 0) => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = call ptr @aura_read_line()").unwrap();
            let value = next_temp(out);
            writeln!(
                out,
                "  {value} = call ptr @aura_llvm_str_new_nullable(ptr {raw})"
            )
            .unwrap();
            writeln!(out, "  call void @free(ptr {raw})").unwrap();
            return Ok(Some(emit_result_variant(out, result_ty, &value, 0, true)?));
        }
        ("readAllStdinResult", 0) => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = call ptr @aura_read_all_stdin()").unwrap();
            let value = next_temp(out);
            writeln!(out, "  {value} = call ptr @aura_llvm_str_new(ptr {raw})").unwrap();
            writeln!(out, "  call void @free(ptr {raw})").unwrap();
            return Ok(Some(emit_result_variant(out, result_ty, &value, 0, true)?));
        }
        ("fileExistsResult", 1) => {
            let path = string_data(&values[0]);
            let value = next_temp(out);
            writeln!(out, "  {value} = call i1 @aura_file_exists(ptr {path})").unwrap();
            return Ok(Some(emit_result_variant(out, result_ty, &value, 0, false)?));
        }
        ("fileSizeResult", 1) => {
            let path = string_data(&values[0]);
            let value = next_temp(out);
            writeln!(out, "  {value} = call i64 @aura_file_size(ptr {path})").unwrap();
            return Ok(Some(emit_result_variant(out, result_ty, &value, 0, false)?));
        }
        _ => return Ok(None),
    };
    let _ = body;
    Ok(Some(value))
}

fn emit_result_variant(
    out: &mut String,
    result_ty: Option<&Ty>,
    value: &str,
    tag: i64,
    pointer_payload: bool,
) -> Result<String, CodegenError> {
    emit_result_variant_at(out, result_ty, value, tag, pointer_payload, 0)
}

fn emit_result_variant_at(
    out: &mut String,
    result_ty: Option<&Ty>,
    value: &str,
    tag: i64,
    pointer_payload: bool,
    payload_index: usize,
) -> Result<String, CodegenError> {
    let Ty::EnumApp { args, .. } = result_ty.ok_or_else(|| unsupported("Result intrinsic type"))?
    else {
        return Err(unsupported("Result intrinsic type"));
    };
    let payload_ty = args
        .get(payload_index)
        .ok_or_else(|| unsupported("Result intrinsic payload type"))?;
    let result = next_temp(out);
    let destructor = enum_payload_destructor_symbol(payload_ty);
    writeln!(
        out,
        "  {result} = call ptr @aura_llvm_enum_alloc(i64 1, ptr {destructor})"
    )
    .unwrap();
    let tag_address = next_temp(out);
    writeln!(
        out,
        "  {tag_address} = getelementptr %AuraLlvmEnum, ptr {result}, i32 0, i32 1"
    )
    .unwrap();
    writeln!(out, "  store i64 {tag}, ptr {tag_address}").unwrap();
    let field_address = next_temp(out);
    writeln!(
        out,
        "  {field_address} = getelementptr %AuraLlvmEnum, ptr {result}, i32 0, i32 3, i64 0"
    )
    .unwrap();
    let raw = if pointer_payload {
        retain_pointer_value(out, value, payload_ty)?;
        let raw = next_temp(out);
        writeln!(out, "  {raw} = ptrtoint ptr {value} to i64").unwrap();
        raw
    } else {
        match payload_ty {
            Ty::Bool => {
                let raw = next_temp(out);
                writeln!(out, "  {raw} = zext i1 {value} to i64").unwrap();
                raw
            }
            Ty::Int => value.to_owned(),
            _ => return Err(unsupported("Result intrinsic payload type")),
        }
    };
    writeln!(out, "  store i64 {raw}, ptr {field_address}").unwrap();
    Ok(result)
}

fn emit_task_error_variant(
    out: &mut String,
    task_error_ty: &Ty,
    message: &str,
    context: &mut EmitContext,
) -> Result<String, CodegenError> {
    let info = context
        .enum_variants
        .get("Failed")
        .cloned()
        .ok_or_else(|| unsupported("TaskError.Failed variant"))?;
    let fields = resolved_variant_fields(&info, task_error_ty, &[]);
    if fields.len() != 1 || fields[0].1 != Ty::String {
        return Err(unsupported("TaskError.Failed payload"));
    }
    emit_pointer_enum_variant(out, "Failed", &info, &Ty::String, message, context)
}

fn emit_task_cancelled_variant(
    out: &mut String,
    task_error_ty: &Ty,
    context: &mut EmitContext,
) -> Result<String, CodegenError> {
    let info = context
        .enum_variants
        .get("Cancelled")
        .cloned()
        .ok_or_else(|| unsupported("TaskError.Cancelled variant"))?;
    let fields = resolved_variant_fields(&info, task_error_ty, &[]);
    if !fields.is_empty() {
        return Err(unsupported("TaskError.Cancelled payload"));
    }
    let result = next_temp(out);
    let destructor = enum_destructor_symbol(Some("Cancelled"), &fields, context);
    writeln!(
        out,
        "  {result} = call ptr @aura_llvm_enum_alloc(i64 0, ptr {destructor})"
    )
    .unwrap();
    let tag = next_temp(out);
    writeln!(
        out,
        "  {tag} = getelementptr %AuraLlvmEnum, ptr {result}, i32 0, i32 1"
    )
    .unwrap();
    writeln!(out, "  store i64 {}, ptr {tag}", info.tag).unwrap();
    Ok(result)
}

fn emit_pointer_enum_variant(
    out: &mut String,
    variant: &str,
    info: &EnumVariantInfo,
    payload_ty: &Ty,
    payload: &str,
    context: &mut EmitContext,
) -> Result<String, CodegenError> {
    let fields = vec![("value".to_owned(), payload_ty.clone())];
    let destructor = enum_destructor_symbol(Some(variant), &fields, context);
    let result = next_temp(out);
    writeln!(
        out,
        "  {result} = call ptr @aura_llvm_enum_alloc(i64 1, ptr {destructor})"
    )
    .unwrap();
    let tag = next_temp(out);
    writeln!(
        out,
        "  {tag} = getelementptr %AuraLlvmEnum, ptr {result}, i32 0, i32 1"
    )
    .unwrap();
    writeln!(out, "  store i64 {}, ptr {tag}", info.tag).unwrap();
    let field = next_temp(out);
    writeln!(
        out,
        "  {field} = getelementptr %AuraLlvmEnum, ptr {result}, i32 0, i32 3, i64 0"
    )
    .unwrap();
    retain_pointer_value(out, payload, payload_ty)?;
    let raw = next_temp(out);
    writeln!(out, "  {raw} = ptrtoint ptr {payload} to i64").unwrap();
    writeln!(out, "  store i64 {raw}, ptr {field}").unwrap();
    Ok(result)
}

fn emit_superclass_arg(
    out: &mut String,
    expr: &SuperclassArgIr,
    own_fields: &[(String, Ty)],
    values: &[String],
    body: &MirBody,
    package: &str,
    context: &mut EmitContext,
) -> Result<(String, Ty), CodegenError> {
    match expr {
        SuperclassArgIr::Int(value) => Ok((value.to_string(), Ty::Int)),
        SuperclassArgIr::Bool(value) => {
            Ok((if *value { "true" } else { "false" }.into(), Ty::Bool))
        }
        SuperclassArgIr::Float(value) => {
            Ok((format_float_constant(f64::from_bits(*value)), Ty::Float))
        }
        SuperclassArgIr::String(value) => {
            let rendered = emit_rvalue(
                out,
                &Rvalue::ConstString(value.clone()),
                body,
                Some(&Ty::String),
                package,
                context,
            )?;
            Ok((rendered, Ty::String))
        }
        SuperclassArgIr::OwnField(name) => {
            let Some(index) = own_fields.iter().position(|(field, _)| field == name) else {
                return Err(unsupported("superclass constructor field reference"));
            };
            Ok((values[index].clone(), own_fields[index].1.clone()))
        }
        SuperclassArgIr::Unsupported => Err(unsupported("superclass constructor argument")),
    }
}

fn raw_class_field_value(
    out: &mut String,
    argument: &str,
    ty: &Ty,
    retain: bool,
) -> Result<String, CodegenError> {
    match ty {
        Ty::Int => Ok(argument.into()),
        Ty::Bool => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = zext i1 {argument} to i64").unwrap();
            Ok(raw)
        }
        Ty::Float => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = bitcast double {argument} to i64").unwrap();
            Ok(raw)
        }
        Ty::String
        | Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::Interface(_)
        | Ty::InterfaceApp { .. }
        | Ty::Enum(_)
        | Ty::EnumApp { .. }
        | Ty::ForeignHandle(_) => {
            if retain {
                retain_pointer_value(out, argument, ty)?;
            }
            let raw = next_temp(out);
            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
            Ok(raw)
        }
        ty if is_pointer_value_type(ty) => {
            if retain {
                retain_pointer_value(out, argument, ty)?;
            }
            let raw = next_temp(out);
            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
            Ok(raw)
        }
        _ => Err(unsupported(&format!(
            "class field store type {}",
            ty.display()
        ))),
    }
}

fn emit_to_string(out: &mut String, value: &str, ty: &Ty) -> Result<String, CodegenError> {
    let temp = next_temp(out);
    match ty {
        Ty::Int => writeln!(
            out,
            "  {temp} = call ptr @aura_llvm_int_to_string(i64 {value})"
        )
        .unwrap(),
        Ty::Float => writeln!(
            out,
            "  {temp} = call ptr @aura_llvm_float_to_string(double {value})"
        )
        .unwrap(),
        Ty::Bool => writeln!(
            out,
            "  {temp} = call ptr @aura_llvm_bool_to_string(i1 {value})"
        )
        .unwrap(),
        Ty::String => return Ok(value.into()),
        _ => return Err(unsupported("String conversion operand")),
    }
    Ok(temp)
}

pub(super) fn emit_async_op(
    out: &mut String,
    operation: &aura_mir::mir::AsyncOp,
    body: &MirBody,
    result_ty: Option<&Ty>,
    package: &str,
    context: &mut EmitContext,
) -> Result<String, CodegenError> {
    use aura_mir::mir::AsyncOp;

    match operation {
        AsyncOp::Spawn {
            body: task_body,
            captures,
        } => {
            if captures.len() > task_body.locals.len() {
                return Err(unsupported("spawn capture arity"));
            }
            if !(matches!(task_body.return_ty, Ty::Unit | Ty::Int)
                || is_pointer_value_type(&task_body.return_ty))
                || captures.iter().any(|capture| {
                    let ty = &body.locals[capture.source.local].ty;
                    !(matches!(ty, Ty::Int | Ty::Bool | Ty::Float) || is_pointer_value_type(ty))
                })
            {
                return Err(unsupported("scheduler-backed spawn payload"));
            }
            let parameter_tys = context
                .signatures
                .get(&(package.to_owned(), task_body.name.clone()))
                .map(|(_, params)| params.clone())
                .unwrap_or_else(|| {
                    task_body
                        .locals
                        .iter()
                        .take(captures.len())
                        .map(|local| local.ty.clone())
                        .collect()
                });
            if parameter_tys.len() != captures.len()
                || parameter_tys.iter().any(|ty| {
                    !(matches!(ty, Ty::Int | Ty::Bool | Ty::Float) || is_pointer_value_type(ty))
                })
            {
                return Err(unsupported("scheduler-backed spawn capture types"));
            }
            let values = captures
                .iter()
                .map(|capture| {
                    if capture.by_ref {
                        let value = next_temp(out);
                        writeln!(
                            out,
                            "  {value} = load ptr, ptr %slot{}",
                            capture.source.local
                        )
                        .unwrap();
                        Ok(value)
                    } else {
                        load_place(out, capture.source, body)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let executor = next_temp(out);
            writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
            let frame = next_temp(out);
            let poll = format!("@aura_llvm_poll_{}", symbol_name(package, &task_body.name));
            let data_size = captures.len() * 8;
            writeln!(
                out,
                "  {frame} = call ptr @aura_task_frame_new(i64 {data_size}, ptr {poll}, ptr null)"
            )
            .unwrap();
            let has_gc_captures = parameter_tys.iter().enumerate().any(|(index, ty)| {
                captures.get(index).is_some_and(|capture| capture.by_ref)
                    || is_pointer_value_type(ty)
            });
            if has_gc_captures {
                let drop = format!("@aura_llvm_drop_{}", symbol_name(package, &task_body.name));
                let mark = format!("@aura_llvm_mark_{}", symbol_name(package, &task_body.name));
                writeln!(
                    out,
                    "  call void @aura_task_frame_set_data_drop(ptr {frame}, ptr {drop})"
                )
                .unwrap();
                writeln!(
                    out,
                    "  call void @aura_task_frame_set_gc_mark(ptr {frame}, ptr {mark})"
                )
                .unwrap();
                let stack_map = format!(
                    "@aura_llvm_stack_map_{}",
                    symbol_name(package, &task_body.name)
                );
                let slot_count = parameter_tys
                    .iter()
                    .enumerate()
                    .filter(|(index, ty)| {
                        captures.get(*index).is_some_and(|capture| capture.by_ref)
                            || is_pointer_value_type(ty)
                    })
                    .count();
                let map_ptr = next_temp(out);
                writeln!(
                    out,
                    "  {map_ptr} = getelementptr inbounds [{slot_count} x i32], ptr {stack_map}, i64 0, i64 0"
                )
                .unwrap();
                writeln!(
                    out,
                    "  call void @aura_task_frame_set_gc_stack_map(ptr {frame}, ptr {map_ptr}, i64 {slot_count})"
                )
                .unwrap();
            }
            if data_size != 0 {
                let data = next_temp(out);
                writeln!(
                    out,
                    "  {data} = call ptr @aura_task_frame_data(ptr {frame})"
                )
                .unwrap();
                for (index, (value, ty)) in values.iter().zip(&parameter_tys).enumerate() {
                    let address = next_temp(out);
                    writeln!(
                        out,
                        "  {address} = getelementptr i64, ptr {data}, i64 {index}"
                    )
                    .unwrap();
                    let raw = if captures[index].by_ref {
                        let helper = match ty {
                            Ty::Int => "aura_box_i64_retain",
                            Ty::Bool => "aura_box_bool_retain",
                            Ty::Float => "aura_box_f64_retain",
                            Ty::String => "aura_box_str_retain",
                            Ty::Class(_)
                            | Ty::ClassApp { .. }
                            | Ty::Interface(_)
                            | Ty::InterfaceApp { .. }
                            | Ty::Fun { .. }
                            | Ty::Task(_)
                            | Ty::TaskHandle(_)
                            | Ty::Channel(_) => "aura_box_ptr_retain",
                            _ => return Err(unsupported("mutable spawn capture type")),
                        };
                        writeln!(out, "  call void @{helper}(ptr {})", value).unwrap();
                        let raw = next_temp(out);
                        writeln!(out, "  {raw} = ptrtoint ptr {value} to i64").unwrap();
                        raw
                    } else {
                        match ty {
                            Ty::Int => value.clone(),
                            Ty::Bool => {
                                let raw = next_temp(out);
                                writeln!(out, "  {raw} = zext i1 {value} to i64").unwrap();
                                raw
                            }
                            Ty::Float => {
                                let raw = next_temp(out);
                                writeln!(out, "  {raw} = bitcast double {value} to i64").unwrap();
                                raw
                            }
                            ty if is_pointer_value_type(ty) => {
                                retain_pointer_value(out, value, ty)?;
                                let raw = next_temp(out);
                                writeln!(out, "  {raw} = ptrtoint ptr {value} to i64").unwrap();
                                raw
                            }
                            _ => return Err(unsupported("scheduler-backed spawn capture type")),
                        }
                    };
                    writeln!(out, "  store i64 {raw}, ptr {address}").unwrap();
                }
            }
            let submitted = next_temp(out);
            writeln!(
                out,
                "  {submitted} = call i32 @aura_task_executor_submit(ptr {executor}, ptr {frame})"
            )
            .unwrap();
            let _ = (submitted, body, context);
            Ok(frame)
        }
        AsyncOp::Join(handle) => {
            let handle_ty = &body.locals[handle.local].ty;
            let Some(payload_ty) = task_payload_type(handle_ty) else {
                return Err(unsupported("joining a non-task handle"));
            };
            let handle_value = load_place(out, *handle, body)?;
            let executor = next_temp(out);
            writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
            if *payload_ty == Ty::Unit {
                let status = next_temp(out);
                writeln!(
                    out,
                    "  {status} = call i32 @aura_llvm_task_join_status(ptr {executor}, ptr {handle_value})"
                )
                .unwrap();
                if result_ty.is_some_and(|ty| ty.mono_suffix().ends_with("Result_Unit_TaskError")) {
                    let ok_info = context
                        .enum_variants
                        .get("Ok")
                        .cloned()
                        .ok_or_else(|| unsupported("task outcome Ok variant"))?;
                    let err_info = context
                        .enum_variants
                        .get("Err")
                        .cloned()
                        .ok_or_else(|| unsupported("task outcome Err variant"))?;
                    let ok = next_temp(out);
                    writeln!(
                        out,
                        "  {ok} = call ptr @aura_llvm_enum_alloc(i64 1, ptr null)"
                    )
                    .unwrap();
                    let ok_tag = next_temp(out);
                    writeln!(
                        out,
                        "  {ok_tag} = getelementptr %AuraLlvmEnum, ptr {ok}, i32 0, i32 1"
                    )
                    .unwrap();
                    writeln!(out, "  store i64 {}, ptr {ok_tag}", ok_info.tag).unwrap();
                    let ok_field = next_temp(out);
                    writeln!(
                        out,
                        "  {ok_field} = getelementptr %AuraLlvmEnum, ptr {ok}, i32 0, i32 3, i64 0"
                    )
                    .unwrap();
                    writeln!(out, "  store i64 0, ptr {ok_field}").unwrap();

                    let error_text = next_temp(out);
                    writeln!(out, "  {error_text} = call ptr @aura_llvm_task_error_message(ptr {handle_value})").unwrap();
                    let error_string = next_temp(out);
                    writeln!(
                        out,
                        "  {error_string} = call ptr @aura_llvm_str_new(ptr {error_text})"
                    )
                    .unwrap();
                    let task_error_ty = Ty::Enum("std_io_TaskError".into());
                    let failed_error =
                        emit_task_error_variant(out, &task_error_ty, &error_string, context)?;
                    let failed = emit_pointer_enum_variant(
                        out,
                        "Err",
                        &err_info,
                        &task_error_ty,
                        &failed_error,
                        context,
                    )?;
                    let cancelled_error =
                        emit_task_cancelled_variant(out, &task_error_ty, context)?;
                    let cancelled = emit_pointer_enum_variant(
                        out,
                        "Err",
                        &err_info,
                        &task_error_ty,
                        &cancelled_error,
                        context,
                    )?;
                    let is_cancelled = next_temp(out);
                    writeln!(out, "  {is_cancelled} = icmp eq i32 {status}, 4").unwrap();
                    let error = next_temp(out);
                    writeln!(
                        out,
                        "  {error} = select i1 {is_cancelled}, ptr {cancelled}, ptr {failed}"
                    )
                    .unwrap();
                    let complete = next_temp(out);
                    writeln!(out, "  {complete} = icmp eq i32 {status}, 2").unwrap();
                    let selected = next_temp(out);
                    writeln!(
                        out,
                        "  {selected} = select i1 {complete}, ptr {ok}, ptr {error}"
                    )
                    .unwrap();
                    writeln!(out, "  call void @aura_llvm_enum_retain(ptr {selected})").unwrap();
                    writeln!(out, "  call void @aura_llvm_enum_release(ptr {ok})").unwrap();
                    writeln!(out, "  call void @aura_llvm_enum_release(ptr {failed})").unwrap();
                    writeln!(out, "  call void @aura_llvm_enum_release(ptr {cancelled})").unwrap();
                    return Ok(selected);
                }
                if let Some(Ty::EnumApp { args, .. }) = result_ty {
                    if args.len() == 2 && context.enum_variants.contains_key("Err") {
                        let ok_info = context
                            .enum_variants
                            .get("Ok")
                            .cloned()
                            .ok_or_else(|| unsupported("unit join Ok variant"))?;
                        let ok_fields = resolved_variant_fields(
                            &ok_info,
                            result_ty.expect("result type is present"),
                            args,
                        );
                        if ok_fields.len() > 1
                            || ok_fields
                                .first()
                                .is_some_and(|(_, ty)| !matches!(ty, Ty::Int | Ty::Unit))
                        {
                            return Err(unsupported("unit join Ok payload"));
                        }
                        let ok = next_temp(out);
                        let ok_drop = enum_destructor_symbol(Some("Ok"), &ok_fields, context);
                        writeln!(
                            out,
                            "  {ok} = call ptr @aura_llvm_enum_alloc(i64 {}, ptr {ok_drop})",
                            ok_fields.len()
                        )
                        .unwrap();
                        let ok_tag = next_temp(out);
                        writeln!(
                            out,
                            "  {ok_tag} = getelementptr %AuraLlvmEnum, ptr {ok}, i32 0, i32 1"
                        )
                        .unwrap();
                        writeln!(out, "  store i64 {}, ptr {ok_tag}", ok_info.tag).unwrap();
                        if !ok_fields.is_empty() {
                            let ok_field = next_temp(out);
                            writeln!(out, "  {ok_field} = getelementptr %AuraLlvmEnum, ptr {ok}, i32 0, i32 3, i64 0").unwrap();
                            writeln!(out, "  store i64 0, ptr {ok_field}").unwrap();
                        }

                        let error_text = next_temp(out);
                        writeln!(out, "  {error_text} = call ptr @aura_llvm_task_error_message(ptr {handle_value})").unwrap();
                        let error_string = next_temp(out);
                        writeln!(
                            out,
                            "  {error_string} = call ptr @aura_llvm_str_new(ptr {error_text})"
                        )
                        .unwrap();
                        let task_error_ty = args[1].clone();
                        let failed_error =
                            emit_task_error_variant(out, &task_error_ty, &error_string, context)?;
                        let failed =
                            emit_result_variant_at(out, result_ty, &failed_error, 1, true, 1)?;
                        let cancelled_error =
                            emit_task_cancelled_variant(out, &task_error_ty, context)?;
                        let cancelled =
                            emit_result_variant_at(out, result_ty, &cancelled_error, 1, true, 1)?;
                        let is_cancelled = next_temp(out);
                        writeln!(out, "  {is_cancelled} = icmp eq i32 {status}, 4").unwrap();
                        let error = next_temp(out);
                        writeln!(
                            out,
                            "  {error} = select i1 {is_cancelled}, ptr {cancelled}, ptr {failed}"
                        )
                        .unwrap();
                        let complete = next_temp(out);
                        writeln!(out, "  {complete} = icmp eq i32 {status}, 2").unwrap();
                        let selected = next_temp(out);
                        writeln!(
                            out,
                            "  {selected} = select i1 {complete}, ptr {ok}, ptr {error}"
                        )
                        .unwrap();
                        writeln!(out, "  call void @aura_llvm_enum_retain(ptr {selected})")
                            .unwrap();
                        writeln!(out, "  call void @aura_llvm_enum_release(ptr {ok})").unwrap();
                        writeln!(out, "  call void @aura_llvm_enum_release(ptr {failed})").unwrap();
                        writeln!(out, "  call void @aura_llvm_enum_release(ptr {cancelled})")
                            .unwrap();
                        return Ok(selected);
                    }
                }
                if matches!(result_ty, Some(Ty::Enum(_) | Ty::EnumApp { .. })) {
                    let result = next_temp(out);
                    writeln!(
                        out,
                        "  {result} = call ptr @aura_llvm_enum_alloc(i64 0, ptr null)"
                    )
                    .unwrap();
                    let tag = next_temp(out);
                    writeln!(
                        out,
                        "  {tag} = getelementptr %AuraLlvmEnum, ptr {result}, i32 0, i32 1"
                    )
                    .unwrap();
                    writeln!(out, "  store i64 0, ptr {tag}").unwrap();
                    return Ok(result);
                }
                if let Some(result_type) = result_ty {
                    let result_args = match result_type {
                        Ty::EnumApp { args, .. } => args.as_slice(),
                        Ty::Enum(_) => &[],
                        _ => &[],
                    };
                    let variant = if context.enum_variants.contains_key("Ok") {
                        "Ok"
                    } else if context.enum_variants.contains_key("OutcomeOk") {
                        "OutcomeOk"
                    } else {
                        ""
                    };
                    if !variant.is_empty() {
                        let info = context
                            .enum_variants
                            .get(variant)
                            .cloned()
                            .ok_or_else(|| unsupported("unit join success variant"))?;
                        let fields = resolved_variant_fields(&info, result_type, result_args);
                        if fields.is_empty() {
                            let destructor =
                                enum_destructor_symbol(Some(variant), &fields, context);
                            let result = next_temp(out);
                            writeln!(out, "  {result} = call ptr @aura_llvm_enum_alloc(i64 0, ptr {destructor})").unwrap();
                            let tag = next_temp(out);
                            writeln!(
                                out,
                                "  {tag} = getelementptr %AuraLlvmEnum, ptr {result}, i32 0, i32 1"
                            )
                            .unwrap();
                            writeln!(out, "  store i64 {}, ptr {tag}", info.tag).unwrap();
                            return Ok(result);
                        }
                    }
                }
                let _ = status;
                Ok(String::new())
            } else if *payload_ty == Ty::Int {
                let slot = next_temp(out);
                writeln!(out, "  {slot} = alloca i64").unwrap();
                writeln!(
                    out,
                    "  call i32 @aura_llvm_task_join_i64(ptr {executor}, ptr {handle_value}, ptr {slot})"
                )
                .unwrap();
                let value = next_temp(out);
                writeln!(out, "  {value} = load i64, ptr {slot}").unwrap();
                if matches!(result_ty, Some(Ty::EnumApp { .. })) {
                    emit_result_variant(out, result_ty, &value, 0, false)
                } else {
                    Ok(value)
                }
            } else if is_pointer_value_type(payload_ty) {
                let slot = next_temp(out);
                writeln!(out, "  {slot} = alloca ptr").unwrap();
                writeln!(
                    out,
                    "  call i32 @aura_llvm_task_join_ptr(ptr {executor}, ptr {handle_value}, ptr {slot})"
                )
                .unwrap();
                let value = next_temp(out);
                writeln!(out, "  {value} = load ptr, ptr {slot}").unwrap();
                if matches!(result_ty, Some(Ty::EnumApp { .. })) {
                    emit_result_variant(out, result_ty, &value, 0, true)
                } else {
                    retain_pointer_value(out, &value, payload_ty)?;
                    Ok(value)
                }
            } else {
                let _ = result_ty;
                Err(unsupported(&format!(
                    "scheduler-backed join payload {}",
                    payload_ty.display()
                )))
            }
        }
        AsyncOp::Cancel(handle) => {
            let handle_value = load_place(out, *handle, body)?;
            let executor = next_temp(out);
            writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
            writeln!(
                out,
                "  call i32 @aura_llvm_task_cancel(ptr {executor}, ptr {handle_value})"
            )
            .unwrap();
            Ok(String::new())
        }
        AsyncOp::ChannelCreate { capacity, .. } => {
            let value = load_place(out, *capacity, body)?;
            let channel = next_temp(out);
            writeln!(
                out,
                "  {channel} = call ptr @aura_llvm_channel_new(i64 {value})"
            )
            .unwrap();
            Ok(channel)
        }
        AsyncOp::ChannelSend { channel, value } => {
            let channel_value = load_place(out, *channel, body)?;
            let value_ty = &body.locals[value.local].ty;
            let value = load_place(out, *value, body)?;
            if is_pointer_value_type(value_ty) {
                retain_pointer_value(out, &value, value_ty)?;
            }
            let raw = array_raw_value(out, &value, value_ty)?;
            let sent = next_temp(out);
            writeln!(
                out,
                "  {sent} = call i1 @aura_llvm_channel_send(ptr {channel_value}, i64 {raw})"
            )
            .unwrap();
            Ok(String::new())
        }
        AsyncOp::ChannelReceive(channel) => {
            let channel_ty = &body.locals[channel.local].ty;
            let Ty::Channel(element_ty) = channel_ty else {
                return Err(unsupported("receiving from a non-channel value"));
            };
            let result_ty = result_ty.ok_or_else(|| unsupported("channel receive result type"))?;
            let nullable =
                matches!(result_ty, Ty::Nullable(inner) if inner.as_ref() == element_ty.as_ref());
            if *result_ty != **element_ty && !nullable {
                return Err(unsupported(&format!(
                    "channel receive result type (channel={}, result={})",
                    channel_ty.display(),
                    result_ty.display()
                )));
            }
            if **element_ty == Ty::Unit {
                return Err(unsupported("Unit channel values"));
            }
            let channel_value = load_place(out, *channel, body)?;
            let raw_slot = next_temp(out);
            writeln!(out, "  {raw_slot} = alloca i64").unwrap();
            let received = next_temp(out);
            writeln!(
                out,
                "  {received} = call i1 @aura_llvm_channel_receive(ptr {channel_value}, ptr {raw_slot})"
            )
            .unwrap();
            let raw = next_temp(out);
            writeln!(out, "  {raw} = load i64, ptr {raw_slot}").unwrap();
            let value = array_value_from_raw(out, raw, element_ty)?;
            if !nullable {
                return Ok(value);
            }
            let value_ty = llvm_type(element_ty)?;
            let present = if matches!(element_ty.as_ref(), Ty::Int | Ty::Bool | Ty::Float) {
                build_optional_value(out, llvm_type(result_ty)?, value_ty, &value)
            } else {
                value
            };
            let zero = nullable_zero_value(Some(result_ty)).unwrap_or("null");
            let result = next_temp(out);
            writeln!(
                out,
                "  {result} = select i1 {received}, {} {present}, {} {zero}",
                llvm_type(result_ty)?,
                llvm_type(result_ty)?
            )
            .unwrap();
            Ok(result)
        }
        AsyncOp::ChannelClose(channel) => {
            let channel_value = load_place(out, *channel, body)?;
            writeln!(
                out,
                "  call void @aura_llvm_channel_close(ptr {channel_value})"
            )
            .unwrap();
            Ok(String::new())
        }
    }
}

pub(super) fn load_place(
    out: &mut String,
    place: Place,
    body: &MirBody,
) -> Result<String, CodegenError> {
    let ty = llvm_type(&body.locals[place.local].ty)?;
    if ty == "void" {
        return Err(unsupported("unit place"));
    }
    if super::is_boxed_local(&body.locals[place.local]) {
        let box_value = next_temp(out);
        writeln!(out, "  {box_value} = load ptr, ptr %slot{}", place.local).unwrap();
        if is_pointer_box_type(&body.locals[place.local].ty) {
            let payload = next_temp(out);
            writeln!(
                out,
                "  {payload} = call ptr @aura_box_ptr_get(ptr {box_value})"
            )
            .unwrap();
            if matches!(body.locals[place.local].ty, Ty::Fun { .. }) {
                let env_addr = next_temp(out);
                let fn_addr = next_temp(out);
                writeln!(
                    out,
                    "  {env_addr} = getelementptr %AuraLlvmFunPayload, ptr {payload}, i32 0, i32 0"
                )
                .unwrap();
                writeln!(
                    out,
                    "  {fn_addr} = getelementptr %AuraLlvmFunPayload, ptr {payload}, i32 0, i32 1"
                )
                .unwrap();
                let env = next_temp(out);
                let function = next_temp(out);
                writeln!(out, "  {env} = load ptr, ptr {env_addr}").unwrap();
                writeln!(out, "  {function} = load ptr, ptr {fn_addr}").unwrap();
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = insertvalue %AuraLlvmFun undef, ptr {env}, 0"
                )
                .unwrap();
                let result = next_temp(out);
                writeln!(
                    out,
                    "  {result} = insertvalue %AuraLlvmFun {value}, ptr {function}, 1"
                )
                .unwrap();
                return Ok(result);
            }
            return Ok(payload);
        }
        let (box_ty, value_ty) = match body.locals[place.local].ty {
            Ty::Int => ("%AuraLlvmBoxI64", "i64"),
            Ty::Bool => ("%AuraLlvmBoxBool", "i1"),
            Ty::Float => ("%AuraLlvmBoxF64", "double"),
            Ty::String => ("%AuraLlvmBoxStr", "ptr"),
            _ => return Err(unsupported("mutable capture type")),
        };
        let address = next_temp(out);
        writeln!(
            out,
            "  {address} = getelementptr {box_ty}, ptr {box_value}, i32 0, i32 0"
        )
        .unwrap();
        let temp = next_temp(out);
        writeln!(out, "  {temp} = load {value_ty}, ptr {address}").unwrap();
        if body.locals[place.local].ty == Ty::String {
            let snapshot = next_temp(out);
            writeln!(
                out,
                "  {snapshot} = call ptr @aura_box_str_get(ptr {box_value})"
            )
            .unwrap();
            let aura_value = next_temp(out);
            writeln!(
                out,
                "  {aura_value} = call ptr @aura_llvm_str_new(ptr {snapshot})"
            )
            .unwrap();
            writeln!(out, "  call void @free(ptr {snapshot})").unwrap();
            Ok(aura_value)
        } else {
            Ok(temp)
        }
    } else {
        let temp = next_temp(out);
        writeln!(out, "  {temp} = load {ty}, ptr %slot{}", place.local).unwrap();
        Ok(temp)
    }
}

pub(super) fn store_place(
    out: &mut String,
    place: Place,
    value: &str,
    body: &MirBody,
) -> Result<(), CodegenError> {
    if super::is_boxed_local(&body.locals[place.local]) {
        let box_value = next_temp(out);
        writeln!(out, "  {box_value} = load ptr, ptr %slot{}", place.local).unwrap();
        if is_pointer_box_type(&body.locals[place.local].ty) {
            let (stored, drop) = if matches!(body.locals[place.local].ty, Ty::Fun { .. }) {
                let payload = next_temp(out);
                writeln!(out, "  {payload} = call ptr @malloc(i64 16)").unwrap();
                let env = next_temp(out);
                let function = next_temp(out);
                writeln!(out, "  {env} = extractvalue %AuraLlvmFun {value}, 0").unwrap();
                writeln!(out, "  {function} = extractvalue %AuraLlvmFun {value}, 1").unwrap();
                writeln!(out, "  call void @aura_fun_env_retain(ptr {env})").unwrap();
                let env_addr = next_temp(out);
                let fn_addr = next_temp(out);
                writeln!(
                    out,
                    "  {env_addr} = getelementptr %AuraLlvmFunPayload, ptr {payload}, i32 0, i32 0"
                )
                .unwrap();
                writeln!(
                    out,
                    "  {fn_addr} = getelementptr %AuraLlvmFunPayload, ptr {payload}, i32 0, i32 1"
                )
                .unwrap();
                writeln!(out, "  store ptr {env}, ptr {env_addr}").unwrap();
                writeln!(out, "  store ptr {function}, ptr {fn_addr}").unwrap();
                (payload, "@aura_llvm_fun_box_drop")
            } else {
                let drop = if is_array_type(&body.locals[place.local].ty) {
                    "@aura_llvm_array_release"
                } else if matches!(body.locals[place.local].ty, Ty::Task(_) | Ty::TaskHandle(_)) {
                    "@aura_llvm_task_box_drop"
                } else if matches!(body.locals[place.local].ty, Ty::Channel(_)) {
                    "@aura_task_channel_destroy"
                } else {
                    "@aura_llvm_class_release"
                };
                (value.to_string(), drop)
            };
            if matches!(body.locals[place.local].ty, Ty::Task(_) | Ty::TaskHandle(_)) {
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                writeln!(
                    out,
                    "  call i32 @aura_task_executor_retain_payload(ptr {executor}, ptr {value})"
                )
                .unwrap();
            } else if matches!(body.locals[place.local].ty, Ty::Channel(_)) {
                writeln!(out, "  call i32 @aura_task_channel_retain(ptr {value})").unwrap();
            }
            writeln!(
                out,
                "  call ptr @aura_box_ptr_set(ptr {box_value}, ptr {stored}, ptr {drop})"
            )
            .unwrap();
            return Ok(());
        }
        let (box_ty, value_ty) = match body.locals[place.local].ty {
            Ty::Int => ("%AuraLlvmBoxI64", "i64"),
            Ty::Bool => ("%AuraLlvmBoxBool", "i1"),
            Ty::Float => ("%AuraLlvmBoxF64", "double"),
            Ty::String => ("%AuraLlvmBoxStr", "ptr"),
            _ => return Err(unsupported("mutable capture type")),
        };
        let address = next_temp(out);
        writeln!(
            out,
            "  {address} = getelementptr {box_ty}, ptr {box_value}, i32 0, i32 0"
        )
        .unwrap();
        if body.locals[place.local].ty == Ty::String {
            let data = next_temp(out);
            writeln!(out, "  {data} = call ptr @aura_llvm_str_data(ptr {value})").unwrap();
            writeln!(
                out,
                "  call ptr @aura_box_str_set(ptr {box_value}, ptr {data})"
            )
            .unwrap();
            writeln!(out, "  call void @aura_llvm_str_release(ptr {value})").unwrap();
        } else {
            writeln!(out, "  store {value_ty} {value}, ptr {address}").unwrap();
        }
    } else {
        let ty = llvm_type(&body.locals[place.local].ty)?;
        writeln!(out, "  store {ty} {value}, ptr %slot{}", place.local).unwrap();
    }
    Ok(())
}

fn is_pointer_box_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Fun { .. }
            | Ty::Class(_)
            | Ty::ClassApp { .. }
            | Ty::Interface(_)
            | Ty::InterfaceApp { .. }
            | Ty::Enum(_)
            | Ty::EnumApp { .. }
            | Ty::Task(_)
            | Ty::TaskHandle(_)
            | Ty::Channel(_)
    )
}

pub(super) fn load_string_byte(
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

pub(super) fn load_array_element(
    out: &mut String,
    collection: Place,
    index: Place,
    element_ty: &Ty,
    body: &MirBody,
) -> Result<String, CodegenError> {
    if body.locals[index.local].ty != Ty::Int {
        return Err(unsupported("Array index is not Int"));
    }
    let array = load_place(out, collection, body)?;
    let offset = load_place(out, index, body)?;
    let raw = next_temp(out);
    writeln!(
        out,
        "  {raw} = call i64 @aura_llvm_array_get(ptr {array}, i64 {offset})"
    )
    .unwrap();
    array_value_from_raw(out, raw, element_ty)
}

pub(super) fn array_raw_value(
    out: &mut String,
    value: &str,
    ty: &Ty,
) -> Result<String, CodegenError> {
    match ty {
        Ty::Int => Ok(value.into()),
        Ty::Bool => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = zext i1 {value} to i64").unwrap();
            Ok(raw)
        }
        Ty::Float => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = bitcast double {value} to i64").unwrap();
            Ok(raw)
        }
        Ty::String
        | Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::Interface(_)
        | Ty::InterfaceApp { .. }
        | Ty::Enum(_)
        | Ty::EnumApp { .. }
        | Ty::ForeignHandle(_) => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = ptrtoint ptr {value} to i64").unwrap();
            Ok(raw)
        }
        ty if is_pointer_value_type(ty) => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = ptrtoint ptr {value} to i64").unwrap();
            Ok(raw)
        }
        _ => Err(unsupported("Array element type")),
    }
}

fn consume_owned_call_args(
    out: &mut String,
    args: &[Place],
    parameter_tys: &[Ty],
    body: &MirBody,
) -> Result<(), CodegenError> {
    let mut consumed = HashSet::new();
    for (arg, expected_ty) in args.iter().zip(parameter_tys) {
        let source_ty = &body.locals[arg.local].ty;
        if !is_moving_call_arg(source_ty, expected_ty) || !consumed.insert(arg.local) {
            continue;
        }
        let zero = llvm_zero(source_ty)?;
        let ty = llvm_type(source_ty)?;
        writeln!(out, "  store {ty} {zero}, ptr %slot{}", arg.local).unwrap();
    }
    Ok(())
}

fn is_moving_call_arg(source_ty: &Ty, expected_ty: &Ty) -> bool {
    // The C ABI transfers only linear array/function owners at call
    // boundaries. Heap classes and strings remain borrowable receivers.
    if !is_array_type(source_ty) && !matches!(source_ty, Ty::Fun { .. }) {
        return false;
    }
    let source_storage = aura_ownership::plan_for_ty(source_ty).storage;
    let expected_storage = aura_ownership::plan_for_ty(expected_ty).storage;
    matches!(
        (source_storage, expected_storage),
        (
            aura_ownership::Storage::Unique | aura_ownership::Storage::FunctionEnvironment,
            aura_ownership::Storage::Unique | aura_ownership::Storage::FunctionEnvironment
        )
    ) && types_compatible(source_ty, expected_ty)
}

pub(super) fn is_pointer_value_type(ty: &Ty) -> bool {
    if let Ty::Nullable(inner) = ty {
        return is_pointer_value_type(inner);
    }
    // Generic arrays are heap objects too, even though they are represented as
    // ClassApp rather than a dedicated Ty variant.
    if is_array_type(ty) {
        return true;
    }
    matches!(
        ty,
        Ty::String
            | Ty::Class(_)
            | Ty::ClassApp { .. }
            | Ty::Interface(_)
            | Ty::InterfaceApp { .. }
            | Ty::Enum(_)
            | Ty::EnumApp { .. }
            | Ty::ForeignHandle(_)
            | Ty::Task(_)
            | Ty::TaskHandle(_)
            | Ty::Channel(_)
            | Ty::TypeParam(_)
    )
}

pub(super) fn is_pointer_abi_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::String
            | Ty::Class(_)
            | Ty::ClassApp { .. }
            | Ty::Enum(_)
            | Ty::EnumApp { .. }
            | Ty::Interface(_)
            | Ty::InterfaceApp { .. }
    )
}

pub(super) fn retain_pointer_value(
    out: &mut String,
    value: &str,
    ty: &Ty,
) -> Result<(), CodegenError> {
    if let Ty::Nullable(inner) = ty {
        return retain_pointer_value(out, value, inner);
    }
    if is_array_type(ty) {
        writeln!(out, "  call void @aura_llvm_array_retain(ptr {value})").unwrap();
        return Ok(());
    }
    let helper = match ty {
        Ty::String => "aura_llvm_str_retain",
        Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::Interface(_)
        | Ty::InterfaceApp { .. }
        | Ty::ForeignHandle(_) => "aura_llvm_class_retain",
        Ty::Enum(_) | Ty::EnumApp { .. } => "aura_llvm_enum_retain",
        Ty::Task(_) | Ty::TaskHandle(_) => {
            let executor = next_temp(out);
            writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
            writeln!(
                out,
                "  call i32 @aura_task_executor_retain_payload(ptr {executor}, ptr {value})"
            )
            .unwrap();
            return Ok(());
        }
        Ty::Channel(_) => {
            writeln!(out, "  call i32 @aura_task_channel_retain(ptr {value})").unwrap();
            return Ok(());
        }
        _ => return Err(unsupported("non-pointer value")),
    };
    writeln!(out, "  call void @{helper}(ptr {value})").unwrap();
    Ok(())
}

pub(super) fn release_raw_value(out: &mut String, raw: &str, ty: &Ty) -> Result<(), CodegenError> {
    if let Ty::Nullable(inner) = ty {
        return release_raw_value(out, raw, inner);
    }
    if is_array_type(ty) {
        let pointer = next_temp(out);
        writeln!(out, "  {pointer} = inttoptr i64 {raw} to ptr").unwrap();
        writeln!(out, "  call void @aura_llvm_array_release(ptr {pointer})").unwrap();
        return Ok(());
    }
    let helper = match ty {
        Ty::String => "aura_llvm_str_release",
        Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::Interface(_)
        | Ty::InterfaceApp { .. }
        | Ty::ForeignHandle(_) => "aura_llvm_class_release",
        Ty::Enum(_) | Ty::EnumApp { .. } => "aura_llvm_enum_release",
        _ => {
            if matches!(ty, Ty::Task(_) | Ty::TaskHandle(_)) {
                let pointer = next_temp(out);
                writeln!(out, "  {pointer} = inttoptr i64 {raw} to ptr").unwrap();
                let holder = next_temp(out);
                writeln!(out, "  {holder} = alloca ptr").unwrap();
                writeln!(out, "  store ptr {pointer}, ptr {holder}").unwrap();
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                writeln!(
                    out,
                    "  call i32 @aura_task_executor_release_payload(ptr {executor}, ptr {holder})"
                )
                .unwrap();
                return Ok(());
            }
            if matches!(ty, Ty::Channel(_)) {
                let pointer = next_temp(out);
                writeln!(out, "  {pointer} = inttoptr i64 {raw} to ptr").unwrap();
                writeln!(out, "  call void @aura_task_channel_destroy(ptr {pointer})").unwrap();
                return Ok(());
            }
            return Err(unsupported("non-pointer value"));
        }
    };
    let pointer = next_temp(out);
    writeln!(out, "  {pointer} = inttoptr i64 {raw} to ptr").unwrap();
    writeln!(out, "  call void @{helper}(ptr {pointer})").unwrap();
    Ok(())
}

pub(super) fn array_value_from_raw(
    out: &mut String,
    raw: String,
    ty: &Ty,
) -> Result<String, CodegenError> {
    match ty {
        Ty::Int => Ok(raw),
        Ty::Bool => {
            let value = next_temp(out);
            writeln!(out, "  {value} = trunc i64 {raw} to i1").unwrap();
            Ok(value)
        }
        Ty::Float => {
            let value = next_temp(out);
            writeln!(out, "  {value} = bitcast i64 {raw} to double").unwrap();
            Ok(value)
        }
        Ty::String
        | Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::Interface(_)
        | Ty::InterfaceApp { .. }
        | Ty::Enum(_)
        | Ty::EnumApp { .. }
        | Ty::ForeignHandle(_) => {
            let value = next_temp(out);
            writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
            retain_pointer_value(out, &value, ty)?;
            Ok(value)
        }
        ty if is_pointer_value_type(ty) => {
            let value = next_temp(out);
            writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
            retain_pointer_value(out, &value, ty)?;
            Ok(value)
        }
        _ => Err(unsupported("Array element type")),
    }
}

pub(super) fn emit_equality(
    out: &mut String,
    left: &str,
    right: &str,
    ty: &Ty,
) -> Result<String, CodegenError> {
    if is_string_type(ty) {
        let value = next_temp(out);
        writeln!(
            out,
            "  {value} = call i1 @aura_llvm_str_eq(ptr {left}, ptr {right})"
        )
        .unwrap();
        return Ok(value);
    }
    if is_class_type(ty) || is_enum_type(ty) || matches!(ty, Ty::Nullable(_)) {
        let value = next_temp(out);
        writeln!(out, "  {value} = icmp eq ptr {left}, {right}").unwrap();
        return Ok(value);
    }
    let llvm = llvm_type(ty)?;
    let value = next_temp(out);
    let operation = if matches!(ty, Ty::Float) {
        "fcmp oeq"
    } else {
        "icmp eq"
    };
    writeln!(out, "  {value} = {operation} {llvm} {left}, {right}").unwrap();
    Ok(value)
}

pub(super) fn next_temp(out: &mut String) -> String {
    let mut next = 0;
    let bytes = out.as_bytes();
    let mut index = 0;
    while index + 2 < bytes.len() {
        if bytes[index] == b'%' && bytes[index + 1] == b't' && bytes[index + 2].is_ascii_digit() {
            let start = index + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if let Ok(value) = out[start..end].parse::<usize>() {
                next = next.max(value + 1);
            }
            index = end;
        } else {
            index += 1;
        }
    }
    let temp = format!("%t{next}");
    writeln!(out, "; reserve {temp}").unwrap();
    temp
}
