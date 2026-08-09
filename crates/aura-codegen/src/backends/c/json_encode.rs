//! C emission for typed `std.json` encode paths.

use super::*;

use aura_ir::intrinsic_registry::{lookup as lookup_std_intrinsic, Intrinsic as StdIntrinsic};

pub(super) fn emit_json_encode_class(
    out: &mut String,
    value: &str,
    target: &Ty,
    checked: &CheckedFile,
) -> bool {
    let (name, class_args) = match target {
        Ty::Class(name) => (name.clone(), Vec::new()),
        Ty::ClassApp { name, args } => (name.clone(), args.clone()),
        _ => return false,
    };
    let base = aura_sema::split_nominal(&name).0;
    if base == "Map" {
        return false;
    }
    let Some(class) = checked.ast.classes.iter().find(|candidate| {
        candidate.name.name == base
            && class_decl_package(candidate, checked) == aura_sema::split_nominal(&name).1
    }) else {
        return false;
    };
    let is_heap = is_heap_class_decl(class);
    let params = class
        .type_params
        .iter()
        .map(|param| param.name.name.clone())
        .collect::<Vec<_>>();
    let mut field_keys = Vec::with_capacity(class.fields.len());
    for field in &class.fields {
        let raw_key = type_ref_local_key_expand(&field.ty, &params, &class_args, checked);
        let key = crate::expr::task_payload_repr_key(&raw_key);
        if !json_encode_key_supported(&key, checked, 0) {
            return false;
        }
        field_keys.push((field, key));
    }
    if is_heap {
        let _ = writeln!(out, "  if ({value} == NULL) return NULL;");
    }
    let count = class.fields.len();
    let _ = writeln!(out, "  const char *__json_values[{}];", count.max(1));
    out.push_str("  int64_t __json_encoded_count = 0;\n");
    for (index, (field, key)) in field_keys.iter().enumerate() {
        let access = if is_heap {
            format!("({value})->{}", mangle_ident(&field.name.name))
        } else {
            format!("({value}).{}", mangle_ident(&field.name.name))
        };
        if field.ty.nullable && !matches!(key.as_str(), "Opt_Int" | "Opt_Bool") {
            let condition = json_nullable_null_condition(&access, key, checked);
            let _ = writeln!(
                out,
                "  if ({condition}) __json_values[{index}] = aura_json_encode_null(); else {{"
            );
            emit_json_encode_value(
                out,
                &format!("__json_values[{index}]"),
                &access,
                key,
                checked,
                &format!("field_{index}"),
            );
            out.push_str("  }\n");
        } else {
            emit_json_encode_value(
                out,
                &format!("__json_values[{index}]"),
                &access,
                key,
                checked,
                &format!("field_{index}"),
            );
        }
        let _ = writeln!(out, "  if (__json_values[{index}] == NULL) goto __json_encode_fail; __json_encoded_count++;" );
    }
    out.push_str("  const char *__json_encoded = aura_json_encode_object((const char *[]) {");
    for (index, (field, _)) in field_keys.iter().enumerate() {
        if index != 0 {
            out.push_str(", ");
        }
        let name = json_field_name(field)
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let _ = write!(out, "\"{name}\"");
    }
    out.push_str("}, __json_values, ");
    let _ = writeln!(out, "{count});");
    out.push_str("  for (int64_t __json_i = 0; __json_i < __json_encoded_count; __json_i++) free((void *)__json_values[__json_i]);\n");
    out.push_str("  return __json_encoded;\n");
    out.push_str("__json_encode_fail:\n");
    out.push_str("  for (int64_t __json_i = 0; __json_i < __json_encoded_count; __json_i++) free((void *)__json_values[__json_i]);\n");
    out.push_str("  return NULL;\n");
    true
}

pub(super) fn json_encode_key_supported(key: &str, checked: &CheckedFile, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    if matches!(
        key,
        "Int" | "Float" | "Bool" | "String" | "Opt_Int" | "Opt_Float" | "Opt_Bool"
    ) {
        return true;
    }
    if let Some(element) = crate::expr::array_elem_local_key(key, checked) {
        return json_encode_key_supported(&element, checked, depth + 1);
    }
    let full = crate::expr::full_type_mono(key, checked);
    let base = crate::expr::mono_base_name(&full, checked);
    let Some(base) = base else {
        return false;
    };
    if base == "Map" {
        let args = crate::expr::mono_split(&full, checked)
            .map(|(_, args)| args)
            .unwrap_or(&[]);
        return matches!(args.first(), Some(Ty::String))
            && args.get(1).is_some_and(|value| {
                json_encode_key_supported(&value.mono_suffix(), checked, depth + 1)
            });
    }
    if let Some(enumeration) = checked.ast.enums.iter().find(|e| {
        type_mono(&enum_decl_package(e, checked), &e.name.name, &[]) == full || e.name.name == base
    }) {
        let enum_args = crate::expr::mono_split(&full, checked)
            .map(|(_, args)| args)
            .unwrap_or(&[]);
        let params = enumeration
            .type_params
            .iter()
            .map(|param| param.name.name.clone())
            .collect::<Vec<_>>();
        if enumeration.variants.iter().all(|variant| {
            variant.fields.iter().all(|field| {
                json_encode_key_supported(
                    &type_ref_local_key_expand(&field.ty, &params, enum_args, checked),
                    checked,
                    depth + 1,
                )
            })
        }) {
            return true;
        }
    }
    let Some(class) = checked
        .ast
        .classes
        .iter()
        .find(|class| {
            class.name.name == base
                && type_mono(&class_decl_package(class, checked), base, &[]) == full
        })
        .or_else(|| {
            checked
                .ast
                .classes
                .iter()
                .find(|class| class.name.name == base)
        })
    else {
        return false;
    };
    let class_args = crate::expr::mono_split(&full, checked)
        .map(|(_, args)| args)
        .unwrap_or(&[]);
    let params = class
        .type_params
        .iter()
        .map(|param| param.name.name.clone())
        .collect::<Vec<_>>();
    class.fields.iter().all(|field| {
        json_encode_key_supported(
            &type_ref_local_key_expand(&field.ty, &params, class_args, checked),
            checked,
            depth + 1,
        )
    })
}

pub(super) fn json_nullable_null_condition(
    access: &str,
    key: &str,
    checked: &CheckedFile,
) -> String {
    if crate::array_emit::is_array_type_key(key) {
        return format!("({access}).len < 0");
    }
    let full = crate::expr::full_type_mono(key, checked);
    if crate::expr::is_enum_mono(&full, checked) {
        return format!("({access}).tag < 0");
    }
    if crate::expr::is_value_struct_mono(&full, checked) {
        return format!("({access}).__aura_present == 0");
    }
    format!("{access} == NULL")
}

