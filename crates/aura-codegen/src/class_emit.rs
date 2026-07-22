//! Class/struct typedefs, constructors, and methods.

use std::collections::HashMap;
use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::ctx::EmitCtx;
use crate::iface::emit_upcast;
use crate::names::*;
use crate::stmt::{emit_block, emit_return_fallback};

pub(crate) fn emit_class_typedef(
    out: &mut String,
    checked: &CheckedFile,
    c: &ClassDecl,
    args: &[Ty],
) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    // Body only — incomplete `typedef struct X X` may already exist (C4u forwards).
    let _ = writeln!(out, "struct {} {{", c_class_type(&mono));
    for f in &c.fields {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&f.ty, checked, &params, args),
            mangle_ident(&f.name.name)
        );
    }
    if c.fields.is_empty() {
        out.push_str("  char _pad;\n");
    }
    out.push_str("};\n\n");
}

pub(crate) fn emit_class_forwards(
    out: &mut String,
    checked: &CheckedFile,
    c: &ClassDecl,
    args: &[Ty],
) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    let _ = writeln!(
        out,
        "{};",
        c_ctor_signature_mono(c, checked, &params, args, &mono)
    );
    for m in &c.methods {
        let _ = writeln!(
            out,
            "{};",
            c_method_signature_mono(c, m, checked, &params, args, &mono)
        );
    }
    // C9a: upcast forwards for non-generic and mono generic class implements.
    for iface_ref in &c.implements {
        if let Some(iface) = checked
            .ast
            .interfaces
            .iter()
            .find(|i| i.name.name == iface_ref.name.name)
        {
            let iargs = crate::iface::iface_args_for_class_implements(c, iface_ref, checked, args);
            if iface_ref.type_args.len() != iargs.len() {
                continue;
            }
            if iargs.iter().any(|a| a.is_open()) {
                continue;
            }
            let imono = iface_mono_args(iface, checked, &iargs);
            let param_ty = if is_heap_class_decl(c) {
                format!("{} *", c_class_type(&mono))
            } else {
                c_class_type(&mono)
            };
            let _ = writeln!(
                out,
                "{} {}({param_ty} v);",
                c_iface_type(&imono),
                c_upcast_name(&mono, &imono),
            );
        }
    }
}

/// C7b: field names that are builtin `Array` (any element type).
fn array_field_names(c: &ClassDecl, params: &[String], args: &[Ty]) -> Vec<String> {
    c.fields
        .iter()
        .filter(|f| {
            let key = type_ref_local_key(&f.ty, params, args);
            crate::array_emit::is_array_type_key(&key)
        })
        .map(|f| f.name.name.clone())
        .collect()
}

/// C7b: field names that are Array-of-heap-class (need mark_extras).
fn array_of_class_field_names(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<String> {
    c.fields
        .iter()
        .filter(|f| {
            let key = type_ref_local_key(&f.ty, params, args);
            let mono = crate::expr::full_type_mono(&key, checked);
            crate::array_emit::is_array_of_heap_class(&mono, checked)
        })
        .map(|f| f.name.name.clone())
        .collect()
}

fn c_dtor_name(mono: &str) -> String {
    format!("aura_dtor_{mono}")
}

fn c_markex_name(mono: &str) -> String {
    format!("aura_markex_{mono}")
}

/// Emit optional dtor / mark_extras for heap classes with Array fields (C7b).
fn emit_class_gc_hooks(
    out: &mut String,
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) {
    if !is_heap_class_decl(c) {
        return;
    }
    let cty = c_class_type(mono);
    let arr_fields = array_field_names(c, params, args);
    let arr_cls_fields = array_of_class_field_names(c, checked, params, args);
    if !arr_fields.is_empty() {
        let _ = writeln!(out, "static void {}(void *p) {{", c_dtor_name(mono));
        let _ = writeln!(out, "  {cty} *self = ({cty} *)p;");
        out.push_str("  if (self == NULL) { return; }\n");
        for name in &arr_fields {
            let f = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (self->{f}.data != NULL) {{ free(self->{f}.data); self->{f}.data = NULL; self->{f}.len = 0; self->{f}.cap = 0; }}"
            );
        }
        out.push_str("}\n\n");
    }
    if !arr_cls_fields.is_empty() {
        let _ = writeln!(out, "static void {}(void *p) {{", c_markex_name(mono));
        let _ = writeln!(out, "  {cty} *self = ({cty} *)p;");
        out.push_str("  if (self == NULL) { return; }\n");
        for name in &arr_cls_fields {
            let f = mangle_ident(name);
            let _ = writeln!(out, "  {{");
            let _ = writeln!(out, "    void **__data = (void **)self->{f}.data;");
            let _ = writeln!(out, "    int64_t __len = self->{f}.len;");
            out.push_str("    if (__data != NULL && __len > 0) {\n");
            out.push_str("      for (int64_t __i = 0; __i < __len; __i++) {\n");
            out.push_str("        aura_gc_mark_ptr(__data[__i]);\n");
            out.push_str("      }\n");
            out.push_str("    }\n");
            out.push_str("  }\n");
        }
        out.push_str("}\n\n");
    }
}

