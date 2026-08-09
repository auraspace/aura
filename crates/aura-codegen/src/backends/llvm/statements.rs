use super::*;

pub(super) fn emit_statement(
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
            store_place(out, *place, &value, body)?;
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
            if super::is_borrowed_box_local(&body.locals[place.local]) {
                // The closure environment owns this shared box; the lambda
                // local only borrows it for the duration of the invocation.
            } else if super::is_boxed_local(&body.locals[place.local]) {
                let helper = match body.locals[place.local].ty {
                    Ty::Int => "aura_box_i64_release",
                    Ty::Bool => "aura_box_bool_release",
                    Ty::Float => "aura_box_f64_release",
                    Ty::String => "aura_box_str_release",
                    Ty::Fun { .. }
                    | Ty::Class(_)
                    | Ty::ClassApp { .. }
                    | Ty::Interface(_)
                    | Ty::InterfaceApp { .. } => "aura_box_ptr_release",
                    _ => return Err(unsupported("mutable capture type")),
                };
                let box_value = next_temp(out);
                writeln!(out, "  {box_value} = load ptr, ptr %slot{}", place.local).unwrap();
                writeln!(out, "  call void @{helper}(ptr {box_value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            } else if is_string_type(&body.locals[place.local].ty) {
                let value = load_place(out, *place, body)?;
                writeln!(out, "  call void @aura_llvm_str_release(ptr {value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            } else if is_enum_type(&body.locals[place.local].ty) {
                let value = load_place(out, *place, body)?;
                writeln!(out, "  call void @aura_llvm_enum_release(ptr {value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            } else if is_class_type(&body.locals[place.local].ty) {
                let value = load_place(out, *place, body)?;
                let helper = class_type_name(&body.locals[place.local].ty)
                    .filter(|name| {
                        context
                            .classes
                            .get(*name)
                            .is_some_and(class_has_pointer_fields)
                    })
                    .map(class_release_symbol)
                    .unwrap_or_else(|| "aura_llvm_class_release".into());
                writeln!(out, "  call void @{helper}(ptr {value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            } else if matches!(
                &body.locals[place.local].ty,
                Ty::Interface(_) | Ty::InterfaceApp { .. }
            ) {
                let value = load_place(out, *place, body)?;
                writeln!(out, "  call void @aura_llvm_class_release(ptr {value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            } else if is_array_type(&body.locals[place.local].ty) {
                let value = load_place(out, *place, body)?;
                writeln!(out, "  call void @aura_llvm_array_release(ptr {value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            } else if matches!(
                &body.locals[place.local].ty,
                Ty::Task(_) | Ty::TaskHandle(_)
            ) {
                let value = load_place(out, *place, body)?;
                let executor = next_temp(out);
                writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
                writeln!(
                    out,
                    "  call i32 @aura_llvm_task_release(ptr {executor}, ptr {value})"
                )
                .unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
            } else if matches!(&body.locals[place.local].ty, Ty::Fun { .. }) {
                let value = load_place(out, *place, body)?;
                writeln!(
                    out,
                    "  call void @aura_llvm_fun_release({} {value})",
                    llvm_type(&body.locals[place.local].ty)?
                )
                .unwrap();
                writeln!(
                    out,
                    "  store {} {}, ptr %slot{}",
                    llvm_type(&body.locals[place.local].ty)?,
                    llvm_zero(&body.locals[place.local].ty)?,
                    place.local
                )
                .unwrap();
            }
        }
        Statement::EnterTry {
            handler, catch_ty, ..
        } => {
            let id = out.lines().count();
            writeln!(out, "  %ex_buf{id} = alloca [256 x i8], align 16").unwrap();
            writeln!(out, "  call void @aura_try_enter(ptr %ex_buf{id})").unwrap();
            writeln!(out, "  %ex_jump{id} = call i32 @_setjmp(ptr %ex_buf{id})").unwrap();
            writeln!(out, "  %ex_thrown{id} = icmp ne i32 %ex_jump{id}, 0").unwrap();
            writeln!(
                out,
                "  br i1 %ex_thrown{id}, label %ex_dispatch{id}, label %try_body{id}"
            )
            .unwrap();
            writeln!(out, "ex_dispatch{id}:").unwrap();
            if let Some(catch_ty) = catch_ty {
                let Some(type_name) = exception_type_name(catch_ty) else {
                    return Err(unsupported("catch type dispatch"));
                };
                let global = exception_type_global(type_name);
                let length = type_name.as_bytes().len() + 1;
                let name = next_temp(out);
                writeln!(
                    out,
                    "  {name} = getelementptr [{length} x i8], ptr {global}, i64 0, i64 0"
                )
                .unwrap();
                let matched = next_temp(out);
                writeln!(out, "  {matched} = call i32 @aura_ex_matches(ptr {name})").unwrap();
                let is_match = next_temp(out);
                writeln!(out, "  {is_match} = icmp ne i32 {matched}, 0").unwrap();
                writeln!(
                    out,
                    "  br i1 {is_match}, label %bb{handler}, label %ex_rethrow{id}"
                )
                .unwrap();
                writeln!(out, "ex_rethrow{id}:").unwrap();
                let cause = next_temp(out);
                writeln!(
                    out,
                    "  {cause} = call i32 @aura_ex_add_cause(ptr {name}, i32 0, i32 0)"
                )
                .unwrap();
                out.push_str("  call void @aura_ex_rethrow()\n");
                out.push_str("  unreachable\n");
            } else {
                writeln!(out, "  br label %bb{handler}").unwrap();
            }
            writeln!(out, "try_body{id}:").unwrap();
        }
        Statement::LeaveTry => {
            out.push_str("  call void @aura_ex_clear()\n");
            out.push_str("  call void @aura_try_leave()\n");
        }
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
            let fields = resolved_variant_fields(info, &body.locals[operand.local].ty, &[]);
            let (field_index, (_, field_ty)) = fields
                .iter()
                .enumerate()
                .find(|(_, (name, _))| name == field)
                .ok_or_else(|| unsupported(&format!("enum field {variant}.{field}")))?;
            if !(matches!(field_ty, Ty::Unit | Ty::Int | Ty::Bool | Ty::Float)
                || is_pointer_value_type(field_ty))
                || (field_ty != &Ty::Unit
                    && !types_compatible(&body.locals[to.local].ty, field_ty)
                    && !(is_pointer_value_type(&body.locals[to.local].ty)
                        && is_pointer_value_type(field_ty)))
            {
                return Err(unsupported("non-primitive enum payload"));
            }
            let address = next_temp(out);
            writeln!(
                out,
                "  {address} = getelementptr %AuraLlvmEnum, ptr {object}, i32 0, i32 3, i64 {field_index}"
            )
            .unwrap();
            let raw = next_temp(out);
            writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
            let value = match field_ty {
                Ty::Unit => "0".to_owned(),
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
                Ty::String
                | Ty::Class(_)
                | Ty::ClassApp { .. }
                | Ty::Enum(_)
                | Ty::EnumApp { .. }
                | Ty::ForeignHandle(_) => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                    retain_pointer_value(out, &value, field_ty)?;
                    value
                }
                Ty::Nullable(inner) if is_pointer_value_type(inner) => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                    retain_pointer_value(out, &value, field_ty)?;
                    value
                }
                _ => {
                    return Err(unsupported(&format!(
                        "enum payload type {}",
                        field_ty.display()
                    )))
                }
            };
            let ty = llvm_type(&body.locals[to.local].ty)?;
            writeln!(out, "  store {ty} {value}, ptr %slot{}", to.local).unwrap();
        }
        Statement::LoadIndex {
            collection,
            index,
            to,
            ..
        } => {
            let collection_ty = &body.locals[collection.local].ty;
            if is_string_type(collection_ty) && body.locals[to.local].ty == Ty::Int {
                let value = load_string_byte(out, *collection, *index, body)?;
                writeln!(out, "  store i64 {value}, ptr %slot{}", to.local).unwrap();
            } else if let Some(element_ty) = array_element_type(collection_ty) {
                if !types_compatible(&body.locals[to.local].ty, element_ty) {
                    return Err(unsupported("Array index result type"));
                }
                let value = load_array_element(out, *collection, *index, element_ty, body)?;
                let ty = llvm_type(element_ty)?;
                writeln!(out, "  store {ty} {value}, ptr %slot{}", to.local).unwrap();
            } else {
                return Err(unsupported("indexing non-String/Array values"));
            }
        }
        Statement::StoreField {
            object,
            field,
            value,
        } => {
            let object_ty = &body.locals[object.local].ty;
            let class_name = class_type_name(object_ty)
                .ok_or_else(|| unsupported("field stores outside classes"))?;
            let type_args = class_type_args(object_ty);
            let fields = class_fields(context, class_name, type_args)
                .ok_or_else(|| unsupported(&format!("class {class_name}")))?;
            let (index, field_ty) = fields
                .iter()
                .enumerate()
                .find(|(_, (name, _))| name == field)
                .map(|(index, (_, ty))| (index, ty))
                .ok_or_else(|| unsupported(&format!("class field {class_name}.{field}")))?;
            if !(matches!(field_ty, Ty::Int | Ty::Bool | Ty::Float)
                || is_pointer_value_type(field_ty))
                || body.locals[value.local].ty != *field_ty
            {
                return Err(unsupported("non-primitive class field store"));
            }
            let object = load_place(out, *object, body)?;
            let value = load_place(out, *value, body)?;
            let raw = array_raw_value(out, &value, field_ty)?;
            let address = next_temp(out);
            writeln!(
                out,
                "  {address} = getelementptr %AuraLlvmClass, ptr {object}, i32 0, i32 1, i64 {index}"
            )
            .unwrap();
            if is_pointer_value_type(field_ty) {
                let old = next_temp(out);
                writeln!(out, "  {old} = load i64, ptr {address}").unwrap();
                release_raw_value(out, &old, field_ty)?;
                retain_pointer_value(out, &value, field_ty)?;
            }
            writeln!(out, "  store i64 {raw}, ptr {address}").unwrap();
        }
    }
    Ok(())
}

pub(super) fn copy_place(
    out: &mut String,
    from: Place,
    to: Place,
    body: &MirBody,
    retain: bool,
) -> Result<(), CodegenError> {
    let source_ty = &body.locals[from.local].ty;
    let destination_ty = &body.locals[to.local].ty;
    let ty = llvm_type(source_ty)?;
    let loaded = load_place(out, from, body)?;
    let value = match (source_ty, destination_ty) {
        (Ty::Null, destination @ Ty::Nullable(_)) => nullable_zero_value(Some(destination))
            .unwrap_or("null")
            .to_owned(),
        (Ty::Int, Ty::Nullable(inner)) if **inner == Ty::Int => {
            build_optional_value(out, "%AuraLlvmOptInt", "i64", &loaded)
        }
        (Ty::Bool, Ty::Nullable(inner)) if **inner == Ty::Bool => {
            build_optional_value(out, "%AuraLlvmOptBool", "i1", &loaded)
        }
        (Ty::Float, Ty::Nullable(inner)) if **inner == Ty::Float => {
            build_optional_value(out, "%AuraLlvmOptFloat", "double", &loaded)
        }
        (Ty::Nullable(inner), destination) if types_compatible(inner, destination) => {
            extract_optional_payload(out, &loaded, source_ty)?
        }
        _ => loaded,
    };
    if retain && is_string_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_str_retain(ptr {value})").unwrap();
    } else if retain && is_enum_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_enum_retain(ptr {value})").unwrap();
    } else if retain
        && (is_class_type(&body.locals[from.local].ty)
            || matches!(
                &body.locals[from.local].ty,
                Ty::Interface(_) | Ty::InterfaceApp { .. }
            ))
    {
        writeln!(out, "  call void @aura_llvm_class_retain(ptr {value})").unwrap();
    } else if retain && is_array_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_array_retain(ptr {value})").unwrap();
    } else if retain && matches!(&body.locals[from.local].ty, Ty::Fun { .. }) {
        writeln!(
            out,
            "  call void @aura_llvm_fun_retain({} {value})",
            llvm_type(&body.locals[from.local].ty)?
        )
        .unwrap();
    }
    store_place(out, to, &value, body)?;
    if !retain && from.local != to.local {
        if !super::is_boxed_local(&body.locals[from.local]) {
            writeln!(
                out,
                "  store {ty} {}, ptr %slot{}",
                llvm_zero(&body.locals[from.local].ty)?,
                from.local
            )
            .unwrap();
        }
    }
    Ok(())
}
