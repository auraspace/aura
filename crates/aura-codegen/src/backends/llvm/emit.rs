use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use aura_ast::Span;
use aura_ir::mir::{BinaryOp, Intrinsic, MirBody, Place, Rvalue, Statement, Terminator, UnaryOp};
use aura_ir::{FunctionIr, LoweredProgram};
use aura_sema::Ty;

use crate::error::CodegenError;

#[path = "runtime.rs"]
mod runtime;
#[path = "statements.rs"]
mod statements;
#[path = "types.rs"]
mod types;
#[path = "values.rs"]
mod values;
use runtime::*;
use statements::emit_statement;
use types::*;
pub(super) use values::*;

type Signatures = HashMap<(String, String), (Ty, Vec<Ty>)>;

struct EmitContext {
    signatures: Signatures,
    enum_variants: HashMap<String, EnumVariantInfo>,
    classes: HashMap<String, Vec<(String, Ty)>>,
    class_superclasses: HashMap<String, String>,
    class_superclass_args: HashMap<String, Vec<aura_ast::Expr>>,
    class_type_ids: HashMap<String, i64>,
    class_type_params: HashMap<String, Vec<String>>,
    foreign_names: HashSet<String>,
    string_literals: Vec<String>,
}

#[derive(Clone)]
struct EnumVariantInfo {
    tag: i64,
    type_params: Vec<String>,
    fields: Vec<(String, Ty)>,
}

pub fn emit_module(program: &LoweredProgram) -> Result<String, CodegenError> {
    validate_program(program)?;
    let mut module = String::from("; ModuleID = 'aura'\nsource_filename = \"aura\"\n\n");
    let mut context = EmitContext {
        signatures: signatures(program),
        enum_variants: enum_variants(program),
        classes: classes(program),
        class_superclasses: program
            .source()
            .ast
            .classes
            .iter()
            .filter_map(|class| {
                class
                    .superclass
                    .as_ref()
                    .map(|parent| (class.name.name.clone(), parent.name.name.clone()))
            })
            .collect(),
        class_superclass_args: program
            .source()
            .ast
            .classes
            .iter()
            .filter_map(|class| {
                (!class.superclass_args.is_empty())
                    .then(|| (class.name.name.clone(), class.superclass_args.clone()))
            })
            .collect(),
        class_type_ids: class_type_ids(program),
        class_type_params: program
            .source()
            .ast
            .classes
            .iter()
            .map(|class| {
                (
                    class.name.name.clone(),
                    class
                        .type_params
                        .iter()
                        .map(|param| param.name.name.clone())
                        .collect(),
                )
            })
            .collect(),
        foreign_names: program
            .source()
            .ast
            .foreign_functions
            .iter()
            .map(|foreign| foreign.name.name.clone())
            .collect(),
        string_literals: Vec::new(),
    };
    let mut extra_functions = async_functions(program);
    extra_functions.extend(generic_method_functions(program));
    let mut seen_spawns = extra_functions
        .iter()
        .map(|function| function.name.clone())
        .collect::<HashSet<_>>();
    for function in program
        .checked()
        .functions
        .iter()
        .chain(program.checked().generic_functions.iter())
    {
        if let Some(body) = &function.body {
            collect_spawn_functions(
                body,
                &function.package,
                &mut extra_functions,
                &mut seen_spawns,
            );
        }
    }
    for body in program
        .checked()
        .async_mir
        .iter()
        .chain(program.checked().open_generic_async_mir.iter())
        .chain(program.checked().generic_async_mir.iter())
        .chain(program.checked().generic_async_method_mir.iter())
    {
        collect_spawn_functions(
            body,
            &program.checked().package,
            &mut extra_functions,
            &mut seen_spawns,
        );
    }
    for function in &extra_functions {
        context.signatures.insert(
            (function.package.clone(), function.name.clone()),
            (
                function.ret.ty.clone(),
                function
                    .params
                    .iter()
                    .map(|param| param.ty.clone())
                    .collect(),
            ),
        );
    }
    module.push_str(STRING_RUNTIME);
    module.push_str(ENUM_RUNTIME);
    module.push_str(CLASS_RUNTIME);
    module.push_str(ARRAY_RUNTIME);
    module.push_str(CHANNEL_RUNTIME);
    module.push_str(MISC_RUNTIME);
    emit_class_destructors(&mut module, &context.classes);
    let mut exception_names = context
        .classes
        .keys()
        .chain(context.enum_variants.keys())
        .cloned()
        .collect::<Vec<_>>();
    exception_names.extend(["String".into(), "Int".into(), "Bool".into()]);
    exception_names.sort();
    exception_names.dedup();
    for name in exception_names {
        let bytes = name.as_bytes();
        writeln!(
            module,
            "{} = private unnamed_addr constant [{} x i8] c\"{}\", align 1",
            exception_type_global(&name),
            bytes.len() + 1,
            escape_llvm_bytes(bytes)
        )
        .unwrap();
    }
    emit_foreign_declarations(&mut module, program, &context.foreign_names)?;
    for function in program
        .checked()
        .functions
        .iter()
        .chain(program.checked().generic_functions.iter())
    {
        let Some(body) = &function.body else {
            continue;
        };
        if body
            .locals
            .iter()
            .any(|local| matches!(local.ty, Ty::TypeParam(_)))
        {
            continue;
        }
        emit_function(&mut module, function, body, &mut context)?;
    }
    for function in &extra_functions {
        if let Some(body) = &function.body {
            emit_function(&mut module, function, body, &mut context)?;
        }
    }
    if let Some(function) = program
        .checked()
        .functions
        .iter()
        .find(|function| function.name == "main" && function.params.is_empty())
    {
        let symbol = symbol_name(&function.package, &function.name);
        match &function.ret.ty {
            Ty::Unit => module.push_str(&format!(
                "define i32 @main() {{\nentry:\n  call void @{symbol}()\n  ret i32 0\n}}\n"
            )),
            Ty::Int => module.push_str(&format!(
                "define i32 @main() {{\nentry:\n  %result = call i64 @{symbol}()\n  %status = trunc i64 %result to i32\n  ret i32 %status\n}}\n"
            )),
            _ => return Err(unsupported("main return type")),
        }
    }
    for (index, literal) in context.string_literals.iter().enumerate() {
        let bytes = literal.as_bytes();
        writeln!(
            module,
            "@.aura_str{index} = private unnamed_addr constant [{} x i8] c\"{}\", align 1",
            bytes.len() + 1,
            escape_llvm_bytes(bytes)
        )
        .unwrap();
    }
    Ok(module)
}

