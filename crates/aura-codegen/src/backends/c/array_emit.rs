//! Builtin `Array<T>` monomorphization (C3j/C3m).

use std::fmt::Write as _;

use aura_sema::{CheckedFile, Ty};

use crate::names::{
    c_class_type, c_ctor_name, c_enum_type, c_method_name, is_heap_class_mono, is_iface_type_key,
    is_primitive_name, mono_key, ty_to_c_array_elem,
};

/// Local/type key is an Array monomorph (`Array`, `Array_Int`, …).
pub(crate) fn is_array_type_key(key: &str) -> bool {
    key == "Array" || key.starts_with("Array_") || is_array_mono(key)
}

pub(crate) fn is_array_mono(name: &str) -> bool {
    name == "Array"
}

/// C6e: element type is a GC heap class (Array stores pointers that must be marked).
pub(crate) fn is_array_of_heap_class(key: &str, checked: &CheckedFile) -> bool {
    if !key.starts_with("Array_") {
        return false;
    }
    let elem = &key["Array_".len()..];
    if elem.is_empty() || is_primitive_name(elem) {
        return false;
    }
    // Interface elements are tagged unions whose heap implementors are stored
    // as pointers, so their backing buffer needs the same explicit root.
    is_heap_class_mono(elem, checked) || array_needs_typed_gc_root(key, checked)
}

/// Arrays whose elements are tagged/value aggregates need a typed GC scan;
/// the legacy runtime root treats every element as a raw pointer.
pub(crate) fn array_needs_typed_gc_root(key: &str, checked: &CheckedFile) -> bool {
    let Some(elem) = array_elem_key(key) else {
        return false;
    };
    if is_iface_type_key(elem, checked) {
        return true;
    }
    if elem.starts_with("Array_") {
        return is_array_of_heap_class(elem, checked);
    }
    let full = crate::expr::full_type_mono(elem, checked);
    crate::expr::is_enum_mono(&full, checked)
        || struct_element_mono_from_key(&full, checked).is_some()
}

/// Emit the runtime registration call matching the element representation.
pub(crate) fn array_gc_root_add_call(
    data_slot: &str,
    len_slot: &str,
    key: &str,
    checked: &CheckedFile,
) -> String {
    if array_needs_typed_gc_root(key, checked) {
        let cty = c_class_type(key);
        format!("aura_gc_add_array_root_typed((void **)&{data_slot}, &{len_slot}, {cty}_mark_elements);")
    } else {
        format!("aura_gc_add_array_root((void **)&{data_slot}, &{len_slot});")
    }
}

/// C13d: element key is owned `String` (`const char *` heap copies).
pub(crate) fn is_string_array_elem_key(elem: &str) -> bool {
    elem == "String"
}

/// Nested Array element type key (`Array_Array_String` → `Array_String`).
pub(crate) fn array_elem_key(key: &str) -> Option<&str> {
    key.strip_prefix("Array_")
}

fn elem_is_nested_array(elem: &Ty) -> bool {
    matches!(elem, Ty::ClassApp { name, .. } if aura_sema::split_nominal(name).0 == "Array")
}

fn nested_array_holds_string(elem: &Ty) -> bool {
    match elem {
        Ty::ClassApp { name, args } if aura_sema::split_nominal(name).0 == "Array" => {
            matches!(args.first(), Some(Ty::String))
        }
        _ => false,
    }
}

fn nested_array_mono(elem: &Ty) -> Option<String> {
    match elem {
        Ty::ClassApp { name, args } if aura_sema::split_nominal(name).0 == "Array" => {
            Some(mono_key("Array", args))
        }
        _ => None,
    }
}

fn foreign_handle_element(elem: &Ty) -> bool {
    matches!(elem, Ty::ForeignHandle(_))
}

/// Enum elements carry their own ownership bit.  Array's generic `memcpy`
/// path is therefore not a valid clone for them: an enum may contain String,
/// Array, or another owned enum payload.
fn enum_element_mono(elem: &Ty, checked: &CheckedFile) -> Option<String> {
    let (name, args) = match elem {
        Ty::Enum(name) | Ty::Class(name) => (name, &[][..]),
        Ty::EnumApp { name, args } | Ty::ClassApp { name, args } => (name, args.as_slice()),
        _ => return None,
    };
    let base = aura_sema::split_nominal(name).0;
    let e = checked.ast.enums.iter().find(|e| e.name.name == base)?;
    let full = crate::names::type_mono(&crate::names::enum_decl_package(e, checked), base, args);
    crate::expr::is_enum_mono(&full, checked).then_some(full)
}