pub(crate) fn emit_class_defs(out: &mut String, checked: &CheckedFile, c: &ClassDecl, args: &[Ty]) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    emit_class_gc_hooks(out, c, checked, &params, args, &mono);
    emit_ctor_mono(out, c, checked, &params, args, &mono);
    out.push('\n');
    for m in &c.methods {
        emit_method_mono(out, c, m, checked, &params, args, &mono);
        out.push('\n');
    }
    // C9a: emit upcasts for this class monomorph's implements.
    for iface_ref in &c.implements {
        if let Some(iface) = checked
            .ast
            .interfaces
            .iter()
            .find(|i| i.name.name == iface_ref.name.name)
        {
            let iargs = crate::iface::iface_args_for_class_implements(c, iface_ref, checked, args);
            if iface_ref.type_args.len() != iargs.len() {
                continue;
            }
            if iargs.iter().any(|a| a.is_open()) {
                continue;
            }
            emit_upcast(out, checked, c, iface, &iargs, args);
            out.push('\n');
        }
    }
}

pub(crate) fn c_ctor_signature_mono(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> String {
    let ret = if is_heap_class_decl(c) {
        format!("{} *", c_class_type(mono))
    } else {
        c_class_type(mono)
    };
    let ps = if c.fields.is_empty() {
        "void".into()
    } else {
        c.fields
            .iter()
            .map(|f| {
                format!(
                    "{} {}",
                    c_type_ref_subst(&f.ty, checked, params, args),
                    mangle_ident(&f.name.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("{ret} {}({ps})", c_ctor_name(mono))
}

pub(crate) fn c_method_signature_mono(
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) -> String {
    let _ = c;
    let ret = c_type_from_opt(&m.return_type, checked, params, args);
    let mut ps = vec![format!("{} *this", c_class_type(mono))];
    for p in &m.params {
        ps.push(format!(
            "{} {}",
            c_type_ref_subst(&p.ty, checked, params, args),
            mangle_ident(&p.name.name)
        ));
    }
    format!(
        "{ret} {}({})",
        c_method_name(mono, &m.name.name),
        ps.join(", ")
    )
}

pub(crate) fn c_fun_signature(f: &FunDecl, checked: &CheckedFile, args: &[Ty]) -> String {
    let params: Vec<String> = f.type_params.iter().map(|p| p.name.name.clone()).collect();
    let ret = c_type_from_opt(&f.return_type, checked, &params, args);
    let ps = if f.params.is_empty() {
        "void".into()
    } else {
        f.params
            .iter()
            .map(|p| {
                format!(
                    "{} {}",
                    c_type_ref_subst(&p.ty, checked, &params, args),
                    mangle_ident(&p.name.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let pkg = fun_decl_package(f, checked);
    format!("{ret} {}({ps})", c_fun_name(&pkg, &f.name.name, args))
}

pub(crate) fn emit_ctor_mono(
    out: &mut String,
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) {
    let _ = writeln!(
        out,
        "{} {{",
        c_ctor_signature_mono(c, checked, params, args, mono)
    );
    let cty = c_class_type(mono);
    if is_heap_class_decl(c) {
        // C3y: allocate class instance on GC heap.
        // C7b: pass dtor / mark_extras when the class owns Array fields.
        let arr_fields = array_field_names(c, params, args);
        let arr_cls = array_of_class_field_names(c, checked, params, args);
        let dtor = if arr_fields.is_empty() {
            "NULL".to_string()
        } else {
            c_dtor_name(mono)
        };
        let markex = if arr_cls.is_empty() {
            "NULL".to_string()
        } else {
            c_markex_name(mono)
        };
        if dtor == "NULL" && markex == "NULL" {
            let _ = writeln!(
                out,
                "  {cty} *self = ({cty} *)aura_gc_alloc(sizeof({cty}));"
            );
        } else {
            let _ = writeln!(
                out,
                "  {cty} *self = ({cty} *)aura_gc_alloc_full(sizeof({cty}), {dtor}, {markex});"
            );
        }
        for f in &c.fields {
            let n = mangle_ident(&f.name.name);
            let _ = writeln!(out, "  self->{n} = {n};");
        }
        out.push_str("  return self;\n}\n");
    } else {
        let _ = writeln!(out, "  {cty} self;");
        for f in &c.fields {
            let n = mangle_ident(&f.name.name);
            let _ = writeln!(out, "  self.{n} = {n};");
        }
        out.push_str("  return self;\n}\n");
    }
}

pub(crate) fn emit_method_mono(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
) {
    let _ = writeln!(
        out,
        "{} {{",
        c_method_signature_mono(c, m, checked, params, args, mono)
    );
    let ret_key = m
        .return_type
        .as_ref()
        .map(|t| type_ref_local_key_expand(t, params, args, checked));
    let mut ctx = EmitCtx {
        checked,
        method_class: Some(mono),
        type_params: params.to_vec(),
        type_args: args.to_vec(),
        locals: vec![HashMap::new()],
        array_owners: vec![std::collections::HashSet::new()],
        fun_owners: vec![std::collections::HashSet::new()],
        box_locals: vec![std::collections::HashSet::new()],
        box_owners: vec![std::collections::HashSet::new()],
        gc_roots: vec![std::collections::HashSet::new()],
        array_gc_roots: vec![std::collections::HashSet::new()],
        return_key: ret_key,
        lambda_ids: crate::emit::build_lambda_ids(checked),
    };
    for f in &c.fields {
        let key = type_ref_local_key_expand(&f.ty, params, args, checked);
        let mono_key = crate::expr::full_type_mono(&key, checked);
        ctx.define_local(&f.name.name, mono_key);
    }
    for p in &m.params {
        let key = type_ref_local_key_expand(&p.ty, params, args, checked);
        let mono_key = crate::expr::full_type_mono(&key, checked);
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
        if is_heap_class_mono(&mono_key, checked) {
            ctx.mark_gc_root(&p.name.name);
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(out, "  aura_gc_add_root((void **)&{n});");
        }
        // C6e: Array-of-class params keep element GC pointers alive.
        if crate::array_emit::is_array_of_heap_class(&mono_key, checked) {
            ctx.mark_array_gc_root(&p.name.name);
            let n = mangle_ident(&p.name.name);
            let _ = writeln!(
                out,
                "  aura_gc_add_array_root((void **)&{n}.data, &{n}.len);"
            );
        }
    }
    // `this` is a heap pointer for classes — root it for the method body.
    if is_heap_class_mono(mono, checked) {
        ctx.mark_gc_root("this");
        out.push_str("  aura_gc_add_root((void **)&this);\n");
    }
    emit_block(out, &m.body, 1, &mut ctx);
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
    crate::stmt::emit_free_fun_owners(out, 1, &ctx, &ctx.fun_owners_all());
    crate::stmt::emit_release_box_locals(out, 1, &ctx, &ctx.box_owners_all());
    emit_return_fallback(out, &m.return_type, checked, params, args);
    out.push_str("}\n");
}