pub(super) fn emit_json_encode_value(
    out: &mut String,
    destination: &str,
    value: &str,
    key: &str,
    checked: &CheckedFile,
    path: &str,
) {
    match key {
        "Opt_Int" => {
            let _ = writeln!(
                out,
                "  {destination} = ({value}).has ? aura_json_encode_int(({value}).value) : aura_json_encode_null();"
            );
        }
        "Opt_Bool" => {
            let _ = writeln!(
                out,
                "  {destination} = ({value}).has ? aura_json_encode_bool(({value}).value) : aura_json_encode_null();"
            );
        }
        "Int" => {
            let _ = writeln!(out, "  {destination} = aura_json_encode_int({value});");
        }
        "Bool" => {
            let _ = writeln!(out, "  {destination} = aura_json_encode_bool({value});");
        }
        "String" => {
            let _ = writeln!(out, "  {destination} = aura_json_escape_string({value});");
        }
        _ if crate::expr::array_elem_local_key(key, checked).is_some() => {
            let element = crate::expr::array_elem_local_key(key, checked).unwrap();
            let values = format!("__json_values_{path}");
            let count = format!("__json_count_{path}");
            let index = format!("__json_index_{path}");
            let _ = writeln!(out, "  const char **{values} = (const char **)malloc(((({value}).len > 0 ? ({value}).len : 1)) * sizeof(*{values}));");
            let _ = writeln!(
                out,
                "  int64_t {count} = 0; if ({values} == NULL) {{ {destination} = NULL; }} else {{"
            );
            let elem_value = if is_heap_class_mono(&element, checked) {
                format!("({value}).data[{index}]")
            } else {
                format!("({value}).data[{index}]")
            };
            let _ = writeln!(out, "  for (int64_t {index} = 0; {index} < ({value}).len && {values} != NULL; {index}++) {{");
            emit_json_encode_value(
                out,
                &format!("{values}[{index}]"),
                &elem_value,
                &element,
                checked,
                &format!("{path}_item"),
            );
            let _ = writeln!(out, "    if ({values}[{index}] == NULL) {{ for (int64_t __j = 0; __j < {count}; __j++) free((void *){values}[__j]); {destination} = NULL; break; }}");
            let _ = writeln!(out, "    {count}++;");
            let _ = writeln!(out, "  }}");
            let _ = writeln!(out, "  if ({values} != NULL && {count} == ({value}).len) {destination} = aura_json_encode_array({values}, ({value}).len);");
            let _ = writeln!(out, "  if ({values} != NULL) {{ for (int64_t __j = 0; __j < {count}; __j++) free((void *){values}[__j]); free({values}); }} }}");
        }
        _ => {
            let full = crate::expr::full_type_mono(key, checked);
            if let Some(enumeration) = checked.ast.enums.iter().find(|e| {
                type_mono(&enum_decl_package(e, checked), &e.name.name, &[]) == full
                    || e.name.name
                        == crate::expr::mono_base_name(&full, checked).unwrap_or_default()
            }) {
                let enum_args = crate::expr::mono_split(&full, checked)
                    .map(|(_, args)| args)
                    .unwrap_or(&[]);
                let params = enumeration
                    .type_params
                    .iter()
                    .map(|param| param.name.name.clone())
                    .collect::<Vec<_>>();
                let _ = writeln!(out, "  switch (({value}).tag) {{");
                for (tag, variant) in enumeration.variants.iter().enumerate() {
                    let _ = writeln!(out, "    case {tag}: {{");
                    if variant.fields.is_empty() {
                        let _ = writeln!(
                            out,
                            "      {destination} = aura_json_escape_string(\"{}\");",
                            variant.name.name.replace('"', "\\\"")
                        );
                    } else {
                        let values = format!("__json_variant_values_{path}_{tag}");
                        let count = format!("__json_variant_count_{path}_{tag}");
                        let _ = writeln!(
                            out,
                            "      const char *{values}[{}]; int64_t {count} = 0;",
                            variant.fields.len().max(1)
                        );
                        for (index, field) in variant.fields.iter().enumerate() {
                            let field_key =
                                type_ref_local_key_expand(&field.ty, &params, enum_args, checked);
                            let access = format!(
                                "({value}).data.{}.{}",
                                mangle_ident(&variant.name.name),
                                mangle_ident(&field.name.name)
                            );
                            emit_json_encode_value(
                                out,
                                &format!("{values}[{index}]"),
                                &access,
                                &field_key,
                                checked,
                                &format!("{path}_{tag}_{index}"),
                            );
                            let _ = writeln!(out, "      if ({values}[{index}] == NULL) {{ for (int64_t __j = 0; __j < {count}; __j++) free((void *){values}[__j]); {destination} = NULL; break; }} {count}++;" );
                        }
                        let _ =
                            writeln!(out, "      if ({count} != {}) break;", variant.fields.len());
                        let _ = writeln!(out, "      {destination} = aura_json_encode_variant(\"{}\", (const char *[]) {{ {} }}, {values}, {});", variant.name.name.replace('"', "\\\""), variant.fields.iter().map(|field| format!("\"{}\"", field.name.name.replace('"', "\\\""))).collect::<Vec<_>>().join(", "), variant.fields.len());
                        let _ = writeln!(out, "      for (int64_t __j = 0; __j < {count}; __j++) free((void *){values}[__j]);");
                    }
                    out.push_str("      break;\n    }\n");
                }
                let _ = writeln!(out, "    default: {destination} = NULL; break;\n  }}");
                return;
            }
            let base = crate::expr::mono_base_name(&full, checked).unwrap_or_default();
            let class = checked
                .ast
                .classes
                .iter()
                .find(|class| class.name.name == base)
                .expect("supported JSON class");
            let class_args = crate::expr::mono_split(&full, checked)
                .map(|(_, args)| args)
                .unwrap_or(&[]);
            let params = class
                .type_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect::<Vec<_>>();
            let heap = is_heap_class_decl(class);
            if base == "Map" {
                let map_values = format!("__json_map_values_{path}");
                let map_keys = format!("__json_map_keys_{path}");
                let map_count = format!("__json_map_count_{path}");
                let value_key = class_args
                    .get(1)
                    .map(|ty| ty.mono_suffix())
                    .unwrap_or_default();
                let map_expr = if heap {
                    format!("({value})")
                } else {
                    format!("&({value})")
                };
                let _ = writeln!(out, "  const char **{map_keys} = (const char **)malloc(((({map_expr})->keys).len > 0 ? (({map_expr})->keys).len : 1) * sizeof(*{map_keys}));");
                let _ = writeln!(out, "  const char **{map_values} = (const char **)malloc(((({map_expr})->keys).len > 0 ? (({map_expr})->keys).len : 1) * sizeof(*{map_values}));");
                let _ = writeln!(out, "  int64_t {map_count} = 0; if ({map_keys} == NULL || {map_values} == NULL) {{ free({map_keys}); free({map_values}); {map_keys} = NULL; {map_values} = NULL; {destination} = NULL; }} else {{");
                let index = format!("__json_map_i_{path}");
                let map_value = format!("(({map_expr})->vals).data[{index}]");
                let _ = writeln!(out, "  for (int64_t {index} = 0; {index} < (({map_expr})->keys).len; {index}++) {{ {map_keys}[{index}] = (({map_expr})->keys).data[{index}];");
                emit_json_encode_value(
                    out,
                    &format!("{map_values}[{index}]"),
                    &map_value,
                    &value_key,
                    checked,
                    &format!("{path}_map_value"),
                );
                let _ = writeln!(out, "    if ({map_values}[{index}] == NULL) {{ for (int64_t __j = 0; __j < {map_count}; __j++) free((void *){map_values}[__j]); free({map_keys}); free({map_values}); {map_keys} = NULL; {map_values} = NULL; {destination} = NULL; break; }} {map_count}++; }}");
                let _ = writeln!(out, "  if ({map_keys} != NULL && {map_count} == (({map_expr})->keys).len) {destination} = aura_json_encode_object({map_keys}, {map_values}, (({map_expr})->keys).len);");
                let _ = writeln!(out, "  if ({map_keys} != NULL) {{ for (int64_t __j = 0; __j < {map_count}; __j++) free((void *){map_values}[__j]); free({map_keys}); free({map_values}); }} }}");
                return;
            }
            let values = format!("__json_fields_{path}");
            let count = format!("__json_field_count_{path}");
            let _ = writeln!(
                out,
                "  const char *{values}[{}]; int64_t {count} = 0;",
                class.fields.len().max(1)
            );
            for (index, field) in class.fields.iter().enumerate() {
                let access = if heap {
                    format!("({value})->{}", mangle_ident(&field.name.name))
                } else {
                    format!("({value}).{}", mangle_ident(&field.name.name))
                };
                let raw_field_key =
                    type_ref_local_key_expand(&field.ty, &params, class_args, checked);
                let field_key = crate::expr::task_payload_repr_key(&raw_field_key);
                if field.ty.nullable && !matches!(field_key.as_str(), "Opt_Int" | "Opt_Bool") {
                    let condition = json_nullable_null_condition(&access, &field_key, checked);
                    let _ = writeln!(
                        out,
                        "  if ({condition}) {values}[{index}] = aura_json_encode_null(); else {{"
                    );
                    emit_json_encode_value(
                        out,
                        &format!("{values}[{index}]"),
                        &access,
                        &field_key,
                        checked,
                        &format!("{path}_{index}"),
                    );
                    out.push_str("  }\n");
                } else {
                    emit_json_encode_value(
                        out,
                        &format!("{values}[{index}]"),
                        &access,
                        &field_key,
                        checked,
                        &format!("{path}_{index}"),
                    );
                }
                let _ = writeln!(out, "  if ({values}[{index}] == NULL) {{ for (int64_t __j = 0; __j < {count}; __j++) free((void *){values}[__j]); {destination} = NULL; goto __json_encode_value_fail_{path}; }} {count}++;" );
            }
            let _ = write!(
                out,
                "  {destination} = aura_json_encode_object((const char *[]) {{"
            );
            for (index, field) in class.fields.iter().enumerate() {
                if index != 0 {
                    out.push_str(", ");
                }
                let name = json_field_name(field).replace('"', "\\\"");
                let _ = write!(out, "\"{name}\"");
            }
            let _ = writeln!(out, "}}, {values}, {});", class.fields.len());
            let _ = writeln!(
                out,
                "  for (int64_t __j = 0; __j < {count}; __j++) free((void *){values}[__j]);"
            );
            let _ = writeln!(out, "__json_encode_value_fail_{path}: ;");
        }
    }
}

