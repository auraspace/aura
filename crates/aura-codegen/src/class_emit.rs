//! Class/struct typedefs, constructors, and methods.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use aura_ast::*;
use aura_sema::{CheckedFile, Ty};

use crate::ctx::EmitCtx;
use crate::iface::emit_upcast;
use crate::names::*;
use crate::stmt::{emit_block, emit_return_fallback};

pub(crate) fn direct_superclass<'a>(
    checked: &'a CheckedFile,
    c: &ClassDecl,
) -> Option<&'a ClassDecl> {
    let parent_name = c
        .superclass
        .as_ref()
        .map(|parent| parent.name.name.clone())
        .or_else(|| {
            checked
                .classes
                .iter()
                .find(|sig| {
                    sig.name == c.name.name && sig.package == class_decl_package(c, checked)
                })
                .and_then(|sig| sig.superclass.as_ref())
                .and_then(Ty::class_name)
                .map(str::to_string)
        })?;
    checked.ast.classes.iter().find(|candidate| {
        candidate.name.name == parent_name
            && candidate.kind == NominalKind::Class
            && class_decl_package(candidate, checked) == class_decl_package(c, checked)
    })
}

/// Resolve a superclass together with its concrete type arguments for a class
/// monomorph. This keeps flattened C layouts consistent with sema's nominal
/// inheritance substitution.
pub(crate) fn direct_superclass_with_args<'a>(
    checked: &'a CheckedFile,
    c: &'a ClassDecl,
    args: &[Ty],
) -> Option<(&'a ClassDecl, Vec<Ty>)> {
    let package = class_decl_package(c, checked);
    let sig = checked
        .classes
        .iter()
        .find(|sig| sig.name == c.name.name && sig.package == package)?;
    let superclass = sig.superclass.as_ref()?;
    let subst = aura_sema::type_subst_map(&sig.type_params, args);
    let superclass = aura_sema::subst_ty(superclass, &subst);
    let (name, parent_args) = match superclass {
        Ty::Class(name) => (name, Vec::new()),
        Ty::ClassApp { name, args } => (name, args),
        _ => return None,
    };
    checked.ast.classes.iter().find_map(|candidate| {
        (candidate.kind == NominalKind::Class
            && aura_sema::nominal_key(
                &class_decl_package(candidate, checked),
                &candidate.name.name,
            ) == name)
            .then_some((candidate, parent_args.clone()))
    })
}

/// Whether a concrete class monomorph has a concrete superclass monomorph.
pub(crate) fn class_mono_extends(
    checked: &CheckedFile,
    child_mono: &str,
    parent_mono: &str,
) -> bool {
    for class in &checked.ast.classes {
        let package = class_decl_package(class, checked);
        let args = if class.type_params.is_empty() {
            Vec::new()
        } else if let Some((_, args)) = checked.mono_classes.iter().find(|(name, args)| {
            name == &class.name.name && type_mono(&package, name, args) == child_mono
        }) {
            args.clone()
        } else {
            continue;
        };
        if type_mono(&package, &class.name.name, &args) != child_mono {
            continue;
        }
        let mut current = direct_superclass_with_args(checked, class, &args);
        while let Some((parent, parent_args)) = current {
            if type_mono(
                &class_decl_package(parent, checked),
                &parent.name.name,
                &parent_args,
            ) == parent_mono
            {
                return true;
            }
            current = direct_superclass_with_args(checked, parent, &parent_args);
        }
    }
    false
}

pub(crate) fn method_owner<'a>(
    checked: &'a CheckedFile,
    c: &'a ClassDecl,
    method: &str,
) -> Option<&'a ClassDecl> {
    if c.methods.iter().any(|m| m.name.name == method) {
        return Some(c);
    }
    direct_superclass(checked, c).and_then(|parent| method_owner(checked, parent, method))
}

pub(crate) fn class_tag(checked: &CheckedFile, c: &ClassDecl) -> u32 {
    checked
        .ast
        .classes
        .iter()
        .position(|candidate| {
            candidate.name.name == c.name.name
                && class_decl_package(candidate, checked) == class_decl_package(c, checked)
        })
        .map(|index| index as u32 + 1)
        .unwrap_or(0)
}

