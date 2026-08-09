//! Textual LLVM runtime declarations and helpers.

pub(crate) const STRING_RUNTIME: &str = r#"
%AuraLlvmString = type { i64, i64, [0 x i8] }
%AuraLlvmOptInt = type { i1, i64 }
%AuraLlvmOptBool = type { i1, i1 }
%AuraLlvmOptFloat = type { i1, double }
%AuraLlvmFun = type { ptr, ptr }
%AuraLlvmFunPayload = type { ptr, ptr }
%AuraLlvmBoxI64 = type { i64, i32 }
%AuraLlvmBoxBool = type { i1, i32 }
%AuraLlvmBoxF64 = type { double, i32 }
%AuraLlvmBoxStr = type { ptr, i32 }
declare ptr @malloc(i64)
declare ptr @realloc(ptr, i64)
declare void @free(ptr)
declare i32 @aura_runtime_check_abi(i32, ptr)
declare i64 @strlen(ptr)
declare ptr @memcpy(ptr, ptr, i64)
declare i32 @strcmp(ptr, ptr)
declare i32 @strncmp(ptr, ptr, i64)
declare i1 @aura_llvm_str_is_empty(ptr)
declare i1 @aura_llvm_str_starts_with(ptr, ptr)
declare i1 @aura_llvm_str_contains(ptr, ptr)
declare i1 @aura_llvm_str_ends_with(ptr, ptr)
declare i64 @aura_llvm_str_char_at(ptr, i64)
declare i64 @aura_llvm_str_index_of(ptr, ptr)
declare ptr @aura_llvm_str_substring(ptr, i64, i64)
declare ptr @aura_llvm_str_trim(ptr, i64)
declare ptr @aura_llvm_str_case(ptr, i1)
declare %AuraLlvmOptInt @aura_llvm_str_to_int(ptr)
declare ptr @aura_llvm_str_split(ptr, ptr)
declare ptr @aura_json_object_get(ptr, ptr)
declare ptr @aura_json_array_at(ptr, i64)
declare i32 @aura_ws_connect(ptr)
declare i64 @aura_ws_send(ptr, i64, ptr)
declare ptr @aura_ws_receive(ptr, ptr)
declare i32 @aura_ws_close(ptr)
declare ptr @aura_http_request_method(ptr)
declare ptr @aura_http_request_target(ptr)
declare ptr @aura_http_request_version(ptr)
declare i64 @aura_http_request_header_count(ptr)
declare ptr @aura_http_request_header_name(ptr, i64)
declare ptr @aura_http_request_header_value(ptr, i64)
declare ptr @aura_http_request_body(ptr)
declare i32 @aura_http_response_status(ptr)
declare i64 @aura_http_response_header_count(ptr)
declare ptr @aura_http_response_header_name(ptr, i64)
declare ptr @aura_http_response_header_value(ptr, i64)
declare ptr @aura_http_response_body(ptr)
declare i32 @aura_http_response_keep_alive(ptr)
declare i32 @aura_http_response_set_status(ptr, i32)
declare i32 @aura_http_response_set_connection(ptr, i32)
declare i32 @aura_http_response_set_body(ptr, ptr, i64)
declare i32 @aura_http_response_add_header(ptr, ptr, ptr)
declare i32 @aura_udp_bind(ptr, i64)
declare i64 @aura_udp_send(ptr, i64, ptr, i64, ptr)
declare ptr @aura_udp_receive(ptr, i64, i64, ptr, ptr)
declare i32 @aura_udp_close(ptr, i64)
declare ptr @aura_llvm_udp_receive_task(ptr, ptr, i64, i64, i64, i64)
declare ptr @aura_llvm_net_listen(ptr)
declare ptr @aura_llvm_net_connect(ptr, i64)
declare i32 @aura_llvm_net_close_listener(ptr)
declare i32 @aura_llvm_net_close_stream(ptr)
declare ptr @aura_llvm_net_accept_task(ptr, ptr)
declare ptr @aura_llvm_net_read_task(ptr, ptr, i64, i64)
declare ptr @aura_llvm_net_read_exact_task(ptr, ptr, i64, i64, i64, i64)
declare ptr @aura_llvm_net_write_all_task(ptr, ptr, ptr, i64)
declare ptr @aura_llvm_tls_read_task(ptr, ptr, i64, i64, i64)
declare ptr @aura_llvm_tls_write_task(ptr, ptr, ptr, i64)
declare ptr @aura_llvm_net_write_task(ptr, ptr, ptr, i64)
declare ptr @aura_llvm_io_read_fd_task(ptr, i64, i64)
declare ptr @aura_llvm_io_write_fd_task(ptr, i64, ptr)
declare ptr @aura_llvm_io_read_fd_result_task(ptr, i64, i64, i64, i64, ptr, ptr)
declare ptr @aura_llvm_io_write_fd_result_task(ptr, i64, ptr, i64, i64, ptr, ptr)
declare ptr @aura_llvm_net_read_result_task(ptr, ptr, i64, i64, ptr)
declare ptr @aura_llvm_net_write_result_task(ptr, ptr, ptr, i64, ptr)
declare ptr @aura_llvm_http_read_chunk_task(ptr, ptr, i64)
declare ptr @aura_llvm_http_write_chunk_task(ptr, ptr, ptr)
declare ptr @aura_llvm_http_read_chunk_result_task(ptr, ptr, i64, i64, ptr)
declare ptr @aura_llvm_http_write_chunk_result_task(ptr, ptr, ptr, ptr, i64, ptr)
declare ptr @aura_llvm_http_serve_connection_task(ptr, ptr, ptr, ptr, i64, i64, i64, i64)
declare ptr @aura_llvm_http_serve_task(ptr, ptr, ptr, ptr, i64, i64, i64, i64)
declare i64 @aura_hash_string(ptr)
declare ptr @aura_llvm_array_clone(ptr)
declare void @aura_llvm_array_clear(ptr)
declare void @aura_llvm_array_reserve(ptr, i64)
declare i1 @aura_llvm_array_is_empty(ptr)
declare i32 @puts(ptr)
declare void @aura_print(ptr)
declare void @aura_println(ptr)
declare void @aura_eprint(ptr)
declare void @aura_eprintln(ptr)
declare void @aura_fun_env_retain(ptr)
declare void @aura_fun_env_free(ptr)
declare ptr @aura_box_i64_new(i64)
declare void @aura_box_i64_retain(ptr)
declare void @aura_box_i64_release(ptr)
declare ptr @aura_box_bool_new(i1)
declare void @aura_box_bool_retain(ptr)
declare void @aura_box_bool_release(ptr)
declare ptr @aura_box_f64_new(double)
declare void @aura_box_f64_retain(ptr)
declare void @aura_box_f64_release(ptr)
declare ptr @aura_box_ptr_new(ptr, ptr)
declare void @aura_box_ptr_retain(ptr)
declare void @aura_box_ptr_release(ptr)
declare ptr @aura_box_ptr_get(ptr)
declare ptr @aura_box_ptr_set(ptr, ptr, ptr)
declare ptr @aura_box_str_new(ptr)
declare void @aura_box_str_retain(ptr)
declare void @aura_box_str_release(ptr)
declare ptr @aura_box_str_set(ptr, ptr)
declare ptr @aura_box_str_get(ptr)
declare ptr @aura_read_file(ptr)
declare ptr @aura_try_read_file(ptr)
declare void @aura_write_file(ptr, ptr)
declare i1 @aura_try_write_file(ptr, ptr)
declare void @aura_append_file(ptr, ptr)
declare i1 @aura_file_exists(ptr)
declare i64 @aura_file_size(ptr)
declare ptr @aura_read_line()
declare ptr @aura_read_all_stdin()
declare i64 @aura_args_count()
declare ptr @aura_args_get(i64)
declare void @aura_exit(i64)
declare i32 @snprintf(ptr, i64, ptr, ...)
declare void @abort()
declare i32 @_setjmp(ptr)
declare void @aura_try_enter(ptr)
declare void @aura_try_leave()
declare void @aura_ex_clear()
declare ptr @aura_task_scope_begin(ptr)
declare i32 @aura_task_scope_end(ptr)
declare ptr @aura_task_executor_new()
declare ptr @aura_task_frame_new(i64, ptr, ptr)
declare ptr @aura_task_frame_new_blocking(ptr, ptr, ptr, ptr)
declare ptr @aura_task_frame_data(ptr)
declare void @aura_task_frame_set_data_drop(ptr, ptr)
declare void @aura_task_frame_set_gc_mark(ptr, ptr)
declare void @aura_task_frame_set_gc_stack_map(ptr, ptr, i64)
declare void @aura_gc_mark_ptr(ptr)
declare void @aura_gc_write_barrier(ptr, ptr)
declare void @aura_task_frame_set_result(ptr, ptr, i64, ptr)
declare i32 @aura_task_executor_submit(ptr, ptr)
declare i32 @aura_llvm_task_join_i64(ptr, ptr, ptr)
declare i32 @aura_llvm_task_join_ptr(ptr, ptr, ptr)
declare i32 @aura_llvm_task_join_unit(ptr, ptr)
declare i32 @aura_llvm_task_join_status(ptr, ptr)
declare void @aura_llvm_task_raise_failure(ptr)
declare ptr @aura_llvm_task_error_message(ptr)
declare ptr @aura_llvm_lazy_int_new(ptr, ptr)
declare i64 @aura_llvm_lazy_int_get(ptr)
declare i32 @aura_llvm_lazy_is_initialized(ptr)
declare void @aura_llvm_lazy_int_destroy(ptr)
declare i64 @aura_llvm_sync_load(ptr)
declare void @aura_llvm_sync_store(ptr, i64)
declare i64 @aura_llvm_sync_fetch_add(ptr, i64)
declare i32 @aura_llvm_sync_compare_exchange(ptr, i64, i64)
declare i32 @aura_llvm_sync_try_lock(ptr)
declare void @aura_llvm_sync_unlock(ptr)
declare i32 @aura_llvm_sync_is_locked(ptr)
declare i32 @aura_llvm_sync_try_read(ptr)
declare i32 @aura_llvm_sync_try_write(ptr)
declare void @aura_llvm_sync_unlock_read(ptr)
declare void @aura_llvm_sync_unlock_write(ptr)
declare i64 @aura_llvm_sync_reader_count(ptr)
declare i32 @aura_llvm_sync_is_write_locked(ptr)
declare i32 @aura_llvm_task_cancel(ptr, ptr)
declare i32 @aura_llvm_task_release(ptr, ptr)
declare i32 @aura_task_executor_retain_payload(ptr, ptr)
declare i32 @aura_task_executor_release_payload(ptr, ptr)
declare i32 @aura_task_channel_retain(ptr)
declare void @aura_task_channel_destroy(ptr)
declare i32 @aura_task_frame_link_cancellation(ptr, ptr)
declare void @aura_throw_string(ptr)
declare void @aura_throw_int(i64)
declare void @aura_throw_bool(i1)
declare i32 @aura_llvm_task_fail_from_exception(ptr)
declare void @aura_throw_obj_with_destructor(ptr, ptr, ptr)
declare i32 @aura_ex_matches(ptr)
declare ptr @aura_ex_as_obj()
declare ptr @aura_ex_take_obj()
declare void @aura_ex_rethrow()
declare ptr @aura_ex_as_string()
declare i64 @aura_ex_as_int()
declare i1 @aura_ex_as_bool()
declare i64 @aura_ex_cause_count()
declare i32 @aura_ex_source_span_start()
declare i32 @aura_ex_source_span_end()
declare ptr @aura_ex_cause_type_copy(i64)
declare i32 @aura_ex_cause_span_start(i64)
declare i32 @aura_ex_cause_span_end(i64)
declare i32 @aura_ex_add_cause(ptr, i32, i32)

