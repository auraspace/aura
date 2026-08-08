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

const CLASS_RUNTIME: &str = r#"
%AuraLlvmClass = type { i64, [0 x i64] }

define ptr @aura_llvm_class_alloc(i64 %fields) {
entry:
  %field_bytes = mul i64 %fields, 8
  %size = add i64 %field_bytes, 8
  %value = call ptr @malloc(i64 %size)
  %refs = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 0
  store i64 1, ptr %refs
  ret ptr %value
}

define void @aura_llvm_class_retain(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %retain
retain:
  %refs = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 0
  %current = load i64, ptr %refs
  %next = add i64 %current, 1
  store i64 %next, ptr %refs
  br label %done
done:
  ret void
}

define void @aura_llvm_class_release(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %release
release:
  %refs = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 0
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

const ARRAY_RUNTIME: &str = r#"
%AuraLlvmArray = type { i64, i64, i64, [0 x i64] }

define ptr @aura_llvm_array_alloc(i64 %len, i64 %kind) {
entry:
  %data_bytes = mul i64 %len, 8
  %size = add i64 %data_bytes, 24
  %value = call ptr @malloc(i64 %size)
  %refs = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 0
  store i64 1, ptr %refs
  %length = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 1
  store i64 %len, ptr %length
  %element_kind = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 2
  store i64 %kind, ptr %element_kind
  ret ptr %value
}

define void @aura_llvm_array_retain(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %retain
retain:
  %refs = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 0
  %current = load i64, ptr %refs
  %next = add i64 %current, 1
  store i64 %next, ptr %refs
  br label %done
done:
  ret void
}

define void @aura_llvm_array_release(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %release
release:
  %refs = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 0
  %current = load i64, ptr %refs
  %next = sub i64 %current, 1
  store i64 %next, ptr %refs
  %last = icmp eq i64 %next, 0
  br i1 %last, label %destroy, label %done
destroy:
  %length_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 1
  %length = load i64, ptr %length_ptr
  %kind_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 2
  %kind = load i64, ptr %kind_ptr
  br label %loop
loop:
  %index = phi i64 [ 0, %destroy ], [ %next_index, %continue ]
  %finished = icmp uge i64 %index, %length
  br i1 %finished, label %free_value, label %load_item
load_item:
  %address = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 3, i64 %index
  %raw = load i64, ptr %address
  switch i64 %kind, label %continue [
    i64 1, label %release_string
    i64 2, label %release_class
    i64 3, label %release_enum
  ]
release_string:
  %string = inttoptr i64 %raw to ptr
  call void @aura_llvm_str_release(ptr %string)
  br label %continue
release_class:
  %class = inttoptr i64 %raw to ptr
  call void @aura_llvm_class_release(ptr %class)
  br label %continue
release_enum:
  %enum = inttoptr i64 %raw to ptr
  call void @aura_llvm_enum_release(ptr %enum)
  br label %continue
continue:
  %next_index = add i64 %index, 1
  br label %loop
free_value:
  call void @free(ptr %value)
  br label %done
done:
  ret void
}

define i64 @aura_llvm_array_len(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %empty, label %read
empty:
  ret i64 0
read:
  %length = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 1
  %result = load i64, ptr %length
  ret i64 %result
}

define i64 @aura_llvm_array_get(ptr %value, i64 %index) {
entry:
  %address = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 3, i64 %index
  %result = load i64, ptr %address
  ret i64 %result
}

define void @aura_llvm_array_set(ptr %value, i64 %index, i64 %raw) {
entry:
  %address = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 3, i64 %index
  %old = load i64, ptr %address
  %kind_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 2
  %kind = load i64, ptr %kind_ptr
  switch i64 %kind, label %store [
    i64 1, label %replace_string
    i64 2, label %replace_class
    i64 3, label %replace_enum
  ]
replace_string:
  %old_string = inttoptr i64 %old to ptr
  call void @aura_llvm_str_release(ptr %old_string)
  %new_string = inttoptr i64 %raw to ptr
  call void @aura_llvm_str_retain(ptr %new_string)
  br label %store
replace_class:
  %old_class = inttoptr i64 %old to ptr
  call void @aura_llvm_class_release(ptr %old_class)
  %new_class = inttoptr i64 %raw to ptr
  call void @aura_llvm_class_retain(ptr %new_class)
  br label %store
replace_enum:
  %old_enum = inttoptr i64 %old to ptr
  call void @aura_llvm_enum_release(ptr %old_enum)
  %new_enum = inttoptr i64 %raw to ptr
  call void @aura_llvm_enum_retain(ptr %new_enum)
  br label %store
store:
  store i64 %raw, ptr %address
  ret void
}

"#;

struct EmitContext {
    signatures: Signatures,
    enum_variants: HashMap<String, EnumVariantInfo>,
    classes: HashMap<String, Vec<(String, Ty)>>,
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
        classes: classes(program),
        string_literals: Vec::new(),
    };
    module.push_str(STRING_RUNTIME);
    module.push_str(ENUM_RUNTIME);
    module.push_str(CLASS_RUNTIME);
    module.push_str(ARRAY_RUNTIME);
    emit_class_destructors(&mut module, &context.classes);
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
            let fields = context
                .classes
                .get(class_name)
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
    } else if retain && is_class_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_class_retain(ptr {value})").unwrap();
    } else if retain && is_array_type(&body.locals[from.local].ty) {
        writeln!(out, "  call void @aura_llvm_array_retain(ptr {value})").unwrap();
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
        Rvalue::Call { target, args } => {
            let values = args
                .iter()
                .map(|place| load_place(out, *place, body))
                .collect::<Result<Vec<_>, _>>()?;
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
            if target.is_constructor {
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
                let fields = context
                    .classes
                    .get(&target.name)
                    .ok_or_else(|| unsupported(&format!("class {}", target.name)))?;
                if fields.len() != args.len()
                    || fields.iter().any(|(_, ty)| {
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
                let value = next_temp(out);
                writeln!(
                    out,
                    "  {value} = call ptr @aura_llvm_class_alloc(i64 {})",
                    args.len()
                )
                .unwrap();
                for (index, ((_, ty), argument)) in fields.iter().zip(values.iter()).enumerate() {
                    let address = next_temp(out);
                    writeln!(
                        out,
                        "  {address} = getelementptr %AuraLlvmClass, ptr {value}, i32 0, i32 1, i64 {index}"
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
                        | Ty::Enum(_)
                        | Ty::EnumApp { .. } => {
                            retain_pointer_value(out, argument, ty)?;
                            let raw = next_temp(out);
                            writeln!(out, "  {raw} = ptrtoint ptr {argument} to i64").unwrap();
                            raw
                        }
                        _ => unreachable!("validated class field type"),
                    };
                    writeln!(out, "  store i64 {raw}, ptr {address}").unwrap();
                }
                return Ok(value);
            }
            if target.name == "set" && args.len() == 3 {
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
            let name = method_name.unwrap_or_else(|| symbol_name(&target.package, &target.name));
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
            let name =
                class_type_name(object_ty).ok_or_else(|| unsupported("fields outside classes"))?;
            let fields = context
                .classes
                .get(name)
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
        Rvalue::Intrinsic(_) | Rvalue::AsyncOp(_) => Err(unsupported("non-scalar MIR operation")),
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

fn load_array_element(
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

fn array_raw_value(out: &mut String, value: &str, ty: &Ty) -> Result<String, CodegenError> {
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
        Ty::String | Ty::Class(_) | Ty::ClassApp { .. } | Ty::Enum(_) | Ty::EnumApp { .. } => {
            let raw = next_temp(out);
            writeln!(out, "  {raw} = ptrtoint ptr {value} to i64").unwrap();
            Ok(raw)
        }
        _ => Err(unsupported("Array element type")),
    }
}

fn is_pointer_value_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::String | Ty::Class(_) | Ty::ClassApp { .. } | Ty::Enum(_) | Ty::EnumApp { .. }
    )
}

fn retain_pointer_value(out: &mut String, value: &str, ty: &Ty) -> Result<(), CodegenError> {
    let helper = match ty {
        Ty::String => "aura_llvm_str_retain",
        Ty::Class(_) | Ty::ClassApp { .. } => "aura_llvm_class_retain",
        Ty::Enum(_) | Ty::EnumApp { .. } => "aura_llvm_enum_retain",
        _ => return Err(unsupported("non-pointer value")),
    };
    writeln!(out, "  call void @{helper}(ptr {value})").unwrap();
    Ok(())
}

fn release_raw_value(out: &mut String, raw: &str, ty: &Ty) -> Result<(), CodegenError> {
    let helper = match ty {
        Ty::String => "aura_llvm_str_release",
        Ty::Class(_) | Ty::ClassApp { .. } => "aura_llvm_class_release",
        Ty::Enum(_) | Ty::EnumApp { .. } => "aura_llvm_enum_release",
        _ => return Err(unsupported("non-pointer value")),
    };
    let pointer = next_temp(out);
    writeln!(out, "  {pointer} = inttoptr i64 {raw} to ptr").unwrap();
    writeln!(out, "  call void @{helper}(ptr {pointer})").unwrap();
    Ok(())
}

fn array_value_from_raw(out: &mut String, raw: String, ty: &Ty) -> Result<String, CodegenError> {
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
        Ty::String | Ty::Class(_) | Ty::ClassApp { .. } | Ty::Enum(_) | Ty::EnumApp { .. } => {
            let value = next_temp(out);
            writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
            Ok(value)
        }
        _ => Err(unsupported("Array element type")),
    }
}

fn llvm_zero(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Unit => Err(unsupported("unit local")),
        Ty::Bool => Ok("false"),
        Ty::Float => Ok("0.0"),
        Ty::Int => Ok("0"),
        Ty::String | Ty::Null => Ok("null"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::String) => Ok("null"),
        Ty::Enum(_) | Ty::EnumApp { .. } | Ty::Class(_) | Ty::ClassApp { .. } => Ok("null"),
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
        Ty::Enum(_) | Ty::EnumApp { .. } | Ty::Class(_) | Ty::ClassApp { .. } => Ok("ptr"),
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
        .or_else(|| {
            signatures
                .iter()
                .find(|((_, name), _)| name.rsplit("::").next() == Some(target.name.as_str()))
                .map(|(_, signature)| signature)
        })
}

fn method_symbol_for(
    signatures: &Signatures,
    target: &aura_ir::mir::CallTarget,
    args: &[Place],
    body: &MirBody,
    package: &str,
) -> Option<String> {
    let receiver_ty = args
        .first()
        .and_then(|place| body.locals.get(place.local))
        .map(|local| &local.ty)?;
    signatures
        .iter()
        .find_map(|((owner_package, name), (_, params))| {
            let method_name = name.rsplit("::").next()?;
            if method_name != target.name
                || (owner_package != package && owner_package != &target.package)
                || !params
                    .first()
                    .is_some_and(|candidate| compatible_receiver(candidate, receiver_ty))
            {
                return None;
            }
            Some(symbol_name(owner_package, name))
        })
}

fn compatible_receiver(left: &Ty, right: &Ty) -> bool {
    match (left, right) {
        (Ty::Class(left), Ty::Class(right)) => left.split('@').next() == right.split('@').next(),
        (
            Ty::ClassApp {
                name: left,
                args: left_args,
            },
            Ty::ClassApp {
                name: right,
                args: right_args,
            },
        ) => left.split('@').next() == right.split('@').next() && left_args == right_args,
        _ => left == right,
    }
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

fn is_class_type(ty: &Ty) -> bool {
    matches!(ty, Ty::Class(_)) || matches!(ty, Ty::ClassApp { name, .. } if name != "Array")
}

fn class_type_name(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Class(name) => Some(name.split('@').next().unwrap_or(name)),
        Ty::ClassApp { name, .. } if name != "Array" => {
            Some(name.split('@').next().unwrap_or(name))
        }
        _ => None,
    }
}

fn is_array_type(ty: &Ty) -> bool {
    matches!(ty, Ty::ClassApp { name, args } if name == "Array" && args.len() == 1)
}

fn array_element_type(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::ClassApp { name, args } if name == "Array" && args.len() == 1 => args.first(),
        _ => None,
    }
}

fn array_kind(ty: &Ty) -> Result<i64, CodegenError> {
    match ty {
        Ty::String => Ok(1),
        Ty::Class(_) | Ty::ClassApp { .. } => Ok(2),
        Ty::Enum(_) | Ty::EnumApp { .. } => Ok(3),
        Ty::Int | Ty::Bool | Ty::Float => Ok(0),
        _ => Err(unsupported("Array element type")),
    }
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

fn classes(program: &LoweredProgram) -> HashMap<String, Vec<(String, Ty)>> {
    program
        .source()
        .classes
        .iter()
        .map(|class| {
            (
                class.name.clone(),
                class
                    .fields
                    .clone()
                    .into_iter()
                    .map(|field| (field.name, field.ty))
                    .collect(),
            )
        })
        .collect()
}

fn class_has_pointer_fields(fields: &Vec<(String, Ty)>) -> bool {
    fields.iter().any(|(_, ty)| is_pointer_value_type(ty))
}

fn class_release_symbol(name: &str) -> String {
    format!(
        "aura_llvm_class_release_{}",
        name.chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>()
    )
}

fn emit_class_destructors(out: &mut String, classes: &HashMap<String, Vec<(String, Ty)>>) {
    for (name, fields) in classes {
        if !class_has_pointer_fields(fields) {
            continue;
        }
        let symbol = class_release_symbol(name);
        writeln!(out, "define void @{symbol}(ptr %value) {{").unwrap();
        out.push_str("entry:\n");
        out.push_str("  %is_null = icmp eq ptr %value, null\n");
        out.push_str("  br i1 %is_null, label %done, label %release\n");
        out.push_str("release:\n");
        out.push_str("  %refs = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 0\n");
        out.push_str("  %current = load i64, ptr %refs\n");
        out.push_str("  %next = sub i64 %current, 1\n");
        out.push_str("  store i64 %next, ptr %refs\n");
        out.push_str("  %last = icmp eq i64 %next, 0\n");
        out.push_str("  br i1 %last, label %destroy, label %done\n");
        out.push_str("destroy:\n");
        for (index, (_, ty)) in fields.iter().enumerate() {
            if !is_pointer_value_type(ty) {
                continue;
            }
            writeln!(
                out,
                "  %field_address{index} = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 1, i64 {index}"
            )
            .unwrap();
            writeln!(
                out,
                "  %field_raw{index} = load i64, ptr %field_address{index}"
            )
            .unwrap();
            let helper = match ty {
                Ty::String => "aura_llvm_str_release",
                Ty::Class(_) | Ty::ClassApp { .. } => "aura_llvm_class_release",
                Ty::Enum(_) | Ty::EnumApp { .. } => "aura_llvm_enum_release",
                _ => unreachable!("pointer field type checked above"),
            };
            writeln!(
                out,
                "  %field_ptr{index} = inttoptr i64 %field_raw{index} to ptr"
            )
            .unwrap();
            writeln!(out, "  call void @{helper}(ptr %field_ptr{index})").unwrap();
        }
        out.push_str("  call void @free(ptr %value)\n");
        out.push_str("  br label %done\n");
        out.push_str("done:\n  ret void\n}\n\n");
    }
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
    let sanitize = |value: &str| {
        value
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>()
    };
    let package = sanitize(package);
    let name = sanitize(name);
    format!("aura_{}_{}", package, name)
}

fn unsupported(feature: &str) -> CodegenError {
    CodegenError::Configuration(format!(
        "LLVM backend does not support {feature} in the current MIR contract"
    ))
}
