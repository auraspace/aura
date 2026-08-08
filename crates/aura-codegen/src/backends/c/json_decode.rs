//! C emission for typed `std.json` decode paths.

use super::*;

#[derive(Debug, Clone)]
pub(super) struct JsonDecodeNode {
    mono: String,
    is_struct: bool,
    fields: Vec<JsonDecodeField>,
}

#[derive(Debug, Clone)]
pub(super) struct JsonDecodeField {
    json_name: String,
    key: String,
    nullable: bool,
    array_element: Option<String>,
    array_enum_mono: Option<String>,
    array_nested: Option<Box<JsonDecodeNode>>,
    enum_mono: Option<String>,
    nested: Option<Box<JsonDecodeNode>>,
}

pub(super) fn json_field_name(field: &FieldDecl) -> String {
    field
        .attributes
        .iter()
        .find(|attribute| attribute.name.name == "json")
        .and_then(|attribute| {
            attribute.args.iter().find_map(|arg| match arg {
                AttributeArg::Named {
                    name,
                    value: AttributeValue::String { value, .. },
                    ..
                } if name.name == "name" => Some(value.clone()),
                _ => None,
            })
        })
        .unwrap_or_else(|| field.name.name.clone())
}

pub(super) fn reflect_type_ref_name(
    type_ref: &TypeRef,
    params: &[TypeParam],
    args: &[Ty],
) -> String {
    let mut name = if type_ref.type_args.is_empty() {
        params
            .iter()
            .position(|param| param.name.name == type_ref.name.name)
            .and_then(|index| args.get(index))
            .map(Ty::display)
            .unwrap_or_else(|| type_ref.name.name.clone())
    } else {
        format!(
            "{}<{}>",
            type_ref.name.name,
            type_ref
                .type_args
                .iter()
                .map(|arg| reflect_type_ref_name(arg, params, args))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    if type_ref.nullable {
        name.push('?');
    }
    name
}

pub(super) fn json_decode_class_for_mono<'a>(
    full: &str,
    checked: &'a CheckedFile,
) -> Option<(&'a ClassDecl, Vec<Ty>)> {
    checked.ast.classes.iter().find_map(|class| {
        let package = class_decl_package(class, checked);
        if class.type_params.is_empty() && type_mono(&package, &class.name.name, &[]) == full {
            return Some((class, Vec::new()));
        }
        checked
            .mono_classes
            .iter()
            .find(|(name, args)| {
                *name == class.name.name && type_mono(&package, name, args) == full
            })
            .map(|(_, args)| (class, args.clone()))
    })
}

pub(super) fn json_decode_unit_enum_for_mono<'a>(
    full: &str,
    checked: &'a CheckedFile,
) -> Option<&'a EnumDecl> {
    checked.ast.enums.iter().find(|en| {
        let package = enum_decl_package(en, checked);
        (type_mono(&package, &en.name.name, &[]) == full
            || checked.mono_enums.iter().any(|(name, args)| {
                name == &en.name.name && type_mono(&package, name, args) == full
            }))
            && en.variants.iter().all(|variant| {
                variant
                    .fields
                    .iter()
                    .all(|field| field.ty.name.name == "Unit")
            })
    })
}

