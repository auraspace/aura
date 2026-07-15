//! Builtin `Array<T>` monomorphization (C3j).

use std::fmt::Write as _;

use aura_sema::Ty;

use crate::names::{c_class_type, c_ctor_name, c_method_name, mono_key, ty_to_c};

pub(crate) fn is_array_mono(name: &str) -> bool {
    name == "Array"
}

pub(crate) fn emit_array_mono(out: &mut String, elem: &Ty) {
    let mono = mono_key("Array", std::slice::from_ref(elem));
    let c_ty = c_class_type(&mono);
    let elem_c = ty_to_c(elem);
    let ctor = c_ctor_name(&mono);
    let get = c_method_name(&mono, "get");
    let set = c_method_name(&mono, "set");

    let _ = writeln!(out, "typedef struct {c_ty} {{");
    out.push_str("  int64_t len;\n");
    let _ = writeln!(out, "  {elem_c} *data;");
    let _ = writeln!(out, "}} {c_ty};\n");

    // Constructor: Array(len) — zero-initialized buffer.
    let _ = writeln!(out, "{c_ty} {ctor}(int64_t len) {{");
    let _ = writeln!(out, "  {c_ty} self;");
    out.push_str("  if (len < 0) { len = 0; }\n");
    out.push_str("  self.len = len;\n");
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
    let _ = writeln!(out, "{elem_c} {get}({c_ty} *this, int64_t i) {{");
    out.push_str("  if (this == NULL || this->data == NULL || i < 0 || i >= this->len) {\n");
    out.push_str("    aura_throw_string(\"Array index out of bounds\");\n");
    out.push_str("  }\n");
    // throw never returns, but silence compilers
    out.push_str("  return this->data[i];\n}\n\n");

    // set(i, v)
    let _ = writeln!(out, "void {set}({c_ty} *this, int64_t i, {elem_c} v) {{");
    out.push_str("  if (this == NULL || this->data == NULL || i < 0 || i >= this->len) {\n");
    out.push_str("    aura_throw_string(\"Array index out of bounds\");\n");
    out.push_str("  }\n");
    out.push_str("  this->data[i] = v;\n}\n\n");
}