fn struct_element_mono(elem: &Ty, checked: &CheckedFile) -> Option<String> {
    let (name, args) = match elem {
        Ty::Class(name) => (name, &[][..]),
        Ty::ClassApp { name, args } => (name, args.as_slice()),
        _ => return None,
    };
    let base = aura_sema::split_nominal(name).0;
    let class = checked
        .ast
        .classes
        .iter()
        .find(|class| class.kind == aura_ast::NominalKind::Struct && class.name.name == base)?;
    let local = mono_key(base, args);
    Some(crate::expr::full_type_mono(
        &format!(
            "{}@{}",
            local,
            crate::names::class_decl_package(class, checked)
        ),
        checked,
    ))
}

fn interface_element_mono(elem: &Ty, checked: &CheckedFile) -> Option<String> {
    let (name, args) = match elem {
        Ty::Interface(name) => (name, &[][..]),
        Ty::InterfaceApp { name, args } => (name, args.as_slice()),
        _ => return None,
    };
    let base = aura_sema::split_nominal(name).0;
    let interface = checked
        .ast
        .interfaces
        .iter()
        .find(|interface| interface.name.name == base)?;
    let full = crate::names::iface_mono_args(interface, checked, args);
    is_iface_type_key(&full, checked).then_some(full)
}

fn struct_element_mono_from_key(key: &str, checked: &CheckedFile) -> Option<String> {
    let full = crate::expr::full_type_mono(key, checked);
    let base = crate::expr::mono_base_name(&full, checked)?;
    checked
        .ast
        .classes
        .iter()
        .find(|class| class.kind == aura_ast::NominalKind::Struct && class.name.name == base)
        .map(|_| full)
}

/// Emit free of owned string pointers in `arr_expr.data[0..arr_expr.len)`.
/// `arr_expr` is a C lvalue/expression with `.data` / `.len` (e.g. `parts` or `this->data[__i]`).
pub(crate) fn emit_free_string_elems(out: &mut String, indent: &str, arr_expr: &str) {
    let _ = writeln!(
        out,
        "{indent}for (int64_t __sf = 0; __sf < ({arr_expr}).len; __sf++) {{"
    );
    let _ = writeln!(
        out,
        "{indent}  if (({arr_expr}).data[__sf] != NULL) {{ free((void *)({arr_expr}).data[__sf]); ({arr_expr}).data[__sf] = NULL; }}"
    );
    let _ = writeln!(out, "{indent}}}");
}

/// Emit free of one nested Array element at `nested_expr` (`.data` buffer + String elems if any).
fn emit_free_one_nested_array(
    out: &mut String,
    indent: &str,
    nested_expr: &str,
    free_strings: bool,
) {
    let _ = writeln!(out, "{indent}if (({nested_expr}).data != NULL) {{");
    if free_strings {
        emit_free_string_elems(out, &format!("{indent}  "), nested_expr);
    }
    let _ = writeln!(out, "{indent}  free(({nested_expr}).data);");
    let _ = writeln!(out, "{indent}  ({nested_expr}).data = NULL;");
    let _ = writeln!(out, "{indent}  ({nested_expr}).len = 0;");
    let _ = writeln!(out, "{indent}  ({nested_expr}).cap = 0;");
    let _ = writeln!(out, "{indent}}}");
}

/// C13d/C8f: free Array contents for a local/field (strings, nested arrays) then free the buffer.
/// Used by scope drop (`emit_free_array_local`).
pub(crate) fn emit_array_contents_free(
    out: &mut String,
    indent: usize,
    name_c: &str,
    ty_key: &str,
) {
    emit_array_contents_free_inner(out, indent, name_c, ty_key, None);
}

/// Checked variant of Array cleanup. The legacy helper remains available for
/// contexts that only have a C type key; compiler-owned cleanup should use
/// this variant so `Array<Enum>` can call the enum's recursive drop hook.
pub(crate) fn emit_array_contents_free_checked(
    out: &mut String,
    indent: usize,
    name_c: &str,
    ty_key: &str,
    checked: &CheckedFile,
) {
    emit_array_contents_free_inner(out, indent, name_c, ty_key, Some(checked));
}

