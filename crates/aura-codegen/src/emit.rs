//! Top-level C translation unit emission.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use aura_ast::*;
use aura_ir::{CheckedIr, GenericOwnerKind, LoweredProgram};
use aura_sema::{CheckedFile, Ty};

use crate::array_emit::{emit_array_mono, is_array_mono, is_array_type_key};
use crate::async_compat::{AsyncCfgNode, AsyncFrameField, AsyncStateMachine};
use crate::class_emit::*;
use crate::ctx::{EmitCtx, EmitOptions};
use crate::enum_emit::*;
use crate::expr::{
    async_inner_key, bounded_capture_box_kind, bounded_spawn_await_shape, bounded_spawn_captures,
    bounded_spawn_data_name, bounded_spawn_destroy_name_with_suffix,
    bounded_spawn_discard_await_shape, bounded_spawn_gc_mark_name, bounded_spawn_layout_suffix,
    bounded_spawn_poll_name_with_suffix, coerce_expr, emit_expr, full_type_mono,
    general_spawn_captures, infer_type_name, mono_base_name, mono_split,
    mutable_spawn_capture_names, owned_string_copy_expr, BoundedSpawnCapture,
};
use crate::iface::*;
use crate::names::*;
use crate::stmt::{emit_block, emit_return_fallback};

pub fn emit_c(checked: &CheckedFile) -> String {
    emit_c_with(checked, EmitOptions::default())
}

fn emit_iface_ownership_prototypes(
    out: &mut String,
    iface: &InterfaceDecl,
    checked: &CheckedFile,
    args: &[Ty],
) {
    let mono = iface_mono_args(iface, checked, args);
    let cty = c_iface_type(&mono);
    let _ = writeln!(out, "{cty} {cty}_clone(const {cty} *source);");
    let _ = writeln!(out, "void {cty}_drop({cty} *value);");
    let _ = writeln!(out, "void {cty}_mark(const {cty} *value);");
}

fn emit_channel_value_drop_helpers(out: &mut String, checked: &CheckedFile) {
    let mut keys = std::collections::BTreeSet::new();
    for class in &checked.ast.classes {
        if class.type_params.is_empty() {
            keys.insert(type_mono(
                &class_decl_package(class, checked),
                &class.name.name,
                &[],
            ));
        }
    }
    for (name, args) in &checked.mono_classes {
        if name == "Array" {
            keys.insert(crate::names::mono_key(name, args));
        } else if let Some(class) = checked.ast.classes.iter().find(|c| c.name.name == *name) {
            if !args.is_empty() || class.type_params.is_empty() {
                keys.insert(type_mono(&class_decl_package(class, checked), name, args));
            }
        }
    }
    for enum_decl in &checked.ast.enums {
        if enum_decl.type_params.is_empty() {
            keys.insert(type_mono(
                &enum_decl_package(enum_decl, checked),
                &enum_decl.name.name,
                &[],
            ));
        }
    }
    for (name, args) in &checked.mono_enums {
        if let Some(enum_decl) = checked.ast.enums.iter().find(|e| e.name.name == *name) {
            if !args.is_empty() || enum_decl.type_params.is_empty() {
                keys.insert(type_mono(
                    &enum_decl_package(enum_decl, checked),
                    name,
                    args,
                ));
            }
        }
    }
    for iface in &checked.ast.interfaces {
        if iface.type_params.is_empty() {
            keys.insert(iface_mono(iface, checked));
        }
    }
    for (name, args) in &checked.mono_interfaces {
        if let Some(iface) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            if !args.is_empty() || iface.type_params.is_empty() {
                keys.insert(iface_mono_args(iface, checked, args));
            }
        }
    }
    for ty in checked.lambda_tys.values().chain(checked.expr_tys.values()) {
        if is_fun_type_key(&ty.mono_suffix()) {
            keys.insert(ty.mono_suffix());
        }
    }

    for key in keys {
        if matches!(key.as_str(), "Int" | "Bool" | "String" | "Unit")
            || key.starts_with("Task")
            || key.starts_with("TaskHandle")
            || key.starts_with("Channel")
            || key.starts_with("ForeignHandle")
            || is_heap_class_mono(&key, checked)
        {
            continue;
        }
        let cty = crate::stmt::local_key_to_c(&key, checked);
        let callback = crate::names::c_channel_drop_name(&key);
        let drop = if cty == "AuraTypeErasedValue" {
            "aura_type_erased_drop".to_owned()
        } else {
            format!("{cty}_drop")
        };
        let _ = writeln!(out, "static void {callback}(void *data, size_t size) {{");
        out.push_str("  (void)size;\n");
        if is_fun_type_key(&key) {
            let _ = writeln!(
                out,
                "  if (data != NULL) {{ {cty} *value = ({cty} *)data; if (value->env != NULL) aura_fun_env_free(value->env); free(value); }}"
            );
        } else {
            let _ = writeln!(
                out,
                "  if (data != NULL) {{ {cty} *value = ({cty} *)data; {drop}(value); free(value); }}"
            );
        }
        out.push_str("}\n\n");
    }
}

/// Interfaces are tagged value unions. Heap implementors remain GC pointers,
/// while struct implementors use their generated value ownership hooks.
fn emit_iface_ownership_hooks(
    out: &mut String,
    iface: &InterfaceDecl,
    checked: &CheckedFile,
    args: &[Ty],
) {
    let mono = iface_mono_args(iface, checked, args);
    let cty = c_iface_type(&mono);
    let impls = crate::iface::mono_implementors_for_iface(checked, iface, args);

    let _ = writeln!(out, "{cty} {cty}_clone(const {cty} *source) {{");
    let _ = writeln!(
        out,
        "  {cty} copy = source == NULL ? ({cty}){{0}} : *source;"
    );
    out.push_str("  if (source == NULL) return copy;\n  switch (source->tag) {\n");
    for imp in &impls {
        let class_mono = type_mono(
            &class_decl_package(imp.class, checked),
            &imp.class.name.name,
            &imp.class_args,
        );
        let _ = writeln!(out, "    case AURA_TAG_{class_mono}:");
        if !is_heap_class_decl(imp.class) {
            let class_cty = c_class_type(&class_mono);
            let _ = writeln!(
                out,
                "      copy.data.as_{class_mono} = {class_cty}_clone(&source->data.as_{class_mono}); break;"
            );
        } else {
            out.push_str("      break;\n");
        }
    }
    out.push_str("    default: break;\n  }\n  return copy;\n}\n\n");

    let _ = writeln!(out, "void {cty}_drop({cty} *value) {{");
    out.push_str("  if (value == NULL) return;\n  switch (value->tag) {\n");
    for imp in &impls {
        let class_mono = type_mono(
            &class_decl_package(imp.class, checked),
            &imp.class.name.name,
            &imp.class_args,
        );
        if !is_heap_class_decl(imp.class) {
            let class_cty = c_class_type(&class_mono);
            let _ = writeln!(out, "    case AURA_TAG_{class_mono}: {class_cty}_drop(&value->data.as_{class_mono}); break;");
        } else {
            let _ = writeln!(out, "    case AURA_TAG_{class_mono}: break;");
        }
    }
    out.push_str("    default: break;\n  }\n}\n\n");

    let _ = writeln!(out, "void {cty}_mark(const {cty} *value) {{");
    out.push_str("  if (value == NULL) return;\n  switch (value->tag) {\n");
    for imp in &impls {
        let class_mono = type_mono(
            &class_decl_package(imp.class, checked),
            &imp.class.name.name,
            &imp.class_args,
        );
        if is_heap_class_decl(imp.class) {
            let _ = writeln!(out, "    case AURA_TAG_{class_mono}: aura_gc_mark_ptr((void *)value->data.as_{class_mono}); break;");
        } else {
            let class_cty = c_class_type(&class_mono);
            let _ = writeln!(out, "    case AURA_TAG_{class_mono}: {class_cty}_mark(&value->data.as_{class_mono}); break;");
        }
    }
    out.push_str("    default: break;\n  }\n}\n\n");
}

/// Emit C with options (normal binary or test harness).
pub fn emit_c_with(checked: &CheckedFile, opts: EmitOptions) -> String {
    let program = LoweredProgram::from_checked(checked.clone());
    emit_c_with_program(&program, opts)
}

/// C backend entrypoint receiving the complete checked/lowered program.
pub fn emit_c_with_program(program: &LoweredProgram, opts: EmitOptions) -> String {
    emit_c_impl(program.alpha_c_source(), Some(program.checked()), opts)
}

fn ir_generic_functions(ir: Option<&CheckedIr>, kind: GenericOwnerKind) -> Vec<(String, Vec<Ty>)> {
    let functions = ir
        .into_iter()
        .flat_map(|checked| checked.generic_instantiations.iter())
        .filter(|item| item.kind == kind)
        .map(|item| (item.owner.clone(), item.args.clone()))
        .collect::<Vec<_>>();
    functions
}

fn emit_c_impl(checked: &CheckedFile, ir: Option<&CheckedIr>, opts: EmitOptions) -> String {
    let generic_functions = ir_generic_functions(ir, GenericOwnerKind::Function);
    let generic_async_functions = ir_generic_functions(ir, GenericOwnerKind::AsyncFunction);
    let mut out = String::new();
    out.push_str("/* generated by aura-codegen (C3 C backend) */\n");
    out.push_str(
        "/* alpha backend: source compatibility lowering may be used only for unsupported MIR capabilities */\n",
    );
    if let Some(ir) = ir {
        let mut unsupported = ir
            .async_mir_unlowered
            .iter()
            .chain(ir.open_generic_async_mir_unlowered.iter())
            .chain(ir.function_mir_unlowered.iter())
            .chain(ir.generic_function_mir_unlowered.iter())
            .chain(ir.generic_async_mir_unlowered.iter())
            .chain(ir.generic_async_method_mir_unlowered.iter())
            .chain(ir.generic_method_mir_unlowered.iter())
            .cloned()
            .collect::<Vec<_>>();
        unsupported.sort();
        unsupported.dedup();
        if !unsupported.is_empty() {
            let _ = writeln!(
                out,
                "/* alpha compatibility fallback symbols: {} */",
                unsupported.join(", ")
            );
        }
    }
    out.push_str("#include <stdint.h>\n");
    out.push_str("#include <stdbool.h>\n");
    out.push_str("#include <stddef.h>\n");
    out.push_str("#include <stdlib.h>\n");
    out.push_str("#include <stdio.h>\n");
    out.push_str("#include <string.h>\n");
    out.push_str("#include <errno.h>\n");
    out.push_str("#include <setjmp.h>\n");
    emit_ffi_abi_declarations(&mut out);
    emit_metadata_abi(&mut out, checked);
    emit_foreign_prototypes(&mut out, checked);
    emit_fallback_unit_join_result(&mut out, checked);
    out.push_str("void aura_print(const char *s);\n");
    out.push_str("void aura_println(const char *s);\n");
    out.push_str("void aura_eprint(const char *s);\n");
    out.push_str("void aura_eprintln(const char *s);\n");
    out.push_str("void aura_log(int level, const char *message);\n");
    out.push_str("int aura_log_set_min_level(int level);\n");
    out.push_str("int aura_log_get_min_level(void);\n");
    out.push_str("const char *aura_read_file(const char *path);\n");
    out.push_str("const char *aura_try_read_file(const char *path);\n");
    out.push_str("void aura_write_file(const char *path, const char *content);\n");
    out.push_str("_Bool aura_try_write_file(const char *path, const char *content);\n");
    out.push_str("void aura_append_file(const char *path, const char *content);\n");
    out.push_str("_Bool aura_file_exists(const char *path);\n");
    out.push_str("int64_t aura_file_size(const char *path);\n");
    out.push_str("int64_t aura_io_read_fd(int fd, void *buffer, uint64_t capacity);\n");
    out.push_str("int64_t aura_io_write_fd(int fd, const void *buffer, uint64_t length);\n");
    out.push_str("int64_t aura_args_count(void);\n");
    out.push_str("const char *aura_args_get(int64_t i);\n");
    out.push_str("const char *aura_read_line(void);\n");
    out.push_str("const char *aura_read_all_stdin(void);\n");
    out.push_str("void aura_exit(int64_t code);\n");
    out.push_str("int64_t aura_time_monotonic_millis(void);\n");
    out.push_str(
        "int aura_task_frame_set_cancel_deadline(AuraTaskFrame *frame, int timeout_ms);\n",
    );
    out.push_str(
        "int aura_task_frame_link_cancellation(AuraTaskFrame *parent, AuraTaskFrame *child);\n",
    );
    out.push_str("const char *aura_encoding_hex_encode(const char *value);\n");
    out.push_str("const char *aura_encoding_hex_decode(const char *value);\n");
    out.push_str("const char *aura_encoding_base64_encode(const char *value);\n");
    out.push_str("const char *aura_encoding_base64_decode(const char *value);\n");
    out.push_str("const char *aura_encoding_percent_encode(const char *value);\n");
    out.push_str("const char *aura_encoding_percent_decode(const char *value);\n");
    out.push_str("_Bool aura_encoding_is_valid_utf8(const char *value);\n");
    out.push_str("_Bool aura_url_is_origin_form(const char *target);\n");
    out.push_str("const char *aura_url_path(const char *target);\n");
    out.push_str("const char *aura_url_normalize_path(const char *path);\n");
    out.push_str("const char *aura_url_query(const char *target);\n");
    out.push_str("_Bool aura_url_is_absolute(const char *target);\n");
    out.push_str("const char *aura_url_authority(const char *target);\n");
    out.push_str("const char *aura_url_authority_host(const char *target);\n");
    out.push_str("const char *aura_url_authority_port(const char *target);\n");
    out.push_str("const char *aura_url_query_value(const char *target, const char *key);\n");
    out.push_str("_Bool aura_mime_is_valid_type(const char *value);\n");
    out.push_str("const char *aura_mime_sanitize_filename(const char *value);\n");
    out.push_str("const char *aura_mime_disposition_filename(const char *value);\n");
    out.push_str("const char *aura_dns_resolve_host(const char *host, int prefer_ipv6);\n");
    out.push_str("const char *aura_dns_resolve_host_list(const char *host, int prefer_ipv6);\n");
    out.push_str("_Bool aura_json_is_valid(const char *value);\n");
    out.push_str("int64_t aura_json_error_offset(const char *value);\n");
    out.push_str("const char *aura_json_escape_string(const char *value);\n");
    out.push_str("const char *aura_json_object_get(const char *value, const char *key);\n");
    out.push_str("const char *aura_json_array_at(const char *value, int64_t index);\n");
    out.push_str("int64_t aura_json_array_count(const char *value);\n");
    out.push_str("const char *aura_json_object_keys(const char *value);\n");
    out.push_str("const char *aura_json_decode_string(const char *value);\n");
    out.push_str("const char *aura_json_duplicate_key(const char *value);\n");
    out.push_str("_Bool aura_json_parse_int(const char *value, int64_t *out);\n");
    out.push_str("_Bool aura_json_parse_bool(const char *value, _Bool *out);\n");
    out.push_str("int aura_signal_install_shutdown(void);\n");
    out.push_str("_Bool aura_signal_shutdown_requested(void);\n");
    out.push_str("void aura_signal_clear_shutdown(void);\n");
    out.push_str("int64_t aura_error_kind_code(int64_t code);\n");
    out.push_str("const char *aura_bytes_copy(const char *value);\n");
    out.push_str("const char *aura_bytes_concat(const char *left, const char *right);\n");
    out.push_str(
        "const char *aura_bytes_slice(const char *value, int64_t start, int64_t length);\n",
    );
    out.push_str("_Bool aura_bytes_equals(const char *left, const char *right);\n");
    out.push_str("const char *aura_crypto_random_bytes(int64_t length);\n");
    out.push_str("const char *aura_crypto_sha256(const char *value);\n");
    out.push_str("const char *aura_crypto_hmac_sha256(const char *key, const char *value);\n");
    out.push_str("_Bool aura_crypto_constant_time_equals(const char *left, const char *right);\n");
    out.push_str(
        "const char *aura_compress_text(const char *value, int64_t codec, int64_t level);\n",
    );
    out.push_str("const char *aura_decompress_text(const char *value, int64_t codec);\n");
    out.push_str("const char *aura_fs_join(const char *base, const char *child);\n");
    out.push_str("const char *aura_fs_basename(const char *path);\n");
    out.push_str("const char *aura_fs_dirname(const char *path);\n");
    out.push_str("const char *aura_fs_extension(const char *path);\n");
    out.push_str("_Bool aura_fs_is_absolute(const char *path);\n");
    out.push_str("_Bool aura_fs_is_directory(const char *path);\n");
    out.push_str("int64_t aura_fs_file_mode(const char *path);\n");
    out.push_str("int64_t aura_fs_permissions(const char *path);\n");
    out.push_str("int64_t aura_fs_modified_millis(const char *path);\n");
    out.push_str("const char *aura_fs_list_names(const char *path);\n");
    out.push_str("_Bool aura_fs_is_symlink(const char *path);\n");
    out.push_str("const char *aura_os_get_env(const char *name);\n");
    out.push_str("_Bool aura_os_set_env(const char *name, const char *value);\n");
    out.push_str("_Bool aura_os_unset_env(const char *name);\n");
    out.push_str("const char *aura_os_cwd(void);\n");
    out.push_str("int64_t aura_os_pid(void);\n");
    out.push_str("const char *aura_os_platform(void);\n");
    out.push_str("void aura_assert(_Bool cond);\n");
    out.push_str("void aura_assert_eq_int(int64_t a, int64_t b);\n");
    out.push_str("void aura_assert_eq_string(const char *a, const char *b);\n");
    out.push_str("void aura_assert_eq_bool(_Bool a, _Bool b);\n");
    out.push_str("void aura_try_enter(jmp_buf *buf);\n");
    out.push_str("void aura_try_leave(void);\n");
    out.push_str("void aura_ex_set_source_span(uint32_t start, uint32_t end);\n");
    out.push_str("uint32_t aura_ex_source_span_start(void);\n");
    out.push_str("uint32_t aura_ex_source_span_end(void);\n");
    out.push_str("int aura_ex_add_cause(const char *type_name, uint32_t source_span_start, uint32_t source_span_end);\n");
    out.push_str("size_t aura_ex_cause_count(void);\n");
    out.push_str("const char *aura_ex_cause_type(size_t index);\n");
    out.push_str("uint32_t aura_ex_cause_span_start(size_t index);\n");
    out.push_str("uint32_t aura_ex_cause_span_end(size_t index);\n");
    out.push_str("const char *aura_ex_cause_type_copy(size_t index);\n");
    out.push_str("void aura_throw_string(const char *s);\n");
    out.push_str("void aura_throw_int(int64_t v);\n");
    out.push_str("void aura_throw_bool(_Bool v);\n");
    out.push_str("void aura_throw_obj(const char *type_name, void *obj);\n");
    out.push_str("void aura_throw_obj_with_destructor(const char *type_name, void *obj, void (*destroy_obj)(void *));\n");
    out.push_str("int aura_ex_matches(const char *type_name);\n");
    out.push_str("const char *aura_ex_type_name(void);\n");
    out.push_str("const char *aura_ex_as_string(void);\n");
    out.push_str("int64_t aura_ex_as_int(void);\n");
    out.push_str("_Bool aura_ex_as_bool(void);\n");
    out.push_str("void *aura_ex_as_obj(void);\n");
    out.push_str("void *aura_ex_take_obj(void);\n");
    out.push_str("void aura_ex_clear(void);\n");
    out.push_str("void aura_ex_rethrow(void);\n");
    out.push_str("void *aura_gc_alloc(size_t size);\n");
    out.push_str(
        "void *aura_gc_alloc_full(size_t size, void (*dtor)(void *), void (*mark_extras)(void *));\n",
    );
    out.push_str("void aura_gc_mark_ptr(void *obj);\n");
    out.push_str("void aura_gc_add_root(void **slot);\n");
    out.push_str("void aura_gc_remove_root(void **slot);\n");
    out.push_str("void aura_gc_add_array_root(void **data_slot, int64_t *len_slot);\n");
    out.push_str("void aura_gc_add_array_root_typed(void **data_slot, int64_t *len_slot, void (*mark)(const void *, int64_t));\n");
    out.push_str("void aura_gc_remove_array_root(void **data_slot);\n");
    out.push_str("void aura_fun_env_retain(void *env);\n");
    out.push_str("void aura_fun_env_free(void *env);\n");
    out.push_str("void aura_gc_collect(void);\n");
    out.push_str("void aura_gc_shutdown(void);\n");
    out.push_str("uint32_t aura_runtime_abi_version(void);\n");
    out.push_str(
        "int aura_runtime_check_abi(uint32_t expected_version, const char *expected_identity);\n",
    );
    out.push_str("const char *aura_runtime_abi_identity(void);\n");
    let abi_id = crate::runtime_abi::ID
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let _ = writeln!(
        out,
        "#define AURA_GENERATED_ABI_VERSION {}u",
        crate::runtime_abi::VERSION
    );
    let _ = writeln!(out, "#define AURA_GENERATED_ABI_ID \"{abi_id}\"");
    // C22 runtime ABI.  The definitions live in runtime/runtime.c; generated
    // translation units only need the stable opaque declarations below.
    out.push_str("typedef struct AuraTaskFrame AuraTaskFrame;\n");
    out.push_str("typedef struct AuraTaskExecutor AuraTaskExecutor;\n");
    out.push_str("void aura_gc_collect_executor(AuraTaskExecutor *executor);\n");
    out.push_str("typedef struct AuraTaskScope AuraTaskScope;\n");
    out.push_str("typedef struct AuraLazyCell AuraLazyCell;\n");
    out.push_str("typedef void (*AuraLazyInitFn)(AuraLazyCell *, void *);\n");
    out.push_str("typedef void (*AuraLazyValueDestroyFn)(void *);\n");
    out.push_str("typedef void (*AuraTaskBlockingFn)(AuraTaskFrame *, void *);\n");
    out.push_str("typedef void (*AuraTaskBlockingEnvDestroyFn)(void *);\n");
    out.push_str("typedef struct AuraTaskChannel AuraTaskChannel;\n");
    out.push_str("typedef struct AuraTaskSelect AuraTaskSelect;\n");
    out.push_str("typedef struct AuraFile AuraFile;\n");
    out.push_str("typedef enum { AURA_FILE_OK = 0, AURA_FILE_PENDING = 1, AURA_FILE_EOF = 2, AURA_FILE_ERROR = -1, AURA_FILE_CLOSED = -2, AURA_FILE_UNSUPPORTED = -3, AURA_FILE_PERMISSION = -4 } AuraFileStatus;\n");
    out.push_str("typedef enum { AURA_FILE_READ = 0, AURA_FILE_WRITE = 1, AURA_FILE_READ_WRITE = 2, AURA_FILE_APPEND = 3 } AuraFileMode;\n");
    out.push_str("AuraFileStatus aura_file_open(const char *, AuraFileMode, AuraFile **);\n");
    out.push_str("const char *aura_file_last_error(void);\n");
    out.push_str("AuraFileStatus aura_file_destroy(AuraFile **);\n");
    out.push_str("AuraFileStatus aura_file_read(AuraFile *, void *, uint64_t, uint64_t *);\n");
    out.push_str(
        "AuraFileStatus aura_file_write(AuraFile *, const void *, uint64_t, uint64_t *);\n",
    );
    out.push_str(
        "AuraFfiStatus aura_ffi_handle_new(void *, void (*)(void *), AuraFfiOpaqueHandle **);\n",
    );
    out.push_str("static void aura_destroy_file_resource(void *resource) { if (resource != NULL) { AuraFile *__file = (AuraFile *)resource; (void)aura_file_destroy(&__file); } }\n");
    out.push_str("typedef struct AuraTcpListener AuraTcpListener;\n");
    out.push_str("typedef struct AuraTcpStream AuraTcpStream;\n");
    out.push_str("typedef enum { AURA_TCP_OK = 0, AURA_TCP_PENDING = 1, AURA_TCP_EOF = 2, AURA_TCP_TIMEOUT = 3, AURA_TCP_ERROR = -1, AURA_TCP_CLOSED = -2, AURA_TCP_UNSUPPORTED = -3 } AuraTcpStatus;\n");
    out.push_str("AuraTcpStatus aura_tcp_listener_bind_endpoint(const char *, uint16_t *, AuraTcpListener **);\n");
    out.push_str(
        "AuraTcpStatus aura_tcp_listener_accept(AuraTcpListener *, int, AuraTcpStream **);\n",
    );
    out.push_str(
        "AuraTcpStatus aura_tcp_stream_connect_endpoint(const char *, int, AuraTcpStream **);\n",
    );
    out.push_str("int aura_tcp_listener_close(AuraTcpListener *);\n");
    out.push_str("int aura_tcp_stream_close(AuraTcpStream *);\n");
    out.push_str("void aura_tcp_listener_destroy(AuraTcpListener *);\n");
    out.push_str("void aura_tcp_stream_destroy(AuraTcpStream *);\n");
    out.push_str(
        "AuraTcpStatus aura_tcp_stream_read(AuraTcpStream *, void *, size_t, size_t *, int);\n",
    );
    out.push_str("AuraTcpStatus aura_tcp_stream_write(AuraTcpStream *, const void *, size_t, size_t *, int);\n");
    out.push_str("static void aura_destroy_tcp_stream_resource(void *resource) { if (resource != NULL) aura_tcp_stream_destroy((AuraTcpStream *)resource); }\n");
    out.push_str("static void aura_destroy_tcp_listener_resource(void *resource) { if (resource != NULL) aura_tcp_listener_destroy((AuraTcpListener *)resource); }\n");
    out.push_str("int aura_udp_bind(const char *, int64_t);\n");
    out.push_str("int aura_udp_wait(const char *, int64_t, int);\n");
    out.push_str(
        "const char *aura_udp_receive(const char *, int64_t, int64_t, int64_t *, const char **);\n",
    );
    out.push_str(
        "int64_t aura_udp_send(const char *, int64_t, const char *, int64_t, const char *);\n",
    );
    out.push_str("int aura_udp_close(const char *, int64_t);\n");
    out.push_str("typedef struct AuraHttpRequest AuraHttpRequest;\n");
    out.push_str("typedef struct AuraHttpResponse AuraHttpResponse;\n");
    out.push_str("typedef struct AuraHttpConnection AuraHttpConnection;\n");
    out.push_str("typedef enum { AURA_HTTP_CONNECTION_OK = 0, AURA_HTTP_CONNECTION_CLOSED = 1, AURA_HTTP_CONNECTION_TIMEOUT = 2, AURA_HTTP_CONNECTION_DISCONNECTED = 3, AURA_HTTP_CONNECTION_SHUTDOWN = 4, AURA_HTTP_CONNECTION_LIMIT = 5, AURA_HTTP_CONNECTION_ERROR = -1, AURA_HTTP_CONNECTION_UNSUPPORTED = -2 } AuraHttpConnectionStatus;\n");
    out.push_str("const char *aura_http_request_method(const AuraHttpRequest *);\n");
    out.push_str("const char *aura_http_request_target(const AuraHttpRequest *);\n");
    out.push_str("const char *aura_http_request_version(const AuraHttpRequest *);\n");
    out.push_str("size_t aura_http_request_header_count(const AuraHttpRequest *);\n");
    out.push_str("const char *aura_http_request_header_name(const AuraHttpRequest *, size_t);\n");
    out.push_str("const char *aura_http_request_header_value(const AuraHttpRequest *, size_t);\n");
    out.push_str("const unsigned char *aura_http_request_body(const AuraHttpRequest *);\n");
    out.push_str("size_t aura_http_request_body_length(const AuraHttpRequest *);\n");
    out.push_str("int aura_http_request_read_body(const AuraHttpRequest *, unsigned char *, size_t, size_t *);\n");
    out.push_str("int aura_http_request_body_read_begin(const AuraHttpRequest *);\n");
    out.push_str("void aura_http_request_body_read_end(const AuraHttpRequest *);\n");
    out.push_str("int aura_http_request_wait_body(AuraTaskFrame *, const AuraHttpRequest *);\n");
    out.push_str("int aura_http_response_status(const AuraHttpResponse *);\n");
    out.push_str("int aura_http_response_keep_alive(const AuraHttpResponse *);\n");
    out.push_str("int aura_http_response_stream_started(const AuraHttpResponse *);\n");
    out.push_str("typedef enum { AURA_HTTP_RESPONSE_OK = 0, AURA_HTTP_RESPONSE_INVALID = -1, AURA_HTTP_RESPONSE_TOO_LARGE = -2, AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL = -3, AURA_HTTP_RESPONSE_ALLOCATION = -4 } AuraHttpResponseStatus;\n");
    out.push_str("typedef enum { AURA_HTTP_RESPONSE_CLOSE = 0, AURA_HTTP_RESPONSE_KEEP_ALIVE = 1 } AuraHttpResponseConnection;\n");
    out.push_str(
        "AuraHttpResponseStatus aura_http_response_set_status(AuraHttpResponse *, int);\n",
    );
    out.push_str("AuraHttpResponseStatus aura_http_response_set_connection(AuraHttpResponse *, AuraHttpResponseConnection);\n");
    out.push_str("AuraHttpResponseStatus aura_http_response_set_body(AuraHttpResponse *, const void *, size_t);\n");
    out.push_str("AuraHttpResponseStatus aura_http_response_add_header(AuraHttpResponse *, const char *, const char *);\n");
    out.push_str(
        "int aura_http_response_stream_begin(AuraHttpResponse *, void *, size_t, size_t *);\n",
    );
    out.push_str(
        "int aura_http_response_stream_chunk(const void *, size_t, void *, size_t, size_t *);\n",
    );
    out.push_str("int aura_http_response_stream_finish(const AuraHttpResponse *, void *, size_t, size_t *);\n");
    out.push_str("int aura_http_connection_stream_write(AuraHttpConnection *, const void *, size_t, size_t *);\n");
    out.push_str(
        "int aura_http_connection_wait_write(AuraTaskFrame *, const AuraHttpConnection *);\n",
    );
    out.push_str("AuraHttpConnectionStatus aura_http_connection_create_from_stream(AuraTcpStream *, const void *, AuraHttpConnection **);\n");
    out.push_str("void aura_http_connection_destroy_resource(void *);\n");
    out.push_str("AuraFfiStatus aura_ffi_handle_take_owned(AuraFfiOpaqueHandle **, void **);\n");
    out.push_str("static char *aura_http_copy_bytes(const void *data, size_t length) { char *copy = (char *)malloc(length + 1); if (copy == NULL) return NULL; if (length != 0 && data != NULL) memcpy(copy, data, length); copy[length] = '\\0'; return copy; }\n");
    out.push_str("static int aura_http_pin_resource(AuraFfiOpaqueHandle *handle, AuraFfiHandlePin *pin) { return handle != NULL && aura_ffi_handle_pin_for_boundary(handle, AURA_FFI_BOUNDARY_SYNC, pin) == AURA_FFI_OK; }\n");
    out.push_str("typedef struct AuraRaceTracker AuraRaceTracker;\n");
    out.push_str("typedef enum { AURA_RACE_READ = 0, AURA_RACE_WRITE = 1, AURA_RACE_TASK_SPAWN = 2, AURA_RACE_TASK_JOIN = 3, AURA_RACE_SYNC_ACQUIRE = 4, AURA_RACE_SYNC_RELEASE = 5, AURA_RACE_TASK_COMPLETE = 6, AURA_RACE_TASK_FAILED = 7, AURA_RACE_TASK_CANCELLED = 8, AURA_RACE_CHANNEL_SEND = 9, AURA_RACE_CHANNEL_RECEIVE = 10, AURA_RACE_CHANNEL_CLOSE = 11 } AuraRaceEventKind;\n");
    out.push_str("AuraRaceTracker *aura_race_tracker_new(void);\n");
    out.push_str("void aura_race_tracker_destroy(AuraRaceTracker *tracker);\n");
    out.push_str("void aura_race_tracker_set_active(AuraRaceTracker *tracker);\n");
    out.push_str("void aura_race_set_source_id(uint32_t source_id);\n");
    out.push_str("void aura_race_record_access(uintptr_t address, uint32_t source_id, AuraRaceEventKind kind);\n");
    out.push_str("typedef void (*AuraTaskChannelValueDestroyFn)(void *data, size_t size);\n");
    out.push_str("typedef struct { void *data; size_t size; AuraTaskChannelValueDestroyFn destroy; } AuraTaskChannelValue;\n");
    out.push_str("typedef enum { AURA_CHANNEL_OK = 0, AURA_CHANNEL_PENDING = 1, AURA_CHANNEL_CLOSED = 2, AURA_CHANNEL_ERROR = 3 } AuraTaskChannelStatus;\n");
    out.push_str("typedef enum { AURA_TASK_READY = 0, AURA_TASK_PENDING = 1, AURA_TASK_COMPLETE = 2, AURA_TASK_FAILED = 3, AURA_TASK_CANCELLED = 4 } AuraTaskPollState;\n");
    out.push_str("AuraTaskPollState aura_http_connection_poll_async_task_handle(AuraTaskFrame *, AuraFfiOpaqueHandle *, AuraTaskPollState (*)(AuraTaskFrame *, const AuraHttpRequest *, AuraHttpResponse *, void *), void *);\n");
    out.push_str("typedef struct { void *data; size_t size; } AuraTaskResult;\n");
    out.push_str("typedef struct { AuraTaskPollState state; AuraTaskResult result; AuraTaskResult error; } AuraTaskOutcome;\n");
    out.push_str("typedef AuraTaskPollState (*AuraTaskPollFn)(AuraTaskFrame *frame);\n");
    out.push_str("typedef AuraTaskPollState (*AuraTaskCancelFn)(AuraTaskFrame *frame);\n");
    out.push_str("typedef void (*AuraTaskFrameDestroyFn)(AuraTaskFrame *frame);\n");
    out.push_str("typedef void (*AuraTaskFrameGcMarkFn)(AuraTaskFrame *frame);\n");
    out.push_str(
        "typedef void (*AuraTaskFrameDataDropFn)(AuraTaskFrame *frame, void *data, size_t size);\n",
    );
    out.push_str("typedef void (*AuraTaskResultDestroyFn)(void *data, size_t size);\n");
    out.push_str("typedef void *(*AuraTaskResultCloneFn)(const void *data, size_t size, size_t *cloned_size);\n");
    out.push_str("AuraTaskPollState aura_task_poll_unit(AuraTaskFrame *frame);\n");
    out.push_str("AuraTaskFrame *aura_task_frame_new(size_t data_size, AuraTaskPollFn poll, AuraTaskFrameDestroyFn destroy);\n");
    out.push_str("AuraTaskFrame *aura_task_frame_new_blocking(AuraTaskExecutor *, AuraTaskBlockingFn, void *, AuraTaskBlockingEnvDestroyFn);\n");
    out.push_str("AuraTaskScope *aura_task_scope_begin(AuraTaskExecutor *);\n");
    out.push_str("int aura_task_scope_end(AuraTaskScope *);\n");
    out.push_str(
        "AuraLazyCell *aura_lazy_cell_new(AuraLazyInitFn, void *, AuraTaskBlockingEnvDestroyFn);\n",
    );
    out.push_str(
        "void aura_lazy_cell_publish(AuraLazyCell *, void *, size_t, AuraLazyValueDestroyFn);\n",
    );
    out.push_str("void *aura_lazy_cell_value(AuraLazyCell *);\n");
    out.push_str("int aura_lazy_cell_is_initialized(AuraLazyCell *);\n");
    out.push_str("void aura_lazy_cell_destroy(AuraLazyCell *);\n");
    out.push_str(
        "void aura_task_frame_set_cancel_handler(AuraTaskFrame *frame, AuraTaskCancelFn cancel);\n",
    );
    out.push_str(
        "void aura_task_frame_set_gc_mark(AuraTaskFrame *frame, AuraTaskFrameGcMarkFn mark);\n",
    );
    out.push_str(
        "void aura_task_frame_set_data_drop(AuraTaskFrame *frame, AuraTaskFrameDataDropFn drop);\n",
    );
    out.push_str("void *aura_task_frame_data(AuraTaskFrame *frame);\n");
    out.push_str("uint64_t aura_task_frame_task_id(const AuraTaskFrame *frame);\n");
    out.push_str(
        "void aura_task_frame_set_race_source_id(AuraTaskFrame *frame, uint32_t source_id);\n",
    );
    out.push_str("uint32_t aura_task_frame_error_source_id(const AuraTaskFrame *frame);\n");
    out.push_str("uint32_t aura_task_frame_error_span_start(const AuraTaskFrame *frame);\n");
    out.push_str("uint32_t aura_task_frame_error_span_end(const AuraTaskFrame *frame);\n");
    out.push_str("const char *aura_task_frame_error_type_name(const AuraTaskFrame *frame);\n");
    out.push_str("AuraTaskResult aura_task_frame_error(const AuraTaskFrame *frame);\n");
    out.push_str(
        "int aura_task_frame_propagate_error(AuraTaskFrame *frame, const AuraTaskFrame *source);\n",
    );
    out.push_str("AuraTaskPollState aura_task_frame_propagate_outcome(AuraTaskFrame *frame, const AuraTaskFrame *source, AuraTaskResultCloneFn result_clone, AuraTaskResultDestroyFn result_destroy);\n");
    out.push_str("void aura_task_frame_set_error_span(AuraTaskFrame *frame, void *data, size_t size, AuraTaskResultDestroyFn destroy, uint32_t source_id, uint32_t span_start, uint32_t span_end);\n");
    out.push_str("void aura_task_frame_set_error_span_with_clone(AuraTaskFrame *frame, void *data, size_t size, AuraTaskResultCloneFn clone, AuraTaskResultDestroyFn destroy, uint32_t source_id, uint32_t span_start, uint32_t span_end);\n");
    out.push_str("void aura_task_frame_set_error_payload_with_clone(AuraTaskFrame *frame, void *data, size_t size, AuraTaskResultCloneFn clone, AuraTaskResultDestroyFn destroy);\n");
    out.push_str(
        "void aura_task_frame_set_error_type_name(AuraTaskFrame *frame, const char *type_name);\n",
    );
    out.push_str("AuraTaskResult aura_task_frame_error_payload(const AuraTaskFrame *frame);\n");
    out.push_str("void aura_task_frame_set_error_at(AuraTaskFrame *frame, void *data, size_t size, AuraTaskResultDestroyFn destroy, uint32_t source_id);\n");
    out.push_str("AuraTaskPollState aura_task_frame_poll_once(AuraTaskFrame *frame);\n");
    out.push_str("AuraTaskPollState aura_task_executor_poll_inline(AuraTaskExecutor *executor, AuraTaskFrame *frame);\n");
    out.push_str("void aura_task_frame_destroy(AuraTaskFrame *frame);\n");
    out.push_str("AuraTaskPollState aura_task_frame_state(const AuraTaskFrame *frame);\n");
    out.push_str("AuraTaskOutcome aura_task_executor_join_outcome(AuraTaskExecutor *executor, AuraTaskFrame *frame);\n");
    out.push_str("int aura_task_frame_cancel_requested(const AuraTaskFrame *frame);\n");
    out.push_str("int aura_task_frame_is_waiting(const AuraTaskFrame *frame);\n");
    out.push_str("void aura_task_frame_set_waiting(AuraTaskFrame *frame, void *token);\n");
    out.push_str("void aura_task_frame_clear_waiting(AuraTaskFrame *frame);\n");
    out.push_str("void *aura_task_frame_waiting_token(const AuraTaskFrame *frame);\n");
    out.push_str("int aura_task_frame_wait_fd(AuraTaskFrame *frame, int fd, short events);\n");
    out.push_str("int aura_task_frame_wait_deadline(AuraTaskFrame *frame, int timeout_ms);\n");
    out.push_str("int aura_task_frame_take_fd_wait_timeout(AuraTaskFrame *frame);\n");
    out.push_str("int aura_task_frame_wait_file(AuraTaskFrame *frame, const AuraFile *file, short events);\n");
    out.push_str("int aura_task_frame_wait_tcp_listener(AuraTaskFrame *frame, const AuraTcpListener *listener, short events);\n");
    out.push_str("int aura_task_frame_wait_tcp_stream(AuraTaskFrame *frame, const AuraTcpStream *stream, short events);\n");
    out.push_str("int aura_task_frame_wait_on(AuraTaskFrame *frame, AuraTaskFrame *target);\n");
    out.push_str(
        "int aura_task_executor_wake_waiting(AuraTaskExecutor *executor, AuraTaskFrame *frame);\n",
    );
    out.push_str(
        "int aura_task_executor_poll_waiting(AuraTaskExecutor *executor, int timeout_ms);\n",
    );
    out.push_str("uint32_t aura_task_frame_resume_state(const AuraTaskFrame *frame);\n");
    out.push_str("void aura_task_frame_set_resume_state(AuraTaskFrame *frame, uint32_t state);\n");
    out.push_str("void aura_task_frame_set_result(AuraTaskFrame *frame, void *data, size_t size, AuraTaskResultDestroyFn destroy);\n");
    out.push_str("AuraTaskResult aura_task_frame_result(const AuraTaskFrame *frame);\n");
    out.push_str("AuraTaskExecutor *aura_task_executor_new(void);\n");
    out.push_str("int aura_task_executor_set_max_live_tasks(AuraTaskExecutor *executor, size_t max_live_tasks);\n");
    out.push_str(
        "int aura_task_executor_start_workers(AuraTaskExecutor *executor, size_t worker_count);\n",
    );
    out.push_str("void aura_task_executor_stop_workers(AuraTaskExecutor *executor);\n");
    out.push_str("void aura_task_executor_set_race_tracker(AuraTaskExecutor *executor, AuraRaceTracker *tracker);\n");
    out.push_str(
        "int aura_task_executor_submit(AuraTaskExecutor *executor, AuraTaskFrame *frame);\n",
    );
    out.push_str(
        "int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame);\n",
    );
    out.push_str(
        "int aura_task_executor_cancel(AuraTaskExecutor *executor, AuraTaskFrame *frame);\n",
    );
    out.push_str(
        "AuraTaskPollState aura_task_executor_join(AuraTaskExecutor *executor, AuraTaskFrame *frame, AuraTaskResult *out_result, AuraTaskResult *out_error);\n",
    );
    out.push_str(
        "int aura_task_executor_release(AuraTaskExecutor *executor, AuraTaskFrame **handle);\n",
    );
    out.push_str("int aura_task_executor_retain_payload(AuraTaskExecutor *executor, AuraTaskFrame *frame);\n");
    out.push_str("int aura_task_executor_release_payload(AuraTaskExecutor *executor, AuraTaskFrame **payload);\n");
    out.push_str("int aura_task_executor_run_one(AuraTaskExecutor *executor);\n");
    out.push_str("size_t aura_task_executor_run(AuraTaskExecutor *executor);\n");
    out.push_str("int aura_task_executor_has_live_tasks(const AuraTaskExecutor *executor);\n");
    out.push_str("int aura_task_executor_release_terminal(AuraTaskExecutor *executor, AuraTaskFrame **handle);\n");
    out.push_str("void aura_task_executor_shutdown(AuraTaskExecutor *executor);\n");
    out.push_str("static AuraTaskExecutor *__aura_task_executor = NULL;\n");
    out.push_str("static AuraRaceTracker *__aura_race_tracker = NULL;\n");
    out.push_str("AuraTaskChannel *aura_task_channel_new(size_t capacity);\n");
    out.push_str("AuraTaskChannelStatus aura_task_channel_send(AuraTaskChannel *channel, AuraTaskFrame *sender, AuraTaskChannelValue value);\n");
    out.push_str("AuraTaskChannelStatus aura_task_channel_receive(AuraTaskChannel *channel, AuraTaskFrame *receiver, AuraTaskChannelValue *out);\n");
    out.push_str("int aura_task_channel_retain(AuraTaskChannel *channel);\n");
    out.push_str("AuraTaskChannelValue aura_task_channel_value_from_task(AuraTaskExecutor *executor, AuraTaskFrame *frame);\n");
    out.push_str("AuraTaskFrame *aura_task_channel_value_take_task(void *data, size_t size);\n");
    out.push_str(
        "AuraTaskChannelValue aura_task_channel_value_from_channel(AuraTaskChannel *channel);\n",
    );
    out.push_str(
        "AuraTaskChannel *aura_task_channel_value_take_channel(void *data, size_t size);\n",
    );
    out.push_str("int aura_task_channel_close(AuraTaskChannel *channel);\n");
    out.push_str("void aura_task_channel_destroy(AuraTaskChannel *channel);\n");
    out.push_str("AuraTaskSelect *aura_task_select_new(void);\n");
    out.push_str("int aura_task_select_add(AuraTaskSelect *select, AuraTaskChannel *channel);\n");
    out.push_str("AuraTaskChannelStatus aura_task_select_next(AuraTaskSelect *select, AuraTaskFrame *frame, AuraTaskChannelValue *out, size_t *index);\n");
    out.push_str("void aura_task_select_destroy(AuraTaskSelect *select);\n");
    out.push_str("void aura_task_channel_value_destroy_free(void *data, size_t size);\n");
    out.push_str("void aura_task_channel_value_destroy_class(void *data, size_t size);\n");
    out.push_str("void aura_task_channel_value_destroy_task(void *data, size_t size);\n");
    out.push_str("void aura_task_channel_value_destroy_channel(void *data, size_t size);\n");
    out.push_str("static void aura_task_channel_value_destroy_foreign_handle(void *data, size_t size) { (void)size; if (data != NULL) { AuraFfiOpaqueHandle **handle = (AuraFfiOpaqueHandle **)data; if (*handle != NULL) (void)aura_ffi_handle_drop(handle); free(data); } }\n");
    // C12m/C13f: shared mutable boxes for var Int/Bool/String captures.
    out.push_str("typedef struct aura_box_i64 { int64_t value; int32_t refs; } aura_box_i64;\n");
    out.push_str("typedef struct aura_box_bool { _Bool value; int32_t refs; } aura_box_bool;\n");
    out.push_str(
        "typedef struct aura_box_str { const char *value; int32_t refs; } aura_box_str;\n",
    );
    out.push_str("aura_box_i64 *aura_box_i64_new(int64_t v);\n");
    out.push_str("void aura_box_i64_retain(aura_box_i64 *b);\n");
    out.push_str("void aura_box_i64_release(aura_box_i64 *b);\n");
    out.push_str("aura_box_bool *aura_box_bool_new(_Bool v);\n");
    out.push_str("void aura_box_bool_retain(aura_box_bool *b);\n");
    out.push_str("void aura_box_bool_release(aura_box_bool *b);\n");
    out.push_str("aura_box_str *aura_box_str_new(const char *v);\n");
    out.push_str("void aura_box_str_retain(aura_box_str *b);\n");
    out.push_str("void aura_box_str_release(aura_box_str *b);\n");
    out.push_str("const char *aura_box_str_set(aura_box_str *b, const char *v);\n");
    out.push_str("const char *aura_box_str_get(aura_box_str *b);\n");
    out.push_str("typedef void (*aura_box_ptr_drop_fn)(void *value);\n");
    out.push_str("typedef struct aura_box_ptr { void *value; int32_t refs; aura_box_ptr_drop_fn drop; } aura_box_ptr;\n");
    out.push_str("aura_box_ptr *aura_box_ptr_new(void *value, aura_box_ptr_drop_fn drop);\n");
    out.push_str("void aura_box_ptr_retain(aura_box_ptr *b);\n");
    out.push_str("void aura_box_ptr_release(aura_box_ptr *b);\n");
    out.push_str("void *aura_box_ptr_get(const aura_box_ptr *b);\n");
    out.push_str(
        "void *aura_box_ptr_set(aura_box_ptr *b, void *value, aura_box_ptr_drop_fn drop);\n",
    );
    out.push_str("int aura_main(void);\n\n");
    // C7a: tagged optional primitives (Int? / Bool?).
    out.push_str("typedef struct { _Bool has; int64_t value; } aura_opt_i64;\n");
    out.push_str("typedef struct { _Bool has; _Bool value; } aura_opt_bool;\n");
    // C13c: Int.toString — malloc'd decimal; caller owns (like other owned strings).
    out.push_str("const char *aura_i64_to_string(int64_t v);\n\n");
    out.push_str("int64_t aura_hash_string(const char *s);\n\n");

    // Stable class tags for interface dispatch (C9a: include generic monomorphs).
    let mut tag_monos: Vec<String> = Vec::new();
    for c in &checked.ast.classes {
        if c.kind != NominalKind::Class {
            continue;
        }
        if c.type_params.is_empty() {
            let pkg = class_decl_package(c, checked);
            tag_monos.push(type_mono(&pkg, &c.name.name, &[]));
        }
    }
    for (name, args) in &checked.mono_classes {
        if is_array_mono(name) {
            continue;
        }
        if let Some(c) = checked
            .ast
            .classes
            .iter()
            .find(|c| c.name.name == *name && c.kind == NominalKind::Class)
        {
            let pkg = class_decl_package(c, checked);
            let mono = type_mono(&pkg, &c.name.name, args);
            if !tag_monos.contains(&mono) {
                tag_monos.push(mono);
            }
        }
    }
    if !tag_monos.is_empty() {
        out.push_str("enum {\n");
        for (i, mono) in tag_monos.iter().enumerate() {
            let _ = writeln!(out, "  AURA_TAG_{mono} = {i},");
        }
        out.push_str("  AURA_TAG__COUNT\n};\n\n");
    }

    // Class typedefs — non-generic classes + monomorphized generic classes.
    // C4u: incomplete struct forwards so nested monomorph field pointers compile
    // regardless of alphabetical mono_classes order (Outer_String → Wrapper_String).
    for c in &checked.ast.classes {
        if c.type_params.is_empty() {
            let pkg = class_decl_package(c, checked);
            let mono = type_mono(&pkg, &c.name.name, &[]);
            let _ = writeln!(
                out,
                "typedef struct {} {};",
                c_class_type(&mono),
                c_class_type(&mono)
            );
        }
    }
    for (name, args) in &checked.mono_classes {
        if is_array_mono(name) {
            continue;
        }
        if let Some(c) = checked.ast.classes.iter().find(|c| c.name.name == *name) {
            let pkg = class_decl_package(c, checked);
            let mono = type_mono(&pkg, &c.name.name, args);
            let _ = writeln!(
                out,
                "typedef struct {} {};",
                c_class_type(&mono),
                c_class_type(&mono)
            );
        }
    }
    // Interface unions must precede enums that store an interface by value.
    let mut emit_iface_union = |iface: &InterfaceDecl, args: &[Ty]| {
        let imono = iface_mono_args(iface, checked, args);
        let impls = crate::iface::mono_implementors_for_iface(checked, iface, args);
        let _ = writeln!(out, "typedef struct {} {{", c_iface_type(&imono));
        out.push_str("  int tag;\n  union {\n");
        for imp in &impls {
            let pkg = class_decl_package(imp.class, checked);
            let mono = type_mono(&pkg, &imp.class.name.name, &imp.class_args);
            let _ = writeln!(out, "    {} *as_{mono};", c_class_type(&mono));
        }
        if impls.is_empty() {
            out.push_str("    char _empty;\n");
        }
        let _ = writeln!(out, "  }} data;\n}} {};\n", c_iface_type(&imono));
    };
    for iface in &checked.ast.interfaces {
        if iface.type_params.is_empty() {
            emit_iface_union(iface, &[]);
        }
    }
    for (name, args) in &checked.mono_interfaces {
        if let Some(iface) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            emit_iface_union(iface, args);
        }
    }

    // Primitive Arrays do not depend on enum/class definitions. Emit them early
    // so generic enums such as Result<Array<Int>, E> can store the value by value.
    for args in checked
        .mono_classes
        .iter()
        .filter(|(name, args)| {
            is_array_mono(name)
                && args
                    .first()
                    .is_some_and(|elem| matches!(elem, Ty::Int | Ty::Bool | Ty::String | Ty::Unit))
        })
        .map(|(_, args)| args.as_slice())
    {
        if let Some(elem) = args.first() {
            emit_array_mono(&mut out, elem, checked);
        }
    }

    // C6g: enums + value structs must be complete before non-primitive Array
    // monomorphs (Array stores elements by value). Heap classes stay incomplete
    // here so Array-of-class can use pointers only. Enum typedefs whose payload
    // itself contains an Array are delayed until that Array typedef exists;
    // otherwise `Result<Array<E>, ...>` is emitted with an incomplete C type.
    let enum_uses_array = |e: &EnumDecl, args: &[Ty]| {
        let params: Vec<String> = e.type_params.iter().map(|p| p.name.name.clone()).collect();
        e.variants.iter().any(|variant| {
            variant.fields.iter().any(|field| {
                let key = type_ref_local_key_expand(&field.ty, &params, args, checked);
                is_array_type_key(&full_type_mono(&key, checked))
            })
        })
    };
    let enum_directly_uses_value_struct = |e: &EnumDecl, args: &[Ty]| {
        let params: Vec<String> = e.type_params.iter().map(|p| p.name.name.clone()).collect();
        e.variants.iter().any(|variant| {
            variant.fields.iter().any(|field| {
                let key = type_ref_local_key_expand(&field.ty, &params, args, checked);
                let full = full_type_mono(&key, checked);
                crate::expr::is_value_struct_mono(&full, checked)
            })
        })
    };
    // A generic enum such as `Result<T, E>` can carry another enum whose
    // concrete layout is delayed by a value-struct/Array field.  Its generic
    // typedef must be delayed as well, otherwise C sees the nested enum only
    // after the by-value Result field has already been emitted.
    let enum_uses_value_struct = |e: &EnumDecl, args: &[Ty]| {
        enum_directly_uses_value_struct(e, args)
            || args.iter().any(|arg| {
                let (name, nested_args) = match arg {
                    Ty::Enum(name) => (name, &[][..]),
                    Ty::EnumApp { name, args } => (name, args.as_slice()),
                    _ => return false,
                };
                let base = aura_sema::split_nominal(name).0;
                checked
                    .ast
                    .enums
                    .iter()
                    .find(|nested| nested.name.name == base)
                    .is_some_and(|nested| enum_directly_uses_value_struct(nested, nested_args))
            })
    };
    for e in &checked.ast.enums {
        if e.type_params.is_empty() && !enum_uses_array(e, &[]) && !enum_uses_value_struct(e, &[]) {
            emit_enum_typedef(&mut out, checked, e, &[]);
        }
    }
    for (name, args) in &checked.mono_enums {
        if let Some(e) = checked.ast.enums.iter().find(|e| e.name.name == *name) {
            if !enum_uses_array(e, args) && !enum_uses_value_struct(e, args) {
                emit_enum_typedef(&mut out, checked, e, args);
            }
        }
    }
    for c in &checked.ast.classes {
        if c.type_params.is_empty() && c.kind == NominalKind::Struct {
            emit_class_typedef(&mut out, checked, c, &[]);
        }
    }
    for (name, args) in &checked.mono_classes {
        if is_array_mono(name) {
            continue;
        }
        if let Some(c) = checked
            .ast
            .classes
            .iter()
            .find(|c| c.name.name == *name && c.kind == NominalKind::Struct)
        {
            emit_class_typedef(&mut out, checked, c, args);
        }
    }

    // Interface elements need their tagged-union definition before Array emits
    // the element pointer type. Leaf arrays still precede nested arrays.
    let mut array_monos: Vec<&[Ty]> = checked
        .mono_classes
        .iter()
        .filter(|(name, _)| is_array_mono(name))
        .map(|(_, args)| args.as_slice())
        .collect();
    array_monos.sort_by_key(|args| {
        args.first()
            .map(|e| e.mono_suffix().matches("Array_").count())
            .unwrap_or(0)
    });
    for args in array_monos {
        if let Some(elem) = args.first() {
            if matches!(elem, Ty::Int | Ty::Bool | Ty::String | Ty::Unit) {
                continue;
            }
            emit_array_mono(&mut out, elem, checked);
        }
    }

    // Complete the enum layouts that contain an Array only after all Array
    // monomorphs are defined. This covers generic Result/Outcome payloads and
    // user enums without requiring a special-case for Task<T>.
    for e in &checked.ast.enums {
        if e.type_params.is_empty() && (enum_uses_array(e, &[]) || enum_uses_value_struct(e, &[])) {
            emit_enum_typedef(&mut out, checked, e, &[]);
        }
    }
    for (name, args) in &checked.mono_enums {
        if let Some(e) = checked.ast.enums.iter().find(|e| e.name.name == *name) {
            if enum_uses_array(e, args) || enum_uses_value_struct(e, args) {
                emit_enum_typedef(&mut out, checked, e, args);
            }
        }
    }

    for c in &checked.ast.classes {
        if c.type_params.is_empty() && c.kind == NominalKind::Class {
            emit_class_typedef(&mut out, checked, c, &[]);
        }
    }
    for (name, args) in &checked.mono_classes {
        if is_array_mono(name) {
            continue;
        }
        if let Some(c) = checked
            .ast
            .classes
            .iter()
            .find(|c| c.name.name == *name && c.kind == NominalKind::Class)
        {
            emit_class_typedef(&mut out, checked, c, args);
        }
    }

    for iface in &checked.ast.interfaces {
        if iface.type_params.is_empty() {
            emit_iface_ownership_prototypes(&mut out, iface, checked, &[]);
        }
    }
    for (name, args) in &checked.mono_interfaces {
        if let Some(iface) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            emit_iface_ownership_prototypes(&mut out, iface, checked, args);
        }
    }

    // Fun-type typedefs must precede class method prototypes that use them.
    emit_fun_typedefs(&mut out, checked);

    // Forward decls
    for c in &checked.ast.classes {
        if c.type_params.is_empty() {
            emit_class_forwards(&mut out, checked, c, &[]);
        }
    }
    for (name, args) in &checked.mono_classes {
        if is_array_mono(name) {
            continue; // Array defs already fully emitted
        }
        if let Some(c) = checked.ast.classes.iter().find(|c| c.name.name == *name) {
            emit_class_forwards(&mut out, checked, c, args);
        }
    }
    for e in &checked.ast.enums {
        if e.type_params.is_empty() {
            emit_enum_forwards(&mut out, checked, e, &[]);
        }
    }
    for (name, args) in &checked.mono_enums {
        if let Some(e) = checked.ast.enums.iter().find(|e| e.name.name == *name) {
            emit_enum_forwards(&mut out, checked, e, args);
        }
    }
    for iface in &checked.ast.interfaces {
        if iface.type_params.is_empty() {
            let imono = iface_mono(iface, checked);
            for m in crate::iface::interface_methods_with_parents(checked, iface) {
                let _ = writeln!(out, "{};", c_iface_method_signature(&imono, m, checked));
            }
        }
    }
    for (name, args) in &checked.mono_interfaces {
        if let Some(iface) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            let imono = iface_mono_args(iface, checked, args);
            for (m, owner, owner_args) in
                crate::iface::interface_method_decls_with_parents(checked, iface, args)
            {
                let owner_tparams = owner
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect::<Vec<_>>();
                let _ = writeln!(
                    out,
                    "{};",
                    c_iface_method_signature_args(&imono, m, checked, &owner_tparams, &owner_args)
                );
            }
        }
    }
    for f in &checked.ast.functions {
        if f.name.name == "main" {
            continue;
        }
        if f.type_params.is_empty() {
            let _ = writeln!(out, "{};", c_fun_signature(f, checked, &[]));
        }
    }
    for (name, args) in &generic_functions {
        if let Some(f) = checked.ast.functions.iter().find(|f| f.name.name == *name) {
            let _ = writeln!(out, "{};", c_fun_signature(f, checked, args));
        }
    }
    for f in &checked.ast.async_functions {
        if f.type_params.is_empty() {
            let _ = writeln!(out, "{};", c_async_fun_signature(f, checked));
        }
    }
    for f in &checked.ast.async_functions {
        if open_erased_async_supported(f) || open_erased_async_forward_supported(f, checked) {
            let _ = writeln!(out, "{};", c_async_fun_signature(f, checked));
        }
    }
    for (name, args) in &generic_async_functions {
        if let Some(f) = checked
            .ast
            .async_functions
            .iter()
            .find(|f| f.name.name == *name)
        {
            let _ = writeln!(out, "{};", c_async_fun_signature_args(f, checked, args));
        }
    }
    out.push('\n');

    emit_capture_drop_helpers(&mut out, checked);

    // C22l: emit the bounded, capture-free spawn pollers after all ordinary
    // declarations are visible. Unsupported bodies keep the explicit abort
    // path in expression emission.
    emit_bounded_spawn_pollers(
        &mut out,
        checked,
        opts.detector,
        &generic_functions,
        &generic_async_functions,
    );
    emit_lazy_helpers(&mut out, checked);

    // Definitions
    for c in &checked.ast.classes {
        if c.type_params.is_empty() {
            emit_class_defs(&mut out, checked, c, &[], opts.detector);
        }
    }
    for (name, args) in &checked.mono_classes {
        if is_array_mono(name) {
            continue;
        }
        if let Some(c) = checked.ast.classes.iter().find(|c| c.name.name == *name) {
            emit_class_defs(&mut out, checked, c, args, opts.detector);
        }
    }

    for iface in &checked.ast.interfaces {
        if iface.type_params.is_empty() {
            emit_iface_ownership_hooks(&mut out, iface, checked, &[]);
        }
    }
    for (name, args) in &checked.mono_interfaces {
        if let Some(iface) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            emit_iface_ownership_hooks(&mut out, iface, checked, args);
        }
    }

    for e in &checked.ast.enums {
        if e.type_params.is_empty() {
            emit_enum_defs(&mut out, checked, e, &[]);
        }
    }
    for (name, args) in &checked.mono_enums {
        if let Some(e) = checked.ast.enums.iter().find(|e| e.name.name == *name) {
            emit_enum_defs(&mut out, checked, e, args);
        }
    }

    emit_channel_value_drop_helpers(&mut out, checked);

    for iface in &checked.ast.interfaces {
        if iface.type_params.is_empty() {
            for m in crate::iface::interface_methods_with_parents(checked, iface) {
                emit_iface_dispatch(&mut out, checked, iface, m, &[]);
                out.push('\n');
            }
        }
    }
    for (name, args) in &checked.mono_interfaces {
        if let Some(iface) = checked.ast.interfaces.iter().find(|i| i.name.name == *name) {
            for (m, owner, owner_args) in
                crate::iface::interface_method_decls_with_parents(checked, iface, args)
            {
                let owner_tparams = owner
                    .type_params
                    .iter()
                    .map(|p| p.name.name.clone())
                    .collect::<Vec<_>>();
                crate::iface::emit_iface_dispatch_with_method_args(
                    &mut out,
                    checked,
                    iface,
                    m,
                    args,
                    &owner_tparams,
                    &owner_args,
                );
                out.push('\n');
            }
        }
    }

    // C10e: non-capturing lambdas as static C functions (typedefs already emitted).
    emit_lambda_fns(&mut out, checked, opts.detector);

    for f in &checked.ast.functions {
        if f.type_params.is_empty() {
            let emitted_from_mir = ir
                .and_then(|ir| {
                    ir.functions
                        .iter()
                        .find(|candidate| candidate.name == f.name.name)
                })
                .is_some_and(|candidate| crate::mir_emit::emit_function(&mut out, candidate));
            if !emitted_from_mir {
                emit_fun(&mut out, f, checked, &[], opts.detector);
            }
            out.push('\n');
        }
    }
    for (name, args) in &generic_functions {
        if let Some(f) = checked.ast.functions.iter().find(|f| f.name.name == *name) {
            let instance_name = format!(
                "{}_{}",
                name,
                args.iter()
                    .map(Ty::mono_suffix)
                    .collect::<Vec<_>>()
                    .join("_")
            );
            let emitted_from_mir = ir
                .and_then(|ir| {
                    ir.generic_functions
                        .iter()
                        .find(|candidate| candidate.name == instance_name)
                })
                .is_some_and(|candidate| crate::mir_emit::emit_function(&mut out, candidate));
            if !emitted_from_mir {
                emit_fun(&mut out, f, checked, args, opts.detector);
            }
            out.push('\n');
        }
    }

    for f in &checked.ast.async_functions {
        if f.type_params.is_empty() {
            let mir_body =
                ir.and_then(|ir| ir.async_mir.iter().find(|body| body.name == f.name.name));
            emit_async_fun_decl(&mut out, f, checked, opts.detector, mir_body);
            out.push('\n');
        }
    }
    for f in &checked.ast.async_functions {
        if open_erased_async_supported(f) {
            emit_open_erased_async_fun(&mut out, f, checked);
            out.push('\n');
        } else if open_erased_async_forward_supported(f, checked) {
            emit_open_erased_async_forward_fun(&mut out, f, checked);
            out.push('\n');
        }
    }
    for (name, args) in &generic_async_functions {
        if let Some(f) = checked
            .ast
            .async_functions
            .iter()
            .find(|f| f.name.name == *name)
        {
            let mono = aura_ir::generic_lowering::close_async_function(f, args, checked);
            let mir_body = ir.and_then(|ir| {
                ir.generic_async_mir
                    .iter()
                    .find(|body| body.name == mono.name.name)
            });
            emit_async_fun_decl(&mut out, &mono, checked, opts.detector, mir_body);
            out.push('\n');
        }
    }

    if opts.test {
        emit_test_main(&mut out, checked, opts.detector);
    } else {
        out.push_str("int aura_main(void) {\n");
        out.push_str("  if (!aura_runtime_check_abi(AURA_GENERATED_ABI_VERSION, AURA_GENERATED_ABI_ID)) return 78;\n");
        out.push_str("  __aura_task_executor = aura_task_executor_new();\n");
        if opts.detector {
            out.push_str("  __aura_race_tracker = aura_race_tracker_new();\n");
            out.push_str("  aura_race_tracker_set_active(__aura_race_tracker);\n");
            out.push_str("  aura_task_executor_set_race_tracker(__aura_task_executor, __aura_race_tracker);\n");
        }
        if checked.ast.functions.iter().any(|f| f.name.name == "main") {
            out.push_str("  aura_fn_main();\n");
        }
        out.push_str("  while (aura_task_executor_has_live_tasks(__aura_task_executor)) { if (aura_task_executor_run(__aura_task_executor) == 0) (void)aura_task_executor_poll_waiting(__aura_task_executor, 1000); }\n");
        out.push_str("  aura_task_executor_shutdown(__aura_task_executor);\n");
        if opts.detector {
            out.push_str("  aura_race_tracker_destroy(__aura_race_tracker);\n");
        }
        out.push_str("  return 0;\n}\n");
    }
    out
}

pub(crate) fn emit_async_fun_decl(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
    mir_body: Option<&aura_ir::mir::MirBody>,
) {
    let normalized = aura_ir::lowering::normalize_return_await(f);
    let lowered = normalized.as_ref().unwrap_or(f);
    let emitted_std_io = emit_async_fun_std_io_fd(out, lowered, checked, detector);
    let emitted_std_net = async_fun_decl_package(lowered, checked) == "std.net"
        && ((lowered.name.name == "accept"
            && emit_async_fun_std_net_accept(out, lowered, checked))
            || (lowered.name.name == "readStream"
                && emit_async_fun_std_net_stream(out, lowered, checked, true))
            || (lowered.name.name == "writeStream"
                && emit_async_fun_std_net_stream(out, lowered, checked, false)));
    let is_std_udp_method = c_async_fun_signature(lowered, checked).contains("std_udp_Socket");
    let emitted_std_udp = is_std_udp_method && emit_async_fun_std_udp(out, lowered, checked);
    let emitted_std_http = async_fun_decl_package(lowered, checked) == "std.http"
        && ((lowered.name.name == "serveConnection"
            && emit_async_fun_std_http_serve_connection(out, lowered, checked))
            || (lowered.name.name == "serve"
                && emit_async_fun_std_http_serve(out, lowered, checked))
            || (lowered.name.name == "readChunk"
                && emit_async_fun_std_http_request_body_chunk(out, lowered, checked)));
    let emitted_std_time = async_fun_decl_package(lowered, checked) == "std.time"
        && emit_async_fun_std_time_sleep(out, lowered, checked);
    if !emitted_std_io
        && !emitted_std_net
        && !emitted_std_udp
        && !emitted_std_http
        && !emitted_std_time
        && !emit_async_fun_cfg_int(out, lowered, checked, detector, false, &HashSet::new())
        && !emit_async_fun_while_branch_join_await_array(out, lowered, checked, detector)
        && !emit_async_fun_while_branch_join_await_int(out, lowered, checked, detector)
        && !emit_async_fun_nested_while_await_int(out, lowered, checked, detector)
        && !emit_async_fun_while_guarded_await_int(out, lowered, checked, detector)
        && !emit_async_fun_top_level_while_conditional_await_int(out, lowered, checked, detector)
        && !emit_async_fun_while_multi_conditional_await_int(out, lowered, checked, detector)
        && !emit_async_fun_while_two_conditional_await_int(out, lowered, checked, detector)
        && !emit_async_fun_while_multi_await_int(out, lowered, checked, detector)
        && !emit_async_fun_for_range_await_int(out, lowered, checked, detector)
        && !emit_async_fun_top_level_while_await_int(out, lowered, checked, detector)
        && !emit_async_fun_nested_if_branch_awaits(out, lowered, checked, detector)
        && !emit_async_fun_if_then_multi_await(out, lowered, checked, detector)
        && !emit_async_fun_if_else_assign_await_continue(out, lowered, checked, detector)
        && !emit_async_fun_if_else_single_await(out, lowered, checked, detector)
        && !emit_async_fun_if_assign_await(out, lowered, checked, detector)
        && !emit_async_fun_if_await_then_continue(out, lowered, checked, detector)
        && !emit_async_fun_if_single_await(out, lowered, checked, detector)
        && !emit_async_fun_general_multi_await(out, lowered, checked, detector)
        && !emit_async_fun_multi_await(out, lowered, checked, detector)
        && !emit_async_fun_single_await(out, lowered, checked, detector)
        // Retry the full CFG with same-typed branch locals merged into shared
        // frame slots after the historical shape-specific lowerers decline.
        && !emit_async_fun_cfg_int(out, lowered, checked, detector, true, &HashSet::new())
    {
        emit_async_fun_no_await(out, lowered, checked, detector, mir_body);
    }
}

/// Direct codegen unit tests intentionally build a file without the CLI's
/// auto-prelude merge. Keep `join(TaskHandle<Unit>)` compilable in that mode;
/// package builds use the real `std.io.TaskError` and generic Result enums.
fn emit_fallback_unit_join_result(out: &mut String, checked: &CheckedFile) {
    let has_std_task_error_decl = checked.ast.enums.iter().any(|e| {
        e.name.name == "TaskError" && (e.origin_package == "std.io" || checked.package == "std.io")
    });
    let has_std_result_decl = checked.ast.enums.iter().any(|e| {
        e.name.name == "Result" && (e.origin_package == "std.io" || checked.package == "std.io")
    });
    let has_result_unit = has_std_result_decl
        && checked.ast.enums.iter().any(|e| e.name.name == "TaskError")
        && checked.mono_enums.iter().any(|(name, args)| {
            name == "Result"
                && args.len() == 2
                && args[0] == Ty::Unit
                && args[1] == Ty::Enum("TaskError@std.io".into())
        });
    if has_result_unit {
        return;
    }
    // A std.io prelude must have recorded Result<Unit, TaskError> above; if
    // it has not, leave the real enum emission in charge rather than defining
    // a duplicate/incomplete by-value TaskError type here.
    if has_std_task_error_decl || has_std_result_decl {
        return;
    }
    out.push_str("typedef struct aura_enum_std_io_TaskError { int tag; union { struct { const char *error; bool owned; const char *type_name; bool type_name_owned; uint32_t source_id; uint32_t span_start; uint32_t span_end; } Failed; char as_Cancelled; } data; } aura_enum_std_io_TaskError;\n");
    out.push_str("static __attribute__((unused)) aura_enum_std_io_TaskError aura_var_std_io_TaskError_Failed(const char *error) { aura_enum_std_io_TaskError self; self.tag = 0; self.data.Failed.error = error; self.data.Failed.owned = false; self.data.Failed.type_name = NULL; self.data.Failed.type_name_owned = false; self.data.Failed.source_id = 0; self.data.Failed.span_start = 0; self.data.Failed.span_end = 0; return self; }\n");
    out.push_str("static __attribute__((unused)) aura_enum_std_io_TaskError aura_var_std_io_TaskError_FailedOwned(const char *error) { aura_enum_std_io_TaskError self; self.tag = 0; self.data.Failed.error = error; self.data.Failed.owned = true; self.data.Failed.type_name = NULL; self.data.Failed.type_name_owned = false; self.data.Failed.source_id = 0; self.data.Failed.span_start = 0; self.data.Failed.span_end = 0; return self; }\n");
    out.push_str("static __attribute__((unused)) aura_enum_std_io_TaskError aura_var_std_io_TaskError_Cancelled(void) { aura_enum_std_io_TaskError self; self.tag = 1; return self; }\n");
    if !has_result_unit {
        out.push_str("typedef struct aura_enum_std_io_Result_Unit_std_io_TaskError { int tag; union { char as_Ok; struct { aura_enum_std_io_TaskError error; } Err; } data; } aura_enum_std_io_Result_Unit_std_io_TaskError;\n");
        out.push_str("static aura_enum_std_io_Result_Unit_std_io_TaskError aura_var_std_io_Result_Unit_std_io_TaskError_Ok(void) { aura_enum_std_io_Result_Unit_std_io_TaskError self; self.tag = 0; return self; }\n");
        out.push_str("static aura_enum_std_io_Result_Unit_std_io_TaskError aura_var_std_io_Result_Unit_std_io_TaskError_Err(aura_enum_std_io_TaskError error) { aura_enum_std_io_Result_Unit_std_io_TaskError self; self.tag = 1; self.data.Err.error = error; return self; }\n");
    }
}

/// Lower a bounded loop whose branch arms await `Task<Array<T>>` and replace
/// one frame-owned Array value on every iteration. The child payload is cloned
/// before the previous frame value is released, so the child frame can retain
/// its terminal result for repeated joins.
fn emit_async_fun_while_branch_join_await_array(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    let Some(return_type) = f.return_type.as_ref() else {
        return false;
    };
    let return_key = type_ref_local_key_expand(return_type, &[], &[], checked);
    if !is_array_type_key(&return_key) || f.body.stmts.len() != 4 {
        return false;
    }
    let Stmt::Var(index_var) = &f.body.stmts[0] else {
        return false;
    };
    let Stmt::Var(value_var) = &f.body.stmts[1] else {
        return false;
    };
    let Stmt::While(loop_stmt) = &f.body.stmts[2] else {
        return false;
    };
    let Some(index_ty) = index_var.ty.as_ref() else {
        return false;
    };
    let Some(value_ty) = value_var.ty.as_ref() else {
        return false;
    };
    if !index_var.mutable
        || !value_var.mutable
        || type_ref_local_key_expand(index_ty, &[], &[], checked) != "Int"
        || type_ref_local_key_expand(value_ty, &[], &[], checked) != return_key
        || matches!(&index_var.init, Expr::Async(_))
        || matches!(&value_var.init, Expr::Async(_))
        || !matches!(loop_stmt.cond, Expr::Binary(_))
        || loop_stmt.body.stmts.len() != 3
    {
        return false;
    }
    let Stmt::If(branch) = &loop_stmt.body.stmts[0] else {
        return false;
    };
    let Some(else_block) = &branch.else_block else {
        return false;
    };
    if branch.then_block.stmts.len() != 1 || else_block.stmts.len() != 1 {
        return false;
    }
    let Some(then_await) = branch_assign_await(&branch.then_block.stmts[0], &value_var.name.name)
    else {
        return false;
    };
    let Some(else_await) = branch_assign_await(&else_block.stmts[0], &value_var.name.name) else {
        return false;
    };
    if matches!(branch.cond, Expr::Async(_)) {
        return false;
    }
    let Stmt::Expr(Expr::Call(gc_call)) = &loop_stmt.body.stmts[1] else {
        return false;
    };
    if !matches!(gc_call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
        || !gc_call.args.is_empty()
    {
        return false;
    }
    let Stmt::Expr(Expr::Assign(index_assign)) = &loop_stmt.body.stmts[2] else {
        return false;
    };
    if index_assign.name.name != index_var.name.name {
        return false;
    }
    let Some(Stmt::Return(return_stmt)) = f.body.stmts.last() else {
        return false;
    };
    if !matches!(return_stmt.value.as_ref(), Some(Expr::Ident(id)) if id.name == value_var.name.name)
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let data_drop = format!("aura_async_data_drop_{base}");
    let gc_mark = format!("aura_async_gc_mark_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let index_name = mangle_ident(&index_var.name.name);
    let value_name = mangle_ident(&value_var.name.name);
    let value_cty = crate::stmt::local_key_to_c(&return_key, checked);
    let clone_fn = crate::names::c_method_name(&return_key, "clone");
    let head_label = format!("aura_async_{base}_array_branch_head");
    let poll_label = format!("aura_async_{base}_array_branch_poll");

    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let index_init = coerce_expr(&index_var.init, "Int", &mut entry_ctx);
    entry_ctx.define_local(&index_var.name.name, "Int".into());
    let value_init = coerce_expr(&value_var.init, &return_key, &mut entry_ctx);
    entry_ctx.define_local(&value_var.name.name, return_key.clone());
    let loop_cond = emit_expr(&loop_stmt.cond, &mut entry_ctx);
    let branch_cond = emit_expr(&branch.cond, &mut entry_ctx);
    let then_task = emit_expr(&then_await.operand, &mut entry_ctx);
    let else_task = emit_expr(&else_await.operand, &mut entry_ctx);

    let mut post_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    post_ctx.define_local(&index_var.name.name, "Int".into());
    post_ctx.define_local(&value_var.name.name, return_key.clone());
    let index_rhs = coerce_expr(&index_assign.value, "Int", &mut post_ctx);

    let _ = writeln!(
        out,
        "/* aura async loop branch-join Array suspension states=2 spans={}:{}|{}:{} */",
        then_await.span.start, then_await.span.end, else_await.span.start, else_await.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(
        out,
        "  int64_t {index_name}; {value_cty} {value_name}; AuraTaskFrame *await_task;\n}} {data_ty};\n"
    );
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    crate::array_emit::emit_array_contents_free_checked(
        out,
        2,
        &format!("data->{value_name}"),
        &return_key,
        checked,
    );
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {data_drop}(AuraTaskFrame *frame, void *raw_data, size_t size) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)raw_data; (void)frame; (void)size;"
    );
    crate::array_emit::emit_array_contents_free_checked(
        out,
        1,
        &format!("data->{value_name}"),
        &return_key,
        checked,
    );
    out.push_str("}\n\n");
    let _ = writeln!(out, "static void {gc_mark}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL) return;"
    );
    let value_mark_cty = crate::stmt::local_key_to_c(&return_key, checked);
    let _ = writeln!(out, "  {value_mark_cty}_mark(&data->{value_name});");
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{"
    );
    let _ = writeln!(
        out,
        "  (void)size; if (data != NULL) {{ {value_cty} *result = ({value_cty} *)data;"
    );
    crate::array_emit::emit_array_contents_free_checked(out, 2, "(*result)", &return_key, checked);
    out.push_str("  free(result); }\n}\n\n");
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    let _ = writeln!(
        out,
        "      data->{index_name} = {index_init}; data->{value_name} = {value_init}; data->await_task = NULL; aura_task_frame_set_resume_state(frame, 1);"
    );
    let _ = writeln!(out, "      goto {head_label};\n    }}\n    case 1: goto {head_label};\n    case 2: goto {poll_label};\n    default: return AURA_TASK_FAILED;\n  }}\n");
    let _ = writeln!(out, "{head_label}:\n  for (;;) {{");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "    {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "    int64_t {index_name} = data->{index_name}; {value_cty} {value_name} = data->{value_name};");
    let _ = writeln!(out, "    if (!({loop_cond})) break;");
    let _ = writeln!(
        out,
        "    data->await_task = ({branch_cond}) ? {then_task} : {else_task};"
    );
    let _ = writeln!(out, "    if (data->await_task == NULL) return AURA_TASK_FAILED;\n    aura_task_frame_set_resume_state(frame, 2);\n    goto {poll_label};\n  }}");
    let _ = writeln!(out, "  {value_cty} *result = ({value_cty} *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = {clone_fn}(&data->{value_name}); aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE;\n");
    let _ = writeln!(out, "{poll_label}:");
    out.push_str("  { AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task); if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; } if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; } if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED; AuraTaskResult child_result = aura_task_frame_result(data->await_task); ");
    let _ = writeln!(
        out,
        "{value_cty} __next = child_result.data == NULL ? ({value_cty}){{0}} : *(({value_cty} *)child_result.data); {value_cty} __copy = {clone_fn}(&__next);"
    );
    crate::array_emit::emit_array_contents_free_checked(
        out,
        2,
        &format!("data->{value_name}"),
        &return_key,
        checked,
    );
    let _ = writeln!(
        out,
        " data->{value_name} = __copy; data->await_task = NULL; int64_t {index_name} = data->{index_name}; {value_cty} {value_name} = data->{value_name}; aura_gc_collect_executor(__aura_task_executor); data->{index_name} = {index_rhs}; goto {head_label}; }}\n}}"
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data}); if (frame == NULL) return NULL; aura_task_frame_set_gc_mark(frame, {gc_mark}); aura_task_frame_set_data_drop(frame, {data_drop});"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  data->await_task = NULL; if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; } return frame;\n}");
    true
}

struct AsyncCfgBuilder<'a> {
    nodes: Vec<Option<AsyncCfgNode>>,
    ctx: EmitCtx<'a>,
    locals: HashMap<String, String>,
    cfg_locals: Vec<(String, String)>,
    owned_class_catches: Vec<(String, String)>,
    match_bindings: Vec<(String, String)>,
    catch_context: Option<AsyncCatchContext>,
    finally_states: BTreeSet<usize>,
    cancel_finally_states: HashMap<usize, usize>,
    in_finally: bool,
    active_finally_state: Option<usize>,
    return_key: String,
    supported: bool,
}

#[derive(Clone)]
struct AsyncCatchContext {
    catch_name: String,
    catch_key: String,
    catch_state: usize,
    failure_state: Option<usize>,
    finally_state: Option<usize>,
}

impl<'a> AsyncCfgBuilder<'a> {
    fn new(ctx: EmitCtx<'a>, locals: HashMap<String, String>, return_key: String) -> Self {
        Self {
            nodes: Vec::new(),
            ctx,
            locals,
            cfg_locals: Vec::new(),
            owned_class_catches: Vec::new(),
            match_bindings: Vec::new(),
            catch_context: None,
            finally_states: BTreeSet::new(),
            cancel_finally_states: HashMap::new(),
            in_finally: false,
            active_finally_state: None,
            return_key,
            supported: true,
        }
    }

    fn alloc(&mut self) -> usize {
        let state = self.nodes.len();
        self.nodes.push(None);
        state
    }

    fn finish(&mut self, state: usize, node: AsyncCfgNode) {
        if self.in_finally {
            self.finally_states.insert(state);
        } else if let Some(finally_state) = self.active_finally_state {
            self.cancel_finally_states.insert(state, finally_state);
        }
        self.nodes[state] = Some(node);
    }

    fn emit_finally_block(
        &mut self,
        stmts: &[Stmt],
        next: usize,
        break_state: Option<usize>,
        continue_state: Option<usize>,
    ) -> usize {
        let was_in_finally = self.in_finally;
        self.in_finally = true;
        let entry = self.emit_block(stmts, next, break_state, continue_state);
        self.in_finally = was_in_finally;
        entry
    }

    fn emit_block(
        &mut self,
        stmts: &[Stmt],
        next: usize,
        break_state: Option<usize>,
        continue_state: Option<usize>,
    ) -> usize {
        let mut entry = next;
        for stmt in stmts.iter().rev() {
            entry = self.emit_stmt(stmt, entry, break_state, continue_state);
        }
        entry
    }

    /// Lower an expression containing one or more awaits into a chain of
    /// typed frame slots and post-resume actions. The recursive continuation
    /// keeps already-evaluated operands from being re-run after suspension.
    fn emit_async_expr_assignment(
        &mut self,
        target: &str,
        target_key: &str,
        expr: &Expr,
        next: usize,
    ) -> Option<usize> {
        if let Expr::If(if_expr) = expr {
            if expr_contains_async(expr) {
                return self.emit_async_expr_if_assignment(target, target_key, if_expr, next);
            }
        }
        if !expr_contains_async(expr) {
            if !async_cfg_value_supported(target_key, self.ctx.checked) {
                return None;
            }
            let value = coerce_expr(expr, target_key, &mut self.ctx);
            let code = async_cfg_assignment_code(
                target,
                target_key,
                expr,
                &value,
                target_key == "String" && crate::expr::string_expr_is_owned_temp(expr, &self.ctx),
                self.ctx.checked,
            );
            let state = self.alloc();
            self.finish(state, AsyncCfgNode::Action { code, next });
            return Some(state);
        }

        let await_name = format!("__aura_async_expr_{}", self.cfg_locals.len());
        let (await_expr, rewritten) = split_single_await_expr(expr, &await_name)?;
        let await_key = infer_type_name(
            &Expr::Async(AsyncExpr::Await(await_expr.clone())),
            &self.ctx,
        );
        if !async_cfg_value_supported(&await_key, self.ctx.checked) {
            return None;
        }
        self.locals.insert(await_name.clone(), await_key.clone());
        self.cfg_locals
            .push((await_name.clone(), await_key.clone()));
        self.ctx.define_local(&await_name, await_key.clone());
        let continuation = self.emit_async_expr_assignment(target, target_key, &rewritten, next)?;
        let operand = emit_expr(&await_expr.operand, &mut self.ctx);
        let owns_task = await_operand_is_temporary(&await_expr.operand, self.ctx.checked);
        let state = self.alloc();
        let node = if let Some(catch) = self.catch_context.clone() {
            AsyncCfgNode::AwaitCatchValue {
                value: mangle_ident(&await_name),
                value_key: await_key,
                operand,
                owns_task,
                catch_name: mangle_ident(&catch.catch_name),
                catch_key: catch.catch_key,
                catch_state: catch.catch_state,
                failure_state: catch.failure_state,
                finally_state: catch.finally_state,
                next: continuation,
            }
        } else {
            AsyncCfgNode::Await {
                value: mangle_ident(&await_name),
                value_key: await_key,
                operand,
                owns_task,
                next: continuation,
            }
        };
        self.finish(state, node);
        Some(state)
    }

    fn emit_async_expr_if_assignment(
        &mut self,
        target: &str,
        target_key: &str,
        if_expr: &IfExpr,
        next: usize,
    ) -> Option<usize> {
        if target_key == "Unit" {
            return None;
        }
        let branch_entry = |builder: &mut Self, block: &Block| -> Option<usize> {
            let Stmt::Expr(value) = block.stmts.last()? else {
                return None;
            };
            let entry = builder.emit_async_expr_assignment(target, target_key, value, next)?;
            Some(builder.emit_block(&block.stmts[..block.stmts.len() - 1], entry, None, None))
        };
        let then_state = branch_entry(self, &if_expr.then_block)?;
        let else_state = branch_entry(self, &if_expr.else_block)?;
        let condition_key = infer_type_name(&if_expr.cond, &self.ctx);
        if condition_key != "Bool" {
            return None;
        }
        if !expr_contains_async(&if_expr.cond) {
            let condition = match &if_expr.cond {
                Expr::Ident(id) => format!("data->{}", mangle_ident(&id.name)),
                _ => emit_expr(&if_expr.cond, &mut self.ctx),
            };
            let state = self.alloc();
            self.finish(
                state,
                AsyncCfgNode::Branch {
                    condition,
                    then_state,
                    else_state,
                },
            );
            return Some(state);
        }
        let condition_name = format!("__aura_async_expr_if_cond_{}", self.cfg_locals.len());
        self.locals.insert(condition_name.clone(), "Bool".into());
        self.cfg_locals
            .push((condition_name.clone(), "Bool".into()));
        self.ctx.define_local(&condition_name, "Bool".into());
        let branch_state = self.alloc();
        self.finish(
            branch_state,
            AsyncCfgNode::Branch {
                condition: format!("data->{}", mangle_ident(&condition_name)),
                then_state,
                else_state,
            },
        );
        self.emit_async_expr_assignment(&condition_name, "Bool", &if_expr.cond, branch_state)
    }

    fn emit_async_expr_discard(&mut self, expr: &Expr, next: usize) -> Option<usize> {
        if !expr_contains_async(expr) {
            let code = emit_expr(expr, &mut self.ctx);
            let state = self.alloc();
            self.finish(
                state,
                AsyncCfgNode::Action {
                    code: format!("(void)({code});"),
                    next,
                },
            );
            return Some(state);
        }

        let await_name = format!("__aura_async_discard_{}", self.cfg_locals.len());
        let (await_expr, rewritten) = split_single_await_expr(expr, &await_name)?;
        let await_key = infer_type_name(
            &Expr::Async(AsyncExpr::Await(await_expr.clone())),
            &self.ctx,
        );
        if !async_cfg_value_supported(&await_key, self.ctx.checked) {
            return None;
        }
        self.locals.insert(await_name.clone(), await_key.clone());
        self.cfg_locals
            .push((await_name.clone(), await_key.clone()));
        self.ctx.define_local(&await_name, await_key.clone());
        let continuation = self.emit_async_expr_discard(&rewritten, next)?;
        let operand = emit_expr(&await_expr.operand, &mut self.ctx);
        let owns_task = await_operand_is_temporary(&await_expr.operand, self.ctx.checked);
        let state = self.alloc();
        let node = if let Some(catch) = self.catch_context.clone() {
            AsyncCfgNode::AwaitCatchValue {
                value: mangle_ident(&await_name),
                value_key: await_key,
                operand,
                owns_task,
                catch_name: mangle_ident(&catch.catch_name),
                catch_key: catch.catch_key,
                catch_state: catch.catch_state,
                failure_state: catch.failure_state,
                finally_state: catch.finally_state,
                next: continuation,
            }
        } else {
            AsyncCfgNode::Await {
                value: mangle_ident(&await_name),
                value_key: await_key,
                operand,
                owns_task,
                next: continuation,
            }
        };
        self.finish(state, node);
        Some(state)
    }

    fn emit_stmt(
        &mut self,
        stmt: &Stmt,
        next: usize,
        break_state: Option<usize>,
        continue_state: Option<usize>,
    ) -> usize {
        if !self.supported {
            return next;
        }
        match stmt {
            Stmt::Var(var) => {
                let name = var.name.name.clone();
                let Some(key) = self.locals.get(&name).cloned() else {
                    self.supported = false;
                    return next;
                };
                if let Expr::Async(AsyncExpr::Await(await_expr)) = &var.init {
                    if !async_cfg_value_supported(&key, self.ctx.checked)
                        || expr_contains_async(&await_expr.operand)
                        || !self.locals.contains_key(&name)
                    {
                        self.supported = false;
                        return next;
                    }
                    let operand = emit_expr(&await_expr.operand, &mut self.ctx);
                    let state = self.alloc();
                    let owns_task =
                        await_operand_is_temporary(&await_expr.operand, self.ctx.checked);
                    let node = if let Some(catch) = self.catch_context.clone() {
                        AsyncCfgNode::AwaitCatchValue {
                            value: mangle_ident(&name),
                            value_key: key,
                            operand,
                            owns_task,
                            catch_name: mangle_ident(&catch.catch_name),
                            catch_key: catch.catch_key,
                            catch_state: catch.catch_state,
                            failure_state: catch.failure_state,
                            finally_state: catch.finally_state,
                            next,
                        }
                    } else {
                        AsyncCfgNode::Await {
                            value: mangle_ident(&name),
                            value_key: key,
                            operand,
                            owns_task,
                            next,
                        }
                    };
                    self.finish(state, node);
                    state
                } else if matches!(var.init, Expr::Async(_)) {
                    if !async_cfg_value_supported(&key, self.ctx.checked) {
                        self.supported = false;
                        return next;
                    }
                    let init = coerce_expr(&var.init, &key, &mut self.ctx);
                    let code = async_cfg_assignment_code(
                        &name,
                        &key,
                        &var.init,
                        &init,
                        false,
                        self.ctx.checked,
                    );
                    let state = self.alloc();
                    self.finish(state, AsyncCfgNode::Action { code, next });
                    state
                } else if expr_contains_async(&var.init) {
                    let Some(state) = self.emit_async_expr_assignment(&name, &key, &var.init, next)
                    else {
                        self.supported = false;
                        return next;
                    };
                    state
                } else {
                    if expr_contains_async(&var.init)
                        || !async_cfg_value_supported(&key, self.ctx.checked)
                    {
                        self.supported = false;
                        return next;
                    }
                    let init = coerce_expr(&var.init, &key, &mut self.ctx);
                    let string_is_owned = key == "String"
                        && crate::expr::string_expr_is_owned_temp(&var.init, &self.ctx);
                    let code = async_cfg_assignment_code(
                        &name,
                        &key,
                        &var.init,
                        &init,
                        string_is_owned,
                        self.ctx.checked,
                    );
                    let state = self.alloc();
                    self.finish(state, AsyncCfgNode::Action { code, next });
                    state
                }
            }
            Stmt::If(branch) => {
                let then_state =
                    self.emit_block(&branch.then_block.stmts, next, break_state, continue_state);
                let else_state = branch
                    .else_block
                    .as_ref()
                    .map(|block| self.emit_block(&block.stmts, next, break_state, continue_state))
                    .unwrap_or(next);
                if let Expr::Async(AsyncExpr::Await(await_expr)) = &branch.cond {
                    if expr_contains_async(&await_expr.operand) {
                        self.supported = false;
                        return next;
                    }
                    let condition_key = infer_type_name(&branch.cond, &self.ctx);
                    if condition_key != "Bool" {
                        self.supported = false;
                        return next;
                    }
                    let condition_name = format!("__aura_async_cond_{}", self.cfg_locals.len());
                    self.locals
                        .insert(condition_name.clone(), condition_key.clone());
                    self.cfg_locals
                        .push((condition_name.clone(), condition_key.clone()));
                    self.ctx
                        .define_local(&condition_name, condition_key.clone());
                    let branch_state = self.alloc();
                    self.finish(
                        branch_state,
                        AsyncCfgNode::Branch {
                            condition: format!("data->{}", mangle_ident(&condition_name)),
                            then_state,
                            else_state,
                        },
                    );
                    let operand = emit_expr(&await_expr.operand, &mut self.ctx);
                    let state = self.alloc();
                    let owns_task =
                        await_operand_is_temporary(&await_expr.operand, self.ctx.checked);
                    let node = if let Some(catch) = self.catch_context.clone() {
                        AsyncCfgNode::AwaitCatchValue {
                            value: mangle_ident(&condition_name),
                            value_key: condition_key,
                            operand,
                            owns_task,
                            catch_name: mangle_ident(&catch.catch_name),
                            catch_key: catch.catch_key,
                            catch_state: catch.catch_state,
                            failure_state: catch.failure_state,
                            finally_state: catch.finally_state,
                            next: branch_state,
                        }
                    } else {
                        AsyncCfgNode::Await {
                            value: mangle_ident(&condition_name),
                            value_key: condition_key,
                            operand,
                            owns_task,
                            next: branch_state,
                        }
                    };
                    self.finish(state, node);
                    state
                } else {
                    if expr_contains_async(&branch.cond) {
                        self.supported = false;
                        return next;
                    }
                    let condition = emit_expr(&branch.cond, &mut self.ctx);
                    let state = self.alloc();
                    self.finish(
                        state,
                        AsyncCfgNode::Branch {
                            condition,
                            then_state,
                            else_state,
                        },
                    );
                    state
                }
            }
            Stmt::While(loop_stmt) => {
                if let Expr::Async(AsyncExpr::Await(await_expr)) = &loop_stmt.cond {
                    if expr_contains_async(&await_expr.operand)
                        || infer_type_name(&loop_stmt.cond, &self.ctx) != "Bool"
                    {
                        self.supported = false;
                        return next;
                    }
                    let condition_name = format!("__aura_async_cond_{}", self.cfg_locals.len());
                    self.locals.insert(condition_name.clone(), "Bool".into());
                    self.cfg_locals
                        .push((condition_name.clone(), "Bool".into()));
                    self.ctx.define_local(&condition_name, "Bool".into());

                    let branch_state = self.alloc();
                    let await_state = self.alloc();
                    let body_state = self.emit_block(
                        &loop_stmt.body.stmts,
                        await_state,
                        Some(next),
                        Some(await_state),
                    );
                    self.finish(
                        branch_state,
                        AsyncCfgNode::Branch {
                            condition: format!("data->{}", mangle_ident(&condition_name)),
                            then_state: body_state,
                            else_state: next,
                        },
                    );
                    let operand = emit_expr(&await_expr.operand, &mut self.ctx);
                    let owns_task =
                        await_operand_is_temporary(&await_expr.operand, self.ctx.checked);
                    let node = if let Some(catch) = self.catch_context.clone() {
                        AsyncCfgNode::AwaitCatchValue {
                            value: mangle_ident(&condition_name),
                            value_key: "Bool".into(),
                            operand,
                            owns_task,
                            catch_name: mangle_ident(&catch.catch_name),
                            catch_key: catch.catch_key,
                            catch_state: catch.catch_state,
                            failure_state: catch.failure_state,
                            finally_state: catch.finally_state,
                            next: branch_state,
                        }
                    } else {
                        AsyncCfgNode::Await {
                            value: mangle_ident(&condition_name),
                            value_key: "Bool".into(),
                            operand,
                            owns_task,
                            next: branch_state,
                        }
                    };
                    self.finish(await_state, node);
                    await_state
                } else {
                    if expr_contains_async(&loop_stmt.cond) {
                        self.supported = false;
                        return next;
                    }
                    let condition_state = self.alloc();
                    let body_state = self.emit_block(
                        &loop_stmt.body.stmts,
                        condition_state,
                        Some(next),
                        Some(condition_state),
                    );
                    let condition = emit_expr(&loop_stmt.cond, &mut self.ctx);
                    self.finish(
                        condition_state,
                        AsyncCfgNode::Branch {
                            condition,
                            then_state: body_state,
                            else_state: next,
                        },
                    );
                    condition_state
                }
            }
            Stmt::ForRange(range) => {
                if expr_contains_async(&range.start)
                    || expr_contains_async(&range.end)
                    || self.locals.contains_key(&range.name.name)
                {
                    self.supported = false;
                    return next;
                }
                let iterator = range.name.name.clone();
                let bound = format!("__aura_range_end_{}", self.cfg_locals.len());
                self.locals.insert(iterator.clone(), "Int".into());
                self.locals.insert(bound.clone(), "Int".into());
                self.cfg_locals.push((iterator.clone(), "Int".into()));
                self.cfg_locals.push((bound.clone(), "Int".into()));
                self.ctx.define_local(&iterator, "Int".into());
                self.ctx.define_local(&bound, "Int".into());

                let condition_state = self.alloc();
                let increment_state = self.alloc();
                let body_state = self.emit_block(
                    &range.body.stmts,
                    increment_state,
                    Some(next),
                    Some(increment_state),
                );
                let comparison = if range.inclusive {
                    format!("({iterator} <= {bound})")
                } else {
                    format!("({iterator} < {bound})")
                };
                self.finish(
                    condition_state,
                    AsyncCfgNode::Branch {
                        condition: comparison,
                        then_state: body_state,
                        else_state: next,
                    },
                );
                self.finish(
                    increment_state,
                    AsyncCfgNode::Action {
                        code: format!("{iterator} = {iterator} + INT64_C(1);"),
                        next: condition_state,
                    },
                );

                let end = coerce_expr(&range.end, "Int", &mut self.ctx);
                let end_state = self.alloc();
                self.finish(
                    end_state,
                    AsyncCfgNode::Action {
                        code: format!("{bound} = {end};"),
                        next: condition_state,
                    },
                );
                let start = coerce_expr(&range.start, "Int", &mut self.ctx);
                let start_state = self.alloc();
                self.finish(
                    start_state,
                    AsyncCfgNode::Action {
                        code: format!("{iterator} = {start};"),
                        next: end_state,
                    },
                );
                start_state
            }
            Stmt::ForIn(for_in) => {
                if expr_contains_async(&for_in.iterable)
                    || self.locals.contains_key(&for_in.name.name)
                {
                    self.supported = false;
                    return next;
                }
                let iterable_key = infer_type_name(&for_in.iterable, &self.ctx);
                let iterable_key = if iterable_key == "Array" {
                    "Array_Int".to_string()
                } else {
                    full_type_mono(&iterable_key, self.ctx.checked)
                };
                let string_iterable = iterable_key == "String";
                let interface_iterable = is_iface_type_key(&iterable_key, self.ctx.checked);
                if !is_array_type_key(&iterable_key) && !string_iterable && !interface_iterable {
                    self.supported = false;
                    return next;
                }
                let slot = self.cfg_locals.len();
                let iterator = format!("__aura_for_iter_{slot}");
                let index = format!("__aura_for_index_{slot}");
                let binding = for_in.name.name.clone();
                let (interface_mono, interface_elem_key) = if interface_iterable {
                    let interface_mono = resolve_iface_mono_key(&iterable_key, self.ctx.checked);
                    let (iface, args) =
                        resolve_iface_decl_and_args(&iterable_key, self.ctx.checked);
                    let (iface, args) = if iface.is_some() {
                        (iface, args)
                    } else {
                        resolve_iface_decl_and_args(&interface_mono, self.ctx.checked)
                    };
                    let tparams = iface
                        .map(|i| {
                            i.type_params
                                .iter()
                                .map(|p| p.name.name.clone())
                                .collect::<Vec<String>>()
                        })
                        .unwrap_or_default();
                    let elem_key = iface
                        .and_then(|i| {
                            i.methods
                                .iter()
                                .find(|m| m.name.name == "get")
                                .and_then(|m| m.return_type.as_ref())
                                .map(|rt| type_ref_local_key(rt, &tparams, &args))
                        })
                        .unwrap_or_else(|| "Int".into());
                    (Some(interface_mono), elem_key)
                } else {
                    (None, String::new())
                };
                let elem_key = if string_iterable {
                    "Int"
                } else if interface_iterable {
                    interface_elem_key.as_str()
                } else if let Some(elem_key) = iterable_key.strip_prefix("Array_") {
                    elem_key
                } else {
                    self.supported = false;
                    return next;
                };
                if !async_cfg_value_supported(elem_key, self.ctx.checked) {
                    self.supported = false;
                    return next;
                }
                if self.locals.contains_key(&binding) {
                    self.supported = false;
                    return next;
                }
                self.locals.insert(binding.clone(), elem_key.to_string());
                self.cfg_locals
                    .push((iterator.clone(), iterable_key.clone()));
                self.cfg_locals.push((index.clone(), "Int".into()));
                self.cfg_locals.push((binding.clone(), elem_key.into()));
                self.ctx
                    .define_local(&iterator, full_type_mono(&iterable_key, self.ctx.checked));
                self.ctx.define_local(&index, "Int".into());
                self.ctx.define_local(&binding, elem_key.into());

                let condition_state = self.alloc();
                let increment_state = self.alloc();
                let body_state = self.emit_block(
                    &for_in.body.stmts,
                    increment_state,
                    Some(next),
                    Some(increment_state),
                );
                let bind_state = self.alloc();
                let get_fn = crate::names::c_method_name(&iterable_key, "get");
                let (condition, bind_expr) = if string_iterable {
                    (
                        format!("({index} < strlen({iterator}))"),
                        format!("(unsigned char){iterator}[{index}]"),
                    )
                } else if interface_iterable {
                    let interface_mono = interface_mono.as_deref().expect("interface mono");
                    (
                        format!(
                            "({index} < {}(&{iterator}))",
                            c_iface_method_name(interface_mono, "len")
                        ),
                        format!(
                            "{}(&{iterator}, {index})",
                            c_iface_method_name(interface_mono, "get")
                        ),
                    )
                } else {
                    (
                        format!("({index} < {iterator}.len)"),
                        format!("{get_fn}(&{iterator}, {index})"),
                    )
                };
                let bind_code = if elem_key == "String" {
                    format!(
                        "if ({binding}__owned && {binding} != NULL) free((void *){binding}); {binding} = {bind_expr}; {binding}__owned = true;"
                    )
                } else if is_array_type_key(elem_key) {
                    let cty = crate::stmt::local_key_to_c(elem_key, self.ctx.checked);
                    let clone = crate::names::c_method_name(elem_key, "clone");
                    let free = crate::array_emit::array_contents_free_expr(&binding, elem_key);
                    format!(
                        "{cty} __for_value = {bind_expr}; {free} {binding} = {clone}(&__for_value);"
                    )
                } else if crate::expr::is_enum_mono(elem_key, self.ctx.checked)
                    || crate::expr::is_value_struct_mono(elem_key, self.ctx.checked)
                    || is_iface_type_key(elem_key, self.ctx.checked)
                {
                    let cty = crate::stmt::local_key_to_c(elem_key, self.ctx.checked);
                    let clone = format!("{cty}_clone");
                    let drop = format!("{cty}_drop");
                    format!(
                        "{cty} __for_value = {bind_expr}; {drop}(&{binding}); {binding} = {clone}(&__for_value);"
                    )
                } else if is_fun_type_key(elem_key) {
                    format!(
                        "{{ {cty} __for_value = {bind_expr}; if ({binding}.env != NULL) aura_fun_env_free({binding}.env); {binding} = __for_value; if ({binding}.env != NULL) aura_fun_env_retain({binding}.env); }}",
                        cty = crate::stmt::local_key_to_c(elem_key, self.ctx.checked),
                    )
                } else if elem_key == "ForeignHandle" || elem_key.starts_with("ForeignHandle_") {
                    format!(
                        "{{ {cty} __for_value = {bind_expr}; if (__for_value != NULL && aura_ffi_handle_retain(__for_value) != AURA_FFI_OK) return AURA_TASK_FAILED; if ({binding} != NULL) (void)aura_ffi_handle_drop(&{binding}); {binding} = __for_value; }}",
                        cty = crate::stmt::local_key_to_c(elem_key, self.ctx.checked),
                    )
                } else {
                    format!("{binding} = {bind_expr};")
                };
                self.finish(
                    bind_state,
                    AsyncCfgNode::Action {
                        code: bind_code,
                        next: body_state,
                    },
                );
                self.finish(
                    condition_state,
                    AsyncCfgNode::Branch {
                        condition,
                        then_state: bind_state,
                        else_state: next,
                    },
                );
                self.finish(
                    increment_state,
                    AsyncCfgNode::Action {
                        code: format!("{index} = {index} + INT64_C(1);"),
                        next: condition_state,
                    },
                );
                let init = coerce_expr(&for_in.iterable, &iterable_key, &mut self.ctx);
                let init_code = if is_array_type_key(&iterable_key)
                    && async_cfg_move_source(&for_in.iterable).is_some()
                {
                    // Keep the source array alive across the async call; the
                    // frame owns and destroys this independent iterator copy.
                    let clone = crate::names::c_method_name(&iterable_key, "clone");
                    format!("{iterator} = {clone}(&({init}));")
                } else if string_iterable {
                    format!("{iterator} = {init};")
                } else {
                    async_cfg_assignment_code(
                        &iterator,
                        &iterable_key,
                        &for_in.iterable,
                        &init,
                        false,
                        self.ctx.checked,
                    )
                };
                let index_init_state = self.alloc();
                self.finish(
                    index_init_state,
                    AsyncCfgNode::Action {
                        code: format!("{index} = INT64_C(0);"),
                        next: condition_state,
                    },
                );
                let init_state = self.alloc();
                self.finish(
                    init_state,
                    AsyncCfgNode::Action {
                        code: init_code,
                        next: index_init_state,
                    },
                );
                init_state
            }
            Stmt::Match(m) => {
                if expr_contains_async(&m.scrutinee) || m.arms.is_empty() {
                    self.supported = false;
                    return next;
                }
                let Some(scrutinee_ident) = async_cfg_move_source(&m.scrutinee) else {
                    self.supported = false;
                    return next;
                };
                let scrutinee = mangle_ident(&scrutinee_ident.name);
                let scrutinee_key = infer_type_name(&m.scrutinee, &self.ctx);
                let enum_name = mono_base_name(&scrutinee_key, self.ctx.checked)
                    .or_else(|| {
                        if is_enum_name(self.ctx.checked, &scrutinee_key) {
                            Some(scrutinee_key.as_str())
                        } else {
                            self.ctx
                                .checked
                                .mono_enums
                                .iter()
                                .find(|(name, args)| mono_key(name, args) == scrutinee_key)
                                .map(|(name, _)| name.as_str())
                        }
                    })
                    .map(str::to_owned);
                let Some(enum_name) = enum_name else {
                    self.supported = false;
                    return next;
                };
                let Some(enum_decl) = self
                    .ctx
                    .checked
                    .ast
                    .enums
                    .iter()
                    .find(|decl| decl.name.name == enum_name)
                    .cloned()
                else {
                    self.supported = false;
                    return next;
                };
                let enum_params: Vec<String> = enum_decl
                    .type_params
                    .iter()
                    .map(|param| param.name.name.clone())
                    .collect();
                let enum_args: Vec<Ty> = mono_split(&scrutinee_key, self.ctx.checked)
                    .map(|(_, args)| args.to_vec())
                    .or_else(|| {
                        self.ctx
                            .checked
                            .mono_enums
                            .iter()
                            .find(|(name, args)| mono_key(name, args) == scrutinee_key)
                            .map(|(_, args)| args.clone())
                    })
                    .unwrap_or_default();
                let mut arm_state = next;
                for arm in m.arms.iter().rev() {
                    let Pattern::Variant { name, bindings, .. } = &arm.pattern;
                    let Some(tag) = enum_decl
                        .variants
                        .iter()
                        .position(|variant| variant.name.name == name.name)
                    else {
                        self.supported = false;
                        return next;
                    };
                    let Some(variant) = enum_decl
                        .variants
                        .iter()
                        .find(|variant| variant.name.name == name.name)
                    else {
                        self.supported = false;
                        return next;
                    };
                    if bindings.len() != variant.fields.len() {
                        self.supported = false;
                        return next;
                    }
                    let mut bind_code = Vec::new();
                    for (binding, field) in bindings.iter().zip(&variant.fields) {
                        let key = full_type_mono(
                            &type_ref_local_key_expand(
                                &field.ty,
                                &enum_params,
                                &enum_args,
                                self.ctx.checked,
                            ),
                            self.ctx.checked,
                        );
                        if !async_cfg_value_supported(&key, self.ctx.checked) {
                            self.supported = false;
                            return next;
                        }
                        if let Some(existing) = self.locals.get(&binding.name) {
                            if existing != &key {
                                self.supported = false;
                                return next;
                            }
                        } else {
                            self.locals.insert(binding.name.clone(), key.clone());
                            self.ctx.define_local(&binding.name, key.clone());
                            self.match_bindings
                                .push((binding.name.clone(), key.clone()));
                        }
                        let binding_name = mangle_ident(&binding.name);
                        let field_name = format!(
                            "{}.data.{}.{}",
                            scrutinee,
                            mangle_ident(&variant.name.name),
                            mangle_ident(&field.name.name)
                        );
                        if key == "String" {
                            bind_code.push(format!(
                                "if ({binding_name}__owned && {binding_name} != NULL) free((void *){binding_name}); {binding_name} = NULL; {binding_name}__owned = false; if ({field_name} != NULL) {{ size_t __match_len = strlen({field_name}); {binding_name} = (char *)malloc(__match_len + 1); if ({binding_name} == NULL) return AURA_TASK_FAILED; memcpy((void *){binding_name}, {field_name}, __match_len + 1); {binding_name}__owned = true; }}"
                            ));
                        } else if is_array_type_key(&key) {
                            let cty = crate::stmt::local_key_to_c(&key, self.ctx.checked);
                            let clone = crate::names::c_method_name(&key, "clone");
                            let mut free_code = String::new();
                            crate::array_emit::emit_array_contents_free(
                                &mut free_code,
                                0,
                                &binding_name,
                                &key,
                            );
                            bind_code.push(format!(
                                "{cty} __match_value = {field_name}; {free_code} {binding_name} = {clone}(&__match_value);"
                            ));
                        } else if crate::expr::is_enum_mono(&key, self.ctx.checked)
                            || crate::expr::is_value_struct_mono(&key, self.ctx.checked)
                            || is_iface_type_key(&key, self.ctx.checked)
                        {
                            let cty = crate::stmt::local_key_to_c(&key, self.ctx.checked);
                            bind_code.push(format!(
                                "{cty} __match_value = {field_name}; {cty} __match_copy = {cty}_clone(&__match_value); {cty}_drop(&{binding_name}); {binding_name} = __match_copy;"
                            ));
                        } else if is_fun_type_key(&key) {
                            bind_code.push(format!(
                                "if ({binding_name}.env != NULL) aura_fun_env_free({binding_name}.env); {binding_name} = {field_name}; if ({binding_name}.env != NULL) aura_fun_env_retain({binding_name}.env);"
                            ));
                        } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
                            bind_code.push(format!(
                                "if ({field_name} != NULL && aura_ffi_handle_retain({field_name}) != AURA_FFI_OK) return AURA_TASK_FAILED; if ({binding_name} != NULL) (void)aura_ffi_handle_drop(&{binding_name}); {binding_name} = {field_name};"
                            ));
                        } else if is_heap_class_mono(&key, self.ctx.checked) {
                            bind_code.push(format!(
                                // Async CFG match bindings are mirrored in the
                                // task frame. Root the frame slot, never the
                                // poller's stack-local copy: the latter dies
                                // when the poll returns at an await.
                                "if (data->{binding_name} != NULL) aura_gc_remove_root((void **)&data->{binding_name}); data->{binding_name} = {field_name}; {binding_name} = data->{binding_name}; if (data->{binding_name} != NULL) aura_gc_add_root((void **)&data->{binding_name});"
                            ));
                        } else {
                            bind_code.push(format!("{binding_name} = {field_name};"));
                        }
                    }
                    let body_state =
                        self.emit_block(&arm.body.stmts, next, break_state, continue_state);
                    let then_state = if bind_code.is_empty() {
                        body_state
                    } else {
                        let state = self.alloc();
                        self.finish(
                            state,
                            AsyncCfgNode::Action {
                                code: bind_code.join(" "),
                                next: body_state,
                            },
                        );
                        state
                    };
                    let branch_state = self.alloc();
                    self.finish(
                        branch_state,
                        AsyncCfgNode::Branch {
                            condition: format!("({scrutinee}).tag == {tag}"),
                            then_state,
                            else_state: arm_state,
                        },
                    );
                    arm_state = branch_state;
                }
                arm_state
            }
            Stmt::Try(try_stmt) => {
                // A `setjmp`-based catch cannot survive an async suspension.
                // Support the bounded task-error shape explicitly: a single
                // awaited expression with a String catch binding and no
                // finally block. Other forms remain on the deferred path.
                let Some(catch) = &try_stmt.catch else {
                    if try_stmt.finally.is_none() {
                        self.supported = false;
                        return next;
                    }
                    if try_stmt.try_block.stmts.len() == 1 {
                        if let Stmt::Expr(Expr::Async(AsyncExpr::Await(await_expr))) =
                            &try_stmt.try_block.stmts[0]
                        {
                            if !expr_contains_async(&await_expr.operand) {
                                let failure_state = self.alloc();
                                self.finish(failure_state, AsyncCfgNode::Fail);
                                let after_finally = self.alloc();
                                let cancelled_state = self.alloc();
                                if let Some(outer_finally_state) = self.active_finally_state {
                                    // Cancellation handled by an inner finally must
                                    // continue through every enclosing finally before
                                    // the task publishes AURA_TASK_CANCELLED.
                                    self.finish(
                                        cancelled_state,
                                        AsyncCfgNode::Action {
                                            code: "data->await_cancelled = true;".into(),
                                            next: outer_finally_state,
                                        },
                                    );
                                } else {
                                    self.finish(cancelled_state, AsyncCfgNode::Cancel);
                                }
                                let after_failure = self.alloc();
                                self.finish(
                                    after_failure,
                                    AsyncCfgNode::Branch {
                                        condition: "data->await_failed".into(),
                                        then_state: failure_state,
                                        else_state: next,
                                    },
                                );
                                self.finish(
                                    after_finally,
                                    AsyncCfgNode::Branch {
                                        condition: "data->await_cancelled".into(),
                                        then_state: cancelled_state,
                                        else_state: after_failure,
                                    },
                                );
                                let finally_state = self.emit_finally_block(
                                    &try_stmt.finally.as_ref().expect("checked above").stmts,
                                    after_finally,
                                    break_state,
                                    continue_state,
                                );
                                let operand = emit_expr(&await_expr.operand, &mut self.ctx);
                                let state = self.alloc();
                                self.finish(
                                    state,
                                    AsyncCfgNode::AwaitFinally {
                                        operand,
                                        owns_task: await_operand_is_temporary(
                                            &await_expr.operand,
                                            self.ctx.checked,
                                        ),
                                        finally_state,
                                        next,
                                    },
                                );
                                return state;
                            }
                        }
                    }
                    let finally_state = self.emit_finally_block(
                        &try_stmt.finally.as_ref().expect("checked above").stmts,
                        next,
                        break_state,
                        continue_state,
                    );
                    let previous_finally = self.active_finally_state.replace(finally_state);
                    let entry = self.emit_block(
                        &try_stmt.try_block.stmts,
                        finally_state,
                        break_state,
                        continue_state,
                    );
                    self.active_finally_state = previous_finally;
                    return entry;
                };
                let catch_key = type_ref_local_key(&catch.ty, &[], &[]);
                if !async_cfg_catch_supported(&catch_key, self.ctx.checked) {
                    self.supported = false;
                    return next;
                }
                if try_stmt.try_block.stmts.is_empty() {
                    self.supported = false;
                    return next;
                }
                let catch_name = catch.name.name.clone();
                // Catch bindings are scoped to the handler. Give each one a
                // distinct frame slot so sequential handlers may reuse the
                // same source name with different payload types.
                let catch_storage = format!("__aura_async_catch_{}", self.cfg_locals.len());
                let previous_key = self.locals.insert(catch_name.clone(), catch_key.clone());
                let previous_c_name = self
                    .ctx
                    .local_c_names
                    .insert(catch_name.clone(), catch_storage.clone());
                self.ctx.define_local(&catch_name, catch_key.clone());
                self.cfg_locals
                    .push((catch_storage.clone(), catch_key.clone()));
                if is_heap_class_mono(&catch_key, self.ctx.checked)
                    && !self
                        .owned_class_catches
                        .iter()
                        .any(|(name, _)| name == &catch_storage)
                {
                    self.owned_class_catches
                        .push((catch_storage.clone(), catch_key.clone()));
                }
                let (catch_state_next, failure_state, finally_state) = if let Some(finally) =
                    &try_stmt.finally
                {
                    let failure_state = self.alloc();
                    let fail_state = self.alloc();
                    self.finish(fail_state, AsyncCfgNode::Fail);
                    self.finish(
                        failure_state,
                        AsyncCfgNode::Action {
                            code: "if (data->await_task != NULL) { (void)aura_task_frame_propagate_error(frame, data->await_task); if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; }".into(),
                            next: fail_state,
                        },
                    );
                    let cancelled_state = self.alloc();
                    self.finish(cancelled_state, AsyncCfgNode::Cancel);
                    let after_failure = self.alloc();
                    self.finish(
                        after_failure,
                        AsyncCfgNode::Branch {
                            condition: "data->await_failed".into(),
                            then_state: failure_state,
                            else_state: next,
                        },
                    );
                    let after_finally = self.alloc();
                    self.finish(
                        after_finally,
                        AsyncCfgNode::Branch {
                            condition: "data->await_cancelled".into(),
                            then_state: cancelled_state,
                            else_state: after_failure,
                        },
                    );
                    let finally_state = self.emit_finally_block(
                        &finally.stmts,
                        after_finally,
                        break_state,
                        continue_state,
                    );
                    (finally_state, Some(failure_state), Some(finally_state))
                } else {
                    (next, None, None)
                };

                // Emit the catch continuation without the inner context: a
                // failure in a catch body belongs to the surrounding try.
                let outer_catch = self.catch_context.clone();
                self.catch_context = outer_catch.clone();
                let previous_active_finally = self.active_finally_state;
                if let Some(finally_state) = finally_state {
                    self.active_finally_state = Some(finally_state);
                }
                let catch_state = self.emit_block(
                    &catch.body.stmts,
                    catch_state_next,
                    break_state,
                    continue_state,
                );
                self.active_finally_state = previous_active_finally;
                if let Some(previous) = previous_key {
                    self.locals.insert(catch_name.clone(), previous);
                } else {
                    self.locals.remove(&catch_name);
                }
                if let Some(previous) = previous_c_name {
                    self.ctx.local_c_names.insert(catch_name.clone(), previous);
                } else {
                    self.ctx.local_c_names.remove(&catch_name);
                }

                // Every await reached in the protected CFG, including awaits
                // nested under branches and loops, targets this continuation.
                self.catch_context = Some(AsyncCatchContext {
                    catch_name: catch_storage,
                    catch_key,
                    catch_state,
                    failure_state,
                    finally_state,
                });
                let previous_active_finally = self.active_finally_state;
                if let Some(finally_state) = finally_state {
                    self.active_finally_state = Some(finally_state);
                }
                let protected_entry = self.emit_block(
                    &try_stmt.try_block.stmts,
                    catch_state_next,
                    break_state,
                    continue_state,
                );
                self.active_finally_state = previous_active_finally;
                self.catch_context = outer_catch;
                protected_entry
            }
            Stmt::Break(_) => break_state.unwrap_or_else(|| {
                self.supported = false;
                next
            }),
            Stmt::Continue(_) => continue_state.unwrap_or_else(|| {
                self.supported = false;
                next
            }),
            Stmt::Throw(throw_stmt) => {
                if expr_contains_async(&throw_stmt.value) {
                    self.supported = false;
                    return next;
                }
                let value_key = infer_type_name(&throw_stmt.value, &self.ctx);
                if (!matches!(value_key.as_str(), "Int" | "Bool" | "String")
                    && !is_array_type_key(&value_key)
                    && !value_key.starts_with("ForeignHandle")
                    && !async_cfg_throw_aggregate_supported(&value_key, self.ctx.checked)
                    && !async_cfg_throw_class_supported(&value_key, self.ctx.checked))
                    || (value_key == "String"
                        && crate::expr::string_expr_is_owned_temp(&throw_stmt.value, &self.ctx))
                {
                    self.supported = false;
                    return next;
                }
                let value = emit_expr(&throw_stmt.value, &mut self.ctx);
                let state = self.alloc();
                self.finish(
                    state,
                    AsyncCfgNode::Throw {
                        value,
                        value_key,
                        span_start: throw_stmt.span.start,
                        span_end: throw_stmt.span.end,
                    },
                );
                state
            }
            Stmt::Return(ret) => {
                if self.return_key == "Unit" && ret.value.is_none() {
                    let state = self.alloc();
                    self.finish(
                        state,
                        AsyncCfgNode::Return {
                            value: "0".into(),
                            value_key: self.return_key.clone(),
                            value_is_ident: false,
                            value_is_owned_temp: false,
                        },
                    );
                    return state;
                }
                let Some(value) = &ret.value else {
                    self.supported = false;
                    return next;
                };
                if expr_contains_async(value) {
                    if self.return_key == "Unit" {
                        let Expr::Async(AsyncExpr::Await(await_expr)) = value else {
                            self.supported = false;
                            return next;
                        };
                        if expr_contains_async(&await_expr.operand) {
                            self.supported = false;
                            return next;
                        }
                        let return_state = self.alloc();
                        self.finish(
                            return_state,
                            AsyncCfgNode::Return {
                                value: "0".into(),
                                value_key: self.return_key.clone(),
                                value_is_ident: false,
                                value_is_owned_temp: false,
                            },
                        );
                        let operand = emit_expr(&await_expr.operand, &mut self.ctx);
                        let state = self.alloc();
                        let owns_task =
                            await_operand_is_temporary(&await_expr.operand, self.ctx.checked);
                        let node = if let Some(catch) = self.catch_context.clone() {
                            AsyncCfgNode::AwaitCatch {
                                operand,
                                owns_task,
                                catch_name: mangle_ident(&catch.catch_name),
                                catch_key: catch.catch_key,
                                catch_state: catch.catch_state,
                                failure_state: catch.failure_state,
                                finally_state: catch.finally_state,
                                next: return_state,
                            }
                        } else {
                            AsyncCfgNode::AwaitUnit {
                                operand,
                                owns_task,
                                next: return_state,
                            }
                        };
                        self.finish(state, node);
                        return state;
                    }
                    if !async_cfg_value_supported(&self.return_key, self.ctx.checked) {
                        self.supported = false;
                        return next;
                    }
                    let name = format!("__aura_async_return_{}", self.cfg_locals.len());
                    self.locals.insert(name.clone(), self.return_key.clone());
                    self.cfg_locals
                        .push((name.clone(), self.return_key.clone()));
                    self.ctx.define_local(&name, self.return_key.clone());
                    let return_state = self.alloc();
                    self.finish(
                        return_state,
                        AsyncCfgNode::Return {
                            value: mangle_ident(&name),
                            value_key: self.return_key.clone(),
                            value_is_ident: true,
                            value_is_owned_temp: false,
                        },
                    );
                    let Some(state) = self.emit_async_expr_assignment(
                        &name,
                        &self.return_key.clone(),
                        value,
                        return_state,
                    ) else {
                        self.supported = false;
                        return next;
                    };
                    return state;
                }
                let value_is_owned_temp = self.return_key == "String"
                    && crate::expr::string_expr_is_owned_temp(value, &self.ctx);
                let value = coerce_expr(value, &self.return_key, &mut self.ctx);
                let state = self.alloc();
                self.finish(
                    state,
                    AsyncCfgNode::Return {
                        value,
                        value_key: self.return_key.clone(),
                        value_is_ident: matches!(ret.value.as_ref(), Some(Expr::Ident(_))),
                        value_is_owned_temp,
                    },
                );
                state
            }
            Stmt::Expr(Expr::Assign(assign)) => {
                let Some(key) = self.locals.get(&assign.name.name).cloned() else {
                    self.supported = false;
                    return next;
                };
                if !async_cfg_value_supported(&key, self.ctx.checked) {
                    self.supported = false;
                    return next;
                }
                if let Expr::Async(AsyncExpr::Await(await_expr)) = assign.value.as_ref() {
                    if expr_contains_async(&await_expr.operand) {
                        self.supported = false;
                        return next;
                    }
                    let operand = emit_expr(&await_expr.operand, &mut self.ctx);
                    let state = self.alloc();
                    let owns_task =
                        await_operand_is_temporary(&await_expr.operand, self.ctx.checked);
                    let node = if let Some(catch) = self.catch_context.clone() {
                        AsyncCfgNode::AwaitCatchValue {
                            value: mangle_ident(&assign.name.name),
                            value_key: key,
                            operand,
                            owns_task,
                            catch_name: mangle_ident(&catch.catch_name),
                            catch_key: catch.catch_key,
                            catch_state: catch.catch_state,
                            failure_state: catch.failure_state,
                            finally_state: catch.finally_state,
                            next,
                        }
                    } else {
                        AsyncCfgNode::Await {
                            value: mangle_ident(&assign.name.name),
                            value_key: key,
                            operand,
                            owns_task,
                            next,
                        }
                    };
                    self.finish(state, node);
                    return state;
                }
                if expr_contains_async(&assign.value) {
                    let Some(state) = self.emit_async_expr_assignment(
                        &assign.name.name,
                        &key,
                        &assign.value,
                        next,
                    ) else {
                        self.supported = false;
                        return next;
                    };
                    return state;
                }
                let value = coerce_expr(&assign.value, &key, &mut self.ctx);
                let code = if self.ctx.is_box_local(&assign.name.name)
                    && matches!(key.as_str(), "Int" | "Bool")
                {
                    format!("({})->value = {value};", mangle_ident(&assign.name.name))
                } else if self.ctx.is_box_local(&assign.name.name) && key == "String" {
                    let box_name = mangle_ident(&assign.name.name);
                    if crate::expr::string_expr_is_owned_temp(&assign.value, &self.ctx) {
                        format!(
                            "{{ const char *__cfg_string = {value}; aura_box_str_set({box_name}, __cfg_string); free((void *)__cfg_string); }}"
                        )
                    } else {
                        format!("aura_box_str_set({box_name}, {value});")
                    }
                } else {
                    async_cfg_assignment_code(
                        &self.ctx.local_c_name(&assign.name.name),
                        &key,
                        &assign.value,
                        &value,
                        key == "String"
                            && crate::expr::string_expr_is_owned_temp(&assign.value, &self.ctx),
                        self.ctx.checked,
                    )
                };
                let state = self.alloc();
                self.finish(state, AsyncCfgNode::Action { code, next });
                state
            }
            Stmt::Expr(expr) if is_gc_collect_expr(expr) => {
                let code = emit_expr(expr, &mut self.ctx);
                let state = self.alloc();
                self.finish(
                    state,
                    AsyncCfgNode::Action {
                        code: format!("(void)({code});"),
                        next,
                    },
                );
                state
            }
            Stmt::Expr(Expr::Async(AsyncExpr::Await(await_expr))) => {
                if expr_contains_async(&await_expr.operand) {
                    self.supported = false;
                    return next;
                }
                let operand = emit_expr(&await_expr.operand, &mut self.ctx);
                let state = self.alloc();
                let owns_task = await_operand_is_temporary(&await_expr.operand, self.ctx.checked);
                let node = if let Some(catch) = self.catch_context.clone() {
                    AsyncCfgNode::AwaitCatch {
                        operand,
                        owns_task,
                        catch_name: mangle_ident(&catch.catch_name),
                        catch_key: catch.catch_key,
                        catch_state: catch.catch_state,
                        failure_state: catch.failure_state,
                        finally_state: catch.finally_state,
                        next,
                    }
                } else {
                    AsyncCfgNode::AwaitUnit {
                        operand,
                        owns_task,
                        next,
                    }
                };
                self.finish(state, node);
                state
            }
            Stmt::Expr(expr @ Expr::Async(AsyncExpr::Cancel(_)))
            | Stmt::Expr(expr @ Expr::Async(AsyncExpr::Join(_)))
            | Stmt::Expr(expr @ Expr::Async(AsyncExpr::ChannelCreate(_)))
            | Stmt::Expr(expr @ Expr::Async(AsyncExpr::ChannelSend(_)))
            | Stmt::Expr(expr @ Expr::Async(AsyncExpr::ChannelReceive(_)))
            | Stmt::Expr(expr @ Expr::Async(AsyncExpr::ChannelClose(_))) => {
                let code = emit_expr(expr, &mut self.ctx);
                let state = self.alloc();
                self.finish(
                    state,
                    AsyncCfgNode::Action {
                        code: format!("{code};"),
                        next,
                    },
                );
                state
            }
            Stmt::Expr(expr) => {
                if expr_contains_async(expr) {
                    let key = infer_type_name(expr, &self.ctx);
                    if key == "Unit" {
                        let Some(state) = self.emit_async_expr_discard(expr, next) else {
                            self.supported = false;
                            return next;
                        };
                        return state;
                    }
                    if !async_cfg_value_supported(&key, self.ctx.checked) {
                        self.supported = false;
                        return next;
                    }
                    let name = format!("__aura_async_discard_value_{}", self.cfg_locals.len());
                    self.locals.insert(name.clone(), key.clone());
                    self.cfg_locals.push((name.clone(), key.clone()));
                    self.ctx.define_local(&name, key.clone());
                    let Some(state) = self.emit_async_expr_assignment(&name, &key, expr, next)
                    else {
                        self.supported = false;
                        return next;
                    };
                    return state;
                }
                let code = emit_expr(expr, &mut self.ctx);
                let state = self.alloc();
                self.finish(
                    state,
                    AsyncCfgNode::Action {
                        code: format!("{code};"),
                        next,
                    },
                );
                state
            }
        }
    }
}

fn async_cfg_value_supported(key: &str, checked: &CheckedFile) -> bool {
    matches!(
        key,
        "Unit" | "Int" | "Bool" | "String" | "Opt_Int" | "Opt_Bool" | "ForeignHandle"
    ) || key.starts_with("ForeignHandle_")
        || async_cfg_scheduler_owned_key(key)
        || is_array_type_key(key)
        || is_iface_type_key(key, checked)
        || is_fun_type_key(key)
        || is_heap_class_mono(key, checked)
        || crate::expr::is_value_struct_mono(key, checked)
        || mono_base_name(key, checked).is_some_and(|base| is_enum_name(checked, base))
}

fn async_cfg_scheduler_owned_key(key: &str) -> bool {
    key == "Task"
        || key.starts_with("Task_")
        || key == "TaskHandle"
        || key.starts_with("TaskHandle_")
        || key == "Channel"
        || key.starts_with("Channel_")
}

/// Extract one scalar await from an expression and replace it with a frame
/// local. The continuation can then evaluate the surrounding expression after
/// suspension without re-running the async operand.
fn split_single_await_expr(expr: &Expr, replacement: &str) -> Option<(AwaitExpr, Expr)> {
    match expr {
        Expr::Async(AsyncExpr::Await(await_expr)) => Some((
            await_expr.clone(),
            Expr::Ident(Ident {
                name: replacement.to_string(),
                span: await_expr.span,
            }),
        )),
        Expr::Binary(binary) => {
            if expr_contains_async(&binary.left) {
                let (await_expr, left) = split_single_await_expr(&binary.left, replacement)?;
                Some((
                    await_expr,
                    Expr::Binary(BinaryExpr {
                        op: binary.op,
                        left: Box::new(left),
                        right: binary.right.clone(),
                        span: binary.span,
                    }),
                ))
            } else if expr_contains_async(&binary.right) {
                let (await_expr, right) = split_single_await_expr(&binary.right, replacement)?;
                Some((
                    await_expr,
                    Expr::Binary(BinaryExpr {
                        op: binary.op,
                        left: binary.left.clone(),
                        right: Box::new(right),
                        span: binary.span,
                    }),
                ))
            } else {
                None
            }
        }
        Expr::Unary(unary) if expr_contains_async(&unary.expr) => {
            let (await_expr, inner) = split_single_await_expr(&unary.expr, replacement)?;
            Some((
                await_expr,
                Expr::Unary(UnaryExpr {
                    op: unary.op,
                    expr: Box::new(inner),
                    span: unary.span,
                }),
            ))
        }
        Expr::Group(inner, span) if expr_contains_async(inner) => {
            let (await_expr, inner) = split_single_await_expr(inner, replacement)?;
            Some((await_expr, Expr::Group(Box::new(inner), *span)))
        }
        Expr::ForceUnwrap(force) if expr_contains_async(&force.expr) => {
            let (await_expr, inner) = split_single_await_expr(&force.expr, replacement)?;
            Some((
                await_expr,
                Expr::ForceUnwrap(ForceUnwrapExpr {
                    expr: Box::new(inner),
                    span: force.span,
                }),
            ))
        }
        // Async operations can themselves contain an awaited operand (for
        // example `join(await make_handle())`). Preserve the operation while
        // lifting only the innermost await into the CFG continuation.
        Expr::Async(async_expr) => match async_expr {
            AsyncExpr::Await(await_expr) => {
                let (inner_await, operand) =
                    split_single_await_expr(&await_expr.operand, replacement)?;
                Some((
                    inner_await,
                    Expr::Async(AsyncExpr::Await(AwaitExpr {
                        operand: Box::new(operand),
                        span: await_expr.span,
                    })),
                ))
            }
            AsyncExpr::Join(join) if expr_contains_async(&join.handle) => {
                let (await_expr, handle) = split_single_await_expr(&join.handle, replacement)?;
                Some((
                    await_expr,
                    Expr::Async(AsyncExpr::Join(JoinExpr {
                        handle: Box::new(handle),
                        span: join.span,
                    })),
                ))
            }
            AsyncExpr::Cancel(cancel) if expr_contains_async(&cancel.handle) => {
                let (await_expr, handle) = split_single_await_expr(&cancel.handle, replacement)?;
                Some((
                    await_expr,
                    Expr::Async(AsyncExpr::Cancel(CancelExpr {
                        handle: Box::new(handle),
                        span: cancel.span,
                    })),
                ))
            }
            AsyncExpr::ChannelCreate(create) if expr_contains_async(&create.capacity) => {
                let (await_expr, capacity) =
                    split_single_await_expr(&create.capacity, replacement)?;
                Some((
                    await_expr,
                    Expr::Async(AsyncExpr::ChannelCreate(ChannelCreateExpr {
                        element_type: create.element_type.clone(),
                        capacity: Box::new(capacity),
                        span: create.span,
                    })),
                ))
            }
            AsyncExpr::ChannelSend(send) if expr_contains_async(&send.channel) => {
                let (await_expr, channel) = split_single_await_expr(&send.channel, replacement)?;
                Some((
                    await_expr,
                    Expr::Async(AsyncExpr::ChannelSend(ChannelSendExpr {
                        channel: Box::new(channel),
                        value: send.value.clone(),
                        span: send.span,
                    })),
                ))
            }
            AsyncExpr::ChannelSend(send) if expr_contains_async(&send.value) => {
                let (await_expr, value) = split_single_await_expr(&send.value, replacement)?;
                Some((
                    await_expr,
                    Expr::Async(AsyncExpr::ChannelSend(ChannelSendExpr {
                        channel: send.channel.clone(),
                        value: Box::new(value),
                        span: send.span,
                    })),
                ))
            }
            AsyncExpr::ChannelReceive(receive) if expr_contains_async(&receive.channel) => {
                let (await_expr, channel) = split_single_await_expr(&receive.channel, replacement)?;
                Some((
                    await_expr,
                    Expr::Async(AsyncExpr::ChannelReceive(ChannelReceiveExpr {
                        channel: Box::new(channel),
                        span: receive.span,
                    })),
                ))
            }
            AsyncExpr::ChannelClose(close) if expr_contains_async(&close.channel) => {
                let (await_expr, channel) = split_single_await_expr(&close.channel, replacement)?;
                Some((
                    await_expr,
                    Expr::Async(AsyncExpr::ChannelClose(ChannelCloseExpr {
                        channel: Box::new(channel),
                        span: close.span,
                    })),
                ))
            }
            // A nested spawn owns a separate body/frame; its awaits must be
            // lowered by that child task rather than the enclosing expression.
            AsyncExpr::Spawn(_)
            | AsyncExpr::Join(_)
            | AsyncExpr::Cancel(_)
            | AsyncExpr::ChannelCreate(_)
            | AsyncExpr::ChannelSend(_)
            | AsyncExpr::ChannelReceive(_)
            | AsyncExpr::ChannelClose(_) => None,
        },
        Expr::Field(field) if expr_contains_async(&field.object) => {
            let (await_expr, object) = split_single_await_expr(&field.object, replacement)?;
            Some((
                await_expr,
                Expr::Field(FieldExpr {
                    object: Box::new(object),
                    field: field.field.clone(),
                    safe: field.safe,
                    span: field.span,
                }),
            ))
        }
        Expr::Is(is_expr) if expr_contains_async(&is_expr.expr) => {
            let (await_expr, value) = split_single_await_expr(&is_expr.expr, replacement)?;
            Some((
                await_expr,
                Expr::Is(IsExpr {
                    expr: Box::new(value),
                    ty: is_expr.ty.clone(),
                    span: is_expr.span,
                }),
            ))
        }
        Expr::If(if_expr) if expr_contains_async(&if_expr.cond) => {
            let (await_expr, cond) = split_single_await_expr(&if_expr.cond, replacement)?;
            Some((
                await_expr,
                Expr::If(Box::new(IfExpr {
                    cond,
                    then_block: if_expr.then_block.clone(),
                    else_block: if_expr.else_block.clone(),
                    span: if_expr.span,
                })),
            ))
        }
        Expr::Call(call) => {
            if expr_contains_async(&call.callee) {
                let (await_expr, callee) = split_single_await_expr(&call.callee, replacement)?;
                Some((
                    await_expr,
                    Expr::Call(CallExpr {
                        callee: Box::new(callee),
                        type_args: call.type_args.clone(),
                        args: call.args.clone(),
                        span: call.span,
                    }),
                ))
            } else {
                let index = call.args.iter().position(expr_contains_async)?;
                let (await_expr, arg) = split_single_await_expr(&call.args[index], replacement)?;
                let mut args = call.args.clone();
                args[index] = arg;
                Some((
                    await_expr,
                    Expr::Call(CallExpr {
                        callee: call.callee.clone(),
                        type_args: call.type_args.clone(),
                        args,
                        span: call.span,
                    }),
                ))
            }
        }
        Expr::Assign(assign) if expr_contains_async(&assign.value) => {
            let (await_expr, value) = split_single_await_expr(&assign.value, replacement)?;
            Some((
                await_expr,
                Expr::Assign(AssignExpr {
                    name: assign.name.clone(),
                    value: Box::new(value),
                    span: assign.span,
                }),
            ))
        }
        _ => None,
    }
}

fn async_cfg_catch_supported(key: &str, checked: &CheckedFile) -> bool {
    matches!(key, "String" | "Int" | "Bool" | "ForeignHandle")
        || key.starts_with("ForeignHandle_")
        || is_array_type_key(key)
        || async_cfg_throw_aggregate_supported(key, checked)
        || async_cfg_throw_class_supported(key, checked)
}

fn async_cfg_foreign_handle_catch_body(catch_name: &str) -> String {
    format!(
        "AuraTaskResult __payload = aura_task_frame_error_payload(data->await_task); if (__payload.data == NULL || __payload.size != sizeof(AuraFfiOpaqueHandle *)) return AURA_TASK_FAILED; AuraFfiOpaqueHandle *__source = *((AuraFfiOpaqueHandle **)__payload.data); if (__source != NULL && aura_ffi_handle_retain(__source) != AURA_FFI_OK) return AURA_TASK_FAILED; if (data->{catch_name} != NULL) (void)aura_ffi_handle_drop(&data->{catch_name}); data->{catch_name} = __source; {catch_name} = data->{catch_name};"
    )
}

fn async_cfg_throw_aggregate_supported(key: &str, checked: &CheckedFile) -> bool {
    (crate::expr::is_enum_mono(key, checked)
        || mono_base_name(key, checked).is_some_and(|base| is_enum_name(checked, base)))
        || crate::expr::is_value_struct_mono(key, checked)
        || is_iface_type_key(key, checked)
        || is_fun_type_key(key)
}

fn async_cfg_aggregate_catch_body(
    catch_key: &str,
    catch_name: &str,
    checked: &CheckedFile,
) -> String {
    let cty = crate::stmt::local_key_to_c(catch_key, checked);
    let source = format!("({cty} *)__payload.data");
    let cleanup = if is_fun_type_key(catch_key) {
        format!("if (data->{catch_name}.env != NULL) aura_fun_env_free(data->{catch_name}.env);")
    } else {
        format!("{cty}_drop(&data->{catch_name});")
    };
    let clone = if is_fun_type_key(catch_key) {
        format!("__copy = *{source}; if (__copy.env != NULL) aura_fun_env_retain(__copy.env);")
    } else {
        format!("__copy = {cty}_clone({source});")
    };
    format!(
        "AuraTaskResult __payload = aura_task_frame_error_payload(data->await_task); if (__payload.data == NULL) {{ (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }} {cty} *__source = {source}; {cty} __copy; {clone} {cleanup} data->{catch_name} = __copy; {catch_name} = data->{catch_name};"
    )
}

fn async_cfg_class_catch_body(catch_key: &str, catch_name: &str, checked: &CheckedFile) -> String {
    let mono = full_type_mono(catch_key, checked);
    let Some(base) = mono_base_name(&mono, checked) else {
        return String::new();
    };
    let Some(class) = checked
        .ast
        .classes
        .iter()
        .find(|class| class.name.name == base && class.type_params.is_empty())
    else {
        return String::new();
    };
    let cty = c_class_type(&mono);
    let dtor = format!("aura_ex_dtor_{mono}");
    let params: Vec<String> = class
        .type_params
        .iter()
        .map(|param| param.name.name.clone())
        .collect();
    let array_fields = ownership_fields(class, checked, &params, &[])
        .into_iter()
        .filter_map(|(name, key)| {
            is_array_type_key(&full_type_mono(&key, checked)).then_some(mangle_ident(&name))
        })
        .collect::<Vec<_>>();
    let old_array_root_cleanup = array_fields
        .iter()
        .map(|name| {
            format!(
                "if (data->{catch_name} != NULL) aura_gc_remove_array_root((void **)&data->{catch_name}->{name}.data);"
            )
        })
        .collect::<String>();
    let mut code = format!(
        "AuraTaskResult __payload = aura_task_frame_error_payload(data->await_task); if (__payload.data == NULL) {{ (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }} {cty} *__source = ({cty} *)__payload.data; {cty} *__copy = ({cty} *)malloc(sizeof(*__copy)); if (__copy == NULL) return AURA_TASK_FAILED; *__copy = *__source; if (data->{catch_name} != NULL) {{ {old_array_root_cleanup} {dtor}(data->{catch_name}); }} data->{catch_name} = __copy; {catch_name} = data->{catch_name};"
    );
    for (field_name, field_key) in ownership_fields(class, checked, &params, &[]) {
        let name = mangle_ident(&field_name);
        let full_key = full_type_mono(&field_key, checked);
        if field_key == "String" {
            code.push_str(&format!(
                "if (__source->{name} != NULL) {{ size_t __len_{name} = strlen(__source->{name}); char *__text_{name} = (char *)malloc(__len_{name} + 1); if (__text_{name} == NULL) {{ {dtor}(__copy); return AURA_TASK_FAILED; }} memcpy(__text_{name}, __source->{name}, __len_{name} + 1); __copy->{name} = __text_{name}; }}"
            ));
        } else if is_array_type_key(&full_key) {
            let clone = crate::names::c_method_name(&full_key, "clone");
            let root = crate::array_emit::array_gc_root_add_call(
                &format!("__copy->{name}.data"),
                &format!("__copy->{name}.len"),
                &full_key,
                checked,
            );
            code.push_str(&format!(
                "__copy->{name} = {clone}(&__source->{name}); {root}"
            ));
        } else if crate::expr::is_enum_mono(&full_key, checked) {
            let cty = crate::stmt::local_key_to_c(&full_key, checked);
            code.push_str(&format!("__copy->{name} = {cty}_clone(&__source->{name});"));
        } else if crate::expr::is_value_struct_mono(&full_key, checked) {
            let cty = crate::stmt::local_key_to_c(&full_key, checked);
            code.push_str(&format!("__copy->{name} = {cty}_clone(&__source->{name});"));
        }
    }
    code
}

fn async_cfg_task_supported(key: &str, checked: &CheckedFile) -> bool {
    key.strip_prefix("Task_")
        .is_some_and(|value| async_cfg_value_supported(value, checked))
}

fn async_cfg_throw_class_supported(key: &str, checked: &CheckedFile) -> bool {
    if !is_heap_class_mono(key, checked) {
        return false;
    }
    let Some(base) = mono_base_name(key, checked) else {
        return false;
    };
    let Some(_class) = checked
        .ast
        .classes
        .iter()
        .find(|class| class.name.name == base && class.type_params.is_empty())
    else {
        return false;
    };
    // Class exception payloads use the same ownership-aware clone/destroy
    // helpers as ordinary async results, including Array-valued fields.
    true
}

fn async_cfg_assignment_code(
    name: &str,
    key: &str,
    init: &Expr,
    value: &str,
    string_is_owned: bool,
    checked: &CheckedFile,
) -> String {
    let name = mangle_ident(name);
    if key == "String" {
        if let Some(source) = async_cfg_move_source(init) {
            let source = mangle_ident(&source.name);
            if source != name {
                return format!(
                    "if ({name}__owned && {name} != NULL) free((void *){name}); {name} = {source}; {name}__owned = {source}__owned; {source} = NULL; {source}__owned = false;"
                );
            }
        }
        return format!(
            "if ({name}__owned && {name} != NULL) free((void *){name}); {name} = {value}; {name}__owned = {};",
            if string_is_owned { "true" } else { "false" }
        );
    }
    if is_array_type_key(key) {
        let cty = crate::stmt::local_key_to_c(key, checked);
        let cleanup = crate::array_emit::array_contents_free_expr(&name, key);
        if let Some(source) = async_cfg_move_source(init) {
            let source = mangle_ident(&source.name);
            if source != name {
                return format!(
                    "{cty} __cfg_next = {source}; {cleanup} {name} = __cfg_next; {source} = ({cty}){{0}};"
                );
            }
        }
        return format!("{cty} __cfg_next = {value}; {cleanup} {name} = __cfg_next;");
    }
    if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
        if let Some(source) = async_cfg_move_source(init) {
            let source = mangle_ident(&source.name);
            if source != name {
                return format!(
                    "if ({name} != NULL) (void)aura_ffi_handle_drop(&{name}); {name} = {source}; {source} = NULL;"
                );
            }
        }
    }
    if is_fun_type_key(key) {
        let cty = crate::stmt::local_key_to_c(key, checked);
        if let Some(source) = async_cfg_move_source(init) {
            let source = mangle_ident(&source.name);
            if source != name {
                return format!(
                    "if ({name}.env != NULL) aura_fun_env_free({name}.env); {name} = {source}; {source} = ({cty}){{0}};"
                );
            }
        }
        let retain = matches!(init, Expr::Async(AsyncExpr::Await(_)));
        return format!(
            "if ({name}.env != NULL) aura_fun_env_free({name}.env); {name} = {value}; {}",
            if retain {
                format!("if ({name}.env != NULL) aura_fun_env_retain({name}.env);")
            } else {
                String::new()
            }
        );
    }
    if is_iface_type_key(key, checked) {
        let cty = crate::stmt::local_key_to_c(key, checked);
        let clone = format!("{cty}_clone");
        return format!(
            "{cty} __cfg_source = {value}; {cty} __cfg_next = {clone}(&__cfg_source); {cty}_drop(&{name}); {name} = __cfg_next;"
        );
    }
    if async_cfg_task_supported(key, checked) || key == "Task" {
        // A task returned by a call carries the executor's lexical handle
        // reference. Store an independent payload reference in the frame,
        // then consume that temporary handle so frame teardown can safely
        // release the payload after the scheduler has reclaimed the child.
        if let Some(source) = async_cfg_move_source(init) {
            let source = mangle_ident(&source.name);
            if source != name {
                return format!(
                    "if ({name} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &{name}); {name} = {source}; {source} = NULL;"
                );
            }
        }
        return format!(
            "{{ AuraTaskFrame *__cfg_next = {value}; if (__cfg_next != NULL && (__aura_task_executor == NULL || !aura_task_executor_retain_payload(__aura_task_executor, __cfg_next))) return AURA_TASK_FAILED; if ({name} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &{name}); {name} = __cfg_next; if (__cfg_next != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &__cfg_next); }}"
        );
    }
    if crate::expr::is_enum_mono(key, checked) {
        let cty = crate::stmt::local_key_to_c(key, checked);
        let clone = format!("{cty}_clone");
        let drop = format!("{cty}_drop");
        return format!(
            "{cty} __cfg_source = {value}; {cty} __cfg_next = {clone}(&__cfg_source); {drop}(&{name}); {name} = __cfg_next;"
        );
    }
    if crate::expr::is_value_struct_mono(key, checked) {
        let cty = crate::stmt::local_key_to_c(key, checked);
        return format!(
            "{cty} __cfg_source = {value}; {cty} __cfg_next = {cty}_clone(&__cfg_source); {cty}_drop(&{name}); {name} = __cfg_next;"
        );
    }
    format!("{name} = {value};")
}

fn async_cfg_move_source(expr: &Expr) -> Option<&Ident> {
    match expr {
        Expr::Ident(id) => Some(id),
        Expr::Group(inner, _) => async_cfg_move_source(inner),
        Expr::ForceUnwrap(ForceUnwrapExpr { expr: inner, .. }) => async_cfg_move_source(inner),
        _ => None,
    }
}

pub(crate) fn emit_owned_value_cleanup(
    out: &mut String,
    indent: usize,
    expr: &str,
    key: &str,
    checked: &CheckedFile,
) {
    if key.contains("Outcome_String") && key.contains("std_error_Error") {
        let pad = "  ".repeat(indent);
        let _ = writeln!(
            out,
            "{pad}if ({expr}.tag == 0 && {expr}.data.OutcomeOk.owned && {expr}.data.OutcomeOk.value != NULL) {{ free((void *){expr}.data.OutcomeOk.value); {expr}.data.OutcomeOk.value = NULL; {expr}.data.OutcomeOk.owned = false; }}"
        );
        let _ = writeln!(
            out,
            "{pad}if ({expr}.tag == 1 && {expr}.data.OutcomeErr.owned && {expr}.data.OutcomeErr.error != NULL) {{ aura_gc_remove_root((void **)&{expr}.data.OutcomeErr.error); {expr}.data.OutcomeErr.error = NULL; {expr}.data.OutcomeErr.owned = false; }}"
        );
        return;
    }
    if is_array_type_key(key) {
        crate::array_emit::emit_array_contents_free_checked(out, indent, expr, key, checked);
        return;
    }
    let Some(base) = crate::expr::mono_base_name(key, checked) else {
        return;
    };
    let Some(enum_decl) = checked.ast.enums.iter().find(|e| e.name.name == base) else {
        return;
    };
    let pad = "  ".repeat(indent);
    for (tag, variant) in enum_decl.variants.iter().enumerate() {
        for field in &variant.fields {
            let field_key = type_ref_local_key_expand(&field.ty, &[], &[], checked);
            let field_expr = format!(
                "{expr}.data.{}.{}",
                mangle_ident(&variant.name.name),
                mangle_ident(&field.name.name)
            );
            if is_array_type_key(&field_key) {
                let _ = writeln!(out, "{pad}if ({expr}.tag == {tag}) {{");
                crate::array_emit::emit_array_contents_free(
                    out,
                    indent + 1,
                    &field_expr,
                    &field_key,
                );
                let _ = writeln!(out, "{pad}}}");
            }
        }
    }
}

fn is_gc_collect_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Call(call)
            if call.args.is_empty()
                && matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
    )
}

fn expr_contains_async(expr: &Expr) -> bool {
    match expr {
        Expr::Async(_) => true,
        Expr::Binary(binary) => {
            expr_contains_async(&binary.left) || expr_contains_async(&binary.right)
        }
        Expr::Unary(unary) => expr_contains_async(&unary.expr),
        Expr::Group(inner, _) | Expr::ForceUnwrap(ForceUnwrapExpr { expr: inner, .. }) => {
            expr_contains_async(inner)
        }
        Expr::Call(call) => {
            expr_contains_async(&call.callee) || call.args.iter().any(expr_contains_async)
        }
        Expr::Field(field) => expr_contains_async(&field.object),
        Expr::Assign(assign) => expr_contains_async(&assign.value),
        Expr::Is(is_expr) => expr_contains_async(&is_expr.expr),
        Expr::If(if_expr) => {
            expr_contains_async(&if_expr.cond)
                || if_expr.then_block.stmts.iter().any(stmt_contains_async)
                || if_expr.else_block.stmts.iter().any(stmt_contains_async)
        }
        _ => false,
    }
}

/// Give nested bindings distinct source-level names before async CFG lowering.
/// The CFG stores locals in a frame-wide layout, so retaining two lexical
/// bindings with the same spelling would alias their suspended values.
fn alpha_rename_async_block(block: &Block, inherited: &HashMap<String, String>) -> (Block, bool) {
    fn expr_has_lambda(expr: &Expr) -> bool {
        match expr {
            Expr::Lambda(_) => true,
            Expr::Call(call) => {
                expr_has_lambda(&call.callee) || call.args.iter().any(expr_has_lambda)
            }
            Expr::Field(field) => expr_has_lambda(&field.object),
            Expr::Assign(assign) => expr_has_lambda(&assign.value),
            Expr::Binary(binary) => expr_has_lambda(&binary.left) || expr_has_lambda(&binary.right),
            Expr::Unary(unary) => expr_has_lambda(&unary.expr),
            Expr::ForceUnwrap(force) => expr_has_lambda(&force.expr),
            Expr::Is(is_expr) => expr_has_lambda(&is_expr.expr),
            Expr::Group(inner, _) => expr_has_lambda(inner),
            Expr::If(if_expr) => {
                expr_has_lambda(&if_expr.cond)
                    || block_has_lambda(&if_expr.then_block)
                    || block_has_lambda(&if_expr.else_block)
            }
            Expr::Async(async_expr) => match async_expr {
                AsyncExpr::Spawn(spawn) => block_has_lambda(&spawn.body),
                AsyncExpr::Await(await_expr) => expr_has_lambda(&await_expr.operand),
                AsyncExpr::Join(join) => expr_has_lambda(&join.handle),
                AsyncExpr::Cancel(cancel) => expr_has_lambda(&cancel.handle),
                AsyncExpr::ChannelCreate(create) => expr_has_lambda(&create.capacity),
                AsyncExpr::ChannelSend(send) => {
                    expr_has_lambda(&send.channel) || expr_has_lambda(&send.value)
                }
                AsyncExpr::ChannelReceive(receive) => expr_has_lambda(&receive.channel),
                AsyncExpr::ChannelClose(close) => expr_has_lambda(&close.channel),
            },
            Expr::Ident(_)
            | Expr::This(_)
            | Expr::Int(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::Null(_) => false,
        }
    }

    fn stmt_has_lambda(stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Var(var) => expr_has_lambda(&var.init),
            Stmt::If(if_stmt) => {
                expr_has_lambda(&if_stmt.cond)
                    || block_has_lambda(&if_stmt.then_block)
                    || if_stmt.else_block.as_ref().is_some_and(block_has_lambda)
            }
            Stmt::While(while_stmt) => {
                expr_has_lambda(&while_stmt.cond) || block_has_lambda(&while_stmt.body)
            }
            Stmt::ForRange(range) => {
                expr_has_lambda(&range.start)
                    || expr_has_lambda(&range.end)
                    || block_has_lambda(&range.body)
            }
            Stmt::ForIn(for_in) => {
                expr_has_lambda(&for_in.iterable) || block_has_lambda(&for_in.body)
            }
            Stmt::Match(match_stmt) => {
                expr_has_lambda(&match_stmt.scrutinee)
                    || match_stmt
                        .arms
                        .iter()
                        .any(|arm| block_has_lambda(&arm.body))
            }
            Stmt::Try(try_stmt) => {
                block_has_lambda(&try_stmt.try_block)
                    || try_stmt
                        .catch
                        .as_ref()
                        .is_some_and(|catch| block_has_lambda(&catch.body))
                    || try_stmt.finally.as_ref().is_some_and(block_has_lambda)
            }
            Stmt::Throw(throw_stmt) => expr_has_lambda(&throw_stmt.value),
            Stmt::Return(ret) => ret.value.as_ref().is_some_and(expr_has_lambda),
            Stmt::Expr(expr) => expr_has_lambda(expr),
            Stmt::Break(_) | Stmt::Continue(_) => false,
        }
    }

    fn block_has_lambda(block: &Block) -> bool {
        block.stmts.iter().any(stmt_has_lambda)
    }

    // Lambda definitions are emitted from the checked, whole-file AST. Keep
    // them on the existing capture path until their lexical environment is
    // lowered into the same scoped-name table as async frames.
    if block_has_lambda(block) {
        return (block.clone(), false);
    }

    fn ident(name: &Ident, env: &HashMap<String, String>) -> Ident {
        let mut out = name.clone();
        if let Some(mapped) = env.get(&name.name) {
            out.name = mapped.clone();
        }
        out
    }

    fn fresh(name: &str, next: &mut usize) -> String {
        let value = format!("__aura_shadow_{}_{}", *next, mangle_ident(name));
        *next += 1;
        value
    }

    fn expr(
        value: &Expr,
        env: &HashMap<String, String>,
        next: &mut usize,
        changed: &mut bool,
    ) -> Expr {
        match value {
            Expr::Ident(id) => Expr::Ident(ident(id, env)),
            Expr::This(span) => Expr::This(*span),
            Expr::Int(v) => Expr::Int(v.clone()),
            Expr::Bool(v) => Expr::Bool(v.clone()),
            Expr::String(v) => Expr::String(v.clone()),
            Expr::Null(span) => Expr::Null(*span),
            Expr::Call(call) => Expr::Call(CallExpr {
                callee: Box::new(expr(&call.callee, env, next, changed)),
                type_args: call.type_args.clone(),
                args: call
                    .args
                    .iter()
                    .map(|arg| expr(arg, env, next, changed))
                    .collect(),
                span: call.span,
            }),
            Expr::Field(field) => Expr::Field(FieldExpr {
                object: Box::new(expr(&field.object, env, next, changed)),
                field: field.field.clone(),
                safe: field.safe,
                span: field.span,
            }),
            Expr::Assign(assign) => Expr::Assign(AssignExpr {
                name: ident(&assign.name, env),
                value: Box::new(expr(&assign.value, env, next, changed)),
                span: assign.span,
            }),
            Expr::Binary(binary) => Expr::Binary(BinaryExpr {
                op: binary.op,
                left: Box::new(expr(&binary.left, env, next, changed)),
                right: Box::new(expr(&binary.right, env, next, changed)),
                span: binary.span,
            }),
            Expr::Unary(unary) => Expr::Unary(UnaryExpr {
                op: unary.op,
                expr: Box::new(expr(&unary.expr, env, next, changed)),
                span: unary.span,
            }),
            Expr::ForceUnwrap(force) => Expr::ForceUnwrap(ForceUnwrapExpr {
                expr: Box::new(expr(&force.expr, env, next, changed)),
                span: force.span,
            }),
            Expr::Is(is_expr) => Expr::Is(IsExpr {
                expr: Box::new(expr(&is_expr.expr, env, next, changed)),
                ty: is_expr.ty.clone(),
                span: is_expr.span,
            }),
            Expr::Group(inner, span) => {
                Expr::Group(Box::new(expr(inner, env, next, changed)), *span)
            }
            Expr::If(if_expr) => Expr::If(Box::new(IfExpr {
                cond: expr(&if_expr.cond, env, next, changed),
                then_block: rename_block(&if_expr.then_block, env, next, changed),
                else_block: rename_block(&if_expr.else_block, env, next, changed),
                span: if_expr.span,
            })),
            // Lambda bodies are emitted once by `emit_lambda_fns`, outside
            // this async frame. Their capture fill uses `local_c_name`, so
            // renaming the enclosing frame here is sufficient; rewriting the
            // lambda AST would desynchronize it from the generated function.
            Expr::Lambda(lambda) => Expr::Lambda(lambda.clone()),
            Expr::Async(async_expr) => Expr::Async(match async_expr {
                AsyncExpr::Spawn(spawn) => AsyncExpr::Spawn(SpawnExpr {
                    body: rename_block(&spawn.body, env, next, changed),
                    span: spawn.span,
                }),
                AsyncExpr::Await(await_expr) => AsyncExpr::Await(AwaitExpr {
                    operand: Box::new(expr(&await_expr.operand, env, next, changed)),
                    span: await_expr.span,
                }),
                AsyncExpr::Join(join) => AsyncExpr::Join(JoinExpr {
                    handle: Box::new(expr(&join.handle, env, next, changed)),
                    span: join.span,
                }),
                AsyncExpr::Cancel(cancel) => AsyncExpr::Cancel(CancelExpr {
                    handle: Box::new(expr(&cancel.handle, env, next, changed)),
                    span: cancel.span,
                }),
                AsyncExpr::ChannelCreate(create) => AsyncExpr::ChannelCreate(ChannelCreateExpr {
                    element_type: create.element_type.clone(),
                    capacity: Box::new(expr(&create.capacity, env, next, changed)),
                    span: create.span,
                }),
                AsyncExpr::ChannelSend(send) => AsyncExpr::ChannelSend(ChannelSendExpr {
                    channel: Box::new(expr(&send.channel, env, next, changed)),
                    value: Box::new(expr(&send.value, env, next, changed)),
                    span: send.span,
                }),
                AsyncExpr::ChannelReceive(receive) => {
                    AsyncExpr::ChannelReceive(ChannelReceiveExpr {
                        channel: Box::new(expr(&receive.channel, env, next, changed)),
                        span: receive.span,
                    })
                }
                AsyncExpr::ChannelClose(close) => AsyncExpr::ChannelClose(ChannelCloseExpr {
                    channel: Box::new(expr(&close.channel, env, next, changed)),
                    span: close.span,
                }),
            }),
        }
    }

    fn rename_block(
        source: &Block,
        inherited: &HashMap<String, String>,
        next: &mut usize,
        changed: &mut bool,
    ) -> Block {
        let mut env = inherited.clone();
        let mut stmts = Vec::with_capacity(source.stmts.len());
        for item in &source.stmts {
            stmts.push(stmt(item, &mut env, next, changed));
        }
        Block {
            stmts,
            span: source.span,
        }
    }

    fn stmt(
        source: &Stmt,
        env: &mut HashMap<String, String>,
        next: &mut usize,
        changed: &mut bool,
    ) -> Stmt {
        match source {
            Stmt::Var(var) => {
                let init = expr(&var.init, env, next, changed);
                let mut name = var.name.clone();
                let source_name = name.name.clone();
                if env.contains_key(&source_name) {
                    name.name = fresh(&source_name, next);
                    *changed = true;
                }
                env.insert(source_name, name.name.clone());
                Stmt::Var(VarStmt {
                    mutable: var.mutable,
                    name,
                    ty: var.ty.clone(),
                    init,
                    span: var.span,
                })
            }
            Stmt::If(if_stmt) => Stmt::If(IfStmt {
                cond: expr(&if_stmt.cond, env, next, changed),
                then_block: rename_block(&if_stmt.then_block, env, next, changed),
                else_block: if_stmt
                    .else_block
                    .as_ref()
                    .map(|nested| rename_block(nested, env, next, changed)),
                span: if_stmt.span,
            }),
            Stmt::While(while_stmt) => Stmt::While(WhileStmt {
                cond: expr(&while_stmt.cond, env, next, changed),
                body: rename_block(&while_stmt.body, env, next, changed),
                span: while_stmt.span,
            }),
            Stmt::ForRange(range) => {
                let start = expr(&range.start, env, next, changed);
                let end = expr(&range.end, env, next, changed);
                let mut loop_env = env.clone();
                let mut name = range.name.clone();
                let source_name = name.name.clone();
                if loop_env.contains_key(&source_name) {
                    name.name = fresh(&source_name, next);
                    *changed = true;
                }
                loop_env.insert(source_name, name.name.clone());
                Stmt::ForRange(ForRangeStmt {
                    name,
                    start,
                    end,
                    inclusive: range.inclusive,
                    body: rename_block(&range.body, &loop_env, next, changed),
                    span: range.span,
                })
            }
            Stmt::ForIn(for_in) => {
                let iterable = expr(&for_in.iterable, env, next, changed);
                let mut loop_env = env.clone();
                let mut name = for_in.name.clone();
                let source_name = name.name.clone();
                if loop_env.contains_key(&source_name) {
                    name.name = fresh(&source_name, next);
                    *changed = true;
                }
                loop_env.insert(source_name, name.name.clone());
                Stmt::ForIn(ForInStmt {
                    name,
                    iterable,
                    body: rename_block(&for_in.body, &loop_env, next, changed),
                    span: for_in.span,
                })
            }
            Stmt::Match(match_stmt) => Stmt::Match(MatchStmt {
                scrutinee: expr(&match_stmt.scrutinee, env, next, changed),
                arms: match_stmt
                    .arms
                    .iter()
                    .map(|arm| {
                        let mut arm_env = env.clone();
                        let pattern = match &arm.pattern {
                            Pattern::Variant {
                                name,
                                bindings,
                                span,
                            } => Pattern::Variant {
                                name: name.clone(),
                                bindings: bindings
                                    .iter()
                                    .map(|binding| {
                                        let mut binding = binding.clone();
                                        let source_name = binding.name.clone();
                                        if arm_env.contains_key(&source_name) {
                                            binding.name = fresh(&source_name, next);
                                            *changed = true;
                                        }
                                        arm_env.insert(source_name, binding.name.clone());
                                        binding
                                    })
                                    .collect(),
                                span: *span,
                            },
                        };
                        MatchArm {
                            pattern,
                            body: rename_block(&arm.body, &arm_env, next, changed),
                            span: arm.span,
                        }
                    })
                    .collect(),
                span: match_stmt.span,
            }),
            Stmt::Try(try_stmt) => Stmt::Try(TryStmt {
                try_block: rename_block(&try_stmt.try_block, env, next, changed),
                catch: try_stmt.catch.as_ref().map(|catch| {
                    let mut catch_env = env.clone();
                    let mut name = catch.name.clone();
                    let source_name = name.name.clone();
                    if catch_env.contains_key(&source_name) {
                        name.name = fresh(&source_name, next);
                        *changed = true;
                    }
                    catch_env.insert(source_name, name.name.clone());
                    CatchClause {
                        name,
                        ty: catch.ty.clone(),
                        body: rename_block(&catch.body, &catch_env, next, changed),
                        span: catch.span,
                    }
                }),
                finally: try_stmt
                    .finally
                    .as_ref()
                    .map(|nested| rename_block(nested, env, next, changed)),
                span: try_stmt.span,
            }),
            Stmt::Throw(throw_stmt) => Stmt::Throw(ThrowStmt {
                value: expr(&throw_stmt.value, env, next, changed),
                span: throw_stmt.span,
            }),
            Stmt::Return(ret) => Stmt::Return(ReturnStmt {
                value: ret
                    .value
                    .as_ref()
                    .map(|value| expr(value, env, next, changed)),
                span: ret.span,
            }),
            Stmt::Expr(value) => Stmt::Expr(expr(value, env, next, changed)),
            Stmt::Break(span) => Stmt::Break(*span),
            Stmt::Continue(span) => Stmt::Continue(*span),
        }
    }

    let mut next = 0;
    let mut changed = false;
    let renamed = rename_block(block, inherited, &mut next, &mut changed);
    (renamed, changed)
}

fn stmt_contains_async(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Var(var) => expr_contains_async(&var.init),
        Stmt::If(branch) => {
            expr_contains_async(&branch.cond)
                || branch.then_block.stmts.iter().any(stmt_contains_async)
                || branch
                    .else_block
                    .as_ref()
                    .is_some_and(|block| block.stmts.iter().any(stmt_contains_async))
        }
        Stmt::While(loop_stmt) => {
            expr_contains_async(&loop_stmt.cond)
                || loop_stmt.body.stmts.iter().any(stmt_contains_async)
        }
        Stmt::ForRange(range) => {
            expr_contains_async(&range.start)
                || expr_contains_async(&range.end)
                || range.body.stmts.iter().any(stmt_contains_async)
        }
        Stmt::ForIn(range) => {
            expr_contains_async(&range.iterable) || range.body.stmts.iter().any(stmt_contains_async)
        }
        Stmt::Match(m) => {
            expr_contains_async(&m.scrutinee)
                || m.arms
                    .iter()
                    .any(|arm| arm.body.stmts.iter().any(stmt_contains_async))
        }
        Stmt::Try(try_stmt) => {
            try_stmt.try_block.stmts.iter().any(stmt_contains_async)
                || try_stmt
                    .finally
                    .as_ref()
                    .is_some_and(|block| block.stmts.iter().any(stmt_contains_async))
                || try_stmt
                    .catch
                    .as_ref()
                    .is_some_and(|catch| catch.body.stmts.iter().any(stmt_contains_async))
        }
        Stmt::Return(ret) => ret.value.as_ref().is_some_and(expr_contains_async),
        Stmt::Expr(expr) => expr_contains_async(expr),
        _ => false,
    }
}

fn collect_async_cfg_vars<'a>(
    stmts: &'a [Stmt],
    checked: &CheckedFile,
    ctx: &mut EmitCtx<'_>,
    vars: &mut Vec<(&'a VarStmt, String)>,
    merge_same_typed_names: bool,
) -> bool {
    for stmt in stmts {
        match stmt {
            Stmt::Var(var) => {
                let key = var
                    .ty
                    .as_ref()
                    .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked))
                    .unwrap_or_else(|| infer_type_name(&var.init, ctx));
                if merge_same_typed_names {
                    if let Some((_, existing_key)) = vars
                        .iter()
                        .find(|(existing, _)| existing.name.name == var.name.name)
                    {
                        if existing_key != &key {
                            return false;
                        }
                    } else {
                        vars.push((var, key.clone()));
                    }
                } else {
                    vars.push((var, key.clone()));
                }
                ctx.define_local(&var.name.name, full_type_mono(&key, checked));
                if let Expr::Async(_) = var.init {
                    continue;
                }
                if expr_contains_async(&var.init) {
                    // Expression-form `if` is lowered by the CFG builder as
                    // explicit branch states; it does not have a single
                    // await operand for the collector's scalar probe.
                    if matches!(var.init, Expr::If(_)) {
                        continue;
                    }
                    let await_name = "__aura_async_expr_probe";
                    let Some((await_expr, _)) = split_single_await_expr(&var.init, await_name)
                    else {
                        return false;
                    };
                    let await_key =
                        infer_type_name(&Expr::Async(AsyncExpr::Await(await_expr)), ctx);
                    if !async_cfg_value_supported(&key, checked)
                        || !async_cfg_value_supported(&await_key, checked)
                    {
                        return false;
                    }
                }
            }
            Stmt::If(branch) => {
                if !collect_async_cfg_vars(
                    &branch.then_block.stmts,
                    checked,
                    ctx,
                    vars,
                    merge_same_typed_names,
                ) {
                    return false;
                }
                if let Some(block) = &branch.else_block {
                    if !collect_async_cfg_vars(
                        &block.stmts,
                        checked,
                        ctx,
                        vars,
                        merge_same_typed_names,
                    ) {
                        return false;
                    }
                }
            }
            Stmt::While(loop_stmt) => {
                if !collect_async_cfg_vars(
                    &loop_stmt.body.stmts,
                    checked,
                    ctx,
                    vars,
                    merge_same_typed_names,
                ) {
                    return false;
                }
            }
            Stmt::ForRange(range) => {
                if !collect_async_cfg_vars(
                    &range.body.stmts,
                    checked,
                    ctx,
                    vars,
                    merge_same_typed_names,
                ) {
                    return false;
                }
            }
            Stmt::ForIn(for_in) => {
                if !collect_async_cfg_vars(
                    &for_in.body.stmts,
                    checked,
                    ctx,
                    vars,
                    merge_same_typed_names,
                ) {
                    return false;
                }
            }
            Stmt::Match(m) => {
                for arm in &m.arms {
                    if !collect_async_cfg_vars(
                        &arm.body.stmts,
                        checked,
                        ctx,
                        vars,
                        merge_same_typed_names,
                    ) {
                        return false;
                    }
                }
            }
            Stmt::Try(try_stmt) => {
                if let Some(catch) = &try_stmt.catch {
                    let catch_key = type_ref_local_key(&catch.ty, &[], &[]);
                    if try_stmt.try_block.stmts.is_empty()
                        || !async_cfg_catch_supported(&catch_key, checked)
                    {
                        return false;
                    }
                    // A single `val x = await ...` in the protected block is
                    // part of the frame and must be collected before CFG emit.
                    if !collect_async_cfg_vars(
                        &try_stmt.try_block.stmts,
                        checked,
                        ctx,
                        vars,
                        merge_same_typed_names,
                    ) {
                        return false;
                    }
                    if !collect_async_cfg_vars(
                        &catch.body.stmts,
                        checked,
                        ctx,
                        vars,
                        merge_same_typed_names,
                    ) {
                        return false;
                    }
                } else {
                    let Some(finally) = &try_stmt.finally else {
                        return false;
                    };
                    if !collect_async_cfg_vars(
                        &try_stmt.try_block.stmts,
                        checked,
                        ctx,
                        vars,
                        merge_same_typed_names,
                    ) || !collect_async_cfg_vars(
                        &finally.stmts,
                        checked,
                        ctx,
                        vars,
                        merge_same_typed_names,
                    ) {
                        return false;
                    }
                }
            }
            _ => {}
        }
    }
    true
}

fn emit_async_result_gc_mark(out: &mut String, result_key: &str, checked: &CheckedFile) {
    let result_cty = crate::stmt::local_key_to_c(result_key, checked);
    let mark = if is_heap_class_mono(result_key, checked) {
        Some(format!(
            "  if (__result.data != NULL && *(({result_cty} **)__result.data) != NULL) aura_gc_mark_ptr((void *)*((({result_cty} **)__result.data)));"
        ))
    } else if is_array_type_key(result_key)
        || crate::expr::is_enum_mono(result_key, checked)
        || crate::expr::is_value_struct_mono(result_key, checked)
        || is_iface_type_key(result_key, checked)
    {
        Some(format!(
            "  if (__result.data != NULL) {result_cty}_mark((const {result_cty} *)__result.data);"
        ))
    } else {
        None
    };
    if let Some(mark) = mark {
        out.push_str("  AuraTaskResult __result = aura_task_frame_result(frame);\n");
        out.push_str(&mark);
        out.push('\n');
    }
}

/// Lower nested branch/loop control flow into an explicit CFG. Aggregate
/// values are kept in frame slots and cloned at await/return boundaries.
fn emit_async_fun_cfg_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
    merge_same_typed_names: bool,
    boxed_params: &HashSet<String>,
) -> bool {
    let mut lexical_params = HashMap::new();
    for param in &f.params {
        lexical_params.insert(param.name.name.clone(), param.name.name.clone());
    }
    let (renamed_body, renamed) = alpha_rename_async_block(&f.body, &lexical_params);
    if renamed {
        let mut renamed_fun = f.clone();
        renamed_fun.body = renamed_body;
        return emit_async_fun_cfg_int(
            out,
            &renamed_fun,
            checked,
            detector,
            merge_same_typed_names,
            boxed_params,
        );
    }
    let Some(ret) = &f.return_type else {
        return false;
    };
    let return_key = full_type_mono(&type_ref_local_key_expand(ret, &[], &[], checked), checked);
    if !async_cfg_value_supported(&return_key, checked)
        || !f.body.stmts.iter().any(stmt_contains_async)
    {
        return false;
    }
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let mut collect_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for param in &f.params {
        let key = full_type_mono(
            &type_ref_local_key_expand(&param.ty, &params, &[], checked),
            checked,
        );
        if !async_cfg_value_supported(&key, checked) && !async_cfg_task_supported(&key, checked) {
            return false;
        }
        collect_ctx.define_local(&param.name.name, full_type_mono(&key, checked));
        if boxed_params.contains(&param.name.name) {
            collect_ctx.mark_box_local(&param.name.name);
        }
    }
    let mut vars = Vec::new();
    if !collect_async_cfg_vars(
        &f.body.stmts,
        checked,
        &mut collect_ctx,
        &mut vars,
        merge_same_typed_names,
    ) {
        return false;
    }
    for (_, key) in &mut vars {
        *key = full_type_mono(key, checked);
    }
    let mut locals = HashMap::new();
    for (var, key) in &vars {
        if !async_cfg_value_supported(key, checked)
            || locals.insert(var.name.name.clone(), key.clone()).is_some()
        {
            return false;
        }
    }
    for param in &f.params {
        let key = full_type_mono(
            &type_ref_local_key_expand(&param.ty, &params, &[], checked),
            checked,
        );
        if locals
            .insert(param.name.name.clone(), full_type_mono(&key, checked))
            .is_some()
        {
            return false;
        }
    }
    let mut ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for param in &f.params {
        let key = full_type_mono(
            &type_ref_local_key_expand(&param.ty, &params, &[], checked),
            checked,
        );
        ctx.define_local(&param.name.name, full_type_mono(&key, checked));
        if boxed_params.contains(&param.name.name) {
            ctx.mark_box_local(&param.name.name);
        }
    }
    for (var, key) in &vars {
        ctx.define_local(&var.name.name, full_type_mono(key, checked));
    }
    let mut builder = AsyncCfgBuilder::new(ctx, locals.clone(), return_key.clone());
    let terminal = builder.alloc();
    let terminal_value = if return_key == "Unit" {
        "0".into()
    } else if return_key == "String" {
        "NULL".into()
    } else if is_array_type_key(&return_key) {
        format!(
            "({}){{0}}",
            crate::stmt::local_key_to_c(&return_key, checked)
        )
    } else if return_key == "Bool" {
        "false".into()
    } else if mono_base_name(&return_key, checked).is_some_and(|base| is_enum_name(checked, base)) {
        format!(
            "({}){{0}}",
            crate::stmt::local_key_to_c(&return_key, checked)
        )
    } else if is_heap_class_mono(&return_key, checked)
        || return_key == "ForeignHandle"
        || return_key.starts_with("ForeignHandle_")
    {
        "NULL".into()
    } else if is_fun_type_key(&return_key) {
        format!(
            "({}){{0}}",
            crate::stmt::local_key_to_c(&return_key, checked)
        )
    } else if crate::expr::is_value_struct_mono(&return_key, checked) {
        format!(
            "({}){{0}}",
            crate::stmt::local_key_to_c(&return_key, checked)
        )
    } else {
        "INT64_C(0)".into()
    };
    builder.finish(
        terminal,
        AsyncCfgNode::Return {
            value: terminal_value,
            value_key: return_key.clone(),
            value_is_ident: false,
            value_is_owned_temp: false,
        },
    );
    let entry = builder.emit_block(&f.body.stmts, terminal, None, None);
    if !builder.supported || builder.nodes.iter().any(Option::is_none) {
        return false;
    }
    let thrown_class_keys: BTreeSet<String> = builder
        .nodes
        .iter()
        .filter_map(|node| match node.as_ref() {
            Some(AsyncCfgNode::Throw { value_key, .. })
                if async_cfg_throw_class_supported(value_key, checked) =>
            {
                Some(full_type_mono(value_key, checked))
            }
            _ => None,
        })
        .collect();
    let thrown_array_keys: BTreeSet<String> = builder
        .nodes
        .iter()
        .filter_map(|node| match node.as_ref() {
            Some(AsyncCfgNode::Throw { value_key, .. }) if is_array_type_key(value_key) => {
                Some(full_type_mono(value_key, checked))
            }
            _ => None,
        })
        .collect();
    let thrown_aggregate_keys: BTreeSet<String> = builder
        .nodes
        .iter()
        .filter_map(|node| match node.as_ref() {
            Some(AsyncCfgNode::Throw { value_key, .. })
                if async_cfg_throw_aggregate_supported(value_key, checked) =>
            {
                Some(full_type_mono(value_key, checked))
            }
            _ => None,
        })
        .collect();
    let thrown_foreign_handle = builder.nodes.iter().any(|node| {
        matches!(
            node.as_ref(),
            Some(AsyncCfgNode::Throw { value_key, .. })
                if value_key == "ForeignHandle" || value_key.starts_with("ForeignHandle_")
        )
    });
    let cfg_locals = builder.cfg_locals.clone();
    let owned_class_catches = builder.owned_class_catches.clone();
    let match_bindings = builder.match_bindings.clone();
    let finally_states = builder.finally_states.clone();
    let cancel_finally_states = builder.cancel_finally_states.clone();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let cancel_fn = format!("aura_async_cancel_{base}");
    let data_drop = format!("aura_async_data_drop_{base}");
    let gc_mark = format!("aura_async_gc_mark_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let lowering_kind = match return_key.as_str() {
        "Unit" => "Unit",
        "Int" => "Int",
        "Bool" => "Bool",
        "String" => "String",
        "ForeignHandle" => "ForeignHandle",
        _ if return_key.starts_with("ForeignHandle_") => "ForeignHandle",
        _ if is_heap_class_mono(&return_key, checked) => "Class",
        _ if is_fun_type_key(&return_key) => "Fun",
        _ if crate::expr::is_value_struct_mono(&return_key, checked) => "Struct",
        _ => "Array",
    };
    let mut frame_fields = Vec::new();
    for param in &f.params {
        frame_fields.push(AsyncFrameField {
            name: param.name.name.clone(),
            type_key: full_type_mono(
                &type_ref_local_key_expand(&param.ty, &params, &[], checked),
                checked,
            ),
        });
    }
    // Keep the debug contract aligned with the actual frame declarations.
    // The model owns its display keys so it remains valid while C is emitted.
    for (var, key) in &vars {
        frame_fields.push(AsyncFrameField {
            name: var.name.name.clone(),
            type_key: key.clone(),
        });
    }
    for (name, key) in &cfg_locals {
        frame_fields.push(AsyncFrameField {
            name: name.clone(),
            type_key: key.clone(),
        });
    }
    for (name, key) in &match_bindings {
        frame_fields.push(AsyncFrameField {
            name: name.clone(),
            type_key: key.clone(),
        });
    }
    frame_fields.push(AsyncFrameField {
        name: "await_task".into(),
        type_key: "TaskFrame*".into(),
    });
    frame_fields.push(AsyncFrameField {
        name: "await_task_owned".into(),
        type_key: "Bool".into(),
    });
    frame_fields.push(AsyncFrameField {
        name: "await_failed".into(),
        type_key: "Bool".into(),
    });
    let async_machine = AsyncStateMachine {
        frame_fields,
        nodes: builder.nodes.clone(),
    };
    if !async_machine.validate_edges() {
        return false;
    }
    async_machine.dump_comments(out);
    let _ = writeln!(
        out,
        "/* aura async general CFG {lowering_kind} lowering states={} */",
        builder.nodes.len()
    );
    if thrown_foreign_handle {
        let clone = format!("aura_async_cfg_foreign_error_clone_{base}");
        let destroy = format!("aura_async_cfg_foreign_error_destroy_{base}");
        let _ = writeln!(out, "static void *{clone}(const void *raw, size_t size, size_t *out_size) {{ if (raw == NULL || size != sizeof(AuraFfiOpaqueHandle *)) return NULL; AuraFfiOpaqueHandle *handle = *((AuraFfiOpaqueHandle *const *)raw); if (handle != NULL && aura_ffi_handle_retain(handle) != AURA_FFI_OK) return NULL; AuraFfiOpaqueHandle **copy = (AuraFfiOpaqueHandle **)malloc(sizeof(*copy)); if (copy == NULL) {{ if (handle != NULL) (void)aura_ffi_handle_drop(&handle); return NULL; }} *copy = handle; if (out_size != NULL) *out_size = sizeof(*copy); return copy; }}");
        let _ = writeln!(out, "static void {destroy}(void *raw, size_t size) {{ if (raw != NULL && size == sizeof(AuraFfiOpaqueHandle *)) {{ AuraFfiOpaqueHandle **handle = (AuraFfiOpaqueHandle **)raw; if (*handle != NULL) (void)aura_ffi_handle_drop(handle); }} free(raw); }}");
    }
    // Keep suspension-point markers stable across the CFG and straight-line
    // lowerers so diagnostics and backend regression fixtures share one ABI.
    for point in f.suspension_points() {
        let _ = writeln!(
            out,
            "/* aura async general suspension state={} kind=await */",
            point.state_id
        );
        let _ = writeln!(
            out,
            "/* aura async suspension state={} kind=await */",
            point.state_id
        );
    }
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for param in &f.params {
        let param_key = full_type_mono(
            &type_ref_local_key_expand(&param.ty, &params, &[], checked),
            checked,
        );
        let cty = if boxed_params.contains(&param.name.name) {
            general_spawn_box_c_type(&param_key).to_string()
        } else {
            c_type_ref_subst(&param.ty, checked, &params, &[])
        };
        let _ = writeln!(out, "  {} {};", cty, mangle_ident(&param.name.name));
        if param_key == "String" && !boxed_params.contains(&param.name.name) {
            let _ = writeln!(out, "  bool {}__owned;", mangle_ident(&param.name.name));
        }
    }
    for (var, key) in &vars {
        let _ = writeln!(
            out,
            "  {} {};",
            crate::stmt::local_key_to_c(key, checked),
            mangle_ident(&var.name.name)
        );
        if key == "String" {
            let _ = writeln!(out, "  bool {}__owned;", mangle_ident(&var.name.name));
        }
    }
    for (name, key) in &cfg_locals {
        let _ = writeln!(
            out,
            "  {} {};",
            crate::stmt::local_key_to_c(key, checked),
            mangle_ident(name)
        );
        if key == "String" {
            let _ = writeln!(out, "  bool {}__owned;", mangle_ident(name));
        }
    }
    for (name, key) in &match_bindings {
        let _ = writeln!(
            out,
            "  {} {};",
            crate::stmt::local_key_to_c(key, checked),
            mangle_ident(name)
        );
        if key == "String" {
            let _ = writeln!(out, "  bool {}__owned;", mangle_ident(name));
        }
    }
    out.push_str("  AuraTaskFrame *await_task; bool await_task_owned; bool await_failed;\n");
    out.push_str("  bool await_cancelled;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (data != NULL && data->await_task != NULL && data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task);\n");
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {data_drop}(AuraTaskFrame *frame, void *raw_data, size_t size) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)raw_data; (void)frame; (void)size;"
    );
    out.push_str("  AuraFfiOpaqueHandle *__aura_released_handle = NULL;\n");
    for param in &f.params {
        let key = full_type_mono(
            &type_ref_local_key_expand(&param.ty, &params, &[], checked),
            checked,
        );
        let name = mangle_ident(&param.name.name);
        if boxed_params.contains(&param.name.name) {
            let release = general_spawn_box_release(&key);
            let _ = writeln!(out, "  if (data->{name} != NULL) {release}(data->{name});");
        } else if key == "String" {
            let _ = writeln!(
                out,
                "  if (data->{name}__owned && data->{name} != NULL) free((void *)data->{name});"
            );
        } else if is_array_type_key(&key)
            || crate::expr::mono_base_name(&key, checked)
                .is_some_and(|base| checked.ast.enums.iter().any(|e| e.name.name == base))
            || crate::expr::is_value_struct_mono(&key, checked)
            || is_iface_type_key(&key, checked)
        {
            if is_array_type_key(&key) && crate::array_emit::is_array_of_heap_class(&key, checked) {
                let _ = writeln!(
                    out,
                    "  if (data != NULL) aura_gc_remove_array_root((void **)&data->{name}.data);"
                );
            }
            let cty = crate::stmt::local_key_to_c(&key, checked);
            let _ = writeln!(out, "  if (data != NULL) {cty}_drop(&data->{name});");
        } else if async_cfg_scheduler_owned_key(&key) {
            if key == "Channel" || key.starts_with("Channel_") {
                let _ = writeln!(
                    out,
                    "  if (data->{name} != NULL) aura_task_channel_destroy(data->{name});"
                );
            } else {
                let _ = writeln!(out, "  if (data->{name} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &data->{name});");
            }
        } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let _ = writeln!(
                out,
                "  if (data->{name} != NULL && data->{name} != __aura_released_handle) {{ __aura_released_handle = data->{name}; (void)aura_ffi_handle_drop(&data->{name}); }}"
            );
        } else if is_fun_type_key(&key) {
            let _ = writeln!(
                out,
                "  if (data != NULL && data->{name}.env != NULL) aura_fun_env_free(data->{name}.env);"
            );
        }
    }
    for (var, key) in &vars {
        if async_cfg_scheduler_owned_key(key) {
            let name = mangle_ident(&var.name.name);
            if key == "Channel" || key.starts_with("Channel_") {
                let _ = writeln!(
                    out,
                    "  if (data->{name} != NULL) aura_task_channel_destroy(data->{name});"
                );
            } else {
                let _ = writeln!(out, "  if (data->{name} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &data->{name});");
            }
        } else if key == "String" {
            let name = mangle_ident(&var.name.name);
            let _ = writeln!(
                out,
                "  if (data->{name}__owned && data->{name} != NULL) free((void *)data->{name});"
            );
        } else if is_array_type_key(key) {
            crate::array_emit::emit_array_contents_free(
                out,
                2,
                &format!("data->{}", mangle_ident(&var.name.name)),
                key,
            );
        } else if is_fun_type_key(key) {
            let name = mangle_ident(&var.name.name);
            let _ = writeln!(
                out,
                "  if (data->{name}.env != NULL) aura_fun_env_free(data->{name}.env);"
            );
        } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let name = mangle_ident(&var.name.name);
            let _ = writeln!(
                out,
                "  if (data->{name} != NULL && data->{name} != __aura_released_handle) {{ __aura_released_handle = data->{name}; (void)aura_ffi_handle_drop(&data->{name}); }}"
            );
        } else if key.contains("Outcome_String") && key.contains("std_error_Error") {
            emit_owned_value_cleanup(
                out,
                1,
                &format!("data->{}", mangle_ident(&var.name.name)),
                key,
                checked,
            );
        } else if crate::expr::is_enum_mono(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(
                out,
                "  {cty}_drop(&data->{});",
                mangle_ident(&var.name.name)
            );
        } else if crate::expr::is_value_struct_mono(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(
                out,
                "  {cty}_drop(&data->{});",
                mangle_ident(&var.name.name)
            );
        } else if is_iface_type_key(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(
                out,
                "  {cty}_drop(&data->{});",
                mangle_ident(&var.name.name)
            );
        }
    }
    for (name, key) in &match_bindings {
        if async_cfg_scheduler_owned_key(key) {
            let name = mangle_ident(name);
            if key == "Channel" || key.starts_with("Channel_") {
                let _ = writeln!(
                    out,
                    "  if (data->{name} != NULL) aura_task_channel_destroy(data->{name});"
                );
            } else {
                let _ = writeln!(out, "  if (data->{name} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &data->{name});");
            }
        } else if key == "String" {
            let name = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (data->{name}__owned && data->{name} != NULL) free((void *)data->{name});"
            );
        } else if is_array_type_key(key) {
            crate::array_emit::emit_array_contents_free(
                out,
                2,
                &format!("data->{}", mangle_ident(name)),
                key,
            );
        } else if is_fun_type_key(key) {
            let name = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (data->{name}.env != NULL) aura_fun_env_free(data->{name}.env);"
            );
        } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let name = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (data->{name} != NULL) (void)aura_ffi_handle_drop(&data->{name});"
            );
        } else if is_heap_class_mono(key, checked) {
            let name = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (data->{name} != NULL) aura_gc_remove_root((void **)&data->{name});"
            );
        } else if key.contains("Outcome_String") && key.contains("std_error_Error") {
            emit_owned_value_cleanup(
                out,
                1,
                &format!("data->{}", mangle_ident(name)),
                key,
                checked,
            );
        } else if crate::expr::is_enum_mono(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_drop(&data->{});", mangle_ident(name));
        } else if crate::expr::is_value_struct_mono(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_drop(&data->{});", mangle_ident(name));
        } else if is_iface_type_key(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_drop(&data->{});", mangle_ident(name));
        }
    }
    for (name, key) in &cfg_locals {
        if async_cfg_scheduler_owned_key(key) {
            let name = mangle_ident(name);
            if key == "Channel" || key.starts_with("Channel_") {
                let _ = writeln!(
                    out,
                    "  if (data->{name} != NULL) aura_task_channel_destroy(data->{name});"
                );
            } else {
                let _ = writeln!(out, "  if (data->{name} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &data->{name});");
            }
        } else if key == "String" {
            let name = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (data->{name}__owned && data->{name} != NULL) free((void *)data->{name});"
            );
        }
        if is_array_type_key(key) {
            crate::array_emit::emit_array_contents_free(
                out,
                2,
                &format!("data->{}", mangle_ident(name)),
                key,
            );
        } else if is_fun_type_key(key) {
            let name = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (data->{name}.env != NULL) aura_fun_env_free(data->{name}.env);"
            );
        } else if key.contains("Outcome_String") && key.contains("std_error_Error") {
            emit_owned_value_cleanup(
                out,
                1,
                &format!("data->{}", mangle_ident(name)),
                key,
                checked,
            );
        } else if crate::expr::is_enum_mono(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_drop(&data->{});", mangle_ident(name));
        } else if crate::expr::is_value_struct_mono(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_drop(&data->{});", mangle_ident(name));
        } else if is_iface_type_key(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_drop(&data->{});", mangle_ident(name));
        }
    }
    for (name, key) in &owned_class_catches {
        let mono = full_type_mono(key, checked);
        let dtor = format!("aura_ex_dtor_{mono}");
        let name = mangle_ident(name);
        if let Some(base) = mono_base_name(&mono, checked) {
            if let Some(class) = checked
                .ast
                .classes
                .iter()
                .find(|class| class.name.name == base && class.type_params.is_empty())
            {
                let params: Vec<String> = class
                    .type_params
                    .iter()
                    .map(|param| param.name.name.clone())
                    .collect();
                for (field_name, field_key) in ownership_fields(class, checked, &params, &[]) {
                    let field_key = full_type_mono(&field_key, checked);
                    if is_array_type_key(&field_key) {
                        let field_name = mangle_ident(&field_name);
                        let _ = writeln!(
                            out,
                            "  if (data->{name} != NULL) aura_gc_remove_array_root((void **)&data->{name}->{field_name}.data);"
                        );
                    }
                }
            }
        }
        let _ = writeln!(
            out,
            "  if (data->{name} != NULL) {{ {dtor}(data->{name}); data->{name} = NULL; }}"
        );
    }
    out.push_str("}\n\n");
    let _ = writeln!(out, "static void {gc_mark}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL) return;"
    );
    emit_async_result_gc_mark(out, &return_key, checked);
    let mut emit_gc_mark = |name: &str, key: &str| {
        let name = mangle_ident(name);
        if is_heap_class_mono(key, checked) {
            let _ = writeln!(out, "  aura_gc_mark_ptr((void *)data->{name});");
        } else if crate::array_emit::is_array_type_key(key) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_mark(&data->{name});");
        } else if crate::expr::is_enum_mono(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_mark(&data->{name});");
        } else if crate::expr::is_value_struct_mono(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_mark(&data->{name});");
        } else if is_iface_type_key(key, checked) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  {cty}_mark(&data->{name});");
        }
    };
    for param in &f.params {
        if !boxed_params.contains(&param.name.name) {
            emit_gc_mark(
                &param.name.name,
                &full_type_mono(
                    &type_ref_local_key_expand(&param.ty, &params, &[], checked),
                    checked,
                ),
            );
        }
    }
    for (var, key) in &vars {
        emit_gc_mark(&var.name.name, key);
    }
    for (name, key) in &cfg_locals {
        emit_gc_mark(name, key);
    }
    for (name, key) in &match_bindings {
        emit_gc_mark(name, key);
    }
    for (name, key) in &owned_class_catches {
        let mono = full_type_mono(key, checked);
        let Some(base) = mono_base_name(&mono, checked) else {
            continue;
        };
        let Some(class) = checked
            .ast
            .classes
            .iter()
            .find(|class| class.name.name == base && class.type_params.is_empty())
        else {
            continue;
        };
        let params: Vec<String> = class
            .type_params
            .iter()
            .map(|param| param.name.name.clone())
            .collect();
        let catch_name = mangle_ident(name);
        for (field_name, field_key) in ownership_fields(class, checked, &params, &[]) {
            let field_name = mangle_ident(&field_name);
            let field_key = full_type_mono(&field_key, checked);
            if is_array_type_key(&field_key) {
                let cty = crate::stmt::local_key_to_c(&field_key, checked);
                let _ = writeln!(
                    out,
                    "  if (data->{catch_name} != NULL) {cty}_mark(&data->{catch_name}->{field_name});"
                );
            } else if is_heap_class_mono(&field_key, checked) {
                let _ = writeln!(
                    out,
                    "  if (data->{catch_name} != NULL) aura_gc_mark_ptr((void *)data->{catch_name}->{field_name});"
                );
            } else if crate::expr::is_enum_mono(&field_key, checked) {
                let cty = crate::stmt::local_key_to_c(&field_key, checked);
                let _ = writeln!(
                    out,
                    "  if (data->{catch_name} != NULL) {cty}_mark(&data->{catch_name}->{field_name});"
                );
            } else if crate::expr::is_value_struct_mono(&field_key, checked) {
                let cty = crate::stmt::local_key_to_c(&field_key, checked);
                let _ = writeln!(
                    out,
                    "  if (data->{catch_name} != NULL) {cty}_mark(&data->{catch_name}->{field_name});"
                );
            }
        }
    }
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{"
    );
    if return_key == "Task"
        || return_key.starts_with("Task_")
        || return_key == "TaskHandle"
        || return_key.starts_with("TaskHandle_")
    {
        out.push_str("  (void)size; if (data != NULL) { AuraTaskFrame **result = (AuraTaskFrame **)data; if (*result != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, result); free(data); }\n}\n\n");
    } else if return_key == "Channel" || return_key.starts_with("Channel_") {
        out.push_str("  (void)size; if (data != NULL) { AuraTaskChannel **result = (AuraTaskChannel **)data; if (*result != NULL) aura_task_channel_destroy(*result); free(data); }\n}\n\n");
    } else if return_key == "String" {
        out.push_str("  (void)size; if (data != NULL) { const char **result = (const char **)data; if (*result != NULL) free((void *)*result); free(result); }\n}\n\n");
    } else if is_array_type_key(&return_key) {
        let result_cty = crate::stmt::local_key_to_c(&return_key, checked);
        let _ = writeln!(
            out,
            "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data;"
        );
        crate::array_emit::emit_array_contents_free(out, 2, "(*result)", &return_key);
        out.push_str("    free(result); }\n}\n\n");
    } else if crate::expr::is_value_struct_mono(&return_key, checked) {
        let result_cty = crate::stmt::local_key_to_c(&return_key, checked);
        let _ = writeln!(
            out,
            "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {result_cty}_drop(result); free(result); }}\n}}\n\n"
        );
    } else if is_heap_class_mono(&return_key, checked) {
        out.push_str("  (void)size; if (data != NULL) { aura_gc_remove_root((void **)data); free(data); }\n}\n\n");
    } else if return_key == "ForeignHandle" || return_key.starts_with("ForeignHandle_") {
        out.push_str("  (void)size; if (data != NULL) { AuraFfiOpaqueHandle **result = (AuraFfiOpaqueHandle **)data; if (*result != NULL) (void)aura_ffi_handle_drop(result); free(result); }\n}\n\n");
    } else {
        let result_cty = crate::stmt::local_key_to_c(&return_key, checked);
        if return_key.contains("Outcome_String") && return_key.contains("std_error_Error") {
            let _ = writeln!(
                out,
                "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; if (result->tag == 0 && result->data.OutcomeOk.owned && result->data.OutcomeOk.value != NULL) free((void *)result->data.OutcomeOk.value); if (result->tag == 1 && result->data.OutcomeErr.owned && result->data.OutcomeErr.error != NULL) aura_gc_remove_root((void **)&result->data.OutcomeErr.error); free(result); }}\n}}\n\n"
            );
        } else if crate::expr::is_enum_mono(&return_key, checked) {
            let _ = writeln!(
                out,
                "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {result_cty}_drop(result); free(result); }}\n}}\n\n"
            );
        } else if crate::expr::is_value_struct_mono(&return_key, checked) {
            let _ = writeln!(
                out,
                "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {result_cty}_drop(result); free(result); }}\n}}\n\n"
            );
        } else {
            out.push_str("  (void)size; free(data);\n}\n\n");
        }
    }
    let clone_error = format!("aura_async_error_clone_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    let _ = writeln!(
        out,
        "static void *{clone_error}(const void *src, size_t size, size_t *out_size) {{ const char *text = (const char *)src; size_t len; char *copy; (void)size; if (text == NULL || out_size == NULL) return NULL; len = strlen(text); copy = (char *)malloc(len + 1); if (copy == NULL) return NULL; memcpy(copy, text, len + 1); *out_size = len + 1; return copy; }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    for mono in &thrown_class_keys {
        let Some(base) = mono_base_name(mono, checked) else {
            continue;
        };
        let Some(class) = checked
            .ast
            .classes
            .iter()
            .find(|class| class.name.name == base && class.type_params.is_empty())
        else {
            continue;
        };
        let cty = c_class_type(mono);
        let suffix = mangle_ident(mono);
        let clone = format!("aura_async_cfg_class_error_clone_{base}_{suffix}");
        let destroy = format!("aura_async_cfg_class_error_destroy_{base}_{suffix}");
        let _ = writeln!(
            out,
            "static void *{clone}(const void *src, size_t size, size_t *out_size) {{"
        );
        let _ = writeln!(out, "  (void)size; const {cty} *source = (const {cty} *)src; if (source == NULL || out_size == NULL) return NULL; {cty} *copy = ({cty} *)malloc(sizeof(*copy)); if (copy == NULL) return NULL; *copy = *source;");
        for field in &class.fields {
            if type_ref_local_key(&field.ty, &[], &[]) != "String" {
                continue;
            }
            let name = mangle_ident(&field.name.name);
            let _ = writeln!(out, "  if (source->{name} != NULL) {{ size_t len = strlen(source->{name}); char *text = (char *)malloc(len + 1); if (text == NULL) {{ free(copy); return NULL; }} memcpy(text, source->{name}, len + 1); copy->{name} = text; }}");
        }
        let _ = writeln!(out, "  *out_size = sizeof(*copy); return copy; }}");
        let _ = writeln!(out, "static void {destroy}(void *data, size_t size) {{ (void)size; aura_ex_dtor_{mono}(data); }}\n");
    }
    for key in &thrown_array_keys {
        let cty = crate::stmt::local_key_to_c(key, checked);
        let clone = crate::names::c_method_name(key, "clone");
        let suffix = mangle_ident(key);
        let clone_error = format!("aura_async_cfg_array_error_clone_{suffix}");
        let destroy_error = format!("aura_async_cfg_array_error_destroy_{suffix}");
        let _ = writeln!(
            out,
            "static void *{clone_error}(const void *src, size_t size, size_t *out_size) {{ (void)size; if (src == NULL || out_size == NULL) return NULL; {cty} *copy = ({cty} *)malloc(sizeof(*copy)); if (copy == NULL) return NULL; *copy = {clone}(({cty} *)src); *out_size = sizeof(*copy); return copy; }}"
        );
        let mut free_code = String::new();
        crate::array_emit::emit_array_contents_free(&mut free_code, 1, "(*value)", key);
        let _ = writeln!(
            out,
            "static void {destroy_error}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ {cty} *value = ({cty} *)data;{free_code} free(data); }} }}"
        );
    }
    for key in &thrown_aggregate_keys {
        let cty = crate::stmt::local_key_to_c(key, checked);
        let suffix = mangle_ident(key);
        let clone_error = format!("aura_async_cfg_aggregate_error_clone_{suffix}");
        let destroy_error = format!("aura_async_cfg_aggregate_error_destroy_{suffix}");
        if is_fun_type_key(key) {
            let _ = writeln!(
                out,
                "static void *{clone_error}(const void *src, size_t size, size_t *out_size) {{ (void)size; if (src == NULL || out_size == NULL) return NULL; {cty} *copy = ({cty} *)malloc(sizeof(*copy)); if (copy == NULL) return NULL; *copy = *((const {cty} *)src); if (copy->env != NULL) aura_fun_env_retain(copy->env); *out_size = sizeof(*copy); return copy; }}"
            );
            let _ = writeln!(
                out,
                "static void {destroy_error}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ {cty} *value = ({cty} *)data; if (value->env != NULL) aura_fun_env_free(value->env); free(value); }} }}"
            );
        } else {
            let _ = writeln!(
                out,
                "static void *{clone_error}(const void *src, size_t size, size_t *out_size) {{ (void)size; if (src == NULL || out_size == NULL) return NULL; {cty} *copy = ({cty} *)malloc(sizeof(*copy)); if (copy == NULL) return NULL; *copy = {cty}_clone((const {cty} *)src); *out_size = sizeof(*copy); return copy; }}"
            );
            let _ = writeln!(
                out,
                "static void {destroy_error}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ {cty} *value = ({cty} *)data; {cty}_drop(value); free(value); }} }}"
            );
        }
    }
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    // Cancellation is routed per CFG state so a pending await can enter its
    // synchronous finally block before the frame publishes cancellation.
    for param in &f.params {
        let name = mangle_ident(&param.name.name);
        let key = full_type_mono(
            &type_ref_local_key_expand(&param.ty, &params, &[], checked),
            checked,
        );
        let cty = if boxed_params.contains(&param.name.name) {
            general_spawn_box_c_type(&key).to_string()
        } else {
            c_type_ref_subst(&param.ty, checked, &params, &[])
        };
        let _ = writeln!(out, "  {cty} {name} = data->{name};");
        if param.name.name == "this" {
            let _ = writeln!(out, "  {cty} this = data->{name};");
        }
    }
    for (var, key) in &vars {
        let name = mangle_ident(&var.name.name);
        let _ = writeln!(
            out,
            "  {} {name} = data->{name};",
            crate::stmt::local_key_to_c(key, checked)
        );
        if key == "String" {
            let _ = writeln!(out, "  bool {name}__owned = data->{name}__owned;");
        }
    }
    for (name, key) in &cfg_locals {
        let name = mangle_ident(name);
        let _ = writeln!(
            out,
            "  {} {name} = data->{name};",
            crate::stmt::local_key_to_c(key, checked)
        );
        if key == "String" {
            let _ = writeln!(out, "  bool {name}__owned = data->{name}__owned;");
        }
    }
    for (name, key) in &match_bindings {
        let name = mangle_ident(name);
        let _ = writeln!(
            out,
            "  {} {name} = data->{name};",
            crate::stmt::local_key_to_c(key, checked)
        );
        if key == "String" {
            let _ = writeln!(out, "  bool {name}__owned = data->{name}__owned;");
        }
    }
    out.push_str("  for (;;) {\n    switch (aura_task_frame_resume_state(frame)) {\n");
    let mut sync_parts = f
        .params
        .iter()
        .map(|param| {
            format!(
                "data->{} = {};",
                mangle_ident(&param.name.name),
                mangle_ident(&param.name.name)
            )
        })
        .collect::<Vec<_>>();
    sync_parts.extend(
        vars.iter()
            .map(|(var, _)| {
                let name = mangle_ident(&var.name.name);
                if locals
                    .get(&var.name.name)
                    .is_some_and(|key| key == "String")
                {
                    format!("data->{name} = {name}; data->{name}__owned = {name}__owned;")
                } else {
                    format!("data->{name} = {name};")
                }
            })
            .collect::<Vec<_>>(),
    );
    sync_parts.extend(cfg_locals.iter().map(|(name, key)| {
        let name = mangle_ident(name);
        if key == "String" {
            format!("data->{name} = {name}; data->{name}__owned = {name}__owned;")
        } else {
            format!("data->{name} = {name};")
        }
    }));
    sync_parts.extend(match_bindings.iter().map(|(name, key)| {
        let name = mangle_ident(name);
        if key == "String" {
            format!("data->{name} = {name}; data->{name}__owned = {name}__owned;")
        } else {
            format!("data->{name} = {name};")
        }
    }));
    let sync = sync_parts.join(" ");
    for (state, node) in builder.nodes.into_iter().enumerate() {
        let node = node.expect("validated CFG node");
        let _ = writeln!(out, "      case {state}: {{");
        if !finally_states.contains(&state) {
            if let Some(target) = cancel_finally_states.get(&state) {
                let _ = writeln!(
                    out,
                    "        if (aura_task_frame_cancel_requested(frame)) {{ data->await_cancelled = true; if (data->await_task != NULL && data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {target}); continue; }}"
                );
            } else {
                match &node {
                AsyncCfgNode::AwaitCatch {
                    finally_state: Some(target), ..
                }
                | AsyncCfgNode::AwaitCatchValue {
                    finally_state: Some(target), ..
                }
                | AsyncCfgNode::AwaitFinally {
                    finally_state: target, ..
                } => {
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_cancel_requested(frame)) {{ data->await_cancelled = true; if (data->await_task != NULL && data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {target}); continue; }}"
                    );
                }
                _ => out.push_str(
                    "        if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n",
                ),
                }
            }
        }
        match node {
            AsyncCfgNode::Action { code, next } => {
                let _ = writeln!(
                    out,
                    "        {code} {sync} aura_task_frame_set_resume_state(frame, {next}); continue;"
                );
            }
            AsyncCfgNode::Branch {
                condition,
                then_state,
                else_state,
            } => {
                let _ = writeln!(out, "        aura_task_frame_set_resume_state(frame, ({condition}) ? {then_state} : {else_state}); continue;");
            }
            AsyncCfgNode::Await {
                value,
                value_key,
                operand,
                owns_task,
                next,
            } => {
                let _ = writeln!(out, "        if (data->await_task == NULL) {{ data->await_task = {operand}; data->await_task_owned = {}; }}", if owns_task { "true" } else { "false" });
                out.push_str("        if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
                out.push_str("        AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
                let _ = writeln!(out, "        if (child_state == AURA_TASK_PENDING) {{ {sync} aura_task_frame_set_resume_state(frame, {state}); if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}");
                out.push_str("        if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n        if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n        if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
                if value_key == "String" {
                    let _ = writeln!(
                        out,
                        "        if (data->await_task != NULL && aura_task_frame_result(data->await_task).data != NULL) {{ const char *__src = *((const char **)aura_task_frame_result(data->await_task).data); if (data->{value}__owned && data->{value} != NULL) free((void *)data->{value}); data->{value} = NULL; data->{value}__owned = false; if (__src != NULL) {{ size_t __len = strlen(__src); data->{value} = (char *)malloc(__len + 1); if (data->{value} == NULL) return AURA_TASK_FAILED; memcpy((void *)data->{value}, __src, __len + 1); data->{value}__owned = true; {value} = data->{value}; {value}__owned = true; }} }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else if value_key == "Channel"
                    || value_key.starts_with("Channel_")
                    || value_key == "Task"
                    || value_key.starts_with("Task_")
                    || value_key == "TaskHandle"
                    || value_key.starts_with("TaskHandle_")
                {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let is_channel = value_key == "Channel" || value_key.starts_with("Channel_");
                    let clone = if is_channel {
                        "if (__child != NULL && !aura_task_channel_retain(__child)) return AURA_TASK_FAILED;".to_string()
                    } else {
                        "if (__child != NULL && (__aura_task_executor == NULL || !aura_task_executor_retain_payload(__aura_task_executor, __child))) return AURA_TASK_FAILED;".to_string()
                    };
                    let old_drop = if is_channel {
                        format!(
                            "if (data->{value} != NULL) aura_task_channel_destroy(data->{value});"
                        )
                    } else {
                        format!("if (data->{value} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &data->{value});")
                    };
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {clone} {old_drop} data->{value} = *__child; {value} = data->{value}; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else if is_fun_type_key(&value_key) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; if (data->{value}.env != NULL) aura_fun_env_free(data->{value}.env); data->{value} = *__child; if (data->{value}.env != NULL) aura_fun_env_retain(data->{value}.env); {value} = data->{value}; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else if crate::expr::is_value_struct_mono(&value_key, checked) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {cty}_drop(&data->{value}); data->{value} = {cty}_clone(__child); {value} = data->{value}; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else if is_array_type_key(&value_key) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let clone_key = full_type_mono(&value_key, checked);
                    let clone = crate::names::c_method_name(&clone_key, "clone");
                    let mut free_code = String::new();
                    crate::array_emit::emit_array_contents_free(
                        &mut free_code,
                        0,
                        &format!("data->{value}"),
                        &value_key,
                    );
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {free_code} data->{value} = {clone}(__child); {value} = data->{value}; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else if value_key == "ForeignHandle" || value_key.starts_with("ForeignHandle_") {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} __child = *(({cty} *)aura_task_frame_result(data->await_task).data); if (__child != NULL && aura_ffi_handle_retain(__child) != AURA_FFI_OK) return AURA_TASK_FAILED; if (data->{value} != NULL) (void)aura_ffi_handle_drop(&data->{value}); data->{value} = __child; {value} = __child; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else if crate::expr::is_enum_mono(&value_key, checked) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {cty}_drop(&data->{value}); data->{value} = {cty}_clone(__child); {value} = data->{value}; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else if crate::expr::is_value_struct_mono(&value_key, checked) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {cty}_drop(&data->{value}); data->{value} = {cty}_clone(__child); {value} = data->{value}; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else if is_iface_type_key(&value_key, checked) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {cty}_drop(&data->{value}); data->{value} = {cty}_clone(__child); {value} = data->{value}; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                } else {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(
                        out,
                        "        if (aura_task_frame_result(data->await_task).data != NULL) {{ data->{value} = *(({cty} *)aura_task_frame_result(data->await_task).data); {value} = data->{value}; }} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;"
                    );
                }
            }
            AsyncCfgNode::AwaitUnit {
                operand,
                owns_task,
                next,
            } => {
                let _ = writeln!(out, "        if (data->await_task == NULL) {{ data->await_task = {operand}; data->await_task_owned = {}; }}", if owns_task { "true" } else { "false" });
                out.push_str("        if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
                out.push_str("        AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
                let _ = writeln!(out, "        if (child_state == AURA_TASK_PENDING) {{ {sync} aura_task_frame_set_resume_state(frame, {state}); if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}");
                out.push_str("        if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n        if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n        if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
                let _ = writeln!(out, "        if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;");
            }
            AsyncCfgNode::AwaitCatch {
                operand,
                owns_task,
                catch_name,
                catch_key,
                catch_state,
                failure_state,
                finally_state,
                next,
            } => {
                let _ = writeln!(out, "        if (data->await_task == NULL) {{ data->await_task = {operand}; data->await_task_owned = {}; }}", if owns_task { "true" } else { "false" });
                out.push_str("        if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
                out.push_str("        AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
                let _ = writeln!(out, "        if (child_state == AURA_TASK_PENDING) {{ {sync} aura_task_frame_set_resume_state(frame, {state}); if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}");
                if let Some(finally_state) = finally_state {
                    let _ = writeln!(
                        out,
                        "        if (child_state == AURA_TASK_CANCELLED) {{ data->await_cancelled = true; if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {finally_state}); continue; }}"
                    );
                } else {
                    out.push_str("        if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
                }
                let catch_body = match catch_key.as_str() {
                    "String" => format!("const char *__src = (const char *)__error.data; size_t __len = __src == NULL ? 0 : strlen(__src); if (data->{catch_name}__owned && data->{catch_name} != NULL) free((void *)data->{catch_name}); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) return AURA_TASK_FAILED; if (__src != NULL) memcpy(__copy, __src, __len + 1); else __copy[0] = '\\0'; data->{catch_name} = __copy; data->{catch_name}__owned = true; {catch_name} = data->{catch_name}; {catch_name}__owned = true;"),
                    "Int" => format!("const char *__src = (const char *)__error.data; char *__end = NULL; long long __parsed = __src == NULL ? 0 : strtoll(__src, &__end, 10); if (__src == NULL || __end == __src || *__end != '\\0') {{ (void)aura_task_frame_propagate_error(frame, data->await_task); if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; return AURA_TASK_FAILED; }} {catch_name} = (int64_t)__parsed;"),
                    "Bool" => format!("const char *__src = (const char *)__error.data; if (__src == NULL || (strcmp(__src, \"true\") != 0 && strcmp(__src, \"false\") != 0)) {{ (void)aura_task_frame_propagate_error(frame, data->await_task); if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; return AURA_TASK_FAILED; }} {catch_name} = strcmp(__src, \"true\") == 0;"),
                    other if other == "ForeignHandle" || other.starts_with("ForeignHandle_") =>
                        async_cfg_foreign_handle_catch_body(&catch_name),
                    other if is_array_type_key(other) => {
                        let cty = crate::stmt::local_key_to_c(other, checked);
                        let clone = crate::names::c_method_name(&full_type_mono(other, checked), "clone");
                        let mut free_code = String::new();
                        crate::array_emit::emit_array_contents_free(
                            &mut free_code,
                            0,
                            &format!("data->{catch_name}"),
                            other,
                        );
                        format!("AuraTaskResult __payload = aura_task_frame_error_payload(data->await_task); {cty} *__source = ({cty} *)__payload.data; if (__source == NULL) return AURA_TASK_FAILED; {free_code} data->{catch_name} = {clone}(__source); {catch_name} = data->{catch_name};")
                    }
                    other if async_cfg_throw_aggregate_supported(other, checked) =>
                        async_cfg_aggregate_catch_body(other, &catch_name, checked),
                    other if async_cfg_throw_class_supported(other, checked) =>
                        async_cfg_class_catch_body(other, &catch_name, checked),
                    _ => unreachable!("validated async catch"),
                };
                let expected_type = catch_key.as_str();
                let mismatch = finally_state.map_or_else(
                    || "(void)aura_task_frame_propagate_error(frame, data->await_task); if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; return AURA_TASK_FAILED;".into(),
                    |state| format!("data->await_failed = true; aura_task_frame_set_resume_state(frame, {state}); continue;"),
                );
                let catch_success_prefix = failure_state
                    .map(|_| "data->await_failed = false; ")
                    .unwrap_or("");
                let _ = writeln!(out, "        if (child_state == AURA_TASK_FAILED) {{ AuraTaskResult __error = aura_task_frame_error(data->await_task); const char *__type = aura_task_frame_error_type_name(data->await_task); if (__type == NULL || strcmp(__type, \"{expected_type}\") != 0) {{ {mismatch} }} {catch_success_prefix}{catch_body} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {catch_state}); continue; }}");
                out.push_str(
                    "        if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n",
                );
                let _ = writeln!(out, "        if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;");
            }
            AsyncCfgNode::AwaitCatchValue {
                value,
                value_key,
                operand,
                owns_task,
                catch_name,
                catch_key,
                catch_state,
                failure_state,
                finally_state,
                next,
            } => {
                let _ = writeln!(out, "        if (data->await_task == NULL) {{ data->await_task = {operand}; data->await_task_owned = {}; }}", if owns_task { "true" } else { "false" });
                out.push_str("        if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
                out.push_str("        AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
                let _ = writeln!(out, "        if (child_state == AURA_TASK_PENDING) {{ {sync} aura_task_frame_set_resume_state(frame, {state}); if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}");
                if let Some(finally_state) = finally_state {
                    let _ = writeln!(
                        out,
                        "        if (child_state == AURA_TASK_CANCELLED) {{ data->await_cancelled = true; if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {finally_state}); continue; }}"
                    );
                } else {
                    out.push_str("        if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
                }
                let catch_body = match catch_key.as_str() {
                    "String" => format!("const char *__src = (const char *)__error.data; size_t __len = __src == NULL ? 0 : strlen(__src); if (data->{catch_name}__owned && data->{catch_name} != NULL) free((void *)data->{catch_name}); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) return AURA_TASK_FAILED; if (__src != NULL) memcpy(__copy, __src, __len + 1); else __copy[0] = '\\0'; data->{catch_name} = __copy; data->{catch_name}__owned = true; {catch_name} = data->{catch_name}; {catch_name}__owned = true;"),
                    "Int" => format!("const char *__src = (const char *)__error.data; char *__end = NULL; long long __parsed = __src == NULL ? 0 : strtoll(__src, &__end, 10); if (__src == NULL || __end == __src || *__end != '\\0') {{ (void)aura_task_frame_propagate_error(frame, data->await_task); if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; return AURA_TASK_FAILED; }} {catch_name} = (int64_t)__parsed;"),
                    "Bool" => format!("const char *__src = (const char *)__error.data; if (__src == NULL || (strcmp(__src, \"true\") != 0 && strcmp(__src, \"false\") != 0)) {{ (void)aura_task_frame_propagate_error(frame, data->await_task); if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; return AURA_TASK_FAILED; }} {catch_name} = strcmp(__src, \"true\") == 0;"),
                    other if other == "ForeignHandle" || other.starts_with("ForeignHandle_") =>
                        async_cfg_foreign_handle_catch_body(&catch_name),
                    other if is_array_type_key(other) => {
                        let cty = crate::stmt::local_key_to_c(other, checked);
                        let clone = crate::names::c_method_name(&full_type_mono(other, checked), "clone");
                        let mut free_code = String::new();
                        crate::array_emit::emit_array_contents_free(
                            &mut free_code,
                            0,
                            &format!("data->{catch_name}"),
                            other,
                        );
                        format!("AuraTaskResult __payload = aura_task_frame_error_payload(data->await_task); {cty} *__source = ({cty} *)__payload.data; if (__source == NULL) return AURA_TASK_FAILED; {free_code} data->{catch_name} = {clone}(__source); {catch_name} = data->{catch_name};")
                    }
                    other if async_cfg_throw_class_supported(other, checked) =>
                        async_cfg_class_catch_body(other, &catch_name, checked),
                    _ => unreachable!("validated async catch"),
                };
                let expected_type = catch_key.as_str();
                let mismatch = finally_state.map_or_else(
                    || "(void)aura_task_frame_propagate_error(frame, data->await_task); if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; return AURA_TASK_FAILED;".into(),
                    |state| format!("data->await_failed = true; aura_task_frame_set_resume_state(frame, {state}); continue;"),
                );
                let catch_success_prefix = failure_state
                    .map(|_| "data->await_failed = false; ")
                    .unwrap_or("");
                let _ = writeln!(out, "        if (child_state == AURA_TASK_FAILED) {{ AuraTaskResult __error = aura_task_frame_error(data->await_task); const char *__type = aura_task_frame_error_type_name(data->await_task); if (__type == NULL || strcmp(__type, \"{expected_type}\") != 0) {{ {mismatch} }} {catch_success_prefix}{catch_body} if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {catch_state}); continue; }}");
                out.push_str(
                    "        if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n",
                );
                if value_key == "String" {
                    let _ = writeln!(out, "        if (aura_task_frame_result(data->await_task).data != NULL) {{ const char *__src = *((const char **)aura_task_frame_result(data->await_task).data); if (data->{value}__owned && data->{value} != NULL) free((void *)data->{value}); data->{value} = NULL; data->{value}__owned = false; if (__src != NULL) {{ size_t __len = strlen(__src); data->{value} = (char *)malloc(__len + 1); if (data->{value} == NULL) return AURA_TASK_FAILED; memcpy((void *)data->{value}, __src, __len + 1); data->{value}__owned = true; }} {value} = data->{value}; {value}__owned = data->{value}__owned; }}");
                } else if is_fun_type_key(&value_key) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(out, "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; if (data->{value}.env != NULL) aura_fun_env_free(data->{value}.env); data->{value} = *__child; if (data->{value}.env != NULL) aura_fun_env_retain(data->{value}.env); {value} = data->{value}; }}");
                } else if is_array_type_key(&value_key) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let clone_key = full_type_mono(&value_key, checked);
                    let clone = crate::names::c_method_name(&clone_key, "clone");
                    let mut free_code = String::new();
                    crate::array_emit::emit_array_contents_free(
                        &mut free_code,
                        0,
                        &format!("data->{value}"),
                        &value_key,
                    );
                    let _ = writeln!(out, "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {free_code} data->{value} = {clone}(__child); {value} = data->{value}; }}");
                } else if crate::expr::is_enum_mono(&value_key, checked) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(out, "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {cty}_drop(&data->{value}); data->{value} = {cty}_clone(__child); {value} = data->{value}; }}");
                } else if is_iface_type_key(&value_key, checked) {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(out, "        if (aura_task_frame_result(data->await_task).data != NULL) {{ {cty} *__child = ({cty} *)aura_task_frame_result(data->await_task).data; {cty}_drop(&data->{value}); data->{value} = {cty}_clone(__child); {value} = data->{value}; }}");
                } else {
                    let cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(out, "        if (aura_task_frame_result(data->await_task).data != NULL) {{ data->{value} = *(({cty} *)aura_task_frame_result(data->await_task).data); {value} = data->{value}; }}");
                }
                let _ = writeln!(out, "        if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {next}); continue;");
            }
            AsyncCfgNode::AwaitFinally {
                operand,
                owns_task,
                finally_state,
                next,
            } => {
                let _ = writeln!(out, "        if (data->await_task == NULL) {{ data->await_task = {operand}; data->await_task_owned = {}; data->await_failed = false; data->await_cancelled = false; }}", if owns_task { "true" } else { "false" });
                out.push_str("        if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
                out.push_str("        AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
                let _ = writeln!(out, "        if (child_state == AURA_TASK_PENDING) {{ {sync} aura_task_frame_set_resume_state(frame, {state}); if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}");
                out.push_str(
                    "        if (child_state == AURA_TASK_CANCELLED) { data->await_cancelled = true; if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, ",
                );
                let _ = writeln!(out, "{finally_state}); continue; }}");
                let _ = writeln!(out, "        if (child_state == AURA_TASK_FAILED) {{ if (!aura_task_frame_propagate_error(frame, data->await_task)) return AURA_TASK_FAILED; data->await_failed = true; if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {finally_state}); continue; }}");
                out.push_str(
                    "        if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n",
                );
                let _ = writeln!(out, "        data->await_failed = false; if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {finally_state}); continue;");
                let _ = next;
            }
            AsyncCfgNode::Fail => {
                out.push_str("        return AURA_TASK_FAILED;\n");
            }
            AsyncCfgNode::Cancel => {
                out.push_str("        return AURA_TASK_CANCELLED;\n");
            }
            AsyncCfgNode::Throw {
                value,
                value_key,
                span_start,
                span_end,
            } => match value_key.as_str() {
                "String" => {
                    let _ = writeln!(
                        out,
                        "        const char *__throw_source = {value}; size_t __throw_length = __throw_source == NULL ? 0 : strlen(__throw_source); char *__throw_error = (char *)malloc(__throw_length + 1); if (__throw_error == NULL) return AURA_TASK_FAILED; if (__throw_source != NULL) memcpy(__throw_error, __throw_source, __throw_length + 1); else __throw_error[0] = '\\0'; aura_task_frame_set_error_span_with_clone(frame, __throw_error, __throw_length + 1, {clone_error}, {destroy_error}, UINT32_C({span_start}), UINT32_C({span_start}), UINT32_C({span_end})); aura_task_frame_set_error_type_name(frame, \"String\"); return AURA_TASK_FAILED;"
                    );
                }
                "Int" => {
                    let _ = writeln!(
                        out,
                        "        char *__throw_error = (char *)malloc(32); if (__throw_error == NULL) return AURA_TASK_FAILED; (void)snprintf(__throw_error, 32, \"%lld\", (long long)({value})); aura_task_frame_set_error_span_with_clone(frame, __throw_error, strlen(__throw_error) + 1, {clone_error}, {destroy_error}, UINT32_C({span_start}), UINT32_C({span_start}), UINT32_C({span_end})); aura_task_frame_set_error_type_name(frame, \"Int\"); return AURA_TASK_FAILED;"
                    );
                }
                "Bool" => {
                    let _ = writeln!(
                        out,
                        "        const char *__throw_source = ({value}) ? \"true\" : \"false\"; size_t __throw_length = strlen(__throw_source); char *__throw_error = (char *)malloc(__throw_length + 1); if (__throw_error == NULL) return AURA_TASK_FAILED; memcpy(__throw_error, __throw_source, __throw_length + 1); aura_task_frame_set_error_span_with_clone(frame, __throw_error, __throw_length + 1, {clone_error}, {destroy_error}, UINT32_C({span_start}), UINT32_C({span_start}), UINT32_C({span_end})); aura_task_frame_set_error_type_name(frame, \"Bool\"); return AURA_TASK_FAILED;"
                    );
                }
                other if other == "ForeignHandle" || other.starts_with("ForeignHandle_") => {
                    let clone = format!("aura_async_cfg_foreign_error_clone_{base}");
                    let destroy = format!("aura_async_cfg_foreign_error_destroy_{base}");
                    let _ = writeln!(
                        out,
                        "        AuraFfiOpaqueHandle *__throw_handle = {value}; if (__throw_handle != NULL && aura_ffi_handle_retain(__throw_handle) != AURA_FFI_OK) return AURA_TASK_FAILED; AuraFfiOpaqueHandle **__throw_payload = (AuraFfiOpaqueHandle **)malloc(sizeof(*__throw_payload)); if (__throw_payload == NULL) {{ if (__throw_handle != NULL) (void)aura_ffi_handle_drop(&__throw_handle); return AURA_TASK_FAILED; }} *__throw_payload = __throw_handle; const char *__throw_text = \"{other}\"; size_t __throw_length = strlen(__throw_text); char *__throw_error = (char *)malloc(__throw_length + 1); if (__throw_error == NULL) {{ {destroy}(__throw_payload, sizeof(*__throw_payload)); return AURA_TASK_FAILED; }} memcpy(__throw_error, __throw_text, __throw_length + 1); aura_task_frame_set_error_span_with_clone(frame, __throw_error, __throw_length + 1, {clone_error}, {destroy_error}, UINT32_C({span_start}), UINT32_C({span_start}), UINT32_C({span_end})); aura_task_frame_set_error_payload_with_clone(frame, __throw_payload, sizeof(*__throw_payload), {clone}, {destroy}); aura_task_frame_set_error_type_name(frame, \"{other}\"); return AURA_TASK_FAILED;"
                    );
                }
                other if is_array_type_key(other) => {
                    let cty = crate::stmt::local_key_to_c(other, checked);
                    let full_key = full_type_mono(other, checked);
                    let clone = crate::names::c_method_name(&full_key, "clone");
                    let suffix = mangle_ident(&full_key);
                    let payload_clone = format!("aura_async_cfg_array_error_clone_{suffix}");
                    let payload_destroy = format!("aura_async_cfg_array_error_destroy_{suffix}");
                    let _ = writeln!(
                        out,
                        "        {cty} *__throw_payload = ({cty} *)malloc(sizeof(*__throw_payload)); if (__throw_payload == NULL) return AURA_TASK_FAILED; *__throw_payload = {clone}(({cty} *)&({value})); const char *__throw_text = \"{other}\"; size_t __throw_length = strlen(__throw_text); char *__throw_error = (char *)malloc(__throw_length + 1); if (__throw_error == NULL) {{ {payload_destroy}(__throw_payload, sizeof(*__throw_payload)); return AURA_TASK_FAILED; }} memcpy(__throw_error, __throw_text, __throw_length + 1); aura_task_frame_set_error_span_with_clone(frame, __throw_error, __throw_length + 1, {clone_error}, {destroy_error}, UINT32_C({span_start}), UINT32_C({span_start}), UINT32_C({span_end})); aura_task_frame_set_error_payload_with_clone(frame, __throw_payload, sizeof(*__throw_payload), {payload_clone}, {payload_destroy}); aura_task_frame_set_error_type_name(frame, \"{other}\"); return AURA_TASK_FAILED;"
                    );
                }
                other if async_cfg_throw_class_supported(other, checked) => {
                    let mono = full_type_mono(other, checked);
                    let Some(base) = mono_base_name(&mono, checked) else {
                        unreachable!("validated class CFG throw must have a base name")
                    };
                    let obj_cty = crate::stmt::local_key_to_c(&mono, checked);
                    let struct_cty = c_class_type(&mono);
                    let suffix = mangle_ident(&mono);
                    let clone = format!("aura_async_cfg_class_error_clone_{base}_{suffix}");
                    let destroy = format!("aura_async_cfg_class_error_destroy_{base}_{suffix}");
                    let message_expr = checked
                        .ast
                        .classes
                        .iter()
                        .find(|class| class.name.name == base && class.type_params.is_empty())
                        .and_then(|class| {
                            class.fields.iter().find_map(|field| {
                                (field.name.name == "message"
                                    && type_ref_local_key(&field.ty, &[], &[]) == "String")
                                .then(|| {
                                    let name = mangle_ident(&field.name.name);
                                    format!("(__throw_obj != NULL && __throw_obj->{name} != NULL) ? __throw_obj->{name} : \"{base}\"")
                                })
                            })
                        })
                        .unwrap_or_else(|| format!("\"{base}\""));
                    let _ = writeln!(
                        out,
                        "        {obj_cty} __throw_obj = ({value}); const char *__throw_text = {message_expr}; size_t __throw_length = __throw_text == NULL ? 0 : strlen(__throw_text); char *__throw_error = (char *)malloc(__throw_length + 1); if (__throw_error == NULL) return AURA_TASK_FAILED; if (__throw_text != NULL) memcpy(__throw_error, __throw_text, __throw_length + 1); else __throw_error[0] = '\\0'; size_t __throw_payload_size = 0; void *__throw_payload = {clone}((const void *)__throw_obj, sizeof({struct_cty}), &__throw_payload_size); if (__throw_payload == NULL) {{ free(__throw_error); return AURA_TASK_FAILED; }} aura_task_frame_set_error_span_with_clone(frame, __throw_error, __throw_length + 1, {clone_error}, {destroy_error}, UINT32_C({span_start}), UINT32_C({span_start}), UINT32_C({span_end})); aura_task_frame_set_error_payload_with_clone(frame, __throw_payload, __throw_payload_size, {clone}, {destroy}); aura_task_frame_set_error_type_name(frame, \"{base}\"); return AURA_TASK_FAILED;"
                    );
                }
                other if async_cfg_throw_aggregate_supported(other, checked) => {
                    let cty = crate::stmt::local_key_to_c(other, checked);
                    let full_key = full_type_mono(other, checked);
                    let suffix = mangle_ident(&full_key);
                    let payload_clone = format!("aura_async_cfg_aggregate_error_clone_{suffix}");
                    let payload_destroy =
                        format!("aura_async_cfg_aggregate_error_destroy_{suffix}");
                    let source_name = format!("__throw_aggregate_{}", span_start);
                    let copy = if is_fun_type_key(other) {
                        format!(
                            "*__throw_payload = {source_name}; if (__throw_payload->env != NULL) aura_fun_env_retain(__throw_payload->env);"
                        )
                    } else {
                        format!("*__throw_payload = {cty}_clone(&{source_name});")
                    };
                    let _ = writeln!(
                        out,
                        "        {cty} {source_name} = {value}; {cty} *__throw_payload = ({cty} *)malloc(sizeof(*__throw_payload)); if (__throw_payload == NULL) return AURA_TASK_FAILED; {copy} const char *__throw_text = \"{other}\"; size_t __throw_length = strlen(__throw_text); char *__throw_error = (char *)malloc(__throw_length + 1); if (__throw_error == NULL) {{ {payload_destroy}(__throw_payload, sizeof(*__throw_payload)); return AURA_TASK_FAILED; }} memcpy(__throw_error, __throw_text, __throw_length + 1); aura_task_frame_set_error_span_with_clone(frame, __throw_error, __throw_length + 1, {clone_error}, {destroy_error}, UINT32_C({span_start}), UINT32_C({span_start}), UINT32_C({span_end})); aura_task_frame_set_error_payload_with_clone(frame, __throw_payload, sizeof(*__throw_payload), {payload_clone}, {payload_destroy}); aura_task_frame_set_error_type_name(frame, \"{other}\"); return AURA_TASK_FAILED;"
                    );
                }
                _ => unreachable!("validated CFG throw kind"),
            },
            AsyncCfgNode::Return {
                value,
                value_key,
                value_is_ident,
                value_is_owned_temp,
            } => {
                let result_cty = crate::stmt::local_key_to_c(&value_key, checked);
                if value_key == "Unit" {
                    out.push_str("        return AURA_TASK_COMPLETE;\n");
                } else if value_key == "String" {
                    let _ = writeln!(
                        out,
                        "        const char *__src = {value}; const char *__copy = NULL; if (__src != NULL) {{ size_t __len = strlen(__src); char *__owned = (char *)malloc(__len + 1); if (__owned == NULL) {{ {free_src_on_error} return AURA_TASK_FAILED; }} memcpy(__owned, __src, __len + 1); __copy = __owned; }} {free_src} const char **__aura_result = (const char **)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) {{ free((void *)__copy); return AURA_TASK_FAILED; }} *__aura_result = __copy; aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;",
                        free_src_on_error = if value_is_owned_temp { "free((void *)__src);" } else { "" },
                        free_src = if value_is_owned_temp { "if (__src != NULL) free((void *)__src);" } else { "" },
                    );
                } else if crate::expr::is_value_struct_mono(&value_key, checked) {
                    let _ = writeln!(
                        out,
                        "        {result_cty} __returned = {value}; {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_cty}_clone(&__returned); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
                    );
                } else if is_array_type_key(&value_key) {
                    let clone_key = full_type_mono(&value_key, checked);
                    let clone = crate::names::c_method_name(&clone_key, "clone");
                    if value_is_ident {
                        let _ = writeln!(
                            out,
                            "        {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {clone}(&{value}); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
                        );
                    } else {
                        let mut free_code = String::new();
                        if value_is_owned_temp {
                            crate::array_emit::emit_array_contents_free(
                                &mut free_code,
                                0,
                                "__returned",
                                &value_key,
                            );
                        }
                        let _ = writeln!(
                            out,
                            "        {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; {result_cty} __returned = {value}; *__aura_result = {clone}(&__returned); {free_code} aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
                        );
                    }
                } else if is_heap_class_mono(&value_key, checked) {
                    let _ = writeln!(
                        out,
                        "        {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {value}; aura_gc_add_root((void **)__aura_result); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
                    );
                } else if value_key == "ForeignHandle" || value_key.starts_with("ForeignHandle_") {
                    let result_cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let _ = writeln!(
                        out,
                        "        {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {value}; if (*__aura_result != NULL && aura_ffi_handle_retain(*__aura_result) != AURA_FFI_OK) {{ free(__aura_result); return AURA_TASK_FAILED; }} aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
                    );
                } else if value_key == "Channel"
                    || value_key.starts_with("Channel_")
                    || value_key == "Task"
                    || value_key.starts_with("Task_")
                    || value_key == "TaskHandle"
                    || value_key.starts_with("TaskHandle_")
                {
                    let result_cty = crate::stmt::local_key_to_c(&value_key, checked);
                    let is_channel = value_key == "Channel" || value_key.starts_with("Channel_");
                    let retain = if is_channel {
                        "if (*__aura_result != NULL && !aura_task_channel_retain(*__aura_result)) { free(__aura_result); return AURA_TASK_FAILED; }"
                    } else {
                        "if (*__aura_result != NULL && (__aura_task_executor == NULL || !aura_task_executor_retain_payload(__aura_task_executor, *__aura_result))) { free(__aura_result); return AURA_TASK_FAILED; }"
                    };
                    let _ = writeln!(
                        out,
                        "        {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {value}; {retain} aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
                    );
                } else if crate::expr::is_enum_mono(&value_key, checked) {
                    let _ = writeln!(
                        out,
                        "        {result_cty} __returned = {value}; {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_cty}_clone(&__returned); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
                    );
                } else if crate::stmt::is_shared_outcome_error_owner_key(&value_key) {
                    let _ = writeln!(
                out,
                "        {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {value}; if (__aura_result->tag == 1 && __aura_result->data.OutcomeErr.error != NULL) {{ aura_gc_add_root((void **)&__aura_result->data.OutcomeErr.error); __aura_result->data.OutcomeErr.owned = true; }} aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
            );
                } else if crate::expr::is_enum_mono(&value_key, checked) {
                    let _ = writeln!(
                out,
                "        {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_cty}_clone(&{value}); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;"
            );
                } else {
                    let _ = writeln!(out, "        {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {value}; aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result}); return AURA_TASK_COMPLETE;");
                }
            }
        }
        out.push_str("      }\n");
    }
    out.push_str("      default: return AURA_TASK_FAILED;\n    }\n  }\n}\n\n");
    if !finally_states.is_empty() {
        // Runtime cancellation normally short-circuits before the poller. Run
        // the CFG once through its cancellation/finally states so nested
        // finally blocks execute before the runtime publishes cancellation.
        let _ = writeln!(
            out,
            "static AuraTaskPollState {cancel_fn}(AuraTaskFrame *frame) {{ return {poll_fn}(frame); }}\n"
        );
    }
    let async_signature = if boxed_params.is_empty() {
        c_async_fun_signature(f, checked)
    } else {
        let signature_params = f
            .params
            .iter()
            .map(|param| {
                let key = full_type_mono(
                    &type_ref_local_key_expand(&param.ty, &params, &[], checked),
                    checked,
                );
                let ty = if boxed_params.contains(&param.name.name) {
                    general_spawn_box_c_type(&key).to_string()
                } else {
                    c_type_ref_subst(&param.ty, checked, &params, &[])
                };
                format!("{} {}", ty, mangle_ident(&param.name.name))
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "AuraTaskFrame * {}({})",
            c_fun_name(&pkg, &f.name.name, &[]),
            if signature_params.is_empty() {
                "void".into()
            } else {
                signature_params
            }
        )
    };
    let _ = writeln!(out, "{} {{", async_signature);
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data}); if (frame == NULL) return NULL; aura_task_frame_set_race_source_id(frame, UINT32_C({}));", f.span.start);
    if !finally_states.is_empty() {
        let _ = writeln!(
            out,
            "  aura_task_frame_set_cancel_handler(frame, {cancel_fn});"
        );
    }
    let _ = writeln!(out, "  aura_task_frame_set_gc_mark(frame, {gc_mark}); aura_task_frame_set_data_drop(frame, {data_drop});");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for param in &f.params {
        let name = mangle_ident(&param.name.name);
        let key = full_type_mono(
            &type_ref_local_key_expand(&param.ty, &params, &[], checked),
            checked,
        );
        if boxed_params.contains(&param.name.name) {
            let _ = writeln!(out, "  data->{name} = {name};");
            let retain = general_spawn_box_retain(&key);
            let _ = writeln!(out, "  if (data->{name} != NULL) {retain}(data->{name});");
        } else if key == "String" {
            let _ = writeln!(out, "  data->{name} = NULL; data->{name}__owned = false;");
            let _ = writeln!(
                out,
                "  if ({name} != NULL) {{ size_t __len = strlen({name}); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) {{ aura_task_frame_destroy(frame); return NULL; }} memcpy(__copy, {name}, __len + 1); data->{name} = __copy; data->{name}__owned = true; }}"
            );
        } else if is_array_type_key(&key) {
            let clone = crate::names::c_method_name(&key, "clone");
            let _ = writeln!(out, "  data->{name} = {clone}(&{name});");
            if crate::array_emit::is_array_of_heap_class(&key, checked) {
                let root = crate::array_emit::array_gc_root_add_call(
                    &format!("data->{name}.data"),
                    &format!("data->{name}.len"),
                    &key,
                    checked,
                );
                let _ = writeln!(out, "  {root}");
            }
        } else if crate::expr::is_enum_mono(&key, checked)
            || is_iface_type_key(&key, checked)
            || crate::expr::is_value_struct_mono(&key, checked)
        {
            let cty = crate::stmt::local_key_to_c(&key, checked);
            let _ = writeln!(out, "  data->{name} = {cty}_clone(&{name});");
        } else if is_fun_type_key(&key) {
            let _ = writeln!(out, "  data->{name} = {name}; if (data->{name}.env != NULL) aura_fun_env_retain(data->{name}.env);");
        } else {
            let _ = writeln!(out, "  data->{name} = {name};");
        }
        if key == "Channel" || key.starts_with("Channel_") {
            let _ = writeln!(out, "  if (data->{name} != NULL && !aura_task_channel_retain(data->{name})) {{ data->{name} = NULL; aura_task_frame_destroy(frame); return NULL; }}");
        } else if key == "Task"
            || key.starts_with("Task_")
            || key == "TaskHandle"
            || key.starts_with("TaskHandle_")
        {
            let _ = writeln!(out, "  if (data->{name} != NULL && (__aura_task_executor == NULL || !aura_task_executor_retain_payload(__aura_task_executor, data->{name}))) {{ data->{name} = NULL; aura_task_frame_destroy(frame); return NULL; }}");
        } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let _ = writeln!(out, "  if (data->{name} != NULL && aura_ffi_handle_retain(data->{name}) != AURA_FFI_OK) {{ data->{name} = NULL; aura_task_frame_destroy(frame); return NULL; }}");
        }
    }
    let mut emit_cfg_local_init = |name: &str, key: &str| {
        let name = mangle_ident(name);
        if key == "String" {
            let _ = writeln!(out, "  data->{name} = NULL; data->{name}__owned = false;");
        } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let _ = writeln!(out, "  data->{name} = NULL;");
        } else if async_cfg_scheduler_owned_key(key) || is_heap_class_mono(key, checked) {
            let _ = writeln!(out, "  data->{name} = NULL;");
        } else if is_array_type_key(key)
            || crate::expr::is_enum_mono(key, checked)
            || crate::expr::is_value_struct_mono(key, checked)
            || is_iface_type_key(key, checked)
            || is_fun_type_key(key)
            || key.starts_with("Opt_")
        {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "  data->{name} = ({cty}){{0}};");
        } else {
            let _ = writeln!(out, "  data->{name} = 0;");
        }
    };
    for (var, key) in &vars {
        emit_cfg_local_init(&var.name.name, key);
    }
    for (name, key) in &cfg_locals {
        emit_cfg_local_init(name, key);
    }
    for (name, key) in &match_bindings {
        emit_cfg_local_init(name, key);
    }
    let _ = writeln!(out, "  data->await_task = NULL; data->await_task_owned = false; aura_task_frame_set_resume_state(frame, {entry});");
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Lower the smallest nested-loop CFG slice: an outer Int loop containing an
/// inner Int loop with one awaited Int value and scalar updates after it.
///
/// This shape keeps the branch choice and child frame stable across a pending
/// poll, then resumes through one loop continuation before reevaluating the
/// loop condition.
fn emit_async_fun_while_branch_join_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 5
    {
        return false;
    }
    let Stmt::Var(index_var) = &f.body.stmts[0] else {
        return false;
    };
    let Stmt::Var(total_var) = &f.body.stmts[1] else {
        return false;
    };
    let Stmt::Var(value_var) = &f.body.stmts[2] else {
        return false;
    };
    let Stmt::While(loop_stmt) = &f.body.stmts[3] else {
        return false;
    };
    let Stmt::Return(return_stmt) = &f.body.stmts[4] else {
        return false;
    };
    if !index_var.mutable
        || !total_var.mutable
        || !value_var.mutable
        || [index_var, total_var, value_var].iter().any(|var| {
            var.ty
                .as_ref()
                .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
                .unwrap_or(true)
                || matches!(&var.init, Expr::Async(_))
        })
        || !matches!(return_stmt.value.as_ref(), Some(Expr::Ident(id)) if id.name == total_var.name.name)
        || !matches!(loop_stmt.cond, Expr::Binary(_))
        || loop_stmt.body.stmts.len() < 3
    {
        return false;
    }
    let Stmt::If(branch) = &loop_stmt.body.stmts[0] else {
        return false;
    };
    let then_await = branch
        .then_block
        .stmts
        .first()
        .and_then(|stmt| branch_assign_await(stmt, &value_var.name.name));
    let Some(else_block) = &branch.else_block else {
        return false;
    };
    let else_await = else_block
        .stmts
        .first()
        .and_then(|stmt| branch_assign_await(stmt, &value_var.name.name));
    let (Some(then_await), Some(else_await)) = (then_await, else_await) else {
        return false;
    };
    if branch.then_block.stmts.len() != 1
        || else_block.stmts.len() != 1
        || matches!(branch.cond, Expr::Async(_))
    {
        return false;
    }
    let gc_stmt = if loop_stmt.body.stmts.len() == 4 {
        let Stmt::Expr(Expr::Call(call)) = &loop_stmt.body.stmts[1] else {
            return false;
        };
        if !matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
            || !call.args.is_empty()
        {
            return false;
        }
        Some(&loop_stmt.body.stmts[1])
    } else {
        None
    };
    let total_assign_index = if gc_stmt.is_some() { 2 } else { 1 };
    let index_assign_index = total_assign_index + 1;
    let Stmt::Expr(Expr::Assign(total_assign)) = &loop_stmt.body.stmts[total_assign_index] else {
        return false;
    };
    let Stmt::Expr(Expr::Assign(index_assign)) = &loop_stmt.body.stmts[index_assign_index] else {
        return false;
    };
    if total_assign.name.name != total_var.name.name
        || index_assign.name.name != index_var.name.name
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let index_name = mangle_ident(&index_var.name.name);
    let total_name = mangle_ident(&total_var.name.name);
    let value_name = mangle_ident(&value_var.name.name);

    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let index_init = coerce_expr(&index_var.init, "Int", &mut entry_ctx);
    entry_ctx.define_local(&index_var.name.name, "Int".into());
    let total_init = coerce_expr(&total_var.init, "Int", &mut entry_ctx);
    entry_ctx.define_local(&total_var.name.name, "Int".into());
    let value_init = coerce_expr(&value_var.init, "Int", &mut entry_ctx);
    entry_ctx.define_local(&value_var.name.name, "Int".into());
    let loop_cond = emit_expr(&loop_stmt.cond, &mut entry_ctx);
    let branch_cond = emit_expr(&branch.cond, &mut entry_ctx);
    let then_task = emit_expr(&then_await.operand, &mut entry_ctx);
    let else_task = emit_expr(&else_await.operand, &mut entry_ctx);

    let mut post_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for var in [index_var, total_var, value_var] {
        post_ctx.define_local(&var.name.name, "Int".into());
    }
    let gc_code = gc_stmt
        .map(|stmt| {
            let mut code = String::new();
            crate::stmt::emit_stmt(&mut code, stmt, 1, &mut post_ctx);
            code
        })
        .unwrap_or_default();
    let total_rhs = coerce_expr(&total_assign.value, "Int", &mut post_ctx);
    let index_rhs = coerce_expr(&index_assign.value, "Int", &mut post_ctx);

    let _ = writeln!(
        out,
        "/* aura async loop branch-join suspension states=2 spans={}:{}|{}:{} */",
        then_await.span.start, then_await.span.end, else_await.span.start, else_await.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(
        out,
        "  int64_t {index_name}; int64_t {total_name}; int64_t {value_name}; AuraTaskFrame *await_task;\n}} {data_ty};\n"
    );
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    let _ = writeln!(
        out,
        "      data->{index_name} = {index_init}; data->{total_name} = {total_init}; data->{value_name} = {value_init}; data->await_task = NULL; aura_task_frame_set_resume_state(frame, 1);"
    );
    out.push_str("      goto aura_async_loop_branch_head;\n    }\n    case 1: goto aura_async_loop_branch_head;\n    case 2: goto aura_async_loop_branch_poll;\n    default: return AURA_TASK_FAILED;\n  }\n\n");
    out.push_str("aura_async_loop_branch_head:\n  for (;;) {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "    {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "    int64_t {index_name} = data->{index_name}; int64_t {total_name} = data->{total_name}; int64_t {value_name} = data->{value_name};");
    let _ = writeln!(out, "    if (!({loop_cond})) break;");
    let _ = writeln!(
        out,
        "    data->await_task = ({branch_cond}) ? {then_task} : {else_task};"
    );
    out.push_str("    if (data->await_task == NULL) return AURA_TASK_FAILED;\n    aura_task_frame_set_resume_state(frame, 2);\n    goto aura_async_loop_branch_poll;\n  }\n");
    let _ = writeln!(out, "  int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = data->{total_name}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE;\n\n");
    out.push_str("aura_async_loop_branch_poll:\n");
    out.push_str("  { AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task); if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; } if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; } if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED; AuraTaskResult child_result = aura_task_frame_result(data->await_task); if (child_result.data != NULL) data->" );
    out.push_str(&value_name);
    out.push_str(" = *((int64_t *)child_result.data); data->await_task = NULL; ");
    let _ = writeln!(
        out,
        "int64_t {index_name} = data->{index_name}; int64_t {total_name} = data->{total_name}; int64_t {value_name} = data->{value_name};"
    );
    out.push_str(&gc_code);
    let _ = writeln!(out, " data->{total_name} = {total_rhs}; data->{index_name} = {index_rhs}; goto aura_async_loop_branch_head; }}\n}}");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, NULL); if (frame == NULL) return NULL;");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  data->await_task = NULL; if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; } return frame;\n}");
    true
}

fn emit_async_fun_nested_while_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 4
    {
        return false;
    }
    let Stmt::Var(outer) = &f.body.stmts[0] else {
        return false;
    };
    let Stmt::Var(total) = &f.body.stmts[1] else {
        return false;
    };
    let Stmt::While(outer_loop) = &f.body.stmts[2] else {
        return false;
    };
    let Some(Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(ret)),
        ..
    })) = f.body.stmts.last()
    else {
        return false;
    };
    if ret.name != total.name.name
        || !outer.mutable
        || outer
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
            != Some("Int".into())
        || total
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
            != Some("Int".into())
        || outer_loop.body.stmts.len() != 3
    {
        return false;
    }
    let Stmt::Var(inner) = &outer_loop.body.stmts[0] else {
        return false;
    };
    let Stmt::While(inner_loop) = &outer_loop.body.stmts[1] else {
        return false;
    };
    let Stmt::Expr(Expr::Assign(outer_assign)) = &outer_loop.body.stmts[2] else {
        return false;
    };
    if !inner.mutable
        || inner
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
            != Some("Int".into())
        || inner_loop.body.stmts.len() != 4
    {
        return false;
    }
    let Stmt::Var(await_var) = &inner_loop.body.stmts[0] else {
        return false;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        return false;
    };
    if await_var
        .ty
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
        != Some("Int".into())
        || matches!(await_expr.operand.as_ref(), Expr::Async(_))
    {
        return false;
    }
    let Stmt::Expr(Expr::Assign(total_assign)) = &inner_loop.body.stmts[1] else {
        return false;
    };
    let Stmt::Expr(Expr::Assign(inner_assign)) = &inner_loop.body.stmts[2] else {
        return false;
    };
    let Stmt::Expr(Expr::Call(gc_call)) = &inner_loop.body.stmts[3] else {
        return false;
    };
    if total_assign.name.name != total.name.name
        || inner_assign.name.name != inner.name.name
        || outer_assign.name.name != outer.name.name
        || !gc_call.args.is_empty()
        || !matches!(gc_call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
        || matches!(total_assign.value.as_ref(), Expr::Async(_))
        || matches!(inner_assign.value.as_ref(), Expr::Async(_))
        || matches!(outer_assign.value.as_ref(), Expr::Async(_))
    {
        return false;
    }
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let outer_name = mangle_ident(&outer.name.name);
    let total_name = mangle_ident(&total.name.name);
    let inner_name = mangle_ident(&inner.name.name);
    let await_name = mangle_ident(&await_var.name.name);
    let mut ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let outer_init = coerce_expr(&outer.init, "Int", &mut ctx);
    ctx.define_local(&outer.name.name, "Int".into());
    let total_init = coerce_expr(&total.init, "Int", &mut ctx);
    ctx.define_local(&total.name.name, "Int".into());
    let outer_cond = emit_expr(&outer_loop.cond, &mut ctx);
    let inner_init = coerce_expr(&inner.init, "Int", &mut ctx);
    ctx.define_local(&inner.name.name, "Int".into());
    let inner_cond = emit_expr(&inner_loop.cond, &mut ctx);
    let operand = emit_expr(&await_expr.operand, &mut ctx);
    ctx.define_local(&await_var.name.name, "Int".into());
    let total_rhs = coerce_expr(&total_assign.value, "Int", &mut ctx);
    let inner_rhs = coerce_expr(&inner_assign.value, "Int", &mut ctx);
    let outer_rhs = coerce_expr(&outer_assign.value, "Int", &mut ctx);
    let mut post_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for name in [&outer.name, &total.name, &inner.name, &await_var.name] {
        post_ctx.define_local(&name.name, "Int".into());
    }
    let gc_code = emit_expr(&Expr::Call(gc_call.clone()), &mut post_ctx);
    let post_code = format!("{total_name} = {total_rhs}; {inner_name} = {inner_rhs}; {gc_code};");
    let _ = writeln!(out, "/* aura async nested while-await Int lowering */");
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "  int64_t {outer_name}; int64_t {total_name}; int64_t {inner_name}; int64_t {await_name}; AuraTaskFrame *await_task;\n}} {data_ty};\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
        if p.name.name == "this" {
            let cty = c_type_ref_subst(&p.ty, checked, &params, &[]);
            let _ = writeln!(out, "  {cty} this = a_this;");
        }
    }
    let _ = writeln!(out, "  int64_t {outer_name} = data->{outer_name}; int64_t {total_name} = data->{total_name}; int64_t {inner_name} = data->{inner_name}; int64_t {await_name} = data->{await_name};");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0:\n");
    let _ = writeln!(out, "      {outer_name} = {outer_init}; {total_name} = {total_init}; data->{outer_name} = {outer_name}; data->{total_name} = {total_name}; aura_task_frame_set_resume_state(frame, 1); goto aura_async_nested_outer_head;\n    case 1: goto aura_async_nested_outer_head;\n    case 2: {{");
    out.push_str("      AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task); if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; } if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; } if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED; AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n");
    let _ = writeln!(out, "      {await_name} = child_result.data == NULL ? 0 : *((int64_t *)child_result.data); data->await_task = NULL; {post_code} data->{total_name} = {total_name}; data->{inner_name} = {inner_name}; goto aura_async_nested_inner_head;\n    }}\n    default: return AURA_TASK_FAILED;\n  }}\n\n");
    out.push_str("aura_async_nested_outer_head:\n  for (;;) {\n");
    let _ = writeln!(out, "    if (!({outer_cond})) break; {inner_name} = {inner_init}; data->{inner_name} = {inner_name};\naura_async_nested_inner_head:\n    while ({inner_cond}) {{");
    let _ = writeln!(out, "      if (data->await_task == NULL) data->await_task = {operand}; if (data->await_task == NULL) return AURA_TASK_FAILED; AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task); if (child_state == AURA_TASK_PENDING) {{ data->{outer_name} = {outer_name}; data->{total_name} = {total_name}; data->{inner_name} = {inner_name}; aura_task_frame_set_resume_state(frame, 2); if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }} if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED; AuraTaskResult child_result = aura_task_frame_result(data->await_task); {await_name} = child_result.data == NULL ? 0 : *((int64_t *)child_result.data); data->await_task = NULL; {post_code} data->{total_name} = {total_name}; data->{inner_name} = {inner_name};\n    }} {outer_name} = {outer_rhs}; data->{outer_name} = {outer_name};\n  }}\n  int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = {total_name}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE;\n}}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, NULL); if (frame == NULL) return NULL;");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  data->await_task = NULL; if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; } return frame;\n}\n");
    true
}

/// Lower one bounded loop iteration with three or more independent conditional awaits.
/// Each selected child owns a distinct resume state; a false condition jumps
/// directly to the next gate without allocating or waiting on a task.
fn emit_async_fun_while_multi_conditional_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
    {
        return false;
    }
    let initial_count = f.body.stmts.len().saturating_sub(2);
    if initial_count < 5 {
        return false;
    }
    let branch_count = initial_count - 2;
    if f.body.stmts.len() != initial_count + 2 {
        return false;
    }
    let vars: Vec<&VarStmt> = f.body.stmts[..initial_count]
        .iter()
        .map(|stmt| match stmt {
            Stmt::Var(var) => Some(var),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    if vars.len() != initial_count {
        return false;
    }
    let index_var = vars[0];
    let total_var = vars[1];
    let branch_vars = &vars[2..];
    let Stmt::While(loop_stmt) = &f.body.stmts[initial_count] else {
        return false;
    };
    let Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(return_id)),
        ..
    }) = &f.body.stmts[initial_count + 1]
    else {
        return false;
    };
    if return_id.name != total_var.name.name
        || vars.iter().any(|var| {
            !var.mutable
                || var
                    .ty
                    .as_ref()
                    .map(|t| type_ref_local_key(t, &[], &[]) != "Int")
                    .unwrap_or(true)
                || matches!(&var.init, Expr::Async(_))
        })
        || !matches!(&loop_stmt.cond, Expr::Binary(_))
        || loop_stmt.body.stmts.len() != branch_count + 3
    {
        return false;
    }
    let mut awaits = Vec::with_capacity(branch_count);
    for (branch_stmt, branch_var) in loop_stmt.body.stmts[..branch_count].iter().zip(branch_vars) {
        let Stmt::If(branch) = branch_stmt else {
            return false;
        };
        if branch.else_block.is_some()
            || branch.then_block.stmts.len() != 1
            || matches!(&branch.cond, Expr::Async(_))
        {
            return false;
        }
        let Some(await_expr) =
            branch_assign_await(&branch.then_block.stmts[0], &branch_var.name.name)
        else {
            return false;
        };
        awaits.push((branch, await_expr));
    }
    let Stmt::Expr(Expr::Assign(total_assign)) = &loop_stmt.body.stmts[branch_count] else {
        return false;
    };
    let Stmt::Expr(Expr::Call(gc_call)) = &loop_stmt.body.stmts[branch_count + 1] else {
        return false;
    };
    let Stmt::Expr(Expr::Assign(index_assign)) = &loop_stmt.body.stmts[branch_count + 2] else {
        return false;
    };
    if total_assign.name.name != total_var.name.name
        || index_assign.name.name != index_var.name.name
        || !matches!(gc_call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
        || !gc_call.args.is_empty()
        || matches!(total_assign.value.as_ref(), Expr::Async(_))
        || matches!(index_assign.value.as_ref(), Expr::Async(_))
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let names: Vec<String> = vars
        .iter()
        .map(|var| mangle_ident(&var.name.name))
        .collect();
    let head_label = format!("aura_async_{base}_multi_cond_head");
    let post_label = format!("aura_async_{base}_multi_cond_post");
    let done_label = format!("aura_async_{base}_multi_cond_done");
    let gate_labels: Vec<String> = (1..branch_count)
        .map(|i| format!("aura_async_{base}_multi_cond_gate_{i}"))
        .collect();
    let poll_labels: Vec<String> = (0..branch_count)
        .map(|i| format!("aura_async_{base}_multi_cond_poll_{i}"))
        .collect();

    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for var in &vars {
        entry_ctx.define_local(&var.name.name, "Int".into());
    }
    let inits: Vec<String> = vars
        .iter()
        .map(|var| coerce_expr(&var.init, "Int", &mut entry_ctx))
        .collect();
    let loop_cond = emit_expr(&loop_stmt.cond, &mut entry_ctx);
    let branch_conds: Vec<String> = awaits
        .iter()
        .map(|(branch, _)| emit_expr(&branch.cond, &mut entry_ctx))
        .collect();
    let branch_tasks: Vec<String> = awaits
        .iter()
        .map(|(_, await_expr)| emit_expr(&await_expr.operand, &mut entry_ctx))
        .collect();
    let branch_owns_task: Vec<bool> = awaits
        .iter()
        .map(|(_, await_expr)| await_operand_is_temporary(&await_expr.operand, checked))
        .collect();

    let mut post_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for var in &vars {
        post_ctx.define_local(&var.name.name, "Int".into());
    }
    let total_rhs = coerce_expr(&total_assign.value, "Int", &mut post_ctx);
    let index_rhs = coerce_expr(&index_assign.value, "Int", &mut post_ctx);

    let _ = writeln!(
        out,
        "/* aura async loop multi-conditional suspension states={} branches={} */",
        branch_count + 1,
        branch_count
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    for name in &names {
        let _ = writeln!(out, "  int64_t {name};");
    }
    for i in 0..branch_count {
        let _ = writeln!(out, "  AuraTaskFrame *await_task_{i};");
    }
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for (index, owns_task) in branch_owns_task.iter().enumerate() {
        if *owns_task {
            let _ = writeln!(out, "  if (data->await_task_{index} != NULL && __aura_task_executor != NULL) {{ (void)aura_task_executor_release(__aura_task_executor, &data->await_task_{index}); }}");
        }
    }
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    for name in &names {
        let _ = writeln!(out, "  int64_t {name} = data->{name};");
    }
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0:\n");
    for (name, init) in names.iter().zip(&inits) {
        let _ = writeln!(out, "      data->{name} = {init};");
    }
    for i in 0..branch_count {
        let _ = writeln!(out, "      data->await_task_{i} = NULL;");
    }
    let _ = writeln!(
        out,
        "      aura_task_frame_set_resume_state(frame, 0); goto {head_label};"
    );
    for (i, label) in poll_labels.iter().enumerate() {
        let _ = writeln!(out, "    case {}: goto {label};", i + 1);
    }
    out.push_str("    default: return AURA_TASK_FAILED;\n  }\n\n");

    let _ = writeln!(out, "{head_label}:\n  for (;;) {{");
    for name in &names {
        let _ = writeln!(out, "    {name} = data->{name};");
    }
    let _ = writeln!(out, "    if (!({loop_cond})) goto {done_label};");
    for i in 0..branch_count {
        let next = if i + 1 < branch_count {
            gate_labels[i].as_str()
        } else {
            post_label.as_str()
        };
        let _ = writeln!(
            out,
            "    if ({}) {{ data->await_task_{i} = {}; if (data->await_task_{i} == NULL) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, {}); goto {}; }}",
            branch_conds[i],
            branch_tasks[i],
            i + 1,
            poll_labels[i]
        );
        let _ = writeln!(out, "    goto {next};");
        if i + 1 < branch_count {
            let _ = writeln!(out, "\n{}:", gate_labels[i]);
            for name in &names {
                let _ = writeln!(out, "  {name} = data->{name};");
            }
        }
    }
    out.push_str("  }\n\n");

    for i in 0..branch_count {
        let next = if i + 1 < branch_count {
            gate_labels[i].as_str()
        } else {
            post_label.as_str()
        };
        let _ = writeln!(out, "{label}:\n  {{", label = poll_labels[i]);
        out.push_str(&format!(
            "    AuraTaskPollState child_state = aura_task_frame_state(data->await_task_{i}); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_{i}); if (child_state == AURA_TASK_PENDING) {{ if (!aura_task_frame_wait_on(frame, data->await_task_{i})) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task_{i}); return AURA_TASK_FAILED; }} if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED; AuraTaskResult child_result = aura_task_frame_result(data->await_task_{i});\n"
        ));
        if branch_owns_task[i] {
            let _ = writeln!(out, "    if (__aura_task_executor != NULL) {{ (void)aura_task_executor_release(__aura_task_executor, &data->await_task_{i}); }}");
        }
        let _ = writeln!(
            out,
            "    data->{} = child_result.data == NULL ? 0 : *((int64_t *)child_result.data); data->await_task_{i} = NULL; goto {next};\n  }}\n",
            names[i + 2]
        );
    }
    let _ = writeln!(out, "{post_label}:\n  {{");
    for name in &names {
        let _ = writeln!(out, "    {name} = data->{name};");
    }
    let _ = writeln!(
        out,
        "    aura_gc_collect_executor(__aura_task_executor); data->{total} = {total_rhs}; data->{index} = {index_rhs}; aura_task_frame_set_resume_state(frame, 0); goto {head_label};\n  }}\n",
        total = names[1],
        index = names[0]
    );
    let _ = writeln!(
        out,
        "{done_label}:\n  {{ int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = data->{}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE; }}\n}}\n\n",
        names[1]
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data}); if (frame == NULL) return NULL;"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    for i in 0..branch_count {
        let _ = writeln!(out, "  data->await_task_{i} = NULL;");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; } return frame;\n}");
    true
}

/// Lower one bounded loop iteration with two independent conditional awaits.
/// Each selected child owns a distinct resume state; a false condition jumps
/// directly to the next gate without allocating or waiting on a task.
fn emit_async_fun_while_two_conditional_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 6
    {
        return false;
    }
    let Stmt::Var(index_var) = &f.body.stmts[0] else {
        return false;
    };
    let Stmt::Var(total_var) = &f.body.stmts[1] else {
        return false;
    };
    let Stmt::Var(first_var) = &f.body.stmts[2] else {
        return false;
    };
    let Stmt::Var(second_var) = &f.body.stmts[3] else {
        return false;
    };
    let Stmt::While(loop_stmt) = &f.body.stmts[4] else {
        return false;
    };
    let Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(return_id)),
        ..
    }) = &f.body.stmts[5]
    else {
        return false;
    };
    let vars = [index_var, total_var, first_var, second_var];
    if return_id.name != total_var.name.name
        || vars.iter().any(|var| {
            !var.mutable
                || var
                    .ty
                    .as_ref()
                    .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
                    .unwrap_or(true)
                || matches!(&var.init, Expr::Async(_))
        })
        || !matches!(&loop_stmt.cond, Expr::Binary(_))
        || loop_stmt.body.stmts.len() != 5
    {
        return false;
    }
    let Stmt::If(first_branch) = &loop_stmt.body.stmts[0] else {
        return false;
    };
    let Stmt::If(second_branch) = &loop_stmt.body.stmts[1] else {
        return false;
    };
    if first_branch.else_block.is_some()
        || second_branch.else_block.is_some()
        || first_branch.then_block.stmts.len() != 1
        || second_branch.then_block.stmts.len() != 1
        || matches!(&first_branch.cond, Expr::Async(_))
        || matches!(&second_branch.cond, Expr::Async(_))
    {
        return false;
    }
    let Some(first_await) =
        branch_assign_await(&first_branch.then_block.stmts[0], &first_var.name.name)
    else {
        return false;
    };
    let Some(second_await) =
        branch_assign_await(&second_branch.then_block.stmts[0], &second_var.name.name)
    else {
        return false;
    };
    let Stmt::Expr(Expr::Assign(total_assign)) = &loop_stmt.body.stmts[2] else {
        return false;
    };
    let Stmt::Expr(Expr::Call(gc_call)) = &loop_stmt.body.stmts[3] else {
        return false;
    };
    let Stmt::Expr(Expr::Assign(index_assign)) = &loop_stmt.body.stmts[4] else {
        return false;
    };
    if total_assign.name.name != total_var.name.name
        || index_assign.name.name != index_var.name.name
        || !matches!(gc_call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
        || !gc_call.args.is_empty()
        || matches!(total_assign.value.as_ref(), Expr::Async(_))
        || matches!(index_assign.value.as_ref(), Expr::Async(_))
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let index_name = mangle_ident(&index_var.name.name);
    let total_name = mangle_ident(&total_var.name.name);
    let first_name = mangle_ident(&first_var.name.name);
    let second_name = mangle_ident(&second_var.name.name);
    let head_label = format!("aura_async_{base}_two_cond_head");
    let first_poll_label = format!("aura_async_{base}_two_cond_first_poll");
    let second_gate_label = format!("aura_async_{base}_two_cond_second_gate");
    let second_poll_label = format!("aura_async_{base}_two_cond_second_poll");
    let post_label = format!("aura_async_{base}_two_cond_post");
    let done_label = format!("aura_async_{base}_two_cond_done");

    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for var in vars {
        entry_ctx.define_local(&var.name.name, "Int".into());
    }
    let index_init = coerce_expr(&index_var.init, "Int", &mut entry_ctx);
    let total_init = coerce_expr(&total_var.init, "Int", &mut entry_ctx);
    let first_init = coerce_expr(&first_var.init, "Int", &mut entry_ctx);
    let second_init = coerce_expr(&second_var.init, "Int", &mut entry_ctx);
    let loop_cond = emit_expr(&loop_stmt.cond, &mut entry_ctx);
    let first_cond = emit_expr(&first_branch.cond, &mut entry_ctx);
    let second_cond = emit_expr(&second_branch.cond, &mut entry_ctx);
    let first_task = emit_expr(&first_await.operand, &mut entry_ctx);
    let second_task = emit_expr(&second_await.operand, &mut entry_ctx);

    let mut post_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for var in vars {
        post_ctx.define_local(&var.name.name, "Int".into());
    }
    let total_rhs = coerce_expr(&total_assign.value, "Int", &mut post_ctx);
    let index_rhs = coerce_expr(&index_assign.value, "Int", &mut post_ctx);

    let _ = writeln!(
        out,
        "/* aura async loop two-conditional suspension states=3 spans={}:{}|{}:{} */",
        first_await.span.start,
        first_await.span.end,
        second_await.span.start,
        second_await.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    for name in [
        index_name.as_str(),
        total_name.as_str(),
        first_name.as_str(),
        second_name.as_str(),
    ] {
        let _ = writeln!(out, "  int64_t {name};");
    }
    out.push_str("  AuraTaskFrame *await_task_0; AuraTaskFrame *await_task_1;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    for name in [
        index_name.as_str(),
        total_name.as_str(),
        first_name.as_str(),
        second_name.as_str(),
    ] {
        let _ = writeln!(out, "  int64_t {name} = data->{name};");
    }
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("    case 0:\n");
    let _ = writeln!(
        out,
        "      data->{index_name} = {index_init}; data->{total_name} = {total_init}; data->{first_name} = {first_init}; data->{second_name} = {second_init}; data->await_task_0 = NULL; data->await_task_1 = NULL; aura_task_frame_set_resume_state(frame, 0); goto {head_label};"
    );
    let _ = writeln!(out, "    case 1: goto {first_poll_label};");
    let _ = writeln!(out, "    case 2: goto {second_poll_label};");
    out.push_str("    default: return AURA_TASK_FAILED;\n  }\n\n");

    let _ = writeln!(out, "{head_label}:\n  for (;;) {{");
    let _ = writeln!(out, "    {index_name} = data->{index_name}; {total_name} = data->{total_name}; {first_name} = data->{first_name}; {second_name} = data->{second_name};");
    let _ = writeln!(out, "    if (!({loop_cond})) goto {done_label};");
    let _ = writeln!(out, "    if ({first_cond}) {{ data->await_task_0 = {first_task}; if (data->await_task_0 == NULL) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, 1); goto {first_poll_label}; }}");
    let _ = writeln!(out, "    goto {second_gate_label};\n  }}\n");

    let _ = writeln!(out, "{first_poll_label}:\n  {{");
    out.push_str("    AuraTaskPollState child_state = aura_task_frame_state(data->await_task_0); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_0); if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task_0)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; } if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task_0); return AURA_TASK_FAILED; } if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED; AuraTaskResult child_result = aura_task_frame_result(data->await_task_0);");
    let _ = writeln!(out, "    data->{first_name} = child_result.data == NULL ? 0 : *((int64_t *)child_result.data); data->await_task_0 = NULL; goto {second_gate_label};\n  }}\n");

    let _ = writeln!(out, "{second_gate_label}:\n  {second_name} = data->{second_name}; {first_name} = data->{first_name}; {index_name} = data->{index_name}; {total_name} = data->{total_name};");
    let _ = writeln!(out, "  if ({second_cond}) {{ data->await_task_1 = {second_task}; if (data->await_task_1 == NULL) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, 2); goto {second_poll_label}; }}");
    let _ = writeln!(out, "  goto {post_label};\n");

    let _ = writeln!(out, "{second_poll_label}:\n  {{");
    out.push_str("    AuraTaskPollState child_state = aura_task_frame_state(data->await_task_1); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_1); if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task_1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; } if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task_1); return AURA_TASK_FAILED; } if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED; AuraTaskResult child_result = aura_task_frame_result(data->await_task_1);");
    let _ = writeln!(out, "    data->{second_name} = child_result.data == NULL ? 0 : *((int64_t *)child_result.data); data->await_task_1 = NULL; goto {post_label};\n  }}\n");

    let _ = writeln!(out, "{post_label}:\n  {index_name} = data->{index_name}; {total_name} = data->{total_name}; {first_name} = data->{first_name}; {second_name} = data->{second_name};");
    let _ = writeln!(out, "  aura_gc_collect_executor(__aura_task_executor); data->{total_name} = {total_rhs}; data->{index_name} = {index_rhs}; aura_task_frame_set_resume_state(frame, 0); goto {head_label};\n");
    let _ = writeln!(out, "{done_label}:\n  {{ int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = data->{total_name}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE; }}\n}}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, NULL); if (frame == NULL) return NULL;");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  data->await_task_0 = NULL; data->await_task_1 = NULL; if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; } return frame;\n}");
    true
}

/// Lower a loop body with multiple sequential `await Task<Int>` statements.
/// Loop state and child handles live in the frame; the poll loop advances only
/// after a child reaches a terminal state, so a pending poll never re-runs the
/// loop condition or allocates a duplicate child.
fn emit_async_fun_while_multi_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 4
    {
        return false;
    }
    let Stmt::Var(first_local) = &f.body.stmts[0] else {
        return false;
    };
    let Stmt::Var(second_local) = &f.body.stmts[1] else {
        return false;
    };
    let Stmt::While(loop_stmt) = &f.body.stmts[2] else {
        return false;
    };
    let Stmt::Return(return_stmt) = &f.body.stmts[3] else {
        return false;
    };
    let Some(return_value) = &return_stmt.value else {
        return false;
    };
    let Some(Expr::Async(AsyncExpr::Await(first_await))) =
        loop_stmt.body.stmts.first().and_then(|stmt| match stmt {
            Stmt::Var(v) => Some(&v.init),
            _ => None,
        })
    else {
        return false;
    };
    let Some(Expr::Async(AsyncExpr::Await(second_await))) =
        loop_stmt.body.stmts.get(1).and_then(|stmt| match stmt {
            Stmt::Var(v) => Some(&v.init),
            _ => None,
        })
    else {
        return false;
    };
    if loop_stmt.body.stmts.len() < 4
        || !matches!(loop_stmt.body.stmts[0], Stmt::Var(_))
        || !matches!(loop_stmt.body.stmts[1], Stmt::Var(_))
        || loop_stmt.body.stmts.iter().take(2).any(|stmt| match stmt {
            Stmt::Var(v) => {
                v.ty.as_ref()
                    .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
                    .unwrap_or(true)
            }
            _ => true,
        })
        || matches!(first_await.operand.as_ref(), Expr::Async(_))
        || matches!(second_await.operand.as_ref(), Expr::Async(_))
    {
        return false;
    }
    let loop_awaits: Vec<(&VarStmt, &AwaitExpr)> = loop_stmt
        .body
        .stmts
        .iter()
        .take_while(|stmt| matches!(stmt, Stmt::Var(v) if matches!(v.init, Expr::Async(AsyncExpr::Await(_)))) )
        .filter_map(|stmt| match stmt {
            Stmt::Var(v) => match &v.init {
                Expr::Async(AsyncExpr::Await(await_expr)) => Some((v, await_expr)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if loop_awaits.len() < 2 || loop_stmt.body.stmts.len() <= loop_awaits.len() {
        return false;
    }
    if loop_stmt.body.stmts[loop_awaits.len()..]
        .iter()
        .any(|stmt| {
            !matches!(
                stmt,
                Stmt::Expr(Expr::Assign(_)) | Stmt::Expr(Expr::Call(_))
            )
        })
    {
        return false;
    }
    let top_locals = [first_local, second_local];
    if top_locals.iter().any(|v| {
        v.ty.as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
            .unwrap_or(true)
            || matches!(v.init, Expr::Async(_))
    }) {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for local in top_locals {
        entry_ctx.define_local(&local.name.name, "Int".into());
    }
    for (local, _) in &loop_awaits {
        entry_ctx.define_local(&local.name.name, "Int".into());
    }
    let condition = emit_expr(&loop_stmt.cond, &mut entry_ctx);
    let initial_values: Vec<String> = top_locals
        .iter()
        .map(|local| coerce_expr(&local.init, "Int", &mut entry_ctx))
        .collect();
    let first_task = emit_expr(&loop_awaits[0].1.operand, &mut entry_ctx);
    let await_owns_task: Vec<bool> = loop_awaits
        .iter()
        .map(|(_, await_expr)| await_operand_is_temporary(&await_expr.operand, checked))
        .collect();
    let return_expr = emit_expr(return_value, &mut entry_ctx);

    let _ = writeln!(
        out,
        "/* aura async loop multi-await suspension states={} */",
        loop_awaits.len()
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    for local in top_locals {
        let _ = writeln!(out, "  int64_t {};", mangle_ident(&local.name.name));
    }
    for (local, _) in &loop_awaits {
        let _ = writeln!(out, "  int64_t {};", mangle_ident(&local.name.name));
    }
    for index in 0..loop_awaits.len() {
        let _ = writeln!(out, "  AuraTaskFrame *await_task_{index};");
    }
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for (index, owns_task) in await_owns_task.iter().enumerate() {
        if *owns_task {
            let _ = writeln!(out, "  if (data->await_task_{index} != NULL && __aura_task_executor != NULL) {{ (void)aura_task_executor_release(__aura_task_executor, &data->await_task_{index}); }}");
        }
    }
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  for (;;) {\n    switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("      case 0: {\n");
    for (local, init) in top_locals.iter().zip(&initial_values) {
        let _ = writeln!(
            out,
            "        data->{} = {init};",
            mangle_ident(&local.name.name)
        );
    }
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "        {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    for local in top_locals {
        let n = mangle_ident(&local.name.name);
        let _ = writeln!(out, "        int64_t {n} = data->{n};");
    }
    let _ = writeln!(out, "        if (!({condition})) {{");
    let _ = writeln!(out, "          int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = {return_expr}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE;");
    out.push_str("        }\n");
    let _ = writeln!(out, "        data->await_task_0 = {first_task};");
    out.push_str("        if (data->await_task_0 == NULL) return AURA_TASK_FAILED;\n        aura_task_frame_set_resume_state(frame, 1);\n        continue;\n      }\n");

    for index in 0..loop_awaits.len() {
        let state = index + 1;
        let _ = writeln!(out, "      case {state}: {{");
        let _ = writeln!(out, "        AuraTaskPollState child_state = aura_task_frame_state(data->await_task_{index});");
        let _ = writeln!(out, "        if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_{index});");
        let _ = writeln!(out, "        if (child_state == AURA_TASK_PENDING) {{ if (!aura_task_frame_wait_on(frame, data->await_task_{index})) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}");
        let _ = writeln!(
            out,
            "        if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;"
        );
        let _ = writeln!(out, "        if (child_state == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task_{index}); return AURA_TASK_FAILED; }}");
        out.push_str("        if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
        let value_name = mangle_ident(&loop_awaits[index].0.name.name);
        let _ = writeln!(out, "        AuraTaskResult child_result = aura_task_frame_result(data->await_task_{index}); if (child_result.data != NULL) data->{value_name} = *((int64_t *)child_result.data);");
        if await_owns_task[index] {
            let _ = writeln!(out, "        if (__aura_task_executor != NULL) {{ (void)aura_task_executor_release(__aura_task_executor, &data->await_task_{index}); }}");
        }
        let _ = writeln!(out, "        data->await_task_{index} = NULL;");
        if index + 1 < loop_awaits.len() {
            for p in &f.params {
                let n = mangle_ident(&p.name.name);
                let _ = writeln!(
                    out,
                    "        {} {n} = data->{n};",
                    c_type_ref_subst(&p.ty, checked, &params, &[])
                );
            }
            for local in top_locals {
                let n = mangle_ident(&local.name.name);
                let _ = writeln!(out, "        int64_t {n} = data->{n};");
            }
            for (prior, _) in loop_awaits.iter().take(index + 1) {
                let n = mangle_ident(&prior.name.name);
                let _ = writeln!(out, "        int64_t {n} = data->{n};");
            }
            let mut next_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
            for local in top_locals {
                next_ctx.define_local(&local.name.name, "Int".into());
            }
            for (prior, _) in loop_awaits.iter().take(index + 1) {
                next_ctx.define_local(&prior.name.name, "Int".into());
            }
            let next_task = emit_expr(&loop_awaits[index + 1].1.operand, &mut next_ctx);
            let _ = writeln!(out, "        data->await_task_{} = {next_task};", index + 1);
            let _ = writeln!(out, "        if (data->await_task_{} == NULL) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, {}); continue;", index + 1, state + 1);
        } else {
            let mut body_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
            for p in &f.params {
                let n = mangle_ident(&p.name.name);
                let _ = writeln!(
                    out,
                    "        {} {n} = data->{n};",
                    c_type_ref_subst(&p.ty, checked, &params, &[])
                );
                let key = type_ref_local_key_expand(&p.ty, &params, &[], checked);
                body_ctx.define_local(&p.name.name, full_type_mono(&key, checked));
            }
            for local in top_locals {
                let n = mangle_ident(&local.name.name);
                let _ = writeln!(out, "        int64_t {n} = data->{n};");
                body_ctx.define_local(&local.name.name, "Int".into());
            }
            for (local, _) in &loop_awaits {
                let n = mangle_ident(&local.name.name);
                let _ = writeln!(out, "        int64_t {n} = data->{n};");
                body_ctx.define_local(&local.name.name, "Int".into());
            }
            for stmt in &loop_stmt.body.stmts[loop_awaits.len()..] {
                crate::stmt::emit_stmt(out, stmt, 4, &mut body_ctx);
            }
            for local in top_locals {
                let n = mangle_ident(&local.name.name);
                let _ = writeln!(out, "        data->{n} = {n};");
            }
            let _ = writeln!(out, "        if (!({condition})) {{ int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = {return_expr}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE; }}");
            let mut next_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
            for local in top_locals {
                next_ctx.define_local(&local.name.name, "Int".into());
            }
            for (local, _) in &loop_awaits {
                next_ctx.define_local(&local.name.name, "Int".into());
            }
            let next_task = emit_expr(&loop_awaits[0].1.operand, &mut next_ctx);
            let _ = writeln!(out, "        data->await_task_0 = {next_task}; if (data->await_task_0 == NULL) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, 1); continue;");
        }
        out.push_str("      }\n");
    }
    out.push_str("      default: return AURA_TASK_FAILED;\n    }\n  }\n}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data}); if (frame == NULL) return NULL;");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Minimal top-level `while`/`await` lowering for Int state. The loop locals
/// live in the task frame and a pending child registers the parent as a
/// dependency. Child completion then wakes the parent through the executor;
/// the poll callback never drives a nested scheduler turn.
fn emit_async_fun_top_level_while_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    let Some(ret_ty) = &f.return_type else {
        return false;
    };
    if type_ref_local_key_expand(ret_ty, &[], &[], checked) != "Int" || f.body.stmts.len() < 4 {
        return false;
    }
    let Stmt::Var(index) = &f.body.stmts[0] else {
        return false;
    };
    let Stmt::Var(total) = &f.body.stmts[1] else {
        return false;
    };
    let Stmt::While(loop_stmt) = &f.body.stmts[2] else {
        return false;
    };
    let Some(Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(ret)),
        ..
    })) = f.body.stmts.last()
    else {
        return false;
    };
    if ret.name != total.name.name
        || index
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
            != Some("Int".into())
        || total
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
            != Some("Int".into())
        || loop_stmt.body.stmts.len() != 3
    {
        return false;
    }
    let Stmt::Var(await_var) = &loop_stmt.body.stmts[0] else {
        return false;
    };
    let Some(Expr::Async(AsyncExpr::Await(await_expr))) = Some(&await_var.init) else {
        return false;
    };
    if await_var
        .ty
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
        != Some("Int".into())
    {
        return false;
    }
    let Stmt::Expr(Expr::Assign(total_assign)) = &loop_stmt.body.stmts[1] else {
        return false;
    };
    let Stmt::Expr(Expr::Assign(index_assign)) = &loop_stmt.body.stmts[2] else {
        return false;
    };
    if total_assign.name.name != total.name.name || index_assign.name.name != index.name.name {
        return false;
    }
    let tail = &f.body.stmts[3..f.body.stmts.len() - 1];
    if tail.iter().any(|s| !matches!(s, Stmt::If(i) if i.else_block.is_none() && i.then_block.stmts.len() == 1 && matches!(i.then_block.stmts[0], Stmt::Expr(Expr::Call(_))))) {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let index_name = mangle_ident(&index.name.name);
    let total_name = mangle_ident(&total.name.name);
    let await_name = mangle_ident(&await_var.name.name);
    let mut ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let index_init = coerce_expr(&index.init, "Int", &mut ctx);
    ctx.define_local(&index.name.name, "Int".into());
    let total_init = coerce_expr(&total.init, "Int", &mut ctx);
    ctx.define_local(&total.name.name, "Int".into());
    let cond = emit_expr(&loop_stmt.cond, &mut ctx);
    let operand = emit_expr(&await_expr.operand, &mut ctx);
    ctx.define_local(&await_var.name.name, "Int".into());
    let total_rhs = coerce_expr(&total_assign.value, "Int", &mut ctx);
    let index_rhs = coerce_expr(&index_assign.value, "Int", &mut ctx);
    let mut tail_code = String::new();
    for stmt in tail {
        let Stmt::If(if_stmt) = stmt else {
            unreachable!()
        };
        let if_cond = emit_expr(&if_stmt.cond, &mut ctx);
        let Stmt::Expr(Expr::Call(call)) = &if_stmt.then_block.stmts[0] else {
            unreachable!()
        };
        let call_code = emit_expr(&Expr::Call(call.clone()), &mut ctx);
        tail_code.push_str(&format!("      if ({if_cond}) {{ {call_code}; }}\n"));
    }

    let _ = writeln!(out, "/* aura async top-level while-await Int lowering */");
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "  int64_t {index_name};\n  int64_t {total_name};\n  AuraTaskFrame *await_task;\n}} {data_ty};\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  int64_t {n} = data->{n};");
    }
    let _ = writeln!(out, "  int64_t {index_name} = data->{index_name};\n  int64_t {total_name} = data->{total_name};");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    let _ = writeln!(out, "      {index_name} = {index_init}; {total_name} = {total_init}; data->{index_name} = {index_name}; data->{total_name} = {total_name};");
    out.push_str("      aura_task_frame_set_resume_state(frame, 1);\n      /* fall through */\n    }\n    case 1: {\n      for (;;) {\n");
    let _ = writeln!(out, "        if (!({cond})) break;\n        if (data->await_task == NULL) data->await_task = {operand};\n        if (data->await_task == NULL) return AURA_TASK_FAILED;\n        AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n        if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n        if (child_state == AURA_TASK_PENDING) {{\n          data->{index_name} = {index_name}; data->{total_name} = {total_name};\n          if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED;\n          return AURA_TASK_PENDING;\n        }}\n        if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n        if (child_state == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }}\n        if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n        int64_t {await_name} = 0; AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n        if (child_result.data != NULL) {await_name} = *((int64_t *)child_result.data);\n        {total_name} = {total_rhs}; {index_name} = {index_rhs};\n        data->{index_name} = {index_name}; data->{total_name} = {total_name}; data->await_task = NULL;\n      }}\n{tail_code}      int64_t *result = (int64_t *)malloc(sizeof(*result));\n      if (result == NULL) return AURA_TASK_FAILED;\n      *result = {total_name};\n      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});\n      return AURA_TASK_COMPLETE;\n    }}\n    default: return AURA_TASK_FAILED;\n  }}\n}}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, NULL);"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Lower a range `for` whose body awaits one `Task<Int>` and accumulates the
/// result. The range cursor, bound, accumulator, and child frame all survive
/// a pending poll; inclusive ranges use the same checked endpoint semantics as
/// the synchronous statement emitter.
fn emit_async_fun_for_range_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    let Some(ret_ty) = &f.return_type else {
        return false;
    };
    if type_ref_local_key_expand(ret_ty, &[], &[], checked) != "Int" || f.body.stmts.len() != 3 {
        return false;
    }
    let Some(Stmt::Var(total)) = f.body.stmts.first() else {
        return false;
    };
    let Some(Stmt::ForRange(range)) = f.body.stmts.get(1) else {
        return false;
    };
    let Some(Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(ret)),
        ..
    })) = f.body.stmts.last()
    else {
        return false;
    };
    if ret.name != total.name.name
        || total
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
            != Some("Int".into())
        || range.body.stmts.len() < 2
    {
        return false;
    }
    let Stmt::Var(await_var) = &range.body.stmts[0] else {
        return false;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        return false;
    };
    if await_var
        .ty
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
        != Some("Int".into())
    {
        return false;
    }
    let Stmt::Expr(Expr::Assign(total_assign)) = &range.body.stmts[1] else {
        return false;
    };
    if total_assign.name.name != total.name.name || !range.body.stmts[2..].iter().all(|stmt| {
        matches!(
            stmt,
            Stmt::Expr(Expr::Call(call))
                if call.args.is_empty()
                    && matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
        )
    }) {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let total_name = mangle_ident(&total.name.name);
    let destroy_data = format!("aura_async_destroy_{base}");
    let cursor_name = mangle_ident(&range.name.name);
    let await_name = mangle_ident(&await_var.name.name);
    let mut ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let total_init = coerce_expr(&total.init, "Int", &mut ctx);
    ctx.define_local(&total.name.name, "Int".into());
    let start = coerce_expr(&range.start, "Int", &mut ctx);
    let end = coerce_expr(&range.end, "Int", &mut ctx);
    ctx.define_local(&range.name.name, "Int".into());
    let operand = emit_expr(&await_expr.operand, &mut ctx);
    let owns_task = await_operand_is_temporary(&await_expr.operand, checked);
    ctx.define_local(&await_var.name.name, "Int".into());
    let total_rhs = coerce_expr(&total_assign.value, "Int", &mut ctx);
    let mut gc_code = String::new();
    for stmt in &range.body.stmts[2..] {
        let Stmt::Expr(Expr::Call(call)) = stmt else {
            unreachable!()
        };
        let code = emit_expr(&Expr::Call(call.clone()), &mut ctx);
        gc_code.push_str(&format!(" {code};"));
    }
    let bound = if range.inclusive {
        format!("{cursor_name} <= data->{data_ty}_end")
    } else {
        format!("{cursor_name} < data->{data_ty}_end")
    };
    let release_code = if owns_task {
        "if (__aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task);"
    } else {
        ""
    };
    let _ = writeln!(out, "/* aura async for-range-await Int lowering */");
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "  int64_t {cursor_name}; int64_t {data_ty}_end; int64_t {total_name}; AuraTaskFrame *await_task;\n}} {data_ty};\n");
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL && data->await_task != NULL && __aura_task_executor != NULL) {{ {release_code} }} }}\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "  int64_t {cursor_name} = data->{cursor_name}; int64_t {total_name} = data->{total_name}; int64_t {await_name} = 0;");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    let _ = writeln!(
        out,
        "      {cursor_name} = {start}; data->{cursor_name} = {cursor_name}; data->{data_ty}_end = {end}; {total_name} = {total_init}; data->{total_name} = {total_name}; aura_task_frame_set_resume_state(frame, 1);"
    );
    out.push_str("    }\n    case 1: {\n      for (;;) {\n");
    let _ = writeln!(out, "        if (!({bound})) break;\n        if (data->await_task == NULL) data->await_task = {operand};\n        if (data->await_task == NULL) return AURA_TASK_FAILED;\n        AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n        if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n        if (child_state == AURA_TASK_PENDING) {{ data->{cursor_name} = {cursor_name}; data->{total_name} = {total_name}; if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}\n        if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n        if (child_state == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }}\n        if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n        AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n        if (child_result.data != NULL) {await_name} = *((int64_t *)child_result.data);\n        {total_name} = {total_rhs};{gc_code}\n        {cursor_name}++; data->{cursor_name} = {cursor_name}; data->{total_name} = {total_name}; {release_code} data->await_task = NULL;\n      }}\n      int64_t *result = (int64_t *)malloc(sizeof(*result));\n      if (result == NULL) return AURA_TASK_FAILED;\n      *result = {total_name};\n      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});\n      return AURA_TASK_COMPLETE;\n    }}\n    default: return AURA_TASK_FAILED;\n  }}\n}}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Lower a loop with pre-await guard branches that can `break` or `continue`.
/// The loop head is a resumable CFG label: state 1 enters it synchronously,
/// while state 2 resumes after the child task completes. This keeps branch
/// decisions and loop locals stable across a pending poll.
fn emit_async_fun_while_guarded_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 3
    {
        return false;
    }
    let Stmt::Var(index) = &f.body.stmts[0] else {
        return false;
    };
    if !index.mutable
        || index
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
            .unwrap_or(true)
    {
        return false;
    }
    let Stmt::While(loop_stmt) = &f.body.stmts[1] else {
        return false;
    };
    let Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(ret)),
        ..
    }) = &f.body.stmts[2]
    else {
        return false;
    };
    if ret.name != index.name.name || matches!(&loop_stmt.cond, Expr::Async(_)) {
        return false;
    }
    let Some(await_pos) = loop_stmt.body.stmts.iter().position(|stmt| {
        matches!(
            stmt,
            Stmt::Var(v) if matches!(&v.init, Expr::Async(AsyncExpr::Await(_)))
        )
    }) else {
        return false;
    };
    let Stmt::Var(await_var) = &loop_stmt.body.stmts[await_pos] else {
        unreachable!()
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        unreachable!()
    };
    if await_var
        .ty
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || matches!(await_expr.operand.as_ref(), Expr::Async(_))
    {
        return false;
    }
    let post = &loop_stmt.body.stmts[await_pos + 1..];
    let Some(post_assign) = post.iter().find_map(|stmt| match stmt {
        Stmt::Expr(Expr::Assign(assign)) if assign.name.name == index.name.name => Some(assign),
        _ => None,
    }) else {
        return false;
    };
    if matches!(post_assign.value.as_ref(), Expr::Async(_)) || post.iter().any(|stmt| {
        !matches!(
            stmt,
            Stmt::Expr(Expr::Assign(assign)) if assign.name.name == index.name.name
                && !matches!(assign.value.as_ref(), Expr::Async(_))
        ) && !matches!(
            stmt,
            Stmt::Expr(Expr::Call(call))
                if call.args.is_empty()
                    && matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
        )
    }) {
        return false;
    }

    let mut entry_ctx = async_ctx(checked, detector, &[], &f.params, &f.return_type);
    let index_init = coerce_expr(&index.init, "Int", &mut entry_ctx);
    entry_ctx.define_local(&index.name.name, "Int".into());
    let loop_cond = emit_expr(&loop_stmt.cond, &mut entry_ctx);
    let operand = emit_expr(&await_expr.operand, &mut entry_ctx);

    let mut control_code = String::new();
    for stmt in &loop_stmt.body.stmts[..await_pos] {
        let Stmt::If(branch) = stmt else {
            return false;
        };
        if branch.else_block.is_some() || branch.then_block.stmts.is_empty() {
            return false;
        }
        let Some(last) = branch.then_block.stmts.last() else {
            return false;
        };
        let (is_break, is_continue) = match last {
            Stmt::Break(_) => (true, false),
            Stmt::Continue(_) => (false, true),
            _ => return false,
        };
        if branch.then_block.stmts[..branch.then_block.stmts.len() - 1]
            .iter()
            .any(|stmt| !matches!(stmt, Stmt::Expr(Expr::Assign(_))))
        {
            return false;
        }
        let condition = emit_expr(&branch.cond, &mut entry_ctx);
        let mut action = String::new();
        for stmt in &branch.then_block.stmts[..branch.then_block.stmts.len() - 1] {
            let Stmt::Expr(expr) = stmt else {
                unreachable!()
            };
            action.push_str(&emit_expr(expr, &mut entry_ctx));
            action.push_str("; ");
        }
        action.push_str("data->");
        action.push_str(&mangle_ident(&index.name.name));
        action.push_str(" = ");
        action.push_str(&mangle_ident(&index.name.name));
        action.push_str("; ");
        if is_break {
            action.push_str("break;");
        } else if is_continue {
            action.push_str("continue;");
        }
        control_code.push_str(&format!("        if ({condition}) {{ {action} }}\n"));
    }

    let mut post_ctx = async_ctx(checked, detector, &[], &f.params, &f.return_type);
    post_ctx.define_local(&index.name.name, "Int".into());
    post_ctx.define_local(&await_var.name.name, "Int".into());
    let post_code = post
        .iter()
        .map(|stmt| {
            let Stmt::Expr(expr) = stmt else {
                unreachable!()
            };
            format!("{};", emit_expr(expr, &mut post_ctx))
        })
        .collect::<Vec<_>>()
        .join(" ");
    let index_name = mangle_ident(&index.name.name);
    let await_name = mangle_ident(&await_var.name.name);
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");

    let _ = writeln!(
        out,
        "/* aura async loop CFG suspension states=1 await={} break_continue */",
        await_expr.span.start
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "  int64_t {index_name};");
    let _ = writeln!(out, "  int64_t {await_name};");
    out.push_str("  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "  int64_t {index_name} = data->{index_name};");
    let _ = writeln!(out, "  int64_t {await_name} = data->{await_name};");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("    case 0: {\n");
    let _ = writeln!(
        out,
        "      {index_name} = {index_init}; data->{index_name} = {index_name}; aura_task_frame_set_resume_state(frame, 1);"
    );
    out.push_str("      goto aura_async_loop_cfg_head;\n    }\n");
    out.push_str("    case 1: goto aura_async_loop_cfg_head;\n");
    out.push_str("    case 2: {\n");
    out.push_str(
        "      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    let _ = writeln!(
        out,
        "      AuraTaskResult child_result = aura_task_frame_result(data->await_task); data->{await_name} = child_result.data == NULL ? 0 : *((int64_t *)child_result.data); {await_name} = data->{await_name}; data->await_task = NULL; {post_code}; data->{index_name} = {index_name}; aura_task_frame_set_resume_state(frame, 1); goto aura_async_loop_cfg_head;\n    }}"
    );
    out.push_str("    default: return AURA_TASK_FAILED;\n  }\n\n");
    out.push_str("aura_async_loop_cfg_head:\n  for (;;) {\n");
    let _ = writeln!(out, "    if (!({loop_cond})) break;");
    out.push_str(&control_code);
    let _ = writeln!(
        out,
        "    if (data->await_task == NULL) data->await_task = {operand};"
    );
    out.push_str("    if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
    out.push_str("    AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n");
    out.push_str("    if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("    if (child_state == AURA_TASK_PENDING) { data->");
    out.push_str(&index_name);
    out.push_str(" = ");
    out.push_str(&index_name);
    out.push_str("; aura_task_frame_set_resume_state(frame, 2); if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("    if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("    if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("    if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    let _ = writeln!(
        out,
        "    AuraTaskResult child_result = aura_task_frame_result(data->await_task); data->{await_name} = child_result.data == NULL ? 0 : *((int64_t *)child_result.data); {await_name} = data->{await_name}; data->await_task = NULL; {post_code}; data->{index_name} = {index_name};"
    );
    out.push_str("  }\n");
    let _ = writeln!(
        out,
        "  int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = {index_name}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE;\n}}\n\n"
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, NULL);"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  data->await_task = NULL;\n");
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// C22l: bounded loop shape with a conditional suspension. The loop body is
/// intentionally narrow: `if (cond) { val x: Int = await task }` followed by
/// an index assignment. The condition is re-evaluated after every resume, but
/// the child handle and loop index live in the frame, so a pending poll cannot
/// restart the iteration or create a duplicate child.
fn emit_async_fun_top_level_while_conditional_await_int(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| {
            !matches!(
                type_ref_local_key_expand(t, &[], &[], checked).as_str(),
                "Int" | "Bool" | "String"
            )
        })
        .unwrap_or(true)
        || f.body.stmts.len() != 3
    {
        return false;
    }
    let Stmt::Var(index) = &f.body.stmts[0] else {
        return false;
    };
    if !index.mutable
        || index
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
            .unwrap_or(true)
    {
        return false;
    }
    let Stmt::While(loop_stmt) = &f.body.stmts[1] else {
        return false;
    };
    if loop_stmt.body.stmts.len() != 2 || matches!(&loop_stmt.cond, Expr::Async(_)) {
        return false;
    }
    let Stmt::If(branch) = &loop_stmt.body.stmts[0] else {
        return false;
    };
    if branch.else_block.is_some()
        || matches!(&branch.cond, Expr::Async(_))
        || branch.then_block.stmts.len() != 1
    {
        return false;
    }
    let Stmt::Var(await_var) = &branch.then_block.stmts[0] else {
        return false;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        return false;
    };
    if await_var
        .ty
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || matches!(await_expr.operand.as_ref(), Expr::Async(_))
    {
        return false;
    }
    let Stmt::Expr(Expr::Assign(index_assign)) = &loop_stmt.body.stmts[1] else {
        return false;
    };
    if index_assign.name.name != index.name.name
        || matches!(index_assign.value.as_ref(), Expr::Async(_))
    {
        return false;
    }
    let Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(ret)),
        ..
    }) = &f.body.stmts[2]
    else {
        return false;
    };
    if ret.name != index.name.name {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret_ty = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let index_name = mangle_ident(&index.name.name);

    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let index_init = coerce_expr(&index.init, "Int", &mut entry_ctx);
    entry_ctx.define_local(&index.name.name, "Int".into());
    let loop_cond = emit_expr(&loop_stmt.cond, &mut entry_ctx);
    let branch_cond = emit_expr(&branch.cond, &mut entry_ctx);
    let operand = emit_expr(&await_expr.operand, &mut entry_ctx);
    let index_rhs = coerce_expr(&index_assign.value, "Int", &mut entry_ctx);

    let _ = writeln!(
        out,
        "/* aura async conditional loop suspension state=1 kind=await span={}:{} */",
        await_expr.span.start, await_expr.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "  int64_t {index_name};");
    out.push_str("  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "  int64_t {index_name} = data->{index_name};");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    let _ = writeln!(
        out,
        "      {index_name} = {index_init}; data->{index_name} = {index_name};"
    );
    out.push_str("      aura_task_frame_set_resume_state(frame, 1);\n      /* fall through */\n    }\n    case 1: {\n      for (;;) {\n");
    let _ = writeln!(out, "        if (!({loop_cond})) break;");
    let _ = writeln!(out, "        if ({branch_cond}) {{");
    let _ = writeln!(
        out,
        "          if (data->await_task == NULL) data->await_task = {operand};"
    );
    out.push_str("          if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
    out.push_str(
        "          AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("          if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("          if (child_state == AURA_TASK_PENDING) { data->");
    out.push_str(&index_name);
    out.push_str(" = ");
    out.push_str(&index_name);
    out.push_str("; if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("          if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("          if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("          if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str("          data->await_task = NULL;\n        }\n");
    let _ = writeln!(
        out,
        "        {index_name} = {index_rhs}; data->{index_name} = {index_name};"
    );
    out.push_str("      }\n");
    let _ = writeln!(
        out,
        "      {ret_ty} *result = ({ret_ty} *)malloc(sizeof(*result));"
    );
    out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(out, "      *result = {index_name};");
    let _ = writeln!(
        out,
        "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});"
    );
    out.push_str("      return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, NULL);"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Rewrite a top-level return-position await into a synthetic typed local.
/// This is intentionally limited to one direct return expression; branches and
/// nested control-flow still require the general state-machine lowering.
/// Lower a bounded branch join where each arm awaits one `Task<Int>` and
/// returns the awaited value.  Both arms share one frame slot; the entry state
/// records the selected arm and state one performs the common terminal/error
/// handling.  This is the smallest general join shape that avoids duplicating
/// the child-wait protocol while remaining explicit about the frame state.
/// Lower a nested branch tree with one await in each leaf:
///
/// ```text
/// if (outer) { if (inner) { await a } else { await b } }
/// else       { if (inner) { await c } else { await d } }
/// ```
///
/// This is deliberately a closed shape.  The selected leaf is recorded in
/// the frame before its child is created, and all four leaves share the same
/// terminal poll state.  Consequently a pending poll never re-evaluates a
/// condition or allocates an unselected child, while cancellation/failure
/// follows the same frame protocol as the other async lowerings.
fn emit_async_fun_nested_if_branch_awaits(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 1
    {
        return false;
    }
    let Stmt::If(outer) = &f.body.stmts[0] else {
        return false;
    };
    let Some(outer_else) = &outer.else_block else {
        return false;
    };
    if outer.then_block.stmts.len() != 1 || outer_else.stmts.len() != 1 {
        return false;
    }
    let Stmt::If(then_tree) = &outer.then_block.stmts[0] else {
        return false;
    };
    let Stmt::If(else_tree) = &outer_else.stmts[0] else {
        return false;
    };
    let Some(then_else) = &then_tree.else_block else {
        return false;
    };
    let Some(else_else) = &else_tree.else_block else {
        return false;
    };
    let Some((_, then_true, _)) = branch_await_return(&then_tree.then_block, checked) else {
        return false;
    };
    let Some((_, then_false, _)) = branch_await_return(then_else, checked) else {
        return false;
    };
    let Some((_, else_true, _)) = branch_await_return(&else_tree.then_block, checked) else {
        return false;
    };
    let Some((_, else_false, _)) = branch_await_return(else_else, checked) else {
        return false;
    };

    // The task expression is created in state 0.  Do not admit another
    // await-shaped expression here: that would require a second suspension
    // state inside a branch and would violate this lowering's invariant.
    let leaves = [then_true, then_false, else_true, else_false];
    if leaves
        .iter()
        .any(|await_expr| matches!(await_expr.operand.as_ref(), Expr::Async(_)))
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, &[]);

    let mut outer_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let outer_condition = emit_expr(&outer.cond, &mut outer_ctx);
    let mut then_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let then_condition = emit_expr(&then_tree.cond, &mut then_ctx);
    let mut else_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let else_condition = emit_expr(&else_tree.cond, &mut else_ctx);
    let task_exprs: Vec<String> = leaves
        .iter()
        .map(|await_expr| {
            let mut ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
            emit_expr(&await_expr.operand, &mut ctx)
        })
        .collect();

    let _ = writeln!(
        out,
        "/* aura async nested-branch suspension states=1 leaves=4 spans={}:{}|{}:{}|{}:{}|{}:{} */",
        then_true.span.start,
        then_true.span.end,
        then_false.span.start,
        then_false.span.end,
        else_true.span.start,
        else_true.span.end,
        else_false.span.start,
        else_false.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    out.push_str("  uint8_t selected_path;\n  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(
        out,
        "static void {destroy_data}(AuraTaskFrame *frame) {{ (void)frame; }}\n"
    );
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
        if p.name.name == "this" {
            let cty = c_type_ref_subst(&p.ty, checked, &params, &[]);
            let _ = writeln!(out, "      {cty} this = a_this;");
        }
    }
    let _ = writeln!(out, "      if ({outer_condition}) {{");
    let _ = writeln!(
        out,
        "        if ({then_condition}) {{ data->selected_path = 0; data->await_task = {}; }} else {{ data->selected_path = 1; data->await_task = {}; }}",
        task_exprs[0],
        task_exprs[1]
    );
    let _ = writeln!(out, "      }} else {{");
    let _ = writeln!(
        out,
        "        if ({else_condition}) {{ data->selected_path = 2; data->await_task = {}; }} else {{ data->selected_path = 3; data->await_task = {}; }}",
        task_exprs[2],
        task_exprs[3]
    );
    out.push_str("      }\n      if (data->await_task == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n    }\n    case 1: {\n");
    out.push_str("      if (data->selected_path > 3) return AURA_TASK_FAILED;\n");
    out.push_str(
        "      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n      AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n");
    let _ = writeln!(
        out,
        "      {ret} *result = ({ret} *)malloc(sizeof(*result));"
    );
    out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(
        out,
        "      *result = child_result.data == NULL ? ({ret}){{0}} : *(({ret} *)child_result.data);"
    );
    let _ = writeln!(
        out,
        "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE;\n    }}\n    default: return AURA_TASK_FAILED;\n  }}\n}}\n\n"
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Lower a branch join whose two arms assign awaited `Task<Int>` values to the
/// same local, followed by a common straight-line continuation. The branch
/// choice and child handle are persisted before suspension; the continuation
/// is emitted only after the selected child reaches a terminal state.
fn branch_assign_await<'a>(stmt: &'a Stmt, result_name: &str) -> Option<&'a AwaitExpr> {
    let Stmt::Expr(Expr::Assign(assign)) = stmt else {
        return None;
    };
    if assign.name.name != result_name {
        return None;
    }
    let Expr::Async(AsyncExpr::Await(await_expr)) = assign.value.as_ref() else {
        return None;
    };
    (!matches!(await_expr.operand.as_ref(), Expr::Async(_))).then_some(await_expr)
}

fn emit_async_fun_if_else_assign_await_continue(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    let Some(return_type) = f.return_type.as_ref() else {
        return false;
    };
    let result_key = type_ref_local_key_expand(return_type, &[], &[], checked);
    if !matches!(result_key.as_str(), "Int" | "String") || f.body.stmts.len() < 3 {
        return false;
    }
    let Stmt::Var(result_var) = &f.body.stmts[0] else {
        return false;
    };
    if !result_var.mutable
        || result_var
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != result_key)
            .unwrap_or(true)
        || matches!(&result_var.init, Expr::Async(_))
    {
        return false;
    }
    let Stmt::If(branch) = &f.body.stmts[1] else {
        return false;
    };
    let Some(else_block) = &branch.else_block else {
        return false;
    };
    if branch.then_block.stmts.len() != 1
        || else_block.stmts.len() != 1
        || matches!(&branch.cond, Expr::Async(_))
    {
        return false;
    }
    let then_await = branch_assign_await(&branch.then_block.stmts[0], &result_var.name.name);
    let else_await = branch_assign_await(&else_block.stmts[0], &result_var.name.name);
    let (Some(then_await), Some(else_await)) = (then_await, else_await) else {
        return false;
    };
    let Some(Stmt::Return(return_stmt)) = f.body.stmts.last() else {
        return false;
    };
    let Some(return_value) = &return_stmt.value else {
        return false;
    };
    if f.body.stmts[2..f.body.stmts.len() - 1]
        .iter()
        .any(|stmt| !matches!(stmt, Stmt::Expr(_)))
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let result_name = mangle_ident(&result_var.name.name);
    let result_cty = crate::stmt::local_key_to_c(&result_key, checked);

    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let result_init = coerce_expr(&result_var.init, &result_key, &mut entry_ctx);
    entry_ctx.define_local(&result_var.name.name, result_key.clone());
    let condition = emit_expr(&branch.cond, &mut entry_ctx);
    let then_task = emit_expr(&then_await.operand, &mut entry_ctx);
    let else_task = emit_expr(&else_await.operand, &mut entry_ctx);

    let mut continuation_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    continuation_ctx.define_local(&result_var.name.name, result_key.clone());
    let mut continuation = String::new();
    for stmt in &f.body.stmts[2..f.body.stmts.len() - 1] {
        crate::stmt::emit_stmt(&mut continuation, stmt, 2, &mut continuation_ctx);
    }
    let return_expr = emit_expr(return_value, &mut continuation_ctx);

    let _ = writeln!(
        out,
        "/* aura async branch-join continuation states=1 spans={}:{}|{}:{} */",
        then_await.span.start, then_await.span.end, else_await.span.start, else_await.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "  {result_cty} {result_name};");
    if result_key == "String" {
        let _ = writeln!(out, "  bool {result_name}__owned;");
    }
    out.push_str("  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    if result_key == "String" {
        let _ = writeln!(
            out,
            "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data->{result_name}__owned) free((void *)data->{result_name});"
        );
    } else {
        out.push_str("  (void)frame;\n");
    }
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{"
    );
    if result_key == "String" {
        out.push_str(
            "  (void)size; if (data != NULL) free((void *)*((const char **)data)); free(data);\n",
        );
    } else {
        out.push_str("  (void)size; free(data);\n");
    }
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "      data->{result_name} = {result_init};");
    if result_key == "String" {
        let _ = writeln!(out, "      data->{result_name}__owned = false;");
    }
    let _ = writeln!(
        out,
        "      data->await_task = ({condition}) ? {then_task} : {else_task};"
    );
    out.push_str("      if (data->await_task == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n    }\n    case 1: {\n");
    out.push_str(
        "      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str("      AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n");
    if result_key == "String" {
        out.push_str("      if (data->");
        out.push_str(&result_name);
        out.push_str("__owned) free((void *)data->");
        out.push_str(&result_name);
        out.push_str("); data->");
        out.push_str(&result_name);
        out.push_str(" = NULL; data->");
        out.push_str(&result_name);
        out.push_str("__owned = false; if (child_result.data != NULL) { const char *__value = *((const char **)child_result.data); if (__value != NULL) { size_t __len = strlen(__value); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) return AURA_TASK_FAILED; memcpy(__copy, __value, __len + 1); data->");
        out.push_str(&result_name);
        out.push_str(" = (const char *)__copy; data->");
        out.push_str(&result_name);
        out.push_str("__owned = true; } }\n");
    } else {
        let _ = writeln!(
            out,
            "      if (child_result.data != NULL) data->{result_name} = *((int64_t *)child_result.data);"
        );
    }
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(
        out,
        "      {result_cty} {result_name} = data->{result_name};"
    );
    out.push_str(&continuation);
    let _ = writeln!(
        out,
        "      {ret} *result = ({ret} *)malloc(sizeof(*result));"
    );
    out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
    if result_key == "String" {
        out.push_str("      const char *__returned = ");
        out.push_str(&return_expr);
        out.push_str("; if (__returned == NULL) { *result = NULL; } else { size_t __len = strlen(__returned); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) { free(result); return AURA_TASK_FAILED; } memcpy(__copy, __returned, __len + 1); *result = (const char *)__copy; }\n");
    } else {
        let _ = writeln!(out, "      *result = {return_expr};");
    }
    let _ = writeln!(
        out,
        "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE;\n    }}\n    default: return AURA_TASK_FAILED;\n  }}\n}}\n\n"
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  data->await_task = NULL;\n  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn emit_async_fun_if_else_single_await(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    let Some(return_type) = f.return_type.as_ref() else {
        return false;
    };
    let return_key = type_ref_local_key_expand(return_type, &[], &[], checked);
    if !(matches!(return_key.as_str(), "Int" | "Bool" | "String") || is_array_type_key(&return_key))
        || f.body.stmts.len() != 1
    {
        return false;
    }
    let Stmt::If(branch) = &f.body.stmts[0] else {
        return false;
    };
    let Some(else_block) = &branch.else_block else {
        return false;
    };
    let Some((_then_var, then_await, _then_return)) =
        branch_await_return(&branch.then_block, checked)
    else {
        return false;
    };
    let Some((_else_var, else_await, _else_return)) = branch_await_return(else_block, checked)
    else {
        return false;
    };

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);

    let _ = writeln!(
        out,
        "/* aura async branch-join suspension state=1 kind=await spans={}:{}|{}:{} */",
        then_await.span.start, then_await.span.end, else_await.span.start, else_await.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    out.push_str("  bool selected_then;\n  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let then_owns_task = await_operand_is_temporary(&then_await.operand, checked);
    let else_owns_task = await_operand_is_temporary(&else_await.operand, checked);
    let owned_condition = match (then_owns_task, else_owns_task) {
        (true, true) => "true".to_string(),
        (true, false) => "data->selected_then".to_string(),
        (false, true) => "!data->selected_then".to_string(),
        (false, false) => "false".to_string(),
    };
    let _ = writeln!(
        out,
        "static void {destroy_data}(AuraTaskFrame *frame) {{ if (frame != NULL && __aura_task_executor != NULL) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if ({owned_condition} && data->await_task != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); }} }}\n"
    );
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{"
    );
    if ret == "const char *" {
        out.push_str("  (void)size;\n  if (data != NULL) free((void *)*((const char **)data));\n  free(data);\n}\n\n");
    } else if is_array_type_key(&return_key) {
        let ret_cty = crate::stmt::local_key_to_c(&return_key, checked);
        let _ = writeln!(
            out,
            "  (void)size; if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data;"
        );
        crate::array_emit::emit_array_contents_free(out, 2, "(*result)", &return_key);
        out.push_str("  free(result); }\n}\n\n");
    } else {
        out.push_str("  (void)size;\n  free(data);\n}\n\n");
    }
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let condition = emit_expr(&branch.cond, &mut entry_ctx);
    let then_task = emit_expr(&then_await.operand, &mut entry_ctx);
    let else_task = emit_expr(&else_await.operand, &mut entry_ctx);
    let _ = writeln!(out, "      data->selected_then = ({condition});");
    let _ = writeln!(
        out,
        "      data->await_task = data->selected_then ? {then_task} : {else_task};"
    );
    out.push_str("      if (data->await_task == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n    }\n    case 1: {\n");
    out.push_str(
        "      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str("      AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n");
    let _ = writeln!(
        out,
        "      {ret} *result = ({ret} *)malloc(sizeof(*result));"
    );
    out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
    if ret == "const char *" {
        out.push_str("      const char *__value = child_result.data == NULL ? NULL : *((const char **)child_result.data); if (__value == NULL) { *result = NULL; } else { size_t __len = strlen(__value); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) { free(result); return AURA_TASK_FAILED; } memcpy(__copy, __value, __len + 1); *result = __copy; }\n");
    } else if is_array_type_key(&return_key) {
        let clone = crate::names::c_method_name(&return_key, "clone");
        let _ = writeln!(
            out,
            "      {ret} __value = child_result.data == NULL ? ({ret}){{0}} : *(({ret} *)child_result.data); *result = {clone}(&__value);"
        );
    } else {
        let _ = writeln!(
            out,
            "      *result = child_result.data == NULL ? ({ret}){{0}} : *(({ret} *)child_result.data);"
        );
    }
    if then_owns_task || else_owns_task {
        let owned_condition = match (then_owns_task, else_owns_task) {
            (true, true) => "true".to_string(),
            (true, false) => "data->selected_then".to_string(),
            (false, true) => "!data->selected_then".to_string(),
            (false, false) => "false".to_string(),
        };
        let _ = writeln!(
            out,
            "      if (__aura_task_executor != NULL && {owned_condition}) {{ (void)aura_task_executor_release(__aura_task_executor, &data->await_task); }}"
        );
    }
    out.push_str("      data->await_task = NULL;\n");
    let _ = writeln!(
        out,
        "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});"
    );
    out.push_str("      return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn branch_await_return<'a>(
    block: &'a aura_ast::Block,
    checked: &CheckedFile,
) -> Option<(&'a VarStmt, &'a AwaitExpr, &'a Ident)> {
    if block.stmts.len() != 2 {
        return None;
    }
    let Stmt::Var(var) = &block.stmts[0] else {
        return None;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &var.init else {
        return None;
    };
    if var
        .ty
        .as_ref()
        .map(|t| {
            let key = type_ref_local_key_expand(t, &[], &[], checked);
            !(matches!(key.as_str(), "Int" | "Bool" | "String") || is_array_type_key(&key))
        })
        .unwrap_or(true)
    {
        return None;
    }
    let Stmt::Return(ret) = &block.stmts[1] else {
        return None;
    };
    let Some(Expr::Ident(id)) = &ret.value else {
        return None;
    };
    (id.name == var.name.name).then_some((var, await_expr, id))
}

/// Lower an `if` whose true arm assigns an awaited `Task<Int>` into a local
/// which is consumed after the branch.  The local and child handle live in
/// the frame, so the continuation observes the assignment after resumption;
/// the false path completes without creating a child task.
fn emit_async_fun_if_assign_await(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 3
    {
        return false;
    }
    let Stmt::Var(result) = &f.body.stmts[0] else {
        return false;
    };
    if !result.mutable
        || result
            .ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
            .unwrap_or(true)
        || matches!(&result.init, Expr::Async(_))
    {
        return false;
    }
    let Stmt::If(branch) = &f.body.stmts[1] else {
        return false;
    };
    if branch.else_block.is_some()
        || matches!(&branch.cond, Expr::Async(_))
        || branch.then_block.stmts.len() != 1
    {
        return false;
    }
    let Stmt::Expr(Expr::Assign(assign)) = &branch.then_block.stmts[0] else {
        return false;
    };
    if assign.name.name != result.name.name {
        return false;
    }
    let Expr::Async(AsyncExpr::Await(await_expr)) = assign.value.as_ref() else {
        return false;
    };
    if matches!(await_expr.operand.as_ref(), Expr::Async(_)) {
        return false;
    }
    let Stmt::Return(ret_stmt) = &f.body.stmts[2] else {
        return false;
    };
    if !matches!(&ret_stmt.value, Some(Expr::Ident(name)) if name.name == result.name.name) {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let mut shape_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    shape_ctx.define_local(&result.name.name, "Int".into());
    if async_inner_key(&await_expr.operand, &shape_ctx) != "Int" {
        return false;
    }

    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let result_name = mangle_ident(&result.name.name);

    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let result_init = coerce_expr(&result.init, "Int", &mut entry_ctx);
    entry_ctx.define_local(&result.name.name, "Int".into());
    let condition = emit_expr(&branch.cond, &mut entry_ctx);
    let operand = emit_expr(&await_expr.operand, &mut entry_ctx);
    let owns_task = await_operand_is_temporary(&await_expr.operand, checked);

    let _ = writeln!(
        out,
        "/* aura async if-assign suspension state=1 kind=await span={}:{} */",
        await_expr.span.start, await_expr.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "  int64_t {result_name};");
    out.push_str("  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    if owns_task {
        let _ = writeln!(
            out,
            "static void {destroy_data}(AuraTaskFrame *frame) {{ if (frame != NULL && __aura_task_executor != NULL) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data->await_task != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); }} }}\n"
        );
    } else {
        let _ = writeln!(
            out,
            "static void {destroy_data}(AuraTaskFrame *frame) {{ (void)frame; }}\n"
        );
    }
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "  int64_t {result_name} = data->{result_name};");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    let _ = writeln!(
        out,
        "      {result_name} = {result_init}; data->{result_name} = {result_name};"
    );
    let _ = writeln!(out, "      if ({condition}) {{");
    let _ = writeln!(out, "        data->await_task = {operand};");
    out.push_str("        if (data->await_task == NULL) return AURA_TASK_FAILED;\n        aura_task_frame_set_resume_state(frame, 1);\n      } else {\n");
    let _ = writeln!(
        out,
        "        {ret} *__aura_result = ({ret} *)malloc(sizeof(*__aura_result));"
    );
    out.push_str("        if (__aura_result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(out, "        *__aura_result = {result_name};");
    let _ = writeln!(
        out,
        "        aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result});"
    );
    out.push_str("        return AURA_TASK_COMPLETE;\n      }\n    }\n    case 1: {\n");
    out.push_str(
        "      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str("      AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n");
    out.push_str("      if (child_result.data != NULL) data->");
    out.push_str(&result_name);
    out.push_str(" = *((int64_t *)child_result.data);\n");
    if owns_task {
        out.push_str("      if (__aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task);\n");
    }
    out.push_str("      data->await_task = NULL;\n");
    let _ = writeln!(
        out,
        "      {ret} *__aura_result = ({ret} *)malloc(sizeof(*__aura_result));"
    );
    out.push_str("      if (__aura_result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(out, "      *__aura_result = data->{result_name};");
    let _ = writeln!(
        out,
        "      aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {destroy_result});"
    );
    out.push_str("      return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    let _ = writeln!(out, "  data->{result_name} = 0;");
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Lower `if (cond) { val x = await task; call(x) } return literal`.
/// The call is a post-suspension continuation, so the awaited value and child
/// frame stay live in the task frame until the resumed branch has executed.
fn emit_async_fun_if_await_then_continue(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 2
    {
        return false;
    }
    let Stmt::If(branch) = &f.body.stmts[0] else {
        return false;
    };
    if branch.else_block.is_some() || branch.then_block.stmts.len() != 2 {
        return false;
    }
    let Stmt::Var(await_var) = &branch.then_block.stmts[0] else {
        return false;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        return false;
    };
    if await_var
        .ty
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || matches!(await_expr.operand.as_ref(), Expr::Async(_))
    {
        return false;
    }
    let Stmt::Expr(Expr::Call(_)) = &branch.then_block.stmts[1] else {
        return false;
    };
    let Stmt::Return(ret) = &f.body.stmts[1] else {
        return false;
    };
    let Some(fallback) = &ret.value else {
        return false;
    };
    if !matches!(fallback, Expr::Int(_))
        || async_inner_key(
            &await_expr.operand,
            &async_ctx(checked, detector, &[], &f.params, &f.return_type),
        ) != "Int"
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret_ty = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let value_name = mangle_ident(&await_var.name.name);
    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let condition = emit_expr(&branch.cond, &mut entry_ctx);
    let task = emit_expr(&await_expr.operand, &mut entry_ctx);
    let owns_task = await_operand_is_temporary(&await_expr.operand, checked);
    entry_ctx.define_local(&await_var.name.name, "Int".into());
    let Stmt::Expr(Expr::Call(call)) = &branch.then_block.stmts[1] else {
        unreachable!()
    };
    let continuation = emit_expr(&Expr::Call(call.clone()), &mut entry_ctx);
    let fallback = emit_expr(fallback, &mut entry_ctx);

    let _ = writeln!(
        out,
        "/* aura async if-await continuation state=1 kind=await span={}:{} */",
        await_expr.span.start, await_expr.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    out.push_str("  int64_t awaited_value;\n  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    if owns_task {
        let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{ if (frame != NULL && __aura_task_executor != NULL) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data->await_task != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); }} }}\n");
    } else {
        let _ = writeln!(
            out,
            "static void {destroy_data}(AuraTaskFrame *frame) {{ (void)frame; }}\n"
        );
    }
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("    case 0: {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "      if ({condition}) {{ data->await_task = {task}; if (data->await_task == NULL) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, 1); }} else {{");
    let _ = writeln!(out, "        {ret_ty} *result = ({ret_ty} *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = {fallback}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE; }}\n    }}\n    case 1: {{");
    out.push_str("      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n      AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n      data->awaited_value = child_result.data == NULL ? 0 : *((int64_t *)child_result.data);\n");
    if owns_task {
        out.push_str("      if (__aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task);\n");
    }
    out.push_str("      data->await_task = NULL;\n");
    let _ = writeln!(
        out,
        "      int64_t {value_name} = data->awaited_value; (void){continuation};"
    );
    let _ = writeln!(out, "      {ret_ty} *result = ({ret_ty} *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = {fallback}; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE;\n    }}\n    default: return AURA_TASK_FAILED;\n  }}\n}}\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data}); if (frame == NULL) return NULL;");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Lower an `if`-guarded await followed by a second await.  The selected
/// branch and both child handles live in the frame, so a pending poll resumes
/// at the child state without re-evaluating the branch condition.
fn emit_async_fun_if_then_multi_await(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || !matches!(f.body.stmts.len(), 3 | 4)
    {
        return false;
    }
    let Stmt::If(branch) = &f.body.stmts[0] else {
        return false;
    };
    if branch.else_block.is_some() || branch.then_block.stmts.len() != 1 {
        return false;
    }
    let Stmt::Var(first_var) = &branch.then_block.stmts[0] else {
        return false;
    };
    let Expr::Async(AsyncExpr::Await(first_await)) = &first_var.init else {
        return false;
    };
    let (gc_stmt, second_index, return_index) = if f.body.stmts.len() == 4 {
        let Stmt::Expr(Expr::Call(call)) = &f.body.stmts[1] else {
            return false;
        };
        if !matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
            || !call.args.is_empty()
        {
            return false;
        }
        (Some(&f.body.stmts[1]), 2, 3)
    } else {
        (None, 1, 2)
    };
    let Stmt::Var(second_var) = &f.body.stmts[second_index] else {
        return false;
    };
    let Expr::Async(AsyncExpr::Await(second_await)) = &second_var.init else {
        return false;
    };
    let Stmt::Return(ret_stmt) = &f.body.stmts[return_index] else {
        return false;
    };
    let Some(ret_value) = &ret_stmt.value else {
        return false;
    };
    let int_key = |var: &VarStmt| {
        var.ty
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
            .unwrap_or_else(|| "Int".into())
    };
    if int_key(first_var) != "Int"
        || int_key(second_var) != "Int"
        || matches!(first_await.operand.as_ref(), Expr::Async(_))
        || matches!(second_await.operand.as_ref(), Expr::Async(_))
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let shape_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    if async_inner_key(&first_await.operand, &shape_ctx) != "Int"
        || async_inner_key(&second_await.operand, &shape_ctx) != "Int"
    {
        return false;
    }

    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret_ty = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let first_name = mangle_ident(&first_var.name.name);
    let second_name = mangle_ident(&second_var.name.name);

    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let condition = emit_expr(&branch.cond, &mut entry_ctx);
    let first_task = emit_expr(&first_await.operand, &mut entry_ctx);
    let second_task_without_first = emit_expr(&second_await.operand, &mut entry_ctx);
    let mut after_first_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    after_first_ctx.define_local(&first_var.name.name, "int64_t".into());
    let second_task_after_first = emit_expr(&second_await.operand, &mut after_first_ctx);
    let mut result_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    result_ctx.define_local(&first_var.name.name, "int64_t".into());
    result_ctx.define_local(&second_var.name.name, "int64_t".into());
    let result_expr = emit_expr(ret_value, &mut result_ctx);

    let _ = writeln!(
        out,
        "/* aura async branch-then-multi suspension states=2 spans={}:{}|{}:{} */",
        first_await.span.start,
        first_await.span.end,
        second_await.span.start,
        second_await.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "  int64_t {first_name};");
    let _ = writeln!(out, "  int64_t {second_name};");
    out.push_str("  AuraTaskFrame *await_task_0;\n  AuraTaskFrame *await_task_1;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "      if ({condition}) {{");
    let _ = writeln!(out, "        data->await_task_0 = {first_task};");
    out.push_str("        if (data->await_task_0 == NULL) return AURA_TASK_FAILED;\n        aura_task_frame_set_resume_state(frame, 1);\n      } else {\n");
    let _ = writeln!(
        out,
        "        data->await_task_1 = {second_task_without_first};"
    );
    out.push_str("        if (data->await_task_1 == NULL) return AURA_TASK_FAILED;\n        aura_task_frame_set_resume_state(frame, 2);\n        goto aura_state_2;\n      }\n    }\n    case 1: {\n");
    out.push_str("      AuraTaskPollState child_state_0 = aura_task_frame_state(data->await_task_0);\n      if (child_state_0 == AURA_TASK_READY) child_state_0 = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_0);\n      if (child_state_0 == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task_0)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      if (child_state_0 == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n      if (child_state_0 == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task_0); return AURA_TASK_FAILED; }\n      if (child_state_0 != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n      AuraTaskResult child_result_0 = aura_task_frame_result(data->await_task_0);\n      data->" );
    out.push_str(&first_name);
    out.push_str(" = child_result_0.data == NULL ? 0 : *((int64_t *)child_result_0.data);\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let _ = writeln!(out, "      int64_t {first_name} = data->{first_name};");
    if let Some(gc_stmt) = gc_stmt {
        let mut gc_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
        gc_ctx.define_local(&first_var.name.name, "int64_t".into());
        crate::stmt::emit_stmt(out, gc_stmt, 3, &mut gc_ctx);
    }
    let _ = writeln!(out, "      data->await_task_1 = {second_task_after_first};");
    out.push_str("      if (data->await_task_1 == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 2);\n    }\n    aura_state_2:\n    case 2: {\n");
    out.push_str("      AuraTaskPollState child_state_1 = aura_task_frame_state(data->await_task_1);\n      if (child_state_1 == AURA_TASK_READY) child_state_1 = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_1);\n      if (child_state_1 == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task_1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      if (child_state_1 == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n      if (child_state_1 == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task_1); return AURA_TASK_FAILED; }\n      if (child_state_1 != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n      AuraTaskResult child_result_1 = aura_task_frame_result(data->await_task_1);\n      data->" );
    out.push_str(&second_name);
    out.push_str(" = child_result_1.data == NULL ? 0 : *((int64_t *)child_result_1.data);\n");
    let _ = writeln!(out, "      int64_t {first_name} = data->{first_name};");
    let _ = writeln!(out, "      int64_t {second_name} = data->{second_name};");
    let _ = writeln!(
        out,
        "      {ret_ty} *result = ({ret_ty} *)malloc(sizeof(*result));"
    );
    out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(out, "      *result = {result_expr};");
    let _ = writeln!(
        out,
        "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});"
    );
    out.push_str("      return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, NULL);"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Lower the first control-flow state-machine slice: an `if` whose true arm
/// awaits one `Task<Int>` local and returns it, followed by a literal `Int`
/// return on the false path. The branch decision is made in entry state and
/// the selected true path resumes through the same child-wait protocol as the
/// straight-line lowering. More general branch joins and loops remain G1 work.
fn emit_async_fun_if_single_await(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    if f.return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
        || f.body.stmts.len() != 2
    {
        return false;
    }
    let Stmt::If(branch) = &f.body.stmts[0] else {
        return false;
    };
    let Stmt::Return(false_return) = &f.body.stmts[1] else {
        return false;
    };
    let Some(false_value) = &false_return.value else {
        return false;
    };
    if !matches!(false_value, Expr::Int(_)) || branch.else_block.is_some() {
        return false;
    }
    if branch.then_block.stmts.len() != 2 {
        return false;
    }
    let Stmt::Var(await_var) = &branch.then_block.stmts[0] else {
        return false;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        return false;
    };
    if await_var
        .ty
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &[], &[], checked) != "Int")
        .unwrap_or(true)
    {
        return false;
    }
    let Stmt::Return(true_return) = &branch.then_block.stmts[1] else {
        return false;
    };
    if !matches!(
        true_return.value.as_ref(),
        Some(Expr::Ident(id)) if id.name == await_var.name.name
    ) {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);

    let _ = writeln!(
        out,
        "/* aura async control-flow suspension state=1 kind=await span={}:{} */",
        await_expr.span.start, await_expr.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    out.push_str("  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    out.push_str("  (void)frame;\n}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("    case 0: {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    let condition = emit_expr(&branch.cond, &mut entry_ctx);
    let _ = writeln!(out, "      if ({condition}) {{");
    let task = emit_expr(&await_expr.operand, &mut entry_ctx);
    let _ = writeln!(out, "        data->await_task = {task};");
    out.push_str("        if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
    out.push_str("        aura_task_frame_set_resume_state(frame, 1);\n");
    out.push_str("      } else {\n");
    let fallback = emit_expr(false_value, &mut entry_ctx);
    let _ = writeln!(
        out,
        "        {ret} *result = ({ret} *)malloc(sizeof(*result));"
    );
    out.push_str("        if (result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(out, "        *result = {fallback};");
    let _ = writeln!(
        out,
        "        aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});"
    );
    out.push_str("        return AURA_TASK_COMPLETE;\n      }\n    }\n    case 1: {\n");
    out.push_str(
        "      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str("      AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n");
    let _ = writeln!(
        out,
        "      {ret} *result = ({ret} *)malloc(sizeof(*result));"
    );
    out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
    out.push_str(
        "      *result = child_result.data == NULL ? 0 : *((int64_t *)child_result.data);\n",
    );
    let _ = writeln!(
        out,
        "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});"
    );
    out.push_str("      return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// F3: expose the stable allocation-only structured-value ABI to generated C.
/// Keep this self-contained because installed builds compile generated source
/// beside `runtime.c` without requiring a system include path.
fn emit_ffi_abi_declarations(out: &mut String) {
    out.push_str("#define AURA_FFI_ABI_VERSION 1u\n");
    out.push_str(
        "typedef enum { AURA_FFI_OK = 0, AURA_FFI_INVALID = 1, AURA_FFI_OOM = 2 } AuraFfiStatus;\n",
    );
    out.push_str("typedef void *(*AuraTypeErasedCloneFn)(const void *, size_t, size_t *);\n");
    out.push_str("typedef void (*AuraTypeErasedDropFn)(void *, size_t);\n");
    out.push_str("typedef void (*AuraTypeErasedMarkFn)(const void *, size_t);\n");
    out.push_str("typedef struct AuraTypeErasedOps { uint32_t abi_version; AuraTypeErasedCloneFn clone; AuraTypeErasedDropFn drop; AuraTypeErasedMarkFn mark; } AuraTypeErasedOps;\n");
    out.push_str("typedef struct AuraTypeErasedValue { void *data; size_t size; const AuraTypeErasedOps *ops; } AuraTypeErasedValue;\n");
    out.push_str("#define AURA_TYPE_ERASED_ABI_VERSION 1u\n");
    out.push_str("AuraFfiStatus aura_type_erased_clone(const AuraTypeErasedValue *, AuraTypeErasedValue *);\n");
    out.push_str("void aura_type_erased_drop(AuraTypeErasedValue *);\n");
    out.push_str("void aura_type_erased_mark(const AuraTypeErasedValue *);\n");
    out.push_str("typedef struct { const char *data; uint64_t len; } AuraFfiStringView;\n");
    out.push_str("typedef struct { char *data; uint64_t len; } AuraFfiString;\n");
    out.push_str("typedef enum { AURA_FFI_ARRAY_BYTES = 1, AURA_FFI_ARRAY_INT64 = 2, AURA_FFI_ARRAY_BOOL = 3 } AuraFfiArrayKind;\n");
    out.push_str("typedef struct { const void *data; uint64_t len; uint64_t cap; uint64_t elem_size; AuraFfiArrayKind kind; } AuraFfiArrayView;\n");
    out.push_str("typedef struct { void *data; uint64_t len; uint64_t cap; uint64_t elem_size; AuraFfiArrayKind kind; } AuraFfiArray;\n");
    out.push_str("typedef struct { void **slot; int active; } AuraFfiRootGuard;\n");
    out.push_str("typedef struct AuraFfiOpaqueHandle AuraFfiOpaqueHandle;\n");
    out.push_str("typedef struct AuraTaskFrame AuraTaskFrame;\n");
    out.push_str("AuraFfiStatus aura_task_frame_set_erased_result(AuraTaskFrame *, const AuraTypeErasedValue *);\n");
    out.push_str("AuraFfiStatus aura_task_frame_result_erased(const AuraTaskFrame *, AuraTypeErasedValue *);\n");
    out.push_str("typedef struct { AuraFfiOpaqueHandle *handle; void *resource; uint64_t generation; } AuraFfiHandlePin;\n");
    out.push_str("typedef enum { AURA_FFI_BOUNDARY_SYNC = 0, AURA_FFI_BOUNDARY_TASK = 1, AURA_FFI_BOUNDARY_AWAIT = 2, AURA_FFI_BOUNDARY_CHANNEL = 3, AURA_FFI_BOUNDARY_CALLBACK = 4 } AuraFfiBoundary;\n");
    out.push_str("AuraFfiStatus aura_ffi_handle_destroy(AuraFfiOpaqueHandle **);\n");
    out.push_str("AuraFfiStatus aura_ffi_handle_retain(AuraFfiOpaqueHandle *);\n");
    out.push_str("AuraFfiStatus aura_ffi_handle_drop(AuraFfiOpaqueHandle **);\n");
    out.push_str("void aura_destroy_foreign_handle_payload(void *);\n");
    out.push_str("AuraFfiStatus aura_ffi_handle_pin_for_boundary(AuraFfiOpaqueHandle *, AuraFfiBoundary, AuraFfiHandlePin *);\n");
    out.push_str("AuraFfiStatus aura_ffi_handle_unpin(AuraFfiHandlePin *);\n");
    out.push_str("AuraFfiStatus aura_task_frame_pin_foreign_handle(AuraTaskFrame *, AuraFfiOpaqueHandle *, AuraFfiBoundary);\n");
    out.push_str(
        "AuraFfiStatus aura_ffi_string_borrow(const char *, uint64_t, AuraFfiStringView *);\n",
    );
    out.push_str("AuraFfiStatus aura_ffi_string_copy(AuraFfiStringView, AuraFfiString *);\n");
    out.push_str("AuraFfiStatus aura_ffi_string_transfer(char *, uint64_t, AuraFfiString *);\n");
    out.push_str("void aura_ffi_string_destroy(AuraFfiString *);\n");
    out.push_str("AuraFfiStatus aura_ffi_array_borrow(const void *, uint64_t, uint64_t, uint64_t, AuraFfiArrayKind, AuraFfiArrayView *);\n");
    out.push_str("AuraFfiStatus aura_ffi_array_copy(AuraFfiArrayView, AuraFfiArray *);\n");
    out.push_str("AuraFfiStatus aura_ffi_array_transfer(void *, uint64_t, uint64_t, uint64_t, AuraFfiArrayKind, AuraFfiArray *);\n");
    out.push_str("void aura_ffi_array_destroy(AuraFfiArray *);\n");
    out.push_str("AuraFfiStatus aura_ffi_root_begin(AuraFfiRootGuard *, void **);\n");
    out.push_str("void aura_ffi_root_end(AuraFfiRootGuard *);\n\n");
    out.push_str("typedef enum { AURA_FFI_OUTCOME_OK = 0, AURA_FFI_OUTCOME_CANCELLED = 1, AURA_FFI_OUTCOME_INVALID = 2, AURA_FFI_OUTCOME_NOT_FOUND = 3, AURA_FFI_OUTCOME_PERMISSION = 4, AURA_FFI_OUTCOME_UNAVAILABLE = 5, AURA_FFI_OUTCOME_TIMEOUT = 6, AURA_FFI_OUTCOME_FOREIGN_ERROR = 7 } AuraFfiOutcome;\n");
    out.push_str("AuraFfiOutcome aura_ffi_map_error(int32_t);\n\n");
}

fn c_metadata_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Emit the versioned compiler metadata ABI. Source-retained attributes stay
/// in `CheckedFile` for tools, while Binary/Runtime entries cross into the
/// generated artifact through this stable, read-only table.
fn emit_metadata_abi(out: &mut String, checked: &CheckedFile) {
    out.push_str("#define AURA_METADATA_ABI_VERSION 1u\n");
    out.push_str("typedef struct { const char *declaration; const char *target; const char *name; const char *args; uint32_t retention; uint32_t span_start; uint32_t span_end; } AuraAttributeMetadata;\n");
    out.push_str("typedef struct { const char *phase; const char *macro_name; const char *generated_item; uint32_t invocation_start; uint32_t invocation_end; uint32_t generated_start; uint32_t generated_end; } AuraExpansionMetadata;\n");
    let retained = checked
        .attribute_metadata
        .iter()
        .filter(|metadata| metadata.retention.abi_code() != 0)
        .collect::<Vec<_>>();
    out.push_str("static const AuraAttributeMetadata aura_attribute_metadata[] = {\n");
    if retained.is_empty() {
        out.push_str("  { NULL, NULL, NULL, NULL, 0u, 0u, 0u },\n");
    } else {
        for metadata in retained {
            let args = metadata.args.join(",");
            let _ = writeln!(
                out,
                "  {{ \"{}\", \"{}\", \"{}\", \"{}\", {}u, {}u, {}u }},",
                c_metadata_string(&metadata.declaration),
                c_metadata_string(&metadata.target),
                c_metadata_string(&metadata.name),
                c_metadata_string(&args),
                metadata.retention.abi_code(),
                metadata.span.start,
                metadata.span.end
            );
        }
    }
    out.push_str("};\n");
    let _ = writeln!(
        out,
        "static const size_t aura_attribute_metadata_count = {}u;",
        checked
            .attribute_metadata
            .iter()
            .filter(|metadata| metadata.retention.abi_code() != 0)
            .count()
    );
    out.push_str("const AuraAttributeMetadata *aura_generated_attribute_metadata(size_t *count) { if (count != NULL) *count = aura_attribute_metadata_count; return aura_attribute_metadata_count == 0 ? NULL : aura_attribute_metadata; }\n");

    out.push_str("static const AuraExpansionMetadata aura_expansion_metadata[] = {\n");
    if checked.expansions.is_empty() {
        out.push_str("  { NULL, NULL, NULL, 0u, 0u, 0u, 0u },\n");
    } else {
        for metadata in &checked.expansions {
            let _ = writeln!(
                out,
                "  {{ \"{}\", \"{}\", \"{}\", {}u, {}u, {}u, {}u }},",
                c_metadata_string(&metadata.phase),
                c_metadata_string(&metadata.macro_name),
                c_metadata_string(&metadata.generated_item),
                metadata.invocation_span.start,
                metadata.invocation_span.end,
                metadata.generated_span.start,
                metadata.generated_span.end
            );
        }
    }
    out.push_str("};\n");
    let _ = writeln!(
        out,
        "static const size_t aura_expansion_metadata_count = {}u;",
        checked.expansions.len()
    );
    out.push_str("const AuraExpansionMetadata *aura_generated_expansion_metadata(size_t *count) { if (count != NULL) *count = aura_expansion_metadata_count; return aura_expansion_metadata_count == 0 ? NULL : aura_expansion_metadata; }\n\n");
}

/// F2: emit only the primitive C ABI surface declared by `@foreign`.
/// String is intentionally represented as a borrowed `const char *` handle.
fn emit_foreign_prototypes(out: &mut String, checked: &CheckedFile) {
    for foreign in &checked.ast.foreign_functions {
        let ret = crate::names::c_type_from_opt(&foreign.return_type, checked, &[], &[]);
        let params = foreign
            .params
            .iter()
            .map(|param| crate::names::c_type_ref_subst(&param.ty, checked, &[], &[]))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "extern {ret} {}({params});", foreign.name.name);
    }
    if !checked.ast.foreign_functions.is_empty() {
        out.push('\n');
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod abi_tests {
    use super::emit_c_with;
    use crate::ctx::EmitOptions;
    use aura_ast::{
        Attribute, AttributeArg, AttributeValue, Block, File, FunDecl, Ident, Path, Span,
    };
    use aura_parser::parse_file;

    #[test]
    fn generated_artifact_embeds_runtime_abi_identity_and_check() {
        let span = Span::new(0, 0);
        let file = File {
            package: Path {
                segments: vec![Ident {
                    name: "demo".into(),
                    span,
                }],
                span,
            },
            imports: vec![],
            interfaces: vec![],
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            consts: vec![],
            functions: vec![],
            foreign_functions: vec![],
            async_functions: vec![],
            span,
        };
        let checked = aura_sema::check_file(&file).expect("empty file checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("#define AURA_GENERATED_ABI_ID \"aura-c-abi/1.0;"));
        assert!(generated.contains("#define AURA_GENERATED_ABI_VERSION 1u"));
        assert!(generated
            .contains("aura_runtime_check_abi(AURA_GENERATED_ABI_VERSION, AURA_GENERATED_ABI_ID)"));
    }

    #[test]
    fn typed_foreign_handle_parameter_uses_checked_task_pin_abi() {
        let file = parse_file(
            "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_use(handle: ForeignHandle<Int>): Unit\n\
fun main(handle: ForeignHandle<Int>) { native_use(handle) }\n",
        )
        .expect("parse typed foreign handle fixture");
        let checked = aura_sema::check_file(&file).expect("typed parameter checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("native_use") && generated.contains("AuraFfiOpaqueHandle *"));
        assert!(generated.contains("aura_ffi_handle_pin_for_boundary"));
        assert!(generated.contains("AURA_FFI_BOUNDARY_TASK"));
        assert!(generated.contains("aura_ffi_handle_unpin"));
    }

    #[test]
    fn typed_foreign_handle_return_emits_opaque_prototype() {
        let file = parse_file(
            "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_open(): ForeignHandle<Int>\n",
        )
        .expect("parse typed foreign handle return fixture");
        let checked = aura_sema::check_file(&file).expect("owned foreign return is supported");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("native_open") && generated.contains("AuraFfiOpaqueHandle *"));
    }

    #[test]
    fn nested_foreign_handle_return_keeps_opaque_pointer_and_task_drop_contract() {
        let file = parse_file(
            "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_open(): ForeignHandle<ForeignHandle<Int>>\n\
async fun produce(): ForeignHandle<ForeignHandle<Int>> { return native_open() }\n\
fun main() {\n\
  val task = spawn { return native_open() }\n\
  val first = join(task)\n\
  val second = join(task)\n\
  gc_collect()\n\
}\n",
        )
        .expect("parse nested foreign handle result fixture");
        let checked = aura_sema::check_file(&file).expect("nested handle fixture checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("native_open"));
        assert!(generated.contains("AuraFfiOpaqueHandle *"));
        assert!(generated.contains("aura_async_result_destroy_demo_produce"));
        assert!(generated.contains("aura_ffi_handle_drop(result)"));
        assert!(generated.contains("aura_ffi_handle_retain(__join_handle"));
        assert!(generated.contains("aura_ffi_handle_drop(&first"));
        assert!(generated.contains("aura_ffi_handle_drop(&second"));
    }

    #[test]
    fn async_foreign_handle_result_retains_each_owned_join() {
        let file = parse_file(
            "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_open(): ForeignHandle<Int>\n\
async fun produce(): ForeignHandle<Int> { return native_open() }\n\
fun main() {\n\
  val task = spawn { return native_open() }\n\
  val first = join(task)\n\
  val second = join(task)\n\
  gc_collect()\n\
}\n",
        )
        .expect("parse async foreign handle result fixture");
        let checked = aura_sema::check_file(&file).expect("async foreign handle result checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("aura_async_result_destroy_demo_produce"));
        assert!(generated.contains("aura_ffi_handle_drop(result)"));
        assert!(generated.contains("aura_ffi_handle_retain(__join_handle)"));
        assert!(generated.contains("aura_ffi_handle_drop(&first"));
        assert!(generated.contains("aura_ffi_handle_drop(&second"));
    }

    #[test]
    fn async_typed_foreign_handle_retains_pin_in_task_frame() {
        let file = parse_file(
            "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_use(handle: ForeignHandle<Int>): Unit\n\
async fun run(handle: ForeignHandle<Int>): Unit { native_use(handle) }\n",
        )
        .expect("parse async typed foreign handle fixture");
        let checked = aura_sema::check_file(&file).expect("async typed parameter checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("aura_async_body_demo_run(AuraTaskFrame *frame"));
        assert!(generated.contains("aura_task_frame_pin_foreign_handle(frame"));
    }
    #[test]
    fn checked_file_retains_attribute_metadata_for_consumers() {
        let span = Span::new(0, 1);
        let ident = |name: &str| Ident {
            name: name.into(),
            span,
        };
        let file = File {
            package: Path {
                segments: vec![ident("demo")],
                span,
            },
            imports: vec![],
            interfaces: vec![],
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            consts: vec![],
            functions: vec![FunDecl {
                is_pub: false,
                origin_package: "demo".into(),
                attributes: vec![Attribute {
                    name: ident("deprecated"),
                    args: vec![AttributeArg::Positional(AttributeValue::String {
                        value: "use newer".into(),
                        span,
                    })],
                    span,
                }],
                modifiers: vec![],
                visibility: aura_ast::MemberVisibility::Package,
                is_test: false,
                name: ident("main"),
                type_params: vec![],
                params: vec![],
                return_type: None,
                body: Block {
                    stmts: vec![],
                    span,
                },
                span,
            }],
            foreign_functions: vec![],
            async_functions: vec![],
            span,
        };
        let checked = aura_sema::check_file(&file).expect("check attribute");
        let attribute = &checked.ast.functions[0].attributes[0];
        assert_eq!(attribute.name.name, "deprecated");
        assert_eq!(attribute.args.len(), 1);
    }

    #[test]
    fn generated_artifact_exposes_retained_attribute_and_expansion_abi() {
        let file = parse_file(
            "package demo\n@reflect\n@derive(Equals) class Point(@deprecated(\"old\") val x: Int) {}\n",
        )
        .expect("metadata fixture parses");
        let checked = aura_sema::check_file(&file).expect("metadata fixture checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("#define AURA_METADATA_ABI_VERSION 1u"));
        assert!(generated.contains("aura_generated_attribute_metadata"));
        assert!(generated.contains("aura_generated_expansion_metadata"));
        assert!(generated.contains("Point.equals"));
        assert!(generated.contains("AURA_METADATA_ABI_VERSION"));
    }

    #[test]
    fn hash_code_derive_reaches_c_codegen_with_builtin_hash_calls() {
        let file = parse_file(
            "package demo\n@derive(HashCode) class Key(val id: Int, val name: String) {}\n",
        )
        .expect("parse derive input");
        let checked = aura_sema::check_file(&file).expect("hashCode derive should typecheck");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("hashCode"));
        assert!(generated.contains("aura_hash_string("));
        assert!(generated.contains("INT64_C(31)"));
    }

    #[test]
    fn debug_derive_reaches_c_codegen_with_deterministic_string_parts() {
        let file = parse_file(
            "package demo\n@derive(Debug) class Point(val x: Int, val label: String) {}\n",
        )
        .expect("parse derive input");
        let checked = aura_sema::check_file(&file).expect("Debug derive should typecheck");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("toString"));
        assert!(generated.contains("aura_i64_to_string"));
        assert!(generated.contains("x="));
        assert!(generated.contains("label="));
    }

    #[test]
    fn detector_profile_instruments_reads_and_writes_with_source_ids() {
        let file = parse_file(
            "package demo\nfun main() {\n  var counter = 0\n  counter = counter + 1\n  println(counter.toString())\n}\n",
        )
        .expect("instrumentation fixture parses");
        let checked = aura_sema::check_file(&file).expect("instrumentation fixture checks");
        let plain = emit_c_with(&checked, EmitOptions::default());
        let instrumented = emit_c_with(
            &checked,
            EmitOptions {
                detector: true,
                ..Default::default()
            },
        );
        assert!(!plain.contains("aura_race_record_access((uintptr_t)&"));
        assert!(instrumented.contains("AURA_RACE_READ"));
        assert!(instrumented.contains("AURA_RACE_WRITE"));
        assert!(instrumented.contains("UINT32_C("));
    }

    #[test]
    fn lowers_nested_branch_local_awaits_to_one_resumable_state() {
        let file = parse_file(
            r#"package demo
async fun nested(outer: Bool, inner: Bool, a: Task<Int>, b: Task<Int>, c: Task<Int>, d: Task<Int>): Int {
  if (outer) {
    if (inner) { val value: Int = await a return value }
    else { val value: Int = await b return value }
  } else {
    if (inner) { val value: Int = await c return value }
    else { val value: Int = await d return value }
  }
}
fun main() {}
"#,
        )
        .expect("parse nested branch-local await fixture");
        let checked = aura_sema::check_file(&file).expect("nested await fixture checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("nested-branch suspension states=1 leaves=4"));
        assert!(generated.contains("uint8_t selected_path;"));
        assert!(generated.contains("data->selected_path = 0"));
        assert!(generated.contains("data->selected_path = 3"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame, 1)"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated.contains("aura_task_frame_cancel_requested(frame)"));
    }

    #[test]
    fn lowers_general_cfg_bool_return_with_loop_awaits() {
        let file = parse_file(
            r#"package demo
async fun choose(flag: Bool, first: Task<Bool>, second: Task<Bool>): Bool {
  var index: Int = 0
  var value: Bool = false
  if (flag) {
    while (index < 2) {
      val next: Bool = await first
      value = next
      index = index + 1
    }
  } else {
    while (index < 2) {
      val alternate: Bool = await second
      value = alternate
      index = index + 1
    }
  }
  return value
}
fun main() {}
"#,
        )
        .expect("parse general CFG Bool fixture");
        let checked = aura_sema::check_file(&file).expect("general CFG Bool fixture checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("aura async general CFG Bool lowering"));
        assert!(generated.contains("bool value;"));
        assert!(generated.contains("AURA_TASK_CANCELLED"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
        assert!(generated.contains("aura_task_frame_set_result(frame, __aura_result"));
    }

    #[test]
    fn lowers_general_cfg_heap_class_return_with_loop_awaits() {
        let file = parse_file(
            r#"package demo
class Box(val value: Int) {}
async fun choose(flag: Bool, first: Task<Box>, second: Task<Box>): Box {
  var index: Int = 0
  var value: Box = Box(0)
  if (flag) {
    while (index < 2) {
      val next: Box = await first
      value = next
      gc_collect()
      index = index + 1
    }
  } else {
    while (index < 2) {
      val alternate: Box = await second
      value = alternate
      gc_collect()
      index = index + 1
    }
  }
  return value
}
fun main() {}
"#,
        )
        .expect("parse general CFG class fixture");
        let checked = aura_sema::check_file(&file).expect("general CFG class fixture checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("aura async general CFG Class lowering"));
        assert!(generated.contains("aura_gc_add_root((void **)__aura_result"));
        assert!(generated.contains("aura_gc_remove_root((void **)data)"));
        assert!(generated.contains("aura_task_frame_propagate_error(frame, data->await_task)"));
    }

    #[test]
    fn lowers_general_cfg_loop_then_branch_awaits() {
        let file = parse_file(
            r#"package demo
async fun choose(flag: Bool, first: Task<Bool>, second: Task<Bool>): Bool {
  var index: Int = 0
  var value: Bool = false
  while (index < 2) {
    if (flag) {
      val next: Bool = await first
      value = next
    } else {
      val alternate: Bool = await second
      value = alternate
    }
    index = index + 1
  }
  return value
}
fun main() {}
"#,
        )
        .expect("parse loop-then-branch CFG fixture");
        let checked = aura_sema::check_file(&file).expect("loop-then-branch CFG checks");
        let generated = emit_c_with(&checked, EmitOptions::default());
        assert!(generated.contains("aura async general CFG Bool lowering"));
        assert!(generated.contains("aura_task_frame_set_resume_state(frame"));
        assert!(generated.contains("aura_task_frame_wait_on(frame, data->await_task)"));
    }
}

fn open_erased_async_supported(f: &AsyncFunDecl) -> bool {
    if f.type_params.len() != 1 || f.params.len() != 1 {
        return false;
    }
    let param_name = &f.type_params[0].name.name;
    let param_is_t = f.params[0].ty.type_args.is_empty()
        && f.params[0].ty.fun.is_none()
        && f.params[0].ty.name.name == *param_name;
    let return_is_t = f.return_type.as_ref().is_some_and(|ty| {
        ty.type_args.is_empty() && ty.fun.is_none() && ty.name.name == *param_name
    });
    let Some(Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(return_id)),
        ..
    })) = f.body.stmts.last()
    else {
        return false;
    };
    if !param_is_t || !return_is_t {
        return false;
    }
    let mut result_binding = None;
    for (index, stmt) in f.body.stmts[..f.body.stmts.len() - 1].iter().enumerate() {
        match stmt {
            Stmt::Expr(Expr::Async(AsyncExpr::Await(await_expr)))
                if !expr_contains_async(&await_expr.operand) => {}
            Stmt::Var(var)
                if var.ty.as_ref().is_some_and(|ty| {
                    ty.type_args.is_empty() && ty.fun.is_none() && ty.name.name == *param_name
                }) && matches!(
                    &var.init,
                    Expr::Async(AsyncExpr::Await(await_expr))
                        if !expr_contains_async(&await_expr.operand)
                ) =>
            {
                if var.name.name == return_id.name {
                    result_binding = Some(index);
                }
            }
            _ => return false,
        }
    }
    if return_id.name != f.params[0].name.name && result_binding.is_none() {
        return false;
    }
    true
}

fn open_erased_async_await_operands(f: &AsyncFunDecl) -> Vec<&Expr> {
    f.body
        .stmts
        .iter()
        .take(f.body.stmts.len().saturating_sub(1))
        .filter_map(|stmt| match stmt {
            Stmt::Expr(Expr::Async(AsyncExpr::Await(await_expr))) => {
                Some(await_expr.operand.as_ref())
            }
            Stmt::Var(var) => match &var.init {
                Expr::Async(AsyncExpr::Await(await_expr)) => Some(await_expr.operand.as_ref()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn open_erased_async_result_await_index(f: &AsyncFunDecl) -> Option<usize> {
    let Some(Stmt::Return(ReturnStmt {
        value: Some(Expr::Ident(return_id)),
        ..
    })) = f.body.stmts.last()
    else {
        return None;
    };
    if return_id.name == f.params[0].name.name {
        return None;
    }
    f.body.stmts[..f.body.stmts.len().saturating_sub(1)]
        .iter()
        .enumerate()
        .find_map(|(index, stmt)| match stmt {
            Stmt::Var(var)
                if var.name.name == return_id.name
                    && matches!(var.init, Expr::Async(AsyncExpr::Await(_))) =>
            {
                Some(index)
            }
            _ => None,
        })
}

fn emit_open_erased_async_operand(
    operand: &Expr,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    data_expr: &str,
) -> String {
    if let Expr::Call(call) = operand {
        if let (Expr::Ident(callee), [Expr::Ident(argument)]) =
            (call.callee.as_ref(), call.args.as_slice())
        {
            if argument.name == f.params[0].name.name {
                if let Some(target) = checked.ast.async_functions.iter().find(|candidate| {
                    candidate.name.name == callee.name
                        && candidate.type_params.len() == 1
                        && candidate.params.len() == 1
                        && candidate.params[0].ty.name.name == candidate.type_params[0].name.name
                        && candidate
                            .return_type
                            .as_ref()
                            .is_some_and(|ty| ty.name.name == candidate.type_params[0].name.name)
                        && (open_erased_async_supported(candidate)
                            || open_erased_async_forward_supported(candidate, checked))
                }) {
                    let package = async_fun_decl_package(target, checked);
                    return format!(
                        "{}({data_expr})",
                        c_fun_name(&package, &target.name.name, &[])
                    );
                }
            }
        }
    }
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let mut ctx = async_ctx(checked, false, &params, &f.params, &f.return_type);
    emit_expr(operand, &mut ctx)
}

fn open_erased_async_forward_target(
    f: &AsyncFunDecl,
    checked: &CheckedFile,
) -> Option<(String, String)> {
    let Some(Stmt::Return(ReturnStmt {
        value: Some(Expr::Async(AsyncExpr::Await(await_expr))),
        ..
    })) = f.body.stmts.first()
    else {
        return None;
    };
    let Expr::Call(call) = await_expr.operand.as_ref() else {
        return None;
    };
    let Expr::Ident(callee) = call.callee.as_ref() else {
        return None;
    };
    if call.args.len() != 1
        || !matches!(&call.args[0], Expr::Ident(id) if id.name == f.params[0].name.name)
    {
        return None;
    }
    let target = checked.ast.async_functions.iter().find(|candidate| {
        candidate.name.name == callee.name
            && candidate.type_params.len() == 1
            && candidate.params.len() == 1
            && candidate.params[0].ty.name.name == candidate.type_params[0].name.name
            && candidate
                .return_type
                .as_ref()
                .is_some_and(|ty| ty.name.name == candidate.type_params[0].name.name)
    })?;
    if !open_erased_async_supported(target) {
        return None;
    }
    Some((
        async_fun_decl_package(target, checked),
        target.name.name.clone(),
    ))
}

fn open_erased_async_forward_supported(f: &AsyncFunDecl, checked: &CheckedFile) -> bool {
    f.type_params.len() == 1
        && f.params.len() == 1
        && f.params[0].ty.name.name == f.type_params[0].name.name
        && f.return_type
            .as_ref()
            .is_some_and(|ty| ty.name.name == f.type_params[0].name.name)
        && open_erased_async_forward_target(f, checked).is_some()
}

/// Emit the descriptor-backed open-generic identity function. The body may
/// contain any number of discarded, non-nested awaits before returning the
/// descriptor payload; value-dependent operations still require specialization.
fn emit_open_erased_async_fun(out: &mut String, f: &AsyncFunDecl, checked: &CheckedFile) {
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_open_erased_data_{base}");
    let poll = format!("aura_open_erased_poll_{base}");
    let destroy = format!("aura_open_erased_destroy_{base}");
    let mark = format!("aura_open_erased_mark_{base}");
    let param = mangle_ident(&f.params[0].name.name);
    let await_operands = open_erased_async_await_operands(f);
    let result_await_index = open_erased_async_result_await_index(f);
    let await_expressions: Vec<String> = await_operands
        .iter()
        .map(|operand| emit_open_erased_async_operand(operand, f, checked, "data->value"))
        .collect();
    let await_fields = await_expressions
        .iter()
        .enumerate()
        .map(|(index, _)| {
            format!(" AuraTaskFrame *await_task_{index}; bool await_task_{index}_owned;")
        })
        .collect::<String>();
    let _ = writeln!(
        out,
        "typedef struct {data_ty}_state {{ AuraTypeErasedValue value;{await_fields} }} {data_ty}_state;"
    );
    let _ = writeln!(out, "static void {destroy}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame);"
    );
    for (index, _) in await_expressions.iter().enumerate() {
        out.push_str(&format!("  if (data != NULL && data->await_task_{index} != NULL && data->await_task_{index}_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task_{index});\n"));
    }
    out.push_str("  if (data != NULL) aura_type_erased_drop(&data->value);\n");
    out.push_str("}\n\n");
    let _ = writeln!(out, "static void {mark}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame); if (data != NULL) aura_type_erased_mark(&data->value);"
    );
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    if !await_expressions.is_empty() {
        let _ = writeln!(out, "  {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame); if (data == NULL) return AURA_TASK_FAILED; switch (aura_task_frame_resume_state(frame)) {{");
        for (index, operand) in await_expressions.iter().enumerate() {
            let init_state = index * 2;
            let poll_state = init_state + 1;
            let _ = writeln!(out, "    case {init_state}: data->await_task_{index} = {operand}; data->await_task_{index}_owned = true; if (data->await_task_{index} == NULL) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, {poll_state}); /* fall through */");
            let result_update = if result_await_index == Some(index) {
                " AuraTypeErasedValue child_value = {0}; if (aura_task_frame_result_erased(data->await_task_INDEX, &child_value) != AURA_FFI_OK) return AURA_TASK_FAILED; aura_type_erased_drop(&data->value); data->value = child_value;"
                    .replace("INDEX", &index.to_string())
            } else {
                String::new()
            };
            let _ = writeln!(out, "    case {poll_state}: {{ AuraTaskPollState child_state = aura_task_frame_state(data->await_task_{index}); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_{index}); if (child_state == AURA_TASK_PENDING) {{ if (!aura_task_frame_wait_on(frame, data->await_task_{index})) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task_{index}); return AURA_TASK_FAILED; }} if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;{result_update} if (data->await_task_{index}_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task_{index}); data->await_task_{index}_owned = false; /* fall through */ }}");
        }
        let _ = writeln!(out, "    case {}: if (aura_task_frame_set_erased_result(frame, &data->value) != AURA_FFI_OK) return AURA_TASK_FAILED; return AURA_TASK_COMPLETE; default: return AURA_TASK_FAILED; }}", await_expressions.len() * 2);
    } else {
        let _ = writeln!(
            out,
            "  {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame); if (data == NULL || aura_task_frame_set_erased_result(frame, &data->value) != AURA_FFI_OK) return AURA_TASK_FAILED; return AURA_TASK_COMPLETE;"
        );
    }
    out.push_str("}\n\n");
    let _ = writeln!(out, "{c_sig} {{", c_sig = c_async_fun_signature(f, checked));
    out.push_str(&format!(
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}_state), {poll}, {destroy}); if (frame == NULL) return NULL; aura_task_frame_set_race_source_id(frame, UINT32_C({source})); {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame); if (data == NULL || aura_type_erased_clone(&{param}, &data->value) != AURA_FFI_OK) {{ aura_task_frame_destroy(frame); return NULL; }} aura_task_frame_set_gc_mark(frame, {mark}); if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) {{ aura_task_frame_destroy(frame); return NULL; }} return frame;\n",
        source = f.span.start
    ));
    out.push_str("}\n");
}

fn emit_open_erased_async_forward_fun(out: &mut String, f: &AsyncFunDecl, checked: &CheckedFile) {
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_open_erased_forward_data_{base}");
    let poll = format!("aura_open_erased_forward_poll_{base}");
    let destroy = format!("aura_open_erased_forward_destroy_{base}");
    let mark = format!("aura_open_erased_forward_mark_{base}");
    let (target_pkg, target_name) =
        open_erased_async_forward_target(f, checked).expect("validated forward target");
    let target_call = c_fun_name(&target_pkg, &target_name, &[]);
    let param = mangle_ident(&f.params[0].name.name);
    let _ = writeln!(
        out,
        "typedef struct {data_ty}_state {{ AuraTypeErasedValue value; AuraTaskFrame *await_task; bool await_task_owned; }} {data_ty}_state;"
    );
    let _ = writeln!(out, "static void {destroy}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame); if (data != NULL && data->await_task != NULL && data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); if (data != NULL) aura_type_erased_drop(&data->value);"
    );
    out.push_str("}\n\n");
    let _ = writeln!(out, "static void {mark}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame); if (data != NULL) aura_type_erased_mark(&data->value);"
    );
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    let _ = writeln!(
        out,
        "  {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame); if (data == NULL) return AURA_TASK_FAILED; switch (aura_task_frame_resume_state(frame)) {{ case 0: data->await_task = {target_call}(data->value); data->await_task_owned = true; if (data->await_task == NULL) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, 1); /* fall through */ case 1: {{ AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task); if (child_state == AURA_TASK_PENDING) {{ if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED; if (child_state == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }} if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED; AuraTypeErasedValue child_value = {{0}}; if (aura_task_frame_result_erased(data->await_task, &child_value) != AURA_FFI_OK) return AURA_TASK_FAILED; aura_type_erased_drop(&data->value); data->value = child_value; if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task_owned = false; if (aura_task_frame_set_erased_result(frame, &data->value) != AURA_FFI_OK) return AURA_TASK_FAILED; return AURA_TASK_COMPLETE; }} default: return AURA_TASK_FAILED; }}"
    );
    out.push_str("}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}_state), {poll}, {destroy}); if (frame == NULL) return NULL; aura_task_frame_set_race_source_id(frame, UINT32_C({source})); {data_ty}_state *data = ({data_ty}_state *)aura_task_frame_data(frame); if (data == NULL || aura_type_erased_clone(&{param}, &data->value) != AURA_FFI_OK) {{ aura_task_frame_destroy(frame); return NULL; }} aura_task_frame_set_gc_mark(frame, {mark}); if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) {{ aura_task_frame_destroy(frame); return NULL; }} return frame;",
        source = f.span.start
    );
    out.push_str("}\n");
}

fn c_async_fun_signature(f: &AsyncFunDecl, checked: &CheckedFile) -> String {
    c_async_fun_signature_args(f, checked, &[])
}

fn emit_c_type_fallback(out: &mut String, c_type: &str) {
    if c_type == "void" {
        return;
    }
    if c_type == "bool" {
        out.push_str("  return false; /* unreachable CFG fallback */\n");
    } else if c_type.ends_with('*') {
        out.push_str("  return NULL; /* unreachable CFG fallback */\n");
    } else if c_type.starts_with("aura_") {
        let _ = writeln!(
            out,
            "  return ({c_type}){{0}}; /* unreachable CFG fallback */"
        );
    } else {
        out.push_str("  return 0; /* unreachable CFG fallback */\n");
    }
}

fn c_async_fun_signature_args(f: &AsyncFunDecl, checked: &CheckedFile, type_args: &[Ty]) -> String {
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let ps = if f.params.is_empty() {
        "void".into()
    } else {
        f.params
            .iter()
            .map(|p| {
                format!(
                    "{} {}",
                    c_type_ref_subst(&p.ty, checked, &params, type_args),
                    mangle_ident(&p.name.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let pkg = async_fun_decl_package(f, checked);
    format!(
        "AuraTaskFrame * {}({ps})",
        c_fun_name(&pkg, &f.name.name, type_args)
    )
}

fn emit_async_fun_std_time_sleep(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
) -> bool {
    if f.name.name != "sleep"
        || f.params.len() != 1
        || type_ref_local_key_expand(&f.params[0].ty, &[], &[], checked) != "Int"
        || f.return_type
            .as_ref()
            .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked))
            .as_deref()
            != Some("Unit")
    {
        return false;
    }
    let base = c_fun_name("std.time", "sleep", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll = format!("aura_async_poll_{base}");
    let destroy = format!("aura_async_destroy_{base}");
    let milliseconds = mangle_ident(&f.params[0].name.name);
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ int64_t milliseconds; }} {data_ty};"
    );
    let _ = writeln!(
        out,
        "static void {destroy}(AuraTaskFrame *frame) {{ (void)frame; }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED; if (aura_task_frame_resume_state(frame) == 0) {{ if (data->milliseconds < 0 || data->milliseconds > INT32_MAX || !aura_task_frame_wait_deadline(frame, (int)data->milliseconds)) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, 1); return AURA_TASK_PENDING; }} (void)aura_task_frame_take_fd_wait_timeout(frame); return AURA_TASK_COMPLETE; }}"
    );
    let _ = writeln!(
        out,
        "{} {{ AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll}, {destroy}); if (frame == NULL) return NULL; {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); data->milliseconds = {milliseconds}; if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) {{ aura_task_frame_destroy(frame); return NULL; }} return frame; }}",
        c_async_fun_signature(f, checked)
    );
    true
}

fn emit_async_fun_std_io_fd(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    _detector: bool,
) -> bool {
    if async_fun_decl_package(f, checked) != "std.io" || f.params.len() != 2 {
        return false;
    }
    if f.name.name == "readFd"
        && f.return_type.as_ref().map(|t| t.name.name.as_str()) == Some("String")
    {
        return emit_async_fun_std_io_read_fd(out, f, checked, _detector);
    }
    if f.name.name == "readFile"
        && f.return_type.as_ref().map(|t| t.name.name.as_str()) == Some("String")
        && type_ref_local_key_expand(&f.params[0].ty, &[], &[], checked)
            .starts_with("ForeignHandle_")
    {
        return emit_async_fun_std_io_read_file(out, f, checked);
    }
    if f.name.name == "writeFile"
        && f.return_type.as_ref().map(|t| t.name.name.as_str()) == Some("Int")
        && type_ref_local_key_expand(&f.params[0].ty, &[], &[], checked)
            .starts_with("ForeignHandle_")
    {
        return emit_async_fun_std_io_write_file(out, f, checked);
    }
    if f.name.name != "writeFd"
        || f.return_type.as_ref().map(|t| t.name.name.as_str()) != Some("Int")
    {
        return false;
    }
    emit_async_fun_std_io_write_fd(out, f, checked, _detector)
}

fn emit_async_fun_std_http_serve(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
) -> bool {
    if f.params.len() != 2 {
        return false;
    }
    let handler_ty = c_type_ref_subst(&f.params[1].ty, checked, &[], &[]);
    if !handler_ty.starts_with("aura_fp_Fun_") {
        return false;
    }
    let base = c_fun_name("std.http", "serve", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll = format!("aura_async_poll_{base}");
    let destroy = format!("aura_async_destroy_{base}");
    let reap = format!("aura_async_reap_{base}");
    let listener = mangle_ident(&f.params[0].name.name);
    let handler = mangle_ident(&f.params[1].name.name);
    let _ = writeln!(out, "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *listener; AuraFfiHandlePin pin; bool pinned; bool stopping; {handler_ty} handler; AuraTaskFrame **connections; size_t connection_count; size_t connection_capacity; }} {data_ty};");
    let _ = writeln!(out, "static void {reap}({data_ty} *data) {{ if (data == NULL || __aura_task_executor == NULL) return; for (size_t i = 0; i < data->connection_count;) {{ AuraTaskPollState state = aura_task_frame_state(data->connections[i]); if (state == AURA_TASK_COMPLETE || state == AURA_TASK_FAILED || state == AURA_TASK_CANCELLED) {{ AuraTaskFrame *connection = data->connections[i]; if (!aura_task_executor_release_terminal(__aura_task_executor, &connection)) {{ i++; continue; }} data->connection_count--; data->connections[i] = data->connections[data->connection_count]; continue; }} i++; }} }}");
    let _ = writeln!(out, "static void {destroy}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ if (__aura_task_executor != NULL) {{ for (size_t i = 0; i < data->connection_count; i++) (void)aura_task_executor_release(__aura_task_executor, &data->connections[i]); }} free(data->connections); if (data->pinned) (void)aura_ffi_handle_unpin(&data->pin); if (data->handler.env != NULL) aura_fun_env_free(data->handler.env); }} }}");
    let _ = writeln!(out, "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED; {reap}(data); if (!data->pinned) {{ if (aura_ffi_handle_pin_for_boundary(data->listener, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED; data->pinned = true; }} if (aura_signal_shutdown_requested() && !data->stopping) {{ (void)aura_tcp_listener_close((AuraTcpListener *)data->pin.resource); data->stopping = true; }} if (data->stopping) {{ if (data->connection_count == 0) return AURA_TASK_COMPLETE; if (!aura_task_frame_wait_on(frame, data->connections[0])) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} if (data->connection_count >= 64) {{ if (!aura_task_frame_wait_on(frame, data->connections[0])) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} AuraTcpStream *stream = NULL; AuraTcpStatus status = aura_tcp_listener_accept((AuraTcpListener *)data->pin.resource, 0, &stream); if (status == AURA_TCP_CLOSED) {{ data->stopping = true; if (data->connection_count == 0) return AURA_TASK_COMPLETE; if (!aura_task_frame_wait_on(frame, data->connections[0])) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT) {{ if (!aura_task_frame_wait_tcp_listener(frame, (const AuraTcpListener *)data->pin.resource, 1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} if (status != AURA_TCP_OK || stream == NULL) return AURA_TASK_FAILED; AuraFfiOpaqueHandle *stream_handle = NULL; if (aura_ffi_handle_new(stream, aura_destroy_tcp_stream_resource, &stream_handle) != AURA_FFI_OK) {{ aura_tcp_stream_destroy(stream); return AURA_TASK_FAILED; }} {handler_ty} connection_handler = data->handler; if (connection_handler.env != NULL) aura_fun_env_retain(connection_handler.env); AuraTaskFrame *connection = aura_fn_std_http_serveConnection(stream_handle, connection_handler); if (connection == NULL) {{ if (connection_handler.env != NULL) aura_fun_env_free(connection_handler.env); (void)aura_ffi_handle_drop(&stream_handle); return AURA_TASK_FAILED; }} if (data->connection_count == data->connection_capacity) {{ size_t next_capacity = data->connection_capacity == 0 ? 8 : data->connection_capacity * 2; AuraTaskFrame **next = (AuraTaskFrame **)realloc(data->connections, next_capacity * sizeof(*next)); if (next == NULL) {{ (void)aura_task_executor_release(__aura_task_executor, &connection); return AURA_TASK_FAILED; }} data->connections = next; data->connection_capacity = next_capacity; }} data->connections[data->connection_count++] = connection; if (!aura_task_frame_wait_tcp_listener(frame, (const AuraTcpListener *)data->pin.resource, 1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}");
    let _ = writeln!(out, "{} {{ AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll}, {destroy}); if (frame == NULL) return NULL; {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); data->listener = {listener}; data->handler = {handler}; if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) {{ aura_task_frame_destroy(frame); return NULL; }} return frame; }}", c_async_fun_signature(f, checked));
    true
}

fn emit_async_fun_std_http_serve_connection(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
) -> bool {
    if f.params.len() != 2 {
        return false;
    }
    let handler_ty = c_type_ref_subst(&f.params[1].ty, checked, &[], &[]);
    if !handler_ty.starts_with("aura_fp_Fun_") {
        return false;
    }
    let base = c_fun_name("std.http", "serveConnection", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let bridge = format!("aura_async_bridge_{base}");
    let poll = format!("aura_async_poll_{base}");
    let cancel = format!("aura_async_cancel_{base}");
    let destroy = format!("aura_async_destroy_{base}");
    let stream = mangle_ident(&f.params[0].name.name);
    let handler = mangle_ident(&f.params[1].name.name);
    let _ = writeln!(out, "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *stream; AuraFfiOpaqueHandle *connection; {handler_ty} handler; AuraTaskFrame *child; aura_cls_std_http_Request *request; aura_cls_std_http_Response *response; AuraFfiOpaqueHandle *request_handle; AuraFfiOpaqueHandle *response_handle; bool rooted; }} {data_ty};");
    let _ = writeln!(out, "static void {destroy}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL) return; if (data->rooted) {{ aura_gc_remove_root((void **)&data->request); aura_gc_remove_root((void **)&data->response); }} if (data->child != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->child); if (data->request_handle != NULL) (void)aura_ffi_handle_drop(&data->request_handle); if (data->response_handle != NULL) (void)aura_ffi_handle_drop(&data->response_handle); if (data->connection != NULL) (void)aura_ffi_handle_drop(&data->connection); if (data->stream != NULL) (void)aura_ffi_handle_drop(&data->stream); if (data->handler.env != NULL) aura_fun_env_free(data->handler.env); }}");
    let _ = writeln!(out, "static AuraTaskPollState {cancel}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL && data->child != NULL && __aura_task_executor != NULL) {{ AuraTaskPollState state = aura_task_frame_state(data->child); if (state != AURA_TASK_COMPLETE && state != AURA_TASK_FAILED && state != AURA_TASK_CANCELLED) (void)aura_task_executor_cancel(__aura_task_executor, data->child); }} return AURA_TASK_CANCELLED; }}");
    let _ = writeln!(out, "static AuraTaskPollState {bridge}(AuraTaskFrame *frame, const AuraHttpRequest *request, AuraHttpResponse *response, void *user_data) {{ {data_ty} *data = ({data_ty} *)user_data; if (data == NULL || request == NULL || response == NULL) return AURA_TASK_FAILED; if (data->child == NULL) {{ if (aura_ffi_handle_new((void *)request, NULL, &data->request_handle) != AURA_FFI_OK || aura_ffi_handle_new((void *)response, NULL, &data->response_handle) != AURA_FFI_OK) return AURA_TASK_FAILED; data->request = aura_new_std_http_Request(data->request_handle); data->response = aura_new_std_http_Response(data->response_handle, data->connection); if (data->request == NULL || data->response == NULL) return AURA_TASK_FAILED; aura_gc_add_root((void **)&data->request); aura_gc_add_root((void **)&data->response); data->rooted = true; data->child = data->handler.fn(data->handler.env, data->request, data->response); if (data->child == NULL) return AURA_TASK_FAILED; }} AuraTaskPollState state = aura_task_frame_state(data->child); if (state == AURA_TASK_READY) state = aura_task_executor_poll_inline(__aura_task_executor, data->child); if (state == AURA_TASK_PENDING) {{ if (!aura_task_frame_wait_on(frame, data->child)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} if (state == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->child); if (__aura_task_executor != NULL) (void)aura_task_executor_release_terminal(__aura_task_executor, &data->child); return AURA_TASK_FAILED; }} if (state != AURA_TASK_COMPLETE) return state; aura_gc_remove_root((void **)&data->request); aura_gc_remove_root((void **)&data->response); data->rooted = false; if (data->request_handle != NULL) (void)aura_ffi_handle_drop(&data->request_handle); if (data->response_handle != NULL) (void)aura_ffi_handle_drop(&data->response_handle); data->request = NULL; data->response = NULL; if (__aura_task_executor != NULL) (void)aura_task_executor_release_terminal(__aura_task_executor, &data->child); return AURA_TASK_COMPLETE; }}");
    let _ = writeln!(out, "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED; if (aura_task_frame_resume_state(frame) == 0) {{ void *raw = NULL; AuraHttpConnection *connection = NULL; if (aura_ffi_handle_take_owned(&data->stream, &raw) != AURA_FFI_OK || aura_http_connection_create_from_stream((AuraTcpStream *)raw, NULL, &connection) != AURA_HTTP_CONNECTION_OK || connection == NULL) return AURA_TASK_FAILED; if (aura_ffi_handle_new(connection, aura_http_connection_destroy_resource, &data->connection) != AURA_FFI_OK) {{ aura_http_connection_destroy_resource(connection); return AURA_TASK_FAILED; }} aura_task_frame_set_resume_state(frame, 1); }} return aura_http_connection_poll_async_task_handle(frame, data->connection, {bridge}, data); }}");
    let _ = writeln!(out, "{} {{ AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll}, {destroy}); if (frame == NULL) return NULL; aura_task_frame_set_cancel_handler(frame, {cancel}); {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); data->stream = {stream}; data->handler = {handler}; if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) {{ aura_task_frame_destroy(frame); return NULL; }} return frame; }}", c_async_fun_signature(f, checked));
    true
}

fn emit_async_fun_std_http_request_body_chunk(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
) -> bool {
    if f.params.len() != 2 || type_ref_local_key_expand(&f.params[1].ty, &[], &[], checked) != "Int"
    {
        return false;
    }
    let base = c_fun_name("std.http", "readChunk", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll = format!("aura_async_poll_{base}");
    let destroy = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    let body = mangle_ident(&f.params[0].name.name);
    let capacity = mangle_ident(&f.params[1].name.name);

    out.push_str("/* compiler-generated std.http.readChunk: bounded borrowed request reader */\n");
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *handle; AuraFfiHandlePin pin; bool pinned; bool read_active; size_t capacity; char *buffer; }} {data_ty};"
    );
    let _ = writeln!(
        out,
        "static void {destroy}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ if (data->read_active && data->pinned) aura_http_request_body_read_end((const AuraHttpRequest *)data->pin.resource); data->read_active = false; free(data->buffer); if (data->pinned) (void)aura_ffi_handle_unpin(&data->pin); }} }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *value, size_t size) {{ (void)size; if (value != NULL) {{ char **text = (char **)value; free(*text); free(text); }} }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *value, size_t size) {{ (void)size; free(value); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n      if (data == NULL || data->handle == NULL || data->capacity == 0) return AURA_TASK_FAILED;\n      if (aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED;\n      data->pinned = true; if (!aura_http_request_body_read_begin((const AuraHttpRequest *)data->pin.resource)) return AURA_TASK_FAILED; data->read_active = true; data->buffer = (char *)malloc(data->capacity + 1); if (data->buffer == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n    }\n    case 1: {\n      size_t count = 0; AuraTcpStatus status = aura_http_request_read_body((const AuraHttpRequest *)data->pin.resource, (unsigned char *)data->buffer, data->capacity, &count);\n      if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT) { if (!aura_http_request_wait_body(frame, (const AuraHttpRequest *)data->pin.resource)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      if (status != AURA_TCP_OK && status != AURA_TCP_EOF) { const char *message = \"request body read failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    out.push_str(&destroy_error);
    out.push_str(", UINT32_C(0)); if (data->read_active && data->pinned) aura_http_request_body_read_end((const AuraHttpRequest *)data->pin.resource); data->read_active = false; return AURA_TASK_FAILED; }\n      data->buffer[count] = '\\0'; if (data->read_active && data->pinned) aura_http_request_body_read_end((const AuraHttpRequest *)data->pin.resource); data->read_active = false; char **result = (char **)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = data->buffer; data->buffer = NULL; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str(
        "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll}, {destroy});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let _ = writeln!(
        out,
        "  if ({capacity} <= 0) {{ aura_task_frame_destroy(frame); return NULL; }} data->handle = {body} == NULL ? NULL : {body}->handle; data->capacity = (size_t){capacity}; if (data->capacity > 16384) data->capacity = 16384; data->pinned = false; data->read_active = false; data->buffer = NULL;"
    );
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn emit_async_fun_std_udp(out: &mut String, f: &AsyncFunDecl, checked: &CheckedFile) -> bool {
    if f.params.is_empty() {
        return false;
    }
    let base = c_fun_name("std.udp", &f.name.name, &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll = format!("aura_async_poll_{base}");
    let destroy = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let method = f.name.name.as_str();
    let receiver = mangle_ident(&f.params[0].name.name);
    if method == "receive" {
        if f.params.len() != 2 {
            return false;
        }
        let capacity = mangle_ident(&f.params[1].name.name);
        out.push_str("/* compiler-generated std.udp.receive: endpoint-keyed datagram wait */\n");
        let _ = writeln!(out, "typedef struct {data_ty} {{ char *host; int64_t port; int64_t capacity; char *payload; }} {data_ty};");
        let _ = writeln!(out, "static void {destroy}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ free(data->host); free(data->payload); }} }}");
        let _ = writeln!(out, "static void {destroy_result}(void *raw, size_t size) {{ (void)size; if (raw != NULL) {{ aura_cls_std_udp_Datagram *value = (aura_cls_std_udp_Datagram *)raw; aura_gc_remove_root((void **)raw); free(value); }} }}");
        let _ = writeln!(out, "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED; if (aura_task_frame_resume_state(frame) == 0) {{ if (data->capacity <= 0 || !aura_udp_bind(data->host, data->port)) return AURA_TASK_FAILED; aura_task_frame_set_resume_state(frame, 1); }} if (!aura_udp_wait(data->host, data->port, 0)) {{ if (!aura_task_frame_wait_deadline(frame, 1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }} int64_t source_port = 0; const char *source_host = NULL; data->payload = (char *)aura_udp_receive(data->host, data->port, data->capacity, &source_port, &source_host); if (data->payload == NULL || source_host == NULL) return AURA_TASK_FAILED; aura_cls_std_udp_Endpoint *source = aura_new_std_udp_Endpoint(source_host, source_port); free((void *)source_host); if (source == NULL) return AURA_TASK_FAILED; aura_cls_std_udp_Datagram *result = aura_new_std_udp_Datagram(source, data->payload); if (result == NULL) return AURA_TASK_FAILED; data->payload = NULL; aura_gc_add_root((void **)result); aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE; }}");
        let _ = writeln!(out, "{} {{ AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll}, {destroy}); if (frame == NULL) return NULL; {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL || {receiver} == NULL || {receiver}->endpoint == NULL) {{ aura_task_frame_destroy(frame); return NULL; }} data->host = strdup({receiver}->endpoint->host); data->port = {receiver}->endpoint->port; data->capacity = {capacity}; data->payload = NULL; if (data->host == NULL || __aura_task_executor == NULL || !aura_task_executor_submit(__aura_task_executor, frame)) {{ aura_task_frame_destroy(frame); return NULL; }} return frame; }}", c_async_fun_signature(f, checked));
        return true;
    }
    if method == "send" {
        if f.params.len() != 3 {
            return false;
        }
        let target = mangle_ident(&f.params[1].name.name);
        let payload = mangle_ident(&f.params[2].name.name);
        out.push_str("/* compiler-generated std.udp.send: bounded datagram write */\n");
        let _ = writeln!(out, "typedef struct {data_ty} {{ char *host; char *target_host; int64_t port; int64_t target_port; char *payload; }} {data_ty};");
        let _ = writeln!(out, "static void {destroy}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ free(data->host); free(data->target_host); free(data->payload); }} }}");
        let _ = writeln!(
            out,
            "static void {destroy_result}(void *raw, size_t size) {{ (void)size; free(raw); }}"
        );
        let _ = writeln!(out, "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED; if (!aura_udp_bind(data->host, data->port)) return AURA_TASK_FAILED; int64_t sent = aura_udp_send(data->host, data->port, data->target_host, data->target_port, data->payload); if (sent < 0) {{ if (!aura_task_frame_wait_deadline(frame, 1)) return AURA_TASK_FAILED; sent = aura_udp_send(data->host, data->port, data->target_host, data->target_port, data->payload); }} if (sent < 0) return AURA_TASK_FAILED; int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = sent; aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result}); return AURA_TASK_COMPLETE; }}");
        let _ = writeln!(out, "{} {{ AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll}, {destroy}); if (frame == NULL) return NULL; {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL || {receiver} == NULL || {receiver}->endpoint == NULL || {target} == NULL || {target}->host == NULL) {{ aura_task_frame_destroy(frame); return NULL; }} data->host = strdup({receiver}->endpoint->host); data->port = {receiver}->endpoint->port; data->target_host = strdup({target}->host); data->target_port = {target}->port; data->payload = strdup({payload} == NULL ? \"\" : {payload}); if (data->host == NULL || data->target_host == NULL || data->payload == NULL || __aura_task_executor == NULL || !aura_task_executor_submit(__aura_task_executor, frame)) {{ aura_task_frame_destroy(frame); return NULL; }} return frame; }}", c_async_fun_signature(f, checked));
        return true;
    }
    false
}

fn emit_async_fun_std_net_stream(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    is_read: bool,
) -> bool {
    let second_key = type_ref_local_key_expand(&f.params[1].ty, &[], &[], checked);
    if (is_read && second_key != "Int") || (!is_read && second_key != "String") {
        return false;
    }
    let base = c_fun_name("std.net", &f.name.name, &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    let description = if is_read { "readStream" } else { "writeStream" };
    out.push_str(&format!(
        "/* compiler-generated std.net.{description}: pinned AuraTcpStream + readiness resume */\n"
    ));
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *handle; AuraFfiHandlePin pin; bool pinned; uint64_t capacity; uint64_t length; uint64_t offset; char *buffer; }} {data_ty};"
    );
    let _ = writeln!(
        out,
        "static void {destroy_data}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ if (data->buffer != NULL) free(data->buffer); if (data->pinned) (void)aura_ffi_handle_unpin(&data->pin); }} }}"
    );
    if is_read {
        let _ = writeln!(out, "static void {destroy_result}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ char **value = (char **)data; free(*value); free(value); }} }}");
    } else {
        let _ = writeln!(
            out,
            "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}"
        );
    }
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  switch (aura_task_frame_resume_state(frame)) {\n    case 0:\n");
    if is_read {
        out.push_str("      if (data == NULL || data->handle == NULL || data->capacity > SIZE_MAX - 1) return AURA_TASK_FAILED;\n      if (aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED; data->pinned = true;\n      data->buffer = (char *)malloc((size_t)data->capacity + 1); if (data->buffer == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n    case 1: {\n      size_t count = 0; AuraTcpStatus status = aura_tcp_stream_read((AuraTcpStream *)data->pin.resource, data->buffer, (size_t)data->capacity, &count, 0);\n      if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT) { if (!aura_task_frame_wait_tcp_stream(frame, (const AuraTcpStream *)data->pin.resource, 1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      if (status != AURA_TCP_OK && status != AURA_TCP_EOF) { const char *message = \"readStream failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    } else {
        out.push_str("      if (data == NULL || data->handle == NULL || data->length > SIZE_MAX) return AURA_TASK_FAILED;\n      if (aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED; data->pinned = true;\n      data->offset = 0; if (data->length == 0) { int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = 0; aura_task_frame_set_result(frame, result, sizeof(*result), ");
        out.push_str(&destroy_result);
        out.push_str("); return AURA_TASK_COMPLETE; }\n      aura_task_frame_set_resume_state(frame, 1);\n    case 1: {\n      size_t count = 0; AuraTcpStatus status = aura_tcp_stream_write((AuraTcpStream *)data->pin.resource, data->buffer + data->offset, (size_t)(data->length - data->offset), &count, 0);\n      if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT) { if (!aura_task_frame_wait_tcp_stream(frame, (const AuraTcpStream *)data->pin.resource, 4)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      if (status != AURA_TCP_OK || count == 0) { const char *message = \"writeStream failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    }
    out.push_str(&destroy_error);
    let _ = writeln!(out, ", UINT32_C(0)); return AURA_TASK_FAILED; }}");
    if is_read {
        out.push_str("\n      data->buffer[count] = '\\0'; char **result = (char **)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = data->buffer; data->buffer = NULL; aura_task_frame_set_result(frame, result, sizeof(*result), ");
        out.push_str(&destroy_result);
        out.push_str(
            "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
        );
    } else {
        out.push_str("\n      data->offset += count; if (data->offset < data->length) { if (!aura_task_frame_wait_tcp_stream(frame, (const AuraTcpStream *)data->pin.resource, 4)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = (int64_t)data->offset; free(data->buffer); data->buffer = NULL; aura_task_frame_set_result(frame, result, sizeof(*result), ");
        out.push_str(&destroy_result);
        out.push_str(
            "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
        );
    }
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let handle = mangle_ident(&f.params[0].name.name);
    let value = mangle_ident(&f.params[1].name.name);
    if is_read {
        let _ = writeln!(out, "  data->handle = {handle}; data->capacity = (uint64_t){value}; data->length = 0; data->offset = 0; data->pinned = false; data->buffer = NULL;");
    } else {
        let _ = writeln!(out, "  data->handle = {handle}; data->length = {value} == NULL ? 0 : (uint64_t)strlen({value}); data->capacity = 0; data->offset = 0; data->pinned = false; data->buffer = NULL;");
        let _ = writeln!(out, "  if (data->length != 0) {{ data->buffer = (char *)malloc((size_t)data->length); if (data->buffer == NULL) {{ aura_task_frame_destroy(frame); return NULL; }} memcpy(data->buffer, {value}, (size_t)data->length); }}");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn emit_async_fun_std_net_accept(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
) -> bool {
    if f.params.len() != 1
        || !type_ref_local_key_expand(&f.params[0].ty, &[], &[], checked)
            .starts_with("ForeignHandle_")
        || !f.return_type.as_ref().is_some_and(|ty| {
            type_ref_local_key_expand(ty, &[], &[], checked).starts_with("ForeignHandle_")
        })
    {
        return false;
    }
    let base = c_fun_name("std.net", "accept", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    let handle = mangle_ident(&f.params[0].name.name);
    out.push_str(
        "/* compiler-generated std.net.accept: pinned AuraTcpListener + readiness resume */\n",
    );
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *listener; AuraFfiHandlePin pin; bool pinned; }} {data_ty};"
    );
    let _ = writeln!(
        out,
        "static void {destroy_data}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL && data->pinned) (void)aura_ffi_handle_unpin(&data->pin); }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ AuraFfiOpaqueHandle **value = (AuraFfiOpaqueHandle **)data; if (*value != NULL) (void)aura_ffi_handle_drop(value); free(value); }} }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  switch (aura_task_frame_resume_state(frame)) {\n    case 0:\n");
    out.push_str("      if (data == NULL || data->listener == NULL) return AURA_TASK_FAILED;\n      if (aura_ffi_handle_pin_for_boundary(data->listener, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED; data->pinned = true;\n      aura_task_frame_set_resume_state(frame, 1);\n    case 1: {\n      AuraTcpStream *__stream = NULL; AuraTcpStatus status = aura_tcp_listener_accept((AuraTcpListener *)data->pin.resource, 0, &__stream);\n      if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT) { if (!aura_task_frame_wait_tcp_listener(frame, (const AuraTcpListener *)data->pin.resource, 1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n      if (status != AURA_TCP_OK || __stream == NULL) { const char *message = \"accept failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    out.push_str(&destroy_error);
    out.push_str(", UINT32_C(0)); return AURA_TASK_FAILED; }\n      AuraFfiOpaqueHandle *__handle = NULL; if (aura_ffi_handle_new((void *)__stream, aura_destroy_tcp_stream_resource, &__handle) != AURA_FFI_OK) { aura_tcp_stream_destroy(__stream); return AURA_TASK_FAILED; } AuraFfiOpaqueHandle **result = (AuraFfiOpaqueHandle **)malloc(sizeof(*result)); if (result == NULL) { (void)aura_ffi_handle_drop(&__handle); return AURA_TASK_FAILED; } *result = __handle; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str(
        "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let _ = writeln!(out, "  data->listener = {handle}; data->pinned = false;");
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn emit_async_fun_std_io_read_file(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
) -> bool {
    if type_ref_local_key_expand(&f.params[1].ty, &[], &[], checked) != "Int" {
        return false;
    }
    let base = c_fun_name("std.io", "readFile", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    out.push_str("/* compiler-generated std.io.readFile: pinned AuraFile + owned task buffer */\n");
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *handle; AuraFfiHandlePin pin; bool pinned; uint64_t capacity; char *buffer; }} {data_ty};"
    );
    let _ = writeln!(
        out,
        "static void {destroy_data}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ if (data->buffer != NULL) free(data->buffer); if (data->pinned) (void)aura_ffi_handle_unpin(&data->pin); }} }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ char **value = (char **)data; free(*value); free(value); }} }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0:\n");
    out.push_str("      if (data == NULL || data->handle == NULL || data->capacity > SIZE_MAX - 1) return AURA_TASK_FAILED;\n");
    out.push_str("      if (aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED;\n      data->pinned = true;\n");
    out.push_str("      data->buffer = (char *)malloc((size_t)data->capacity + 1); if (data->buffer == NULL) return AURA_TASK_FAILED;\n");
    out.push_str("      aura_task_frame_set_resume_state(frame, 1);\n      /* regular files are normally ready; descriptor-backed adapters may pend */\n    case 1: {\n");
    out.push_str("      uint64_t count = 0; AuraFileStatus status = aura_file_read((AuraFile *)data->pin.resource, data->buffer, data->capacity, &count);\n");
    out.push_str("      if (status == AURA_FILE_PENDING) { if (!aura_task_frame_wait_file(frame, (const AuraFile *)data->pin.resource, 1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (status != AURA_FILE_OK && status != AURA_FILE_EOF) { const char *message = \"readFile failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    out.push_str(&destroy_error);
    out.push_str(", UINT32_C(0)); return AURA_TASK_FAILED; }\n");
    out.push_str("      data->buffer[count] = '\\0'; char **result = (char **)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = data->buffer; data->buffer = NULL; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str(
        "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let handle = mangle_ident(&f.params[0].name.name);
    let capacity = mangle_ident(&f.params[1].name.name);
    let _ = writeln!(out, "  data->handle = {handle}; data->capacity = (uint64_t){capacity}; data->pinned = false; data->buffer = NULL;");
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn emit_async_fun_std_io_write_file(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
) -> bool {
    if type_ref_local_key_expand(&f.params[1].ty, &[], &[], checked) != "String" {
        return false;
    }
    let base = c_fun_name("std.io", "writeFile", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    out.push_str(
        "/* compiler-generated std.io.writeFile: pinned AuraFile + short-write resume */\n",
    );
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ AuraFfiOpaqueHandle *handle; AuraFfiHandlePin pin; bool pinned; uint64_t length; uint64_t offset; char *buffer; }} {data_ty};"
    );
    let _ = writeln!(
        out,
        "static void {destroy_data}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) {{ if (data->buffer != NULL) free(data->buffer); if (data->pinned) (void)aura_ffi_handle_unpin(&data->pin); }} }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  switch (aura_task_frame_resume_state(frame)) {\n    case 0:\n");
    out.push_str("      if (data == NULL || data->handle == NULL || data->length > SIZE_MAX) return AURA_TASK_FAILED;\n");
    out.push_str("      if (aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED; data->pinned = true;\n");
    out.push_str("      data->offset = 0; if (data->length == 0) { int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = 0; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str("); return AURA_TASK_COMPLETE; }\n");
    out.push_str("      aura_task_frame_set_resume_state(frame, 1);\n    case 1: {\n");
    out.push_str("      uint64_t count = 0; AuraFileStatus status = aura_file_write((AuraFile *)data->pin.resource, data->buffer + data->offset, data->length - data->offset, &count);\n");
    out.push_str("      if (status == AURA_FILE_PENDING) { if (!aura_task_frame_wait_file(frame, (const AuraFile *)data->pin.resource, 4)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (status != AURA_FILE_OK || count == 0) { const char *message = \"writeFile failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    out.push_str(&destroy_error);
    out.push_str(", UINT32_C(0)); return AURA_TASK_FAILED; }\n");
    out.push_str("      data->offset += count; if (data->offset < data->length) { if (!aura_task_frame_wait_file(frame, (const AuraFile *)data->pin.resource, 4)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = (int64_t)data->offset; free(data->buffer); data->buffer = NULL; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str(
        "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let handle = mangle_ident(&f.params[0].name.name);
    let content = mangle_ident(&f.params[1].name.name);
    let _ = writeln!(out, "  data->handle = {handle}; data->length = {content} == NULL ? 0 : (uint64_t)strlen({content}); data->offset = 0; data->pinned = false; data->buffer = NULL;");
    out.push_str("  if (data->length != 0) { data->buffer = (char *)malloc((size_t)data->length); if (data->buffer == NULL) { aura_task_frame_destroy(frame); return NULL; } memcpy(data->buffer, ");
    out.push_str(&content);
    out.push_str(", (size_t)data->length); }\n");
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn emit_async_fun_std_io_read_fd(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    _detector: bool,
) -> bool {
    let base = c_fun_name("std.io", "readFd", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    out.push_str("/* compiler-generated std.io.readFd: descriptor wait + resume state */\n");
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ int64_t fd; uint64_t capacity; char *buffer; }} {data_ty};"
    );
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) free(data->buffer); }}");
    let _ = writeln!(out, "static void {destroy_result}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ char **value = (char **)data; free(*value); free(value); }} }}");
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    out.push_str("  ");
    let _ = writeln!(
        out,
        "{data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("    case 0:\n");
    out.push_str("      if (data == NULL || data->fd < 0 || data->capacity > SIZE_MAX - 1) return AURA_TASK_FAILED;\n");
    out.push_str("      data->buffer = (char *)malloc((size_t)data->capacity + 1);\n");
    out.push_str("      if (data->buffer == NULL) return AURA_TASK_FAILED;\n");
    out.push_str(
        "      if (!aura_task_frame_wait_fd(frame, (int)data->fd, 1)) return AURA_TASK_FAILED;\n",
    );
    out.push_str("      aura_task_frame_set_resume_state(frame, 1);\n");
    out.push_str("      /* fall through for descriptors that were already ready */\n");
    out.push_str("    case 1: {\n");
    out.push_str(
        "      int64_t count = aura_io_read_fd((int)data->fd, data->buffer, data->capacity);\n",
    );
    out.push_str("      if (count == -EAGAIN || count == -EWOULDBLOCK) { if (!aura_task_frame_wait_fd(frame, (int)data->fd, 1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (count < 0) { const char *message = \"readFd failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    out.push_str(&destroy_error);
    out.push_str(", UINT32_C(0)); return AURA_TASK_FAILED; }\n");
    out.push_str("      data->buffer[count] = '\\0'; char **result = (char **)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = data->buffer; data->buffer = NULL; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str(
        "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let fd = mangle_ident(&f.params[0].name.name);
    let capacity = mangle_ident(&f.params[1].name.name);
    let _ = writeln!(
        out,
        "  data->fd = {fd}; data->capacity = (uint64_t){capacity}; data->buffer = NULL;"
    );
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn emit_async_fun_std_io_write_fd(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    _detector: bool,
) -> bool {
    let base = c_fun_name("std.io", "writeFd", &[]);
    let data_ty = format!("aura_async_data_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    out.push_str(
        "/* compiler-generated std.io.writeFd: owned buffer + short-write resume state */\n",
    );
    let _ = writeln!(
        out,
        "typedef struct {data_ty} {{ int64_t fd; uint64_t length; uint64_t offset; char *buffer; }} {data_ty};"
    );
    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{ {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data != NULL) free(data->buffer); }}");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *data, size_t size) {{ (void)size; free(data); }}"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0:\n");
    out.push_str("      if (data == NULL || data->fd < 0 || data->length > SIZE_MAX) return AURA_TASK_FAILED;\n");
    out.push_str("      data->offset = 0;\n");
    out.push_str("      if (data->length == 0) { int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = 0; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str("); return AURA_TASK_COMPLETE; }\n");
    out.push_str("      if (!aura_task_frame_wait_fd(frame, (int)data->fd, 4)) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n      /* fall through for descriptors that were already ready */\n    case 1: {\n");
    out.push_str("      int64_t count = aura_io_write_fd((int)data->fd, data->buffer + data->offset, data->length - data->offset);\n");
    out.push_str("      if (count == -EAGAIN || count == -EWOULDBLOCK) { if (!aura_task_frame_wait_fd(frame, (int)data->fd, 4)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (count <= 0) { const char *message = \"writeFd failed\"; size_t length = strlen(message) + 1; char *error = (char *)malloc(length); if (error == NULL) return AURA_TASK_FAILED; memcpy(error, message, length); aura_task_frame_set_error_at(frame, error, length, ");
    out.push_str(&destroy_error);
    out.push_str(", UINT32_C(0)); return AURA_TASK_FAILED; }\n");
    out.push_str("      data->offset += (uint64_t)count; if (data->offset < data->length) { if (!aura_task_frame_wait_fd(frame, (int)data->fd, 4)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      int64_t *result = (int64_t *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = (int64_t)data->offset; free(data->buffer); data->buffer = NULL; aura_task_frame_set_result(frame, result, sizeof(*result), ");
    out.push_str(&destroy_result);
    out.push_str(
        "); return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n",
    );
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    let fd = mangle_ident(&f.params[0].name.name);
    let content = mangle_ident(&f.params[1].name.name);
    let _ = writeln!(out, "  data->fd = {fd}; data->length = {content} == NULL ? 0 : (uint64_t)strlen({content}); data->offset = 0; data->buffer = NULL;");
    let _ = writeln!(out, "  if (data->length != 0) {{ data->buffer = (char *)malloc((size_t)data->length); if (data->buffer == NULL) {{ aura_task_frame_destroy(frame); return NULL; }} memcpy(data->buffer, {content}, (size_t)data->length); }}");
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Emit an unbounded straight-line sequence of four or more awaits.
///
/// The older A6 helper deliberately spells out two and three awaits.  This
/// helper uses the same frame ABI but derives every resume state from the AST,
/// so adding another await does not require another hand-written state arm.
/// Primitive values and aggregate arrays use independent frame ownership so a
/// repeated join never aliases a child result or a frame-local aggregate.
fn emit_async_fun_general_multi_await(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    let awaits: Vec<(usize, &VarStmt, &AwaitExpr)> = f
        .body
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, stmt)| {
            let Stmt::Var(v) = stmt else { return None };
            let Expr::Async(AsyncExpr::Await(a)) = &v.init else {
                return None;
            };
            Some((index, v, a))
        })
        .collect();
    let await_keys: Vec<String> = awaits
        .iter()
        .map(|(_, v, _)| {
            v.ty.as_ref()
                .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
                .unwrap_or_else(|| "Int".into())
        })
        .collect();
    let await_owns_task: Vec<bool> = awaits
        .iter()
        .map(|(_, _, await_expr)| await_operand_is_temporary(&await_expr.operand, checked))
        .collect();
    if awaits.len() < 4
        || f.return_type
            .as_ref()
            .map(|t| {
                let key = type_ref_local_key_expand(t, &[], &[], checked);
                !matches!(key.as_str(), "Int" | "Bool" | "String") && !is_array_type_key(&key)
            })
            .unwrap_or(true)
        || await_keys.iter().any(|key| {
            !matches!(key.as_str(), "Int" | "Bool" | "String") && !is_array_type_key(key)
        })
    {
        return false;
    }
    if f.body.stmts.iter().enumerate().any(|(index, stmt)| {
        if awaits.iter().any(|(i, _, _)| *i == index) {
            return false;
        }
        let invalid = match stmt {
            Stmt::Var(v) => {
                let key =
                    v.ty.as_ref()
                        .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
                        .unwrap_or_default();
                !(matches!(key.as_str(), "Int" | "String") || is_array_type_key(&key))
                    || matches!(v.init, Expr::Async(_))
            }
            Stmt::Return(_) => index <= awaits.last().unwrap().0,
            Stmt::Expr(Expr::Call(call))
                if matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
                    && call.args.is_empty() =>
            {
                false
            }
            Stmt::Expr(_) => index <= awaits.last().unwrap().0,
            _ => true,
        };
        invalid
    }) {
        return false;
    }
    for pair in awaits.windows(2) {
        if f.body.stmts[pair[0].0 + 1..pair[1].0]
            .iter()
            .any(|s| {
                !matches!(s, Stmt::Var(_))
                    && !matches!(
                        s,
                        Stmt::Expr(Expr::Call(call))
                            if matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
                                && call.args.is_empty()
                    )
            })
        {
            return false;
        }
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let resume_fn = format!("aura_async_resume_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let ret_key = f
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &params, &[], checked))
        .unwrap_or_else(|| "Unit".into());
    let ret_is_array = is_array_type_key(&ret_key);
    let last_await_index = awaits.last().map(|(index, _, _)| *index).unwrap_or(0);
    let locals: Vec<(&VarStmt, String)> = f
        .body
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, stmt)| {
            let Stmt::Var(v) = stmt else { return None };
            // A local created after the final suspension is ordinary resume
            // code, not frame state. Keeping it out of `locals` prevents the
            // resume function from declaring it once from frame data and a
            // second time at its source declaration.
            if index > last_await_index {
                return None;
            }
            if awaits.iter().any(|(_, a, _)| std::ptr::eq(*a, v)) {
                return None;
            }
            let key =
                v.ty.as_ref()
                    .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
                    .unwrap_or_else(|| "Int".into());
            Some((v, key))
        })
        .collect();

    for (state, (_, _, await_expr)) in awaits.iter().enumerate() {
        let _ = writeln!(
            out,
            "/* aura async general suspension state={} kind=await span={}:{} */",
            state + 1,
            await_expr.span.start,
            await_expr.span.end
        );
    }
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    for (v, key) in &locals {
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(out, "  {} {n};", crate::stmt::local_key_to_c(key, checked));
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let _ = writeln!(out, "  bool {n}__owned;");
        }
    }
    for ((_, v, _), key) in awaits.iter().zip(&await_keys) {
        let _ = writeln!(
            out,
            "  {} {};",
            crate::stmt::local_key_to_c(key, checked),
            mangle_ident(&v.name.name)
        );
    }
    for index in 0..awaits.len() {
        let _ = writeln!(out, "  AuraTaskFrame *await_task_{index};");
    }
    let _ = writeln!(out, "}} {data_ty};\n");

    let mut resume_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for (v, key) in &locals {
        resume_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    for ((_, v, _), key) in awaits.iter().zip(&await_keys) {
        resume_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    let _ = writeln!(out, "static {ret} {resume_fn}({data_ty} *data) {{");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
        if p.name.name == "this" {
            let cty = c_type_ref_subst(&p.ty, checked, &params, &[]);
            let _ = writeln!(out, "  {cty} this = data->{n};");
        }
    }
    for (v, key) in &locals {
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            crate::stmt::local_key_to_c(key, checked)
        );
    }
    for ((_, v, _), key) in awaits.iter().zip(&await_keys) {
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            crate::stmt::local_key_to_c(key, checked)
        );
    }
    for stmt in &f.body.stmts[awaits.last().unwrap().0 + 1..] {
        crate::stmt::emit_stmt(out, stmt, 1, &mut resume_ctx);
    }
    emit_return_fallback(out, &f.return_type, checked, &params, &[]);
    out.push_str("}\n\n");

    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for (index, owns_task) in await_owns_task.iter().enumerate() {
        if *owns_task {
            let _ = writeln!(
                out,
                "  if (data->await_task_{index} != NULL && __aura_task_executor != NULL) {{ (void)aura_task_executor_release(__aura_task_executor, &data->await_task_{index}); }}"
            );
        }
    }
    for (v, key) in &locals {
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let n = mangle_ident(&v.name.name);
            let _ = writeln!(out, "  if (data->{n}__owned) free((void *)data->{n});");
        }
    }
    for ((_, v, _), key) in awaits.iter().zip(&await_keys) {
        if key == "String" {
            let n = mangle_ident(&v.name.name);
            let _ = writeln!(out, "  if (data->{n} != NULL) free((void *)data->{n});");
        } else if is_array_type_key(key) {
            let n = mangle_ident(&v.name.name);
            crate::array_emit::emit_array_contents_free(out, 2, &format!("data->{n}"), key);
        }
    }
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{"
    );
    if ret_is_array {
        let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
        let _ = writeln!(
            out,
            "  (void)size; if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data;"
        );
        crate::array_emit::emit_array_contents_free(out, 2, "(*result)", &ret_key);
        out.push_str("  free(result); }\n}\n\n");
    } else if ret == "const char *" {
        out.push_str("  (void)size;\n  if (data != NULL) free((void *)*((const char **)data));\n  free(data);\n}\n\n");
    } else if is_heap_class_mono(&ret_key, checked) {
        out.push_str("  (void)size; if (data != NULL) { aura_gc_remove_root((void **)data); free(data); }\n}\n\n");
    } else if crate::expr::is_enum_mono(&ret_key, checked)
        || crate::expr::is_value_struct_mono(&ret_key, checked)
        || is_iface_type_key(&ret_key, checked)
    {
        let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
        let _ = writeln!(
            out,
            "  (void)size; if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data; {ret_cty}_drop(result); free(result); }}\n"
        );
    } else if is_fun_type_key(&ret_key) {
        let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
        let _ = writeln!(out, "  (void)size; if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data; if (result->env != NULL) aura_fun_env_free(result->env); free(result); }}");
    } else {
        out.push_str("  (void)size;\n  free(data);\n}\n\n");
    }

    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  switch (aura_task_frame_resume_state(frame)) {\n");
    // Entry initializes the first child once.  Every following case polls the
    // child retained by the previous case, then creates the next child only
    // after successful completion.  This is the key invariant that makes the
    // number of suspension points independent of hand-written C cases.
    let (first_index, _, first_await) = awaits[0];
    let mut entry_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let _ = writeln!(out, "    case 0: {{");
    for stmt in &f.body.stmts[..first_index] {
        let Stmt::Var(v) = stmt else { continue };
        let key = locals
            .iter()
            .find(|(x, _)| std::ptr::eq(*x, v))
            .map(|(_, k)| k.as_str())
            .unwrap();
        let init = coerce_expr(&v.init, key, &mut entry_ctx);
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(out, "      data->{n} = {init};");
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let _ = writeln!(out, "      data->{n}__owned = true;");
        }
        entry_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    let first_task = emit_expr(&first_await.operand, &mut entry_ctx);
    let _ = writeln!(out, "      data->await_task_0 = {first_task};");
    out.push_str("      if (data->await_task_0 == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n    }\n");
    for index in 0..awaits.len() {
        let (stmt_index, _, _await_expr) = awaits[index];
        let state = index + 1;
        let _ = writeln!(out, "    case {state}: {{");
        out.push_str(&format!("      AuraTaskPollState child_state_{index} = aura_task_frame_state(data->await_task_{index});\n"));
        out.push_str(&format!("      if (child_state_{index} == AURA_TASK_READY) child_state_{index} = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_{index});\n"));
        out.push_str(&format!("      if (child_state_{index} == AURA_TASK_PENDING) {{ if (!aura_task_frame_wait_on(frame, data->await_task_{index})) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}\n"));
        out.push_str(&format!(
            "      if (child_state_{index} == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n"
        ));
        out.push_str(&format!("      if (child_state_{index} == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task_{index}); return AURA_TASK_FAILED; }}\n"));
        out.push_str(&format!(
            "      if (child_state_{index} != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n"
        ));
        out.push_str(&format!("      AuraTaskResult child_result_{index} = aura_task_frame_result(data->await_task_{index});\n"));
        let value_name = mangle_ident(&awaits[index].1.name.name);
        if await_keys[index] == "String" {
            out.push_str(&format!("      if (data->{value_name} != NULL) free((void *)data->{value_name}); data->{value_name} = NULL; if (child_result_{index}.data != NULL) {{ const char *__s = *((const char **)child_result_{index}.data); if (__s != NULL) {{ size_t __len = strlen(__s); data->{value_name} = (char *)malloc(__len + 1); if (data->{value_name} == NULL) return AURA_TASK_FAILED; memcpy((void *)data->{value_name}, __s, __len + 1); }} }}\n"));
        } else if is_array_type_key(&await_keys[index]) {
            let full_key = full_type_mono(&await_keys[index], checked);
            let cty = crate::stmt::local_key_to_c(&full_key, checked);
            let clone = crate::names::c_method_name(&full_key, "clone");
            let mut free_code = String::new();
            crate::array_emit::emit_array_contents_free(
                &mut free_code,
                0,
                &format!("data->{value_name}"),
                &full_key,
            );
            out.push_str(&format!(
                "      {free_code} data->{value_name} = ({cty}){{0}}; if (child_result_{index}.data != NULL) data->{value_name} = {clone}(({cty} *)child_result_{index}.data);\n"
            ));
        } else {
            let cty = crate::stmt::local_key_to_c(&await_keys[index], checked);
            out.push_str(&format!("      if (child_result_{index}.data != NULL) data->{value_name} = *(({cty} *)child_result_{index}.data);\n"));
        }
        if await_owns_task[index] {
            out.push_str(&format!("      if (__aura_task_executor != NULL) {{ (void)aura_task_executor_release(__aura_task_executor, &data->await_task_{index}); }}\n"));
        }
        out.push_str(&format!("      data->await_task_{index} = NULL;\n"));
        if index + 1 < awaits.len() {
            let next_index = awaits[index + 1].0;
            let mut ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
            for (await_pos, (_, v, _)) in awaits[..=index].iter().enumerate() {
                ctx.define_local(
                    &v.name.name,
                    full_type_mono(&await_keys[await_pos], checked),
                );
            }
            for stmt in &f.body.stmts[..stmt_index] {
                if let Stmt::Var(v) = stmt {
                    if let Some((_, key)) = locals.iter().find(|(x, _)| std::ptr::eq(*x, v)) {
                        ctx.define_local(&v.name.name, full_type_mono(key, checked));
                    }
                }
            }
            for stmt in &f.body.stmts[stmt_index + 1..next_index] {
                if matches!(
                    stmt,
                    Stmt::Expr(Expr::Call(call))
                        if matches!(call.callee.as_ref(), Expr::Ident(id) if id.name == "gc_collect")
                            && call.args.is_empty()
                ) {
                    crate::stmt::emit_stmt(out, stmt, 2, &mut ctx);
                    continue;
                }
                let Stmt::Var(v) = stmt else { continue };
                let key = locals
                    .iter()
                    .find(|(x, _)| std::ptr::eq(*x, v))
                    .map(|(_, k)| k.as_str())
                    .unwrap();
                let init = coerce_expr(&v.init, key, &mut ctx);
                let n = mangle_ident(&v.name.name);
                let _ = writeln!(out, "      data->{n} = {init};");
                if key == "String" && matches!(&v.init, Expr::Binary(_)) {
                    let _ = writeln!(out, "      data->{n}__owned = true;");
                }
                ctx.define_local(&v.name.name, full_type_mono(key, checked));
            }
            let task = emit_expr(&awaits[index + 1].2.operand, &mut ctx);
            let _ = writeln!(out, "      data->await_task_{} = {task};", index + 1);
            let _ = writeln!(
                out,
                "      if (data->await_task_{} == NULL) return AURA_TASK_FAILED;",
                index + 1
            );
            let _ = writeln!(
                out,
                "      aura_task_frame_set_resume_state(frame, {});",
                state + 1
            );
            out.push_str("    }\n");
        } else {
            let _ = writeln!(
                out,
                "      {ret} *result = ({ret} *)malloc(sizeof(*result));"
            );
            out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
            if ret == "const char *" {
                out.push_str("      const char *__returned = ");
                let _ = writeln!(out, "{resume_fn}(data);");
                out.push_str("      if (__returned == NULL) { *result = NULL; } else { size_t __len = strlen(__returned); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) { free(result); return AURA_TASK_FAILED; } memcpy(__copy, __returned, __len + 1); *result = __copy; }\n");
            } else if ret_is_array {
                let clone = crate::names::c_method_name(&ret_key, "clone");
                let _ = writeln!(
                    out,
                    "      {ret} __returned = {resume_fn}(data); *result = {clone}(&__returned);"
                );
            } else if crate::expr::is_enum_mono(&ret_key, checked)
                || crate::expr::is_value_struct_mono(&ret_key, checked)
                || is_iface_type_key(&ret_key, checked)
            {
                let cty = crate::stmt::local_key_to_c(&ret_key, checked);
                let _ = writeln!(
                    out,
                    "      {ret} __returned = {resume_fn}(data); *result = {cty}_clone(&__returned);"
                );
            } else {
                let _ = writeln!(out, "      *result = {resume_fn}(data);");
            }
            let _ = writeln!(out, "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});");
            out.push_str("      return AURA_TASK_COMPLETE;\n    }\n");
        }
    }
    out.push_str("    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

fn await_operand_is_temporary(expr: &Expr, checked: &CheckedFile) -> bool {
    match expr {
        Expr::Async(AsyncExpr::Spawn(_)) => true,
        Expr::Call(call) => match call.callee.as_ref() {
            Expr::Ident(id) => checked
                .ast
                .async_functions
                .iter()
                .any(|f| f.name.name == id.name),
            _ => false,
        },
        _ => false,
    }
}

/// Emit the bounded two/three-await straight-line A6 slice. Each await
/// receives a distinct resume state and child frame slot; locals initialized
/// between the awaits are initialized only after the earlier child completes.
fn emit_async_fun_multi_await(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    let awaits: Vec<(usize, &VarStmt, &AwaitExpr)> = f
        .body
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, stmt)| {
            let Stmt::Var(v) = stmt else { return None };
            let Expr::Async(AsyncExpr::Await(a)) = &v.init else {
                return None;
            };
            Some((index, v, a))
        })
        .collect();
    let await_keys: Vec<String> = awaits
        .iter()
        .map(|(_, v, _)| {
            v.ty.as_ref()
                .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
                .unwrap_or_else(|| "Int".into())
        })
        .collect();
    if !matches!(awaits.len(), 2 | 3)
        || await_keys
            .iter()
            .any(|key| !matches!(key.as_str(), "Int" | "Bool" | "String"))
    {
        return false;
    }
    if f.body.stmts.iter().enumerate().any(|(index, stmt)| {
        if awaits.iter().any(|(i, _, _)| *i == index) {
            return false;
        }
        match stmt {
            Stmt::Var(v) => {
                if matches!(&v.init, Expr::Async(_)) {
                    return true;
                }
                !matches!(
                    v.ty.as_ref()
                        .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
                        .as_deref(),
                    Some("Int") | Some("String")
                )
            }
            Stmt::Expr(e) => matches!(e, Expr::Async(_)),
            _ => false,
        }
    }) {
        return false;
    }
    for pair in awaits.windows(2) {
        if f.body.stmts[pair[0].0 + 1..pair[1].0]
            .iter()
            .any(|s| !matches!(s, Stmt::Var(_)))
        {
            return false;
        }
    }
    if f.body.stmts[awaits.last().expect("awaits is non-empty").0 + 1..]
        .iter()
        .any(|s| !matches!(s, Stmt::Return(_) | Stmt::Expr(_)))
    {
        return false;
    }

    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let resume_fn = format!("aura_async_resume_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let ret_key = f
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &params, &[], checked))
        .unwrap_or_else(|| "Unit".into());
    let locals: Vec<(&VarStmt, String)> = f
        .body
        .stmts
        .iter()
        .filter_map(|stmt| {
            let Stmt::Var(v) = stmt else { return None };
            if awaits.iter().any(|(_, a, _)| std::ptr::eq(*a, v)) {
                return None;
            }
            let key =
                v.ty.as_ref()
                    .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
                    .unwrap_or_else(|| "Int".into());
            Some((v, key))
        })
        .collect();

    for (state, (_, _, a)) in awaits.iter().enumerate() {
        let _ = writeln!(
            out,
            "/* aura async suspension state={} kind=await span={}:{} */",
            state + 1,
            a.span.start,
            a.span.end
        );
    }
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    for (v, key) in &locals {
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(out, "  {} {n};", crate::stmt::local_key_to_c(key, checked));
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let _ = writeln!(out, "  bool {n}__owned;");
        }
    }
    for ((_, v, _), key) in awaits.iter().zip(&await_keys) {
        let _ = writeln!(
            out,
            "  {} {};",
            crate::stmt::local_key_to_c(key, checked),
            mangle_ident(&v.name.name)
        );
    }
    for index in 0..awaits.len() {
        let _ = writeln!(out, "  AuraTaskFrame *await_task_{index};");
    }
    let _ = writeln!(out, "}} {data_ty};\n");

    let mut resume_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for (v, key) in &locals {
        resume_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    for ((_, v, _), key) in awaits.iter().zip(&await_keys) {
        resume_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    let _ = writeln!(out, "static {ret} {resume_fn}({data_ty} *data) {{");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    for (v, key) in &locals {
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            crate::stmt::local_key_to_c(key, checked)
        );
    }
    for ((_, v, _), key) in awaits.iter().zip(&await_keys) {
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            crate::stmt::local_key_to_c(key, checked)
        );
    }
    for stmt in &f.body.stmts[awaits.last().expect("awaits is non-empty").0 + 1..] {
        crate::stmt::emit_stmt(out, stmt, 1, &mut resume_ctx);
    }
    emit_return_fallback(out, &f.return_type, checked, &params, &[]);
    out.push_str("}\n\n");

    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let key = type_ref_local_key_expand(&p.ty, &params, &[], checked);
        if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let name = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  if (data->{name} != NULL) (void)aura_ffi_handle_drop(&data->{name});"
            );
        }
    }
    for (v, key) in &locals {
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let n = mangle_ident(&v.name.name);
            let _ = writeln!(out, "  if (data->{n}__owned) free((void *)data->{n});");
        }
    }
    for ((_, v, _), key) in awaits.iter().zip(&await_keys) {
        if key == "String" {
            let n = mangle_ident(&v.name.name);
            let _ = writeln!(out, "  if (data->{n} != NULL) free((void *)data->{n});");
        }
    }
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{"
    );
    if ret == "const char *" {
        out.push_str("  (void)size;\n  if (data != NULL) free((void *)*((const char **)data));\n  free(data);\n}\n\n");
    } else {
        out.push_str("  (void)size;\n  free(data);\n}\n\n");
    }
    let mut initial_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("    case 0: {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
        if p.name.name == "this" {
            let cty = c_type_ref_subst(&p.ty, checked, &params, &[]);
            let _ = writeln!(out, "      {cty} this = data->{n};");
        }
    }
    for stmt in &f.body.stmts[..awaits[0].0] {
        let Stmt::Var(v) = stmt else { continue };
        let key = locals
            .iter()
            .find(|(x, _)| std::ptr::eq(*x, v))
            .map(|(_, k)| k.as_str())
            .unwrap_or("Int");
        let init = coerce_expr(&v.init, key, &mut initial_ctx);
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = {init};",
            crate::stmt::local_key_to_c(key, checked)
        );
        let _ = writeln!(out, "      data->{n} = {n};");
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let _ = writeln!(out, "      data->{n}__owned = true;");
        }
        initial_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    let task0 = emit_expr(&awaits[0].2.operand, &mut initial_ctx);
    let _ = writeln!(out, "      data->await_task_0 = {task0};");
    out.push_str("      if (data->await_task_0 == NULL) return AURA_TASK_FAILED;\n");
    out.push_str("      aura_task_frame_set_resume_state(frame, 1);\n    }\n");
    out.push_str("    case 1: {\n");
    out.push_str(
        "      AuraTaskPollState child_state_0 = aura_task_frame_state(data->await_task_0);\n",
    );
    out.push_str("      if (child_state_0 == AURA_TASK_READY) child_state_0 = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_0);\n");
    out.push_str("      if (child_state_0 == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task_0)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state_0 == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state_0 == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task_0); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state_0 != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str(
        "      AuraTaskResult child_result_0 = aura_task_frame_result(data->await_task_0);\n",
    );
    let first_name = mangle_ident(&awaits[0].1.name.name);
    if await_keys[0] == "String" {
        out.push_str("      if (data->");
        out.push_str(&first_name);
        out.push_str(" != NULL) free((void *)data->");
        out.push_str(&first_name);
        out.push_str("); data->");
        out.push_str(&first_name);
        out.push_str(" = NULL; if (child_result_0.data != NULL) { const char *__s = *((const char **)child_result_0.data); if (__s != NULL) { size_t __len = strlen(__s); data->");
        out.push_str(&first_name);
        out.push_str(" = (char *)malloc(__len + 1); if (data->");
        out.push_str(&first_name);
        out.push_str(" == NULL) return AURA_TASK_FAILED; memcpy((void *)data->");
        out.push_str(&first_name);
        out.push_str(", __s, __len + 1); } }\n");
    } else {
        let cty = crate::stmt::local_key_to_c(&await_keys[0], checked);
        let _ = writeln!(
            out,
            "      if (child_result_0.data != NULL) data->{first_name} = *(({cty} *)child_result_0.data);"
        );
    }

    let mut middle_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = data->{n};",
            c_type_ref_subst(&p.ty, checked, &params, &[])
        );
    }
    for stmt in &f.body.stmts[..awaits[0].0] {
        if let Stmt::Var(v) = stmt {
            let key = locals
                .iter()
                .find(|(x, _)| std::ptr::eq(*x, v))
                .map(|(_, k)| k.as_str())
                .unwrap_or("Int");
            let n = mangle_ident(&v.name.name);
            let _ = writeln!(
                out,
                "      {} {n} = data->{n};",
                crate::stmt::local_key_to_c(key, checked)
            );
            middle_ctx.define_local(&v.name.name, full_type_mono(key, checked));
        }
    }
    middle_ctx.define_local(
        &awaits[0].1.name.name,
        full_type_mono(&await_keys[0], checked),
    );
    let first_cty = crate::stmt::local_key_to_c(&await_keys[0], checked);
    let _ = writeln!(out, "      {first_cty} {first_name} = data->{first_name};");
    for stmt in &f.body.stmts[awaits[0].0 + 1..awaits[1].0] {
        let Stmt::Var(v) = stmt else { continue };
        let key = locals
            .iter()
            .find(|(x, _)| std::ptr::eq(*x, v))
            .map(|(_, k)| k.as_str())
            .unwrap_or("Int");
        let init = coerce_expr(&v.init, key, &mut middle_ctx);
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(
            out,
            "      {} {n} = {init};",
            crate::stmt::local_key_to_c(key, checked)
        );
        let _ = writeln!(out, "      data->{n} = {n};");
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let _ = writeln!(out, "      data->{n}__owned = true;");
        }
        middle_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    let task1 = emit_expr(&awaits[1].2.operand, &mut middle_ctx);
    let _ = writeln!(out, "      data->await_task_1 = {task1};");
    out.push_str("      if (data->await_task_1 == NULL) return AURA_TASK_FAILED;\n");
    if awaits.len() == 3 {
        out.push_str("      aura_task_frame_set_resume_state(frame, 2);\n    }\n");
        out.push_str("    case 2: {\n");
        out.push_str(
            "      AuraTaskPollState child_state_1 = aura_task_frame_state(data->await_task_1);\n",
        );
        out.push_str("      if (child_state_1 == AURA_TASK_READY) child_state_1 = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_1);\n");
        out.push_str("      if (child_state_1 == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task_1)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
        out.push_str(
            "      if (child_state_1 == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n",
        );
        out.push_str("      if (child_state_1 == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task_1); return AURA_TASK_FAILED; }\n");
        out.push_str("      if (child_state_1 != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
        out.push_str(
            "      AuraTaskResult child_result_1 = aura_task_frame_result(data->await_task_1);\n",
        );
        let second_name = mangle_ident(&awaits[1].1.name.name);
        if await_keys[1] == "String" {
            out.push_str("      if (data->");
            out.push_str(&second_name);
            out.push_str(" != NULL) free((void *)data->");
            out.push_str(&second_name);
            out.push_str("); data->");
            out.push_str(&second_name);
            out.push_str(" = NULL; if (child_result_1.data != NULL) { const char *__s = *((const char **)child_result_1.data); if (__s != NULL) { size_t __len = strlen(__s); data->");
            out.push_str(&second_name);
            out.push_str(" = (char *)malloc(__len + 1); if (data->");
            out.push_str(&second_name);
            out.push_str(" == NULL) return AURA_TASK_FAILED; memcpy((void *)data->");
            out.push_str(&second_name);
            out.push_str(", __s, __len + 1); } }\n");
        } else {
            let cty = crate::stmt::local_key_to_c(&await_keys[1], checked);
            let _ = writeln!(
                out,
                "      if (child_result_1.data != NULL) data->{second_name} = *(({cty} *)child_result_1.data);"
            );
        }

        let mut last_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
        for p in &f.params {
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "      {} {n} = data->{n};",
                c_type_ref_subst(&p.ty, checked, &params, &[])
            );
            last_ctx.define_local(
                &p.name.name,
                full_type_mono(
                    &type_ref_local_key_expand(&p.ty, &params, &[], checked),
                    checked,
                ),
            );
        }
        for stmt in &f.body.stmts[..awaits[0].0] {
            if let Stmt::Var(v) = stmt {
                let key = locals
                    .iter()
                    .find(|(x, _)| std::ptr::eq(*x, v))
                    .map(|(_, k)| k.as_str())
                    .unwrap_or("Int");
                let n = mangle_ident(&v.name.name);
                let _ = writeln!(
                    out,
                    "      {} {n} = data->{n};",
                    crate::stmt::local_key_to_c(key, checked)
                );
                last_ctx.define_local(&v.name.name, full_type_mono(key, checked));
            }
        }
        for (await_pos, (_, v, _)) in awaits[..2].iter().enumerate() {
            let n = mangle_ident(&v.name.name);
            let key = &await_keys[await_pos];
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "      {cty} {n} = data->{n};");
            last_ctx.define_local(&v.name.name, full_type_mono(key, checked));
        }
        for stmt in &f.body.stmts[awaits[1].0 + 1..awaits[2].0] {
            let Stmt::Var(v) = stmt else { continue };
            let key = locals
                .iter()
                .find(|(x, _)| std::ptr::eq(*x, v))
                .map(|(_, k)| k.as_str())
                .unwrap_or("Int");
            let init = coerce_expr(&v.init, key, &mut last_ctx);
            let n = mangle_ident(&v.name.name);
            let _ = writeln!(
                out,
                "      {} {n} = {init};",
                crate::stmt::local_key_to_c(key, checked)
            );
            let _ = writeln!(out, "      data->{n} = {n};");
            if key == "String" && matches!(&v.init, Expr::Binary(_)) {
                let _ = writeln!(out, "      data->{n}__owned = true;");
            }
            last_ctx.define_local(&v.name.name, full_type_mono(key, checked));
        }
        let task2 = emit_expr(&awaits[2].2.operand, &mut last_ctx);
        let _ = writeln!(out, "      data->await_task_2 = {task2};");
        out.push_str("      if (data->await_task_2 == NULL) return AURA_TASK_FAILED;\n");
        out.push_str("      aura_task_frame_set_resume_state(frame, 3);\n    }\n");
        out.push_str("    case 3: {\n");
    } else {
        out.push_str("      aura_task_frame_set_resume_state(frame, 2);\n    }\n");
        out.push_str("    case 2: {\n");
    }
    let final_index = awaits.len() - 1;
    out.push_str(
        &format!("      AuraTaskPollState child_state_{final_index} = aura_task_frame_state(data->await_task_{final_index});\n"),
    );
    let _ = writeln!(out, "      if (child_state_{final_index} == AURA_TASK_READY) child_state_{final_index} = aura_task_executor_poll_inline(__aura_task_executor, data->await_task_{final_index});");
    let _ = writeln!(out, "      if (child_state_{final_index} == AURA_TASK_PENDING) {{ if (!aura_task_frame_wait_on(frame, data->await_task_{final_index})) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }}");
    out.push_str(&format!(
        "      if (child_state_{final_index} == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n"
    ));
    let _ = writeln!(out, "      if (child_state_{final_index} == AURA_TASK_FAILED) {{ (void)aura_task_frame_propagate_error(frame, data->await_task_{final_index}); return AURA_TASK_FAILED; }}");
    let _ = writeln!(
        out,
        "      if (child_state_{final_index} != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;"
    );
    let _ = writeln!(out, "      AuraTaskResult child_result_{final_index} = aura_task_frame_result(data->await_task_{final_index});");
    let final_name = mangle_ident(&awaits[final_index].1.name.name);
    if await_keys[final_index] == "String" {
        out.push_str("      if (data->");
        out.push_str(&final_name);
        out.push_str(" != NULL) free((void *)data->");
        out.push_str(&final_name);
        out.push_str("); data->");
        out.push_str(&final_name);
        out.push_str(" = NULL; if (child_result_");
        out.push_str(&final_index.to_string());
        out.push_str(".data != NULL) { const char *__s = *((const char **)child_result_");
        out.push_str(&final_index.to_string());
        out.push_str(".data); if (__s != NULL) { size_t __len = strlen(__s); data->");
        out.push_str(&final_name);
        out.push_str(" = (char *)malloc(__len + 1); if (data->");
        out.push_str(&final_name);
        out.push_str(" == NULL) return AURA_TASK_FAILED; memcpy((void *)data->");
        out.push_str(&final_name);
        out.push_str(", __s, __len + 1); } }\n");
    } else {
        let cty = crate::stmt::local_key_to_c(&await_keys[final_index], checked);
        let _ = writeln!(
            out,
            "      if (child_result_{final_index}.data != NULL) data->{final_name} = *(({cty} *)child_result_{final_index}.data);"
        );
    }
    if ret == "void" {
        let _ = writeln!(out, "      {resume_fn}(data);");
        out.push_str("      aura_task_frame_set_result(frame, NULL, 0, NULL);\n");
    } else {
        let _ = writeln!(
            out,
            "      {ret} *result = ({ret} *)malloc(sizeof(*result));"
        );
        out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
        if ret == "const char *" {
            out.push_str("      const char *__returned = ");
            let _ = writeln!(out, "{resume_fn}(data);");
            out.push_str("      if (__returned == NULL) { *result = NULL; } else { size_t __len = strlen(__returned); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) { free(result); return AURA_TASK_FAILED; } memcpy(__copy, __returned, __len + 1); *result = __copy; }\n");
        } else if crate::expr::is_enum_mono(&ret_key, checked)
            || crate::expr::is_value_struct_mono(&ret_key, checked)
            || is_iface_type_key(&ret_key, checked)
        {
            let cty = crate::stmt::local_key_to_c(&ret_key, checked);
            let _ = writeln!(
                out,
                "      {ret} __returned = {resume_fn}(data); *result = {cty}_clone(&__returned);"
            );
        } else {
            let _ = writeln!(out, "      *result = {resume_fn}(data);");
        }
        let _ = writeln!(
            out,
            "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});"
        );
    }
    let _ = writeln!(out, "      return AURA_TASK_COMPLETE;\n    }}\n    default: return AURA_TASK_FAILED;\n  }}\n}}\n\n");
    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
        let key = type_ref_local_key_expand(&p.ty, &params, &[], checked);
        if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let _ = writeln!(
                out,
                "  if (data->{n} != NULL && aura_ffi_handle_retain(data->{n}) != AURA_FFI_OK) {{ aura_task_frame_destroy(frame); return NULL; }}"
            );
        }
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n  return frame;\n}\n");
    true
}

/// Emit the first executable A4/A5 state-machine slice: a straight-line async
/// body with one top-level `val x: Int = await task`. Locals declared before
/// the await are copied into frame data and reused by the resume helper. More
/// complex control flow keeps the bounded helper lowering below until its
/// state partitioning and cleanup rules are implemented.
fn emit_async_fun_single_await(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
) -> bool {
    let await_index = f.body.stmts.iter().position(|stmt| {
        matches!(
            stmt,
            Stmt::Var(v)
                if matches!(&v.init, Expr::Async(AsyncExpr::Await(_)))
        )
    });
    let Some(await_index) = await_index else {
        return false;
    };
    if f.body
        .stmts
        .iter()
        .enumerate()
        .any(|(index, stmt)| match stmt {
            Stmt::Var(v) if index == await_index => false,
            Stmt::Var(v) => matches!(&v.init, Expr::Async(_)),
            Stmt::Expr(e) => matches!(e, Expr::Async(_)),
            _ => false,
        })
    {
        return false;
    }
    if f.body.stmts[await_index + 1..]
        .iter()
        .any(|stmt| !matches!(stmt, Stmt::Return(_) | Stmt::Expr(_)))
    {
        return false;
    }
    let Stmt::Var(await_var) = &f.body.stmts[await_index] else {
        return false;
    };
    let Expr::Async(AsyncExpr::Await(await_expr)) = &await_var.init else {
        return false;
    };
    let Some(await_ty) = await_var.ty.as_ref() else {
        return false;
    };
    let await_key = type_ref_local_key_expand(await_ty, &[], &[], checked);
    if await_key != "Int"
        && await_key != "Bool"
        && await_key != "String"
        && !is_array_type_key(&await_key)
        && !is_heap_class_mono(&await_key, checked)
    {
        return false;
    }
    let mut locals = Vec::new();
    for stmt in &f.body.stmts[..await_index] {
        let Stmt::Var(v) = stmt else {
            return false;
        };
        let key =
            v.ty.as_ref()
                .map(|t| type_ref_local_key_expand(t, &[], &[], checked))
                .unwrap_or_else(|| infer_type_name(&v.init, &async_ctx_for_shape(checked)));
        if key != "Int" && key != "String" {
            return false;
        }
        locals.push((v, key));
    }
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let resume_fn = format!("aura_async_resume_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, &[]);
    let await_cty = crate::stmt::local_key_to_c(&await_key, checked);

    let _ = writeln!(
        out,
        "/* aura async suspension state=1 kind=await span={}:{} */",
        await_expr.span.start, await_expr.span.end
    );
    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, &[]),
            mangle_ident(&p.name.name)
        );
    }
    for (v, key) in &locals {
        let cty = crate::stmt::local_key_to_c(key, checked);
        let _ = writeln!(out, "  {cty} {};", mangle_ident(&v.name.name));
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let _ = writeln!(out, "  bool {}__owned;", mangle_ident(&v.name.name));
        }
    }
    out.push_str("  AuraTaskFrame *await_task;\n");
    let _ = writeln!(out, "}} {data_ty};\n");

    let mut resume_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for p in &f.params {
        resume_ctx.define_local(
            &p.name.name,
            full_type_mono(
                &type_ref_local_key_expand(&p.ty, &params, &[], checked),
                checked,
            ),
        );
    }
    for (v, key) in &locals {
        resume_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    resume_ctx.define_local(&await_var.name.name, full_type_mono(&await_key, checked));
    let _ = writeln!(
        out,
        "static {ret} {resume_fn}({data_ty} *data, {await_cty} await_value) {{"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let cty = c_type_ref_subst(&p.ty, checked, &params, &[]);
        let _ = writeln!(out, "  {cty} {n} = data->{n};");
        if p.name.name == "this" {
            let _ = writeln!(out, "  {cty} this = data->{n};");
        }
    }
    for (v, key) in &locals {
        let n = mangle_ident(&v.name.name);
        let _ = writeln!(
            out,
            "  {} {n} = data->{n};",
            crate::stmt::local_key_to_c(key, checked)
        );
    }
    let _ = writeln!(
        out,
        "  {await_cty} {} = await_value;",
        mangle_ident(&await_var.name.name)
    );
    for stmt in &f.body.stmts[await_index + 1..] {
        crate::stmt::emit_stmt(out, stmt, 1, &mut resume_ctx);
    }
    emit_return_fallback(out, &f.return_type, checked, &params, &[]);
    out.push_str("}\n\n");

    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let key = type_ref_local_key_expand(&p.ty, &params, &[], checked);
        if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let name = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  if (data->{name} != NULL) (void)aura_ffi_handle_drop(&data->{name});"
            );
        }
    }
    for (v, key) in &locals {
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let n = mangle_ident(&v.name.name);
            let _ = writeln!(out, "  if (data->{n}__owned) free((void *)data->{n});");
        }
    }
    out.push_str("}\n\n");
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{"
    );
    let ret_key = f
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &params, &[], checked))
        .unwrap_or_else(|| "Unit".into());
    if ret == "const char *" {
        out.push_str("  (void)size;\n  if (data != NULL) free((void *)*((const char **)data));\n  free(data);\n}\n\n");
    } else if is_heap_class_mono(&ret_key, checked) {
        out.push_str("  (void)size;\n  if (data != NULL) { aura_gc_remove_root((void **)data); free(data); }\n}\n\n");
    } else if is_array_type_key(&ret_key) {
        let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
        let _ = writeln!(
            out,
            "  (void)size; if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data;"
        );
        crate::array_emit::emit_array_contents_free(out, 2, "(*result)", &ret_key);
        out.push_str("  free(result); }\n}\n\n");
    } else {
        out.push_str("  (void)size;\n  free(data);\n}\n\n");
    }
    let mut init_ctx = async_ctx(checked, detector, &params, &f.params, &f.return_type);
    for p in &f.params {
        init_ctx.define_local(
            &p.name.name,
            full_type_mono(
                &type_ref_local_key_expand(&p.ty, &params, &[], checked),
                checked,
            ),
        );
    }
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("    case 0: {\n");
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let cty = c_type_ref_subst(&p.ty, checked, &params, &[]);
        let _ = writeln!(out, "      {cty} {n} = data->{n};");
        if p.name.name == "this" {
            let _ = writeln!(out, "      {cty} this = data->{n};");
        }
    }
    for (v, key) in &locals {
        let n = mangle_ident(&v.name.name);
        let init = coerce_expr(&v.init, key, &mut init_ctx);
        let _ = writeln!(
            out,
            "      {} {n} = {init};",
            crate::stmt::local_key_to_c(key, checked)
        );
        let _ = writeln!(out, "      data->{n} = {n};");
        if key == "String" && matches!(&v.init, Expr::Binary(_)) {
            let _ = writeln!(out, "      data->{n}__owned = true;");
        }
        init_ctx.define_local(&v.name.name, full_type_mono(key, checked));
    }
    let task = emit_expr(&await_expr.operand, &mut init_ctx);
    let _ = writeln!(out, "      data->await_task = {task};");
    out.push_str("      if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
    out.push_str("      aura_task_frame_set_resume_state(frame, 1);\n");
    out.push_str("      /* fall through to poll an immediately-ready child. */\n    }\n");
    out.push_str("    case 1: {\n");
    out.push_str(
        "      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) {\n");
    out.push_str(
        "        if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED;\n",
    );
    out.push_str("        return AURA_TASK_PENDING;\n");
    out.push_str("      }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str("      AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n");
    let _ = writeln!(out, "      {await_cty} observed = 0;");
    let _ = writeln!(
        out,
        "      if (child_result.data != NULL) observed = *(({await_cty} *)child_result.data);"
    );
    if ret == "void" {
        let _ = writeln!(out, "      {resume_fn}(data, observed);",);
        out.push_str("      aura_task_frame_set_result(frame, NULL, 0, NULL);\n");
    } else {
        let _ = writeln!(
            out,
            "      {ret} *result = ({ret} *)malloc(sizeof(*result));"
        );
        out.push_str("      if (result == NULL) return AURA_TASK_FAILED;\n");
        if ret == "const char *" {
            out.push_str("      const char *__returned = ");
            let _ = writeln!(out, "{resume_fn}(data, observed);");
            out.push_str("      if (__returned == NULL) { *result = NULL; } else { size_t __len = strlen(__returned); char *__copy = (char *)malloc(__len + 1); if (__copy == NULL) { free(result); return AURA_TASK_FAILED; } memcpy(__copy, __returned, __len + 1); *result = __copy; }\n");
        } else {
            let _ = writeln!(out, "      *result = {resume_fn}(data, observed);");
        }
        if is_heap_class_mono(&ret_key, checked) {
            out.push_str("      aura_gc_add_root((void **)result);\n");
        }
        let _ = writeln!(
            out,
            "      aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});"
        );
    }
    out.push_str("      return AURA_TASK_COMPLETE;\n    }\n");
    out.push_str("    default: return AURA_TASK_FAILED;\n  }\n}\n\n");

    let _ = writeln!(out, "{} {{", c_async_fun_signature(f, checked));
    let _ = writeln!(out, "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});");
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
        let key = type_ref_local_key_expand(&p.ty, &params, &[], checked);
        if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let _ = writeln!(
                out,
                "  if (data->{n} != NULL && aura_ffi_handle_retain(data->{n}) != AURA_FFI_OK) {{ aura_task_frame_destroy(frame); return NULL; }}"
            );
        }
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n");
    out.push_str("  return frame;\n}\n");
    true
}

fn async_ctx<'a>(
    checked: &'a CheckedFile,
    detector: bool,
    params: &[String],
    fparams: &[Param],
    ret: &Option<TypeRef>,
) -> EmitCtx<'a> {
    // Synthetic async class methods carry `this` as their first parameter.
    // Preserve the receiver class in expression lowering so field access such
    // as `this.value` keeps its declared type across CFG suspension states.
    let method_class = fparams.iter().find(|p| p.name.name == "this").map(|p| {
        let key = type_ref_local_key_expand(&p.ty, params, &[], checked);
        Box::leak(full_type_mono(&key, checked).into_boxed_str()) as &'a str
    });
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class,
        type_params: params.to_vec(),
        type_args: Vec::new(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![std::collections::HashSet::new()],
        fun_owners: vec![std::collections::HashSet::new()],
        string_owners: vec![std::collections::HashSet::new()],
        channel_owners: vec![std::collections::HashSet::new()],
        task_result_owners: vec![std::collections::HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![std::collections::HashSet::new()],
        box_owners: vec![std::collections::HashSet::new()],
        gc_roots: vec![std::collections::HashSet::new()],
        array_gc_roots: vec![std::collections::HashSet::new()],
        return_key: ret
            .as_ref()
            .map(|t| type_ref_local_key_expand(t, params, &[], checked)),
        lambda_ids: build_lambda_ids(checked),
        spawn_params: fparams.iter().map(|p| p.name.name.clone()).collect(),
        mutable_spawn_captures: HashSet::new(),
        async_frame: Some("frame".into()),
        task_poller: false,
    };
    for p in fparams {
        let key = type_ref_local_key_expand(&p.ty, params, &[], checked);
        ctx.define_local(&p.name.name, full_type_mono(&key, checked));
    }
    ctx
}

fn async_ctx_for_shape<'a>(checked: &'a CheckedFile) -> EmitCtx<'a> {
    async_ctx(checked, false, &[], &[], &None)
}

/// C22l slice 1: lower an async function with no suspension points to a task
/// frame whose first poll executes an ordinary helper body exactly once.
pub(crate) fn emit_async_fun_no_await(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
    mir_body: Option<&aura_ir::mir::MirBody>,
) {
    emit_async_fun_no_await_args(out, f, checked, detector, &[], mir_body);
}

fn emit_async_fun_no_await_args(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    detector: bool,
    type_args: &[Ty],
    mir_body: Option<&aura_ir::mir::MirBody>,
) {
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = async_fun_decl_package(f, checked);
    let base = format!("{}_{}", mangle_package(&pkg), mangle_ident(&f.name.name));
    let data_ty = format!("aura_async_data_{base}");
    let body_fn = format!("aura_async_body_{base}");
    let poll_fn = format!("aura_async_poll_{base}");
    let destroy_data = format!("aura_async_destroy_{base}");
    let gc_mark = format!("aura_async_gc_mark_{base}");
    let destroy_result = format!("aura_async_result_destroy_{base}");
    let ret = c_type_from_opt(&f.return_type, checked, &params, type_args);

    for point in f.suspension_points() {
        let kind = match point.kind {
            AsyncSuspensionKind::Await => "await",
        };
        let _ = writeln!(
            out,
            "/* aura async suspension state={} kind={} span={}:{} */",
            point.state_id, kind, point.span.start, point.span.end
        );
    }

    let _ = writeln!(out, "typedef struct {data_ty} {{");
    for p in &f.params {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&p.ty, checked, &params, type_args),
            mangle_ident(&p.name.name)
        );
    }
    let _ = writeln!(out, "}} {data_ty};\n");

    let _ = writeln!(out, "static void {destroy_data}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let key = type_ref_local_key_expand(&p.ty, &params, type_args, checked);
        if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  if (data != NULL && data->{n} != NULL) (void)aura_ffi_handle_drop(&data->{n});"
            );
        }
        if key == "Channel" || key.starts_with("Channel_") {
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  if (data != NULL && data->{n} != NULL) aura_task_channel_destroy(data->{n});"
            );
        } else if key == "Task"
            || key.starts_with("Task_")
            || key == "TaskHandle"
            || key.starts_with("TaskHandle_")
        {
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  if (data != NULL && data->{n} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &data->{n});"
            );
        }
        if is_heap_class_mono(&full_type_mono(&key, checked), checked) {
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  if (data != NULL && data->{n} != NULL) aura_gc_remove_root((void **)&data->{n});"
            );
        }
        if crate::array_emit::is_array_of_heap_class(&full_type_mono(&key, checked), checked) {
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  if (data != NULL) aura_gc_remove_array_root((void **)&data->{n}.data);"
            );
        }
    }
    out.push_str("}\n\n");

    let _ = writeln!(out, "static void {gc_mark}(AuraTaskFrame *frame) {{");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL) return;"
    );
    let frame_result_key = f
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &params, type_args, checked))
        .unwrap_or_else(|| "Unit".into());
    emit_async_result_gc_mark(out, &frame_result_key, checked);
    for p in &f.params {
        let key = type_ref_local_key_expand(&p.ty, &params, type_args, checked);
        let full_key = full_type_mono(&key, checked);
        let n = mangle_ident(&p.name.name);
        if is_heap_class_mono(&full_key, checked) {
            let _ = writeln!(out, "  aura_gc_mark_ptr((void *)data->{n});");
        } else if crate::array_emit::is_array_of_heap_class(&full_key, checked) {
            let array_cty = crate::stmt::local_key_to_c(&full_key, checked);
            let _ = writeln!(out, "  {array_cty}_mark(&data->{n});");
        } else if crate::expr::is_enum_mono(&full_key, checked) {
            let cty = crate::stmt::local_key_to_c(&full_key, checked);
            let _ = writeln!(out, "  {cty}_mark(&data->{n});");
        }
    }
    out.push_str("}\n\n");

    let body_params = f
        .params
        .iter()
        .map(|p| {
            format!(
                "{} {}",
                c_type_ref_subst(&p.ty, checked, &params, type_args),
                mangle_ident(&p.name.name)
            )
        })
        .collect::<Vec<_>>();
    let _ = writeln!(
        out,
        "static {ret} {body_fn}(AuraTaskFrame *frame{}) {{",
        if body_params.is_empty() {
            String::new()
        } else {
            format!(", {}", body_params.join(", "))
        }
    );
    let is_std_udp_method = (f.name.name.ends_with("_receive") || f.name.name.ends_with("_send"))
        && f.params.len() >= 2;
    let rendered_mir = !is_std_udp_method
        && mir_body.is_some_and(|body| {
            crate::mir_emit::emit_body_from_mir(out, body, &checked.package, f.params.len(), 1)
        });
    if !rendered_mir {
        emit_async_body(out, f, checked, &params, type_args, detector);
    }
    // CFG lowering may make every source path terminal while C still cannot
    // prove that fact. Keep generated non-void wrappers warning-free with a
    // typed unreachable fallback after the emitted body.
    emit_c_type_fallback(out, &ret);
    out.push_str("}\n\n");

    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{"
    );
    out.push_str("  (void)size;\n");
    let ret_key = f
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &params, type_args, checked))
        .unwrap_or_else(|| "Unit".into());
    if is_array_type_key(&ret_key) {
        let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
        let _ = writeln!(
            out,
            "  if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data;"
        );
        crate::array_emit::emit_array_contents_free(out, 2, "(*result)", &ret_key);
        out.push_str("    free(result); }\n");
    } else if is_heap_class_mono(&ret_key, checked) {
        out.push_str("  if (data != NULL) { aura_gc_remove_root((void **)data); free(data); }\n");
    } else if ret_key.starts_with("ForeignHandle_") {
        out.push_str("  if (data != NULL) { AuraFfiOpaqueHandle **result = (AuraFfiOpaqueHandle **)data; if (*result != NULL) (void)aura_ffi_handle_drop(result); free(result); }\n");
    } else if ret_key == "Channel" || ret_key.starts_with("Channel_") {
        out.push_str("  if (data != NULL) { AuraTaskChannel **result = (AuraTaskChannel **)data; if (*result != NULL) aura_task_channel_destroy(*result); free(result); }\n");
    } else if ret_key == "Task"
        || ret_key.starts_with("Task_")
        || ret_key == "TaskHandle"
        || ret_key.starts_with("TaskHandle_")
    {
        out.push_str("  if (data != NULL) { AuraTaskFrame **result = (AuraTaskFrame **)data; if (*result != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, result); free(result); }\n");
    } else if crate::expr::is_value_struct_mono(&ret_key, checked) {
        let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
        let _ = writeln!(
            out,
            "  if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data; {ret_cty}_drop(result); free(result); }}"
        );
    } else if crate::expr::is_enum_mono(&ret_key, checked) || is_iface_type_key(&ret_key, checked) {
        let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
        let _ = writeln!(
            out,
            "  (void)size; if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data; {ret_cty}_drop(result); free(result); }}\n"
        );
    } else if is_fun_type_key(&ret_key) {
        let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
        let _ = writeln!(out, "  (void)size; if (data != NULL) {{ {ret_cty} *result = ({ret_cty} *)data; if (result->env != NULL) aura_fun_env_free(result->env); free(result); }}");
    } else {
        out.push_str("  free(data);\n");
    }
    out.push_str("}\n\n");
    let clone_bytes = format!("aura_async_error_clone_{base}");
    let clone_string = format!("aura_async_string_error_clone_{base}");
    let destroy_error = format!("aura_async_error_destroy_{base}");
    let _ = writeln!(
        out,
        "static void *{clone_bytes}(const void *src, size_t size, size_t *out_size) {{ void *copy; if (src == NULL || out_size == NULL) return NULL; copy = malloc(size == 0 ? 1 : size); if (copy == NULL) return NULL; if (size != 0) memcpy(copy, src, size); *out_size = size; return copy; }}"
    );
    let _ = writeln!(
        out,
        "static void *{clone_string}(const void *src, size_t size, size_t *out_size) {{ const char *text = (const char *)src; size_t len; char *copy; (void)size; if (text == NULL || out_size == NULL) return NULL; len = strlen(text); copy = (char *)malloc(len + 1); if (copy == NULL) return NULL; memcpy(copy, text, len + 1); *out_size = len + 1; return copy; }}"
    );
    let _ = writeln!(
        out,
        "static void {destroy_error}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let mut class_error_helpers = Vec::new();
    for class in checked
        .ast
        .classes
        .iter()
        .filter(|class| class.type_params.is_empty())
    {
        let mono = type_mono(&class_decl_package(class, checked), &class.name.name, &[]);
        if !is_heap_class_mono(&mono, checked) {
            continue;
        }
        let cty = c_class_type(&mono);
        let clone = format!("aura_async_class_error_clone_{base}_{mono}");
        let destroy = format!("aura_async_class_error_destroy_{base}_{mono}");
        let params: Vec<String> = class
            .type_params
            .iter()
            .map(|param| param.name.name.clone())
            .collect();
        let fields = ownership_fields(class, checked, &params, &[]);
        let mut root_fields = Vec::new();
        let mut array_root_fields = Vec::new();
        let _ = writeln!(
            out,
            "static void *{clone}(const void *src, size_t size, size_t *out_size) {{"
        );
        out.push_str("  (void)size;\n");
        let _ = writeln!(out, "  const {cty} *source = (const {cty} *)src;");
        let _ = writeln!(out, "  {cty} *copy;");
        out.push_str("  if (source == NULL || out_size == NULL) return NULL;\n");
        let _ = writeln!(out, "  copy = ({cty} *)malloc(sizeof(*copy));");
        out.push_str("  if (copy == NULL) return NULL;\n  *copy = *source;\n");
        for (field_name, field_key) in fields {
            let name = mangle_ident(&field_name);
            let full_key = full_type_mono(&field_key, checked);
            if field_key == "String" {
                let _ = writeln!(
                    out,
                    "  if (source->{name} != NULL) {{ size_t len = strlen(source->{name}); char *text = (char *)malloc(len + 1); if (text == NULL) abort(); memcpy(text, source->{name}, len + 1); copy->{name} = text; }}"
                );
            } else if crate::array_emit::is_array_type_key(&full_key) {
                let clone = crate::names::c_method_name(&full_key, "clone");
                let array_cty = crate::stmt::local_key_to_c(&full_key, checked);
                let _ = writeln!(
                    out,
                    "  copy->{name} = {clone}(({array_cty} *)&source->{name});"
                );
                let root = crate::array_emit::array_gc_root_add_call(
                    &format!("copy->{name}.data"),
                    &format!("copy->{name}.len"),
                    &full_key,
                    checked,
                );
                let _ = writeln!(out, "  {root}");
                array_root_fields.push(name.clone());
            } else if is_heap_class_mono(&full_key, checked) {
                let _ = writeln!(
                    out,
                    "  copy->{name} = source->{name}; if (copy->{name} != NULL) aura_gc_add_root((void **)&copy->{name});"
                );
                root_fields.push(name);
            } else if crate::expr::is_enum_mono(&full_key, checked) {
                let nested_cty = crate::stmt::local_key_to_c(&full_key, checked);
                let _ = writeln!(out, "  copy->{name} = {nested_cty}_clone(&source->{name});");
            } else if crate::expr::is_value_struct_mono(&full_key, checked) {
                let nested_cty = crate::stmt::local_key_to_c(&full_key, checked);
                let _ = writeln!(out, "  copy->{name} = {nested_cty}_clone(&source->{name});");
            } else {
                let _ = writeln!(out, "  copy->{name} = source->{name};");
            }
        }
        out.push_str("  *out_size = sizeof(*copy);\n  return copy;\n}\n\n");
        let _ = writeln!(out, "static void {destroy}(void *data, size_t size) {{");
        out.push_str("  (void)size;\n  if (data != NULL) {\n");
        let _ = writeln!(out, "    {cty} *copy = ({cty} *)data;");
        for field in &root_fields {
            let _ = writeln!(
                out,
                "    if (copy->{field} != NULL) aura_gc_remove_root((void **)&copy->{field});"
            );
        }
        for field in &array_root_fields {
            let _ = writeln!(
                out,
                "    aura_gc_remove_array_root((void **)&copy->{field}.data);"
            );
        }
        let _ = writeln!(out, "    aura_ex_dtor_{mono}(data);");
        out.push_str("  }\n}\n\n");
        let message_field = class
            .fields
            .iter()
            .find(|field| {
                field.name.name == "message" && type_ref_local_key(&field.ty, &[], &[]) == "String"
            })
            .map(|field| mangle_ident(&field.name.name));
        class_error_helpers.push((class.name.name.clone(), cty, clone, destroy, message_field));
    }
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll_fn}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n");
    out.push_str("    case 0: break;\n");
    out.push_str("    case 1: return AURA_TASK_COMPLETE;\n");
    out.push_str("    default: return AURA_TASK_FAILED;\n");
    out.push_str("  }\n");
    out.push_str("  aura_task_frame_set_resume_state(frame, 1);\n");
    let args = f
        .params
        .iter()
        .map(|p| format!("data->{}", mangle_ident(&p.name.name)))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str("  jmp_buf __async_jb;\n  if (setjmp(__async_jb) == 0) {\n    aura_try_enter(&__async_jb);\n");
    if ret == "void" {
        let _ = writeln!(
            out,
            "    {body_fn}(frame{});",
            if args.is_empty() {
                String::new()
            } else {
                format!(", {args}")
            }
        );
        out.push_str(
            "    aura_try_leave();\n    aura_task_frame_set_result(frame, NULL, 0, NULL);\n",
        );
    } else {
        // Do not allocate the result slot before calling the body: an Aura
        // throw uses longjmp and would bypass that local allocation. Keep the
        // returned value on the stack until the exception boundary is left,
        // then publish an owned heap result only on the success path.
        let _ = writeln!(
            out,
            "    {ret} body_value = {body_fn}(frame{});",
            if args.is_empty() {
                String::new()
            } else {
                format!(", {args}")
            }
        );
        out.push_str("    aura_try_leave();\n");
        let _ = writeln!(out, "    {ret} *result = ({ret} *)malloc(sizeof(*result));");
        out.push_str("    if (result == NULL) return AURA_TASK_FAILED;\n");
        if crate::expr::is_enum_mono(&ret_key, checked)
            || crate::expr::is_value_struct_mono(&ret_key, checked)
            || is_iface_type_key(&ret_key, checked)
        {
            let ret_cty = crate::stmt::local_key_to_c(&ret_key, checked);
            let _ = writeln!(out, "    *result = {ret_cty}_clone(&body_value);");
        } else {
            out.push_str("    *result = body_value;\n");
        }
        if ret_key.starts_with("ForeignHandle_") {
            out.push_str("    if (*result == NULL || aura_ffi_handle_retain(*result) != AURA_FFI_OK) { free(result); return AURA_TASK_FAILED; }\n");
        }
        if ret_key == "Channel" || ret_key.starts_with("Channel_") {
            out.push_str("    if (*result != NULL && !aura_task_channel_retain(*result)) { free(result); return AURA_TASK_FAILED; }\n");
        } else if ret_key == "Task"
            || ret_key.starts_with("Task_")
            || ret_key == "TaskHandle"
            || ret_key.starts_with("TaskHandle_")
        {
            out.push_str("    if (*result != NULL && (__aura_task_executor == NULL || !aura_task_executor_retain_payload(__aura_task_executor, *result))) { free(result); return AURA_TASK_FAILED; }\n");
        }
        if is_heap_class_mono(&ret_key, checked) {
            out.push_str("    aura_gc_add_root((void **)result);\n");
        }
        let _ = writeln!(
            out,
            "    aura_task_frame_set_result(frame, result, sizeof(*result), {destroy_result});"
        );
    }
    out.push_str("    return AURA_TASK_COMPLETE;\n  }\n");
    out.push_str("  if (aura_ex_matches(\"Int\")) { char *error = (char *)malloc(32); if (error == NULL) { aura_ex_clear(); aura_try_leave(); return AURA_TASK_FAILED; } (void)snprintf(error, 32, \"%lld\", (long long)aura_ex_as_int()); aura_ex_clear(); aura_try_leave(); aura_task_frame_set_error_span_with_clone(frame, error, strlen(error) + 1, ");
    out.push_str(&clone_string);
    out.push_str(", ");
    out.push_str(&destroy_error);
    out.push_str(", 0, 0, 0); aura_task_frame_set_error_type_name(frame, \"Int\"); return AURA_TASK_FAILED; }\n");
    out.push_str("  if (aura_ex_matches(\"Bool\")) { const char *text = aura_ex_as_bool() ? \"true\" : \"false\"; size_t len = strlen(text); char *error = (char *)malloc(len + 1); if (error == NULL) { aura_ex_clear(); aura_try_leave(); return AURA_TASK_FAILED; } memcpy(error, text, len + 1); aura_ex_clear(); aura_try_leave(); aura_task_frame_set_error_span_with_clone(frame, error, len + 1, ");
    out.push_str(&clone_string);
    out.push_str(", ");
    out.push_str(&destroy_error);
    out.push_str(", 0, 0, 0); aura_task_frame_set_error_type_name(frame, \"Bool\"); return AURA_TASK_FAILED; }\n");
    out.push_str("  if (aura_ex_matches(\"String\")) { const char *value = aura_ex_as_string(); size_t len = value ? strlen(value) : 0; char *error = (char *)malloc(len + 1); if (error == NULL) { aura_ex_clear(); aura_try_leave(); return AURA_TASK_FAILED; } if (value != NULL) memcpy(error, value, len + 1); else error[0] = '\\0'; aura_ex_clear(); aura_try_leave(); aura_task_frame_set_error_span_with_clone(frame, error, len + 1, ");
    out.push_str(&clone_string);
    out.push_str(", ");
    out.push_str(&destroy_error);
    out.push_str(", 0, 0, 0); aura_task_frame_set_error_type_name(frame, \"String\"); return AURA_TASK_FAILED; }\n");
    for (type_name, cty, clone, destroy, message_field) in &class_error_helpers {
        let message_expr = message_field.as_ref().map_or_else(
            || format!("\"{type_name}\""),
            |field| format!("(({cty} *)__error_obj)->{field}"),
        );
        let _ = writeln!(
            out,
            "  if (aura_ex_matches(\"{type_name}\")) {{ uint32_t __error_start = aura_ex_source_span_start(); uint32_t __error_end = aura_ex_source_span_end(); void *__error_obj = aura_ex_take_obj(); if (__error_obj == NULL) {{ aura_ex_clear(); aura_try_leave(); return AURA_TASK_FAILED; }} const char *__class_error_text = {message_expr}; size_t __class_error_len = __class_error_text != NULL ? strlen(__class_error_text) : 0; char *__class_error_copy = (char *)malloc(__class_error_len + 1); if (__class_error_copy == NULL) abort(); if (__class_error_text != NULL) memcpy(__class_error_copy, __class_error_text, __class_error_len + 1); else __class_error_copy[0] = '\\0'; aura_try_leave(); aura_task_frame_set_error_span_with_clone(frame, __class_error_copy, __class_error_len + 1, {clone_string}, {destroy_error}, __error_start, __error_start, __error_end); aura_task_frame_set_error_payload_with_clone(frame, __error_obj, sizeof({cty}), {clone}, {destroy}); aura_task_frame_set_error_type_name(frame, \"{type_name}\"); return AURA_TASK_FAILED; }}"
        );
    }
    out.push_str("  { const char *type = aura_ex_type_name(); size_t len = type ? strlen(type) : 0; char *error = (char *)malloc(len + 1); if (error == NULL) { aura_ex_clear(); aura_try_leave(); return AURA_TASK_FAILED; } if (type != NULL) memcpy(error, type, len + 1); else error[0] = '\\0'; aura_ex_clear(); aura_try_leave(); aura_task_frame_set_error_span_with_clone(frame, error, len + 1, ");
    out.push_str(&clone_string);
    out.push_str(", ");
    out.push_str(&destroy_error);
    out.push_str(", 0, 0, 0); return AURA_TASK_FAILED; }\n");
    out.push_str("  aura_ex_clear();\n  aura_try_leave();\n  return AURA_TASK_FAILED;\n}\n\n");

    let _ = writeln!(
        out,
        "{} {{",
        c_async_fun_signature_args(f, checked, type_args)
    );
    let _ = writeln!(
        out,
        "  AuraTaskFrame *frame = aura_task_frame_new(sizeof({data_ty}), {poll_fn}, {destroy_data});"
    );
    out.push_str("  if (frame == NULL) return NULL;\n");
    let _ = writeln!(out, "  aura_task_frame_set_gc_mark(frame, {gc_mark});");
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    for p in &f.params {
        let n = mangle_ident(&p.name.name);
        let _ = writeln!(out, "  data->{n} = {n};");
        let key = type_ref_local_key_expand(&p.ty, &params, type_args, checked);
        if key == "Channel" || key.starts_with("Channel_") {
            let _ = writeln!(
                out,
                "  if (data->{n} != NULL && !aura_task_channel_retain(data->{n})) {{ data->{n} = NULL; aura_task_frame_destroy(frame); return NULL; }}"
            );
        } else if key == "Task"
            || key.starts_with("Task_")
            || key == "TaskHandle"
            || key.starts_with("TaskHandle_")
        {
            let _ = writeln!(
                out,
                "  if (data->{n} != NULL && (__aura_task_executor == NULL || !aura_task_executor_retain_payload(__aura_task_executor, data->{n}))) {{ data->{n} = NULL; aura_task_frame_destroy(frame); return NULL; }}"
            );
        }
        if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let _ = writeln!(
                out,
                "  if (data->{n} != NULL && aura_ffi_handle_retain(data->{n}) != AURA_FFI_OK) {{ aura_task_frame_destroy(frame); return NULL; }}"
            );
        }
        if is_heap_class_mono(&full_type_mono(&key, checked), checked) {
            let _ = writeln!(out, "  aura_gc_add_root((void **)&data->{n});");
        }
        if crate::array_emit::is_array_of_heap_class(&full_type_mono(&key, checked), checked) {
            let full_key = full_type_mono(&key, checked);
            let root = crate::array_emit::array_gc_root_add_call(
                &format!("data->{n}.data"),
                &format!("data->{n}.len"),
                &full_key,
                checked,
            );
            let _ = writeln!(out, "  {root}");
        }
    }
    out.push_str("  if (__aura_task_executor != NULL && !aura_task_executor_submit(__aura_task_executor, frame)) { aura_task_frame_destroy(frame); return NULL; }\n");
    out.push_str("  return frame;\n}\n");
}

fn emit_async_body(
    out: &mut String,
    f: &AsyncFunDecl,
    checked: &CheckedFile,
    params: &[String],
    type_args: &[Ty],
    detector: bool,
) {
    let ret_key = f
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, params, &[], checked));
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: f.params.iter().find(|p| p.name.name == "this").map(|p| {
            let key = type_ref_local_key_expand(&p.ty, params, type_args, checked);
            Box::leak(full_type_mono(&key, checked).into_boxed_str()) as &'static str
        }),
        type_params: params.to_vec(),
        type_args: type_args.to_vec(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![std::collections::HashSet::new()],
        fun_owners: vec![std::collections::HashSet::new()],
        string_owners: vec![std::collections::HashSet::new()],
        channel_owners: vec![std::collections::HashSet::new()],
        task_result_owners: vec![std::collections::HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![std::collections::HashSet::new()],
        box_owners: vec![std::collections::HashSet::new()],
        gc_roots: vec![std::collections::HashSet::new()],
        array_gc_roots: vec![std::collections::HashSet::new()],
        return_key: ret_key,
        lambda_ids: build_lambda_ids(checked),
        spawn_params: f.params.iter().map(|p| p.name.name.clone()).collect(),
        mutable_spawn_captures: mutable_spawn_capture_names(&f.body),
        async_frame: Some("frame".into()),
        task_poller: false,
    };
    for p in &f.params {
        let key = type_ref_local_key_expand(&p.ty, params, &[], checked);
        ctx.define_local(&p.name.name, full_type_mono(&key, checked));
    }
    if let Some(this_param) = f.params.iter().find(|p| p.name.name == "this") {
        let cty = c_type_ref_subst(&this_param.ty, checked, params, &[]);
        out.push_str("  /* Async class-method bodies use the normal `this` spelling. */\n");
        let _ = writeln!(out, "  {cty} this = a_this;");
        if f.name.name.ends_with("_receive") && f.params.len() == 2 && cty.contains("Socket") {
            let capacity = mangle_ident(&f.params[1].name.name);
            out.push_str("  if (this == NULL || this->endpoint == NULL || ");
            out.push_str(&capacity);
            out.push_str(" <= 0 || !aura_udp_bind(this->endpoint->host, this->endpoint->port) || !aura_udp_wait(this->endpoint->host, this->endpoint->port, 1000)) { aura_throw_string(\"std.udp receive failed\"); return NULL; }\n");
            out.push_str("  int64_t __source_port = 0; const char *__source_host = NULL; const char *__payload = aura_udp_receive(this->endpoint->host, this->endpoint->port, ");
            out.push_str(&capacity);
            out.push_str(", &__source_port, &__source_host); if (__payload == NULL || __source_host == NULL) { aura_throw_string(\"std.udp receive failed\"); return NULL; } aura_cls_std_udp_Endpoint *__source = aura_new_std_udp_Endpoint(__source_host, __source_port); free((void *)__source_host); if (__source == NULL) { free((void *)__payload); return NULL; } aura_cls_std_udp_Datagram *__result = aura_new_std_udp_Datagram(__source, __payload); free((void *)__payload); return __result;\n");
            return;
        }
        if f.name.name.ends_with("_send") && f.params.len() == 3 && cty.contains("Socket") {
            let target = mangle_ident(&f.params[1].name.name);
            let payload = mangle_ident(&f.params[2].name.name);
            let _ = writeln!(out, "  if (this == NULL || this->endpoint == NULL || {target} == NULL || {target}->host == NULL || !aura_udp_bind(this->endpoint->host, this->endpoint->port)) {{ aura_throw_string(\"std.udp send failed\"); return 0; }} int64_t __sent = aura_udp_send(this->endpoint->host, this->endpoint->port, {target}->host, {target}->port, {payload} == NULL ? \"\" : {payload}); if (__sent < 0) {{ aura_throw_string(\"std.udp send failed\"); return 0; }} return __sent;");
            return;
        }
    }
    emit_block(out, &f.body, 1, &mut ctx);
    crate::stmt::emit_release_task_handle_owners(out, 1, &ctx, &ctx.task_handle_owners_all());
    crate::stmt::emit_free_fun_owners(out, 1, &ctx, &ctx.fun_owners_all());
    crate::stmt::emit_release_box_locals(out, 1, &ctx, &ctx.box_owners_all());
    emit_return_fallback(out, &f.return_type, checked, params, &[]);
}

pub(crate) fn emit_test_main(out: &mut String, checked: &CheckedFile, detector: bool) {
    let tests: Vec<_> = checked.ast.functions.iter().filter(|f| f.is_test).collect();
    out.push_str("int aura_main(void) {\n");
    out.push_str("  if (!aura_runtime_check_abi(AURA_GENERATED_ABI_VERSION, AURA_GENERATED_ABI_ID)) return 78;\n");
    out.push_str("  __aura_task_executor = aura_task_executor_new();\n");
    if detector {
        out.push_str("  __aura_race_tracker = aura_race_tracker_new();\n");
        out.push_str("  aura_race_tracker_set_active(__aura_race_tracker);\n");
        out.push_str(
            "  aura_task_executor_set_race_tracker(__aura_task_executor, __aura_race_tracker);\n",
        );
    }
    out.push_str("  int failed = 0;\n");
    out.push_str("  int ran = 0;\n");
    if tests.is_empty() {
        out.push_str("  puts(\"no tests\");\n");
        out.push_str("  aura_task_executor_shutdown(__aura_task_executor);\n");
        if detector {
            out.push_str("  aura_race_tracker_destroy(__aura_race_tracker);\n");
        }
        out.push_str("  return 0;\n}\n");
        return;
    }
    for t in &tests {
        let name = &t.name.name;
        let pkg = fun_decl_package(t, checked);
        let fn_c = c_fun_name(&pkg, name, &[]);
        let _ = writeln!(out, "  /* test {name} */");
        out.push_str("  {\n");
        out.push_str("    jmp_buf __tjb;\n");
        let _ = writeln!(out, "    printf(\"test {name} ... \");");
        out.push_str("    fflush(stdout);\n");
        out.push_str("    ran++;\n");
        out.push_str("    if (setjmp(__tjb) == 0) {\n");
        out.push_str("      aura_try_enter(&__tjb);\n");
        let _ = writeln!(out, "      {fn_c}();");
        out.push_str("      aura_try_leave();\n");
        out.push_str("      puts(\"ok\");\n");
        out.push_str("    } else {\n");
        out.push_str("      const char *__msg = aura_ex_matches(\"String\") ? aura_ex_as_string() : \"exception\";\n");
        out.push_str("      printf(\"FAILED (%s)\\n\", __msg ? __msg : \"?\");\n");
        out.push_str("      aura_ex_clear();\n");
        out.push_str("      aura_try_leave();\n");
        out.push_str("      failed++;\n");
        out.push_str("    }\n");
        out.push_str("  }\n");
    }
    out.push_str("  printf(\"%d passed, %d failed\\n\", ran - failed, failed);\n");
    out.push_str("  aura_task_executor_shutdown(__aura_task_executor);\n");
    if detector {
        out.push_str("  aura_race_tracker_destroy(__aura_race_tracker);\n");
    }
    out.push_str("  return failed ? 1 : 0;\n}\n");
}

/// C10e: map LambdaExpr.span.start → stable sequential id (sorted by span).
pub(crate) fn build_lambda_ids(checked: &CheckedFile) -> HashMap<u32, usize> {
    let mut starts: Vec<u32> = checked.lambda_tys.keys().copied().collect();
    starts.sort_unstable();
    starts
        .into_iter()
        .enumerate()
        .map(|(i, s)| (s, i))
        .collect()
}

fn spawn_type_ref_from_key(key: &str, checked: &CheckedFile, span: Span) -> Option<TypeRef> {
    let primitive = match key {
        "Unit" | "Int" | "Bool" | "String" => Some(key.to_string()),
        "Opt_Int" => {
            return Some(TypeRef {
                nullable: true,
                ..aura_ir::generic_lowering::type_ref_from_ty(&Ty::Int, span)
            })
        }
        "Opt_Bool" => {
            return Some(TypeRef {
                nullable: true,
                ..aura_ir::generic_lowering::type_ref_from_ty(&Ty::Bool, span)
            })
        }
        _ => None,
    };
    if let Some(name) = primitive {
        return Some(TypeRef {
            qualifier: None,
            name: Ident { name, span },
            type_args: Vec::new(),
            nullable: false,
            reference: false,
            span,
            fun: None,
        });
    }
    if let Some(inner) = key.strip_prefix("TaskHandle_") {
        let inner_ref = spawn_type_ref_from_key(inner, checked, span)?;
        let inner_ty = crate::names::type_ref_to_ty_subst(&inner_ref, checked, &[], &[]);
        return Some(aura_ir::generic_lowering::type_ref_from_ty(
            &Ty::TaskHandle(Box::new(inner_ty)),
            span,
        ));
    }
    if let Some(inner) = key.strip_prefix("Task_") {
        let inner_ref = spawn_type_ref_from_key(inner, checked, span)?;
        let inner_ty = crate::names::type_ref_to_ty_subst(&inner_ref, checked, &[], &[]);
        return Some(aura_ir::generic_lowering::type_ref_from_ty(
            &Ty::Task(Box::new(inner_ty)),
            span,
        ));
    }
    if let Some(inner) = key.strip_prefix("Channel_") {
        let inner_ref = spawn_type_ref_from_key(inner, checked, span)?;
        let inner_ty = crate::names::type_ref_to_ty_subst(&inner_ref, checked, &[], &[]);
        return Some(aura_ir::generic_lowering::type_ref_from_ty(
            &Ty::Channel(Box::new(inner_ty)),
            span,
        ));
    }
    if let Some(inner) = key.strip_prefix("ForeignHandle_") {
        let inner_ref = spawn_type_ref_from_key(inner, checked, span)?;
        let inner_ty = crate::names::type_ref_to_ty_subst(&inner_ref, checked, &[], &[]);
        return Some(aura_ir::generic_lowering::type_ref_from_ty(
            &Ty::ForeignHandle(Box::new(inner_ty)),
            span,
        ));
    }
    if key.starts_with("Fun_") {
        return None;
    }
    let (base, args) = crate::expr::mono_split(key, checked)?;
    if base == "Array" {
        return Some(aura_ir::generic_lowering::type_ref_from_ty(
            &Ty::ClassApp {
                name: "Array".into(),
                args: args.to_vec(),
            },
            span,
        ));
    }
    let nominal = checked
        .ast
        .classes
        .iter()
        .find(|decl| decl.name.name == base)
        .map(|decl| Ty::ClassApp {
            name: format!(
                "{}@{}",
                base,
                if decl.origin_package.is_empty() {
                    &checked.package
                } else {
                    &decl.origin_package
                }
            ),
            args: args.to_vec(),
        })
        .or_else(|| {
            checked
                .ast
                .enums
                .iter()
                .find(|decl| decl.name.name == base)
                .map(|decl| Ty::EnumApp {
                    name: format!(
                        "{}@{}",
                        base,
                        if decl.origin_package.is_empty() {
                            &checked.package
                        } else {
                            &decl.origin_package
                        }
                    ),
                    args: args.to_vec(),
                })
        })
        .or_else(|| {
            checked
                .ast
                .interfaces
                .iter()
                .find(|decl| decl.name.name == base)
                .map(|decl| Ty::InterfaceApp {
                    name: format!(
                        "{}@{}",
                        base,
                        if decl.origin_package.is_empty() {
                            &checked.package
                        } else {
                            &decl.origin_package
                        }
                    ),
                    args: args.to_vec(),
                })
        })?;
    Some(aura_ir::generic_lowering::type_ref_from_ty(&nominal, span))
}

fn emit_general_spawn_cfg(
    out: &mut String,
    spawn: &SpawnExpr,
    available: &HashMap<String, String>,
    checked: &CheckedFile,
    mutable_captures: &HashSet<String>,
    detector: bool,
) -> bool {
    let Some(captures) = general_spawn_captures(&spawn.body, available, checked, mutable_captures)
    else {
        return false;
    };
    let mut params = Vec::new();
    for capture in &captures {
        let Some(ty) = spawn_type_ref_from_key(&capture.key, checked, spawn.span) else {
            return false;
        };
        params.push(Param {
            attributes: Vec::new(),
            name: Ident {
                name: capture.name.clone(),
                span: spawn.span,
            },
            ty,
            span: spawn.span,
        });
    }
    let return_ty = spawn
        .body
        .stmts
        .iter()
        .rev()
        .find_map(|stmt| match stmt {
            Stmt::Return(ret) => ret.value.as_ref().and_then(|value| {
                checked
                    .expr_tys
                    .get(&(value.span().start, value.span().end))
                    .cloned()
            }),
            _ => None,
        })
        .unwrap_or(Ty::Unit);
    let synthetic = AsyncFunDecl {
        is_pub: false,
        origin_package: checked.package.clone(),
        attributes: Vec::new(),
        is_test: false,
        name: Ident {
            name: format!("__spawn_cfg_{}", spawn.span.start),
            span: spawn.span,
        },
        type_params: Vec::new(),
        params,
        return_type: Some(aura_ir::generic_lowering::type_ref_from_ty(
            &return_ty, spawn.span,
        )),
        body: spawn.body.clone(),
        span: spawn.span,
    };
    emit_async_fun_cfg_int(out, &synthetic, checked, detector, true, mutable_captures)
}

fn general_spawn_box_c_type(key: &str) -> &'static str {
    match key {
        "Int" => "aura_box_i64 *",
        "Bool" => "aura_box_bool *",
        "String" => "aura_box_str *",
        _ => "aura_box_ptr *",
    }
}

fn general_spawn_box_retain(key: &str) -> &'static str {
    match key {
        "Int" => "aura_box_i64_retain",
        "Bool" => "aura_box_bool_retain",
        "String" => "aura_box_str_retain",
        _ => "aura_box_ptr_retain",
    }
}

fn general_spawn_box_release(key: &str) -> &'static str {
    match key {
        "Int" => "aura_box_i64_release",
        "Bool" => "aura_box_bool_release",
        "String" => "aura_box_str_release",
        _ => "aura_box_ptr_release",
    }
}

fn emit_bounded_spawn_pollers(
    out: &mut String,
    checked: &CheckedFile,
    detector: bool,
    generic_functions: &[(String, Vec<Ty>)],
    generic_async_functions: &[(String, Vec<Ty>)],
) {
    let mut spawns = Vec::new();
    // Generic bodies are emitted once per concrete instantiation.  Discover
    // their spawn frames from the same closed AST so TypeParam locals do not
    // leave an unmaterialized poller behind.
    let mono_functions: Vec<FunDecl> = generic_functions
        .iter()
        .filter_map(|(name, args)| {
            checked
                .ast
                .functions
                .iter()
                .find(|f| f.name.name == *name)
                .map(|f| aura_ir::generic_lowering::close_function(f, args, checked))
        })
        .collect();
    let mono_async_functions: Vec<AsyncFunDecl> = generic_async_functions
        .iter()
        .filter_map(|(name, args)| {
            checked
                .ast
                .async_functions
                .iter()
                .find(|f| f.name.name == *name)
                .map(|f| aura_ir::generic_lowering::close_async_function(f, args, checked))
        })
        .collect();
    for f in &checked.ast.functions {
        if !f.type_params.is_empty() {
            continue;
        }
        let available = spawn_parameter_locals(&f.params, &[], &[], checked);
        let mutable = mutable_spawn_capture_names(&f.body);
        collect_spawns_block(&f.body, &available, checked, &mutable, &mut spawns);
    }
    for f in &mono_functions {
        let available = spawn_parameter_locals(&f.params, &[], &[], checked);
        let mutable = mutable_spawn_capture_names(&f.body);
        collect_spawns_block(&f.body, &available, checked, &mutable, &mut spawns);
    }
    for f in &checked.ast.async_functions {
        if !f.type_params.is_empty() {
            continue;
        }
        let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
        let available = spawn_parameter_locals(&f.params, &params, &[], checked);
        let mutable = mutable_spawn_capture_names(&f.body);
        collect_spawns_block(&f.body, &available, checked, &mutable, &mut spawns);
    }
    for f in &mono_async_functions {
        let available = spawn_parameter_locals(&f.params, &[], &[], checked);
        let mutable = mutable_spawn_capture_names(&f.body);
        collect_spawns_block(&f.body, &available, checked, &mutable, &mut spawns);
    }
    for c in &checked.ast.classes {
        for m in &c.methods {
            let mutable = mutable_spawn_capture_names(&m.body);
            collect_spawns_block(&m.body, &HashMap::new(), checked, &mutable, &mut spawns);
        }
    }
    for c in &checked.ast.consts {
        collect_spawns_expr(
            &c.value,
            &HashMap::new(),
            checked,
            &HashSet::new(),
            &mut spawns,
        );
    }

    // Emit declarations for the complete spawn set before definitions. A
    // nested spawn can be referenced from its parent's poller regardless of
    // source-order traversal.
    let mut declared = std::collections::HashSet::new();
    for (spawn, available, mutable_captures) in &spawns {
        let Some(captures) =
            bounded_spawn_captures(&spawn.body, available, checked, mutable_captures)
        else {
            continue;
        };
        let suffix = bounded_spawn_layout_suffix(&captures);
        let poll = bounded_spawn_poll_name_with_suffix(spawn.span, &suffix);
        if declared.insert(poll.clone()) {
            let _ = writeln!(
                out,
                "static AuraTaskPollState {poll}(AuraTaskFrame *frame);"
            );
        }
    }
    let mut emitted = std::collections::HashSet::new();
    for (spawn, available, mutable_captures) in spawns {
        if spawn.body.stmts.is_empty() {
            continue;
        }
        let Some(captures) =
            bounded_spawn_captures(&spawn.body, &available, checked, &mutable_captures)
        else {
            let _ = emit_general_spawn_cfg(
                out,
                spawn,
                &available,
                checked,
                &mutable_captures,
                detector,
            );
            continue;
        };
        let suffix = bounded_spawn_layout_suffix(&captures);
        let poll = bounded_spawn_poll_name_with_suffix(spawn.span, &suffix);
        if !emitted.insert(poll.clone()) {
            continue;
        }
        let await_shape = bounded_spawn_await_shape(&spawn.body, checked);
        let discard_await = bounded_spawn_discard_await_shape(&spawn.body);
        let destroy = bounded_spawn_destroy_name_with_suffix(spawn.span, &suffix);
        let data_ty = bounded_spawn_data_name(spawn.span, &suffix);
        if !captures.is_empty() || await_shape.is_some() || discard_await {
            let _ = writeln!(out, "typedef struct {data_ty} {{");
            for capture in &captures {
                let key = &capture.key;
                let cty = if capture.boxed {
                    match bounded_capture_box_kind(capture) {
                        "string" => "aura_box_str *".to_string(),
                        "i64" => "aura_box_i64 *".to_string(),
                        "bool" => "aura_box_bool *".to_string(),
                        _ => "aura_box_ptr *".to_string(),
                    }
                } else if key == "String" {
                    "aura_box_str *".to_string()
                } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
                    "AuraFfiOpaqueHandle *".to_string()
                } else {
                    crate::stmt::local_key_to_c(key, checked)
                };
                let _ = writeln!(out, "  {} {};", cty, mangle_ident(&capture.name));
            }
            if let Some((await_var, _)) = await_shape.as_ref() {
                let await_key = await_var
                    .ty
                    .as_ref()
                    .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked))
                    .unwrap_or_else(|| "Int".into());
                let await_cty = crate::stmt::local_key_to_c(&await_key, checked);
                let _ = writeln!(
                    out,
                    "  AuraTaskFrame *await_task;\n  bool await_task_owned;\n  {await_cty} await_value;"
                );
            } else if discard_await {
                out.push_str("  AuraTaskFrame *await_task;\n  bool await_task_owned;\n");
            }
            let _ = writeln!(out, "}} {data_ty};\n");
            let gc_mark = bounded_spawn_gc_mark_name(spawn.span, &suffix);
            let _ = writeln!(out, "static void {gc_mark}(AuraTaskFrame *frame) {{");
            let _ = writeln!(out, "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame); if (data == NULL) return;");
            for capture in &captures {
                let key = &capture.key;
                let n = mangle_ident(&capture.name);
                if capture.boxed {
                    continue;
                }
                if is_heap_class_mono(key, checked) {
                    let _ = writeln!(out, "  aura_gc_mark_ptr((void *)data->{n});");
                } else if is_array_type_key(key)
                    || is_iface_type_key(key, checked)
                    || crate::expr::is_enum_mono(key, checked)
                    || crate::expr::is_value_struct_mono(key, checked)
                {
                    let cty = crate::stmt::local_key_to_c(key, checked);
                    let _ = writeln!(out, "  {cty}_mark(&data->{n});");
                }
            }
            if let Some((await_var, _)) = await_shape.as_ref() {
                let await_key = await_var
                    .ty
                    .as_ref()
                    .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked))
                    .unwrap_or_else(|| "Int".into());
                if is_heap_class_mono(&await_key, checked)
                    || is_array_type_key(&await_key)
                    || is_iface_type_key(&await_key, checked)
                    || crate::expr::is_enum_mono(&await_key, checked)
                    || crate::expr::is_value_struct_mono(&await_key, checked)
                {
                    let cty = crate::stmt::local_key_to_c(&await_key, checked);
                    if is_heap_class_mono(&await_key, checked) {
                        let _ = writeln!(out, "  aura_gc_mark_ptr((void *)data->await_value);");
                    } else {
                        let _ = writeln!(out, "  {cty}_mark(&data->await_value);");
                    }
                }
            }
            out.push_str("}\n\n");
            let _ = writeln!(out, "static void {destroy}(AuraTaskFrame *frame) {{");
            let _ = writeln!(
                out,
                "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
            );
            if await_shape.is_some() || discard_await {
                out.push_str("  if (data != NULL && data->await_task != NULL && data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task);\n");
            }
            for capture in &captures {
                let key = &capture.key;
                let n = mangle_ident(&capture.name);
                if capture.boxed {
                    let release = match bounded_capture_box_kind(capture) {
                        "string" => "aura_box_str_release",
                        "i64" => "aura_box_i64_release",
                        "bool" => "aura_box_bool_release",
                        _ => "aura_box_ptr_release",
                    };
                    let _ = writeln!(
                        out,
                        "  if (data != NULL && data->{n} != NULL) {release}(data->{n});"
                    );
                } else if key == "String" {
                    let _ = writeln!(
                        out,
                        "  if (data != NULL && data->{n} != NULL) aura_box_str_release(data->{n});"
                    );
                } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
                    let _ = writeln!(
                        out,
                        "  if (data != NULL && data->{n} != NULL) (void)aura_ffi_handle_drop(&data->{n});"
                    );
                } else if key == "Channel" || key.starts_with("Channel_") {
                    let _ = writeln!(
                        out,
                        "  if (data != NULL && data->{n} != NULL) aura_task_channel_destroy(data->{n});"
                    );
                } else if key == "Task"
                    || key.starts_with("Task_")
                    || key == "TaskHandle"
                    || key.starts_with("TaskHandle_")
                {
                    let _ = writeln!(
                        out,
                        "  if (data != NULL && data->{n} != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, &data->{n});"
                    );
                } else if is_heap_class_mono(key, checked) {
                    let _ = writeln!(
                        out,
                        "  if (data != NULL && data->{n} != NULL) aura_gc_remove_root((void **)&data->{n});"
                    );
                } else if is_iface_type_key(key, checked) {
                    let cty = crate::stmt::local_key_to_c(key, checked);
                    let _ = writeln!(out, "  if (data != NULL) {cty}_drop(&data->{n});");
                } else if is_array_type_key(key) {
                    if crate::array_emit::is_array_of_heap_class(key, checked) {
                        let _ = writeln!(
                            out,
                            "  if (data != NULL) aura_gc_remove_array_root((void **)&data->{n}.data);"
                        );
                    }
                    let cty = crate::stmt::local_key_to_c(key, checked);
                    let _ = writeln!(out, "  if (data != NULL) {cty}_drop(&data->{n});");
                } else if is_fun_type_key(key) {
                    let _ = writeln!(
                        out,
                        "  if (data != NULL && data->{n}.env != NULL) aura_fun_env_free(data->{n}.env);"
                    );
                } else if crate::expr::is_enum_mono(key, checked) {
                    let cty = crate::stmt::local_key_to_c(key, checked);
                    let _ = writeln!(out, "  if (data != NULL) {cty}_drop(&data->{n});");
                } else if crate::expr::is_value_struct_mono(key, checked) {
                    let cty = crate::stmt::local_key_to_c(key, checked);
                    let _ = writeln!(out, "  if (data != NULL) {cty}_drop(&data->{n});");
                } else if is_iface_type_key(key, checked) {
                    let cty = crate::stmt::local_key_to_c(key, checked);
                    let _ = writeln!(out, "  if (data != NULL) {cty}_drop(&data->{n});");
                }
            }
            if let Some((await_var, _)) = await_shape.as_ref() {
                let await_key = await_var
                    .ty
                    .as_ref()
                    .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked))
                    .unwrap_or_else(|| "Int".into());
                if is_array_type_key(&await_key) {
                    let cty = crate::stmt::local_key_to_c(&await_key, checked);
                    let _ = writeln!(out, "  if (data != NULL) {cty}_drop(&data->await_value);");
                } else if crate::expr::is_value_struct_mono(&await_key, checked) {
                    let cty = crate::stmt::local_key_to_c(&await_key, checked);
                    let _ = writeln!(out, "  if (data != NULL) {cty}_drop(&data->await_value);");
                }
            }
            out.push_str("}\n\n");
        }
        if let Some((await_var, await_expr)) = await_shape {
            emit_bounded_spawn_await_poller(
                out,
                &spawn.body,
                await_var,
                await_expr,
                &captures,
                checked,
                detector,
                (&data_ty, &poll),
                spawn.span,
            );
            continue;
        }
        if discard_await {
            emit_bounded_spawn_discard_await_poller(
                out,
                &spawn.body,
                captures.as_slice(),
                checked,
                detector,
                (&data_ty, &poll),
                spawn.span,
            );
            continue;
        }
        if matches!(
            spawn.body.stmts.last(),
            Some(Stmt::Return(ReturnStmt {
                value: Some(Expr::String(_)),
                ..
            }))
        ) {
            emit_bounded_spawn_string_return_poller(
                out,
                &spawn.body,
                &captures,
                checked,
                detector,
                (&data_ty, &poll),
                spawn.span,
            );
            continue;
        }
        if let Some(handle_key) = bounded_spawn_foreign_handle_return_key(&spawn.body, checked) {
            emit_bounded_spawn_foreign_handle_return_poller(
                out,
                &spawn.body,
                &captures,
                checked,
                detector,
                (&data_ty, &poll),
                spawn.span,
                &handle_key,
            );
            continue;
        }
        if let Some(array_key) = bounded_spawn_array_return_key(&spawn.body, checked) {
            emit_bounded_spawn_array_return_poller(
                out,
                &spawn.body,
                &captures,
                checked,
                detector,
                (&data_ty, &poll),
                spawn.span,
                &array_key,
            );
            continue;
        }
        if let Some(class_key) = bounded_spawn_class_return_key(&spawn.body, checked) {
            emit_bounded_spawn_class_return_poller(
                out,
                &spawn.body,
                &captures,
                checked,
                detector,
                (&data_ty, &poll),
                spawn.span,
                &class_key,
            );
            continue;
        }
        let result_destroy = format!("aura_spawn_result_destroy_{}", spawn.span.start);
        let _ = writeln!(
            out,
            "static void {result_destroy}(void *data, size_t size);"
        );
        let _ = writeln!(
            out,
            "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
        );
        if !captures.is_empty() {
            let _ = writeln!(
                out,
                "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
            );
            for capture in &captures {
                let name = &capture.name;
                let key = &capture.key;
                let n = mangle_ident(name);
                if capture.boxed {
                    let cty = match bounded_capture_box_kind(capture) {
                        "string" => "aura_box_str *",
                        "i64" => "aura_box_i64 *",
                        "bool" => "aura_box_bool *",
                        _ => "aura_box_ptr *",
                    };
                    let _ = writeln!(out, "  {cty}{n} = data->{n};");
                } else if key == "String" {
                    let _ = writeln!(
                        out,
                        "  const char *{n} = data->{n} != NULL ? data->{n}->value : NULL;"
                    );
                } else if key == "Channel"
                    || key.starts_with("Channel_")
                    || key == "Task"
                    || key.starts_with("Task_")
                    || key == "TaskHandle"
                    || key.starts_with("TaskHandle_")
                {
                    let _ = writeln!(
                        out,
                        "  {} {n} = data->{n};",
                        crate::stmt::local_key_to_c(key, checked)
                    );
                } else if is_array_type_key(key) {
                    let _ = writeln!(
                        out,
                        "  {} {n} = {}(&data->{n});",
                        crate::stmt::local_key_to_c(key, checked),
                        crate::names::c_method_name(key, "clone")
                    );
                } else if is_fun_type_key(key) {
                    let _ = writeln!(
                        out,
                        "  {} {n} = data->{n}; if ({n}.env != NULL) aura_fun_env_retain({n}.env);",
                        crate::stmt::local_key_to_c(key, checked)
                    );
                } else if crate::expr::is_enum_mono(key, checked) {
                    let _ = writeln!(
                        out,
                        "  {} {n} = data->{n};",
                        crate::stmt::local_key_to_c(key, checked)
                    );
                } else if crate::expr::is_value_struct_mono(key, checked) {
                    let cty = crate::stmt::local_key_to_c(key, checked);
                    let _ = writeln!(out, "  {cty} {n} = {cty}_clone(&data->{n});");
                } else if is_iface_type_key(key, checked) {
                    let cty = crate::stmt::local_key_to_c(key, checked);
                    let _ = writeln!(out, "  {cty} {n} = {cty}_clone(&data->{n});");
                } else {
                    let _ = writeln!(
                        out,
                        "  {} {n} = data->{n};",
                        crate::stmt::local_key_to_c(key, checked)
                    );
                }
            }
        }
        out.push_str(
            "  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n",
        );
        out.push_str(
            "  if (aura_task_frame_resume_state(frame) != 0) return AURA_TASK_COMPLETE;\n",
        );
        out.push_str("  aura_task_frame_set_resume_state(frame, 1);\n");
        let mut ctx = EmitCtx {
            checked,
            detector,
            method_class: None,
            type_params: Vec::new(),
            type_args: Vec::new(),
            locals: vec![HashMap::new()],
            local_c_names: HashMap::new(),
            array_owners: vec![std::collections::HashSet::new()],
            fun_owners: vec![std::collections::HashSet::new()],
            string_owners: vec![std::collections::HashSet::new()],
            channel_owners: vec![std::collections::HashSet::new()],
            task_result_owners: vec![std::collections::HashSet::new()],
            task_handle_owners: vec![std::collections::HashSet::new()],
            box_locals: vec![std::collections::HashSet::new()],
            box_owners: vec![std::collections::HashSet::new()],
            gc_roots: vec![std::collections::HashSet::new()],
            array_gc_roots: vec![std::collections::HashSet::new()],
            return_key: Some("Unit".into()),
            lambda_ids: build_lambda_ids(checked),
            spawn_params: HashSet::new(),
            mutable_spawn_captures: HashSet::new(),
            async_frame: None,
            task_poller: true,
        };
        for capture in &captures {
            let name = &capture.name;
            let key = &capture.key;
            ctx.define_local(name, key.clone());
            if capture.boxed {
                ctx.mark_box_local(name);
            } else if is_array_type_key(key) {
                ctx.mark_array_owner(name);
            }
            if !capture.boxed && is_fun_type_key(key) {
                ctx.mark_fun_owner(name);
            }
        }
        let spawn_return_key =
            full_type_mono(&crate::expr::spawn_result_key(&spawn.body, &ctx), checked);
        ctx.return_key = Some(spawn_return_key.clone());
        let defer_return = spawn_return_key != "Unit";
        for (index, stmt) in spawn.body.stmts.iter().enumerate() {
            crate::stmt::emit_stmt(out, stmt, 1, &mut ctx);
            if defer_return && index + 1 == spawn.body.stmts.len() {
                // The generic poller publishes the captured __ret value below.
                let suffix = "  return AURA_TASK_COMPLETE;\n";
                if out.ends_with(suffix) {
                    out.truncate(out.len() - suffix.len());
                }
            }
        }
        if spawn_return_key != "Unit" {
            let Some(Stmt::Return(ReturnStmt {
                span: return_span, ..
            })) = spawn.body.stmts.last()
            else {
                unreachable!();
            };
            let result_cty = crate::stmt::local_key_to_c(&spawn_return_key, checked);
            let result_destroy = format!("aura_spawn_result_destroy_{}", spawn.span.start);
            let result_tmp = format!("__ret_{}", return_span.start);
            if spawn_return_key == "Task"
                || spawn_return_key.starts_with("Task_")
                || spawn_return_key == "TaskHandle"
                || spawn_return_key.starts_with("TaskHandle_")
            {
                let _ = writeln!(
                    out,
                    "  {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_tmp}; AuraTaskFrame *__owned = *__aura_result; if (__owned != NULL && (__aura_task_executor == NULL || !aura_task_executor_retain_payload(__aura_task_executor, __owned))) {{ free(__aura_result); return AURA_TASK_FAILED; }} if (__owned != NULL) (void)aura_task_executor_release(__aura_task_executor, &__owned); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
                );
            } else if spawn_return_key == "Channel" || spawn_return_key.starts_with("Channel_") {
                let _ = writeln!(
                    out,
                    "  {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_tmp}; if (*__aura_result != NULL && !aura_task_channel_retain(*__aura_result)) {{ free(__aura_result); return AURA_TASK_FAILED; }} if (*__aura_result != NULL) aura_task_channel_destroy({result_tmp}); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
                );
            } else if spawn_return_key == "String" {
                let _ = writeln!(
                    out,
                    "  {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; const char *__source = {result_tmp}; size_t __length = __source == NULL ? 0 : strlen(__source); char *__copy = (char *)malloc(__length + 1); if (__copy == NULL) return AURA_TASK_FAILED; if (__source != NULL) memcpy(__copy, __source, __length + 1); *__aura_result = (const char *)__copy; aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
                );
            } else if is_array_type_key(&spawn_return_key) {
                let clone = crate::names::c_method_name(&spawn_return_key, "clone");
                let _ = writeln!(
                    out,
                    "  {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {clone}(&{result_tmp}); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
                );
            } else if crate::expr::mono_split(&spawn_return_key, checked)
                .is_some_and(|(base, _)| checked.ast.enums.iter().any(|e| e.name.name == base))
            {
                let _ = writeln!(
                    out,
                    "  {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_cty}_clone(&{result_tmp}); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
                );
            } else if is_iface_type_key(&spawn_return_key, checked) {
                let _ = writeln!(
                    out,
                    "  {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_cty}_clone(&{result_tmp}); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
                );
            } else if is_fun_type_key(&spawn_return_key) {
                let _ = writeln!(
                    out,
                    "  {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_tmp}; if (__aura_result->env != NULL) aura_fun_env_retain(__aura_result->env); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
                );
            } else {
                let _ = writeln!(
                    out,
                    "  {result_cty} *__aura_result = ({result_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {result_tmp}; aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
                );
            }
            let _ = writeln!(out, "}}");
            let _ = writeln!(
                out,
                "static void {result_destroy}(void *data, size_t size) {{"
            );
            if spawn_return_key == "Task"
                || spawn_return_key.starts_with("Task_")
                || spawn_return_key == "TaskHandle"
                || spawn_return_key.starts_with("TaskHandle_")
            {
                out.push_str("  (void)size; if (data != NULL) { AuraTaskFrame **result = (AuraTaskFrame **)data; if (*result != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release_payload(__aura_task_executor, result); free(data); }\n");
            } else if spawn_return_key == "Channel" || spawn_return_key.starts_with("Channel_") {
                out.push_str("  (void)size; if (data != NULL) { AuraTaskChannel **result = (AuraTaskChannel **)data; if (*result != NULL) aura_task_channel_destroy(*result); free(data); }\n");
            } else if spawn_return_key == "String" {
                out.push_str("  (void)size; if (data != NULL) { const char **result = (const char **)data; free((void *)*result); free(result); }\n");
            } else if is_array_type_key(&spawn_return_key) {
                let drop = format!("{result_cty}_drop");
                let _ = writeln!(
                    out,
                    "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {drop}(result); free(result); }}"
                );
            } else if crate::stmt::is_shared_outcome_error_owner_key(&spawn_return_key) {
                let _ = writeln!(
                    out,
                    "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; if (result->tag == 0 && result->data.OutcomeOk.owned && result->data.OutcomeOk.value != NULL) free((void *)result->data.OutcomeOk.value); if (result->tag == 1 && result->data.OutcomeErr.owned && result->data.OutcomeErr.error != NULL) aura_gc_remove_root((void **)&result->data.OutcomeErr.error); free(result); }}"
                );
            } else if crate::expr::mono_split(&spawn_return_key, checked)
                .is_some_and(|(base, _)| checked.ast.enums.iter().any(|e| e.name.name == base))
            {
                let _ = writeln!(
                    out,
                    "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {result_cty}_drop(result); free(result); }}"
                );
            } else if is_iface_type_key(&spawn_return_key, checked) {
                let _ = writeln!(
                    out,
                    "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {result_cty}_drop(result); free(result); }}"
                );
            } else if crate::expr::is_value_struct_mono(&spawn_return_key, checked) {
                let _ = writeln!(
                    out,
                    "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {result_cty}_drop(result); free(result); }}"
                );
            } else if is_fun_type_key(&spawn_return_key) {
                let _ = writeln!(
                    out,
                    "  (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; if (result->env != NULL) aura_fun_env_free(result->env); free(result); }}"
                );
            } else {
                out.push_str("  (void)size; free(data);\n");
            }
            out.push_str("}\n\n");
            continue;
        }
        let array_owners = ctx.array_owners_all();
        crate::stmt::emit_free_array_owners(out, 1, &ctx, &array_owners);
        let fun_owners = ctx.fun_owners_all();
        crate::stmt::emit_free_fun_owners(out, 1, &ctx, &fun_owners);
        out.push_str("  return AURA_TASK_COMPLETE;\n}\n\n");
    }
}

fn bounded_spawn_array_return_key(body: &Block, checked: &CheckedFile) -> Option<String> {
    let Some(Stmt::Return(ReturnStmt {
        value: Some(Expr::Call(call)),
        ..
    })) = body.stmts.last()
    else {
        return None;
    };
    let Expr::Ident(callee) = call.callee.as_ref() else {
        return None;
    };
    if callee.name != "Array" || call.type_args.len() != 1 {
        return None;
    }
    let elem = type_ref_local_key_expand(&call.type_args[0], &[], &[], checked);
    Some(format!("Array_{elem}"))
}

fn bounded_spawn_class_return_key(body: &Block, checked: &CheckedFile) -> Option<String> {
    let Some(Stmt::Return(ReturnStmt {
        value: Some(Expr::Call(call)),
        ..
    })) = body.stmts.last()
    else {
        return None;
    };
    let Expr::Ident(callee) = call.callee.as_ref() else {
        return None;
    };
    let inst = checked.call_instantiations.get(&call.span.start)?;
    if !inst.is_constructor
        || inst.name == "Array"
        || !checked
            .ast
            .classes
            .iter()
            .any(|class| class.name.name == callee.name && class.kind == NominalKind::Class)
    {
        return None;
    }
    Some(type_mono(&inst.package, &inst.name, &inst.type_args))
}

#[allow(clippy::too_many_arguments)]
fn emit_bounded_spawn_array_return_poller(
    out: &mut String,
    body: &Block,
    captures: &[BoundedSpawnCapture],
    checked: &CheckedFile,
    detector: bool,
    names: (&str, &str),
    span: Span,
    array_key: &str,
) {
    let (data_ty, poll) = names;
    let result_destroy = format!("aura_spawn_result_destroy_{}", span.start);
    let array_cty = crate::stmt::local_key_to_c(array_key, checked);
    let _ = writeln!(
        out,
        "static void {result_destroy}(void *data, size_t size) {{"
    );
    let _ = writeln!(out, "  (void)size;");
    let _ = writeln!(out, "  if (data != NULL) {{");
    let _ = writeln!(out, "    {array_cty} *result = ({array_cty} *)data;");
    crate::array_emit::emit_array_contents_free(out, 4, "(*result)", array_key);
    out.push_str("    free(result);\n  }\n}\n\n");
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    if !captures.is_empty() {
        let _ = writeln!(
            out,
            "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
        );
    }
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  if (aura_task_frame_resume_state(frame) != 0) return AURA_TASK_COMPLETE;\n");
    out.push_str("  aura_task_frame_set_resume_state(frame, 1);\n");
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: None,
        type_params: Vec::new(),
        type_args: Vec::new(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![HashSet::new()],
        fun_owners: vec![HashSet::new()],
        string_owners: vec![HashSet::new()],
        channel_owners: vec![HashSet::new()],
        task_result_owners: vec![HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![HashSet::new()],
        box_owners: vec![HashSet::new()],
        gc_roots: vec![HashSet::new()],
        array_gc_roots: vec![HashSet::new()],
        return_key: Some(array_key.to_string()),
        lambda_ids: build_lambda_ids(checked),
        spawn_params: HashSet::new(),
        mutable_spawn_captures: HashSet::new(),
        async_frame: None,
        task_poller: true,
    };
    for capture in captures {
        let name = &capture.name;
        let key = &capture.key;
        let n = mangle_ident(name);
        let cty = if capture.boxed {
            match bounded_capture_box_kind(capture) {
                "string" => "aura_box_str *".to_string(),
                "i64" => "aura_box_i64 *".to_string(),
                "bool" => "aura_box_bool *".to_string(),
                _ => "aura_box_ptr *".to_string(),
            }
        } else if key == "String" {
            "aura_box_str *".to_string()
        } else {
            crate::stmt::local_key_to_c(key, checked)
        };
        let _ = writeln!(out, "  {cty}{n} = data->{n};");
        ctx.define_local(name, key.clone());
        if capture.boxed {
            ctx.mark_box_local(name);
        }
    }
    let Some(Stmt::Return(ReturnStmt {
        value: Some(value), ..
    })) = body.stmts.last()
    else {
        unreachable!()
    };
    for stmt in &body.stmts[..body.stmts.len() - 1] {
        crate::stmt::emit_stmt(out, stmt, 1, &mut ctx);
    }
    let value_code = emit_expr(value, &mut ctx);
    let _ = writeln!(
        out,
        "  {array_cty} *result = ({array_cty} *)malloc(sizeof(*result));"
    );
    out.push_str("  if (result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(out, "  *result = {value_code};");
    let _ = writeln!(
        out,
        "  aura_task_frame_set_result(frame, result, sizeof(*result), {result_destroy});"
    );
    out.push_str("  return AURA_TASK_COMPLETE;\n}\n\n");
}

#[allow(clippy::too_many_arguments)]
fn emit_bounded_spawn_class_return_poller(
    out: &mut String,
    body: &Block,
    captures: &[BoundedSpawnCapture],
    checked: &CheckedFile,
    detector: bool,
    names: (&str, &str),
    span: Span,
    class_key: &str,
) {
    let (data_ty, poll) = names;
    let result_destroy = format!("aura_spawn_result_destroy_{}", span.start);
    let class_cty = crate::stmt::local_key_to_c(class_key, checked);
    let _ = writeln!(
        out,
        "static void {result_destroy}(void *data, size_t size) {{"
    );
    let _ = writeln!(out, "  (void)size;");
    let _ = writeln!(out, "  if (data != NULL) {{");
    let _ = writeln!(out, "    {class_cty} *result = ({class_cty} *)data;");
    out.push_str("    aura_gc_remove_root((void **)result);\n    free(result);\n  }\n}\n\n");
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    if !captures.is_empty() {
        let _ = writeln!(
            out,
            "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
        );
    }
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  if (aura_task_frame_resume_state(frame) != 0) return AURA_TASK_COMPLETE;\n");
    out.push_str("  aura_task_frame_set_resume_state(frame, 1);\n");
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: None,
        type_params: Vec::new(),
        type_args: Vec::new(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![HashSet::new()],
        fun_owners: vec![HashSet::new()],
        string_owners: vec![HashSet::new()],
        channel_owners: vec![HashSet::new()],
        task_result_owners: vec![HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![HashSet::new()],
        box_owners: vec![HashSet::new()],
        gc_roots: vec![HashSet::new()],
        array_gc_roots: vec![HashSet::new()],
        return_key: Some(class_key.to_string()),
        lambda_ids: build_lambda_ids(checked),
        spawn_params: HashSet::new(),
        mutable_spawn_captures: HashSet::new(),
        async_frame: None,
        task_poller: true,
    };
    for capture in captures {
        let name = &capture.name;
        let key = &capture.key;
        let n = mangle_ident(name);
        let cty = if capture.boxed {
            match bounded_capture_box_kind(capture) {
                "string" => "aura_box_str *".to_string(),
                "i64" => "aura_box_i64 *".to_string(),
                "bool" => "aura_box_bool *".to_string(),
                _ => "aura_box_ptr *".to_string(),
            }
        } else if key == "String" {
            "aura_box_str *".to_string()
        } else {
            crate::stmt::local_key_to_c(key, checked)
        };
        let _ = writeln!(out, "  {cty}{n} = data->{n};");
        ctx.define_local(name, key.clone());
        if capture.boxed {
            ctx.mark_box_local(name);
        }
    }
    let Some(Stmt::Return(ReturnStmt {
        value: Some(value), ..
    })) = body.stmts.last()
    else {
        unreachable!()
    };
    for stmt in &body.stmts[..body.stmts.len() - 1] {
        crate::stmt::emit_stmt(out, stmt, 1, &mut ctx);
    }
    let value_code = emit_expr(value, &mut ctx);
    let _ = writeln!(
        out,
        "  {class_cty} *result = ({class_cty} *)malloc(sizeof(*result));"
    );
    out.push_str("  if (result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(out, "  *result = {value_code};");
    out.push_str("  aura_gc_add_root((void **)result);\n");
    let _ = writeln!(
        out,
        "  aura_task_frame_set_result(frame, result, sizeof(*result), {result_destroy});"
    );
    out.push_str("  return AURA_TASK_COMPLETE;\n}\n\n");
}

fn bounded_spawn_foreign_handle_return_key(body: &Block, checked: &CheckedFile) -> Option<String> {
    let Some(Stmt::Return(ReturnStmt {
        value: Some(value), ..
    })) = body.stmts.last()
    else {
        return None;
    };
    let key = infer_type_name(
        value,
        &EmitCtx {
            checked,
            detector: false,
            method_class: None,
            type_params: Vec::new(),
            type_args: Vec::new(),
            locals: vec![HashMap::new()],
            local_c_names: HashMap::new(),
            array_owners: vec![HashSet::new()],
            fun_owners: vec![HashSet::new()],
            string_owners: vec![HashSet::new()],
            channel_owners: vec![HashSet::new()],
            task_result_owners: vec![HashSet::new()],
            task_handle_owners: vec![HashSet::new()],
            box_locals: vec![HashSet::new()],
            box_owners: vec![HashSet::new()],
            gc_roots: vec![HashSet::new()],
            array_gc_roots: vec![HashSet::new()],
            return_key: None,
            lambda_ids: HashMap::new(),
            spawn_params: HashSet::new(),
            mutable_spawn_captures: HashSet::new(),
            async_frame: None,
            task_poller: true,
        },
    );
    key.starts_with("ForeignHandle_").then_some(key)
}

#[allow(clippy::too_many_arguments)]
fn emit_bounded_spawn_foreign_handle_return_poller(
    out: &mut String,
    body: &Block,
    captures: &[BoundedSpawnCapture],
    checked: &CheckedFile,
    detector: bool,
    names: (&str, &str),
    span: Span,
    handle_key: &str,
) {
    let (data_ty, poll) = names;
    let result_destroy = format!("aura_spawn_result_destroy_{}", span.start);
    let handle_cty = crate::stmt::local_key_to_c(handle_key, checked);
    let _ = writeln!(
        out,
        "static void {result_destroy}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ AuraFfiOpaqueHandle **result = (AuraFfiOpaqueHandle **)data; if (*result != NULL) (void)aura_ffi_handle_drop(result); free(result); }} }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    if !captures.is_empty() {
        let _ = writeln!(
            out,
            "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
        );
    }
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  if (aura_task_frame_resume_state(frame) != 0) return AURA_TASK_COMPLETE;\n");
    out.push_str("  aura_task_frame_set_resume_state(frame, 1);\n");
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: None,
        type_params: Vec::new(),
        type_args: Vec::new(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![HashSet::new()],
        fun_owners: vec![HashSet::new()],
        string_owners: vec![HashSet::new()],
        channel_owners: vec![HashSet::new()],
        task_result_owners: vec![HashSet::new()],
        task_handle_owners: vec![HashSet::new()],
        box_locals: vec![HashSet::new()],
        box_owners: vec![HashSet::new()],
        gc_roots: vec![HashSet::new()],
        array_gc_roots: vec![HashSet::new()],
        return_key: Some(handle_key.to_string()),
        lambda_ids: build_lambda_ids(checked),
        spawn_params: HashSet::new(),
        mutable_spawn_captures: HashSet::new(),
        async_frame: None,
        task_poller: true,
    };
    for capture in captures {
        let name = &capture.name;
        let key = &capture.key;
        let n = mangle_ident(name);
        let cty = if capture.boxed {
            match bounded_capture_box_kind(capture) {
                "string" => "aura_box_str *".to_string(),
                "i64" => "aura_box_i64 *".to_string(),
                "bool" => "aura_box_bool *".to_string(),
                _ => "aura_box_ptr *".to_string(),
            }
        } else if key == "String" {
            "aura_box_str *".to_string()
        } else {
            crate::stmt::local_key_to_c(key, checked)
        };
        let _ = writeln!(out, "  {cty}{n} = data->{n};");
        ctx.define_local(name, key.clone());
        if capture.boxed {
            ctx.mark_box_local(name);
        }
    }
    let Some(Stmt::Return(ReturnStmt {
        value: Some(value), ..
    })) = body.stmts.last()
    else {
        unreachable!()
    };
    for stmt in &body.stmts[..body.stmts.len() - 1] {
        crate::stmt::emit_stmt(out, stmt, 1, &mut ctx);
    }
    let value_code = emit_expr(value, &mut ctx);
    let _ = writeln!(
        out,
        "  {handle_cty} *result = ({handle_cty} *)malloc(sizeof(*result));"
    );
    out.push_str("  if (result == NULL) return AURA_TASK_FAILED;\n");
    let _ = writeln!(out, "  *result = {value_code};");
    out.push_str("  if (*result == NULL) { free(result); return AURA_TASK_FAILED; }\n");
    let _ = writeln!(
        out,
        "  aura_task_frame_set_result(frame, result, sizeof(*result), {result_destroy}); return AURA_TASK_COMPLETE;\n}}\n\n"
    );
}

fn emit_bounded_spawn_string_return_poller(
    out: &mut String,
    body: &Block,
    captures: &[BoundedSpawnCapture],
    checked: &CheckedFile,
    detector: bool,
    names: (&str, &str),
    span: Span,
) {
    let (data_ty, poll) = names;
    let destroy_result = format!("aura_spawn_result_destroy_{}", span.start);
    let _ = writeln!(
        out,
        "static void {destroy_result}(void *data, size_t size) {{ (void)size; free(data); }}\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    if !captures.is_empty() {
        let _ = writeln!(
            out,
            "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
        );
    }
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n  if (aura_task_frame_resume_state(frame) != 0) return AURA_TASK_COMPLETE;\n  aura_task_frame_set_resume_state(frame, 1);\n");
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: None,
        type_params: Vec::new(),
        type_args: Vec::new(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![HashSet::new()],
        fun_owners: vec![HashSet::new()],
        string_owners: vec![HashSet::new()],
        channel_owners: vec![HashSet::new()],
        task_result_owners: vec![HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![HashSet::new()],
        box_owners: vec![HashSet::new()],
        gc_roots: vec![HashSet::new()],
        array_gc_roots: vec![HashSet::new()],
        return_key: Some("String".into()),
        lambda_ids: build_lambda_ids(checked),
        spawn_params: HashSet::new(),
        mutable_spawn_captures: HashSet::new(),
        async_frame: None,
        task_poller: true,
    };
    for capture in captures {
        let name = &capture.name;
        let key = &capture.key;
        let n = mangle_ident(name);
        if capture.boxed {
            let cty = match bounded_capture_box_kind(capture) {
                "string" => "aura_box_str *",
                "i64" => "aura_box_i64 *",
                "bool" => "aura_box_bool *",
                _ => "aura_box_ptr *",
            };
            let _ = writeln!(out, "  {cty}{n} = data->{n};");
            ctx.mark_box_local(name);
        } else if key == "String" {
            let _ = writeln!(
                out,
                "  const char *{n} = data->{n} != NULL ? data->{n}->value : NULL;"
            );
        } else {
            let _ = writeln!(
                out,
                "  {} {n} = data->{n};",
                crate::stmt::local_key_to_c(key, checked)
            );
        }
        ctx.define_local(name, key.clone());
    }
    let Some(Stmt::Return(ReturnStmt {
        value: Some(value),
        span: return_span,
    })) = body.stmts.last()
    else {
        unreachable!()
    };
    for stmt in &body.stmts[..body.stmts.len() - 1] {
        crate::stmt::emit_stmt(out, stmt, 1, &mut ctx);
    }
    let value_code = owned_string_copy_expr(emit_expr(value, &mut ctx), *return_span);
    let _ = writeln!(
        out,
        "  const char *result = {value_code}; if (result == NULL) return AURA_TASK_FAILED; aura_task_frame_set_result(frame, (void *)result, strlen(result) + 1, {destroy_result}); return AURA_TASK_COMPLETE;\n}}\n\n"
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_bounded_spawn_await_poller(
    out: &mut String,
    body: &Block,
    await_var: &VarStmt,
    await_expr: &AwaitExpr,
    captures: &[BoundedSpawnCapture],
    checked: &CheckedFile,
    detector: bool,
    names: (&str, &str),
    span: Span,
) {
    let (data_ty, poll) = names;
    let result_destroy = format!("aura_spawn_result_destroy_{}", span.start);
    let _ = writeln!(
        out,
        "static void {result_destroy}(void *data, size_t size);\n"
    );
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    let mut initial_ctx = EmitCtx {
        checked,
        detector,
        method_class: None,
        type_params: Vec::new(),
        type_args: Vec::new(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![HashSet::new()],
        fun_owners: vec![HashSet::new()],
        string_owners: vec![HashSet::new()],
        channel_owners: vec![HashSet::new()],
        task_result_owners: vec![HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![HashSet::new()],
        box_owners: vec![HashSet::new()],
        gc_roots: vec![HashSet::new()],
        array_gc_roots: vec![HashSet::new()],
        return_key: Some("Unit".into()),
        lambda_ids: build_lambda_ids(checked),
        spawn_params: HashSet::new(),
        mutable_spawn_captures: HashSet::new(),
        async_frame: None,
        task_poller: false,
    };
    for capture in captures {
        let name = &capture.name;
        let key = &capture.key;
        let n = mangle_ident(name);
        if capture.boxed {
            let cty = match bounded_capture_box_kind(capture) {
                "string" => "aura_box_str *",
                "i64" => "aura_box_i64 *",
                "bool" => "aura_box_bool *",
                _ => "aura_box_ptr *",
            };
            let _ = writeln!(out, "      {cty}{n} = data->{n};");
            initial_ctx.mark_box_local(name);
        } else if key == "String" {
            let _ = writeln!(
                out,
                "      const char *{n} = data->{n} != NULL ? data->{n}->value : NULL;"
            );
        } else if is_array_type_key(key) {
            let _ = writeln!(
                out,
                "      {} {n} = {}(&data->{n});",
                crate::stmt::local_key_to_c(key, checked),
                crate::names::c_method_name(key, "clone")
            );
        } else {
            let _ = writeln!(
                out,
                "      {} {n} = data->{n};",
                crate::stmt::local_key_to_c(key, checked)
            );
        }
        initial_ctx.define_local(name, key.clone());
    }
    let task = emit_expr(&await_expr.operand, &mut initial_ctx);
    let await_is_temporary = await_operand_is_temporary(&await_expr.operand, checked);
    let _ = writeln!(out, "      data->await_task = {task};");
    let _ = writeln!(
        out,
        "      data->await_task_owned = {};",
        if await_is_temporary { "true" } else { "false" }
    );
    out.push_str("      if (data->await_task == NULL) return AURA_TASK_FAILED;\n");
    out.push_str("      aura_task_frame_set_resume_state(frame, 1);\n    }\n    case 1: {\n");
    out.push_str(
        "      AuraTaskPollState child_state = aura_task_frame_state(data->await_task);\n",
    );
    out.push_str("      if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n");
    out.push_str("      if (child_state == AURA_TASK_FAILED) { (void)aura_task_frame_propagate_error(frame, data->await_task); return AURA_TASK_FAILED; }\n");
    out.push_str("      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str("      AuraTaskResult child_result = aura_task_frame_result(data->await_task);\n");
    let await_key = await_var
        .ty
        .as_ref()
        .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked))
        .map(|key| full_type_mono(&key, checked))
        .unwrap_or_else(|| "Int".into());
    let await_cty = crate::stmt::local_key_to_c(&await_key, checked);
    let typed_return_key = if matches!(
        body.stmts.last(),
        Some(Stmt::Return(ReturnStmt {
            value: Some(Expr::Ident(id)),
            ..
        })) if id.name == await_var.name.name
    ) && (matches!(
        await_key.as_str(),
        "Int" | "Bool" | "String" | "Opt_Int" | "Opt_Bool"
    ) || is_array_type_key(&await_key)
        || is_heap_class_mono(&await_key, checked)
        || crate::expr::is_enum_mono(&await_key, checked)
        || crate::expr::is_value_struct_mono(&await_key, checked))
    {
        Some(await_key.as_str())
    } else {
        None
    };
    if let Some(key) = typed_return_key {
        let _ = writeln!(
            out,
            "      /* typed suspended {key} result is copied into spawn-owned storage */"
        );
    }
    if is_array_type_key(&await_key) {
        let clone = crate::names::c_method_name(&await_key, "clone");
        let mut free_old = String::new();
        crate::array_emit::emit_array_contents_free(
            &mut free_old,
            0,
            "data->await_value",
            &await_key,
        );
        let _ = writeln!(
            out,
            "      if (child_result.data != NULL) {{ {await_cty} *__child = ({await_cty} *)child_result.data; {free_old} data->await_value = {clone}(__child); }}"
        );
    } else if crate::expr::is_enum_mono(&await_key, checked) {
        let _ = writeln!(
            out,
            "      if (child_result.data != NULL) {{ {await_cty} *__child = ({await_cty} *)child_result.data; {await_cty}_drop(&data->await_value); data->await_value = {await_cty}_clone(__child); }}"
        );
    } else if crate::expr::is_value_struct_mono(&await_key, checked) {
        let _ = writeln!(
            out,
            "      if (child_result.data != NULL) {{ {await_cty} *__child = ({await_cty} *)child_result.data; {await_cty}_drop(&data->await_value); data->await_value = {await_cty}_clone(__child); }}"
        );
    } else {
        let _ = writeln!(
            out,
            "      if (child_result.data != NULL) data->await_value = *(({await_cty} *)child_result.data);"
        );
    }
    if await_is_temporary {
        out.push_str("      if (data->await_task != NULL && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task);\n");
        out.push_str("      data->await_task = NULL;\n");
        out.push_str("      data->await_task_owned = false;\n");
    }

    let body_tail = if typed_return_key.is_some() {
        &body.stmts[1..body.stmts.len() - 1]
    } else {
        &body.stmts[1..]
    };
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: None,
        type_params: Vec::new(),
        type_args: Vec::new(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![HashSet::new()],
        fun_owners: vec![HashSet::new()],
        string_owners: vec![HashSet::new()],
        channel_owners: vec![HashSet::new()],
        task_result_owners: vec![HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![HashSet::new()],
        box_owners: vec![HashSet::new()],
        gc_roots: vec![HashSet::new()],
        array_gc_roots: vec![HashSet::new()],
        return_key: Some(typed_return_key.unwrap_or("Unit").into()),
        lambda_ids: build_lambda_ids(checked),
        spawn_params: HashSet::new(),
        mutable_spawn_captures: HashSet::new(),
        async_frame: None,
        task_poller: true,
    };
    if !body_tail.is_empty() {
        for capture in captures {
            let name = &capture.name;
            let key = &capture.key;
            let n = mangle_ident(name);
            if capture.boxed {
                let cty = match bounded_capture_box_kind(capture) {
                    "string" => "aura_box_str *",
                    "i64" => "aura_box_i64 *",
                    "bool" => "aura_box_bool *",
                    _ => "aura_box_ptr *",
                };
                let _ = writeln!(out, "      {cty}{n} = data->{n};");
                ctx.mark_box_local(name);
            } else if key == "String" {
                let _ = writeln!(
                    out,
                    "      const char *{n} = data->{n} != NULL ? data->{n}->value : NULL;"
                );
            } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
                let _ = writeln!(out, "      AuraFfiOpaqueHandle *{n} = data->{n};");
            } else if is_array_type_key(key) {
                let _ = writeln!(
                    out,
                    "      {} {n} = {}(&data->{n});",
                    crate::stmt::local_key_to_c(key, checked),
                    crate::names::c_method_name(key, "clone")
                );
                ctx.mark_array_owner(name);
            } else if is_fun_type_key(key) {
                let _ = writeln!(
                    out,
                    "      {} {n} = data->{n}; if ({n}.env != NULL) aura_fun_env_retain({n}.env);",
                    crate::stmt::local_key_to_c(key, checked)
                );
                ctx.mark_fun_owner(name);
            } else if crate::expr::is_enum_mono(key, checked) {
                let cty = crate::stmt::local_key_to_c(key, checked);
                let _ = writeln!(
                    out,
                    "      {} {n} = {cty}_clone(&data->{n});",
                    crate::stmt::local_key_to_c(key, checked)
                );
            } else {
                let _ = writeln!(
                    out,
                    "      {} {n} = data->{n};",
                    crate::stmt::local_key_to_c(key, checked)
                );
            }
            ctx.define_local(name, key.clone());
        }
    }
    ctx.define_local(&await_var.name.name, await_key.clone());
    let _ = writeln!(
        out,
        "      {await_cty} {} = data->await_value;",
        mangle_ident(&await_var.name.name)
    );
    for stmt in body_tail {
        crate::stmt::emit_stmt(out, stmt, 3, &mut ctx);
    }
    if let Some(key) = typed_return_key {
        if key == "String" {
            let _ = writeln!(
                out,
                "      const char *__aura_result_text = {}; if (__aura_result_text == NULL) return AURA_TASK_FAILED; aura_task_frame_set_result(frame, (void *)__aura_result_text, strlen(__aura_result_text) + 1, {result_destroy}); return AURA_TASK_COMPLETE;",
                owned_string_copy_expr("data->await_value".into(), await_expr.span)
            );
        } else if is_array_type_key(key) {
            let clone = crate::names::c_method_name(key, "clone");
            let mut free_await_value = String::new();
            crate::array_emit::emit_array_contents_free(
                &mut free_await_value,
                0,
                "data->await_value",
                key,
            );
            let _ = writeln!(
                out,
                "      {await_cty} *__aura_result = ({await_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {clone}(&data->await_value); {free_await_value} memset(&data->await_value, 0, sizeof(data->await_value)); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
            );
        } else if is_heap_class_mono(key, checked) {
            let _ = writeln!(
                out,
                "      {await_cty} *__aura_result = ({await_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = data->await_value; aura_gc_add_root((void **)__aura_result); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
            );
        } else if crate::expr::is_enum_mono(key, checked) {
            let _ = writeln!(
                out,
                "      {await_cty} *__aura_result = ({await_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = {await_cty}_clone(&data->await_value); {await_cty}_drop(&data->await_value); memset(&data->await_value, 0, sizeof(data->await_value)); aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
            );
        } else if crate::expr::is_value_struct_mono(key, checked) {
            let _ = writeln!(
                out,
                "      {await_cty} *result = ({await_cty} *)malloc(sizeof(*result)); if (result == NULL) return AURA_TASK_FAILED; *result = {await_cty}_clone(&data->await_value); {await_cty}_drop(&data->await_value); memset(&data->await_value, 0, sizeof(data->await_value)); aura_task_frame_set_result(frame, result, sizeof(*result), {result_destroy}); return AURA_TASK_COMPLETE;"
            );
        } else {
            let _ = writeln!(
                out,
                "      {await_cty} *__aura_result = ({await_cty} *)malloc(sizeof(*__aura_result)); if (__aura_result == NULL) return AURA_TASK_FAILED; *__aura_result = data->await_value; aura_task_frame_set_result(frame, __aura_result, sizeof(*__aura_result), {result_destroy}); return AURA_TASK_COMPLETE;"
            );
        }
        out.push_str("    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
        if is_array_type_key(key) {
            let result_cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(
                out,
                "static void {result_destroy}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data;"
            );
            crate::array_emit::emit_array_contents_free(out, 2, "(*result)", key);
            out.push_str(" free(result); } }\n\n");
        } else if is_heap_class_mono(key, checked) {
            let _ = writeln!(
                out,
                "static void {result_destroy}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ aura_gc_remove_root((void **)data); free(data); }} }}\n\n"
            );
        } else if crate::expr::is_enum_mono(key, checked) {
            let result_cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(
                out,
                "static void {result_destroy}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {result_cty}_drop(result); free(result); }} }}\n\n"
            );
        } else if crate::expr::is_value_struct_mono(key, checked) {
            let result_cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(
                out,
                "static void {result_destroy}(void *data, size_t size) {{ (void)size; if (data != NULL) {{ {result_cty} *result = ({result_cty} *)data; {result_cty}_drop(result); free(result); }} }}\n\n"
            );
        } else {
            let _ = writeln!(
                out,
                "static void {result_destroy}(void *data, size_t size) {{ (void)size; free(data); }}\n\n"
            );
        }
        return;
    }
    let array_owners = ctx.array_owners_all();
    crate::stmt::emit_free_array_owners(out, 3, &ctx, &array_owners);
    let fun_owners = ctx.fun_owners_all();
    crate::stmt::emit_free_fun_owners(out, 3, &ctx, &fun_owners);
    out.push_str("      return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
}

/// Lower `spawn { await unit_task(); ... }`.  This shape has no value slot,
/// but it still needs the same child outcome propagation as value awaits.
#[allow(clippy::too_many_arguments)]
fn emit_bounded_spawn_discard_await_poller(
    out: &mut String,
    body: &Block,
    captures: &[BoundedSpawnCapture],
    checked: &CheckedFile,
    detector: bool,
    names: (&str, &str),
    _span: Span,
) {
    let (data_ty, poll) = names;
    let _ = writeln!(
        out,
        "static AuraTaskPollState {poll}(AuraTaskFrame *frame) {{"
    );
    let _ = writeln!(
        out,
        "  {data_ty} *data = ({data_ty} *)aura_task_frame_data(frame);"
    );
    out.push_str("  if (aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;\n");
    out.push_str("  switch (aura_task_frame_resume_state(frame)) {\n    case 0: {\n");
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: None,
        type_params: Vec::new(),
        type_args: Vec::new(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![HashSet::new()],
        fun_owners: vec![HashSet::new()],
        string_owners: vec![HashSet::new()],
        channel_owners: vec![HashSet::new()],
        task_result_owners: vec![HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![HashSet::new()],
        box_owners: vec![HashSet::new()],
        gc_roots: vec![HashSet::new()],
        array_gc_roots: vec![HashSet::new()],
        return_key: Some("Unit".into()),
        lambda_ids: build_lambda_ids(checked),
        spawn_params: HashSet::new(),
        mutable_spawn_captures: HashSet::new(),
        async_frame: None,
        task_poller: true,
    };
    for capture in captures {
        let n = mangle_ident(&capture.name);
        let key = &capture.key;
        if capture.boxed {
            let cty = match bounded_capture_box_kind(capture) {
                "string" => "aura_box_str *",
                "i64" => "aura_box_i64 *",
                "bool" => "aura_box_bool *",
                _ => "aura_box_ptr *",
            };
            let _ = writeln!(out, "      {cty}{n} = data->{n};");
            ctx.mark_box_local(&capture.name);
        } else if key == "String" {
            let _ = writeln!(
                out,
                "      const char *{n} = data->{n} != NULL ? data->{n}->value : NULL;"
            );
        } else {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(out, "      {cty}{n} = data->{n};");
        }
        ctx.define_local(&capture.name, key.clone());
    }
    let spawn_return_key = crate::expr::spawn_result_key(body, &ctx);
    ctx.return_key = Some(spawn_return_key);
    let task = match &body.stmts[0] {
        Stmt::Expr(Expr::Async(AsyncExpr::Await(await_expr))) => {
            emit_expr(&await_expr.operand, &mut ctx)
        }
        _ => unreachable!("validated discarded await shape"),
    };
    let _ = writeln!(
        out,
        "      data->await_task = {task}; data->await_task_owned = true;"
    );
    out.push_str("      if (data->await_task == NULL) return AURA_TASK_FAILED;\n      aura_task_frame_set_resume_state(frame, 1);\n    }\n    case 1: {\n");
    out.push_str("      AuraTaskPollState child_state = aura_task_frame_state(data->await_task); if (child_state == AURA_TASK_READY) child_state = aura_task_executor_poll_inline(__aura_task_executor, data->await_task);\n");
    out.push_str("      if (child_state == AURA_TASK_PENDING) { if (!aura_task_frame_wait_on(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_PENDING; }\n");
    out.push_str("      if (child_state == AURA_TASK_CANCELLED) return AURA_TASK_CANCELLED;\n      if (child_state == AURA_TASK_FAILED) { if (!aura_task_frame_propagate_error(frame, data->await_task)) return AURA_TASK_FAILED; return AURA_TASK_FAILED; }\n      if (child_state != AURA_TASK_COMPLETE) return AURA_TASK_FAILED;\n");
    out.push_str("      if (data->await_task_owned && __aura_task_executor != NULL) (void)aura_task_executor_release(__aura_task_executor, &data->await_task); data->await_task = NULL; data->await_task_owned = false;\n");
    // The first poll and the post-await continuation are separate C scopes.
    // Re-materialize captures in the continuation so references such as
    // `cancel(handle)` use the frame-owned value after suspension.
    for capture in captures {
        let n = mangle_ident(&capture.name);
        let key = &capture.key;
        if capture.boxed {
            let cty = match bounded_capture_box_kind(capture) {
                "string" => "aura_box_str *",
                "i64" => "aura_box_i64 *",
                "bool" => "aura_box_bool *",
                _ => "aura_box_ptr *",
            };
            let _ = writeln!(out, "      {cty}{n} = data->{n};");
            ctx.mark_box_local(&capture.name);
        } else if key == "String" {
            let _ = writeln!(
                out,
                "      const char *{n} = data->{n} != NULL ? data->{n}->value : NULL;"
            );
        } else if key == "ForeignHandle" || key.starts_with("ForeignHandle_") {
            let _ = writeln!(out, "      AuraFfiOpaqueHandle *{n} = data->{n};");
        } else if is_array_type_key(key) {
            let _ = writeln!(
                out,
                "      {} {n} = {}(&data->{n});",
                crate::stmt::local_key_to_c(key, checked),
                crate::names::c_method_name(key, "clone")
            );
            ctx.mark_array_owner(&capture.name);
        } else if is_fun_type_key(key) {
            let cty = crate::stmt::local_key_to_c(key, checked);
            let _ = writeln!(
                out,
                "      {cty} {n} = data->{n}; if ({n}.env != NULL) aura_fun_env_retain({n}.env);"
            );
            ctx.mark_fun_owner(&capture.name);
        } else {
            let _ = writeln!(
                out,
                "      {} {n} = data->{n};",
                crate::stmt::local_key_to_c(key, checked)
            );
        }
        ctx.define_local(&capture.name, key.clone());
    }
    for stmt in &body.stmts[1..] {
        crate::stmt::emit_stmt(out, stmt, 3, &mut ctx);
    }
    out.push_str("      return AURA_TASK_COMPLETE;\n    }\n    default: return AURA_TASK_FAILED;\n  }\n}\n\n");
}

fn spawn_parameter_locals(
    params: &[Param],
    type_params: &[String],
    type_args: &[Ty],
    checked: &CheckedFile,
) -> HashMap<String, String> {
    params
        .iter()
        .map(|p| {
            let key = type_ref_local_key_expand(&p.ty, type_params, type_args, checked);
            (p.name.name.clone(), full_type_mono(&key, checked))
        })
        .collect()
}

/// Infer the small set of local types needed while collecting spawn frames.
/// Sema already checked the initializer; this fallback keeps unannotated
/// locals available to the frame layout pass instead of silently omitting them.
fn infer_spawn_local_key(expr: &Expr, checked: &CheckedFile) -> Option<String> {
    // Sema has already resolved every expression, including binary, field, and
    // conditional initializers. Reuse that result so frame discovery does not
    // silently drop unannotated locals whose initializer is not a simple call.
    let span = expr.span();
    if let Some(ty) = checked.expr_tys.get(&(span.start, span.end)) {
        return Some(full_type_mono(&ty.mono_suffix(), checked));
    }
    match expr {
        Expr::Int(_) => Some("Int".into()),
        Expr::Bool(_) => Some("Bool".into()),
        Expr::String(_) => Some("String".into()),
        Expr::Group(inner, _) => infer_spawn_local_key(inner, checked),
        Expr::Lambda(lambda) => checked
            .lambda_tys
            .get(&lambda.span.start)
            .map(Ty::mono_suffix),
        Expr::Call(call) => {
            let inst = checked.call_instantiations.get(&call.span.start)?;
            let args = inst.type_args.clone();
            if inst.is_constructor {
                if inst.name == "Array" && args.len() == 1 {
                    return Some(format!("Array_{}", args[0].mono_suffix()));
                }
                return Some(type_mono(&inst.package, &inst.name, &args));
            }
            if let Some(function) = checked
                .ast
                .functions
                .iter()
                .find(|function| function.name.name == inst.name)
            {
                let params = function
                    .type_params
                    .iter()
                    .map(|param| param.name.name.clone())
                    .collect::<Vec<_>>();
                return function
                    .return_type
                    .as_ref()
                    .map(|ty| type_ref_local_key_expand(ty, &params, &args, checked));
            }
            // Async function calls are represented by Task<T> at the source
            // level. Keep the same generic substitution here so a local such
            // as `val task = make<Int>()` remains visible to the spawn-frame
            // discovery pass even when semantic expression types are absent
            // from a host-provided CheckedFile.
            let function = checked
                .ast
                .async_functions
                .iter()
                .find(|function| function.name.name == inst.name)?;
            let params = function
                .type_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect::<Vec<_>>();
            function.return_type.as_ref().map(|ty| {
                format!(
                    "Task_{}",
                    full_type_mono(
                        &type_ref_local_key_expand(ty, &params, &args, checked),
                        checked
                    )
                )
            })
        }
        _ => None,
    }
}

fn collect_spawns_block<'a>(
    block: &'a Block,
    available: &HashMap<String, String>,
    checked: &CheckedFile,
    mutable_captures: &HashSet<String>,
    out: &mut Vec<(&'a SpawnExpr, HashMap<String, String>, HashSet<String>)>,
) {
    let mut available = available.clone();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Var(v) => {
                collect_spawns_expr(&v.init, &available, checked, mutable_captures, out);
                let key =
                    v.ty.as_ref()
                        .map(|ty| type_ref_local_key_expand(ty, &[], &[], checked))
                        .or_else(|| infer_spawn_local_key(&v.init, checked));
                if let Some(key) = key {
                    available.insert(v.name.name.clone(), full_type_mono(&key, checked));
                }
            }
            Stmt::If(i) => {
                collect_spawns_expr(&i.cond, &available, checked, mutable_captures, out);
                collect_spawns_block(&i.then_block, &available, checked, mutable_captures, out);
                if let Some(b) = &i.else_block {
                    collect_spawns_block(b, &available, checked, mutable_captures, out);
                }
            }
            Stmt::While(w) => {
                collect_spawns_expr(&w.cond, &available, checked, mutable_captures, out);
                collect_spawns_block(&w.body, &available, checked, mutable_captures, out);
            }
            Stmt::ForRange(f) => {
                collect_spawns_expr(&f.start, &available, checked, mutable_captures, out);
                collect_spawns_expr(&f.end, &available, checked, mutable_captures, out);
                collect_spawns_block(&f.body, &available, checked, mutable_captures, out);
            }
            Stmt::ForIn(f) => {
                collect_spawns_expr(&f.iterable, &available, checked, mutable_captures, out);
                collect_spawns_block(&f.body, &available, checked, mutable_captures, out);
            }
            Stmt::Match(m) => {
                collect_spawns_expr(&m.scrutinee, &available, checked, mutable_captures, out);
                for arm in &m.arms {
                    collect_spawns_block(&arm.body, &available, checked, mutable_captures, out);
                }
            }
            Stmt::Try(t) => {
                collect_spawns_block(&t.try_block, &available, checked, mutable_captures, out);
                if let Some(c) = &t.catch {
                    collect_spawns_block(&c.body, &available, checked, mutable_captures, out);
                }
                if let Some(f) = &t.finally {
                    collect_spawns_block(f, &available, checked, mutable_captures, out);
                }
            }
            Stmt::Throw(t) => {
                collect_spawns_expr(&t.value, &available, checked, mutable_captures, out)
            }
            Stmt::Return(r) => {
                if let Some(e) = &r.value {
                    collect_spawns_expr(e, &available, checked, mutable_captures, out);
                }
            }
            Stmt::Expr(e) => collect_spawns_expr(e, &available, checked, mutable_captures, out),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
}

fn collect_spawns_expr<'a>(
    expr: &'a Expr,
    available: &HashMap<String, String>,
    checked: &CheckedFile,
    mutable_captures: &HashSet<String>,
    out: &mut Vec<(&'a SpawnExpr, HashMap<String, String>, HashSet<String>)>,
) {
    match expr {
        Expr::Call(c) => {
            collect_spawns_expr(&c.callee, available, checked, mutable_captures, out);
            for arg in &c.args {
                collect_spawns_expr(arg, available, checked, mutable_captures, out);
            }
        }
        Expr::Field(f) => collect_spawns_expr(&f.object, available, checked, mutable_captures, out),
        Expr::Assign(a) => collect_spawns_expr(&a.value, available, checked, mutable_captures, out),
        Expr::Binary(b) => {
            collect_spawns_expr(&b.left, available, checked, mutable_captures, out);
            collect_spawns_expr(&b.right, available, checked, mutable_captures, out);
        }
        Expr::Unary(u) => collect_spawns_expr(&u.expr, available, checked, mutable_captures, out),
        Expr::ForceUnwrap(f) => {
            collect_spawns_expr(&f.expr, available, checked, mutable_captures, out)
        }
        Expr::Is(i) => collect_spawns_expr(&i.expr, available, checked, mutable_captures, out),
        Expr::Group(e, _) => collect_spawns_expr(e, available, checked, mutable_captures, out),
        Expr::If(i) => {
            collect_spawns_expr(&i.cond, available, checked, mutable_captures, out);
            collect_spawns_block(&i.then_block, available, checked, mutable_captures, out);
            collect_spawns_block(&i.else_block, available, checked, mutable_captures, out);
        }
        Expr::Lambda(l) => match &l.body {
            LambdaBody::Expr(e) => {
                collect_spawns_expr(e, available, checked, mutable_captures, out)
            }
            LambdaBody::Block(b) => {
                collect_spawns_block(b, available, checked, mutable_captures, out)
            }
        },
        Expr::Async(a) => match a {
            AsyncExpr::Spawn(s) => {
                out.push((s, available.clone(), mutable_captures.clone()));
                collect_spawns_block(&s.body, available, checked, mutable_captures, out);
            }
            AsyncExpr::Await(a) => {
                collect_spawns_expr(&a.operand, available, checked, mutable_captures, out)
            }
            AsyncExpr::Join(j) => {
                collect_spawns_expr(&j.handle, available, checked, mutable_captures, out)
            }
            AsyncExpr::Cancel(c) => {
                collect_spawns_expr(&c.handle, available, checked, mutable_captures, out)
            }
            AsyncExpr::ChannelCreate(c) => {
                collect_spawns_expr(&c.capacity, available, checked, mutable_captures, out)
            }
            AsyncExpr::ChannelSend(c) => {
                collect_spawns_expr(&c.channel, available, checked, mutable_captures, out);
                collect_spawns_expr(&c.value, available, checked, mutable_captures, out);
            }
            AsyncExpr::ChannelReceive(c) => {
                collect_spawns_expr(&c.channel, available, checked, mutable_captures, out)
            }
            AsyncExpr::ChannelClose(c) => {
                collect_spawns_expr(&c.channel, available, checked, mutable_captures, out)
            }
        },
        Expr::Ident(_)
        | Expr::This(_)
        | Expr::Int(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::Null(_) => {}
    }
}

/// Collect LambdaExpr nodes from the AST (for body emission).
fn collect_lambdas(file: &File) -> Vec<&LambdaExpr> {
    let mut out = Vec::new();
    for f in &file.functions {
        walk_block_lambdas(&f.body, &mut out);
    }
    for f in &file.async_functions {
        walk_block_lambdas(&f.body, &mut out);
    }
    for c in &file.classes {
        for m in &c.methods {
            walk_block_lambdas(&m.body, &mut out);
        }
    }
    for k in &file.consts {
        walk_expr_lambdas(&k.value, &mut out);
    }
    out
}

fn walk_block_lambdas<'a>(b: &'a Block, out: &mut Vec<&'a LambdaExpr>) {
    for s in &b.stmts {
        walk_stmt_lambdas(s, out);
    }
}

fn walk_stmt_lambdas<'a>(s: &'a Stmt, out: &mut Vec<&'a LambdaExpr>) {
    match s {
        Stmt::Var(v) => walk_expr_lambdas(&v.init, out),
        Stmt::If(i) => {
            walk_expr_lambdas(&i.cond, out);
            walk_block_lambdas(&i.then_block, out);
            if let Some(e) = &i.else_block {
                walk_block_lambdas(e, out);
            }
        }
        Stmt::While(w) => {
            walk_expr_lambdas(&w.cond, out);
            walk_block_lambdas(&w.body, out);
        }
        Stmt::ForRange(f) => {
            walk_expr_lambdas(&f.start, out);
            walk_expr_lambdas(&f.end, out);
            walk_block_lambdas(&f.body, out);
        }
        Stmt::ForIn(f) => {
            walk_expr_lambdas(&f.iterable, out);
            walk_block_lambdas(&f.body, out);
        }
        Stmt::Match(m) => {
            walk_expr_lambdas(&m.scrutinee, out);
            for a in &m.arms {
                walk_block_lambdas(&a.body, out);
            }
        }
        Stmt::Try(t) => {
            walk_block_lambdas(&t.try_block, out);
            if let Some(c) = &t.catch {
                walk_block_lambdas(&c.body, out);
            }
            if let Some(f) = &t.finally {
                walk_block_lambdas(f, out);
            }
        }
        Stmt::Throw(t) => walk_expr_lambdas(&t.value, out),
        Stmt::Return(r) => {
            if let Some(e) = &r.value {
                walk_expr_lambdas(e, out);
            }
        }
        Stmt::Expr(e) => walk_expr_lambdas(e, out),
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

fn walk_expr_lambdas<'a>(e: &'a Expr, out: &mut Vec<&'a LambdaExpr>) {
    match e {
        Expr::Lambda(l) => {
            out.push(l);
            match &l.body {
                LambdaBody::Expr(body) => walk_expr_lambdas(body, out),
                LambdaBody::Block(block) => walk_block_lambdas(block, out),
            }
        }
        Expr::Call(c) => {
            walk_expr_lambdas(&c.callee, out);
            for a in &c.args {
                walk_expr_lambdas(a, out);
            }
        }
        Expr::Field(f) => walk_expr_lambdas(&f.object, out),
        Expr::Assign(a) => walk_expr_lambdas(&a.value, out),
        Expr::Binary(b) => {
            walk_expr_lambdas(&b.left, out);
            walk_expr_lambdas(&b.right, out);
        }
        Expr::Unary(u) => walk_expr_lambdas(&u.expr, out),
        Expr::ForceUnwrap(f) => walk_expr_lambdas(&f.expr, out),
        Expr::Is(i) => walk_expr_lambdas(&i.expr, out),
        Expr::Group(inner, _) => walk_expr_lambdas(inner, out),
        Expr::If(i) => {
            walk_expr_lambdas(&i.cond, out);
            walk_block_lambdas(&i.then_block, out);
            walk_block_lambdas(&i.else_block, out);
        }
        Expr::Async(_) => {}
        Expr::Ident(_)
        | Expr::This(_)
        | Expr::Int(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::Null(_) => {}
    }
}

/// Collect Fun types that need C typedefs (lambdas + AST annotations).
fn collect_fun_tys(checked: &CheckedFile) -> Vec<Ty> {
    let mut out: Vec<Ty> = checked.lambda_tys.values().cloned().collect();
    fn from_semantic_ty(ty: &Ty, acc: &mut Vec<Ty>) {
        match ty {
            Ty::Fun { params, ret } => {
                acc.push(ty.clone());
                for param in params {
                    from_semantic_ty(param, acc);
                }
                from_semantic_ty(ret, acc);
            }
            Ty::Nullable(inner)
            | Ty::Task(inner)
            | Ty::TaskHandle(inner)
            | Ty::Channel(inner)
            | Ty::ForeignHandle(inner) => from_semantic_ty(inner, acc),
            Ty::ClassApp { args, .. }
            | Ty::EnumApp { args, .. }
            | Ty::InterfaceApp { args, .. } => {
                for arg in args {
                    from_semantic_ty(arg, acc);
                }
            }
            _ => {}
        }
    }
    fn from_type_ref(t: &TypeRef, acc: &mut Vec<Ty>, open_params: &[String]) {
        if let Some(fun) = &t.fun {
            let params: Vec<Ty> = fun
                .params
                .iter()
                .map(|p| {
                    // shallow: only nested fun handled by recursion
                    let mut nested = Vec::new();
                    from_type_ref(p, &mut nested, open_params);
                    if let Some(fun) = &p.fun {
                        let ps = fun
                            .params
                            .iter()
                            .map(|p| type_ref_to_ty_loose(p, open_params))
                            .collect();
                        let ret = type_ref_to_ty_loose(&fun.ret, open_params);
                        Ty::Fun {
                            params: ps,
                            ret: Box::new(ret),
                        }
                    } else {
                        type_ref_to_ty_loose(p, open_params)
                    }
                })
                .collect();
            let ret = type_ref_to_ty_loose(&fun.ret, open_params);
            acc.push(Ty::Fun {
                params,
                ret: Box::new(ret),
            });
            for p in &fun.params {
                from_type_ref(p, acc, open_params);
            }
            from_type_ref(&fun.ret, acc, open_params);
        }
        for a in &t.type_args {
            from_type_ref(a, acc, open_params);
        }
    }
    fn type_ref_to_ty_loose(t: &TypeRef, open_params: &[String]) -> Ty {
        if let Some(fun) = &t.fun {
            let params = fun
                .params
                .iter()
                .map(|p| type_ref_to_ty_loose(p, open_params))
                .collect();
            let ret = type_ref_to_ty_loose(&fun.ret, open_params);
            let base = Ty::Fun {
                params,
                ret: Box::new(ret),
            };
            return if t.nullable {
                Ty::Nullable(Box::new(base))
            } else {
                base
            };
        }
        let base = match t.name.name.as_str() {
            "Int" => Ty::Int,
            "Bool" => Ty::Bool,
            "String" => Ty::String,
            "Unit" => Ty::Unit,
            name if open_params.iter().any(|p| p == name) => Ty::TypeParam(name.into()),
            other => {
                if t.type_args.is_empty() {
                    Ty::Class(other.to_string())
                } else {
                    Ty::ClassApp {
                        name: other.to_string(),
                        args: t
                            .type_args
                            .iter()
                            .map(|a| type_ref_to_ty_loose(a, open_params))
                            .collect(),
                    }
                }
            }
        };
        if t.nullable {
            Ty::Nullable(Box::new(base))
        } else {
            base
        }
    }
    for f in &checked.ast.functions {
        let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
        for p in &f.params {
            from_type_ref(&p.ty, &mut out, &params);
        }
        if let Some(rt) = &f.return_type {
            from_type_ref(rt, &mut out, &params);
        }
    }
    // Type aliases are already expanded in checked function signatures. Walk
    // those semantic types as well, otherwise an alias to a function type
    // misses the C fat-pointer typedef required by the generated signature.
    for f in &checked.functions {
        for param in &f.params {
            from_semantic_ty(param, &mut out);
        }
        from_semantic_ty(&f.ret, &mut out);
    }
    for c in &checked.ast.classes {
        let class_params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
        for field in &c.fields {
            from_type_ref(&field.ty, &mut out, &class_params);
        }
        for m in &c.methods {
            let mut params = class_params.clone();
            params.extend(m.type_params.iter().map(|p| p.name.name.clone()));
            for p in &m.params {
                from_type_ref(&p.ty, &mut out, &params);
            }
            if let Some(rt) = &m.return_type {
                from_type_ref(rt, &mut out, &params);
            }
        }
    }
    out
}

fn emit_fun_typedefs(out: &mut String, checked: &CheckedFile) {
    let mut seen = std::collections::HashSet::new();
    let mut tys = collect_fun_tys(checked);
    tys.sort_by_key(|t| t.mono_suffix());
    for ty in &tys {
        // Generic declarations contribute open `Fun<T, R>` annotations while
        // their concrete call sites contribute the monomorphized function
        // types.  An open function type has no valid C representation yet;
        // emitting it would produce identifiers such as `aura_cls_T`.
        if !matches!(ty, Ty::Fun { .. }) || ty.is_open() {
            continue;
        }
        let key = ty.mono_suffix();
        if seen.insert(key) {
            emit_fun_typedef(out, ty, checked);
        }
    }
    // `std.http` server lowering is emitted for the imported package even when
    // an application only calls a client helper. Its handler ABI must therefore
    // be declared independently of whether the application has a matching
    // lambda from which generic typedef collection can infer it.
    let http_handler_key = "Fun_std_http_Request_std_http_Response__Task_Unit";
    let uses_http_server_lowering = checked.ast.async_functions.iter().any(|fun_decl| {
        async_fun_decl_package(fun_decl, checked) == "std.http"
            && matches!(fun_decl.name.name.as_str(), "serve" | "serveConnection")
    });
    if uses_http_server_lowering && seen.insert(http_handler_key.to_string()) {
        out.push_str("typedef struct {\n");
        out.push_str("  void *env;\n");
        out.push_str("  AuraTaskFrame *(*fn)(void *env, aura_cls_std_http_Request *, aura_cls_std_http_Response *);\n");
        out.push_str("} aura_fp_Fun_std_http_Request_std_http_Response__Task_Unit;\n");
    }
    if !seen.is_empty() {
        out.push('\n');
    }
}

fn emit_capture_drop_helpers(out: &mut String, checked: &CheckedFile) {
    let mut array_keys = std::collections::BTreeSet::new();
    let mut array_c_types = std::collections::HashMap::new();
    let mut has_fun = false;
    let mut has_obj = checked
        .ast
        .classes
        .iter()
        .any(|class| class.kind == NominalKind::Class);
    for captures in checked.lambda_captures.values() {
        for cap in captures {
            if !cap.by_ref {
                continue;
            }
            if is_array_capture_ty(&cap.ty) {
                array_keys.insert(cap.ty.mono_suffix());
            } else if is_fun_capture_ty(&cap.ty) {
                has_fun = true;
            } else if is_heap_class_capture_ty(&cap.ty, checked) {
                has_obj = true;
            }
        }
    }
    for (name, args) in &checked.mono_classes {
        if is_array_mono(name) {
            if let Some(elem) = args.first() {
                let key = format!("Array_{}", elem.mono_suffix());
                array_c_types.insert(key.clone(), crate::stmt::local_key_to_c(&key, checked));
                array_keys.insert(key);
            }
        }
    }
    has_fun |= collect_fun_tys(checked)
        .iter()
        .any(|ty| matches!(ty, Ty::Fun { .. }));
    for key in array_keys {
        let c_ty = array_c_types.get(&key).cloned().unwrap_or_else(|| {
            c_type_from_ty(
                &checked
                    .lambda_captures
                    .values()
                    .flatten()
                    .find(|c| c.by_ref && c.ty.mono_suffix() == key)
                    .expect("array capture key must have a capture")
                    .ty,
                checked,
            )
        });
        let _ = writeln!(out, "static void aura_capture_drop_{key}(void *value) {{");
        let _ = writeln!(out, "  {c_ty} *__a = ({c_ty} *)value;");
        if crate::array_emit::is_array_of_heap_class(&key, checked) {
            out.push_str("  aura_gc_remove_array_root((void **)&__a->data);\n");
        }
        crate::array_emit::emit_array_contents_free(out, 2, "(*__a)", &key);
        out.push_str("  free(__a);\n}\n\n");
    }
    if has_fun {
        out.push_str("typedef struct { void *env; void *fn; } aura_capture_fun_payload;\n");
        out.push_str("static void aura_capture_drop_fun(void *value) {\n");
        out.push_str("  aura_capture_fun_payload *__f = (aura_capture_fun_payload *)value;\n");
        out.push_str("  aura_fun_env_free(__f->env);\n");
        out.push_str("  free(__f);\n}\n\n");
    }
    if has_obj {
        out.push_str("typedef struct { void *value; } aura_capture_obj_payload;\n");
        out.push_str("static void aura_capture_drop_obj(void *value) {\n");
        out.push_str("  aura_capture_obj_payload *__o = (aura_capture_obj_payload *)value;\n");
        out.push_str("  aura_gc_remove_root(&__o->value);\n");
        out.push_str("  free(__o);\n}\n\n");
    }
}

fn emit_lazy_helpers(out: &mut String, checked: &CheckedFile) {
    let mut emitted = HashSet::new();
    for (name, args) in &checked.mono_classes {
        let Some(class) = checked.ast.classes.iter().find(|candidate| {
            candidate.name.name == *name
                && candidate.origin_package == "std.sync"
                && candidate.name.name == "Lazy"
        }) else {
            continue;
        };
        let _ = class;
        let Some(result_ty) = args.first() else {
            continue;
        };
        let suffix = result_ty.mono_suffix();
        if !emitted.insert(suffix.clone()) {
            continue;
        }
        let fun = Ty::Fun {
            params: Vec::new(),
            ret: Box::new(result_ty.clone()),
        };
        let fun_ty = c_fun_typedef(&fun.mono_suffix());
        let env_ty = format!("aura_lazy_env_{suffix}");
        let init = format!("aura_lazy_init_{suffix}");
        let env_destroy = format!("aura_lazy_env_destroy_{suffix}");
        let value_destroy = format!("aura_lazy_value_destroy_{suffix}");
        let result_cty = c_type_from_ty(result_ty, checked);
        let _ = writeln!(out, "typedef struct {{ {fun_ty} body; }} {env_ty};");
        let _ = writeln!(out, "static void {env_destroy}(void *value) {{ {env_ty} *env = ({env_ty} *)value; if (env != NULL) {{ aura_fun_env_free(env->body.env); free(env); }} }}");
        let _ = writeln!(out, "static void {value_destroy}(void *value) {{");
        if suffix == "String" {
            out.push_str(
                "  if (value != NULL) { char **text = (char **)value; free(*text); free(text); }\n",
            );
        } else {
            out.push_str("  free(value);\n");
        }
        out.push_str("}\n");
        let _ = writeln!(
            out,
            "static void {init}(AuraLazyCell *cell, void *value) {{"
        );
        let _ = writeln!(out, "  {env_ty} *env = ({env_ty} *)value;");
        if suffix == "Unit" {
            out.push_str("  env->body.fn(env->body.env);\n  void *__marker = malloc(1);\n");
            let _ = writeln!(
                out,
                "  aura_lazy_cell_publish(cell, __marker, 1, {value_destroy});"
            );
        } else if suffix == "String" {
            out.push_str("  const char *__value = env->body.fn(env->body.env);\n  char **result = (char **)malloc(sizeof(*result));\n  if (result == NULL) return;\n  if (__value == NULL) *result = NULL; else { size_t length = strlen(__value) + 1; *result = (char *)malloc(length); if (*result != NULL) memcpy(*result, __value, length); }\n  if (__value != NULL && *result == NULL) { free(result); return; }\n");
            let _ = writeln!(
                out,
                "  aura_lazy_cell_publish(cell, result, sizeof(*result), {value_destroy});"
            );
        } else {
            let _ = writeln!(
                out,
                "  {result_cty} *__value = ({result_cty} *)malloc(sizeof(*__value));"
            );
            out.push_str("  if (__value == NULL) return;\n");
            out.push_str("  *__value = env->body.fn(env->body.env);\n");
            let _ = writeln!(
                out,
                "  aura_lazy_cell_publish(cell, __value, sizeof(*__value), {value_destroy});"
            );
        }
        out.push_str("}\n\n");
    }
}

fn emit_lambda_fns(out: &mut String, checked: &CheckedFile, detector: bool) {
    if checked.lambda_tys.is_empty() {
        return;
    }
    let ids = build_lambda_ids(checked);
    let mut lambdas = collect_lambdas(&checked.ast);
    lambdas.sort_by_key(|l| l.span.start);
    for lam in &lambdas {
        let Some(&id) = ids.get(&lam.span.start) else {
            continue;
        };
        let Some(Ty::Fun { params, ret }) = checked.lambda_tys.get(&lam.span.start) else {
            continue;
        };
        let ret_c = if matches!(ret.as_ref(), Ty::Unit) {
            "void".to_string()
        } else {
            c_type_from_ty(ret, checked)
        };
        let mut parts = vec!["void *env".to_string()];
        for p in params {
            parts.push(c_type_from_ty(p, checked));
        }
        let _ = writeln!(
            out,
            "static {ret_c} aura_lambda_{id}({});",
            parts.join(", ")
        );
    }
    out.push('\n');
    // C10h/C12k/C12l/C12m/C13e: env structs for capturing lambdas (stable field order from sema).
    // Header: `__drop` + `__refs` (refcount for shared nested Fun envs / multi-owner free).
    // Immutable Array captures store an owned snapshot; mutable captures use
    // the shared pointer-cell path below.
    // By-ref Int/Bool/String captures store aura_box_* pointers (retain on fill, release on drop).
    // Fun captures store fat pointer; retain nested env on fill, release on drop.
    for lam in &lambdas {
        let Some(&id) = ids.get(&lam.span.start) else {
            continue;
        };
        let captures = checked
            .lambda_captures
            .get(&lam.span.start)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        if captures.is_empty() {
            continue;
        }
        let _ = writeln!(out, "typedef struct {{");
        let _ = writeln!(out, "  void (*__drop)(void *);");
        let _ = writeln!(out, "  int32_t __refs;");
        for cap in captures {
            let field_ty = c_capture_field_type(&cap.ty, cap.by_ref, checked);
            let _ = writeln!(out, "  {field_ty} {};", mangle_ident(&cap.name));
        }
        let _ = writeln!(out, "}} aura_lenv_{id};");
        // Per-env drop: release owned Array snapshots, remove GC roots for heap-class
        // captures, release by-ref boxes and nested Fun envs, then free the env.
        let _ = writeln!(out, "static void aura_lenv_{id}_drop(void *env) {{");
        let _ = writeln!(out, "  aura_lenv_{id} *__e = (aura_lenv_{id} *)env;");
        for cap in captures {
            let m = mangle_ident(&cap.name);
            if cap.by_ref {
                let rel = match &cap.ty {
                    Ty::Bool => format!("aura_box_bool_release(__e->{m});"),
                    Ty::String => format!("aura_box_str_release(__e->{m});"),
                    Ty::Int => format!("aura_box_i64_release(__e->{m});"),
                    _ => format!("aura_box_ptr_release(__e->{m});"),
                };
                let _ = writeln!(out, "  {rel}");
            } else if is_fun_capture_ty(&cap.ty) {
                // C13e: release retained nested env (no-op when env is NULL).
                let _ = writeln!(out, "  aura_fun_env_free(__e->{m}.env);");
            } else if is_array_capture_ty(&cap.ty) {
                let key = cap.ty.mono_suffix();
                if crate::array_emit::is_array_of_heap_class(&key, checked) {
                    let _ = writeln!(out, "  aura_gc_remove_array_root((void **)&__e->{m}.data);");
                }
                crate::array_emit::emit_array_contents_free(out, 2, &format!("__e->{m}"), &key);
            } else if is_heap_class_capture_ty(&cap.ty, checked) {
                let _ = writeln!(out, "  aura_gc_remove_root((void **)&__e->{m});");
            }
        }
        let _ = writeln!(out, "  free(__e);");
        let _ = writeln!(out, "}}\n");
    }

    for lam in lambdas {
        let Some(&id) = ids.get(&lam.span.start) else {
            continue;
        };
        let Some(fun_ty) = checked.lambda_tys.get(&lam.span.start) else {
            continue;
        };
        let Ty::Fun { params, ret } = fun_ty else {
            continue;
        };
        let captures = checked
            .lambda_captures
            .get(&lam.span.start)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let ret_c = match ret.as_ref() {
            Ty::Unit => "void".to_string(),
            r => c_type_from_ty(r, checked),
        };
        // C10h: first param is always env (fat-pointer convention).
        let mut param_parts = vec!["void *env".to_string()];
        for (i, pty) in params.iter().enumerate() {
            let pname = if let Some(p) = lam.params.get(i) {
                mangle_ident(&p.name.name)
            } else {
                format!("p{i}")
            };
            param_parts.push(format!("{} {pname}", c_type_from_ty(pty, checked)));
        }
        let ps = param_parts.join(", ");
        let _ = writeln!(out, "static {ret_c} aura_lambda_{id}({ps}) {{");
        if captures.is_empty() {
            out.push_str("  (void)env;\n");
        } else {
            let _ = writeln!(out, "  aura_lenv_{id} *__e = (aura_lenv_{id} *)env;");
            for cap in captures {
                let m = mangle_ident(&cap.name);
                let field_ty = c_capture_field_type(&cap.ty, cap.by_ref, checked);
                let _ = writeln!(out, "  {field_ty} {m} = __e->{m};");
            }
        }
        let ret_key = Some(ret.mono_suffix());
        let mut ctx = EmitCtx {
            checked,
            detector,
            method_class: None,
            type_params: Vec::new(),
            type_args: Vec::new(),
            locals: vec![HashMap::new()],
            local_c_names: HashMap::new(),
            array_owners: vec![std::collections::HashSet::new()],
            fun_owners: vec![std::collections::HashSet::new()],
            string_owners: vec![std::collections::HashSet::new()],
            channel_owners: vec![std::collections::HashSet::new()],
            task_result_owners: vec![std::collections::HashSet::new()],
            task_handle_owners: vec![std::collections::HashSet::new()],
            box_locals: vec![std::collections::HashSet::new()],
            box_owners: vec![std::collections::HashSet::new()],
            gc_roots: vec![std::collections::HashSet::new()],
            array_gc_roots: vec![std::collections::HashSet::new()],
            return_key: ret_key,
            lambda_ids: ids.clone(),
            spawn_params: HashSet::new(),
            mutable_spawn_captures: HashSet::new(),
            async_frame: None,
            task_poller: false,
        };
        // Immutable Array captures are owned by the environment. The lambda
        // receives a shallow local view and must not free it per invocation.
        // C12m: by-ref captures are box pointers; mark so reads/writes use ->value.
        for cap in captures {
            ctx.define_local(&cap.name, cap.ty.mono_suffix());
            if cap.by_ref {
                ctx.mark_box_local(&cap.name);
            }
        }
        for (i, p) in lam.params.iter().enumerate() {
            let key = params
                .get(i)
                .map(|t| t.mono_suffix())
                .unwrap_or_else(|| "Int".into());
            ctx.define_local(&p.name.name, key);
        }
        match &lam.body {
            LambdaBody::Expr(body) => {
                let body_c = crate::expr::emit_expr(body, &mut ctx);
                if matches!(ret.as_ref(), Ty::Unit) {
                    let _ = writeln!(out, "  {body_c};");
                } else {
                    let _ = writeln!(out, "  return {body_c};");
                }
            }
            LambdaBody::Block(block) => {
                // C10g: emit statements; returns handled by Stmt::Return.
                emit_block(out, block, 1, &mut ctx);
                // Unreachable fallback if all paths return (mirrors fun bodies).
                match ret.as_ref() {
                    Ty::Unit => {}
                    Ty::Int => out.push_str("  return INT64_C(0); /* fallback */\n"),
                    Ty::Bool => out.push_str("  return false; /* fallback */\n"),
                    Ty::String => out.push_str("  return \"\"; /* fallback */\n"),
                    other => {
                        let ct = c_type_from_ty(other, checked);
                        if ct != "void" {
                            let _ = writeln!(out, "  return ({ct}){{0}}; /* fallback */");
                        }
                    }
                }
            }
        }
        out.push_str("}\n\n");
    }
}

fn emit_spawn_blocking_helper(out: &mut String, f: &FunDecl, checked: &CheckedFile, args: &[Ty]) {
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let Some(result_ty) = args.first() else {
        return;
    };
    let suffix = result_ty.mono_suffix();
    let helper = format!("aura_spawn_blocking_{suffix}");
    let env_ty = format!("aura_spawn_blocking_env_{suffix}");
    let fun_ty = c_type_ref_subst(&f.params[0].ty, checked, &params, args);
    let result_cty = c_type_from_ty(result_ty, checked);
    let destroy = format!("aura_spawn_blocking_destroy_{suffix}");
    let result_destroy = format!("aura_spawn_blocking_result_destroy_{suffix}");

    let _ = writeln!(out, "typedef struct {{ {fun_ty} body; }} {env_ty};");
    let _ = writeln!(out, "static void {destroy}(void *value) {{");
    let _ = writeln!(out, "  {env_ty} *env = ({env_ty} *)value;");
    out.push_str("  if (env != NULL) { aura_fun_env_free(env->body.env); free(env); }\n}\n");
    let _ = writeln!(
        out,
        "static void {result_destroy}(void *value, size_t size) {{"
    );
    out.push_str("  (void)size;\n");
    if result_ty.mono_suffix() == "String" {
        out.push_str(
            "  if (value != NULL) { char **text = (char **)value; free(*text); free(text); }\n",
        );
    } else {
        out.push_str("  free(value);\n");
    }
    out.push_str("}\n");
    let _ = writeln!(
        out,
        "static void {helper}(AuraTaskFrame *frame, void *value) {{"
    );
    let _ = writeln!(out, "  {env_ty} *env = ({env_ty} *)value;");
    out.push_str("  if (env == NULL || aura_task_frame_cancel_requested(frame)) return;\n");
    if result_ty.mono_suffix() == "Unit" {
        out.push_str("  env->body.fn(env->body.env);\n");
    } else if result_ty.mono_suffix() == "String" {
        out.push_str("  const char *__value = env->body.fn(env->body.env);\n");
        out.push_str("  char **result = (char **)malloc(sizeof(*result));\n");
        out.push_str("  if (result == NULL) return;\n");
        out.push_str("  if (__value == NULL) *result = NULL; else { size_t length = strlen(__value) + 1; *result = (char *)malloc(length); if (*result != NULL) memcpy(*result, __value, length); }\n");
        out.push_str("  if (__value != NULL && *result == NULL) { free(result); return; }\n");
        let _ = writeln!(
            out,
            "  aura_task_frame_set_result(frame, result, sizeof(*result), {result_destroy});"
        );
    } else {
        let _ = writeln!(out, "  {result_cty} __value = env->body.fn(env->body.env);");
        let _ = writeln!(
            out,
            "  {result_cty} *result = ({result_cty} *)malloc(sizeof(*result));"
        );
        out.push_str("  if (result == NULL) return;\n");
        out.push_str("  *result = __value;\n");
        let _ = writeln!(
            out,
            "  aura_task_frame_set_result(frame, result, sizeof(*result), {result_destroy});"
        );
    }
    out.push_str("}\n\n");
}

pub(crate) fn emit_fun(
    out: &mut String,
    f: &FunDecl,
    checked: &CheckedFile,
    args: &[Ty],
    detector: bool,
) {
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = fun_decl_package(f, checked);
    let is_spawn_blocking = pkg == "std.task"
        && f.name.name == "spawnBlocking"
        && f.params.len() == 1
        && args.len() == 1;
    let is_task_scope = pkg == "std.task" && f.name.name == "taskScope" && f.params.len() == 1;
    let is_lazy =
        pkg == "std.sync" && f.name.name == "lazy" && f.params.len() == 1 && args.len() == 1;
    let is_select =
        pkg == "std.task" && f.name.name == "select" && f.params.is_empty() && args.len() == 1;
    if is_spawn_blocking {
        emit_spawn_blocking_helper(out, f, checked, args);
    }
    let _ = writeln!(out, "{} {{", c_fun_signature(f, checked, args));
    if is_spawn_blocking {
        let suffix = args[0].mono_suffix();
        let env_ty = format!("aura_spawn_blocking_env_{suffix}");
        let helper = format!("aura_spawn_blocking_{suffix}");
        let destroy = format!("aura_spawn_blocking_destroy_{suffix}");
        let body = mangle_ident(&f.params[0].name.name);
        let fun_ty = c_type_ref_subst(&f.params[0].ty, checked, &params, args);
        let _ = writeln!(
            out,
            "  {env_ty} *__env = ({env_ty} *)malloc(sizeof(*__env));"
        );
        out.push_str(
            "  if (__env == NULL || __aura_task_executor == NULL) { free(__env); return NULL; }\n",
        );
        let _ = writeln!(out, "  __env->body = ({fun_ty}){body};");
        out.push_str("  if (__env->body.env != NULL) aura_fun_env_retain(__env->body.env);\n");
        let _ = writeln!(out, "  return aura_task_frame_new_blocking(__aura_task_executor, {helper}, __env, {destroy});");
        out.push_str("}\n");
        return;
    }
    if is_task_scope {
        let body = mangle_ident(&f.params[0].name.name);
        out.push_str("  AuraTaskScope *__scope = aura_task_scope_begin(__aura_task_executor);\n");
        out.push_str("  if (__scope == NULL) return;\n");
        let _ = writeln!(out, "  jmp_buf __scope_jb_{};", f.span.start);
        let _ = writeln!(out, "  if (setjmp(__scope_jb_{}) == 0) {{", f.span.start);
        let _ = writeln!(out, "    aura_try_enter(&__scope_jb_{});", f.span.start);
        let _ = writeln!(out, "    {body}.fn({body}.env);");
        out.push_str("    aura_try_leave();\n");
        out.push_str("    int __scope_status = aura_task_scope_end(__scope);\n");
        out.push_str(
            "    if (__scope_status == 1) aura_throw_string(\"structured child task failed\");\n",
        );
        out.push_str("    if (__scope_status == 2) aura_throw_string(\"structured child task cancelled\");\n");
        out.push_str("  } else {\n");
        out.push_str("    aura_task_scope_end(__scope);\n");
        out.push_str("    aura_ex_rethrow();\n");
        out.push_str("  }\n");
        out.push_str("}\n");
        return;
    }
    if is_lazy {
        let suffix = args[0].mono_suffix();
        let env_ty = format!("aura_lazy_env_{suffix}");
        let init = format!("aura_lazy_init_{suffix}");
        let env_destroy = format!("aura_lazy_env_destroy_{suffix}");
        let body = mangle_ident(&f.params[0].name.name);
        let fun_ty = c_type_ref_subst(&f.params[0].ty, checked, &params, args);
        let mono = type_mono("std.sync", "Lazy", args);
        let ctor = c_ctor_name(&mono);
        out.push_str("  if (__aura_task_executor == NULL) return NULL;\n");
        let _ = writeln!(
            out,
            "  {env_ty} *__env = ({env_ty} *)malloc(sizeof(*__env));"
        );
        out.push_str("  if (__env == NULL) return NULL;\n");
        let _ = writeln!(out, "  __env->body = ({fun_ty}){body};");
        out.push_str("  if (__env->body.env != NULL) aura_fun_env_retain(__env->body.env);\n");
        let _ = writeln!(
            out,
            "  AuraLazyCell *__cell = aura_lazy_cell_new({init}, __env, {env_destroy});"
        );
        out.push_str("  if (__cell == NULL) { ");
        let _ = writeln!(out, "{env_destroy}(__env); return NULL; }}");
        let _ = writeln!(out, "  return {ctor}((int64_t)(uintptr_t)__cell);");
        out.push_str("}\n");
        return;
    }
    if is_select {
        let mono = type_mono("std.task", "Select", args);
        let ctor = c_ctor_name(&mono);
        out.push_str("  if (__aura_task_executor == NULL) return NULL;\n");
        out.push_str("  AuraTaskSelect *__select = aura_task_select_new();\n");
        out.push_str("  if (__select == NULL) return NULL;\n");
        let _ = writeln!(out, "  return {ctor}((int64_t)(uintptr_t)__select);");
        out.push_str("}\n");
        return;
    }
    if pkg == "std.time" && f.name.name == "nowMillis" && f.params.is_empty() {
        out.push_str("  return aura_time_monotonic_millis();\n}\n");
        return;
    }
    if pkg == "std.io"
        && matches!(
            f.name.name.as_str(),
            "taskErrorTypeName" | "taskErrorSourceId" | "taskErrorSpanStart" | "taskErrorSpanEnd"
        )
        && f.params.len() == 1
    {
        let error = mangle_ident(&f.params[0].name.name);
        match f.name.name.as_str() {
            "taskErrorTypeName" => out.push_str(&format!(
                "  if ({error}.tag != 0 || {error}.data.Failed.type_name == NULL) return NULL; size_t __type_len = strlen({error}.data.Failed.type_name); char *__type_copy = (char *)malloc(__type_len + 1); if (__type_copy == NULL) return NULL; memcpy(__type_copy, {error}.data.Failed.type_name, __type_len + 1); return (const char *)__type_copy;\n}}\n"
            )),
            "taskErrorSourceId" => out.push_str(&format!(
                "  return {error}.tag == 0 ? (int64_t){error}.data.Failed.source_id : 0;\n}}\n"
            )),
            "taskErrorSpanStart" => out.push_str(&format!(
                "  return {error}.tag == 0 ? (int64_t){error}.data.Failed.span_start : 0;\n}}\n"
            )),
            "taskErrorSpanEnd" => out.push_str(&format!(
                "  return {error}.tag == 0 ? (int64_t){error}.data.Failed.span_end : 0;\n}}\n"
            )),
            _ => unreachable!(),
        }
        return;
    }
    if pkg == "std.task" && f.name.name == "cancelAfter" && f.params.len() == 2 {
        let task = mangle_ident(&f.params[0].name.name);
        let timeout = mangle_ident(&f.params[1].name.name);
        let _ = writeln!(out, "  return {task} != NULL && aura_task_frame_set_cancel_deadline({task}, (int){timeout}) != 0;");
        out.push_str("}\n");
        return;
    }
    if pkg == "std.task" && f.name.name == "linkCancellation" && f.params.len() == 2 {
        let parent = mangle_ident(&f.params[0].name.name);
        let child = mangle_ident(&f.params[1].name.name);
        let _ = writeln!(
            out,
            "  return aura_task_frame_link_cancellation({parent}, {child}) != 0;"
        );
        out.push_str("}\n");
        return;
    }
    if pkg == "std.encoding" && f.params.len() == 1 {
        let value = mangle_ident(&f.params[0].name.name);
        let intrinsic = match f.name.name.as_str() {
            "hexEncode" => "aura_encoding_hex_encode",
            "hexDecode" => "aura_encoding_hex_decode",
            "base64Encode" => "aura_encoding_base64_encode",
            "base64Decode" => "aura_encoding_base64_decode",
            "percentEncode" => "aura_encoding_percent_encode",
            "percentDecode" => "aura_encoding_percent_decode",
            "isValidUtf8" => "aura_encoding_is_valid_utf8",
            _ => "",
        };
        if !intrinsic.is_empty() {
            let _ = writeln!(out, "  return {intrinsic}({value});");
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.url" && f.params.len() == 1 {
        let value = mangle_ident(&f.params[0].name.name);
        let intrinsic = match f.name.name.as_str() {
            "isOriginForm" => "aura_url_is_origin_form",
            "path" => "aura_url_path",
            "normalizePath" => "aura_url_normalize_path",
            "query" => "aura_url_query",
            "isAbsolute" => "aura_url_is_absolute",
            "authority" => "aura_url_authority",
            "authorityHost" => "aura_url_authority_host",
            "authorityPort" => "aura_url_authority_port",
            _ => "",
        };
        if !intrinsic.is_empty() {
            let _ = writeln!(out, "  return {intrinsic}({value});");
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.url" && f.name.name == "queryValue" && f.params.len() == 2 {
        let target = mangle_ident(&f.params[0].name.name);
        let key = mangle_ident(&f.params[1].name.name);
        out.push_str(&format!(
            "  return aura_url_query_value({target}, {key});\n}}\n"
        ));
        return;
    }
    if pkg == "std.mime" && f.params.len() == 1 {
        let value = mangle_ident(&f.params[0].name.name);
        let intrinsic = match f.name.name.as_str() {
            "isValidType" => "aura_mime_is_valid_type",
            "sanitizeFilename" => "aura_mime_sanitize_filename",
            "dispositionFilename" => "aura_mime_disposition_filename",
            _ => "",
        };
        if !intrinsic.is_empty() {
            let _ = writeln!(out, "  return {intrinsic}({value});");
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.bytes" {
        let intrinsic = match (f.name.name.as_str(), f.params.len()) {
            ("copy", 1) => Some((
                "aura_bytes_copy",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("concat", 2) => Some((
                "aura_bytes_concat",
                vec![
                    mangle_ident(&f.params[0].name.name),
                    mangle_ident(&f.params[1].name.name),
                ],
            )),
            ("slice", 3) => Some((
                "aura_bytes_slice",
                vec![
                    mangle_ident(&f.params[0].name.name),
                    mangle_ident(&f.params[1].name.name),
                    mangle_ident(&f.params[2].name.name),
                ],
            )),
            ("equals", 2) => Some((
                "aura_bytes_equals",
                vec![
                    mangle_ident(&f.params[0].name.name),
                    mangle_ident(&f.params[1].name.name),
                ],
            )),
            _ => None,
        };
        if let Some((name, args)) = intrinsic {
            let _ = writeln!(out, "  return {name}({});", args.join(", "));
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.fs" {
        let intrinsic = match (f.name.name.as_str(), f.params.len()) {
            ("join", 2) => Some((
                "aura_fs_join",
                vec![
                    mangle_ident(&f.params[0].name.name),
                    mangle_ident(&f.params[1].name.name),
                ],
            )),
            ("basename", 1) => Some((
                "aura_fs_basename",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("dirname", 1) => Some((
                "aura_fs_dirname",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("extension", 1) => Some((
                "aura_fs_extension",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("isAbsolute", 1) => Some((
                "aura_fs_is_absolute",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("isDirectory", 1) => Some((
                "aura_fs_is_directory",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("fileMode", 1) => Some((
                "aura_fs_file_mode",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("permissions", 1) => Some((
                "aura_fs_permissions",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("modifiedMillis", 1) => Some((
                "aura_fs_modified_millis",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("listNames", 1) => Some((
                "aura_fs_list_names",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("isSymlink", 1) => Some((
                "aura_fs_is_symlink",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            _ => None,
        };
        if let Some((name, args)) = intrinsic {
            let _ = writeln!(out, "  return {name}({});", args.join(", "));
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.os" {
        let intrinsic = match (f.name.name.as_str(), f.params.len()) {
            ("getEnv", 1) => Some((
                "aura_os_get_env",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("setEnv", 2) => Some((
                "aura_os_set_env",
                vec![
                    mangle_ident(&f.params[0].name.name),
                    mangle_ident(&f.params[1].name.name),
                ],
            )),
            ("unsetEnv", 1) => Some((
                "aura_os_unset_env",
                vec![mangle_ident(&f.params[0].name.name)],
            )),
            ("cwd", 0) => Some(("aura_os_cwd", Vec::new())),
            ("pid", 0) => Some(("aura_os_pid", Vec::new())),
            ("platform", 0) => Some(("aura_os_platform", Vec::new())),
            _ => None,
        };
        if let Some((name, args)) = intrinsic {
            let _ = writeln!(out, "  return {name}({});", args.join(", "));
            out.push_str("}\n");
            return;
        }
    }
    // std.io console + file intrinsics (runtime `aura_*`).
    if pkg == "std.io" {
        match (f.name.name.as_str(), f.params.len()) {
            ("print", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  aura_print({a});");
                out.push_str("}\n");
                return;
            }
            ("println", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  aura_println({a});");
                out.push_str("}\n");
                return;
            }
            ("eprint", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  aura_eprint({a});");
                out.push_str("}\n");
                return;
            }
            ("eprintln", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  aura_eprintln({a});");
                out.push_str("}\n");
                return;
            }
            ("readFile", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  return aura_read_file({a});");
                out.push_str("}\n");
                return;
            }
            // C12p: soft read — NULL on missing path / I/O / oversize / NUL.
            ("tryReadFile", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  return aura_try_read_file({a});");
                out.push_str("}\n");
                return;
            }
            ("writeFile", 2) => {
                let p = mangle_ident(&f.params[0].name.name);
                let c = mangle_ident(&f.params[1].name.name);
                let _ = writeln!(out, "  aura_write_file({p}, {c});");
                out.push_str("}\n");
                return;
            }
            // C13o: soft write — false on empty path / I/O fail (no throw).
            ("tryWriteFile", 2) => {
                let p = mangle_ident(&f.params[0].name.name);
                let c = mangle_ident(&f.params[1].name.name);
                let _ = writeln!(out, "  return aura_try_write_file({p}, {c});");
                out.push_str("}\n");
                return;
            }
            ("appendFile", 2) => {
                let p = mangle_ident(&f.params[0].name.name);
                let c = mangle_ident(&f.params[1].name.name);
                let _ = writeln!(out, "  aura_append_file({p}, {c});");
                out.push_str("}\n");
                return;
            }
            ("fileExists", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  return aura_file_exists({a});");
                out.push_str("}\n");
                return;
            }
            ("fileSize", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  return aura_file_size({a});");
                out.push_str("}\n");
                return;
            }
            ("openFile", 2) => {
                let path = mangle_ident(&f.params[0].name.name);
                let mode = mangle_ident(&f.params[1].name.name);
                out.push_str("  AuraFile *__file = NULL; AuraFfiOpaqueHandle *__handle = NULL;\n");
                let _ = writeln!(
                    out,
                    "  AuraFileStatus __open_status = aura_file_open({path}, (AuraFileMode){mode}, &__file); if (__open_status != AURA_FILE_OK || __file == NULL) {{ char __message[320]; snprintf(__message, sizeof(__message), \"openFile failed: %s\", aura_file_last_error()); aura_throw_string(__message); return NULL; }}"
                );
                out.push_str("  AuraFfiStatus __handle_status = aura_ffi_handle_new((void *)__file, aura_destroy_file_resource, &__handle); if (__handle_status != AURA_FFI_OK || __handle == NULL) { (void)aura_file_destroy(&__file); aura_throw_string(\"openFile failed: handle allocation\"); return NULL; }\n");
                out.push_str("  return __handle;\n}\n");
                return;
            }
            // C12b: std.io.args() → Array<String> from stashed argc/argv.
            ("args", 0) => {
                let arr_ty = c_class_type("Array_String");
                let ctor = c_ctor_name("Array_String");
                let _ = writeln!(out, "  int64_t __n = aura_args_count();");
                let _ = writeln!(out, "  {arr_ty} __a = {ctor}(__n);");
                out.push_str("  for (int64_t __i = 0; __i < __n; __i++) {\n");
                out.push_str("    __a.data[__i] = aura_args_get(__i);\n");
                out.push_str("  }\n");
                out.push_str("  return __a;\n");
                out.push_str("}\n");
                return;
            }
            // C12d: stdin line / whole-stdin (NULL = EOF for readLine).
            ("readLine", 0) => {
                out.push_str("  return aura_read_line();\n");
                out.push_str("}\n");
                return;
            }
            ("readAllStdin", 0) => {
                out.push_str("  return aura_read_all_stdin();\n");
                out.push_str("}\n");
                return;
            }
            // C12e: std.io.exit(code) → process exit (does not return).
            ("exit", 1) => {
                let a = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  aura_exit({a});");
                out.push_str("}\n");
                return;
            }
            _ => {}
        }
    }
    if pkg == "std.crypto" {
        match (f.name.name.as_str(), f.params.len()) {
            ("randomBytes", 1) => {
                let length = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  return aura_crypto_random_bytes({length});");
                out.push_str("}\n");
                return;
            }
            ("constantTimeEquals", 2) => {
                let left = mangle_ident(&f.params[0].name.name);
                let right = mangle_ident(&f.params[1].name.name);
                let _ = writeln!(
                    out,
                    "  return aura_crypto_constant_time_equals({left}, {right});"
                );
                out.push_str("}\n");
                return;
            }
            ("sha256", 1) => {
                let value = mangle_ident(&f.params[0].name.name);
                out.push_str("  const char *__hex = aura_crypto_sha256(");
                out.push_str(&value);
                out.push_str(");\n  return aura_new_std_crypto_Digest(\"SHA-256\", __hex);\n}\n");
                return;
            }
            ("hmacSha256", 2) => {
                let key = mangle_ident(&f.params[0].name.name);
                let value = mangle_ident(&f.params[1].name.name);
                let _ = writeln!(
                    out,
                    "  const char *__hex = aura_crypto_hmac_sha256({key}, {value});"
                );
                out.push_str("  return aura_new_std_crypto_Digest(\"HMAC-SHA-256\", __hex);\n}\n");
                return;
            }
            _ => {}
        }
    }
    if pkg == "std.reflect" {
        if f.name.name == "typeOf" && f.params.is_empty() && args.len() == 1 {
            let name = args[0].mono_suffix();
            let _ = writeln!(
                out,
                "  return aura_new_std_reflect_Type(0, aura_bytes_copy(\"{name}\"));"
            );
            out.push_str("}\n");
            return;
        }
        if f.name.name == "typeIdOf" && f.params.is_empty() && args.len() == 1 {
            let name = args[0].mono_suffix();
            let _ = writeln!(
                out,
                "  return aura_new_std_reflect_TypeId(aura_bytes_copy(\"{name}\"));"
            );
            out.push_str("}\n");
            return;
        }
        if f.name.name == "typeInfo" && f.params.len() == 1 {
            let value = mangle_ident(&f.params[0].name.name);
            out.push_str("  aura_enum_std_reflect_TypeKind __kind = aura_var_std_reflect_TypeKind_Unknown();\n");
            let _ = writeln!(out, "  if ({value} != NULL && (strcmp({value}, \"Int\") == 0 || strcmp({value}, \"Bool\") == 0 || strcmp({value}, \"String\") == 0 || strcmp({value}, \"Unit\") == 0)) __kind = aura_var_std_reflect_TypeKind_Primitive();");
            let _ = writeln!(
                out,
                "  return aura_new_std_reflect_TypeInfo(aura_bytes_copy({value}), __kind);"
            );
            out.push_str("}\n");
            return;
        }
        if f.name.name == "isReflectable" && f.params.len() == 1 {
            let value = mangle_ident(&f.params[0].name.name);
            let _ = writeln!(out, "  return {value} != NULL && (strcmp({value}, \"Int\") == 0 || strcmp({value}, \"Bool\") == 0 || strcmp({value}, \"String\") == 0 || strcmp({value}, \"Unit\") == 0);");
            out.push_str("}\n");
            return;
        }
        if matches!(f.name.name.as_str(), "fields" | "methods") && f.params.len() == 1 {
            let value = mangle_ident(&f.params[0].name.name);
            let element_names = |class: &aura_ast::ClassDecl| {
                if f.name.name == "fields" {
                    class
                        .fields
                        .iter()
                        .map(|field| field.name.name.clone())
                        .collect::<Vec<_>>()
                } else {
                    class
                        .methods
                        .iter()
                        .map(|method| method.name.name.clone())
                        .collect::<Vec<_>>()
                }
            };
            out.push_str("  if (");
            out.push_str(&value);
            out.push_str(" == NULL) return aura_new_Array_String(INT64_C(0));\n");
            for class in &checked.ast.classes {
                let names = element_names(class);
                let class_name = &class.name.name;
                let qualified_name =
                    type_mono(&class_decl_package(class, checked), class_name, &[]);
                let _ = writeln!(out, "  if (strcmp({value}, \"{class_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0) {{");
                let _ = writeln!(
                    out,
                    "    aura_cls_Array_String __metadata = aura_new_Array_String(INT64_C({}));",
                    names.len()
                );
                for (index, name) in names.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "    __metadata.data[{index}] = aura_bytes_copy(\"{name}\");"
                    );
                }
                out.push_str("    return __metadata;\n  }\n");
            }
            out.push_str("  return aura_new_Array_String(INT64_C(0));\n}\n");
            return;
        }
        if matches!(f.name.name.as_str(), "fieldMetadata" | "methodMetadata") && f.params.len() == 1
        {
            let value = mangle_ident(&f.params[0].name.name);
            out.push_str("  if (");
            out.push_str(&value);
            out.push_str(" == NULL) return aura_new_Array_String(INT64_C(0));\n");
            for class in &checked.ast.classes {
                let metadata = if f.name.name == "fieldMetadata" {
                    class
                        .fields
                        .iter()
                        .map(|field| format!("{}:{}", field.name.name, field.ty.name.name))
                        .collect::<Vec<_>>()
                } else {
                    class
                        .methods
                        .iter()
                        .map(|method| {
                            let return_name = method
                                .return_type
                                .as_ref()
                                .map(|ty| ty.name.name.clone())
                                .unwrap_or_else(|| "Unit".into());
                            format!("{}:{return_name}", method.name.name)
                        })
                        .collect::<Vec<_>>()
                };
                let class_name = &class.name.name;
                let qualified_name =
                    type_mono(&class_decl_package(class, checked), class_name, &[]);
                let _ = writeln!(out, "  if (strcmp({value}, \"{class_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0) {{");
                let _ = writeln!(
                    out,
                    "    aura_cls_Array_String __metadata = aura_new_Array_String(INT64_C({}));",
                    metadata.len()
                );
                for (index, name) in metadata.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "    __metadata.data[{index}] = aura_bytes_copy(\"{name}\");"
                    );
                }
                out.push_str("    return __metadata;\n  }\n");
            }
            out.push_str("  return aura_new_Array_String(INT64_C(0));\n}\n");
            return;
        }
    }
    if pkg == "std.json" && f.name.name == "decode" && f.params.len() == 1 && args.len() == 1 {
        let value = mangle_ident(&f.params[0].name.name);
        let name = args[0].mono_suffix();
        if let Some((class_name, class_args)) = match &args[0] {
            Ty::Class(name) => Some((name.clone(), Vec::new())),
            Ty::ClassApp { name, args } => Some((name.clone(), args.clone())),
            _ => None,
        } {
            let base = aura_sema::split_nominal(&class_name).0;
            if let Some(class) = checked.ast.classes.iter().find(|class| {
                class.name.name == base
                    && class_decl_package(class, checked) == aura_sema::split_nominal(&class_name).1
            }) {
                let params = class
                    .type_params
                    .iter()
                    .map(|param| param.name.name.clone())
                    .collect::<Vec<_>>();
                let mono = type_mono(
                    &class_decl_package(class, checked),
                    &class.name.name,
                    &class_args,
                );
                let ctor = c_ctor_name(&mono);
                let mut supported = true;
                let mut field_keys = Vec::new();
                for field in &class.fields {
                    let key = type_ref_local_key_expand(&field.ty, &params, &class_args, checked);
                    if !matches!(key.as_str(), "Int" | "Bool" | "String") {
                        supported = false;
                    }
                    field_keys.push((field, key));
                }
                if supported {
                    out.push_str(
                        "  if (value == NULL || !aura_json_is_valid((*value).text)) return NULL;\n",
                    );
                    let emit_cleanup = |out: &mut String, end: usize| {
                        for prior in 0..=end {
                            let prior_raw = format!("__json_field_{prior}");
                            let _ = writeln!(out, "    free((void *){prior_raw});");
                            if prior < end && field_keys[prior].1 == "String" {
                                let _ = writeln!(out, "    free((void *)__json_string_{prior});");
                            }
                        }
                    };
                    for (index, (field, key)) in field_keys.iter().enumerate() {
                        let raw = format!("__json_field_{index}");
                        let name = field.name.name.replace('\\', "\\\\").replace('"', "\\\"");
                        let _ = writeln!(
                            out,
                            "  const char *{raw} = aura_json_object_get((*{value}).text, \"{name}\");"
                        );
                        let _ = writeln!(out, "  if ({raw} == NULL) {{");
                        emit_cleanup(&mut *out, index);
                        out.push_str("    return NULL;\n  }\n");
                        match key.as_str() {
                            "Int" => {
                                let _ = writeln!(
                                    out,
                                    "  int64_t __json_int_{index} = 0; if (!aura_json_parse_int({raw}, &__json_int_{index})) {{"
                                );
                                emit_cleanup(&mut *out, index);
                                out.push_str("    return NULL;\n  }\n");
                            }
                            "Bool" => {
                                let _ = writeln!(
                                    out,
                                    "  bool __json_bool_{index} = false; if (!aura_json_parse_bool({raw}, &__json_bool_{index})) {{"
                                );
                                emit_cleanup(&mut *out, index);
                                out.push_str("    return NULL;\n  }\n");
                            }
                            "String" => {
                                let _ = writeln!(
                                    out,
                                    "  const char *__json_string_{index} = aura_json_decode_string({raw}); if (__json_string_{index} == NULL) {{"
                                );
                                emit_cleanup(&mut *out, index);
                                out.push_str("    return NULL;\n  }\n");
                            }
                            _ => {}
                        }
                    }
                    let ctor_args = field_keys
                        .iter()
                        .enumerate()
                        .map(|(index, (_, key))| match key.as_str() {
                            "Int" => format!("__json_int_{index}"),
                            "Bool" => format!("__json_bool_{index}"),
                            "String" => format!("__json_string_{index}"),
                            _ => "0".into(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let _ = writeln!(
                        out,
                        "  {c_class} *__decoded = {ctor}({ctor_args});",
                        c_class = c_class_type(&mono)
                    );
                    for (index, (_, key)) in field_keys.iter().enumerate() {
                        let _ = writeln!(out, "  free((void *)__json_field_{index});");
                        if key == "String" {
                            let _ = writeln!(out, "  free((void *)__json_string_{index});");
                        }
                    }
                    out.push_str("  return __decoded;\n}\n");
                    return;
                }
            }
        }
        if name == "String" {
            let _ = writeln!(out, "  return aura_json_decode_string((*{value}).text);");
            out.push_str("}\n");
            return;
        }
        if name == "Int" {
            out.push_str(
                "  aura_opt_i64 __decoded = { .has = false, .value = 0 }; int64_t __number = 0;\n",
            );
            let _ = writeln!(out, "  if (aura_json_parse_int((*{value}).text, &__number)) __decoded = (aura_opt_i64){{ .has = true, .value = __number }};");
            out.push_str("  return __decoded;\n}\n");
            return;
        }
        if name == "Bool" {
            out.push_str("  aura_opt_bool __decoded = { .has = false, .value = false }; bool __boolean = false;\n");
            let _ = writeln!(out, "  if (aura_json_parse_bool((*{value}).text, &__boolean)) __decoded = (aura_opt_bool){{ .has = true, .value = __boolean }};");
            out.push_str("  return __decoded;\n}\n");
            return;
        }
        if name.ends_with("std_json_Value") || name == "Value" {
            let _ = writeln!(out, "  return aura_method_std_json_Value_clone({value});");
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.compress" {
        if f.name.name == "compress" && f.params.len() == 2 {
            let value = mangle_ident(&f.params[0].name.name);
            let settings = mangle_ident(&f.params[1].name.name);
            let _ = writeln!(out, "  return aura_compress_text({value}, (int64_t)((*{settings}).codec.tag), (int64_t)((*{settings}).level));");
            out.push_str("}\n");
            return;
        }
        if f.name.name == "decompress" && f.params.len() == 2 {
            let value = mangle_ident(&f.params[0].name.name);
            let codec = mangle_ident(&f.params[1].name.name);
            let _ = writeln!(
                out,
                "  return aura_decompress_text({value}, (int64_t){codec}.tag);"
            );
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.dns" && f.name.name == "resolveHost" && f.params.len() == 2 {
        let host = mangle_ident(&f.params[0].name.name);
        let prefer = mangle_ident(&f.params[1].name.name);
        let _ = writeln!(
            out,
            "  return aura_dns_resolve_host({host}, {prefer} ? 1 : 0);"
        );
        out.push_str("}\n");
        return;
    }
    if pkg == "std.dns" && f.name.name == "resolveHostList" && f.params.len() == 2 {
        let host = mangle_ident(&f.params[0].name.name);
        let prefer = mangle_ident(&f.params[1].name.name);
        let _ = writeln!(
            out,
            "  return aura_dns_resolve_host_list({host}, {prefer} ? 1 : 0);"
        );
        out.push_str("}\n");
        return;
    }
    if pkg == "std.json" && f.params.len() == 1 {
        let value = mangle_ident(&f.params[0].name.name);
        match f.name.name.as_str() {
            "isValid" => {
                let _ = writeln!(out, "  return aura_json_is_valid({value});");
                out.push_str("}\n");
                return;
            }
            "errorOffset" => {
                let _ = writeln!(out, "  return aura_json_error_offset({value});");
                out.push_str("}\n");
                return;
            }
            "escapeString" => {
                let _ = writeln!(out, "  return aura_json_escape_string({value});");
                out.push_str("}\n");
                return;
            }
            "jsonArrayCount" => {
                let _ = writeln!(out, "  return aura_json_array_count({value});");
                out.push_str("}\n");
                return;
            }
            "jsonObjectKeys" => {
                let _ = writeln!(out, "  return aura_json_object_keys({value});");
                out.push_str("}\n");
                return;
            }
            "jsonDecodeString" => {
                let _ = writeln!(out, "  return aura_json_decode_string({value});");
                out.push_str("}\n");
                return;
            }
            "jsonDuplicateKey" => {
                let _ = writeln!(out, "  return aura_json_duplicate_key({value});");
                out.push_str("}\n");
                return;
            }
            _ => {}
        }
    }
    if pkg == "std.json" && f.params.len() == 2 {
        let value = mangle_ident(&f.params[0].name.name);
        let second = mangle_ident(&f.params[1].name.name);
        match f.name.name.as_str() {
            "jsonObjectGet" => {
                let _ = writeln!(out, "  return aura_json_object_get({value}, {second});");
                out.push_str("}\n");
                return;
            }
            "jsonArrayAt" => {
                let _ = writeln!(out, "  return aura_json_array_at({value}, {second});");
                out.push_str("}\n");
                return;
            }
            _ => {}
        }
    }
    if pkg == "std.signal" {
        match (f.name.name.as_str(), f.params.len()) {
            ("installShutdown", 0) => {
                out.push_str("  return aura_signal_install_shutdown() != 0;\n}\n");
                return;
            }
            ("shutdownRequested", 0) => {
                out.push_str("  return aura_signal_shutdown_requested();\n}\n");
                return;
            }
            ("clearShutdown", 0) => {
                out.push_str("  aura_signal_clear_shutdown();\n}\n");
                return;
            }
            _ => {}
        }
    }
    if pkg == "std.error" && f.name.name == "kindCode" && f.params.len() == 1 {
        let code = mangle_ident(&f.params[0].name.name);
        let _ = writeln!(out, "  return aura_error_kind_code({code});");
        out.push_str("}\n");
        return;
    }
    if pkg == "std.log" && f.params.len() == 1 {
        let message = mangle_ident(&f.params[0].name.name);
        let level = match f.name.name.as_str() {
            "debug" => Some(0),
            "info" => Some(1),
            "warn" => Some(2),
            "error" => Some(3),
            _ => None,
        };
        if let Some(level) = level {
            let _ = writeln!(out, "  aura_log({level}, {message});");
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.log" {
        if f.name.name == "setMinLevel" && f.params.len() == 1 {
            let level = mangle_ident(&f.params[0].name.name);
            let _ = writeln!(out, "  return aura_log_set_min_level({level});");
            out.push_str("}\n");
            return;
        }
        if f.name.name == "minLevel" && f.params.is_empty() {
            out.push_str("  return aura_log_get_min_level();\n}\n");
            return;
        }
    }
    if pkg == "std.udp" {
        if f.name.name == "bind" && f.params.len() == 1 {
            let endpoint = mangle_ident(&f.params[0].name.name);
            let _ = writeln!(out, "  if ({endpoint} == NULL || !aura_udp_bind({endpoint}->host, {endpoint}->port)) {{ aura_throw_string(\"std.udp.bind failed\"); return NULL; }}");
            let _ = writeln!(out, "  return aura_new_std_udp_Socket({endpoint});");
            out.push_str("}\n");
            return;
        }
        if f.name.name == "close" && f.params.len() == 1 {
            let this = mangle_ident(&f.params[0].name.name);
            let _ = writeln!(out, "  if ({this} != NULL && {this}->endpoint != NULL) (void)aura_udp_close({this}->endpoint->host, {this}->endpoint->port);");
            out.push_str("}\n");
            return;
        }
    }
    if pkg == "std.net" {
        if let ("closeListener", 1) = (f.name.name.as_str(), f.params.len()) {
            let listener = mangle_ident(&f.params[0].name.name);
            out.push_str("  AuraFfiHandlePin __pin = {0}; if (");
            out.push_str(&listener);
            out.push_str(" == NULL || aura_ffi_handle_pin_for_boundary(");
            out.push_str(&listener);
            out.push_str(", AURA_FFI_BOUNDARY_SYNC, &__pin) != AURA_FFI_OK) return false; (void)aura_tcp_listener_close((AuraTcpListener *)__pin.resource); (void)aura_ffi_handle_unpin(&__pin); return true;\n}\n");
            return;
        }
        if let ("closeStream", 1) = (f.name.name.as_str(), f.params.len()) {
            let stream = mangle_ident(&f.params[0].name.name);
            out.push_str("  AuraFfiHandlePin __pin = {0}; if (");
            out.push_str(&stream);
            out.push_str(" == NULL || aura_ffi_handle_pin_for_boundary(");
            out.push_str(&stream);
            out.push_str(", AURA_FFI_BOUNDARY_SYNC, &__pin) != AURA_FFI_OK) return false; (void)aura_tcp_stream_close((AuraTcpStream *)__pin.resource); (void)aura_ffi_handle_unpin(&__pin); return true;\n}\n");
            return;
        }
        if let ("listen", 1) = (f.name.name.as_str(), f.params.len()) {
            let endpoint = mangle_ident(&f.params[0].name.name);
            out.push_str(
                "  AuraTcpListener *__listener = NULL; AuraFfiOpaqueHandle *__handle = NULL; uint16_t __bound_port = 0;\n",
            );
            let _ = writeln!(
                out,
                "  if ({endpoint} == NULL || aura_tcp_listener_bind_endpoint({endpoint}, &__bound_port, &__listener) != AURA_TCP_OK || __listener == NULL) {{ aura_throw_string(\"std.net.listen failed\"); return NULL; }}"
            );
            out.push_str("  if (aura_ffi_handle_new((void *)__listener, aura_destroy_tcp_listener_resource, &__handle) != AURA_FFI_OK) { aura_tcp_listener_destroy(__listener); aura_throw_string(\"std.net.listen failed\"); return NULL; }\n");
            out.push_str("  return __handle;\n}\n");
            return;
        }
        if let ("connect", 2) = (f.name.name.as_str(), f.params.len()) {
            let endpoint = mangle_ident(&f.params[0].name.name);
            let timeout = mangle_ident(&f.params[1].name.name);
            out.push_str(
                "  AuraTcpStream *__stream = NULL; AuraFfiOpaqueHandle *__handle = NULL;\n",
            );
            let _ = writeln!(
                out,
                "  if ({endpoint} == NULL || aura_tcp_stream_connect_endpoint({endpoint}, (int){timeout}, &__stream) != AURA_TCP_OK || __stream == NULL) {{ aura_throw_string(\"std.net.connect failed\"); return NULL; }}"
            );
            out.push_str("  if (aura_ffi_handle_new((void *)__stream, aura_destroy_tcp_stream_resource, &__handle) != AURA_FFI_OK) { aura_tcp_stream_destroy(__stream); aura_throw_string(\"std.net.connect failed\"); return NULL; }\n");
            out.push_str("  return __handle;\n}\n");
            return;
        }
    }
    if pkg == "std.http" && !f.params.is_empty() {
        let handle = mangle_ident(&f.params[0].name.name);
        match (f.name.name.as_str(), f.params.len()) {
            ("requestMethod", 1) => {
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http request handle is invalid\"); return NULL; } const char *__value = aura_http_request_method((const AuraHttpRequest *)__pin.resource); size_t __length = __value == NULL ? 0 : strlen(__value); char *__copy = aura_http_copy_bytes(__value, __length); (void)aura_ffi_handle_unpin(&__pin); if (__copy == NULL) { aura_throw_string(\"http request method allocation failed\"); return NULL; } return __copy;\n}");
                return;
            }
            ("requestTarget", 1) => {
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http request handle is invalid\"); return NULL; } const char *__value = aura_http_request_target((const AuraHttpRequest *)__pin.resource); size_t __length = __value == NULL ? 0 : strlen(__value); char *__copy = aura_http_copy_bytes(__value, __length); (void)aura_ffi_handle_unpin(&__pin); if (__copy == NULL) { aura_throw_string(\"http request target allocation failed\"); return NULL; } return __copy;\n}");
                return;
            }
            ("requestVersion", 1) => {
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http request handle is invalid\"); return NULL; } const char *__value = aura_http_request_version((const AuraHttpRequest *)__pin.resource); size_t __length = __value == NULL ? 0 : strlen(__value); char *__copy = aura_http_copy_bytes(__value, __length); (void)aura_ffi_handle_unpin(&__pin); if (__copy == NULL) { aura_throw_string(\"http request version allocation failed\"); return NULL; } return __copy;\n}");
                return;
            }
            ("requestHeaderCount", 1) => {
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http request handle is invalid\"); return 0; } int64_t __count = (int64_t)aura_http_request_header_count((const AuraHttpRequest *)__pin.resource); (void)aura_ffi_handle_unpin(&__pin); return __count;\n}");
                return;
            }
            ("requestHeaderName", 2) | ("requestHeaderValue", 2) => {
                let index = mangle_ident(&f.params[1].name.name);
                let accessor = if f.name.name == "requestHeaderName" {
                    "aura_http_request_header_name"
                } else {
                    "aura_http_request_header_value"
                };
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http request handle is invalid\"); return NULL; } const char *__value = ");
                out.push_str(accessor);
                out.push_str("((const AuraHttpRequest *)__pin.resource, ");
                out.push_str(&index);
                out.push_str(" < 0 ? SIZE_MAX : (size_t)");
                out.push_str(&index);
                out.push_str("); size_t __length = __value == NULL ? 0 : strlen(__value); char *__copy = aura_http_copy_bytes(__value, __length); (void)aura_ffi_handle_unpin(&__pin); if (__copy == NULL) { aura_throw_string(\"http request header allocation failed\"); return NULL; } return __copy;\n}");
                return;
            }
            ("requestBody", 1) => {
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http request handle is invalid\"); return NULL; } const unsigned char *__value = aura_http_request_body((const AuraHttpRequest *)__pin.resource); size_t __length = aura_http_request_body_length((const AuraHttpRequest *)__pin.resource); char *__copy = aura_http_copy_bytes(__value, __length); (void)aura_ffi_handle_unpin(&__pin); if (__copy == NULL) { aura_throw_string(\"http request body allocation failed\"); return NULL; } return __copy;\n}");
                return;
            }
            ("responseStatus", 1) => {
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http response handle is invalid\"); return 0; } int64_t __status = (int64_t)aura_http_response_status((const AuraHttpResponse *)__pin.resource); (void)aura_ffi_handle_unpin(&__pin); return __status;\n}");
                return;
            }
            ("responseKeepAlive", 1) => {
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http response handle is invalid\"); return false; } _Bool __keep_alive = aura_http_response_keep_alive((const AuraHttpResponse *)__pin.resource) != 0; (void)aura_ffi_handle_unpin(&__pin); return __keep_alive;\n}");
                return;
            }
            ("responseSetStatus", 2) => {
                let status = mangle_ident(&f.params[1].name.name);
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http response handle is invalid\"); return false; } _Bool __ok = aura_http_response_set_status((AuraHttpResponse *)__pin.resource, (int)");
                out.push_str(&status);
                out.push_str(") == AURA_HTTP_RESPONSE_OK; (void)aura_ffi_handle_unpin(&__pin); return __ok;\n}");
                return;
            }
            ("responseSetKeepAlive", 2) => {
                let keep_alive = mangle_ident(&f.params[1].name.name);
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http response handle is invalid\"); return false; } _Bool __ok = aura_http_response_set_connection((AuraHttpResponse *)__pin.resource, ");
                out.push_str(&keep_alive);
                out.push_str(" ? AURA_HTTP_RESPONSE_KEEP_ALIVE : AURA_HTTP_RESPONSE_CLOSE) == AURA_HTTP_RESPONSE_OK; (void)aura_ffi_handle_unpin(&__pin); return __ok;\n}");
                return;
            }
            ("responseSetBody", 2) => {
                let body = mangle_ident(&f.params[1].name.name);
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http response handle is invalid\"); return false; } const char *__body = ");
                out.push_str(&body);
                out.push_str(" == NULL ? \"\" : ");
                out.push_str(&body);
                out.push_str("; _Bool __ok = aura_http_response_set_body((AuraHttpResponse *)__pin.resource, __body, strlen(__body)) == AURA_HTTP_RESPONSE_OK; (void)aura_ffi_handle_unpin(&__pin); return __ok;\n}");
                return;
            }
            ("responseAddHeader", 3) => {
                let name = mangle_ident(&f.params[1].name.name);
                let value = mangle_ident(&f.params[2].name.name);
                out.push_str("  AuraFfiHandlePin __pin = {0}; if (!aura_http_pin_resource(");
                out.push_str(&handle);
                out.push_str(", &__pin)) { aura_throw_string(\"http response handle is invalid\"); return false; } _Bool __ok = aura_http_response_add_header((AuraHttpResponse *)__pin.resource, ");
                out.push_str(&name);
                out.push_str(", ");
                out.push_str(&value);
                out.push_str(") == AURA_HTTP_RESPONSE_OK; (void)aura_ffi_handle_unpin(&__pin); return __ok;\n}");
                return;
            }
            _ => {}
        }
    }
    // C4h: std.assert.assert → aura_assert.
    if pkg == "std.assert" && f.name.name == "assert" && f.params.len() == 1 {
        let arg = mangle_ident(&f.params[0].name.name);
        let _ = writeln!(out, "  aura_assert({arg});");
        out.push_str("}\n");
        return;
    }
    if pkg == "std.test" {
        let intrinsic = match (f.name.name.as_str(), f.params.len()) {
            ("assert", 1) => Some(("aura_assert", vec![mangle_ident(&f.params[0].name.name)])),
            ("assertEqInt", 2) => Some((
                "aura_assert_eq_int",
                vec![
                    mangle_ident(&f.params[0].name.name),
                    mangle_ident(&f.params[1].name.name),
                ],
            )),
            ("assertEqString", 2) => Some((
                "aura_assert_eq_string",
                vec![
                    mangle_ident(&f.params[0].name.name),
                    mangle_ident(&f.params[1].name.name),
                ],
            )),
            ("assertEqBool", 2) => Some((
                "aura_assert_eq_bool",
                vec![
                    mangle_ident(&f.params[0].name.name),
                    mangle_ident(&f.params[1].name.name),
                ],
            )),
            _ => None,
        };
        if let Some((name, args)) = intrinsic {
            let _ = writeln!(out, "  {name}({});", args.join(", "));
            out.push_str("}\n");
            return;
        }
    }
    let ret_key = f
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, &params, args, checked));
    let mut ctx = EmitCtx {
        checked,
        detector,
        method_class: None,
        type_params: params.clone(),
        type_args: args.to_vec(),
        locals: vec![HashMap::new()],
        local_c_names: HashMap::new(),
        array_owners: vec![std::collections::HashSet::new()],
        fun_owners: vec![std::collections::HashSet::new()],
        string_owners: vec![std::collections::HashSet::new()],
        channel_owners: vec![std::collections::HashSet::new()],
        task_result_owners: vec![std::collections::HashSet::new()],
        task_handle_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![std::collections::HashSet::new()],
        box_owners: vec![std::collections::HashSet::new()],
        gc_roots: vec![std::collections::HashSet::new()],
        array_gc_roots: vec![std::collections::HashSet::new()],
        return_key: ret_key,
        lambda_ids: build_lambda_ids(checked),
        spawn_params: f.params.iter().map(|p| p.name.name.clone()).collect(),
        mutable_spawn_captures: mutable_spawn_capture_names(&f.body),
        async_frame: None,
        task_poller: false,
    };
    for p in &f.params {
        let key = type_ref_local_key_expand(&p.ty, &params, args, checked);
        let mono_key = full_type_mono(&key, checked);
        ctx.define_local(&p.name.name, mono_key.clone());
        // C6b/C21d: owning Array params own the buffer; `ref Array<T>` params
        // are header views over their caller's storage and must not free it.
        if !p.ty.reference && crate::array_emit::is_array_type_key(&key) {
            ctx.mark_array_owner(&p.name.name);
        }
        // Fun params own capture env (caller moves).
        if is_fun_type_key(&key) {
            ctx.mark_fun_owner(&p.name.name);
        }
        // C5g: heap-class params are GC roots for the function body.
        if is_heap_class_mono(&mono_key, checked) {
            ctx.mark_gc_root(&p.name.name);
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(out, "  aura_gc_add_root((void **)&{n});");
        }
        // C6e: Array-of-class params keep element GC pointers alive.
        if crate::array_emit::is_array_of_heap_class(&mono_key, checked) {
            ctx.mark_array_gc_root(&p.name.name);
            let n = mangle_ident(&p.name.name);
            let root = crate::array_emit::array_gc_root_add_call(
                &format!("{n}.data"),
                &format!("{n}.len"),
                &mono_key,
                checked,
            );
            let _ = writeln!(out, "  {root}");
        }
    }
    emit_block(out, &f.body, 1, &mut ctx);
    crate::stmt::emit_release_task_handle_owners(out, 1, &ctx, &ctx.task_handle_owners_all());
    // Function parameters live in the outer emission scope, so the block
    // cleanup above does not release owning Array parameters that were not
    // moved or returned. Keep the parameter ownership invariant symmetric
    // with the nested-block cleanup path.
    let array_owners = ctx.array_owners_all();
    crate::stmt::emit_free_array_owners(out, 1, &ctx, &array_owners);
    // Drop param roots when leaving the function.
    for name in ctx.array_gc_roots_all() {
        let n = mangle_ident(&name);
        let _ = writeln!(out, "  aura_gc_remove_array_root((void **)&{n}.data);");
    }
    for name in ctx.gc_roots_all() {
        let n = if name == "this" {
            "this".to_string()
        } else {
            mangle_ident(&name)
        };
        let _ = writeln!(out, "  aura_gc_remove_root((void **)&{n});");
    }
    // Free remaining Fun capture envs (params / locals not transferred by return).
    crate::stmt::emit_free_fun_owners(out, 1, &ctx, &ctx.fun_owners_all());
    // C12m: release remaining by-ref boxes (outer retain).
    crate::stmt::emit_release_box_locals(out, 1, &ctx, &ctx.box_owners_all());
    emit_return_fallback(out, &f.return_type, checked, &params, args);
    emit_c_type_fallback(
        out,
        &c_type_from_opt(&f.return_type, checked, &params, args),
    );
    out.push_str("}\n");
}