pub(super) fn build_json_decode_node(
    key: &str,
    checked: &CheckedFile,
    seen: &mut Vec<String>,
    depth: usize,
) -> Option<JsonDecodeNode> {
    if depth > 64 || matches!(key, "Int" | "Bool" | "String") {
        return None;
    }
    let requested = crate::expr::full_type_mono(key, checked);
    if seen.iter().any(|item| item == &requested) {
        return None;
    }
    let (class, class_args) = json_decode_class_for_mono(&requested, checked)?;
    let full = type_mono(
        &class_decl_package(class, checked),
        &class.name.name,
        &class_args,
    );
    let params = class
        .type_params
        .iter()
        .map(|param| param.name.name.clone())
        .collect::<Vec<_>>();
    seen.push(full.clone());
    let mut fields = Vec::with_capacity(class.fields.len());
    for field in &class.fields {
        let local_key = type_ref_local_key_expand(&field.ty, &params, &class_args, checked);
        // Nullable reference-like fields keep their nullability in the
        // semantic field metadata, while JSON construction uses the same C
        // payload layout as the non-nullable class/array.
        let payload_key = crate::expr::task_payload_repr_key(&local_key);
        let field_key = if payload_key == "Opt_Int" {
            "Int".to_string()
        } else if payload_key == "Opt_Bool" {
            "Bool".to_string()
        } else if crate::array_emit::is_array_type_key(&payload_key) {
            crate::expr::full_type_mono(&payload_key, checked)
        } else {
            payload_key
        };
        let array_element = crate::expr::array_elem_local_key(&field_key, checked)
            .filter(|element| matches!(element.as_str(), "Int" | "Bool" | "String"));
        let array_nested = if array_element.is_none() {
            crate::expr::array_elem_local_key(&field_key, checked).and_then(|element| {
                build_json_decode_node(&element, checked, seen, depth + 1).map(Box::new)
            })
        } else {
            None
        };
        let array_enum_mono = if array_element.is_none() && array_nested.is_none() {
            crate::expr::array_elem_local_key(&field_key, checked).and_then(|element| {
                let full = crate::expr::full_type_mono(&element, checked);
                json_decode_unit_enum_for_mono(&full, checked).map(|_| full)
            })
        } else {
            None
        };
        let enum_mono =
            if !matches!(field_key.as_str(), "Int" | "Bool" | "String") && array_nested.is_none() {
                let full = crate::expr::full_type_mono(&field_key, checked);
                json_decode_unit_enum_for_mono(&full, checked).map(|_| full)
            } else {
                None
            };
        let nested = if matches!(field_key.as_str(), "Int" | "Bool" | "String")
            || array_element.is_some()
            || array_enum_mono.is_some()
            || array_nested.is_some()
            || enum_mono.is_some()
        {
            None
        } else {
            Some(Box::new(build_json_decode_node(
                &field_key,
                checked,
                seen,
                depth + 1,
            )?))
        };
        fields.push(JsonDecodeField {
            json_name: json_field_name(field),
            key: field_key,
            nullable: field.ty.nullable,
            array_element,
            array_enum_mono,
            array_nested,
            enum_mono,
            nested,
        });
    }
    seen.pop();
    Some(JsonDecodeNode {
        mono: full,
        is_struct: class.kind == NominalKind::Struct,
        fields,
    })
}

pub(super) fn json_decode_var(path: &str, suffix: &str) -> String {
    format!("__json_{suffix}_{path}")
}