fn emit_array_contents_free_inner(
    out: &mut String,
    indent: usize,
    name_c: &str,
    ty_key: &str,
    checked: Option<&CheckedFile>,
) {
    let p = " ".repeat(indent);
    let _ = writeln!(out, "{p}if ({name_c}.data != NULL) {{");
    if let Some(elem) = array_elem_key(ty_key) {
        if is_string_array_elem_key(elem) {
            emit_free_string_elems(out, &format!("{p}  "), name_c);
        } else if is_array_type_key(elem) {
            let _ = writeln!(
                out,
                "{p}  for (int64_t __af = 0; __af < {name_c}.len; __af++) {{"
            );
            let nested_expr = format!("{name_c}.data[__af]");
            if let Some(checked) = checked {
                let nested_key = crate::expr::full_type_mono(elem, checked);
                let nested_cty = c_class_type(&nested_key);
                let _ = writeln!(out, "{p}    {nested_cty}_drop(&{nested_expr});");
            } else {
                let free_strings = array_elem_key(elem).is_some_and(is_string_array_elem_key);
                emit_free_one_nested_array(out, &format!("{p}    "), &nested_expr, free_strings);
            }
            let _ = writeln!(out, "{p}  }}");
        } else if let Some(checked) = checked {
            let elem_key = crate::expr::full_type_mono(elem, checked);
            if crate::expr::is_enum_mono(&elem_key, checked) {
                let enum_cty = crate::names::c_enum_type(&elem_key);
                let _ = writeln!(
                    out,
                    "{p}  for (int64_t __af = 0; __af < {name_c}.len; __af++) {{ {enum_cty}_drop(&{name_c}.data[__af]); }}"
                );
            } else if struct_element_mono_from_key(&elem_key, checked).is_some() {
                let struct_cty = crate::stmt::local_key_to_c(&elem_key, checked);
                let _ = writeln!(
                    out,
                    "{p}  for (int64_t __af = 0; __af < {name_c}.len; __af++) {{ {struct_cty}_drop(&{name_c}.data[__af]); }}"
                );
            } else if is_iface_type_key(&elem_key, checked) {
                let iface_cty = crate::stmt::local_key_to_c(&elem_key, checked);
                let _ = writeln!(
                    out,
                    "{p}  for (int64_t __af = 0; __af < {name_c}.len; __af++) {{ {iface_cty}_drop(&{name_c}.data[__af]); }}"
                );
            } else if elem_key == "ForeignHandle" || elem_key.starts_with("ForeignHandle_") {
                let _ = writeln!(
                    out,
                    "{p}  for (int64_t __af = 0; __af < {name_c}.len; __af++) {{ if ({name_c}.data[__af] != NULL) (void)aura_ffi_handle_drop(&{name_c}.data[__af]); }}"
                );
            }
        }
    }
    let _ = writeln!(out, "{p}  free({name_c}.data);");
    let _ = writeln!(out, "{p}  {name_c}.data = NULL;");
    let _ = writeln!(out, "{p}  {name_c}.len = 0;");
    let _ = writeln!(out, "{p}  {name_c}.cap = 0;");
    let _ = writeln!(out, "{p}}}");
}

/// Return deep cleanup for an Array lvalue inside a statement expression.
/// Assignment codegen evaluates the RHS before running this cleanup, while
/// using the same element-aware drop as lexical scope teardown.
pub(crate) fn array_contents_free_expr(name_c: &str, ty_key: &str) -> String {
    let mut out = String::new();
    emit_array_contents_free(&mut out, 0, name_c, ty_key);
    out
}

/// C13d: heap-copy a `const char *` into `__sc` (NULL → NULL); aborts on OOM.
fn emit_string_heap_copy(out: &mut String, src_expr: &str, dst_var: &str) {
    let _ = writeln!(out, "  const char *{dst_var} = NULL;");
    let _ = writeln!(out, "  if (({src_expr}) != NULL) {{");
    let _ = writeln!(out, "    size_t __sn = strlen({src_expr});");
    let _ = writeln!(out, "    char *__sm = (char *)malloc(__sn + 1);");
    out.push_str("    if (__sm == NULL) {\n");
    out.push_str("      fputs(\"aura: Array string copy failed\\n\", stderr);\n");
    out.push_str("      abort();\n");
    out.push_str("    }\n");
    let _ = writeln!(out, "    if (__sn > 0) memcpy(__sm, {src_expr}, __sn);");
    out.push_str("    __sm[__sn] = '\\0';\n");
    let _ = writeln!(out, "    {dst_var} = (const char *)__sm;");
    out.push_str("  }\n");
}