define void @aura_llvm_fun_retain(%AuraLlvmFun %value) {
entry:
  %env = extractvalue %AuraLlvmFun %value, 0
  call void @aura_fun_env_retain(ptr %env)
  ret void
}

define void @aura_llvm_fun_release(%AuraLlvmFun %value) {
entry:
  %env = extractvalue %AuraLlvmFun %value, 0
  call void @aura_fun_env_free(ptr %env)
  ret void
}

define void @aura_llvm_fun_box_drop(ptr %value) {
entry:
  %env_ptr = getelementptr %AuraLlvmFunPayload, ptr %value, i32 0, i32 0
  %env = load ptr, ptr %env_ptr
  call void @aura_fun_env_free(ptr %env)
  call void @free(ptr %value)
  ret void
}

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

define ptr @aura_llvm_string_single_byte(i64 %value) {
entry:
  %byte = trunc i64 %value to i8
  %storage = alloca [2 x i8], align 1
  %first = getelementptr [2 x i8], ptr %storage, i64 0, i64 0
  store i8 %byte, ptr %first
  %last = getelementptr [2 x i8], ptr %storage, i64 0, i64 1
  store i8 0, ptr %last
  %result = call ptr @aura_llvm_str_new(ptr %first)
  ret ptr %result
}

define ptr @aura_llvm_str_new_nullable(ptr %source) {
entry:
  %is_null = icmp eq ptr %source, null
  br i1 %is_null, label %null, label %copy
null:
  ret ptr null
copy:
  %value = call ptr @aura_llvm_str_new(ptr %source)
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
@.aura_scope_failed = private unnamed_addr constant [29 x i8] c"structured child task failed\00", align 1
@.aura_scope_cancelled = private unnamed_addr constant [32 x i8] c"structured child task cancelled\00", align 1

"#;

pub(crate) const ENUM_RUNTIME: &str = r#"
%AuraLlvmEnum = type { i64, i64, ptr, [0 x i64] }

define ptr @aura_llvm_enum_alloc(i64 %fields, ptr %destructor) {
entry:
  %field_bytes = mul i64 %fields, 8
  %size = add i64 %field_bytes, 24
  %value = call ptr @malloc(i64 %size)
  %refs = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 0
  store i64 1, ptr %refs
  %drop = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 2
  store ptr %destructor, ptr %drop
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
  %drop_ptr = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 2
  %drop = load ptr, ptr %drop_ptr
  %has_drop = icmp ne ptr %drop, null
  br i1 %has_drop, label %drop_payload, label %free_value
drop_payload:
  call void %drop(ptr %value)
  br label %free_value
free_value:
  call void @free(ptr %value)
  br label %done
done:
  ret void
}

; Result intrinsics use these small ABI-stable destructors for unnamed enum
; variants whose payload type is known at the call site.
define void @aura_llvm_enum_drop_string(ptr %value) {
entry:
  %field = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 3, i64 0
  %raw = load i64, ptr %field
  %payload = inttoptr i64 %raw to ptr
  call void @aura_llvm_str_release(ptr %payload)
  ret void
}

define void @aura_llvm_enum_drop_class(ptr %value) {
entry:
  %field = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 3, i64 0
  %raw = load i64, ptr %field
  %payload = inttoptr i64 %raw to ptr
  call void @aura_llvm_class_release(ptr %payload)
  ret void
}

define void @aura_llvm_enum_drop_enum(ptr %value) {
entry:
  %field = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 3, i64 0
  %raw = load i64, ptr %field
  %payload = inttoptr i64 %raw to ptr
  call void @aura_llvm_enum_release(ptr %payload)
  ret void
}

define void @aura_llvm_enum_drop_array(ptr %value) {
entry:
  %field = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 3, i64 0
  %raw = load i64, ptr %field
  %payload = inttoptr i64 %raw to ptr
  call void @aura_llvm_array_release(ptr %payload)
  ret void
}

"#;

pub(crate) const CLASS_RUNTIME: &str = r#"
%AuraLlvmClass = type { i64, [0 x i64] }

define ptr @aura_llvm_class_alloc(i64 %fields, i64 %type_id) {
entry:
  %field_bytes = mul i64 %fields, 8
  %size = add i64 %field_bytes, 8
  %value = call ptr @malloc(i64 %size)
  %refs = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 0
  %tag = shl i64 %type_id, 32
  %initial = or i64 %tag, 1
  store i64 %initial, ptr %refs
  ret ptr %value
}

define i64 @aura_llvm_class_type(ptr %value) {
entry:
  %refs = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 0
  %encoded = load i64, ptr %refs
  %type_id = lshr i64 %encoded, 32
  ret i64 %type_id
}

define void @aura_llvm_class_retain(ptr %value) {
entry:
  %is_null = icmp eq ptr %value, null
  br i1 %is_null, label %done, label %retain
retain:
  %refs = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 0
  %current = load i64, ptr %refs
  %count = and i64 %current, 4294967295
  %next_count = add i64 %count, 1
  %tag = and i64 %current, -4294967296
  %next = or i64 %tag, %next_count
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
  %count = and i64 %current, 4294967295
  %next_count = sub i64 %count, 1
  %tag = and i64 %current, -4294967296
  %next = or i64 %tag, %next_count
  store i64 %next, ptr %refs
  %last = icmp eq i64 %next_count, 0
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
    i64 4, label %release_array
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
release_array:
  %array = inttoptr i64 %raw to ptr
  call void @aura_llvm_array_release(ptr %array)
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
    i64 4, label %replace_array
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
replace_array:
  %old_array = inttoptr i64 %old to ptr
  call void @aura_llvm_array_release(ptr %old_array)
  %new_array = inttoptr i64 %raw to ptr
  call void @aura_llvm_array_retain(ptr %new_array)
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
@aura_llvm_executor_global = internal global ptr null

%AuraLlvmBlockingI64 = type { ptr, ptr }
%AuraLlvmImmediatePtr = type { ptr }

define void @aura_llvm_drop_immediate_class(ptr %frame, ptr %data, i64 %size) {
entry:
  %value = load ptr, ptr %data
  call void @aura_llvm_class_release(ptr %value)
  store ptr null, ptr %data
  ret void
}

define void @aura_llvm_drop_immediate_result(ptr %data, i64 %size) {
entry:
  %value = load ptr, ptr %data
  call void @aura_llvm_class_release(ptr %value)
  call void @free(ptr %data)
  ret void
}

define i32 @aura_llvm_poll_immediate_ptr(ptr %frame) {
entry:
  %data = call ptr @aura_task_frame_data(ptr %frame)
  %value = load ptr, ptr %data
  store ptr null, ptr %data
  call void @aura_llvm_task_set_ptr(ptr %frame, ptr %value, ptr @aura_llvm_drop_immediate_result)
  ret i32 2
}

define ptr @aura_llvm_task_immediate_ptr(ptr %value) {
entry:
  %executor = call ptr @aura_llvm_executor()
  %frame = call ptr @aura_task_frame_new(i64 8, ptr @aura_llvm_poll_immediate_ptr, ptr null)
  %data = call ptr @aura_task_frame_data(ptr %frame)
  store ptr %value, ptr %data
  call void @aura_task_frame_set_data_drop(ptr %frame, ptr @aura_llvm_drop_immediate_class)
  %submitted = call i32 @aura_task_executor_submit(ptr %executor, ptr %frame)
  ret ptr %frame
}

define i32 @aura_llvm_poll_immediate_i64(ptr %frame) {
entry:
  %data = call ptr @aura_task_frame_data(ptr %frame)
  %value = load i64, ptr %data
  call void @aura_llvm_task_set_i64(ptr %frame, i64 %value)
  ret i32 2
}

define ptr @aura_llvm_task_immediate_i64(i64 %value) {
entry:
  %executor = call ptr @aura_llvm_executor()
  %frame = call ptr @aura_task_frame_new(i64 8, ptr @aura_llvm_poll_immediate_i64, ptr null)
  %data = call ptr @aura_task_frame_data(ptr %frame)
  store i64 %value, ptr %data
  %submitted = call i32 @aura_task_executor_submit(ptr %executor, ptr %frame)
  ret ptr %frame
}

define void @aura_llvm_blocking_i64(ptr %frame, ptr %environment) {
entry:
  %function_address = getelementptr %AuraLlvmBlockingI64, ptr %environment, i32 0, i32 1
  %function = load ptr, ptr %function_address
  %closure_address = getelementptr %AuraLlvmBlockingI64, ptr %environment, i32 0, i32 0
  %closure = load ptr, ptr %closure_address
  %value = call i64 %function(ptr %closure)
  call void @aura_llvm_task_set_i64(ptr %frame, i64 %value)
  ret void
}

define void @aura_llvm_blocking_i64_destroy(ptr %environment) {
entry:
  %closure_address = getelementptr %AuraLlvmBlockingI64, ptr %environment, i32 0, i32 0
  %closure = load ptr, ptr %closure_address
  call void @aura_fun_env_free(ptr %closure)
  call void @free(ptr %environment)
  ret void
}

define ptr @aura_llvm_spawn_blocking_i64(%AuraLlvmFun %body) {
entry:
  %environment = call ptr @malloc(i64 16)
  %closure = extractvalue %AuraLlvmFun %body, 0
  %function = extractvalue %AuraLlvmFun %body, 1
  %closure_address = getelementptr %AuraLlvmBlockingI64, ptr %environment, i32 0, i32 0
  store ptr %closure, ptr %closure_address
  %function_address = getelementptr %AuraLlvmBlockingI64, ptr %environment, i32 0, i32 1
  store ptr %function, ptr %function_address
  %executor = call ptr @aura_llvm_executor()
  %frame = call ptr @aura_task_frame_new_blocking(ptr %executor, ptr @aura_llvm_blocking_i64, ptr %environment, ptr @aura_llvm_blocking_i64_destroy)
  ret ptr %frame
}

define ptr @aura_llvm_executor() {
entry:
  %current = load ptr, ptr @aura_llvm_executor_global
  %missing = icmp eq ptr %current, null
  br i1 %missing, label %create, label %done
create:
  %created = call ptr @aura_task_executor_new()
  store ptr %created, ptr @aura_llvm_executor_global
  br label %done
done:
  %result = phi ptr [%current, %entry], [%created, %create]
  ret ptr %result
}

define void @aura_llvm_task_set_i64(ptr %frame, i64 %value) {
entry:
  %data = call ptr @malloc(i64 8)
  store i64 %value, ptr %data
  call void @aura_task_frame_set_result(ptr %frame, ptr %data, i64 8, ptr null)
  ret void
}

define void @aura_llvm_task_set_ptr(ptr %frame, ptr %value, ptr %destroy) {
entry:
  %data = call ptr @malloc(i64 8)
  store ptr %value, ptr %data
  call void @aura_task_frame_set_result(ptr %frame, ptr %data, i64 8, ptr %destroy)
  ret void
}

define ptr @aura_llvm_args() {
entry:
  %count = call i64 @aura_args_count()
  %array = call ptr @aura_llvm_array_alloc(i64 %count, i64 1)
  br label %loop
loop:
  %index = phi i64 [0, %entry], [%next, %body]
  %done = icmp uge i64 %index, %count
  br i1 %done, label %finish, label %body
body:
  %raw = call ptr @aura_args_get(i64 %index)
  %value = call ptr @aura_llvm_str_new(ptr %raw)
  %encoded = ptrtoint ptr %value to i64
  call void @aura_llvm_array_set(ptr %array, i64 %index, i64 %encoded)
  call void @aura_llvm_str_release(ptr %value)
  %next = add i64 %index, 1
  br label %loop
finish:
  ret ptr %array
}

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