pub(super) fn emit_json_decode_declarations(out: &mut String, node: &JsonDecodeNode, path: &str) {
    for (index, field) in node.fields.iter().enumerate() {
        let field_path = format!("{path}_{index}");
        let raw = json_decode_var(&field_path, "raw");
        let _ = writeln!(out, "  const char *{raw} = NULL;");
        match field.key.as_str() {
            "Int" => {
                let _ = writeln!(
                    out,
                    "  int64_t {} = 0;",
                    json_decode_var(&field_path, "int")
                );
            }
            "Bool" => {
                let _ = writeln!(
                    out,
                    "  bool {} = false;",
                    json_decode_var(&field_path, "bool")
                );
            }
            "String" => {
                let _ = writeln!(
                    out,
                    "  const char *{} = NULL;",
                    json_decode_var(&field_path, "string")
                );
            }
            _ if field.enum_mono.is_some() => {
                let enum_value = json_decode_var(&field_path, "enum");
                let _ = writeln!(
                    out,
                    "  {} {} = {{ 0 }};",
                    c_enum_type(field.enum_mono.as_ref().expect("enum mono")),
                    enum_value
                );
                let decoded = json_decode_var(&field_path, "enum_string");
                let _ = writeln!(out, "  const char *{decoded} = NULL;");
                let matched = json_decode_var(&field_path, "enum_matched");
                let _ = writeln!(out, "  bool {matched} = false;");
                if field.nullable {
                    let _ = writeln!(out, "  {enum_value}.tag = -1;");
                }
            }
            _ if field.array_element.is_some()
                || field.array_enum_mono.is_some()
                || field.array_nested.is_some() =>
            {
                let array = json_decode_var(&field_path, "array");
                let _ = writeln!(out, "  {} {array} = {{ 0 }};", c_class_type(&field.key));
                if field.nullable {
                    let _ = writeln!(out, "  {array}.len = -1; {array}.cap = -1;");
                }
                let rooted = json_decode_var(&field_path, "array_rooted");
                let _ = writeln!(out, "  bool {rooted} = false;");
                if let Some(nested) = &field.array_nested {
                    let item = json_decode_var(&field_path, "array_item");
                    if nested.is_struct {
                        let _ = writeln!(out, "  {} {item} = {{ 0 }};", c_class_type(&nested.mono));
                    } else {
                        let _ = writeln!(out, "  {} *{item} = NULL;", c_class_type(&nested.mono));
                        let _ = writeln!(out, "  aura_gc_add_root((void **)&{item});");
                    }
                    emit_json_decode_declarations(out, nested, &format!("{field_path}_item"));
                }
            }
            _ => {
                let nested = field.nested.as_ref().expect("nested JSON node");
                let class = json_decode_var(&field_path, "class");
                if nested.is_struct {
                    let _ = writeln!(out, "  {} {class} = {{ 0 }};", c_class_type(&nested.mono));
                    emit_json_decode_declarations(out, nested, &field_path);
                    continue;
                }
                let _ = writeln!(out, "  {} *{class} = NULL;", c_class_type(&nested.mono));
                let _ = writeln!(out, "  aura_gc_add_root((void **)&{class});");
                emit_json_decode_declarations(out, nested, &field_path);
            }
        }
    }
}