fn async_functions(program: &LoweredProgram) -> Vec<FunctionIr> {
    program
        .checked()
        .async_mir
        .iter()
        .chain(program.checked().open_generic_async_mir.iter())
        .chain(program.checked().generic_async_mir.iter())
        .chain(program.checked().generic_async_method_mir.iter())
        .map(|body| {
            let ast = &program.source().ast;
            let parameter_count = ast
                .async_functions
                .iter()
                .find(|function| {
                    body.name == function.name.name
                        || body.name.starts_with(&format!("{}_", function.name.name))
                })
                .map(|function| function.params.len())
                .or_else(|| {
                    ast.classes.iter().find_map(|class| {
                        class.methods.iter().find_map(|method| {
                            let matches = body.name == method.name.name
                                || body.name.starts_with(&format!("{}_", method.name.name))
                                || body.name.contains(&format!("_{}_", method.name.name));
                            matches.then_some(method.params.len() + 1)
                        })
                    })
                })
                .unwrap_or(0);
            synthetic_function(body, program.checked().package.clone(), parameter_count)
        })
        .collect()
}

fn generic_method_functions(program: &LoweredProgram) -> Vec<FunctionIr> {
    program
        .checked()
        .generic_method_mir
        .iter()
        .map(|body| {
            let parameter_count = body
                .locals
                .iter()
                .take_while(|local| !local.name.starts_with("__"))
                .count();
            synthetic_function(body, program.checked().package.clone(), parameter_count)
        })
        .collect()
}