pub(crate) fn virtual_overrides<'a>(
    checked: &'a CheckedFile,
    base: &'a ClassDecl,
    method: &str,
) -> Vec<&'a ClassDecl> {
    fn is_descendant(checked: &CheckedFile, candidate: &ClassDecl, base: &ClassDecl) -> bool {
        let mut current = direct_superclass(checked, candidate);
        while let Some(parent) = current {
            if parent.name.name == base.name.name {
                return true;
            }
            current = direct_superclass(checked, parent);
        }
        false
    }

    checked
        .ast
        .classes
        .iter()
        .filter(|candidate| {
            candidate.kind == NominalKind::Class
                && candidate.name.name != base.name.name
                && class_decl_package(candidate, checked) == class_decl_package(base, checked)
                && is_descendant(checked, candidate, base)
                && candidate.methods.iter().any(|m| {
                    m.name.name == method && m.modifiers.contains(&aura_ast::Modifier::Override)
                })
        })
        .collect()
}

pub(crate) fn has_field_in_hierarchy(checked: &CheckedFile, c: &ClassDecl, field: &str) -> bool {
    c.fields
        .iter()
        .any(|candidate| candidate.name.name == field)
        || direct_superclass(checked, c)
            .is_some_and(|parent| has_field_in_hierarchy(checked, parent, field))
}

fn emit_layout_fields(
    out: &mut String,
    checked: &CheckedFile,
    c: &ClassDecl,
    params: &[String],
    args: &[Ty],
) {
    if let Some((parent, parent_args)) = direct_superclass_with_args(checked, c, args) {
        let parent_params = parent
            .type_params
            .iter()
            .map(|param| param.name.name.clone())
            .collect::<Vec<_>>();
        emit_layout_fields(out, checked, parent, &parent_params, &parent_args);
    }
    for f in &c.fields {
        let _ = writeln!(
            out,
            "  {} {};",
            c_type_ref_subst(&f.ty, checked, params, args),
            mangle_ident(&f.name.name)
        );
    }
}

fn simple_ctor_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(id) => Some(mangle_ident(&id.name)),
        Expr::Int(value) => Some(format!("INT64_C({})", value.value)),
        Expr::Bool(value) => Some(if value.value { "true" } else { "false" }.into()),
        Expr::String(value) => Some(format!(
            "\"{}\"",
            value.value.replace('\\', "\\\\").replace('"', "\\\"")
        )),
        Expr::Group(inner, _) => simple_ctor_expr(inner),
        _ => None,
    }
}

/// Class fields own independent String buffers. Constructors clone every
/// non-null source so literals and caller-owned strings are both safe to drop.
fn emit_string_copy_assignment(out: &mut String, target: &str, source: &str) {
    let _ = writeln!(out, "  {{");
    let _ = writeln!(out, "    const char *__aura_string_src = {source};");
    let _ = writeln!(
        out,
        "    if (__aura_string_src == NULL) {{ {target} = NULL; }} else {{"
    );
    let _ = writeln!(
        out,
        "      size_t __aura_string_len = strlen(__aura_string_src);"
    );
    let _ = writeln!(
        out,
        "      {target} = (char *)malloc(__aura_string_len + 1);"
    );
    out.push_str("      if (");
    out.push_str(target);
    out.push_str(" == NULL) { aura_throw_string(\"class String field allocation failed\"); }\n");
    let _ = writeln!(
        out,
        "      memcpy((char *){target}, __aura_string_src, __aura_string_len + 1);"
    );
    out.push_str("    }\n  }\n");
}

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
    if is_heap_class_decl(c) {
        out.push_str("  uint32_t __aura_class_tag;\n");
    }
    emit_layout_fields(out, checked, c, &params, args);
    if c.fields.is_empty() && direct_superclass(checked, c).is_none() {
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
    let mut emitted_upcasts = HashSet::new();
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
            for (target, target_args) in crate::iface::interface_and_parents(checked, iface, &iargs)
            {
                let imono = iface_mono_args(target, checked, &target_args);
                if !emitted_upcasts.insert(imono.clone()) {
                    continue;
                }
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
}

fn ownership_fields<'a>(
    c: &'a ClassDecl,
    checked: &'a CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some((parent, parent_args)) = direct_superclass_with_args(checked, c, args) {
        let parent_params = parent
            .type_params
            .iter()
            .map(|param| param.name.name.clone())
            .collect::<Vec<_>>();
        out.extend(ownership_fields(
            parent,
            checked,
            &parent_params,
            &parent_args,
        ));
    }
    out.extend(c.fields.iter().map(|field| {
        (
            field.name.name.clone(),
            type_ref_local_key(&field.ty, params, args),
        )
    }));
    out
}

