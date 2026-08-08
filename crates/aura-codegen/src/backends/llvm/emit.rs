use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use aura_ast::Span;
use aura_ir::mir::{BinaryOp, Intrinsic, MirBody, Place, Rvalue, Statement, Terminator, UnaryOp};
use aura_ir::{FunctionIr, LoweredProgram};
use aura_sema::Ty;

use crate::error::CodegenError;

#[path = "types.rs"]
mod types;
use types::*;

type Signatures = HashMap<(String, String), (Ty, Vec<Ty>)>;

const STRING_RUNTIME: &str = r#"
%AuraLlvmString = type { i64, i64, [0 x i8] }
%AuraLlvmOptInt = type { i1, i64 }
%AuraLlvmOptBool = type { i1, i1 }
%AuraLlvmOptFloat = type { i1, double }
declare ptr @malloc(i64)
declare ptr @realloc(ptr, i64)
declare void @free(ptr)
declare i64 @strlen(ptr)
declare ptr @memcpy(ptr, ptr, i64)
declare i32 @strcmp(ptr, ptr)
declare i32 @puts(ptr)
declare i32 @snprintf(ptr, i64, ptr, ...)
declare void @abort()

@.aura_int_fmt = private unnamed_addr constant [4 x i8] c"%ld\00", align 1
@.aura_float_fmt = private unnamed_addr constant [3 x i8] c"%g\00", align 1

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

define ptr @aura_llvm_int_to_string(i64 %value) {
entry:
  %buffer = alloca [64 x i8]
  %data = getelementptr [64 x i8], ptr %buffer, i64 0, i64 0
  %format = getelementptr [4 x i8], ptr @.aura_int_fmt, i64 0, i64 0
  %ignored = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %data, i64 64, ptr %format, i64 %value)
  %result = call ptr @aura_llvm_str_new(ptr %data)
  ret ptr %result
}

define ptr @aura_llvm_float_to_string(double %value) {
entry:
  %buffer = alloca [64 x i8]
  %data = getelementptr [64 x i8], ptr %buffer, i64 0, i64 0
  %format = getelementptr [3 x i8], ptr @.aura_float_fmt, i64 0, i64 0
  %ignored = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %data, i64 64, ptr %format, double %value)
  %result = call ptr @aura_llvm_str_new(ptr %data)
  ret ptr %result
}

define ptr @aura_llvm_bool_to_string(i1 %value) {
entry:
  br i1 %value, label %true_value, label %false_value
true_value:
  %true = call ptr @aura_llvm_str_new(ptr getelementptr ([5 x i8], ptr @.aura_true, i64 0, i64 0))
  ret ptr %true
false_value:
  %false = call ptr @aura_llvm_str_new(ptr getelementptr ([6 x i8], ptr @.aura_false, i64 0, i64 0))
  ret ptr %false
}

@.aura_true = private unnamed_addr constant [5 x i8] c"true\00", align 1
@.aura_false = private unnamed_addr constant [6 x i8] c"false\00", align 1

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
%AuraLlvmArray = type { i64, i64, i64, i64, ptr }

define ptr @aura_llvm_array_alloc(i64 %len, i64 %kind) {
entry:
  %positive = icmp sgt i64 %len, 0
  %capacity = select i1 %positive, i64 %len, i64 1
  %data_bytes = mul i64 %capacity, 8
  %value = call ptr @malloc(i64 40)
  %refs = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 0
  store i64 1, ptr %refs
  %length = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 1
  store i64 %len, ptr %length
  %element_kind = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 2
  store i64 %kind, ptr %element_kind
  %capacity_field = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 3
  store i64 %capacity, ptr %capacity_field
  %data = call ptr @malloc(i64 %data_bytes)
  %data_field = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 4
  store ptr %data, ptr %data_field
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
  %data_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 4
  %data = load ptr, ptr %data_ptr
  br label %loop
loop:
  %index = phi i64 [ 0, %destroy ], [ %next_index, %continue ]
  %finished = icmp uge i64 %index, %length
  br i1 %finished, label %free_value, label %load_item
load_item:
  %address = getelementptr i64, ptr %data, i64 %index
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
  call void @free(ptr %data)
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
  %data_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 4
  %data = load ptr, ptr %data_ptr
  %address = getelementptr i64, ptr %data, i64 %index
  %result = load i64, ptr %address
  ret i64 %result
}