pub(super) fn emit_json_decode_parse(
    out: &mut String,
    node: &JsonDecodeNode,
    path: &str,
    source: &str,
    checked: &CheckedFile,
) {
    for (index, field) in node.fields.iter().enumerate() {
        let field_path = format!("{path}_{index}");
        let raw = json_decode_var(&field_path, "raw");
        let escaped = field.json_name.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(
            out,
            "  {raw} = aura_json_object_get({source}, \"{escaped}\");"
        );
        if field.nullable {
            let _ = writeln!(out, "  if ({raw} != NULL && !aura_json_is_null({raw})) {{");
        } else {
            let _ = writeln!(out, "  if ({raw} == NULL) goto __json_decode_fail;");
        }
        match field.key.as_str() {
            "Int" => {
                let _ = writeln!(
                    out,
                    "  if (!aura_json_parse_int({raw}, &{})) goto __json_decode_fail;",
                    json_decode_var(&field_path, "int")
                );
            }
            "Bool" => {
                let _ = writeln!(
                    out,
                    "  if (!aura_json_parse_bool({raw}, &{})) goto __json_decode_fail;",
                    json_decode_var(&field_path, "bool")
                );
            }
            "String" => {
                let decoded = json_decode_var(&field_path, "string");
                let _ = writeln!(
                    out,
                    "  {decoded} = aura_json_decode_string({raw}); if ({decoded} == NULL) goto __json_decode_fail;"
                );
            }
            _ if field.enum_mono.is_some() => {
                let enum_mono = field.enum_mono.as_ref().expect("enum mono");
                let decoded = json_decode_var(&field_path, "enum_string");
                let enum_value = json_decode_var(&field_path, "enum");
                let matched = json_decode_var(&field_path, "enum_matched");
                let enum_decl = json_decode_unit_enum_for_mono(enum_mono, checked)
                    .expect("unit enum declaration");
                let _ = writeln!(
                    out,
                    "  {decoded} = aura_json_decode_string({raw}); if ({decoded} == NULL) goto __json_decode_fail;"
                );
                for variant in &enum_decl.variants {
                    let variant_name = variant.name.name.replace('\\', "\\\\").replace('"', "\\\"");
                    let ctor = c_variant_ctor_name(enum_mono, &variant.name.name);
                    let _ = writeln!(
                        out,
                        "  if (strcmp({decoded}, \"{variant_name}\") == 0) {{ {enum_value} = {ctor}(); {matched} = true; }}"
                    );
                }
                let _ = writeln!(out, "  if (!{matched}) goto __json_decode_fail;");
            }
            _ if field.array_element.is_some()
                || field.array_enum_mono.is_some()
                || field.array_nested.is_some() =>
            {
                let array = json_decode_var(&field_path, "array");
                let array_ctor = c_ctor_name(&field.key);
                let array_push = c_method_name(&field.key, "push");
                let _ = writeln!(
                    out,
                    "  if (!aura_json_is_array({raw})) goto __json_decode_fail;"
                );
                let _ = writeln!(out, "  {array} = {array_ctor}(0);");
                let rooted = json_decode_var(&field_path, "array_rooted");
                let _ = writeln!(
                    out,
                    "  aura_gc_add_array_root((void **)&{array}.data, &{array}.len); {rooted} = true;"
                );
                out.push_str("  for (int64_t __json_i = 0, __json_n = aura_json_array_count(");
                let _ = writeln!(out, "{raw}); __json_i < __json_n; __json_i++) {{");
                let _ = writeln!(
                    out,
                    "    const char *__json_item = aura_json_array_at({raw}, __json_i); if (__json_item == NULL) goto __json_decode_fail;"
                );
                match field.array_element.as_deref() {
                    Some("Int") => out.push_str(
                        "    int64_t __json_item_value = 0; if (!aura_json_parse_int(__json_item, &__json_item_value)) { free((void *)__json_item); goto __json_decode_fail; }\n",
                    ),
                    Some("Bool") => out.push_str(
                        "    bool __json_item_value = false; if (!aura_json_parse_bool(__json_item, &__json_item_value)) { free((void *)__json_item); goto __json_decode_fail; }\n",
                    ),
                    Some("String") => out.push_str(
                        "    const char *__json_item_value = aura_json_decode_string(__json_item); if (__json_item_value == NULL) { free((void *)__json_item); goto __json_decode_fail; }\n",
                    ),
                    None if field.array_enum_mono.is_some() => {
                        let enum_mono = field
                            .array_enum_mono
                            .as_ref()
                            .expect("array enum mono");
                        let enum_decl = json_decode_unit_enum_for_mono(enum_mono, checked)
                            .expect("unit array enum declaration");
                        let _ = writeln!(
                            out,
                            "    const char *__json_item_enum_string = aura_json_decode_string(__json_item); if (__json_item_enum_string == NULL) {{ free((void *)__json_item); goto __json_decode_fail; }}"
                        );
                        let _ = writeln!(
                            out,
                            "    {} __json_item_enum_value = {{ 0 }}; bool __json_item_enum_matched = false;",
                            c_enum_type(enum_mono)
                        );
                        for variant in &enum_decl.variants {
                            let variant_name = variant
                                .name
                                .name
                                .replace('\\', "\\\\")
                                .replace('"', "\\\"");
                            let ctor = c_variant_ctor_name(enum_mono, &variant.name.name);
                            let _ = writeln!(
                                out,
                                "    if (strcmp(__json_item_enum_string, \"{variant_name}\") == 0) {{ __json_item_enum_value = {ctor}(); __json_item_enum_matched = true; }}"
                            );
                        }
                        out.push_str("    free((void *)__json_item_enum_string); if (!__json_item_enum_matched) { free((void *)__json_item); goto __json_decode_fail; }\n");
                    }
                    _ => {
                        let nested = field.array_nested.as_ref().expect("nested array node");
                        let item = json_decode_var(&field_path, "array_item");
                        let item_path = format!("{field_path}_item");
                        emit_json_decode_parse(out, nested, &item_path, "__json_item", checked);
                        let args = emit_json_decode_ctor_args(nested, &item_path);
                        if nested.is_struct {
                            let _ = writeln!(
                                out,
                                "    {item} = {}({args});",
                                c_ctor_name(&nested.mono)
                            );
                        } else {
                            let _ = writeln!(
                                out,
                                "    {item} = {}({args}); if ({item} == NULL) goto __json_decode_fail;",
                                c_ctor_name(&nested.mono)
                            );
                        }
                        emit_json_decode_transfer_arrays(out, nested, &item_path);
                        let _ = writeln!(out, "    {array_push}(&{array}, {item});");
                        out.push_str("    ");
                        emit_json_decode_item_cleanup(out, nested, &item_path);
                        if !nested.is_struct {
                            let _ = writeln!(out, "    {item} = NULL;");
                        }
                    }
                }
                if field.array_element.is_some() {
                    let _ = writeln!(out, "    {array_push}(&{array}, __json_item_value);");
                    if field.array_element.as_deref() == Some("String") {
                        out.push_str("    free((void *)__json_item_value);\n");
                    }
                } else if field.array_enum_mono.is_some() {
                    let _ = writeln!(out, "    {array_push}(&{array}, __json_item_enum_value);");
                }
                out.push_str("    free((void *)__json_item);\n  }\n");
            }
            _ => {
                let class = json_decode_var(&field_path, "class");
                let nested = field.nested.as_ref().expect("nested JSON node");
                if nested.is_struct {
                    emit_json_decode_parse(out, nested, &field_path, &raw, checked);
                    let args = emit_json_decode_ctor_args(nested, &field_path);
                    let _ = writeln!(out, "  {class} = {}({args});", c_ctor_name(&nested.mono));
                } else {
                    if field.nullable {
                        let _ = writeln!(
                            out,
                            "  if (aura_json_is_null({raw})) {{ {class} = NULL; }} else {{"
                        );
                    } else {
                        let _ = writeln!(out, "  {{");
                    }
                    emit_json_decode_parse(out, nested, &field_path, &raw, checked);
                    let args = nested
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(nested_index, nested_field)| {
                            let nested_path = format!("{field_path}_{nested_index}");
                            match nested_field.key.as_str() {
                                "Int" => json_decode_var(&nested_path, "int"),
                                "Bool" => json_decode_var(&nested_path, "bool"),
                                "String" => json_decode_var(&nested_path, "string"),
                                _ if nested_field.enum_mono.is_some() => {
                                    json_decode_var(&nested_path, "enum")
                                }
                                _ => json_decode_var(&nested_path, "class"),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    let _ = writeln!(
                        out,
                        "    {class} = {}({args}); if ({class} == NULL) goto __json_decode_fail;",
                        c_ctor_name(&nested.mono)
                    );
                    emit_json_decode_transfer_arrays(out, nested, &field_path);
                    out.push_str("  }\n");
                }
            }
        }
        if field.nullable {
            out.push_str("  }\n");
        }
    }
}

pub(super) fn emit_json_decode_cleanup(out: &mut String, node: &JsonDecodeNode, path: &str) {
    for (index, field) in node.fields.iter().enumerate() {
        let field_path = format!("{path}_{index}");
        let raw = json_decode_var(&field_path, "raw");
        let _ = writeln!(out, "  if ({raw} != NULL) free((void *){raw});");
        if field.key == "String" {
            let string = json_decode_var(&field_path, "string");
            let _ = writeln!(out, "  if ({string} != NULL) free((void *){string});");
        }
        if field.enum_mono.is_some() {
            let enum_string = json_decode_var(&field_path, "enum_string");
            let _ = writeln!(
                out,
                "  if ({enum_string} != NULL) free((void *){enum_string});"
            );
        }
        if field.array_element.is_some()
            || field.array_enum_mono.is_some()
            || field.array_nested.is_some()
        {
            let array = json_decode_var(&field_path, "array");
            let array_type = c_class_type(&field.key);
            let rooted = json_decode_var(&field_path, "array_rooted");
            let _ = writeln!(
                out,
                "  if ({rooted}) aura_gc_remove_array_root((void **)&{array}.data);"
            );
            let _ = writeln!(
                out,
                "  if ({array}.data != NULL) {array_type}_drop(&{array});"
            );
            if let Some(nested) = &field.array_nested {
                emit_json_decode_cleanup(out, nested, &format!("{field_path}_item"));
                if !nested.is_struct {
                    let item = json_decode_var(&field_path, "array_item");
                    let _ = writeln!(out, "  aura_gc_remove_root((void **)&{item});");
                }
            }
        }
        if let Some(nested) = &field.nested {
            emit_json_decode_cleanup(out, nested, &field_path);
            if !nested.is_struct {
                let class = json_decode_var(&field_path, "class");
                let _ = writeln!(out, "  aura_gc_remove_root((void **)&{class});");
            }
        }
    }
}

pub(super) fn emit_json_decode_item_cleanup(out: &mut String, node: &JsonDecodeNode, path: &str) {
    for (index, field) in node.fields.iter().enumerate() {
        let field_path = format!("{path}_{index}");
        let raw = json_decode_var(&field_path, "raw");
        let _ = writeln!(
            out,
            "if ({raw} != NULL) {{ free((void *){raw}); {raw} = NULL; }}"
        );
        if field.key == "String" {
            let string = json_decode_var(&field_path, "string");
            let _ = writeln!(
                out,
                "if ({string} != NULL) {{ free((void *){string}); {string} = NULL; }}"
            );
        }
        if field.enum_mono.is_some() {
            let enum_string = json_decode_var(&field_path, "enum_string");
            let _ = writeln!(
                out,
                "if ({enum_string} != NULL) {{ free((void *){enum_string}); {enum_string} = NULL; }}"
            );
        }
        if let Some(nested) = &field.nested {
            emit_json_decode_item_cleanup(out, nested, &field_path);
        }
        if let Some(nested) = &field.array_nested {
            emit_json_decode_item_cleanup(out, nested, &format!("{field_path}_item"));
        }
    }
}

pub(super) fn emit_json_decode_transfer_arrays(
    out: &mut String,
    node: &JsonDecodeNode,
    path: &str,
) {
    for (index, field) in node.fields.iter().enumerate() {
        let field_path = format!("{path}_{index}");
        if field.array_element.is_some()
            || field.array_enum_mono.is_some()
            || field.array_nested.is_some()
        {
            let array = json_decode_var(&field_path, "array");
            let rooted = json_decode_var(&field_path, "array_rooted");
            let _ = writeln!(
                out,
                "  if ({rooted}) aura_gc_remove_array_root((void **)&{array}.data); {array}.data = NULL; {array}.len = 0; {array}.cap = 0; {rooted} = false;"
            );
        }
        if let Some(nested) = &field.nested {
            emit_json_decode_transfer_arrays(out, nested, &field_path);
        }
        if let Some(nested) = &field.array_nested {
            emit_json_decode_transfer_arrays(out, nested, &format!("{field_path}_item"));
        }
    }
}

pub(super) fn emit_json_decode_ctor_args(node: &JsonDecodeNode, path: &str) -> String {
    node.fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_path = format!("{path}_{index}");
            match field.key.as_str() {
                "Int" if field.nullable => format!(
                    "(aura_opt_i64){{ .has = {} != NULL && !aura_json_is_null({}), .value = {} }}",
                    json_decode_var(&field_path, "raw"),
                    json_decode_var(&field_path, "raw"),
                    json_decode_var(&field_path, "int")
                ),
                "Bool" if field.nullable => format!(
                    "(aura_opt_bool){{ .has = {} != NULL && !aura_json_is_null({}), .value = {} }}",
                    json_decode_var(&field_path, "raw"),
                    json_decode_var(&field_path, "raw"),
                    json_decode_var(&field_path, "bool")
                ),
                "Int" => json_decode_var(&field_path, "int"),
                "Bool" => json_decode_var(&field_path, "bool"),
                "String" => json_decode_var(&field_path, "string"),
                _ if field.enum_mono.is_some() => json_decode_var(&field_path, "enum"),
                _ if field.array_element.is_some()
                    || field.array_enum_mono.is_some()
                    || field.array_nested.is_some() =>
                {
                    json_decode_var(&field_path, "array")
                }
                _ => json_decode_var(&field_path, "class"),
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn emit_json_decode_node(
    out: &mut String,
    node: &JsonDecodeNode,
    value: &str,
    target_type: &str,
    constructor: &str,
    checked: &CheckedFile,
) {
    out.push_str("  if (value == NULL || !aura_json_is_valid((*value).text)) return NULL;\n");
    out.push_str("  aura_gc_add_root((void **)&value);\n");
    emit_json_decode_declarations(out, node, "root");
    emit_json_decode_parse(out, node, "root", &format!("(*{value}).text"), checked);
    let args = emit_json_decode_ctor_args(node, "root");
    let _ = writeln!(
        out,
        "  {target_type} *__decoded = {constructor}({args}); if (__decoded == NULL) goto __json_decode_fail;"
    );
    emit_json_decode_transfer_arrays(out, node, "root");
    out.push_str("  goto __json_decode_done;\n");
    out.push_str("__json_decode_fail:\n");
    emit_json_decode_cleanup(out, node, "root");
    out.push_str("  aura_gc_remove_root((void **)&value);\n  return NULL;\n");
    out.push_str("__json_decode_done:\n");
    emit_json_decode_cleanup(out, node, "root");
    out.push_str("  aura_gc_remove_root((void **)&value);\n  return __decoded;\n}\n");
}

pub(super) fn emit_json_decode_primitive_array(
    out: &mut String,
    array_key: &str,
    element_key: &str,
    checked: &CheckedFile,
) {
    let array_type = crate::stmt::local_key_to_c(array_key, checked);
    let ctor = c_ctor_name(array_key);
    let push = c_method_name(array_key, "push");
    let drop = format!("{array_type}_drop");
    out.push_str("  if (value == NULL || !aura_json_is_valid((*value).text) || !aura_json_is_array((*value).text)) return (\n");
    let _ = writeln!(out, "    {array_type}){{0}};");
    out.push_str("  aura_gc_add_root((void **)&value);\n");
    let _ = writeln!(out, "  {array_type} __decoded = {ctor}(0);");
    out.push_str("  aura_gc_add_array_root((void **)&__decoded.data, &__decoded.len);\n");
    out.push_str(
        "  for (int64_t __json_i = 0, __json_n = aura_json_array_count((*value).text); __json_i < __json_n; __json_i++) {\n",
    );
    out.push_str(
        "    const char *__json_item = aura_json_array_at((*value).text, __json_i); if (__json_item == NULL) goto __json_array_fail;\n",
    );
    match element_key {
        "Int" => out.push_str(
            "    int64_t __json_item_value = 0; if (!aura_json_parse_int(__json_item, &__json_item_value)) { free((void *)__json_item); goto __json_array_fail; }\n",
        ),
        "Bool" => out.push_str(
            "    bool __json_item_value = false; if (!aura_json_parse_bool(__json_item, &__json_item_value)) { free((void *)__json_item); goto __json_array_fail; }\n",
        ),
        "String" => out.push_str(
            "    const char *__json_item_value = aura_json_decode_string(__json_item); if (__json_item_value == NULL) { free((void *)__json_item); goto __json_array_fail; }\n",
        ),
        _ => unreachable!("primitive JSON array emitter received non-primitive element"),
    }
    let _ = writeln!(out, "    {push}(&__decoded, __json_item_value);");
    if element_key == "String" {
        out.push_str("    free((void *)__json_item_value);\n");
    }
    out.push_str("    free((void *)__json_item);\n  }\n");
    out.push_str(
        "  aura_gc_remove_array_root((void **)&__decoded.data); aura_gc_remove_root((void **)&value); return __decoded;\n",
    );
    out.push_str("__json_array_fail:\n  aura_gc_remove_array_root((void **)&__decoded.data); ");
    let _ = writeln!(
        out,
        "{drop}(&__decoded); aura_gc_remove_root((void **)&value); return ({array_type}){{0}};\n}}"
    );
}

pub(super) fn emit_json_decode_nested_primitive_array(
    out: &mut String,
    array_key: &str,
    checked: &CheckedFile,
) {
    let outer_type = crate::stmt::local_key_to_c(array_key, checked);
    out.push_str("  if (value == NULL || !aura_json_is_valid((*value).text) || !aura_json_is_array((*value).text)) return (\n");
    let _ = writeln!(out, "    {outer_type}){{0}};");
    out.push_str("  aura_gc_add_root((void **)&value);\n");
    out.push_str("  bool __json_array_ok = true;\n");
    let decoded =
        emit_json_decode_primitive_array_level(out, array_key, "(*value).text", 0, checked);
    out.push_str("  aura_gc_remove_root((void **)&value);\n");
    let _ = writeln!(
        out,
        "  if (!__json_array_ok) return ({outer_type}){{0}};\n  return {decoded};\n}}"
    );
}

pub(super) fn emit_json_decode_primitive_array_level(
    out: &mut String,
    array_key: &str,
    json_expr: &str,
    depth: usize,
    checked: &CheckedFile,
) -> String {
    assert!(depth < 16, "primitive JSON array nesting did not converge");
    let array_type = crate::stmt::local_key_to_c(array_key, checked);
    let array_var = if depth == 0 {
        "__decoded".to_string()
    } else {
        format!("__json_array_{depth}")
    };
    let ctor = c_ctor_name(array_key);
    let push = c_method_name(array_key, "push");
    let drop = format!("{array_type}_drop");
    let item_var = format!("__json_item_{depth}");
    let index_var = format!("__json_i_{depth}");
    let nested_key = crate::expr::array_elem_local_key(array_key, checked)
        .expect("primitive nested JSON array must have an element type");

    let _ = writeln!(out, "  {array_type} {array_var} = {ctor}(0);");
    let _ = writeln!(
        out,
        "  bool __json_array_rooted_{depth} = true; aura_gc_add_array_root((void **)&{array_var}.data, &{array_var}.len);"
    );
    let _ = writeln!(
        out,
        "  for (int64_t {index_var} = 0, __json_n_{depth} = aura_json_array_count({json_expr}); {index_var} < __json_n_{depth}; {index_var}++) {{"
    );
    let _ = writeln!(
        out,
        "    const char *{item_var} = aura_json_array_at({json_expr}, {index_var}); if ({item_var} == NULL) {{ __json_array_ok = false; break; }}"
    );

    if nested_key.starts_with("Array_") {
        let _ = writeln!(
            out,
            "    if (!aura_json_is_array({item_var})) {{ free((void *){item_var}); __json_array_ok = false; break; }}"
        );
        let child =
            emit_json_decode_primitive_array_level(out, &nested_key, &item_var, depth + 1, checked);
        let _ = writeln!(
            out,
            "    if (!__json_array_ok) {{ free((void *){item_var}); break; }}"
        );
        let _ = writeln!(out, "    {push}(&{array_var}, {child});");
        out.push_str(&format!(
            "    {child}.data = NULL; {child}.len = 0; {child}.cap = 0; free((void *){item_var});\n"
        ));
    } else {
        let value_var = format!("__json_value_{depth}");
        match nested_key.as_str() {
            "Int" => out.push_str(&format!(
                "    int64_t {value_var} = 0; if (!aura_json_parse_int({item_var}, &{value_var})) {{ free((void *){item_var}); __json_array_ok = false; break; }}\n"
            )),
            "Bool" => out.push_str(&format!(
                "    bool {value_var} = false; if (!aura_json_parse_bool({item_var}, &{value_var})) {{ free((void *){item_var}); __json_array_ok = false; break; }}\n"
            )),
            "String" => out.push_str(&format!(
                "    const char *{value_var} = aura_json_decode_string({item_var}); if ({value_var} == NULL) {{ free((void *){item_var}); __json_array_ok = false; break; }}\n"
            )),
            _ => unreachable!("primitive JSON array emitter received non-primitive element"),
        }
        let _ = writeln!(out, "    {push}(&{array_var}, {value_var});");
        if nested_key == "String" {
            let _ = writeln!(out, "    free((void *){value_var});");
        }
        let _ = writeln!(out, "    free((void *){item_var});");
    }
    out.push_str("  }\n");
    let _ = writeln!(
        out,
        "  if (__json_array_rooted_{depth}) {{ aura_gc_remove_array_root((void **)&{array_var}.data); __json_array_rooted_{depth} = false; }}"
    );
    let _ = writeln!(out, "  if (!__json_array_ok) {{ {drop}(&{array_var}); }}");
    out.push('\n');
    array_var
}