/// C7b: field names that are builtin `Array` (any element type).
fn array_field_names(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<String> {
    ownership_fields(c, checked, params, args)
        .into_iter()
        .filter(|(_, key)| crate::array_emit::is_array_type_key(key))
        .map(|(name, _)| name)
        .collect()
}

/// C7b: field names that are Array-of-heap-class (need mark_extras).
fn array_of_class_field_names(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<String> {
    ownership_fields(c, checked, params, args)
        .into_iter()
        .filter(|(_, key)| {
            let mono = crate::expr::full_type_mono(key, checked);
            crate::array_emit::is_array_of_heap_class(&mono, checked)
        })
        .map(|(name, _)| name)
        .collect()
}

fn array_of_interface_field_specs(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<(String, String)> {
    ownership_fields(c, checked, params, args)
        .into_iter()
        .filter_map(|(name, key)| {
            let elem = crate::array_emit::array_elem_key(&key)?;
            crate::names::is_iface_type_key(elem, checked).then(|| (name, elem.to_string()))
        })
        .collect()
}

fn string_field_names(
    c: &ClassDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
) -> Vec<String> {
    ownership_fields(c, checked, params, args)
        .into_iter()
        .filter(|(_, key)| key == "String")
        .map(|(name, _)| name)
        .collect()
}

fn c_dtor_name(mono: &str) -> String {
    format!("aura_dtor_{mono}")
}

fn c_exception_dtor_name(mono: &str) -> String {
    format!("aura_ex_dtor_{mono}")
}

fn c_markex_name(mono: &str) -> String {
    format!("aura_markex_{mono}")
}

/// Emit ownership hooks for heap classes.  Exception payload copies use the
/// same generated destructor as the GC object contract, so owned String and
/// Array fields are released when the exception frame disposes the copy.
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
    let arr_fields = array_field_names(c, checked, params, args);
    let string_fields = string_field_names(c, checked, params, args);
    let arr_cls_fields = array_of_class_field_names(c, checked, params, args);
    let arr_iface_fields = array_of_interface_field_specs(c, checked, params, args);
    {
        let _ = writeln!(out, "static void {}(void *p) {{", c_dtor_name(mono));
        let _ = writeln!(out, "  {cty} *self = ({cty} *)p;");
        out.push_str("  if (self == NULL) { return; }\n");
        for name in &string_fields {
            let f = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (self->{f} != NULL) {{ free((void *)self->{f}); self->{f} = NULL; }}"
            );
        }
        for name in &arr_fields {
            let f = mangle_ident(name);
            let _ = writeln!(
                out,
                "  if (self->{f}.data != NULL) {{ free(self->{f}.data); self->{f}.data = NULL; self->{f}.len = 0; self->{f}.cap = 0; }}"
            );
        }
        out.push_str("}\n\n");
        let _ = writeln!(
            out,
            "static void {}(void *p) {{ {}(p); free(p); }}\n",
            c_exception_dtor_name(mono),
            c_dtor_name(mono)
        );
    }
    if !arr_cls_fields.is_empty() || !arr_iface_fields.is_empty() {
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
        for (name, iface_key) in &arr_iface_fields {
            let f = mangle_ident(name);
            let (iface, iface_args) = crate::names::resolve_iface_decl_and_args(iface_key, checked);
            let Some(iface) = iface else {
                continue;
            };
            let imono = crate::names::iface_mono_args(iface, checked, &iface_args);
            let _ = writeln!(out, "  {{");
            let _ = writeln!(
                out,
                "    {} *__data = ({0} *)self->{f}.data;",
                crate::names::c_iface_type(&imono),
            );
            out.push_str("    int64_t __len = self->");
            out.push_str(&f);
            out.push_str(".len;\n");
            out.push_str("    if (__data != NULL && __len > 0) {\n");
            out.push_str("      for (int64_t __i = 0; __i < __len; __i++) {\n");
            out.push_str("        switch (__data[__i].tag) {\n");
            for imp in crate::iface::mono_implementors_for_iface(checked, iface, &iface_args) {
                let mono = type_mono(
                    &class_decl_package(imp.class, checked),
                    &imp.class.name.name,
                    &imp.class_args,
                );
                let _ = writeln!(
                    out,
                    "          case AURA_TAG_{mono}: aura_gc_mark_ptr(__data[__i].data.as_{mono}); break;"
                );
            }
            out.push_str("          default: break;\n");
            out.push_str("        }\n");
            out.push_str("      }\n");
            out.push_str("    }\n");
            out.push_str("  }\n");
        }
        out.push_str("}\n\n");
    }
}

pub(crate) fn emit_class_defs(
    out: &mut String,
    checked: &CheckedFile,
    c: &ClassDecl,
    args: &[Ty],
    detector: bool,
) {
    let params: Vec<String> = c.type_params.iter().map(|p| p.name.name.clone()).collect();
    let pkg = class_decl_package(c, checked);
    let mono = type_mono(&pkg, &c.name.name, args);
    emit_class_gc_hooks(out, c, checked, &params, args, &mono);
    emit_ctor_mono(out, c, checked, &params, args, &mono);
    out.push('\n');
    for m in &c.methods {
        emit_method_mono(out, c, m, checked, &params, args, &mono, detector);
        out.push('\n');
    }
    // C9a: emit upcasts for this class monomorph's implements.
    let mut emitted_upcasts = HashSet::new();
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
            for (target, target_args) in crate::iface::interface_and_parents(checked, iface, &iargs)
            {
                let imono = iface_mono_args(target, checked, &target_args);
                if !emitted_upcasts.insert(imono) {
                    continue;
                }
                emit_upcast(out, checked, c, target, &target_args, args);
                out.push('\n');
            }
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
        let arr_cls = array_of_class_field_names(c, checked, params, args);
        let dtor = c_dtor_name(mono);
        let markex = if arr_cls.is_empty()
            && array_of_interface_field_specs(c, checked, params, args).is_empty()
        {
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
        out.push_str("  memset(self, 0, sizeof(*self));\n");
        let _ = writeln!(
            out,
            "  self->__aura_class_tag = UINT32_C({});",
            class_tag(checked, c)
        );
        for f in &c.fields {
            let n = mangle_ident(&f.name.name);
            if type_ref_local_key(&f.ty, params, args) == "String" {
                emit_string_copy_assignment(out, &format!("self->{n}"), &n);
            } else {
                let _ = writeln!(out, "  self->{n} = {n};");
            }
        }
        if let Some((parent, parent_args)) = direct_superclass_with_args(checked, c, args) {
            let parent_params = parent
                .type_params
                .iter()
                .map(|param| param.name.name.clone())
                .collect::<Vec<_>>();
            for (field, arg) in parent.fields.iter().zip(c.superclass_args.iter()) {
                if let Some(value) = simple_ctor_expr(arg) {
                    let name = mangle_ident(&field.name.name);
                    if type_ref_local_key(&field.ty, &parent_params, &parent_args) == "String" {
                        emit_string_copy_assignment(out, &format!("self->{name}"), &value);
                    } else {
                        let _ = writeln!(out, "  self->{name} = {value};");
                    }
                }
            }
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_method_mono(
    out: &mut String,
    c: &ClassDecl,
    m: &FunDecl,
    checked: &CheckedFile,
    params: &[String],
    args: &[Ty],
    mono: &str,
    detector: bool,
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
        detector,
        method_class: Some(mono),
        type_params: params.to_vec(),
        type_args: args.to_vec(),
        locals: vec![HashMap::new()],
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
        lambda_ids: crate::emit::build_lambda_ids(checked),
        spawn_params: std::collections::HashSet::new(),
        mutable_spawn_captures: std::collections::HashSet::new(),
        async_frame: None,
        task_poller: false,
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
    crate::stmt::emit_release_task_handle_owners(out, 1, &ctx, &ctx.task_handle_owners_all());
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