pub(crate) fn emit_array_mono(out: &mut String, elem: &Ty, checked: &CheckedFile) {
    let mono = mono_key("Array", std::slice::from_ref(elem));
    let c_ty = c_class_type(&mono);
    // C4c/C4q: heap class elements are pointers; structs/primitives by value.
    let elem_c = ty_to_c_array_elem(elem, checked);
    let ctor = c_ctor_name(&mono);
    let get = c_method_name(&mono, "get");
    let set = c_method_name(&mono, "set");
    let push = c_method_name(&mono, "push");
    let pop = c_method_name(&mono, "pop");
    let clear = c_method_name(&mono, "clear");
    let is_empty = c_method_name(&mono, "isEmpty");
    let reserve = c_method_name(&mono, "reserve");
    let clone = c_method_name(&mono, "clone");
    let elem_is_string = matches!(elem, Ty::String);
    let elem_is_array = elem_is_nested_array(elem);
    let nested_holds_string = nested_array_holds_string(elem);
    let nested_mono = nested_array_mono(elem);
    let enum_elem = enum_element_mono(elem, checked);
    let struct_elem = struct_element_mono(elem, checked);
    let interface_elem = interface_element_mono(elem, checked);
    let foreign_elem = foreign_handle_element(elem);

    // Array monomorphs are emitted before enum function forwards.  Declare
    // the element ownership hooks here so the generated C remains valid even
    // when cloning/dropping an Array<Enum>.
    if let Some(enum_mono) = &enum_elem {
        let enum_cty = c_enum_type(enum_mono);
        let _ = writeln!(
            out,
            "{enum_cty} {enum_cty}_clone(const {enum_cty} *source);"
        );
        let _ = writeln!(out, "void {enum_cty}_drop({enum_cty} *value);\n");
        let _ = writeln!(out, "void {enum_cty}_mark(const {enum_cty} *value);\n");
    }
    if let Some(struct_mono) = &struct_elem {
        let struct_cty = c_class_type(struct_mono);
        let _ = writeln!(
            out,
            "{struct_cty} {struct_cty}_clone(const {struct_cty} *source);"
        );
        let _ = writeln!(out, "void {struct_cty}_drop({struct_cty} *value);\n");
        let _ = writeln!(out, "void {struct_cty}_mark(const {struct_cty} *value);\n");
    }
    if let Some(interface_mono) = &interface_elem {
        let iface_cty = crate::names::c_iface_type(interface_mono);
        let _ = writeln!(
            out,
            "{iface_cty} {iface_cty}_clone(const {iface_cty} *source);"
        );
        let _ = writeln!(out, "void {iface_cty}_drop({iface_cty} *value);");
        let _ = writeln!(out, "void {iface_cty}_mark(const {iface_cty} *value);\n");
    }
    if let Some(nested_mono) = &nested_mono {
        let nested_cty = c_class_type(nested_mono);
        let nested_clone = c_method_name(nested_mono, "clone");
        let _ = writeln!(out, "{nested_cty} {nested_clone}({nested_cty} *this);");
        let _ = writeln!(out, "void {nested_cty}_drop({nested_cty} *value);");
        let _ = writeln!(out, "void {nested_cty}_mark(const {nested_cty} *value);\n");
    }

    let _ = writeln!(out, "typedef struct {c_ty} {{");
    out.push_str("  int64_t len;\n");
    out.push_str("  int64_t cap;\n");
    let _ = writeln!(out, "  {elem_c} *data;");
    let _ = writeln!(out, "}} {c_ty};\n");

    // Async frames may retain an Array across a suspension.  Keep the
    // element graph visible to the collector for every owned element shape,
    // not only Array<HeapClass>.  Enum/struct elements recurse through their
    // generated hooks; nested arrays recurse through this same hook.
    let array_mark = format!("{c_ty}_mark");
    let elem_key = array_elem_key(&mono).map(|key| crate::expr::full_type_mono(key, checked));
    let elem_is_heap = elem_key
        .as_deref()
        .is_some_and(|key| is_heap_class_mono(key, checked));
    let nested_mark = nested_mono
        .as_deref()
        .map(|key| format!("{}_mark", c_class_type(key)));
    let _ = writeln!(out, "void {array_mark}(const {c_ty} *value) {{");
    out.push_str("  if (value == NULL || value->data == NULL) return;\n");
    out.push_str("  for (int64_t __gm = 0; __gm < value->len; __gm++) {\n");
    if elem_is_heap {
        out.push_str("    aura_gc_mark_ptr((void *)value->data[__gm]);\n");
    } else if let Some(enum_mono) = &enum_elem {
        let elem_cty = c_enum_type(enum_mono);
        let _ = writeln!(out, "    {elem_cty}_mark(&value->data[__gm]);");
    } else if let Some(struct_mono) = &struct_elem {
        let elem_cty = c_class_type(struct_mono);
        let _ = writeln!(out, "    {elem_cty}_mark(&value->data[__gm]);");
    } else if let Some(interface_mono) = &interface_elem {
        let elem_cty = crate::names::c_iface_type(interface_mono);
        let _ = writeln!(out, "    {elem_cty}_mark(&value->data[__gm]);");
    } else if let Some(nested_mark) = nested_mark {
        let _ = writeln!(out, "    {nested_mark}(&value->data[__gm]);");
    }
    out.push_str("  }\n}\n\n");
    let _ = writeln!(
        out,
        "static void {c_ty}_mark_elements(const void *data, int64_t len) {{"
    );
    out.push_str("  if (data == NULL || len <= 0) return;\n");
    let _ = writeln!(
        out,
        "  {c_ty} view = {{ .len = len, .cap = len, .data = ({elem_c} *)data }};"
    );
    let _ = writeln!(out, "  {array_mark}(&view);");
    out.push_str("}\n\n");

    // Constructor: Array(len) — zero-initialized buffer; cap == len.
    let _ = writeln!(out, "{c_ty} {ctor}(int64_t len) {{");
    let _ = writeln!(out, "  {c_ty} self;");
    out.push_str("  if (len < 0) { len = 0; }\n");
    out.push_str("  self.len = len;\n");
    out.push_str("  self.cap = len;\n");
    let _ = writeln!(
        out,
        "  self.data = ({elem_c} *)calloc((size_t)len, sizeof({elem_c}));"
    );
    out.push_str("  if (self.data == NULL && len > 0) {\n");
    out.push_str("    fputs(\"aura: Array allocation failed\\n\", stderr);\n");
    out.push_str("    abort();\n");
    out.push_str("  }\n");
    out.push_str("  return self;\n}\n\n");

    // get(i)
    // C13d: String elems return a heap copy so callers (e.g. join) can free the Array
    // without invalidating a returned String that aliased data[i].
    let _ = writeln!(out, "{elem_c} {get}({c_ty} *this, int64_t i) {{");
    out.push_str("  if (this == NULL || this->data == NULL || i < 0 || i >= this->len) {\n");
    out.push_str("    aura_throw_string(\"Array index out of bounds\");\n");
    out.push_str("  }\n");
    if elem_is_string {
        out.push_str("  {\n");
        out.push_str("    const char *__v = this->data[i];\n");
        out.push_str("    if (__v == NULL) { return NULL; }\n");
        out.push_str("    size_t __sn = strlen(__v);\n");
        out.push_str("    char *__sm = (char *)malloc(__sn + 1);\n");
        out.push_str("    if (__sm == NULL) {\n");
        out.push_str("      fputs(\"aura: Array get string copy failed\\n\", stderr);\n");
        out.push_str("      abort();\n");
        out.push_str("    }\n");
        out.push_str("    if (__sn > 0) memcpy(__sm, __v, __sn);\n");
        out.push_str("    __sm[__sn] = '\\0';\n");
        out.push_str("    return (const char *)__sm;\n");
        out.push_str("  }\n");
    } else if elem_is_array {
        if let Some(nested_mono) = &nested_mono {
            let nested_clone = c_method_name(nested_mono, "clone");
            let _ = writeln!(out, "  return {nested_clone}(&this->data[i]);");
        } else {
            out.push_str("  return this->data[i];\n");
        }
    } else if let Some(enum_mono) = &enum_elem {
        let enum_cty = c_enum_type(enum_mono);
        let _ = writeln!(out, "  return {enum_cty}_clone(&this->data[i]);");
    } else if let Some(struct_mono) = &struct_elem {
        let struct_cty = c_class_type(struct_mono);
        let _ = writeln!(out, "  return {struct_cty}_clone(&this->data[i]);");
    } else if let Some(interface_mono) = &interface_elem {
        let iface_cty = crate::names::c_iface_type(interface_mono);
        let _ = writeln!(out, "  return {iface_cty}_clone(&this->data[i]);");
    } else if foreign_elem {
        out.push_str("  if (this->data[i] != NULL && aura_ffi_handle_retain(this->data[i]) != AURA_FFI_OK) return NULL;\n");
        out.push_str("  return this->data[i];\n");
    } else {
        out.push_str("  return this->data[i];\n");
    }
    out.push_str("}\n\n");

    // set(i, v)
    let _ = writeln!(out, "void {set}({c_ty} *this, int64_t i, {elem_c} v) {{");
    out.push_str("  if (this == NULL || this->data == NULL || i < 0 || i >= this->len) {\n");
    out.push_str("    aura_throw_string(\"Array index out of bounds\");\n");
    out.push_str("  }\n");
    if elem_is_string {
        // C13d: free previous owned string; store heap copy so free is safe for literals.
        // Copy first so `set(i, get(i))` is not use-after-free.
        emit_string_heap_copy(out, "v", "__sc");
        out.push_str("  if (this->data[i] != NULL) { free((void *)this->data[i]); }\n");
        out.push_str("  this->data[i] = __sc;\n");
    } else if elem_is_array {
        // C8f/C13d: free previous nested Array buffer (and String elems if Array_String).
        if let Some(nested_mono) = &nested_mono {
            let nested_cty = c_class_type(nested_mono);
            let _ = writeln!(out, "  {nested_cty}_drop(&this->data[i]);");
        } else {
            emit_free_one_nested_array(out, "  ", "this->data[i]", nested_holds_string);
        }
        out.push_str("  this->data[i] = v;\n");
    } else if let Some(enum_mono) = &enum_elem {
        let enum_cty = c_enum_type(enum_mono);
        let _ = writeln!(
            out,
            "  {enum_cty} __copy = {enum_cty}_clone(&v); {enum_cty}_drop(&this->data[i]); this->data[i] = __copy;"
        );
    } else if let Some(struct_mono) = &struct_elem {
        let struct_cty = c_class_type(struct_mono);
        let _ = writeln!(
            out,
            "  {struct_cty} __copy = {struct_cty}_clone(&v); {struct_cty}_drop(&this->data[i]); this->data[i] = __copy;"
        );
    } else if let Some(interface_mono) = &interface_elem {
        let iface_cty = crate::names::c_iface_type(interface_mono);
        let _ = writeln!(
            out,
            "  {iface_cty} __copy = {iface_cty}_clone(&v); {iface_cty}_drop(&this->data[i]); this->data[i] = __copy;"
        );
    } else if foreign_elem {
        out.push_str("  if (v != NULL && aura_ffi_handle_retain(v) != AURA_FFI_OK) return;\n");
        out.push_str("  if (this->data[i] != NULL) (void)aura_ffi_handle_drop(&this->data[i]);\n");
        out.push_str("  this->data[i] = v;\n");
    } else {
        out.push_str("  this->data[i] = v;\n");
    }
    out.push_str("}\n\n");

    // push(v) — grow by doubling (min cap 4) via realloc (C3m).
    let _ = writeln!(out, "void {push}({c_ty} *this, {elem_c} v) {{");
    out.push_str("  if (this == NULL) {\n");
    out.push_str("    aura_throw_string(\"Array push on null\");\n");
    out.push_str("  }\n");
    out.push_str("  if (this->len >= this->cap) {\n");
    out.push_str("    int64_t ncap = this->cap < 4 ? 4 : this->cap * 2;\n");
    let _ = writeln!(
        out,
        "    {elem_c} *nd = ({elem_c} *)realloc(this->data, (size_t)ncap * sizeof({elem_c}));"
    );
    out.push_str("    if (nd == NULL) {\n");
    out.push_str("      fputs(\"aura: Array reallocation failed\\n\", stderr);\n");
    out.push_str("      abort();\n");
    out.push_str("    }\n");
    out.push_str("    this->data = nd;\n");
    out.push_str("    this->cap = ncap;\n");
    out.push_str("  }\n");
    if elem_is_string {
        // C13d: heap-copy so drop/clear free is safe for string literals.
        emit_string_heap_copy(out, "v", "__sc");
        out.push_str("  this->data[this->len] = __sc;\n");
    } else if let Some(enum_mono) = &enum_elem {
        let enum_cty = c_enum_type(enum_mono);
        let _ = writeln!(out, "  this->data[this->len] = {enum_cty}_clone(&v);");
    } else if let Some(struct_mono) = &struct_elem {
        let struct_cty = c_class_type(struct_mono);
        let _ = writeln!(out, "  this->data[this->len] = {struct_cty}_clone(&v);");
    } else if let Some(interface_mono) = &interface_elem {
        let iface_cty = crate::names::c_iface_type(interface_mono);
        let _ = writeln!(out, "  this->data[this->len] = {iface_cty}_clone(&v);");
    } else if foreign_elem {
        out.push_str("  if (v != NULL && aura_ffi_handle_retain(v) != AURA_FFI_OK) return;\n");
        out.push_str("  this->data[this->len] = v;\n");
    } else {
        out.push_str("  this->data[this->len] = v;\n");
    }
    out.push_str("  this->len += 1;\n");
    out.push_str("}\n\n");

    // pop() — remove last element; throw if empty (C3r). Capacity unchanged.
    // Ownership of the element transfers to the caller (String: caller owns the pointer).
    let _ = writeln!(out, "{elem_c} {pop}({c_ty} *this) {{");
    out.push_str("  if (this == NULL || this->data == NULL || this->len <= 0) {\n");
    out.push_str("    aura_throw_string(\"Array pop on empty\");\n");
    out.push_str("  }\n");
    out.push_str("  this->len -= 1;\n");
    out.push_str("  return this->data[this->len];\n");
    out.push_str("}\n\n");

    // clear() — set len = 0; keep capacity and buffer (C4f).
    // C8f: free nested Array element buffers before clearing len.
    // C13d: free owned String elems before clearing len.
    let _ = writeln!(out, "void {clear}({c_ty} *this) {{");
    out.push_str("  if (this == NULL) {\n");
    out.push_str("    aura_throw_string(\"Array clear on null\");\n");
    out.push_str("  }\n");
    if elem_is_string {
        out.push_str("  if (this->data != NULL) {\n");
        emit_free_string_elems(out, "    ", "(*this)");
        out.push_str("  }\n");
    } else if elem_is_array {
        out.push_str("  if (this->data != NULL) {\n");
        out.push_str("    for (int64_t __i = 0; __i < this->len; __i++) {\n");
        if let Some(nested_mono) = &nested_mono {
            let nested_cty = c_class_type(nested_mono);
            let _ = writeln!(out, "      {nested_cty}_drop(&this->data[__i]);");
        } else {
            emit_free_one_nested_array(out, "      ", "this->data[__i]", nested_holds_string);
        }
        out.push_str("    }\n");
        out.push_str("  }\n");
    } else if let Some(enum_mono) = &enum_elem {
        let enum_cty = c_enum_type(enum_mono);
        let _ = writeln!(out, "  if (this->data != NULL) {{");
        let _ = writeln!(out, "    for (int64_t __i = 0; __i < this->len; __i++) {{ {enum_cty}_drop(&this->data[__i]); }}");
        out.push_str("  }\n");
    } else if let Some(struct_mono) = &struct_elem {
        let struct_cty = c_class_type(struct_mono);
        let _ = writeln!(out, "  if (this->data != NULL) {{ for (int64_t __i = 0; __i < this->len; __i++) {{ {struct_cty}_drop(&this->data[__i]); }} }}");
    } else if foreign_elem {
        out.push_str("  if (this->data != NULL) {\n");
        out.push_str("    for (int64_t __i = 0; __i < this->len; __i++) {\n");
        out.push_str(
            "      if (this->data[__i] != NULL) (void)aura_ffi_handle_drop(&this->data[__i]);\n",
        );
        out.push_str("    }\n");
        out.push_str("  }\n");
    }
    out.push_str("  this->len = 0;\n");
    out.push_str("}\n\n");

    // isEmpty() — C4n.
    let _ = writeln!(out, "bool {is_empty}({c_ty} *this) {{");
    out.push_str("  if (this == NULL) {\n");
    out.push_str("    aura_throw_string(\"Array isEmpty on null\");\n");
    out.push_str("  }\n");
    out.push_str("  return this->len == 0;\n");
    out.push_str("}\n\n");

    // reserve(n) — grow capacity only (C4o).
    let _ = writeln!(out, "void {reserve}({c_ty} *this, int64_t n) {{");
    out.push_str("  if (this == NULL) {\n");
    out.push_str("    aura_throw_string(\"Array reserve on null\");\n");
    out.push_str("  }\n");
    out.push_str("  if (n <= this->cap) { return; }\n");
    let _ = writeln!(
        out,
        "  {elem_c} *nd = ({elem_c} *)realloc(this->data, (size_t)n * sizeof({elem_c}));"
    );
    out.push_str("  if (nd == NULL) {\n");
    out.push_str("    fputs(\"aura: Array reallocation failed\\n\", stderr);\n");
    out.push_str("    abort();\n");
    out.push_str("  }\n");
    out.push_str("  this->data = nd;\n");
    out.push_str("  this->cap = n;\n");
    out.push_str("}\n\n");

    // clone() — owning copy of buffer (C9c). Nested Array elems get deep buffer copies.
    // C13d: String elems are heap-copied so clone + original can each free safely.
    let _ = writeln!(out, "{c_ty} {clone}({c_ty} *this) {{");
    let _ = writeln!(out, "  {c_ty} out;");
    out.push_str("  if (this == NULL) {\n");
    out.push_str("    aura_throw_string(\"Array clone on null\");\n");
    out.push_str("  }\n");
    out.push_str("  out.len = this->len;\n");
    out.push_str("  out.cap = this->len;\n");
    out.push_str("  if (this->len <= 0 || this->data == NULL) {\n");
    out.push_str("    out.data = NULL;\n");
    out.push_str("    out.len = 0;\n");
    out.push_str("    out.cap = 0;\n");
    out.push_str("    return out;\n");
    out.push_str("  }\n");
    let _ = writeln!(
        out,
        "  out.data = ({elem_c} *)malloc((size_t)this->len * sizeof({elem_c}));"
    );
    out.push_str("  if (out.data == NULL) {\n");
    out.push_str("    fputs(\"aura: Array clone allocation failed\\n\", stderr);\n");
    out.push_str("    abort();\n");
    out.push_str("  }\n");
    if elem_is_string {
        out.push_str("  for (int64_t __i = 0; __i < this->len; __i++) {\n");
        out.push_str("    if (this->data[__i] != NULL) {\n");
        out.push_str("      size_t __sn = strlen(this->data[__i]);\n");
        out.push_str("      char *__sm = (char *)malloc(__sn + 1);\n");
        out.push_str("      if (__sm == NULL) {\n");
        out.push_str("        fputs(\"aura: Array string clone failed\\n\", stderr);\n");
        out.push_str("        abort();\n");
        out.push_str("      }\n");
        out.push_str("      if (__sn > 0) memcpy(__sm, this->data[__i], __sn);\n");
        out.push_str("      __sm[__sn] = '\\0';\n");
        out.push_str("      out.data[__i] = (const char *)__sm;\n");
        out.push_str("    } else {\n");
        out.push_str("      out.data[__i] = NULL;\n");
        out.push_str("    }\n");
        out.push_str("  }\n");
    } else if elem_is_array {
        // Deep-copy nested Array element buffers.
        out.push_str("  for (int64_t __i = 0; __i < this->len; __i++) {\n");
        if let Some(nested_mono) = &nested_mono {
            let nested_clone = c_method_name(nested_mono, "clone");
            let _ = writeln!(out, "    out.data[__i] = {nested_clone}(&this->data[__i]);");
            out.push_str("  }\n");
        } else {
            out.push_str("    out.data[__i].len = this->data[__i].len;\n");
            out.push_str("    out.data[__i].cap = this->data[__i].len;\n");
            out.push_str("    if (this->data[__i].len > 0 && this->data[__i].data != NULL) {\n");
            out.push_str("      size_t __esz = sizeof(*this->data[__i].data);\n");
            out.push_str(
                "      out.data[__i].data = malloc((size_t)this->data[__i].len * __esz);\n",
            );
            out.push_str("      if (out.data[__i].data == NULL) {\n");
            out.push_str("        fputs(\"aura: Array nested clone failed\\n\", stderr);\n");
            out.push_str("        abort();\n");
            out.push_str("      }\n");
            if nested_holds_string {
                // C13d: deep-copy each String pointer in nested Array_String.
                out.push_str("      for (int64_t __j = 0; __j < this->data[__i].len; __j++) {\n");
                out.push_str("        if (this->data[__i].data[__j] != NULL) {\n");
                out.push_str("          size_t __sn = strlen(this->data[__i].data[__j]);\n");
                out.push_str("          char *__sm = (char *)malloc(__sn + 1);\n");
                out.push_str("          if (__sm == NULL) {\n");
                out.push_str(
                    "            fputs(\"aura: Array nested string clone failed\\n\", stderr);\n",
                );
                out.push_str("            abort();\n");
                out.push_str("          }\n");
                out.push_str(
                    "          if (__sn > 0) memcpy(__sm, this->data[__i].data[__j], __sn);\n",
                );
                out.push_str("          __sm[__sn] = '\\0';\n");
                out.push_str("          out.data[__i].data[__j] = (const char *)__sm;\n");
                out.push_str("        } else {\n");
                out.push_str("          out.data[__i].data[__j] = NULL;\n");
                out.push_str("        }\n");
                out.push_str("      }\n");
            } else {
                out.push_str(
                "      memcpy(out.data[__i].data, this->data[__i].data, (size_t)this->data[__i].len * __esz);\n",
            );
            }
            out.push_str("    } else {\n");
            out.push_str("      out.data[__i].data = NULL;\n");
            out.push_str("      out.data[__i].len = 0;\n");
            out.push_str("      out.data[__i].cap = 0;\n");
            out.push_str("    }\n");
            out.push_str("  }\n");
        }
    } else if let Some(enum_mono) = &enum_elem {
        let enum_cty = c_enum_type(enum_mono);
        let _ = writeln!(out, "  for (int64_t __i = 0; __i < this->len; __i++) {{ out.data[__i] = {enum_cty}_clone(&this->data[__i]); }}");
    } else if let Some(struct_mono) = &struct_elem {
        let struct_cty = c_class_type(struct_mono);
        let _ = writeln!(out, "  for (int64_t __i = 0; __i < this->len; __i++) {{ out.data[__i] = {struct_cty}_clone(&this->data[__i]); }}");
    } else if let Some(interface_mono) = &interface_elem {
        let interface_cty = crate::names::c_iface_type(interface_mono);
        let _ = writeln!(out, "  for (int64_t __i = 0; __i < this->len; __i++) {{ out.data[__i] = {interface_cty}_clone(&this->data[__i]); }}");
    } else if foreign_elem {
        out.push_str("  for (int64_t __i = 0; __i < this->len; __i++) {\n");
        out.push_str("    out.data[__i] = this->data[__i];\n");
        out.push_str(
            "    if (out.data[__i] != NULL) (void)aura_ffi_handle_retain(out.data[__i]);\n",
        );
        out.push_str("  }\n");
    } else {
        out.push_str("  memcpy(out.data, this->data, (size_t)this->len * sizeof(*out.data));\n");
    }
    out.push_str("  return out;\n");
    out.push_str("}\n\n");

    // Typed destructor used by nested arrays and aggregate enum fields.
    let _ = writeln!(out, "void {c_ty}_drop({c_ty} *value) {{");
    out.push_str("  if (value == NULL) return;\n");
    let mut cleanup = String::new();
    emit_array_contents_free_checked(&mut cleanup, 2, "(*value)", &mono, checked);
    out.push_str(&cleanup);
    out.push_str("}\n\n");
}