fn collect_spawn_functions(
    body: &MirBody,
    package: &str,
    output: &mut Vec<FunctionIr>,
    seen: &mut HashSet<String>,
) {
    for block in &body.blocks {
        for statement in &block.statements {
            let value = match statement {
                Statement::Assign { value, .. } | Statement::Evaluate(value) => value,
                _ => continue,
            };
            let Rvalue::AsyncOp(aura_ir::mir::AsyncOp::Spawn { body, captures }) = value else {
                continue;
            };
            if seen.insert(body.name.clone()) {
                output.push(synthetic_function(body, package.to_owned(), captures.len()));
            }
            collect_spawn_functions(body, package, output, seen);
        }
    }
}

fn synthetic_function(body: &MirBody, package: String, parameter_count: usize) -> FunctionIr {
    FunctionIr {
        name: body.name.clone(),
        package,
        params: body
            .locals
            .iter()
            .take(parameter_count)
            .map(|local| aura_ir::ValueFact {
                ty: local.ty.clone(),
                ownership: aura_ir::ownership::mode_for_ty(&local.ty),
                span: Span::new(0, 0),
            })
            .collect(),
        ret: aura_ir::ValueFact {
            ty: body.return_ty.clone(),
            ownership: aura_ir::ownership::mode_for_ty(&body.return_ty),
            span: Span::new(0, 0),
        },
        type_params: Vec::new(),
        bounds: HashMap::new(),
        effect: body.effect,
        body: Some(body.clone()),
        span: Span::new(0, 0),
    }
}

fn emit_foreign_declarations(
    out: &mut String,
    program: &LoweredProgram,
    foreign_names: &HashSet<String>,
) -> Result<(), CodegenError> {
    for function in program
        .checked()
        .functions
        .iter()
        .filter(|function| foreign_names.contains(&function.name))
    {
        let params = function
            .params
            .iter()
            .map(|param| llvm_type(&param.ty))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        writeln!(
            out,
            "declare {} @{}({params})",
            llvm_type(&function.ret.ty)?,
            function.name
        )
        .unwrap();
    }
    Ok(())
}

fn validate_program(program: &LoweredProgram) -> Result<(), CodegenError> {
    let checked = program.checked();
    for function in checked
        .functions
        .iter()
        .chain(checked.generic_functions.iter())
    {
        if let Some(body) = &function.body {
            body.validate()
                .map_err(|error| unsupported(&format!("invalid MIR: {error:?}")))?;
        }
    }
    for body in checked
        .async_mir
        .iter()
        .chain(checked.open_generic_async_mir.iter())
        .chain(checked.generic_async_mir.iter())
        .chain(checked.generic_async_method_mir.iter())
    {
        body.validate()
            .map_err(|error| unsupported(&format!("invalid async MIR: {error:?}")))?;
    }
    Ok(())
}

fn emit_function(
    out: &mut String,
    function: &FunctionIr,
    body: &MirBody,
    context: &mut EmitContext,
) -> Result<(), CodegenError> {
    let ret = llvm_type(&function.ret.ty)?;
    let params = function
        .params
        .iter()
        .enumerate()
        .map(|(index, value)| Ok(format!("{} %arg{index}", llvm_type(&value.ty)?)))
        .collect::<Result<Vec<_>, CodegenError>>()?
        .join(", ");
    writeln!(
        out,
        "define {ret} @{}({params}) {{",
        symbol_name(&function.package, &function.name)
    )
    .unwrap();
    out.push_str("entry:\n");
    for (index, local) in body.locals.iter().enumerate() {
        if local.ty != Ty::Unit {
            let ty = llvm_type(&local.ty)?;
            writeln!(out, "  %slot{index} = alloca {ty}").unwrap();
            writeln!(
                out,
                "  store {ty} {}, ptr %slot{index}",
                llvm_zero(&local.ty)?
            )
            .unwrap();
        }
    }
    for (index, local) in body.locals.iter().take(function.params.len()).enumerate() {
        if local.ty != Ty::Unit {
            writeln!(
                out,
                "  store {} %arg{index}, ptr %slot{index}",
                llvm_type(&local.ty)?
            )
            .unwrap();
        }
    }
    writeln!(out, "  br label %bb{}", body.entry).unwrap();
    for (index, block) in body.blocks.iter().enumerate() {
        writeln!(out, "bb{index}:").unwrap();
        for statement in &block.statements {
            emit_statement(out, statement, body, context, &function.package)?;
        }
        emit_terminator(out, &block.terminator, body, ret, context)?;
    }
    out.push_str("}\n\n");
    Ok(())
}

