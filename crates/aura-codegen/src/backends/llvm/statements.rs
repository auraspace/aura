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
            } else if is_array_type(&body.locals[place.local].ty) {
                let value = load_place(out, *place, body)?;
                writeln!(out, "  call void @aura_llvm_array_release(ptr {value})").unwrap();
                writeln!(out, "  store ptr null, ptr %slot{}", place.local).unwrap();
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
            if !matches!(
                field_ty,
                Ty::Int
                    | Ty::Bool
                    | Ty::Float
                    | Ty::String
                    | Ty::Class(_)
                    | Ty::ClassApp { .. }
                    | Ty::Enum(_)
                    | Ty::EnumApp { .. }
            ) || !types_compatible(&body.locals[to.local].ty, field_ty)
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
                Ty::String
                | Ty::Class(_)
                | Ty::ClassApp { .. }
                | Ty::Enum(_)
                | Ty::EnumApp { .. } => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                    retain_pointer_value(out, &value, field_ty)?;
                    value
                }
                _ => unreachable!("validated enum payload"),
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
            if is_string_type(collection_ty) && body.locals[to.local].ty == Ty::Int {
                let value = load_string_byte(out, *collection, *index, body)?;
                writeln!(out, "  store i64 {value}, ptr %slot{}", to.local).unwrap();
            } else if let Some(element_ty) = array_element_type(collection_ty) {
                if body.locals[to.local].ty != *element_ty {
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
            let type_args = match object_ty {
                Ty::ClassApp { args, .. } => args.as_slice(),
                _ => &[],
            };
            let fields = class_fields(context, class_name, type_args)
                .ok_or_else(|| unsupported(&format!("class {class_name}")))?;
            let (index, field_ty) = fields
                .iter()
                .enumerate()
                .find(|(_, (name, _))| name == field)
                .map(|(index, (_, ty))| (index, ty))
                .ok_or_else(|| unsupported(&format!("class field {class_name}.{field}")))?;
            if !matches!(
                field_ty,
                Ty::Int
                    | Ty::Bool
                    | Ty::Float
                    | Ty::String
                    | Ty::Class(_)
                    | Ty::ClassApp { .. }
                    | Ty::Enum(_)
                    | Ty::EnumApp { .. }
            ) || body.locals[value.local].ty != *field_ty
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
    let ty = llvm_type(&body.locals[from.local].ty)?;
    let value = load_place(out, from, body)?;
    if retain && is_string_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_str_retain(ptr {value})").unwrap();
    } else if retain && is_enum_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_enum_retain(ptr {value})").unwrap();
    } else if retain && is_class_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_class_retain(ptr {value})").unwrap();
    } else if retain && is_array_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_array_retain(ptr {value})").unwrap();
    }
    writeln!(out, "  store {ty} {value}, ptr %slot{}", to.local).unwrap();
    Ok(())
}
