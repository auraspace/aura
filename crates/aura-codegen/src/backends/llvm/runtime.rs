//! Textual LLVM runtime declarations and helpers.

pub(crate) const STRING_RUNTIME: &str = r#"
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
declare i32 @_setjmp(ptr)
declare void @aura_try_enter(ptr)
declare void @aura_try_leave()
declare void @aura_throw_string(ptr)
declare void @aura_throw_int(i64)
declare void @aura_throw_bool(i1)

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

pub(crate) const ENUM_RUNTIME: &str = r#"
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

pub(crate) const CLASS_RUNTIME: &str = r#"
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

pub(crate) const ARRAY_RUNTIME: &str = r#"
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

pub(crate) const CHANNEL_RUNTIME: &str = r#"
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

pub(crate) const MISC_RUNTIME: &str = r#"
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