fn emit_terminator(
    out: &mut String,
    term: &Terminator,
    body: &MirBody,
    ret: &str,
    context: &EmitContext,
) -> Result<(), CodegenError> {
    match term {
        Terminator::Goto { target } => writeln!(out, "  br label %bb{target}").unwrap(),
        Terminator::SwitchInt {
            condition,
            then_target,
            else_target,
        } => {
            let value = load_place(out, *condition, body)?;
            writeln!(
                out,
                "  br i1 {value}, label %bb{then_target}, label %bb{else_target}"
            )
            .unwrap();
        }
        Terminator::Return { value } => {
            if ret == "void" {
                out.push_str("  ret void\n");
            } else {
                let Some(value) = value else {
                    out.push_str("  unreachable\n");
                    return Ok(());
                };
                let loaded = load_place(out, *value, body)?;
                writeln!(out, "  ret {ret} {loaded}").unwrap();
            }
        }
        Terminator::Unreachable => out.push_str("  unreachable\n"),
        Terminator::SwitchTag {
            discriminant,
            targets,
            otherwise,
        } => {
            let value = load_place(out, *discriminant, body)?;
            writeln!(out, "  switch i64 {value}, label %bb{otherwise} [").unwrap();
            for (tag, target) in targets {
                writeln!(out, "    i64 {tag}, label %bb{target}").unwrap();
            }
            out.push_str("  ]\n");
        }
        Terminator::Await {
            task,
            result,
            resume,
            unwind,
        } => {
            let task_ty = &body.locals[task.local].ty;
            let Some(payload_ty) = task_payload_type(task_ty) else {
                return Err(unsupported("awaiting a non-task value"));
            };
            let value = if *payload_ty == Ty::Unit {
                None
            } else {
                Some(load_place(out, *task, body)?)
            };
            if let Some(value) = value {
                if body.locals[result.local].ty != *payload_ty {
                    return Err(unsupported("await result type"));
                }
                writeln!(
                    out,
                    "  store {} {value}, ptr %slot{}",
                    llvm_type(payload_ty)?,
                    result.local
                )
                .unwrap();
            }
            writeln!(out, "  br label %bb{resume}").unwrap();
            if unwind.is_some() {
                return Err(unsupported("async unwind edges"));
            }
        }
        Terminator::Throw { value, .. } => {
            let ty = &body.locals[value.local].ty;
            let loaded = load_place(out, *value, body)?;
            match ty {
                Ty::String => {
                    let data = next_temp(out);
                    writeln!(out, "  {data} = call ptr @aura_llvm_str_data(ptr {loaded})").unwrap();
                    writeln!(out, "  call void @aura_throw_string(ptr {data})").unwrap();
                }
                Ty::Int => writeln!(out, "  call void @aura_throw_int(i64 {loaded})").unwrap(),
                Ty::Bool => writeln!(out, "  call void @aura_throw_bool(i1 {loaded})").unwrap(),
                Ty::Class(_) | Ty::ClassApp { .. } | Ty::Enum(_) | Ty::EnumApp { .. } => {
                    let Some(raw_type_name) = exception_type_name(ty) else {
                        return Err(unsupported("exception payload type"));
                    };
                    let type_name = context
                        .classes
                        .keys()
                        .find(|name| {
                            raw_type_name == name.as_str()
                                || raw_type_name.starts_with(&format!("{name}_"))
                        })
                        .map(String::as_str)
                        .unwrap_or_else(|| {
                            raw_type_name.split('_').next().unwrap_or(raw_type_name)
                        });
                    let global = exception_type_global(type_name);
                    let length = type_name.as_bytes().len() + 1;
                    let name = next_temp(out);
                    writeln!(
                        out,
                        "  {name} = getelementptr [{length} x i8], ptr {global}, i64 0, i64 0"
                    )
                    .unwrap();
                    let destructor = match ty {
                        Ty::Class(_) | Ty::ClassApp { .. } => context
                            .classes
                            .get(type_name)
                            .filter(|fields| class_has_pointer_fields(fields))
                            .map(|_| class_release_symbol(type_name))
                            .unwrap_or_else(|| "aura_llvm_class_release".into()),
                        Ty::Enum(_) | Ty::EnumApp { .. } => "aura_llvm_enum_release".into(),
                        _ => unreachable!("exception type checked above"),
                    };
                    writeln!(
                        out,
                        "  call void @aura_throw_obj_with_destructor(ptr {name}, ptr {loaded}, ptr @{destructor})"
                    )
                    .unwrap();
                }
                _ => return Err(unsupported("non-primitive LLVM throw payload")),
            }
            out.push_str("  unreachable\n");
        }
        Terminator::Cancel => {
            return Err(unsupported("cancellation control flow"));
        }
    }
    Ok(())
}