define void @aura_llvm_array_set(ptr %value, i64 %index, i64 %raw) {
entry:
  %data_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 4
  %data = load ptr, ptr %data_ptr
  %address = getelementptr i64, ptr %data, i64 %index
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

define void @aura_llvm_array_push(ptr %value, i64 %raw) {
entry:
  %length_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 1
  %length = load i64, ptr %length_ptr
  %capacity_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 3
  %capacity = load i64, ptr %capacity_ptr
  %full = icmp uge i64 %length, %capacity
  br i1 %full, label %grow, label %store
grow:
  %doubled = mul i64 %capacity, 2
  %new_capacity = select i1 %full, i64 %doubled, i64 1
  %new_bytes = mul i64 %new_capacity, 8
  %data_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 4
  %old_data = load ptr, ptr %data_ptr
  %new_data = call ptr @realloc(ptr %old_data, i64 %new_bytes)
  store ptr %new_data, ptr %data_ptr
  store i64 %new_capacity, ptr %capacity_ptr
  br label %store
store:
  %data_ptr2 = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 4
  %data = load ptr, ptr %data_ptr2
  %address = getelementptr i64, ptr %data, i64 %length
  store i64 %raw, ptr %address
  %new_length = add i64 %length, 1
  store i64 %new_length, ptr %length_ptr
  ret void
}

define i64 @aura_llvm_array_pop(ptr %value) {
entry:
  %length_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 1
  %length = load i64, ptr %length_ptr
  %empty = icmp eq i64 %length, 0
  br i1 %empty, label %none, label %load_value
none:
  ret i64 0
load_value:
  %index = sub i64 %length, 1
  %data_ptr = getelementptr %AuraLlvmArray, ptr %value, i32 0, i32 4
  %data = load ptr, ptr %data_ptr
  %address = getelementptr i64, ptr %data, i64 %index
  %raw = load i64, ptr %address
  store i64 %index, ptr %length_ptr
  ret i64 %raw
}

"#;

struct EmitContext {
    signatures: Signatures,
    enum_variants: HashMap<String, EnumVariantInfo>,
    classes: HashMap<String, Vec<(String, Ty)>>,
    foreign_names: HashSet<String>,
    string_literals: Vec<String>,
}

const CHANNEL_RUNTIME: &str = r#"
%AuraLlvmChannel = type { i64, i64, i64, i64, i64, ptr }

define ptr @aura_llvm_channel_new(i64 %capacity) {
entry:
  %positive = icmp sgt i64 %capacity, 0
  %size = select i1 %positive, i64 %capacity, i64 1
  %value = call ptr @malloc(i64 48)
  %data_size = mul i64 %size, 8
  %data = call ptr @malloc(i64 %data_size)
  %capacity_ptr = getelementptr %AuraLlvmChannel, ptr %value, i32 0, i32 0
  store i64 %size, ptr %capacity_ptr
  %count_ptr = getelementptr %AuraLlvmChannel, ptr %value, i32 0, i32 1
  store i64 0, ptr %count_ptr
  %head_ptr = getelementptr %AuraLlvmChannel, ptr %value, i32 0, i32 2
  store i64 0, ptr %head_ptr
  %tail_ptr = getelementptr %AuraLlvmChannel, ptr %value, i32 0, i32 3
  store i64 0, ptr %tail_ptr
  %closed_ptr = getelementptr %AuraLlvmChannel, ptr %value, i32 0, i32 4
  store i64 0, ptr %closed_ptr
  %data_ptr = getelementptr %AuraLlvmChannel, ptr %value, i32 0, i32 5
  store ptr %data, ptr %data_ptr
  ret ptr %value
}

define i1 @aura_llvm_channel_send(ptr %channel, i64 %raw) {
entry:
  %closed_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 4
  %closed = load i64, ptr %closed_ptr
  %is_closed = icmp ne i64 %closed, 0
  br i1 %is_closed, label %fail, label %check_full
check_full:
  %capacity_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 0
  %capacity = load i64, ptr %capacity_ptr
  %count_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 1
  %count = load i64, ptr %count_ptr
  %full = icmp uge i64 %count, %capacity
  br i1 %full, label %fail, label %store
store:
  %tail_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 3
  %tail = load i64, ptr %tail_ptr
  %data_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 5
  %data = load ptr, ptr %data_ptr
  %address = getelementptr i64, ptr %data, i64 %tail
  store i64 %raw, ptr %address
  %next = add i64 %tail, 1
  %wrapped = urem i64 %next, %capacity
  store i64 %wrapped, ptr %tail_ptr
  %new_count = add i64 %count, 1
  store i64 %new_count, ptr %count_ptr
  ret i1 true
fail:
  ret i1 false
}

define i1 @aura_llvm_channel_receive(ptr %channel, ptr %out) {
entry:
  %count_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 1
  %count = load i64, ptr %count_ptr
  %empty = icmp eq i64 %count, 0
  br i1 %empty, label %fail, label %load_value
load_value:
  %head_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 2
  %head = load i64, ptr %head_ptr
  %data_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 5
  %data = load ptr, ptr %data_ptr
  %address = getelementptr i64, ptr %data, i64 %head
  %raw = load i64, ptr %address
  store i64 %raw, ptr %out
  %capacity_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 0
  %capacity = load i64, ptr %capacity_ptr
  %next = add i64 %head, 1
  %wrapped = urem i64 %next, %capacity
  store i64 %wrapped, ptr %head_ptr
  %new_count = sub i64 %count, 1
  store i64 %new_count, ptr %count_ptr
  ret i1 true
fail:
  ret i1 false
}

define void @aura_llvm_channel_close(ptr %channel) {
entry:
  %closed_ptr = getelementptr %AuraLlvmChannel, ptr %channel, i32 0, i32 4
  store i64 1, ptr %closed_ptr
  ret void
}
"#;

const MISC_RUNTIME: &str = r#"
define void @aura_llvm_assert(i1 %condition) {
entry:
  br i1 %condition, label %done, label %failed
failed:
  call void @abort()
  unreachable
done:
  ret void
}

define void @aura_llvm_gc_collect() {
entry:
  ret void
}
"#;

#[derive(Clone)]
struct EnumVariantInfo {
    tag: i64,
    type_params: Vec<String>,
    fields: Vec<(String, Ty)>,
}

pub fn emit_module(program: &LoweredProgram) -> Result<String, CodegenError> {
    validate_program(program)?;
    let mut module = String::from("; ModuleID = 'aura'\nsource_filename = \"aura\"\n\n");
    let mut context = EmitContext {
        signatures: signatures(program),
        enum_variants: enum_variants(program),
        classes: classes(program),
        foreign_names: program
            .source()
            .ast
            .foreign_functions
            .iter()
            .map(|foreign| foreign.name.name.clone())
            .collect(),
        string_literals: Vec::new(),
    };
    let mut extra_functions = async_functions(program);
    let mut seen_spawns = extra_functions
        .iter()
        .map(|function| function.name.clone())
        .collect::<HashSet<_>>();
    for function in program
        .checked()
        .functions
        .iter()
        .chain(program.checked().generic_functions.iter())
    {
        if let Some(body) = &function.body {
            collect_spawn_functions(
                body,
                &function.package,
                &mut extra_functions,
                &mut seen_spawns,
            );
        }
    }
    for body in program
        .checked()
        .async_mir
        .iter()
        .chain(program.checked().open_generic_async_mir.iter())
        .chain(program.checked().generic_async_mir.iter())
        .chain(program.checked().generic_async_method_mir.iter())
    {
        collect_spawn_functions(
            body,
            &program.checked().package,
            &mut extra_functions,
            &mut seen_spawns,
        );
    }
    for function in &extra_functions {
        context.signatures.insert(
            (function.package.clone(), function.name.clone()),
            (
                function.ret.ty.clone(),
                function
                    .params
                    .iter()
                    .map(|param| param.ty.clone())
                    .collect(),
            ),
        );
    }
    module.push_str(STRING_RUNTIME);
    module.push_str(ENUM_RUNTIME);
    module.push_str(CLASS_RUNTIME);
    module.push_str(ARRAY_RUNTIME);
    module.push_str(CHANNEL_RUNTIME);
    module.push_str(MISC_RUNTIME);
    emit_class_destructors(&mut module, &context.classes);
    emit_foreign_declarations(&mut module, program, &context.foreign_names)?;
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
    for function in &extra_functions {
        if let Some(body) = &function.body {
            emit_function(&mut module, function, body, &mut context)?;
        }
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

fn async_functions(program: &LoweredProgram) -> Vec<FunctionIr> {
    program
        .checked()
        .async_mir
        .iter()
        .chain(program.checked().open_generic_async_mir.iter())
        .chain(program.checked().generic_async_mir.iter())
        .chain(program.checked().generic_async_method_mir.iter())
        .map(|body| {
            let ast = &program.source().ast;
            let parameter_count = ast
                .async_functions
                .iter()
                .find(|function| {
                    body.name == function.name.name
                        || body.name.starts_with(&format!("{}_", function.name.name))
                })
                .map(|function| function.params.len())
                .or_else(|| {
                    ast.classes.iter().find_map(|class| {
                        class.methods.iter().find_map(|method| {
                            let matches = body.name == method.name.name
                                || body.name.starts_with(&format!("{}_", method.name.name))
                                || body.name.contains(&format!("_{}_", method.name.name));
                            matches.then_some(method.params.len() + 1)
                        })
                    })
                })
                .unwrap_or(0);
            synthetic_function(body, program.checked().package.clone(), parameter_count)
        })
        .collect()
}

fn collect_spawn_functions(
    body: &MirBody,
    package: &str,
    output: &mut Vec<FunctionIr>,
    seen: &mut HashSet<String>,
) {
    for block in &body.blocks {
        for statement in &block.statements {
            let value = match statement {
                Statement::Assign { value, .. } | Statement::Evaluate(value) => value,
                _ => continue,
            };
            let Rvalue::AsyncOp(aura_ir::mir::AsyncOp::Spawn { body, captures }) = value else {
                continue;
            };
            if seen.insert(body.name.clone()) {
                output.push(synthetic_function(body, package.to_owned(), captures.len()));
            }
            collect_spawn_functions(body, package, output, seen);
        }
    }
}

fn synthetic_function(body: &MirBody, package: String, parameter_count: usize) -> FunctionIr {
    FunctionIr {
        name: body.name.clone(),
        package,
        params: body
            .locals
            .iter()
            .take(parameter_count)
            .map(|local| aura_ir::ValueFact {
                ty: local.ty.clone(),
                ownership: aura_ir::ownership::mode_for_ty(&local.ty),
                span: Span::new(0, 0),
            })
            .collect(),
        ret: aura_ir::ValueFact {
            ty: body.return_ty.clone(),
            ownership: aura_ir::ownership::mode_for_ty(&body.return_ty),
            span: Span::new(0, 0),
        },
        type_params: Vec::new(),
        bounds: HashMap::new(),
        effect: body.effect,
        body: Some(body.clone()),
        span: Span::new(0, 0),
    }
}

fn emit_foreign_declarations(
    out: &mut String,
    program: &LoweredProgram,
    foreign_names: &HashSet<String>,
) -> Result<(), CodegenError> {
    for function in program
        .checked()
        .functions
        .iter()
        .filter(|function| foreign_names.contains(&function.name))
    {
        let params = function
            .params
            .iter()
            .map(|param| llvm_type(&param.ty))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        writeln!(
            out,
            "declare {} @{}({params})",
            llvm_type(&function.ret.ty)?,
            function.name
        )
        .unwrap();
    }
    Ok(())
}

fn validate_program(program: &LoweredProgram) -> Result<(), CodegenError> {
    let checked = program.checked();
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
    for body in checked
        .async_mir
        .iter()
        .chain(checked.open_generic_async_mir.iter())
        .chain(checked.generic_async_mir.iter())
        .chain(checked.generic_async_method_mir.iter())
    {
        body.validate()
            .map_err(|error| unsupported(&format!("invalid async MIR: {error:?}")))?;
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
        Terminator::Await {
            task,
            result,
            resume,
            unwind,
        } => {
            let task_ty = &body.locals[task.local].ty;
            let Some(payload_ty) = task_payload_type(task_ty) else {
                return Err(unsupported("awaiting a non-task value"));
            };
            let value = if *payload_ty == Ty::Unit {
                None
            } else {
                Some(load_place(out, *task, body)?)
            };
            if let Some(value) = value {
                if body.locals[result.local].ty != *payload_ty {
                    return Err(unsupported("await result type"));
                }
                writeln!(
                    out,
                    "  store {} {value}, ptr %slot{}",
                    llvm_type(payload_ty)?,
                    result.local
                )
                .unwrap();
            }
            writeln!(out, "  br label %bb{resume}").unwrap();
            if unwind.is_some() {
                return Err(unsupported("async unwind edges"));
            }
        }
        Terminator::Throw { .. } | Terminator::Cancel => {
            return Err(unsupported("exception or cancellation control flow"));
        }
    }
    Ok(())
}

fn optional_literal(result_ty: Option<&Ty>, llvm_value_ty: &str, value: &str) -> String {
    match result_ty {
        Some(Ty::Nullable(inner)) if matches!(inner.as_ref(), Ty::Int | Ty::Bool | Ty::Float) => {
            format!("{{ i1 true, {llvm_value_ty} {value} }}")
        }
        _ => value.to_owned(),
    }
}

fn nullable_zero_value(result_ty: Option<&Ty>) -> Option<&'static str> {
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

fn build_optional_value(out: &mut String, llvm_ty: &str, value_ty: &str, value: &str) -> String {
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

fn extract_optional_payload(
    out: &mut String,
    value: &str,
    ty: &Ty,
) -> Result<String, CodegenError> {
    let payload = next_temp(out);
    writeln!(
        out,
        "  {payload} = extractvalue {} {value}, 1",
        llvm_type(ty)?
    )
    .unwrap();
    Ok(payload)
}

fn emit_use_value(
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
        (Ty::Nullable(inner), Some(destination)) if inner.as_ref() == destination => {
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

fn coerce_llvm_argument(
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
    if is_pointer_value_type(source_ty) && is_pointer_value_type(expected_ty) {
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

fn emit_rvalue(
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
                    _ => Err(unsupported("toInt operand type")),
                };
            }
            if let Some(variant) = &target.variant {
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
                    )
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
                        | Ty::Enum(_)
                        | Ty::EnumApp { .. } => {
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
            if target.name == "send" && args.len() == 2 {
                let Ty::Channel(element_ty) = &body.locals[args[0].local].ty else {
                    return Err(unsupported("send target outside Channel"));
                };
                if body.locals[args[1].local].ty != **element_ty {
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
            if target.name == "get" && args.len() == 2 {
                let Some(element_ty) = array_element_type(&body.locals[args[0].local].ty) else {
                    return Err(unsupported("get target outside Array"));
                };
                if body.locals[args[1].local].ty != Ty::Int {
                    return Err(unsupported("Array get index type"));
                }
                return load_array_element(out, args[0], args[1], element_ty, body);
            }
            if target.name == "push" && args.len() == 2 {
                let Some(element_ty) = array_element_type(&body.locals[args[0].local].ty) else {
                    return Err(unsupported("push target outside Array"));
                };
                if !types_compatible(&body.locals[args[1].local].ty, element_ty) {
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
            if target.name == "pop" && args.len() == 1 {
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
            let name = if context.foreign_names.contains(&target.name) {
                target.name.clone()
            } else {
                method_name.unwrap_or_else(|| symbol_name(&target.package, &target.name))
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
            let (return_ty, parameter_tys) = signature_for(&context.signatures, package, target)
                .ok_or_else(|| unsupported(&format!("call target {}", target.name)))?;
            if parameter_tys.len() != values.len() {
                return Err(unsupported(&format!("call arity for {}", target.name)));
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
        Rvalue::TypeTest { operand, .. } => {
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
            if !is_string_type(&body.locals[operand.local].ty) {
                return Err(unsupported("type tests outside nullable heap value"));
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
        Rvalue::Intrinsic(intrinsic) => match intrinsic {
            Intrinsic::GcCollect => {
                writeln!(out, "  call void @aura_llvm_gc_collect()").unwrap();
                Ok(String::new())
            }
        },
        Rvalue::AsyncOp(operation) => {
            emit_async_op(out, operation, body, result_ty, package, context)
        }
    }
}

fn emit_async_op(
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

fn emit_equality(
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

fn llvm_zero(ty: &Ty) -> Result<&'static str, CodegenError> {
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
