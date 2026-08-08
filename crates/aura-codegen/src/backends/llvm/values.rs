use super::*;

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
        Rvalue::Function { name } => Ok(format!("@{}", symbol_name(package, name))),
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
            let values = args
                .iter()
                .map(|place| load_place(out, *place, body))
                .collect::<Result<Vec<_>, _>>()?;
            let arguments = values
                .iter()
                .zip(params)
                .map(|(value, ty)| Ok(format!("{} {value}", llvm_type(ty)?)))
                .collect::<Result<Vec<_>, CodegenError>>()?
                .join(", ");
            let return_ty = result_ty.unwrap_or(ret.as_ref());
            let llvm_return_ty = llvm_type(return_ty)?;
            if *return_ty == Ty::Unit {
                writeln!(out, "  call void {callee}({arguments})").unwrap();
                Ok(String::new())
            } else {
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call {llvm_return_ty} {callee}({arguments})"
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
            if let Some(value) = emit_builtin_method(out, target, args, &values, body)? {
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
                    .ok_or_else(|| unsupported(&format!("enum variant {variant}")))?;
                let fields = resolved_variant_fields(
                    info,
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
                    !matches!(
                        ty,
                        Ty::Int
                            | Ty::Bool
                            | Ty::Float
                            | Ty::String
                            | Ty::Class(_)
                            | Ty::ClassApp { .. }
                            | Ty::Enum(_)
                            | Ty::EnumApp { .. }
                    ) && !matches!(ty, Ty::Nullable(inner) if is_pointer_value_type(inner))
                }) {
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
                for (index, ((_, ty), argument)) in fields.iter().zip(values.iter()).enumerate() {
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
                        Ty::Nullable(inner) if is_pointer_value_type(inner) => {
                            retain_pointer_value(out, argument, ty)?;
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
                            raw
                        }
                        _ => unreachable!("validated enum payload"),
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
                        !matches!(
                            ty,
                            Ty::Int
                                | Ty::Bool
                                | Ty::Float
                                | Ty::String
                                | Ty::Class(_)
                                | Ty::ClassApp { .. }
                                | Ty::Enum(_)
                                | Ty::EnumApp { .. }
                        )
                    })
                {
                    return Err(unsupported("class field type"));
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
                        return Err(unsupported("superclass constructor"));
                    };
                    let parent_fields = class_fields(context, parent, &target.type_args)
                        .ok_or_else(|| unsupported("superclass constructor fields"))?;
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
                        let raw = raw_class_field_value(out, &argument, field_ty)?;
                        writeln!(out, "  store i64 {raw}, ptr {address}").unwrap();
                    }
                }
                for (index, ((_, ty), argument)) in own_fields.iter().zip(values.iter()).enumerate()
                {
                    let index = inherited_count + index;
                    let address = next_temp(out);
                    writeln!(
                        out,
                        "  {address} = getelementptr %AuraLlvmClass, ptr {value}, i32 0, i32 1, i64 {index}"
                    )
                    .unwrap();
                    let raw = raw_class_field_value(out, argument, ty)?;
                    writeln!(out, "  store i64 {raw}, ptr {address}").unwrap();
                }
                return Ok(value);
            }
            if target.name == "send" && args.len() == 2 {
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
                if *result_ty != **element_ty {
                    return Err(unsupported("channel receive result type"));
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
                return array_value_from_raw(out, raw, element_ty);
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
            let method_name = method_symbol_for(&context.signatures, target, args, body, package);
            let generic_name = monomorphized_symbol_for(&context.signatures, target, package);
            let name = if context.foreign_names.contains(&target.name) {
                target.name.clone()
            } else {
                generic_name
                    .clone()
                    .or(method_name.clone())
                    .clone()
                    .unwrap_or_else(|| symbol_name(&target.package, &target.name))
            };
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
            let (return_ty, parameter_tys) = generic_name
                .as_deref()
                .and_then(|symbol| signature_for_symbol(&context.signatures, symbol))
                .or_else(|| {
                    method_name
                        .as_deref()
                        .and_then(|symbol| signature_for_symbol(&context.signatures, symbol))
                })
                .or_else(|| signature_for(&context.signatures, package, target))
                .ok_or_else(|| unsupported(&format!("call target {}", target.name)))?;
            if parameter_tys.len() != values.len() {
                return Err(unsupported(&format!("call arity for {}", target.name)));
            }
            if !context.foreign_names.contains(&target.name) {
                for (index, value) in values.iter().enumerate() {
                    let source_ty = &body.locals[args[index].local].ty;
                    if is_pointer_value_type(source_ty) {
                        retain_pointer_value(out, value, source_ty)?;
                    }
                }
            }
            let arguments = values
                .iter()
                .zip(parameter_tys)
                .enumerate()
                .map(|(index, (value, ty))| {
                    let source_ty = &body.locals[args[index].local].ty;
                    let value = coerce_llvm_argument(out, value, source_ty, ty)?;
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
                let fallback = next_temp(out);
                writeln!(
                    out,
                    "  {fallback} = call {} @{name}({arguments})",
                    llvm_type(return_ty)?
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
                    llvm_type(return_ty)?,
                    incoming.join(", ")
                )
                .unwrap();
                return Ok(result);
            }
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
            let type_args = match object_ty {
                Ty::ClassApp { args, .. } => args.as_slice(),
                _ => &[],
            };
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
                Ty::String
                | Ty::Class(_)
                | Ty::ClassApp { .. }
                | Ty::Enum(_)
                | Ty::EnumApp { .. } => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                    retain_pointer_value(out, &value, field_ty)?;
                    Ok(value)
                }
                _ => Err(unsupported("class field type")),
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
    target: &aura_ir::mir::CallTarget,
    args: &[Place],
    values: &[String],
    body: &MirBody,
) -> Result<Option<String>, CodegenError> {
    let Some(receiver) = args.first() else {
        return Ok(None);
    };
    let receiver_ty = &body.locals[receiver.local].ty;
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
    target: &aura_ir::mir::CallTarget,
    values: &[String],
    body: &MirBody,
    result_ty: Option<&Ty>,
) -> Result<Option<String>, CodegenError> {
    if !target.package.starts_with("std.io") {
        return Ok(None);
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
    let Ty::EnumApp { args, .. } = result_ty.ok_or_else(|| unsupported("Result intrinsic type"))?
    else {
        return Err(unsupported("Result intrinsic type"));
    };
    let payload_ty = args
        .first()
        .ok_or_else(|| unsupported("Result intrinsic payload type"))?;
    let result = next_temp(out);
    writeln!(out, "  {result} = call ptr @aura_llvm_enum_alloc(i64 1)").unwrap();
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
        "  {field_address} = getelementptr %AuraLlvmEnum, ptr {result}, i32 0, i32 2, i64 0"
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

fn emit_superclass_arg(
    out: &mut String,
    expr: &aura_ast::Expr,
    own_fields: &[(String, Ty)],
    values: &[String],
    body: &MirBody,
    package: &str,
    context: &mut EmitContext,
) -> Result<(String, Ty), CodegenError> {
    match expr {
        aura_ast::Expr::Group(inner, _) => {
            emit_superclass_arg(out, inner, own_fields, values, body, package, context)
        }
        aura_ast::Expr::Int(value) => Ok((value.value.to_string(), Ty::Int)),
        aura_ast::Expr::Bool(value) => {
            Ok((if value.value { "true" } else { "false" }.into(), Ty::Bool))
        }
        aura_ast::Expr::Float(value) => Ok((format_float_constant(value.value), Ty::Float)),
        aura_ast::Expr::String(value) => {
            let rendered = emit_rvalue(
                out,
                &Rvalue::ConstString(value.value.clone()),
                body,
                Some(&Ty::String),
                package,
                context,
            )?;
            Ok((rendered, Ty::String))
        }
        aura_ast::Expr::Ident(identifier) => {
            let Some(index) = own_fields
                .iter()
                .position(|(name, _)| name == &identifier.name)
            else {
                return Err(unsupported("superclass constructor field reference"));
            };
            Ok((values[index].clone(), own_fields[index].1.clone()))
        }
        _ => Err(unsupported("superclass constructor argument")),
    }
}

fn raw_class_field_value(
    out: &mut String,
    argument: &str,
    ty: &Ty,
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
            retain_pointer_value(out, argument, ty)?;
            let raw = next_temp(out);
            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
            Ok(raw)
        }
        _ => Err(unsupported("class field type")),
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
    operation: &aura_ir::mir::AsyncOp,
    body: &MirBody,
    result_ty: Option<&Ty>,
    package: &str,
    context: &mut EmitContext,
) -> Result<String, CodegenError> {
    use aura_ir::mir::AsyncOp;

    match operation {
        AsyncOp::Spawn {
            body: task_body,
            captures,
        } => {
            if captures.len() > task_body.locals.len() {
                return Err(unsupported("spawn capture arity"));
            }
            let values = captures
                .iter()
                .map(|capture| load_place(out, capture.source, body))
                .collect::<Result<Vec<_>, _>>()?;
            let (_, parameter_tys) = context
                .signatures
                .get(&(package.to_owned(), task_body.name.clone()))
                .ok_or_else(|| unsupported(&format!("spawn body {}", task_body.name)))?;
            if parameter_tys.len() != values.len() {
                return Err(unsupported("spawn body parameter arity"));
            }
            let arguments = values
                .iter()
                .zip(parameter_tys)
                .map(|(value, ty)| Ok(format!("{} {value}", llvm_type(ty)?)))
                .collect::<Result<Vec<_>, CodegenError>>()?
                .join(", ");
            let payload_ty = &task_body.return_ty;
            if *payload_ty == Ty::Unit {
                writeln!(
                    out,
                    "  call void @{}({arguments})",
                    symbol_name(package, &task_body.name)
                )
                .unwrap();
                Ok("null".into())
            } else {
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call {} @{}({arguments})",
                    llvm_type(payload_ty)?,
                    symbol_name(package, &task_body.name)
                )
                .unwrap();
                Ok(value)
            }
        }
        AsyncOp::Join(handle) => {
            let handle_ty = &body.locals[handle.local].ty;
            if task_payload_type(handle_ty).is_none() {
                return Err(unsupported("joining a non-task handle"));
            }
            if result_ty.is_some_and(|ty| *ty == Ty::Unit) {
                Ok(String::new())
            } else {
                load_place(out, *handle, body)
            }
        }
        AsyncOp::Cancel(_) => Ok(String::new()),
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
            if *result_ty != **element_ty {
                return Err(unsupported("channel receive result type"));
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
            array_value_from_raw(out, raw, element_ty)
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
    let temp = next_temp(out);
    writeln!(out, "  {temp} = load {ty}, ptr %slot{}", place.local).unwrap();
    Ok(temp)
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
        _ => Err(unsupported("Array element type")),
    }
}

pub(super) fn is_pointer_value_type(ty: &Ty) -> bool {
    if let Ty::Nullable(inner) = ty {
        return is_pointer_value_type(inner);
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
        _ => return Err(unsupported("non-pointer value")),
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

pub(super) fn llvm_zero(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Unit => Err(unsupported("unit local")),
        Ty::Bool => Ok("false"),
        Ty::Float => Ok("0.0"),
        Ty::Int => Ok("0"),
        Ty::String | Ty::Null => Ok("null"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::String) => Ok("null"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Int) => Ok("{ i1 false, i64 0 }"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Bool) => Ok("{ i1 false, i1 false }"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Float) => {
            Ok("{ i1 false, double 0.0 }")
        }
        Ty::Nullable(inner)
            if matches!(
                inner.as_ref(),
                Ty::Class(_)
                    | Ty::ClassApp { .. }
                    | Ty::Enum(_)
                    | Ty::EnumApp { .. }
                    | Ty::Interface(_)
                    | Ty::InterfaceApp { .. }
                    | Ty::Fun { .. }
                    | Ty::Channel(_)
                    | Ty::ForeignHandle(_)
            ) =>
        {
            Ok("null")
        }
        Ty::Interface(_) | Ty::InterfaceApp { .. } | Ty::Fun { .. } => Ok("null"),
        Ty::Enum(_)
        | Ty::EnumApp { .. }
        | Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::ForeignHandle(_) => Ok("null"),
        Ty::Task(inner) | Ty::TaskHandle(inner) => {
            if matches!(inner.as_ref(), Ty::Unit) {
                Ok("null")
            } else {
                llvm_zero(inner)
            }
        }
        Ty::Channel(_) => Ok("null"),
        _ => Err(unsupported(&format!("type {}", ty.display()))),
    }
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

pub(crate) fn llvm_type(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Unit => Ok("void"),
        Ty::Int => Ok("i64"),
        Ty::Bool => Ok("i1"),
        Ty::Float => Ok("double"),
        Ty::String | Ty::Null => Ok("ptr"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::String) => Ok("ptr"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Int) => Ok("%AuraLlvmOptInt"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Bool) => Ok("%AuraLlvmOptBool"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Float) => Ok("%AuraLlvmOptFloat"),
        Ty::Nullable(inner)
            if matches!(
                inner.as_ref(),
                Ty::Class(_)
                    | Ty::ClassApp { .. }
                    | Ty::Enum(_)
                    | Ty::EnumApp { .. }
                    | Ty::Interface(_)
                    | Ty::InterfaceApp { .. }
                    | Ty::Fun { .. }
                    | Ty::Channel(_)
                    | Ty::ForeignHandle(_)
            ) =>
        {
            Ok("ptr")
        }
        Ty::Interface(_) | Ty::InterfaceApp { .. } | Ty::Fun { .. } => Ok("ptr"),
        Ty::Enum(_)
        | Ty::EnumApp { .. }
        | Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::ForeignHandle(_) => Ok("ptr"),
        Ty::Task(inner) | Ty::TaskHandle(inner) => {
            if matches!(inner.as_ref(), Ty::Unit) {
                Ok("ptr")
            } else {
                llvm_type(inner)
            }
        }
        Ty::Channel(_) => Ok("ptr"),
        _ => Err(unsupported(&format!("type {}", ty.display()))),
    }
}