fn classes(program: &LoweredProgram) -> HashMap<String, Vec<(String, Ty)>> {
    program
        .source()
        .classes
        .iter()
        .map(|class| {
            (
                class.name.clone(),
                class
                    .fields
                    .clone()
                    .into_iter()
                    .map(|field| (field.name, field.ty))
                    .collect(),
            )
        })
        .collect()
}

fn class_type_ids(program: &LoweredProgram) -> HashMap<String, i64> {
    let mut names = program
        .source()
        .classes
        .iter()
        .map(|class| class.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name, index as i64 + 1))
        .collect()
}

fn class_fields(context: &EmitContext, name: &str, args: &[Ty]) -> Option<Vec<(String, Ty)>> {
    let mut fields = context
        .class_superclasses
        .get(name)
        .and_then(|parent| class_fields(context, parent, args))
        .unwrap_or_default();
    fields.extend(class_own_fields(context, name, args)?);
    Some(fields)
}

fn class_own_fields(context: &EmitContext, name: &str, args: &[Ty]) -> Option<Vec<(String, Ty)>> {
    let fields = context.classes.get(name)?;
    let params = context.class_type_params.get(name)?;
    let substitutions = params
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .collect::<HashMap<_, _>>();
    Some(
        fields
            .iter()
            .map(|(field, ty)| (field.clone(), substitute_ty(ty, &substitutions)))
            .collect(),
    )
}

fn dynamic_method_targets(
    context: &EmitContext,
    receiver_ty: &Ty,
    target: &aura_ir::mir::CallTarget,
) -> Vec<(i64, String)> {
    let (Ty::Class(base) | Ty::ClassApp { name: base, .. }) = receiver_ty else {
        return Vec::new();
    };
    let base = base.split('@').next().unwrap_or(base);
    let mut descendants = context
        .class_superclasses
        .iter()
        .filter_map(|(class, parent)| (parent == base).then_some(class.clone()))
        .collect::<Vec<_>>();
    let mut index = 0;
    while index < descendants.len() {
        let parent = descendants[index].clone();
        descendants.extend(
            context
                .class_superclasses
                .iter()
                .filter_map(|(class, candidate)| (candidate == &parent).then_some(class.clone())),
        );
        index += 1;
    }
    descendants.sort_by_key(|class| class_depth(context, class));
    descendants.reverse();
    descendants
        .into_iter()
        .filter_map(|class| {
            let method = format!("{class}::{}", target.name);
            let (package, _) = context
                .signatures
                .keys()
                .find(|(_, name)| name == &method)?;
            let type_id = *context.class_type_ids.get(&class)?;
            Some((type_id, symbol_name(package, &method)))
        })
        .collect()
}

fn class_depth(context: &EmitContext, class: &str) -> usize {
    context
        .class_superclasses
        .get(class)
        .map(|parent| class_depth(context, parent) + 1)
        .unwrap_or(0)
}

fn class_has_pointer_fields(fields: &Vec<(String, Ty)>) -> bool {
    fields.iter().any(|(_, ty)| is_pointer_value_type(ty))
}

fn class_release_symbol(name: &str) -> String {
    format!(
        "aura_llvm_class_release_{}",
        name.chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>()
    )
}

