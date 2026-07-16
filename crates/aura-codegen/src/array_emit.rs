//! Builtin `Array<T>` monomorphization (C3j/C3m).

use std::fmt::Write as _;

use aura_sema::{CheckedFile, Ty};

use crate::names::{c_class_type, c_ctor_name, c_method_name, mono_key, ty_to_c_array_elem};

pub(crate) fn is_array_mono(name: &str) -> bool {
    name == "Array"
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

    let _ = writeln!(out, "typedef struct {c_ty} {{");
    out.push_str("  int64_t len;\n");
    out.push_str("  int64_t cap;\n");
    let _ = writeln!(out, "  {elem_c} *data;");
    let _ = writeln!(out, "}} {c_ty};\n");

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
    let _ = writeln!(out, "{elem_c} {get}({c_ty} *this, int64_t i) {{");
    out.push_str("  if (this == NULL || this->data == NULL || i < 0 || i >= this->len) {\n");
    out.push_str("    aura_throw_string(\"Array index out of bounds\");\n");
    out.push_str("  }\n");
    out.push_str("  return this->data[i];\n}\n\n");

    // set(i, v)
    let _ = writeln!(out, "void {set}({c_ty} *this, int64_t i, {elem_c} v) {{");
    out.push_str("  if (this == NULL || this->data == NULL || i < 0 || i >= this->len) {\n");
    out.push_str("    aura_throw_string(\"Array index out of bounds\");\n");
    out.push_str("  }\n");
    out.push_str("  this->data[i] = v;\n}\n\n");

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
    out.push_str("  this->data[this->len] = v;\n");
    out.push_str("  this->len += 1;\n");
    out.push_str("}\n\n");

    // pop() — remove last element; throw if empty (C3r). Capacity unchanged.
    let _ = writeln!(out, "{elem_c} {pop}({c_ty} *this) {{");
    out.push_str("  if (this == NULL || this->data == NULL || this->len <= 0) {\n");
    out.push_str("    aura_throw_string(\"Array pop on empty\");\n");
    out.push_str("  }\n");
    out.push_str("  this->len -= 1;\n");
    out.push_str("  return this->data[this->len];\n");
    out.push_str("}\n\n");

    // clear() — set len = 0; keep capacity and buffer (C4f).
    let _ = writeln!(out, "void {clear}({c_ty} *this) {{");
    out.push_str("  if (this == NULL) {\n");
    out.push_str("    aura_throw_string(\"Array clear on null\");\n");
    out.push_str("  }\n");
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
}