pub(super) fn emit_json_encode_array(
    out: &mut String,
    value: &str,
    target: &Ty,
    checked: &CheckedFile,
) -> bool {
    let key = crate::expr::full_type_mono(&target.mono_suffix(), checked);
    if !is_array_type_key(&key) {
        return false;
    }
    if !json_encode_key_supported(&key, checked, 0) {
        return false;
    }
    out.push_str("  const char *__json_encoded = NULL;\n");
    emit_json_encode_value(out, "__json_encoded", value, &key, checked, "root_array");
    out.push_str("  return __json_encoded;\n");
    true
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
    let is_spawn_blocking = lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::TaskSpawnBlocking)
        && f.name.name == "spawnBlocking"
        && f.params.len() == 1
        && args.len() == 1;
    let is_task_scope = lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::TaskScope)
        && f.params.len() == 1;
    let is_lazy = lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::SyncLazy)
        && f.name.name == "lazy"
        && f.params.len() == 1
        && args.len() == 1;
    let is_select = lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::TaskSelect)
        && f.params.is_empty()
        && args.len() == 1;
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Time)
        && f.params.is_empty()
    {
        out.push_str("  return aura_time_monotonic_millis();\n}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::TaskErrorMetadata)
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::TaskCancellation)
        && f.name.name == "cancelAfter"
        && f.params.len() == 2
    {
        let task = mangle_ident(&f.params[0].name.name);
        let timeout = mangle_ident(&f.params[1].name.name);
        let _ = writeln!(out, "  return {task} != NULL && aura_task_frame_set_cancel_deadline({task}, (int){timeout}) != 0;");
        out.push_str("}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::TaskCancellation)
        && f.name.name == "linkCancellation"
        && f.params.len() == 2
    {
        let parent = mangle_ident(&f.params[0].name.name);
        let child = mangle_ident(&f.params[1].name.name);
        let _ = writeln!(
            out,
            "  return aura_task_frame_link_cancellation({parent}, {child}) != 0;"
        );
        out.push_str("}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Encoding)
        && f.params.len() == 1
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Url)
        && f.params.len() == 1
        && f.name.name != "queryValue"
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Url)
        && f.name.name == "queryValue"
        && f.params.len() == 2
    {
        let target = mangle_ident(&f.params[0].name.name);
        let key = mangle_ident(&f.params[1].name.name);
        out.push_str(&format!(
            "  return aura_url_query_value({target}, {key});\n}}\n"
        ));
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Mime)
        && f.params.len() == 1
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Bytes)
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Fs)
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Os)
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name).is_some_and(|spec| {
        matches!(
            spec.intrinsic,
            StdIntrinsic::Io | StdIntrinsic::IoFd | StdIntrinsic::IoOpenFile
        )
    }) {
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
    if lookup_std_intrinsic(&pkg, &f.name.name).is_some_and(|spec| {
        matches!(
            spec.intrinsic,
            StdIntrinsic::Crypto | StdIntrinsic::CryptoRandomBytes
        )
    }) {
        match (f.name.name.as_str(), f.params.len()) {
            ("randomBytes", 1) => {
                let length = mangle_ident(&f.params[0].name.name);
                let _ = writeln!(out, "  return aura_crypto_random_bytes({length});");
                out.push_str("}\n");
                return;
            }
            ("randomBytesBuffer", 1) => {
                let length = mangle_ident(&f.params[0].name.name);
                out.push_str("  if (");
                let _ = writeln!(out, "{length} < 0 || {length} > INT64_MAX - 1) {{ aura_throw_string(\"random byte length is invalid\"); return NULL; }}");
                out.push_str("  size_t __length = (size_t)");
                out.push_str(&length);
                out.push_str("; uint8_t *__raw = (uint8_t *)malloc(__length == 0 ? 1 : __length); if (__raw == NULL || !aura_crypto_random_bytes_raw(__raw, __length)) { free(__raw); aura_throw_string(\"secure randomness unavailable\"); return NULL; } aura_cls_Array_Int __values = aura_new_Array_Int((int64_t)__length); for (int64_t __i = 0; __i < (int64_t)__length; __i++) __values.data[__i] = __raw[__i]; free(__raw); return aura_new_std_bytes_Buffer(__values);\n}");
                return;
            }
            ("md5Bytes", 1) | ("sha256Bytes", 1) => {
                let value = mangle_ident(&f.params[0].name.name);
                let is_md5 = f.name.name == "md5Bytes";
                let digest_len = if is_md5 { 16 } else { 32 };
                let digest_call = if is_md5 {
                    "aura_crypto_md5_bytes"
                } else {
                    "aura_crypto_sha256_bytes"
                };
                let _ = writeln!(out, "  if ({value} == NULL) {{ aura_throw_string(\"binary digest input is null\"); return NULL; }} size_t __input_length = (size_t){value}->values.len; uint8_t *__input = (uint8_t *)malloc(__input_length == 0 ? 1 : __input_length); if (__input == NULL) {{ aura_throw_string(\"binary digest allocation failed\"); return NULL; }} for (size_t __i = 0; __i < __input_length; __i++) __input[__i] = (uint8_t){value}->values.data[__i]; uint8_t __digest[{digest_len}]; if (!{digest_call}(__input, __input_length, __digest)) {{ free(__input); aura_throw_string(\"binary digest failed\"); return NULL; }} free(__input); aura_cls_Array_Int __values = aura_new_Array_Int(INT64_C({digest_len})); for (int64_t __i = 0; __i < INT64_C({digest_len}); __i++) __values.data[__i] = __digest[__i]; return aura_new_std_bytes_Buffer(__values);\n}}", value = value, digest_call = digest_call, digest_len = digest_len);
                return;
            }
            ("hmacSha256Bytes", 2) => {
                let key = mangle_ident(&f.params[0].name.name);
                let value = mangle_ident(&f.params[1].name.name);
                let _ = writeln!(out, "  if ({key} == NULL || {value} == NULL) {{ aura_throw_string(\"binary HMAC input is null\"); return NULL; }} size_t __key_length = (size_t){key}->values.len; size_t __value_length = (size_t){value}->values.len; uint8_t *__key_bytes = (uint8_t *)malloc(__key_length == 0 ? 1 : __key_length); uint8_t *__value_bytes = (uint8_t *)malloc(__value_length == 0 ? 1 : __value_length); if (__key_bytes == NULL || __value_bytes == NULL) {{ free(__key_bytes); free(__value_bytes); aura_throw_string(\"binary HMAC allocation failed\"); return NULL; }} for (size_t __i = 0; __i < __key_length; __i++) __key_bytes[__i] = (uint8_t){key}->values.data[__i]; for (size_t __i = 0; __i < __value_length; __i++) __value_bytes[__i] = (uint8_t){value}->values.data[__i]; uint8_t __digest[32]; if (!aura_crypto_hmac_sha256_bytes(__key_bytes, __key_length, __value_bytes, __value_length, __digest)) {{ free(__key_bytes); free(__value_bytes); aura_throw_string(\"binary HMAC failed\"); return NULL; }} free(__key_bytes); free(__value_bytes); aura_cls_Array_Int __values = aura_new_Array_Int(INT64_C(32)); for (int64_t __i = 0; __i < INT64_C(32); __i++) __values.data[__i] = __digest[__i]; return aura_new_std_bytes_Buffer(__values);\n}}", key = key, value = value);
                return;
            }
            ("pbkdf2Sha256", 4) => {
                let password = mangle_ident(&f.params[0].name.name);
                let salt = mangle_ident(&f.params[1].name.name);
                let iterations = mangle_ident(&f.params[2].name.name);
                let length = mangle_ident(&f.params[3].name.name);
                let _ = writeln!(out, "  if ({password} == NULL || {salt} == NULL || {iterations} <= 0 || {length} < 0 || {length} > INT64_MAX - 1) {{ aura_throw_string(\"invalid PBKDF2 input\"); return NULL; }} size_t __password_length = (size_t){password}->values.len; size_t __salt_length = (size_t){salt}->values.len; uint8_t *__password_bytes = (uint8_t *)malloc(__password_length == 0 ? 1 : __password_length); uint8_t *__salt_bytes = (uint8_t *)malloc(__salt_length == 0 ? 1 : __salt_length); if (__password_bytes == NULL || __salt_bytes == NULL) {{ free(__password_bytes); free(__salt_bytes); aura_throw_string(\"PBKDF2 allocation failed\"); return NULL; }} for (size_t __i = 0; __i < __password_length; __i++) __password_bytes[__i] = (uint8_t){password}->values.data[__i]; for (size_t __i = 0; __i < __salt_length; __i++) __salt_bytes[__i] = (uint8_t){salt}->values.data[__i]; size_t __length = (size_t){length}; uint8_t *__raw = (uint8_t *)malloc(__length == 0 ? 1 : __length); if (__raw == NULL || !aura_crypto_pbkdf2_sha256(__password_bytes, __password_length, __salt_bytes, __salt_length, (uint32_t){iterations}, __raw, __length)) {{ free(__password_bytes); free(__salt_bytes); free(__raw); aura_throw_string(\"PBKDF2 failed\"); return NULL; }} free(__password_bytes); free(__salt_bytes); aura_cls_Array_Int __values = aura_new_Array_Int((int64_t)__length); for (int64_t __i = 0; __i < (int64_t)__length; __i++) __values.data[__i] = __raw[__i]; free(__raw); return aura_new_std_bytes_Buffer(__values);\n}}", password = password, salt = salt, iterations = iterations, length = length);
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
            ("tlsConfig", 2) => {
                let server = mangle_ident(&f.params[0].name.name);
                let verify = mangle_ident(&f.params[1].name.name);
                out.push_str("  return aura_new_std_crypto_TlsConfig(");
                let _ = writeln!(out, "{server}, {verify});");
                out.push_str("}\n");
                return;
            }
            _ => {}
        }
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Tls)
    {
        if f.name.name == "wrapStream" && f.params.len() == 3 {
            let stream = mangle_ident(&f.params[0].name.name);
            let endpoint = mangle_ident(&f.params[1].name.name);
            let config = mangle_ident(&f.params[2].name.name);
            out.push_str("  AuraFfiHandlePin __pin = {0};\n");
            let _ = writeln!(out, "  if ({stream} == NULL || {endpoint} == NULL || {config} == NULL || aura_ffi_handle_pin_for_boundary({stream}, AURA_FFI_BOUNDARY_SYNC, &__pin) != AURA_FFI_OK || aura_ffi_handle_retain({stream}) != AURA_FFI_OK) {{ if (__pin.handle != NULL) (void)aura_ffi_handle_unpin(&__pin); aura_throw_string(\"TLS stream wrap failed\"); return NULL; }} if (!aura_tls_wrap_stream({endpoint}, (struct AuraTcpStream *)__pin.resource, {stream}, {config}->serverName, {config}->verifyPeer)) {{ (void)aura_ffi_handle_unpin(&__pin); char __message[320]; snprintf(__message, sizeof(__message), \"TLS stream wrap failed: %s\", aura_tls_last_error()); aura_throw_string(__message); return NULL; }} (void)aura_ffi_handle_unpin(&__pin); return aura_new_std_tls_Connection({endpoint});");
            out.push_str("}\n");
            return;
        }
        if f.name.name == "config" && f.params.len() == 2 {
            let server = mangle_ident(&f.params[0].name.name);
            let verify = mangle_ident(&f.params[1].name.name);
            let _ = writeln!(out, "  return aura_new_std_tls_Config({server}, {verify});");
            out.push_str("}\n");
            return;
        }
        if f.name.name == "loadCertificate" && f.params.len() == 1 {
            let path = mangle_ident(&f.params[0].name.name);
            out.push_str("  const char *__subject = aura_tls_certificate_subject(");
            out.push_str(&path);
            out.push_str("); const char *__issuer = aura_tls_certificate_issuer(");
            out.push_str(&path);
            out.push_str("); /* AURA_TLS_REQUIRED */ if (__subject == NULL || __issuer == NULL) { char __message[320]; snprintf(__message, sizeof(__message), \"certificate load failed: %s\", aura_tls_last_error()); aura_throw_string(__message); return NULL; } return aura_new_std_tls_Certificate(__subject, __issuer);\n}");
            return;
        }
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Reflect)
    {
        let has_reflect_attribute = |attributes: &[Attribute]| {
            attributes
                .iter()
                .any(|attribute| attribute.name.name == "reflect")
        };
        let type_ref_name = |type_ref: &TypeRef| {
            let mut name = type_ref.name.name.clone();
            if !type_ref.type_args.is_empty() {
                name.push('<');
                name.push_str(
                    &type_ref
                        .type_args
                        .iter()
                        .map(|arg| arg.name.name.clone())
                        .collect::<Vec<_>>()
                        .join(","),
                );
                name.push('>');
            }
            if type_ref.nullable {
                name.push('?');
            }
            name
        };
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
            for class in &checked.ast.classes {
                let kind = match class.kind {
                    NominalKind::Class => "Class",
                    NominalKind::Struct => "Struct",
                };
                let class_name = &class.name.name;
                let qualified_name =
                    type_mono(&class_decl_package(class, checked), class_name, &[]);
                let _ = writeln!(
                    out,
                    "  if ({value} != NULL && (strcmp({value}, \"{class_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0)) __kind = aura_var_std_reflect_TypeKind_{kind}();"
                );
                if has_reflect_attribute(&class.attributes) {
                    for (mono_name, args) in checked
                        .mono_classes
                        .iter()
                        .filter(|(name, args)| name == class_name && !args.iter().any(Ty::is_open))
                    {
                        let mono = type_mono(&class_decl_package(class, checked), class_name, args);
                        let _ = writeln!(
                            out,
                            "  if ({value} != NULL && strcmp({value}, \"{mono}\") == 0) __kind = aura_var_std_reflect_TypeKind_{kind}();"
                        );
                        let _ = mono_name;
                    }
                }
            }
            for enumeration in &checked.ast.enums {
                let enum_name = &enumeration.name.name;
                let qualified_name =
                    type_mono(&enum_decl_package(enumeration, checked), enum_name, &[]);
                let _ = writeln!(
                    out,
                    "  if ({value} != NULL && (strcmp({value}, \"{enum_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0)) __kind = aura_var_std_reflect_TypeKind_Enum();"
                );
            }
            for interface in &checked.ast.interfaces {
                let interface_name = &interface.name.name;
                let qualified_name =
                    type_mono(&iface_decl_package(interface, checked), interface_name, &[]);
                let _ = writeln!(
                    out,
                    "  if ({value} != NULL && (strcmp({value}, \"{interface_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0)) __kind = aura_var_std_reflect_TypeKind_Interface();"
                );
                for (_mono_name, args) in checked
                    .mono_interfaces
                    .iter()
                    .filter(|(name, args)| name == interface_name && !args.iter().any(Ty::is_open))
                {
                    let mono = type_mono(
                        &iface_decl_package(interface, checked),
                        interface_name,
                        args,
                    );
                    let _ = writeln!(
                        out,
                        "  if ({value} != NULL && strcmp({value}, \"{mono}\") == 0) __kind = aura_var_std_reflect_TypeKind_Interface();"
                    );
                }
            }
            for function in &checked.ast.functions {
                let function_name = &function.name.name;
                let _ = writeln!(
                    out,
                    "  if ({value} != NULL && strcmp({value}, \"{function_name}\") == 0) __kind = aura_var_std_reflect_TypeKind_Function();"
                );
            }
            let _ = writeln!(
                out,
                "  return aura_new_std_reflect_TypeInfo(aura_bytes_copy({value}), __kind);"
            );
            out.push_str("}\n");
            return;
        }
        if f.name.name == "isReflectable" && f.params.len() == 1 {
            let value = mangle_ident(&f.params[0].name.name);
            out.push_str("  return ");
            let _ = write!(
                out,
                "{value} != NULL && (strcmp({value}, \"Int\") == 0 || strcmp({value}, \"Bool\") == 0 || strcmp({value}, \"String\") == 0 || strcmp({value}, \"Unit\") == 0"
            );
            for class in &checked.ast.classes {
                if has_reflect_attribute(&class.attributes) {
                    let class_name = &class.name.name;
                    let qualified_name =
                        type_mono(&class_decl_package(class, checked), class_name, &[]);
                    let _ = write!(
                        out,
                        " || strcmp({value}, \"{class_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0"
                    );
                    for (mono_name, args) in checked
                        .mono_classes
                        .iter()
                        .filter(|(name, args)| name == class_name && !args.iter().any(Ty::is_open))
                    {
                        let mono = type_mono(&class_decl_package(class, checked), class_name, args);
                        let _ = write!(out, " || strcmp({value}, \"{mono}\") == 0");
                        let _ = mono_name;
                    }
                }
            }
            for enumeration in &checked.ast.enums {
                if has_reflect_attribute(&enumeration.attributes) {
                    let enum_name = &enumeration.name.name;
                    let qualified_name =
                        type_mono(&enum_decl_package(enumeration, checked), enum_name, &[]);
                    let _ = write!(
                        out,
                        " || strcmp({value}, \"{enum_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0"
                    );
                }
            }
            for interface in &checked.ast.interfaces {
                if has_reflect_attribute(&interface.attributes) {
                    let interface_name = &interface.name.name;
                    let qualified_name =
                        type_mono(&iface_decl_package(interface, checked), interface_name, &[]);
                    let _ = write!(
                        out,
                        " || strcmp({value}, \"{interface_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0"
                    );
                    for (_mono_name, args) in
                        checked.mono_interfaces.iter().filter(|(name, args)| {
                            name == interface_name && !args.iter().any(Ty::is_open)
                        })
                    {
                        let mono = type_mono(
                            &iface_decl_package(interface, checked),
                            interface_name,
                            args,
                        );
                        let _ = write!(out, " || strcmp({value}, \"{mono}\") == 0");
                    }
                }
            }
            out.push_str(");\n");
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
                        .filter(|field| field.visibility == MemberVisibility::Public)
                        .map(|field| field.name.name.clone())
                        .collect::<Vec<_>>()
                } else {
                    class
                        .methods
                        .iter()
                        .filter(|method| method.visibility == MemberVisibility::Public)
                        .map(|method| method.name.name.clone())
                        .collect::<Vec<_>>()
                }
            };
            out.push_str("  if (");
            out.push_str(&value);
            out.push_str(" == NULL) return aura_new_Array_String(INT64_C(0));\n");
            for class in &checked.ast.classes {
                if !has_reflect_attribute(&class.attributes) {
                    continue;
                }
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
                for (_mono_name, args) in checked.mono_classes.iter().filter(|(name, args)| {
                    name == &class.name.name && !args.iter().any(Ty::is_open)
                }) {
                    let mono =
                        type_mono(&class_decl_package(class, checked), &class.name.name, args);
                    let _ = writeln!(out, "  if (strcmp({value}, \"{mono}\") == 0) {{");
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
            }
            if f.name.name == "methods" {
                for interface in &checked.ast.interfaces {
                    if !has_reflect_attribute(&interface.attributes) {
                        continue;
                    }
                    let interface_name = &interface.name.name;
                    let qualified_name =
                        type_mono(&iface_decl_package(interface, checked), interface_name, &[]);
                    let _ = writeln!(
                        out,
                        "  if (strcmp({value}, \"{interface_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0) {{"
                    );
                    let _ = writeln!(
                        out,
                        "    aura_cls_Array_String __metadata = aura_new_Array_String(INT64_C({}));",
                        interface.methods.len()
                    );
                    for (index, method) in interface.methods.iter().enumerate() {
                        let _ = writeln!(
                            out,
                            "    __metadata.data[{index}] = aura_bytes_copy(\"{}\");",
                            method.name.name
                        );
                    }
                    out.push_str("    return __metadata;\n  }\n");
                    for (_mono_name, args) in
                        checked.mono_interfaces.iter().filter(|(name, args)| {
                            name == interface_name && !args.iter().any(Ty::is_open)
                        })
                    {
                        let mono = type_mono(
                            &iface_decl_package(interface, checked),
                            interface_name,
                            args,
                        );
                        let _ = writeln!(out, "  if (strcmp({value}, \"{mono}\") == 0) {{");
                        let _ = writeln!(
                            out,
                            "    aura_cls_Array_String __metadata = aura_new_Array_String(INT64_C({}));",
                            interface.methods.len()
                        );
                        for (index, method) in interface.methods.iter().enumerate() {
                            let _ = writeln!(
                                out,
                                "    __metadata.data[{index}] = aura_bytes_copy(\"{}\");",
                                method.name.name
                            );
                        }
                        out.push_str("    return __metadata;\n  }\n");
                    }
                }
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
                if !has_reflect_attribute(&class.attributes) {
                    continue;
                }
                let metadata = if f.name.name == "fieldMetadata" {
                    class
                        .fields
                        .iter()
                        .filter(|field| field.visibility == MemberVisibility::Public)
                        .map(|field| format!("{}:{}", field.name.name, type_ref_name(&field.ty)))
                        .collect::<Vec<_>>()
                } else {
                    class
                        .methods
                        .iter()
                        .filter(|method| method.visibility == MemberVisibility::Public)
                        .map(|method| {
                            let return_name = method
                                .return_type
                                .as_ref()
                                .map(type_ref_name)
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
                for (_mono_name, args) in checked.mono_classes.iter().filter(|(name, args)| {
                    name == &class.name.name && !args.iter().any(Ty::is_open)
                }) {
                    let mono =
                        type_mono(&class_decl_package(class, checked), &class.name.name, args);
                    let metadata = if f.name.name == "fieldMetadata" {
                        class
                            .fields
                            .iter()
                            .filter(|field| field.visibility == MemberVisibility::Public)
                            .map(|field| {
                                format!(
                                    "{}:{}",
                                    field.name.name,
                                    reflect_type_ref_name(&field.ty, &class.type_params, args)
                                )
                            })
                            .collect::<Vec<_>>()
                    } else {
                        class
                            .methods
                            .iter()
                            .filter(|method| method.visibility == MemberVisibility::Public)
                            .map(|method| {
                                let return_name = method
                                    .return_type
                                    .as_ref()
                                    .map(|ty| reflect_type_ref_name(ty, &class.type_params, args))
                                    .unwrap_or_else(|| "Unit".into());
                                format!("{}:{return_name}", method.name.name)
                            })
                            .collect::<Vec<_>>()
                    };
                    let _ = writeln!(out, "  if (strcmp({value}, \"{mono}\") == 0) {{");
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
            }
            if f.name.name == "methodMetadata" {
                for interface in &checked.ast.interfaces {
                    if !has_reflect_attribute(&interface.attributes) {
                        continue;
                    }
                    let interface_name = &interface.name.name;
                    let qualified_name =
                        type_mono(&iface_decl_package(interface, checked), interface_name, &[]);
                    let _ = writeln!(
                        out,
                        "  if (strcmp({value}, \"{interface_name}\") == 0 || strcmp({value}, \"{qualified_name}\") == 0) {{"
                    );
                    let _ = writeln!(
                        out,
                        "    aura_cls_Array_String __metadata = aura_new_Array_String(INT64_C({}));",
                        interface.methods.len()
                    );
                    for (index, method) in interface.methods.iter().enumerate() {
                        let return_name = method
                            .return_type
                            .as_ref()
                            .map(type_ref_name)
                            .unwrap_or_else(|| "Unit".into());
                        let _ = writeln!(
                            out,
                            "    __metadata.data[{index}] = aura_bytes_copy(\"{}:{}\");",
                            method.name.name, return_name
                        );
                    }
                    out.push_str("    return __metadata;\n  }\n");
                    for (_mono_name, args) in
                        checked.mono_interfaces.iter().filter(|(name, args)| {
                            name == interface_name && !args.iter().any(Ty::is_open)
                        })
                    {
                        let mono = type_mono(
                            &iface_decl_package(interface, checked),
                            interface_name,
                            args,
                        );
                        let _ = writeln!(out, "  if (strcmp({value}, \"{mono}\") == 0) {{");
                        let _ = writeln!(
                            out,
                            "    aura_cls_Array_String __metadata = aura_new_Array_String(INT64_C({}));",
                            interface.methods.len()
                        );
                        for (index, method) in interface.methods.iter().enumerate() {
                            let return_name = method
                                .return_type
                                .as_ref()
                                .map(|ty| reflect_type_ref_name(ty, &interface.type_params, args))
                                .unwrap_or_else(|| "Unit".into());
                            let _ = writeln!(
                                out,
                                "    __metadata.data[{index}] = aura_bytes_copy(\"{}:{}\");",
                                method.name.name, return_name
                            );
                        }
                        out.push_str("    return __metadata;\n  }\n");
                    }
                }
            }
            out.push_str("  return aura_new_Array_String(INT64_C(0));\n}\n");
            return;
        }
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Json)
        && matches!(f.name.name.as_str(), "encode" | "stringify")
        && f.params.len() == 1
        && args.len() == 1
    {
        let value = mangle_ident(&f.params[0].name.name);
        match &args[0] {
            Ty::Int => {
                let _ = writeln!(out, "  return aura_json_encode_int({value});");
                out.push_str("}\n");
                return;
            }
            Ty::Bool => {
                let _ = writeln!(out, "  return aura_json_encode_bool({value});");
                out.push_str("}\n");
                return;
            }
            Ty::String => {
                let _ = writeln!(out, "  return aura_json_escape_string({value});");
                out.push_str("}\n");
                return;
            }
            _ => {
                if emit_json_encode_array(out, &value, &args[0], checked)
                    || emit_json_encode_class(out, &value, &args[0], checked)
                {
                    out.push_str("}\n");
                    return;
                }
                let key = crate::expr::full_type_mono(&args[0].mono_suffix(), checked);
                if json_encode_key_supported(&key, checked, 0) {
                    out.push_str("  const char *__json_encoded = NULL;\n");
                    emit_json_encode_value(
                        out,
                        "__json_encoded",
                        &value,
                        &key,
                        checked,
                        "root_value",
                    );
                    out.push_str("  return __json_encoded;\n}\n");
                    return;
                }
            }
        }
        out.push_str("  return NULL; /* unsupported JSON shape */\n}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Json)
        && f.name.name == "decode"
        && f.params.len() == 1
        && args.len() == 1
    {
        let value = mangle_ident(&f.params[0].name.name);
        let name = args[0].mono_suffix();
        let array_key = crate::expr::full_type_mono(&name, checked);
        if is_array_type_key(&array_key) {
            if let Some(element_key) = crate::expr::array_elem_local_key(&array_key, checked) {
                let mut leaf_key = element_key.as_str();
                while let Some(rest) = leaf_key.strip_prefix("Array_") {
                    leaf_key = rest;
                }
                if matches!(leaf_key, "Int" | "Bool" | "String")
                    && element_key.starts_with("Array_")
                {
                    emit_json_decode_nested_primitive_array(out, &array_key, checked);
                    return;
                }
                if matches!(element_key.as_str(), "Int" | "Bool" | "String") {
                    emit_json_decode_primitive_array(out, &array_key, &element_key, checked);
                    return;
                }
            }
        }
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
                let mut seen = Vec::new();
                if let Some(node) = build_json_decode_node(&mono, checked, &mut seen, 0) {
                    emit_json_decode_node(out, &node, &value, &c_class_type(&mono), &ctor, checked);
                    return;
                }
                let mut supported = true;
                let mut field_keys = Vec::new();
                for field in &class.fields {
                    let key = type_ref_local_key_expand(&field.ty, &params, &class_args, checked);
                    if !matches!(key.as_str(), "Int" | "Bool" | "String") {
                        supported = false;
                    }
                    field_keys.push((field, key));
                }
                // JSON class mapping also supports one level of nested
                // non-generic classes. Keep the generated path explicit so
                // every owned source slice is released on success or error.
                let nested_specs = field_keys
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (field, key))| {
                        if matches!(key.as_str(), "Int" | "Bool" | "String") {
                            return None;
                        }
                        let full = crate::expr::full_type_mono(key, checked);
                        let nested = checked.ast.classes.iter().find(|candidate| {
                            let package = class_decl_package(candidate, checked);
                            type_mono(&package, &candidate.name.name, &[]) == full
                                && candidate.type_params.is_empty()
                        })?;
                        let nested_fields = nested
                            .fields
                            .iter()
                            .map(|nested_field| {
                                (
                                    json_field_name(nested_field),
                                    type_ref_local_key_expand(&nested_field.ty, &[], &[], checked),
                                )
                            })
                            .collect::<Vec<_>>();
                        if nested_fields.iter().all(|(_, nested_key)| {
                            matches!(nested_key.as_str(), "Int" | "Bool" | "String")
                        }) {
                            Some((index, nested_fields, full))
                        } else {
                            let _ = field;
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                if !nested_specs.is_empty()
                    && nested_specs.len()
                        == field_keys
                            .iter()
                            .filter(|(_, key)| !matches!(key.as_str(), "Int" | "Bool" | "String"))
                            .count()
                {
                    out.push_str(
                        "  if (value == NULL || !aura_json_is_valid((*value).text)) return NULL;\n",
                    );
                    out.push_str("  aura_gc_add_root((void **)&value);\n");
                    out.push_str("  int __json_stage = 0;\n");
                    for (index, (field, key)) in field_keys.iter().enumerate() {
                        let _ = writeln!(out, "  const char *__json_field_{index} = NULL;");
                        match key.as_str() {
                            "Int" => {
                                let _ = writeln!(out, "  int64_t __json_int_{index} = 0;");
                            }
                            "Bool" => {
                                let _ = writeln!(out, "  bool __json_bool_{index} = false;");
                            }
                            "String" => {
                                let _ =
                                    writeln!(out, "  const char *__json_string_{index} = NULL;");
                            }
                            _ => {
                                let _ = writeln!(
                                    out,
                                    "  {cty} *__json_class_{index} = NULL;",
                                    cty = c_class_type(&crate::expr::full_type_mono(key, checked))
                                );
                                let _ = writeln!(
                                    out,
                                    "  aura_gc_add_root((void **)&__json_class_{index});"
                                );
                                if let Some((_, nested_fields, _)) = nested_specs
                                    .iter()
                                    .find(|(nested_index, _, _)| *nested_index == index)
                                {
                                    for (nested_index, (_, nested_key)) in
                                        nested_fields.iter().enumerate()
                                    {
                                        let _ = writeln!(out, "  const char *__json_nested_{index}_{nested_index} = NULL;");
                                        if nested_key == "Int" {
                                            let _ = writeln!(out, "  int64_t __json_nested_int_{index}_{nested_index} = 0;");
                                        } else if nested_key == "Bool" {
                                            let _ = writeln!(out, "  bool __json_nested_bool_{index}_{nested_index} = false;");
                                        } else {
                                            let _ = writeln!(out, "  const char *__json_nested_string_{index}_{nested_index} = NULL;");
                                        }
                                    }
                                }
                            }
                        }
                        let _ = writeln!(out, "  __json_stage = {stage};", stage = index + 1);
                        let json_name = json_field_name(field);
                        let _ = writeln!(out, "  __json_field_{index} = aura_json_object_get((*{value}).text, \"{}\");", json_name.replace('\\', "\\\\").replace('"', "\\\""));
                        out.push_str(&format!(
                            "  if (__json_field_{index} == NULL) goto __json_decode_fail;\n"
                        ));
                        match key.as_str() {
                            "Int" => {
                                let _ = writeln!(out, "  if (!aura_json_parse_int(__json_field_{index}, &__json_int_{index})) goto __json_decode_fail;");
                            }
                            "Bool" => {
                                let _ = writeln!(out, "  if (!aura_json_parse_bool(__json_field_{index}, &__json_bool_{index})) goto __json_decode_fail;");
                            }
                            "String" => {
                                let _ = writeln!(out, "  __json_string_{index} = aura_json_decode_string(__json_field_{index}); if (__json_string_{index} == NULL) goto __json_decode_fail;");
                            }
                            _ => {
                                let (_, nested_fields, full) = nested_specs
                                    .iter()
                                    .find(|(nested_index, _, _)| *nested_index == index)
                                    .expect("nested JSON class spec");
                                for (nested_index, (nested_name, nested_key)) in
                                    nested_fields.iter().enumerate()
                                {
                                    let _ = writeln!(out, "  __json_nested_{index}_{nested_index} = aura_json_object_get(__json_field_{index}, \"{}\");", nested_name.replace('\\', "\\\\").replace('"', "\\\""));
                                    let _ = writeln!(out, "  if (__json_nested_{index}_{nested_index} == NULL) goto __json_decode_fail;");
                                    match nested_key.as_str() {
                                        "Int" => {
                                            let _ = writeln!(out, "  if (!aura_json_parse_int(__json_nested_{index}_{nested_index}, &__json_nested_int_{index}_{nested_index})) goto __json_decode_fail;");
                                        }
                                        "Bool" => {
                                            let _ = writeln!(out, "  if (!aura_json_parse_bool(__json_nested_{index}_{nested_index}, &__json_nested_bool_{index}_{nested_index})) goto __json_decode_fail;");
                                        }
                                        _ => {
                                            let _ = writeln!(out, "  __json_nested_string_{index}_{nested_index} = aura_json_decode_string(__json_nested_{index}_{nested_index}); if (__json_nested_string_{index}_{nested_index} == NULL) goto __json_decode_fail;");
                                        }
                                    }
                                }
                                let nested_ctor_args = nested_fields
                                    .iter()
                                    .enumerate()
                                    .map(|(nested_index, (_, nested_key))| {
                                        match nested_key.as_str() {
                                            "Int" => {
                                                format!("__json_nested_int_{index}_{nested_index}")
                                            }
                                            "Bool" => {
                                                format!("__json_nested_bool_{index}_{nested_index}")
                                            }
                                            _ => format!(
                                                "__json_nested_string_{index}_{nested_index}"
                                            ),
                                        }
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                let _ = writeln!(out, "  __json_class_{index} = {}({nested_ctor_args}); if (__json_class_{index} == NULL) goto __json_decode_fail;", c_ctor_name(full));
                            }
                        }
                    }
                    let ctor_args = field_keys
                        .iter()
                        .enumerate()
                        .map(|(index, (_, key))| match key.as_str() {
                            "Int" => format!("__json_int_{index}"),
                            "Bool" => format!("__json_bool_{index}"),
                            "String" => format!("__json_string_{index}"),
                            _ => format!("__json_class_{index}"),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let _ = writeln!(
                        out,
                        "  {c_class} *__decoded = {ctor}({ctor_args});",
                        c_class = c_class_type(&mono)
                    );
                    out.push_str("  goto __json_decode_done;\n");
                    out.push_str("__json_decode_fail:\n");
                    for (index, (_, key)) in field_keys.iter().enumerate() {
                        let _ = writeln!(
                            out,
                            "  if (__json_stage > {index}) free((void *)__json_field_{index});"
                        );
                        if key == "String" {
                            let _ = writeln!(out, "  if (__json_stage > {index}) free((void *)__json_string_{index});");
                        }
                        if let Some((_, nested_fields, _)) = nested_specs
                            .iter()
                            .find(|(nested_index, _, _)| *nested_index == index)
                        {
                            for (nested_index, (_, nested_key)) in nested_fields.iter().enumerate()
                            {
                                let _ = writeln!(out, "  if (__json_stage > {index}) free((void *)__json_nested_{index}_{nested_index});");
                                if nested_key == "String" {
                                    let _ = writeln!(out, "  if (__json_stage > {index}) free((void *)__json_nested_string_{index}_{nested_index});");
                                }
                            }
                        }
                    }
                    for (index, (_, key)) in field_keys.iter().enumerate() {
                        if !matches!(key.as_str(), "Int" | "Bool" | "String") {
                            let _ = writeln!(
                                out,
                                "  if (__json_stage > {index}) aura_gc_remove_root((void **)&__json_class_{index});"
                            );
                        }
                    }
                    out.push_str("  aura_gc_remove_root((void **)&value);\n");
                    out.push_str("  return NULL;\n__json_decode_done:\n");
                    for (index, (_, key)) in field_keys.iter().enumerate() {
                        let _ = writeln!(
                            out,
                            "  if (__json_stage > {index}) free((void *)__json_field_{index});"
                        );
                        if key == "String" {
                            let _ = writeln!(out, "  if (__json_stage > {index}) free((void *)__json_string_{index});");
                        }
                        if let Some((_, nested_fields, _)) = nested_specs
                            .iter()
                            .find(|(nested_index, _, _)| *nested_index == index)
                        {
                            for (nested_index, (_, nested_key)) in nested_fields.iter().enumerate()
                            {
                                let _ = writeln!(out, "  if (__json_stage > {index}) free((void *)__json_nested_{index}_{nested_index});");
                                if nested_key == "String" {
                                    let _ = writeln!(out, "  if (__json_stage > {index}) free((void *)__json_nested_string_{index}_{nested_index});");
                                }
                            }
                        }
                    }
                    for (index, (_, key)) in field_keys.iter().enumerate() {
                        if !matches!(key.as_str(), "Int" | "Bool" | "String") {
                            let _ = writeln!(
                                out,
                                "  if (__json_stage > {index}) aura_gc_remove_root((void **)&__json_class_{index});"
                            );
                        }
                    }
                    out.push_str("  aura_gc_remove_root((void **)&value);\n");
                    out.push_str("  return __decoded;\n}\n");
                    return;
                }
                if supported {
                    out.push_str(
                        "  if (value == NULL || !aura_json_is_valid((*value).text)) return NULL;\n",
                    );
                    out.push_str("  aura_gc_add_root((void **)&value);\n");
                    let emit_cleanup = |out: &mut String, end: usize| {
                        for (prior, (_, prior_key)) in field_keys.iter().enumerate().take(end + 1) {
                            let prior_raw = format!("__json_field_{prior}");
                            let _ = writeln!(out, "    free((void *){prior_raw});");
                            if prior < end && *prior_key == "String" {
                                let _ = writeln!(out, "    free((void *)__json_string_{prior});");
                            }
                        }
                        out.push_str("    aura_gc_remove_root((void **)&value);\n");
                    };
                    for (index, (field, key)) in field_keys.iter().enumerate() {
                        let raw = format!("__json_field_{index}");
                        let name = json_field_name(field)
                            .replace('\\', "\\\\")
                            .replace('"', "\\\"");
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
                    out.push_str("  aura_gc_remove_root((void **)&value);\n");
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Compress)
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Dns)
        && f.name.name == "resolveHost"
        && f.params.len() == 2
    {
        let host = mangle_ident(&f.params[0].name.name);
        let prefer = mangle_ident(&f.params[1].name.name);
        let _ = writeln!(
            out,
            "  return aura_dns_resolve_host({host}, {prefer} ? 1 : 0);"
        );
        out.push_str("}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Dns)
        && f.name.name == "resolveHostList"
        && f.params.len() == 2
    {
        let host = mangle_ident(&f.params[0].name.name);
        let prefer = mangle_ident(&f.params[1].name.name);
        let _ = writeln!(
            out,
            "  return aura_dns_resolve_host_list({host}, {prefer} ? 1 : 0);"
        );
        out.push_str("}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Json)
        && f.params.len() == 1
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Json)
        && f.params.len() == 2
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Signal)
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Error)
        && f.params.len() == 1
    {
        let code = mangle_ident(&f.params[0].name.name);
        let _ = writeln!(out, "  return aura_error_kind_code({code});");
        out.push_str("}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Log)
        && f.params.len() == 1
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Log)
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Udp)
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Tls)
        && f.name.name == "close"
        && f.params.len() == 1
        && f.params[0].name.name == "this"
    {
        let this = mangle_ident(&f.params[0].name.name);
        out.push_str("  /* AURA_TLS_REQUIRED */\n");
        let _ = writeln!(out, "  if ({this} != NULL && {this}->endpoint != NULL) (void)aura_tls_close({this}->endpoint);");
        out.push_str("}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Net)
    {
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
                "  if ({endpoint} == NULL || aura_tcp_listener_bind_endpoint({endpoint}, &__bound_port, &__listener) != AURA_TCP_OK || __listener == NULL) {{ char __message[320]; snprintf(__message, sizeof(__message), \"std.net.listen failed: %s\", aura_tcp_last_error()); aura_throw_string(__message); return NULL; }}"
            );
            out.push_str("  if (aura_ffi_handle_new((void *)__listener, aura_destroy_tcp_listener_resource, &__handle) != AURA_FFI_OK) { aura_tcp_listener_destroy(__listener); aura_throw_string(\"std.net.listen failed: could not create listener handle\"); return NULL; }\n");
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::HttpAccessor)
        && !f.params.is_empty()
    {
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
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Assert)
        && f.params.len() == 1
    {
        let arg = mangle_ident(&f.params[0].name.name);
        let _ = writeln!(out, "  aura_assert({arg});");
        out.push_str("}\n");
        return;
    }
    if lookup_std_intrinsic(&pkg, &f.name.name)
        .is_some_and(|spec| spec.intrinsic == StdIntrinsic::Test)
    {
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
            ("assertEqFloat", 2) => Some((
                "aura_assert_eq_float",
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
        let key = param_local_key_expand(p, &params, args, checked);
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
    // Function parameters live in the outer emission scope, so clean every
    // remaining owner here, including owned strings from generic returns.
    crate::stmt::emit_function_end_cleanup(out, 1, &ctx);
    emit_return_fallback(out, &f.return_type, checked, &params, args);
    emit_c_type_fallback(
        out,
        &c_type_from_opt(&f.return_type, checked, &params, args),
    );
    out.push_str("}\n");
}