fn emit_class_destructors(out: &mut String, classes: &HashMap<String, Vec<(String, Ty)>>) {
    for (name, fields) in classes {
        if !class_has_pointer_fields(fields) {
            continue;
        }
        let symbol = class_release_symbol(name);
        writeln!(out, "define void @{symbol}(ptr %value) {{").unwrap();
        out.push_str("entry:\n");
        out.push_str("  %is_null = icmp eq ptr %value, null\n");
        out.push_str("  br i1 %is_null, label %done, label %release\n");
        out.push_str("release:\n");
        out.push_str("  %refs = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 0\n");
        out.push_str("  %current = load i64, ptr %refs\n");
        out.push_str("  %count = and i64 %current, 4294967295\n");
        out.push_str("  %next_count = sub i64 %count, 1\n");
        out.push_str("  %tag = and i64 %current, -4294967296\n");
        out.push_str("  %next = or i64 %tag, %next_count\n");
        out.push_str("  store i64 %next, ptr %refs\n");
        out.push_str("  %last = icmp eq i64 %next_count, 0\n");
        out.push_str("  br i1 %last, label %destroy, label %done\n");
        out.push_str("destroy:\n");
        for (index, (_, ty)) in fields.iter().enumerate() {
            if !is_pointer_value_type(ty) {
                continue;
            }
            writeln!(
                out,
                "  %field_address{index} = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 1, i64 {index}"
            )
            .unwrap();
            writeln!(
                out,
                "  %field_raw{index} = load i64, ptr %field_address{index}"
            )
            .unwrap();
            let helper = match ty {
                Ty::String => "aura_llvm_str_release",
                Ty::Class(_) | Ty::ClassApp { .. } => "aura_llvm_class_release",
                Ty::Enum(_) | Ty::EnumApp { .. } => "aura_llvm_enum_release",
                _ => unreachable!("pointer field type checked above"),
            };
            writeln!(
                out,
                "  %field_ptr{index} = inttoptr i64 %field_raw{index} to ptr"
            )
            .unwrap();
            writeln!(out, "  call void @{helper}(ptr %field_ptr{index})").unwrap();
        }
        out.push_str("  call void @free(ptr %value)\n");
        out.push_str("  br label %done\n");
        out.push_str("done:\n  ret void\n}\n\n");
    }
}

fn is_print_call(target: &aura_ir::mir::CallTarget) -> bool {
    matches!(
        target.name.as_str(),
        "print" | "println" | "eprint" | "eprintln"
    ) && (target.package.is_empty() || target.package.starts_with("std."))
}

pub(super) fn format_float_constant(value: f64) -> String {
    if value == 0.0 {
        "0.0".into()
    } else {
        format!("{value:.17}")
    }
}

fn escape_llvm_bytes(bytes: &[u8]) -> String {
    let mut escaped = String::new();
    for &byte in bytes {
        match byte {
            b'\\' => escaped.push_str("\\5C"),
            b'"' => escaped.push_str("\\22"),
            0x20..=0x7e => escaped.push(byte as char),
            _ => write!(escaped, "\\{byte:02X}").unwrap(),
        }
    }
    escaped.push_str("\\00");
    escaped
}

fn symbol_name(package: &str, name: &str) -> String {
    let sanitize = |value: &str| {
        value
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>()
    };
    let package = sanitize(package);
    let name = sanitize(name);
    format!("aura_{}_{}", package, name)
}

fn exception_type_global(name: &str) -> String {
    format!(
        "@.aura_type_{}",
        name.chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>()
    )
}

fn exception_type_name(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::String => Some("String"),
        Ty::Int => Some("Int"),
        Ty::Bool => Some("Bool"),
        Ty::Class(name) | Ty::Enum(name) => Some(
            name.split('@')
                .next()
                .unwrap_or(name)
                .split('_')
                .next()
                .unwrap_or(name),
        ),
        Ty::ClassApp { name, .. } | Ty::EnumApp { name, .. } => Some(
            name.split('@')
                .next()
                .unwrap_or(name)
                .split('_')
                .next()
                .unwrap_or(name),
        ),
        _ => None,
    }
}

fn unsupported(feature: &str) -> CodegenError {
    CodegenError::Configuration(format!(
        "LLVM backend does not support {feature} in the current MIR contract"
    ))
}
