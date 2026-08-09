use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use aura_ast::{Block, Expr, File, LambdaBody, LambdaExpr, ReturnStmt, Span, Stmt};
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
pub(super) use types::llvm_type;
use types::*;
use values::*;

type Signatures = HashMap<(String, String), (Ty, Vec<Ty>)>;

struct EmitContext {
    signatures: Signatures,
    enum_variants: HashMap<String, EnumVariantInfo>,
    classes: HashMap<String, Vec<(String, Ty)>>,
    class_superclasses: HashMap<String, String>,
    class_superclass_args: HashMap<String, Vec<aura_ast::Expr>>,
    class_interfaces: HashMap<String, Vec<String>>,
    class_type_ids: HashMap<String, i64>,
    class_type_params: HashMap<String, Vec<String>>,
    foreign_names: HashSet<String>,
    string_literals: Vec<String>,
    enum_destructors: HashMap<String, Vec<(usize, Ty)>>,
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
        class_interfaces: program
            .source()
            .ast
            .classes
            .iter()
            .map(|class| {
                (
                    class.name.name.clone(),
                    class
                        .implements
                        .iter()
                        .map(|interface| interface.name.name.clone())
                        .collect(),
                )
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
        enum_destructors: HashMap::new(),
    };
    let mut extra_functions = async_functions(program);
    extra_functions.extend(lambda_functions(program)?);
    extra_functions.extend(generic_method_functions(program));
    let mut seen_spawns = extra_functions
        .iter()
        .map(|function| function.name.clone())
        .collect::<HashSet<_>>();
    let lambda_bodies = extra_functions
        .iter()
        .filter_map(|function| function.body.clone())
        .collect::<Vec<_>>();
    for body in &lambda_bodies {
        collect_spawn_functions(
            body,
            &program.checked().package,
            &mut extra_functions,
            &mut seen_spawns,
        );
    }
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
        context
            .signatures
            .entry((function.package.clone(), function.name.clone()))
            .or_insert_with(|| {
                (
                    function.ret.ty.clone(),
                    function
                        .params
                        .iter()
                        .map(|param| param.ty.clone())
                        .collect(),
                )
            });
        if let Some(public_name) = function.name.strip_prefix("__aura_async_body_") {
            context
                .signatures
                .entry((function.package.clone(), public_name.to_owned()))
                .or_insert_with(|| {
                    (
                        Ty::Task(Box::new(function.ret.ty.clone())),
                        function
                            .params
                            .iter()
                            .map(|param| param.ty.clone())
                            .collect(),
                    )
                });
        }
    }
    module.push_str(STRING_RUNTIME);
    module.push_str(ENUM_RUNTIME);
    module.push_str(CLASS_RUNTIME);
    module.push_str(ARRAY_RUNTIME);
    module.push_str(CHANNEL_RUNTIME);
    module.push_str(MISC_RUNTIME);
    emit_async_wrappers(&mut module, program, &extra_functions)?;
    emit_spawn_pollers(&mut module, &extra_functions, &program.checked().package)?;
    for function in &extra_functions {
        if function.closure_captures.is_empty() {
            continue;
        }
        let capture_fields = function
            .closure_captures
            .iter()
            .map(|capture| {
                if capture.by_ref {
                    Ok("ptr")
                } else {
                    llvm_type(&capture.ty)
                }
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        let fields = if capture_fields.is_empty() {
            "ptr, i32".to_string()
        } else {
            format!("ptr, i32, {capture_fields}")
        };
        writeln!(
            module,
            "%{} = type {{ {fields} }}",
            closure_env_name(&function.name)
        )
        .unwrap();
    }
    for function in &extra_functions {
        if !function.closure_captures.iter().any(|capture| {
            capture.by_ref
                || capture.ty == Ty::String
                || matches!(capture.ty, Ty::Fun { .. })
                || is_array_type(&capture.ty)
                || matches!(
                    capture.ty,
                    Ty::Class(_) | Ty::ClassApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. }
                )
        }) {
            continue;
        }
        writeln!(
            module,
            "define void @{}(ptr %env) {{",
            closure_drop_name(&function.name)
        )
        .unwrap();
        module.push_str("entry:\n");
        for (index, capture) in function.closure_captures.iter().enumerate() {
            if capture.by_ref {
                let address = format!("%drop_box_addr_{index}");
                let value = format!("%drop_box_value_{index}");
                let helper = box_release_helper(&capture.ty)?;
                writeln!(
                    module,
                    "  {address} = getelementptr %{}, ptr %env, i32 0, i32 {}",
                    closure_env_name(&function.name),
                    index + 2
                )
                .unwrap();
                writeln!(module, "  {value} = load ptr, ptr {address}").unwrap();
                writeln!(module, "  call void @{helper}(ptr {value})").unwrap();
                continue;
            }
            let release = if capture.ty == Ty::String {
                "aura_llvm_str_release"
            } else if is_array_type(&capture.ty) {
                "aura_llvm_array_release"
            } else if matches!(
                capture.ty,
                Ty::Class(_) | Ty::ClassApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. }
            ) {
                "aura_llvm_class_release"
            } else {
                continue;
            };
            let address = format!("%drop_addr_{index}");
            let value = format!("%drop_value_{index}");
            writeln!(
                module,
                "  {address} = getelementptr %{}, ptr %env, i32 0, i32 {}",
                closure_env_name(&function.name),
                index + 2
            )
            .unwrap();
            if matches!(capture.ty, Ty::Fun { .. }) {
                writeln!(module, "  {value} = load %AuraLlvmFun, ptr {address}").unwrap();
                writeln!(
                    module,
                    "  call void @aura_llvm_fun_release(%AuraLlvmFun {value})"
                )
                .unwrap();
            } else {
                writeln!(module, "  {value} = load ptr, ptr {address}").unwrap();
                writeln!(module, "  call void @{release}(ptr {value})").unwrap();
            }
        }
        module.push_str("  call void @free(ptr %env)\n  ret void\n}\n\n");
    }
    emit_class_destructors(&mut module, &context.classes);
    let mut exception_names = context
        .classes
        .keys()
        .chain(context.enum_variants.keys())
        .cloned()
        .collect::<Vec<_>>();
    exception_names.extend(
        program
            .source()
            .enums
            .iter()
            .map(|enum_decl| enum_decl.name.clone()),
    );
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
            .any(|local| contains_type_param(&local.ty))
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
    emit_enum_destructors(&mut module, &context.enum_destructors, &context.classes);
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
    let ast = &program.source().ast;
    let mut origin_offsets = HashMap::<String, usize>::new();
    program
        .checked()
        .async_mir
        .iter()
        .chain(program.checked().open_generic_async_mir.iter())
        .chain(program.checked().generic_async_mir.iter())
        .chain(program.checked().generic_async_method_mir.iter())
        .map(|body| {
            let origin_package = ast
                .async_functions
                .iter()
                .filter(|function| {
                    body.name == function.name.name
                        || body.name.starts_with(&format!("{}_", function.name.name))
                })
                .nth({
                    let key = body.name.split('_').next().unwrap_or(&body.name);
                    let offset = origin_offsets.entry(key.to_owned()).or_default();
                    let current = *offset;
                    *offset += 1;
                    current
                })
                .map(|function| {
                    if function.origin_package.is_empty() {
                        program.checked().package.clone()
                    } else {
                        function.origin_package.clone()
                    }
                })
                .unwrap_or_else(|| program.checked().package.clone());
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
            let mut function = synthetic_function(body, origin_package, parameter_count);
            function.name = format!("__aura_async_body_{}", body.name);
            function
        })
        .collect()
}

fn generic_method_functions(program: &LoweredProgram) -> Vec<FunctionIr> {
    program
        .checked()
        .generic_method_mir
        .iter()
        .map(|body| {
            let owner_package = program
                .source()
                .ast
                .classes
                .iter()
                .find(|class| body.name.starts_with(&format!("{}_", class.name.name)))
                .map(|class| class.origin_package.clone())
                .unwrap_or_else(|| program.checked().package.clone());
            let parameter_count = program
                .checked()
                .generic_method_signatures
                .iter()
                .find(|(name, _, _)| name == &body.name)
                .map(|(_, params, _)| params.len())
                .unwrap_or_else(|| {
                    body.locals
                        .iter()
                        .take_while(|local| !local.name.starts_with("__"))
                        .count()
                });
            synthetic_function(body, owner_package, parameter_count)
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
                let mut function = synthetic_function(body, package.to_owned(), captures.len());
                function.closure_captures = captures
                    .iter()
                    .map(|capture| aura_ir::mir::ClosureCapture {
                        source: capture.source,
                        ty: body.locals[capture.source.local].ty.clone(),
                        by_ref: capture.by_ref,
                    })
                    .collect();
                output.push(function);
            }
            collect_spawn_functions(body, package, output, seen);
        }
    }
}

fn emit_spawn_pollers(
    out: &mut String,
    functions: &[FunctionIr],
    package: &str,
) -> Result<(), CodegenError> {
    for function in functions {
        let Some(body) = &function.body else { continue };
        if !function.name.contains("__spawn_") {
            continue;
        }
        if function.params.iter().any(|param| {
            !(matches!(param.ty, Ty::Int | Ty::Bool | Ty::Float)
                || is_pointer_value_type(&param.ty))
        }) {
            continue;
        }
        let poll_name = format!("aura_llvm_poll_{}", symbol_name(package, &function.name));
        let pointer_result = is_pointer_value_type(&body.return_ty);
        if pointer_result {
            let drop_name = format!(
                "aura_llvm_drop_result_{}",
                symbol_name(package, &function.name)
            );
            writeln!(out, "define void @{drop_name}(ptr %data, i64 %size) {{").unwrap();
            out.push_str("entry:\n  %value = load ptr, ptr %data\n");
            let helper = if body.return_ty == Ty::String {
                "aura_llvm_str_release"
            } else if is_array_type(&body.return_ty) {
                "aura_llvm_array_release"
            } else if matches!(
                body.return_ty,
                Ty::Class(_) | Ty::ClassApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. }
            ) {
                "aura_llvm_class_release"
            } else {
                "aura_llvm_enum_release"
            };
            writeln!(out, "  call void @{helper}(ptr %value)\n  call void @free(ptr %data)\n  ret void\n}}\n\n").unwrap();
        }
        let pointer_params = function.params.iter().enumerate().any(|(index, param)| {
            function
                .closure_captures
                .get(index)
                .is_some_and(|capture| capture.by_ref)
                || is_pointer_value_type(&param.ty)
        });
        if pointer_params {
            let drop_name = format!("aura_llvm_drop_{}", symbol_name(package, &function.name));
            let mark_name = format!("aura_llvm_mark_{}", symbol_name(package, &function.name));
            writeln!(
                out,
                "define void @{drop_name}(ptr %frame, ptr %data, i64 %size) {{"
            )
            .unwrap();
            out.push_str("entry:\n");
            for (index, parameter) in function.params.iter().enumerate() {
                let by_ref = function
                    .closure_captures
                    .get(index)
                    .is_some_and(|capture| capture.by_ref);
                if !by_ref && !is_pointer_value_type(&parameter.ty) {
                    continue;
                }
                let address = format!("%drop_addr_{index}");
                let raw = format!("%drop_raw_{index}");
                let value = format!("%drop_value_{index}");
                writeln!(
                    out,
                    "  {address} = getelementptr i64, ptr %data, i64 {index}"
                )
                .unwrap();
                writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                let helper = if by_ref {
                    match parameter.ty {
                        Ty::Int => "aura_box_i64_release",
                        Ty::Bool => "aura_box_bool_release",
                        Ty::Float => "aura_box_f64_release",
                        _ => "aura_box_ptr_release",
                    }
                } else if parameter.ty == Ty::String {
                    "aura_llvm_str_release"
                } else if is_array_type(&parameter.ty) {
                    "aura_llvm_array_release"
                } else if matches!(
                    parameter.ty,
                    Ty::Class(_) | Ty::ClassApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. }
                ) {
                    "aura_llvm_class_release"
                } else {
                    "aura_llvm_enum_release"
                };
                writeln!(out, "  call void @{helper}(ptr {value})").unwrap();
            }
            out.push_str("  ret void\n}\n\n");
            writeln!(out, "define void @{mark_name}(ptr %frame) {{").unwrap();
            out.push_str("entry:\n  %mark_data = call ptr @aura_task_frame_data(ptr %frame)\n");
            for (index, parameter) in function.params.iter().enumerate() {
                let by_ref = function
                    .closure_captures
                    .get(index)
                    .is_some_and(|capture| capture.by_ref);
                if !by_ref && !is_pointer_value_type(&parameter.ty) {
                    continue;
                }
                let address = format!("%mark_addr_{index}");
                let raw = format!("%mark_raw_{index}");
                let value = format!("%mark_value_{index}");
                writeln!(
                    out,
                    "  {address} = getelementptr i64, ptr %mark_data, i64 {index}"
                )
                .unwrap();
                writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
                writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                writeln!(out, "  call void @aura_gc_mark_ptr(ptr {value})").unwrap();
            }
            out.push_str("  ret void\n}\n\n");
        }
        match body.return_ty {
            Ty::Unit => {
                writeln!(out, "define i32 @{poll_name}(ptr %frame) {{").unwrap();
                out.push_str("entry:\n");
                let exception_id = out.lines().count();
                writeln!(
                    out,
                    "  %spawn_ex_buf{exception_id} = alloca [256 x i8], align 16"
                )
                .unwrap();
                writeln!(
                    out,
                    "  call void @aura_try_enter(ptr %spawn_ex_buf{exception_id})"
                )
                .unwrap();
                writeln!(out, "  %spawn_ex_jump{exception_id} = call i32 @_setjmp(ptr %spawn_ex_buf{exception_id})").unwrap();
                writeln!(out, "  %spawn_ex_thrown{exception_id} = icmp ne i32 %spawn_ex_jump{exception_id}, 0").unwrap();
                writeln!(out, "  br i1 %spawn_ex_thrown{exception_id}, label %spawn_ex_fail{exception_id}, label %spawn_try{exception_id}").unwrap();
                writeln!(out, "spawn_try{exception_id}:").unwrap();
                let args = emit_spawn_poll_arguments(out, function)?;
                writeln!(
                    out,
                    "  call void @{}({args})",
                    symbol_name(package, &function.name)
                )
                .unwrap();
                out.push_str("  call void @aura_try_leave()\n  ret i32 2\n");
                writeln!(out, "spawn_ex_fail{exception_id}:").unwrap();
                writeln!(out, "  %spawn_ex_result{exception_id} = call i32 @aura_llvm_task_fail_from_exception(ptr %frame)").unwrap();
                writeln!(out, "  ret i32 %spawn_ex_result{exception_id}\n}}\n\n").unwrap();
            }
            Ty::Int => {
                writeln!(out, "define i32 @{poll_name}(ptr %frame) {{").unwrap();
                out.push_str("entry:\n");
                let exception_id = out.lines().count();
                writeln!(
                    out,
                    "  %spawn_ex_buf{exception_id} = alloca [256 x i8], align 16"
                )
                .unwrap();
                writeln!(
                    out,
                    "  call void @aura_try_enter(ptr %spawn_ex_buf{exception_id})"
                )
                .unwrap();
                writeln!(out, "  %spawn_ex_jump{exception_id} = call i32 @_setjmp(ptr %spawn_ex_buf{exception_id})").unwrap();
                writeln!(out, "  %spawn_ex_thrown{exception_id} = icmp ne i32 %spawn_ex_jump{exception_id}, 0").unwrap();
                writeln!(out, "  br i1 %spawn_ex_thrown{exception_id}, label %spawn_ex_fail{exception_id}, label %spawn_try{exception_id}").unwrap();
                writeln!(out, "spawn_try{exception_id}:").unwrap();
                let args = emit_spawn_poll_arguments(out, function)?;
                let value = "%task_result";
                writeln!(
                    out,
                    "  {value} = call i64 @{}({args})",
                    symbol_name(package, &function.name)
                )
                .unwrap();
                writeln!(
                    out,
                    "  call void @aura_llvm_task_set_i64(ptr %frame, i64 {value})"
                )
                .unwrap();
                out.push_str("  call void @aura_try_leave()\n  ret i32 2\n");
                writeln!(out, "spawn_ex_fail{exception_id}:").unwrap();
                writeln!(out, "  %spawn_ex_result{exception_id} = call i32 @aura_llvm_task_fail_from_exception(ptr %frame)").unwrap();
                writeln!(out, "  ret i32 %spawn_ex_result{exception_id}\n}}\n\n").unwrap();
            }
            ref ty if is_pointer_value_type(ty) => {
                writeln!(out, "define i32 @{poll_name}(ptr %frame) {{").unwrap();
                out.push_str("entry:\n");
                let exception_id = out.lines().count();
                writeln!(
                    out,
                    "  %spawn_ex_buf{exception_id} = alloca [256 x i8], align 16"
                )
                .unwrap();
                writeln!(
                    out,
                    "  call void @aura_try_enter(ptr %spawn_ex_buf{exception_id})"
                )
                .unwrap();
                writeln!(out, "  %spawn_ex_jump{exception_id} = call i32 @_setjmp(ptr %spawn_ex_buf{exception_id})").unwrap();
                writeln!(out, "  %spawn_ex_thrown{exception_id} = icmp ne i32 %spawn_ex_jump{exception_id}, 0").unwrap();
                writeln!(out, "  br i1 %spawn_ex_thrown{exception_id}, label %spawn_ex_fail{exception_id}, label %spawn_try{exception_id}").unwrap();
                writeln!(out, "spawn_try{exception_id}:").unwrap();
                let args = emit_spawn_poll_arguments(out, function)?;
                let value = "%task_result";
                writeln!(
                    out,
                    "  {value} = call ptr @{}({args})",
                    symbol_name(package, &function.name)
                )
                .unwrap();
                let drop_name = format!(
                    "@aura_llvm_drop_result_{}",
                    symbol_name(package, &function.name)
                );
                writeln!(
                    out,
                    "  call void @aura_llvm_task_set_ptr(ptr %frame, ptr {value}, ptr {drop_name})"
                )
                .unwrap();
                out.push_str("  call void @aura_try_leave()\n  ret i32 2\n");
                writeln!(out, "spawn_ex_fail{exception_id}:").unwrap();
                writeln!(out, "  %spawn_ex_result{exception_id} = call i32 @aura_llvm_task_fail_from_exception(ptr %frame)").unwrap();
                writeln!(out, "  ret i32 %spawn_ex_result{exception_id}\n}}\n\n").unwrap();
            }
            _ => continue,
        }
    }
    Ok(())
}

fn emit_spawn_poll_arguments(
    out: &mut String,
    function: &FunctionIr,
) -> Result<String, CodegenError> {
    if function.params.is_empty() {
        return Ok(String::new());
    }
    out.push_str("  %capture_data = call ptr @aura_task_frame_data(ptr %frame)\n");
    let mut arguments = Vec::new();
    for (index, parameter) in function.params.iter().enumerate() {
        let address = format!("%capture_addr_{index}");
        let raw = format!("%capture_raw_{index}");
        writeln!(
            out,
            "  {address} = getelementptr i64, ptr %capture_data, i64 {index}"
        )
        .unwrap();
        writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
        let value = match &parameter.ty {
            Ty::Int => raw,
            Ty::Bool => {
                let value = format!("%capture_bool_{index}");
                writeln!(out, "  {value} = trunc i64 {raw} to i1").unwrap();
                value
            }
            Ty::Float => {
                let value = format!("%capture_float_{index}");
                writeln!(out, "  {value} = bitcast i64 {raw} to double").unwrap();
                value
            }
            _ty if function
                .closure_captures
                .get(index)
                .is_some_and(|capture| capture.by_ref) =>
            {
                let value = format!("%capture_box_{index}");
                writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                value
            }
            ty if is_pointer_value_type(ty) => {
                let value = format!("%capture_ptr_{index}");
                writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                value
            }
            _ => return Err(unsupported("scheduler-backed spawn capture type")),
        };
        arguments.push(format!("{} {value}", llvm_type(&parameter.ty)?));
    }
    Ok(arguments.join(", "))
}

fn emit_async_wrappers(
    out: &mut String,
    program: &LoweredProgram,
    functions: &[FunctionIr],
) -> Result<(), CodegenError> {
    for declaration in &program.source().ast.async_functions {
        if matches!(declaration.name.name.as_str(), "serve" | "serveConnection") {
            continue;
        }
        let origin_package = if declaration.origin_package.is_empty() {
            program.checked().package.as_str()
        } else {
            declaration.origin_package.as_str()
        };
        let Some(function) = functions.iter().find(|candidate| {
            candidate.package == origin_package
                && (candidate.name == format!("__aura_async_body_{}", declaration.name.name)
                    || candidate
                        .name
                        .starts_with(&format!("__aura_async_body_{}_", declaration.name.name)))
        }) else {
            continue;
        };
        let Some(body) = &function.body else { continue };
        if !(matches!(body.return_ty, Ty::Unit | Ty::Int) || is_pointer_value_type(&body.return_ty))
        {
            continue;
        }
        let public_symbol = symbol_name(origin_package, &declaration.name.name);
        let body_symbol = symbol_name(&function.package, &function.name);
        let poll_name = format!("aura_llvm_poll_async_{public_symbol}");
        let result_drop = format!("aura_llvm_drop_async_result_{public_symbol}");
        let parameter_tys = body
            .locals
            .iter()
            .take(declaration.params.len())
            .map(|local| local.ty.clone())
            .collect::<Vec<_>>();
        if is_pointer_value_type(&body.return_ty) {
            writeln!(out, "define void @{result_drop}(ptr %data, i64 %size) {{").unwrap();
            out.push_str("entry:\n  %value = load ptr, ptr %data\n");
            let helper = if body.return_ty == Ty::String {
                "aura_llvm_str_release"
            } else if is_array_type(&body.return_ty) {
                "aura_llvm_array_release"
            } else {
                "aura_llvm_class_release"
            };
            writeln!(out, "  call void @{helper}(ptr %value)\n  call void @free(ptr %data)\n  ret void\n}}\n\n").unwrap();
        }
        writeln!(out, "define i32 @{poll_name}(ptr %frame) {{").unwrap();
        out.push_str("entry:\n");
        let exception_id = out.lines().count();
        writeln!(
            out,
            "  %async_ex_buf{exception_id} = alloca [256 x i8], align 16"
        )
        .unwrap();
        writeln!(
            out,
            "  call void @aura_try_enter(ptr %async_ex_buf{exception_id})"
        )
        .unwrap();
        writeln!(
            out,
            "  %async_ex_jump{exception_id} = call i32 @_setjmp(ptr %async_ex_buf{exception_id})"
        )
        .unwrap();
        writeln!(
            out,
            "  %async_ex_thrown{exception_id} = icmp ne i32 %async_ex_jump{exception_id}, 0"
        )
        .unwrap();
        writeln!(out, "  br i1 %async_ex_thrown{exception_id}, label %async_ex_fail{exception_id}, label %async_try{exception_id}").unwrap();
        writeln!(out, "async_try{exception_id}:").unwrap();
        let data = if parameter_tys.is_empty() {
            None
        } else {
            let data = "%async_data";
            writeln!(out, "  {data} = call ptr @aura_task_frame_data(ptr %frame)").unwrap();
            Some(data)
        };
        let mut call_args = Vec::new();
        for (index, ty) in parameter_tys.iter().enumerate() {
            let data = data.expect("async wrapper data for parameters");
            let address = next_temp(out);
            writeln!(
                out,
                "  {address} = getelementptr i64, ptr {data}, i64 {index}"
            )
            .unwrap();
            let raw = next_temp(out);
            writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
            let value = match ty {
                Ty::Int => raw,
                Ty::Bool => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = trunc i64 {raw} to i1").unwrap();
                    value
                }
                Ty::Float => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = bitcast i64 {raw} to double").unwrap();
                    value
                }
                _ if is_pointer_value_type(ty) => {
                    let value = next_temp(out);
                    writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
                    value
                }
                _ => return Err(unsupported("async wrapper parameter type")),
            };
            call_args.push(format!("{} {value}", llvm_type(ty)?));
        }
        let call_args = call_args.join(", ");
        if body.return_ty == Ty::Unit {
            writeln!(out, "  call void @{body_symbol}({call_args})").unwrap();
        } else if is_pointer_value_type(&body.return_ty) {
            let value = "%async_result";
            writeln!(out, "  {value} = call ptr @{body_symbol}({call_args})").unwrap();
            writeln!(
                out,
                "  call void @aura_llvm_task_set_ptr(ptr %frame, ptr {value}, ptr @{result_drop})"
            )
            .unwrap();
        } else {
            let value = "%async_result";
            writeln!(out, "  {value} = call i64 @{body_symbol}({call_args})").unwrap();
            writeln!(
                out,
                "  call void @aura_llvm_task_set_i64(ptr %frame, i64 {value})"
            )
            .unwrap();
        }
        out.push_str("  call void @aura_try_leave()\n  ret i32 2\n");
        writeln!(out, "async_ex_fail{exception_id}:").unwrap();
        writeln!(out, "  %async_ex_result{exception_id} = call i32 @aura_llvm_task_fail_from_exception(ptr %frame)").unwrap();
        writeln!(out, "  ret i32 %async_ex_result{exception_id}\n}}\n\n").unwrap();
        let public_params = parameter_tys
            .iter()
            .enumerate()
            .map(|(index, ty)| format!("{} %arg{index}", llvm_type(ty).unwrap()))
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(out, "define ptr @{public_symbol}({public_params}) {{").unwrap();
        out.push_str("entry:\n");
        out.push_str("  %executor = call ptr @aura_llvm_executor()\n");
        writeln!(
            out,
            "  %frame = call ptr @aura_task_frame_new(i64 {}, ptr @{poll_name}, ptr null)",
            parameter_tys.len() * 8
        )
        .unwrap();
        if !parameter_tys.is_empty() {
            out.push_str("  %data = call ptr @aura_task_frame_data(ptr %frame)\n");
            for (index, ty) in parameter_tys.iter().enumerate() {
                let raw = if matches!(ty, Ty::Int) {
                    format!("%arg{index}")
                } else if matches!(ty, Ty::Bool) {
                    let value = format!("%arg_raw{index}");
                    writeln!(out, "  {value} = zext i1 %arg{index} to i64").unwrap();
                    value
                } else if matches!(ty, Ty::Float) {
                    let value = format!("%arg_raw{index}");
                    writeln!(out, "  {value} = bitcast double %arg{index} to i64").unwrap();
                    value
                } else {
                    let value = format!("%arg_raw{index}");
                    writeln!(out, "  {value} = ptrtoint ptr %arg{index} to i64").unwrap();
                    value
                };
                writeln!(
                    out,
                    "  %arg_slot{index} = getelementptr i64, ptr %data, i64 {index}"
                )
                .unwrap();
                writeln!(out, "  store i64 {raw}, ptr %arg_slot{index}").unwrap();
            }
        }
        out.push_str(
            "  %submitted = call i32 @aura_task_executor_submit(ptr %executor, ptr %frame)\n",
        );
        out.push_str("  ret ptr %frame\n}\n\n");
    }
    Ok(())
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
        closure_captures: Vec::new(),
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

fn lambda_functions(program: &LoweredProgram) -> Result<Vec<FunctionIr>, CodegenError> {
    let mut functions = Vec::new();
    let mut lambdas = collect_lambdas(&program.source().ast);
    lambdas.sort_by_key(|lambda| lambda.span.start);
    for lambda in lambdas {
        let captures = program
            .source()
            .lambda_captures
            .get(&lambda.span.start)
            .cloned()
            .unwrap_or_default();
        if captures.iter().any(|capture| {
            (capture.by_ref && !is_boxable_capture_type(&capture.ty))
                || (!matches!(
                    capture.ty,
                    Ty::Int
                        | Ty::Bool
                        | Ty::Float
                        | Ty::String
                        | Ty::Fun { .. }
                        | Ty::Class(_)
                        | Ty::ClassApp { .. }
                        | Ty::Interface(_)
                        | Ty::InterfaceApp { .. }
                ) && !is_array_type(&capture.ty))
        }) {
            return Err(unsupported(
                "unsupported mutable or aggregate lambda capture",
            ));
        }
        let Some(Ty::Fun { params, ret }) = program.source().lambda_tys.get(&lambda.span.start)
        else {
            return Err(unsupported("lambda type"));
        };
        if params.len() != lambda.params.len() {
            return Err(unsupported("lambda parameter metadata"));
        }
        let body = match &lambda.body {
            LambdaBody::Expr(expression) => Block {
                stmts: vec![Stmt::Return(ReturnStmt {
                    value: Some(expression.as_ref().clone()),
                    span: lambda.span,
                })],
                span: lambda.span,
            },
            LambdaBody::Block(body) => body.clone(),
        };
        let body = aura_ir::lowering::normalize_nested_call_awaits(&body, program.source());
        let mut parameters = captures
            .iter()
            .map(|capture| (capture.name.clone(), capture.ty.clone()))
            .collect::<Vec<_>>();
        parameters.extend(
            lambda
                .params
                .iter()
                .zip(params)
                .map(|(parameter, ty)| (parameter.name.name.clone(), ty.clone()))
                .collect::<Vec<_>>(),
        );
        let name = format!("__lambda_{}", lambda.span.start);
        let body = aura_ir::lowering::lower_body(
            &name,
            &body,
            &parameters,
            (**ret).clone(),
            Some(program.source()),
            aura_ir::Effect::Pure,
        )
        .map_err(|_| unsupported("lambda body"))?;
        functions.push(FunctionIr {
            name,
            package: program.checked().package.clone(),
            params: parameters
                .iter()
                .map(|(_, ty)| aura_ir::ValueFact {
                    ty: ty.clone(),
                    ownership: aura_ir::ownership::mode_for_ty(ty),
                    span: lambda.span,
                })
                .collect(),
            closure_captures: captures
                .iter()
                .enumerate()
                .map(|(index, capture)| aura_ir::mir::ClosureCapture {
                    source: aura_ir::mir::Place { local: index },
                    ty: capture.ty.clone(),
                    by_ref: capture.by_ref,
                })
                .collect(),
            ret: aura_ir::ValueFact {
                ty: (**ret).clone(),
                ownership: aura_ir::ownership::mode_for_ty(ret),
                span: lambda.span,
            },
            type_params: Vec::new(),
            bounds: HashMap::new(),
            effect: aura_ir::Effect::Pure,
            body: Some(body),
            span: lambda.span,
        });
    }
    Ok(functions)
}

fn collect_lambdas(file: &File) -> Vec<&LambdaExpr> {
    let mut output = Vec::new();
    for function in &file.functions {
        walk_block_lambdas(&function.body, &mut output);
    }
    for function in &file.async_functions {
        walk_block_lambdas(&function.body, &mut output);
    }
    for class in &file.classes {
        for constructor in &class.constructors {
            walk_block_lambdas(&constructor.body, &mut output);
            for argument in &constructor.delegation_args {
                walk_expr_lambdas(argument, &mut output);
            }
        }
        for method in &class.methods {
            walk_block_lambdas(&method.body, &mut output);
        }
    }
    for constant in &file.consts {
        walk_expr_lambdas(&constant.value, &mut output);
    }
    output
}

fn walk_block_lambdas<'a>(block: &'a Block, output: &mut Vec<&'a LambdaExpr>) {
    for statement in &block.stmts {
        walk_stmt_lambdas(statement, output);
    }
}

fn walk_stmt_lambdas<'a>(statement: &'a Stmt, output: &mut Vec<&'a LambdaExpr>) {
    match statement {
        Stmt::Var(value) => walk_expr_lambdas(&value.init, output),
        Stmt::If(value) => {
            walk_expr_lambdas(&value.cond, output);
            walk_block_lambdas(&value.then_block, output);
            if let Some(block) = &value.else_block {
                walk_block_lambdas(block, output);
            }
        }
        Stmt::While(value) => {
            walk_expr_lambdas(&value.cond, output);
            walk_block_lambdas(&value.body, output);
        }
        Stmt::ForRange(value) => {
            walk_expr_lambdas(&value.start, output);
            walk_expr_lambdas(&value.end, output);
            walk_block_lambdas(&value.body, output);
        }
        Stmt::ForIn(value) => {
            walk_expr_lambdas(&value.iterable, output);
            walk_block_lambdas(&value.body, output);
        }
        Stmt::Match(value) => {
            walk_expr_lambdas(&value.scrutinee, output);
            for arm in &value.arms {
                walk_block_lambdas(&arm.body, output);
            }
        }
        Stmt::Try(value) => {
            walk_block_lambdas(&value.try_block, output);
            if let Some(catch) = &value.catch {
                walk_block_lambdas(&catch.body, output);
            }
            if let Some(finally) = &value.finally {
                walk_block_lambdas(finally, output);
            }
        }
        Stmt::Throw(value) => walk_expr_lambdas(&value.value, output),
        Stmt::Return(value) => {
            if let Some(expression) = &value.value {
                walk_expr_lambdas(expression, output);
            }
        }
        Stmt::Expr(expression) => walk_expr_lambdas(expression, output),
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

fn walk_expr_lambdas<'a>(expression: &'a Expr, output: &mut Vec<&'a LambdaExpr>) {
    match expression {
        Expr::Lambda(lambda) => {
            output.push(lambda);
            match &lambda.body {
                LambdaBody::Expr(body) => walk_expr_lambdas(body, output),
                LambdaBody::Block(block) => walk_block_lambdas(block, output),
            }
        }
        Expr::Call(call) => {
            walk_expr_lambdas(&call.callee, output);
            for argument in &call.args {
                walk_expr_lambdas(argument, output);
            }
        }
        Expr::Field(field) => walk_expr_lambdas(&field.object, output),
        Expr::Assign(assign) => walk_expr_lambdas(&assign.value, output),
        Expr::Binary(binary) => {
            walk_expr_lambdas(&binary.left, output);
            walk_expr_lambdas(&binary.right, output);
        }
        Expr::Unary(unary) => walk_expr_lambdas(&unary.expr, output),
        Expr::ForceUnwrap(value) => walk_expr_lambdas(&value.expr, output),
        Expr::Is(value) => walk_expr_lambdas(&value.expr, output),
        Expr::Group(value, _) => walk_expr_lambdas(value, output),
        Expr::If(value) => {
            walk_expr_lambdas(&value.cond, output);
            walk_block_lambdas(&value.then_block, output);
            walk_block_lambdas(&value.else_block, output);
        }
        Expr::Async(_) => {}
        Expr::Ident(_)
        | Expr::This(_)
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::Null(_) => {}
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
    let mut body = body.clone();
    let mut capture_indices = function
        .closure_captures
        .iter()
        .filter(|capture| capture.by_ref)
        .map(|capture| capture.source.local)
        .collect::<HashSet<_>>();
    for block in &body.blocks {
        for statement in &block.statements {
            let value = match statement {
                Statement::Assign { value, .. } | Statement::Evaluate(value) => value,
                _ => continue,
            };
            match value {
                Rvalue::Function { captures, .. } => capture_indices.extend(
                    captures
                        .iter()
                        .filter(|capture| capture.by_ref)
                        .map(|capture| capture.source.local),
                ),
                Rvalue::AsyncOp(aura_ir::mir::AsyncOp::Spawn { captures, .. }) => capture_indices
                    .extend(
                        captures
                            .iter()
                            .filter(|capture| capture.by_ref)
                            .map(|capture| capture.source.local),
                    ),
                _ => {}
            }
        }
    }
    for (index, local) in body.locals.iter_mut().enumerate() {
        let captured = capture_indices.contains(&index);
        if captured && is_boxable_capture_type(&local.ty) {
            local.name = if capture_indices.contains(&index) {
                format!("__aura_borrowed_boxed_{index}")
            } else {
                format!("__aura_boxed_{index}")
            };
        }
    }
    let ret = llvm_type(&function.ret.ty)?;
    let is_lambda = function.name.starts_with("__lambda_");
    let capture_count = function.closure_captures.len();
    let params = function
        .params
        .iter()
        .skip(if is_lambda { capture_count } else { 0 })
        .enumerate()
        .map(|(index, value)| {
            let index = index + usize::from(is_lambda);
            let by_ref = !is_lambda
                && function
                    .closure_captures
                    .get(index)
                    .is_some_and(|capture| capture.by_ref);
            let ty = if by_ref { "ptr" } else { llvm_type(&value.ty)? };
            Ok(format!("{ty} %arg{index}"))
        })
        .collect::<Result<Vec<_>, CodegenError>>()?
        .join(", ");
    let params = if is_lambda {
        if params.is_empty() {
            "ptr %arg0".to_string()
        } else {
            format!("ptr %arg0, {params}")
        }
    } else {
        params
    };
    let symbol = symbol_name(&function.package, &function.name);
    if function.name.contains("linkCancellation_") && function.params.len() == 2 {
        writeln!(out, "define i1 @{symbol}({params}) {{").unwrap();
        out.push_str("entry:\n");
        let result = next_temp(out);
        writeln!(
            out,
            "  {result} = call i32 @aura_task_frame_link_cancellation(ptr %arg0, ptr %arg1)"
        )
        .unwrap();
        writeln!(out, "  %link_ok = icmp ne i32 {result}, 0").unwrap();
        out.push_str("  ret i1 %link_ok\n}\n\n");
        return Ok(());
    }
    if function.name.contains("cancelTask_") && function.params.len() == 1 {
        writeln!(out, "define void @{symbol}({params}) {{").unwrap();
        out.push_str("entry:\n");
        let executor = next_temp(out);
        writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
        writeln!(
            out,
            "  call i32 @aura_llvm_task_cancel(ptr {executor}, ptr %arg0)"
        )
        .unwrap();
        out.push_str("  ret void\n}\n\n");
        return Ok(());
    }
    writeln!(out, "define {ret} @{}({params}) {{", symbol).unwrap();
    out.push_str("entry:\n");
    for (index, local) in body.locals.iter().enumerate() {
        if local.ty != Ty::Unit {
            let ty = if is_boxed_local(&body.locals[index]) {
                "ptr"
            } else {
                llvm_type(&local.ty)?
            };
            writeln!(out, "  %slot{index} = alloca {ty}").unwrap();
            if is_boxed_local(&body.locals[index]) {
                if is_lambda && capture_indices.contains(&index) {
                    writeln!(out, "  store ptr null, ptr %slot{index}").unwrap();
                    continue;
                }
                let boxed = box_new_expression(&local.ty)?;
                writeln!(out, "  %box{index} = {boxed}").unwrap();
                writeln!(out, "  store ptr %box{index}, ptr %slot{index}").unwrap();
                continue;
            }
            writeln!(
                out,
                "  store {ty} {}, ptr %slot{index}",
                llvm_zero(&local.ty)?
            )
            .unwrap();
        }
    }
    if is_lambda && capture_count > 0 {
        for (index, capture) in function.closure_captures.iter().enumerate() {
            let address = next_temp(out);
            writeln!(
                out,
                "  {address} = getelementptr %{}, ptr %arg0, i32 0, i32 {}",
                closure_env_name(&function.name),
                index + 2
            )
            .unwrap();
            let value = next_temp(out);
            let value_ty = if capture.by_ref {
                "ptr"
            } else {
                llvm_type(&capture.ty)?
            };
            writeln!(out, "  {value} = load {value_ty}, ptr {address}").unwrap();
            if capture.by_ref {
                writeln!(
                    out,
                    "  store ptr {value}, ptr %slot{}",
                    capture.source.local
                )
                .unwrap();
                continue;
            } else if capture.ty == Ty::String {
                writeln!(out, "  call void @aura_llvm_str_retain(ptr {value})").unwrap();
            } else if matches!(capture.ty, Ty::Fun { .. }) {
                writeln!(
                    out,
                    "  call void @aura_llvm_fun_retain(%AuraLlvmFun {value})"
                )
                .unwrap();
            } else if is_array_type(&capture.ty) {
                writeln!(out, "  call void @aura_llvm_array_retain(ptr {value})").unwrap();
            } else if matches!(
                capture.ty,
                Ty::Class(_) | Ty::ClassApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. }
            ) {
                writeln!(out, "  call void @aura_llvm_class_retain(ptr {value})").unwrap();
            }
            writeln!(
                out,
                "  store {} {value}, ptr %slot{}",
                llvm_type(&capture.ty)?,
                capture.source.local
            )
            .unwrap();
        }
    }
    for (index, local) in body.locals.iter().take(function.params.len()).enumerate() {
        if is_lambda && index < capture_count {
            continue;
        }
        if local.ty != Ty::Unit {
            let arg_index = if is_lambda {
                index - capture_count + 1
            } else {
                index
            };
            let by_ref = !is_lambda
                && function
                    .closure_captures
                    .get(index)
                    .is_some_and(|capture| capture.by_ref);
            if by_ref {
                writeln!(out, "  store ptr %arg{arg_index}, ptr %slot{index}").unwrap();
            } else {
                writeln!(
                    out,
                    "  store {} %arg{arg_index}, ptr %slot{index}",
                    llvm_type(&local.ty)?
                )
                .unwrap();
            }
        }
    }
    writeln!(out, "  br label %bb{}", body.entry).unwrap();
    for (index, block) in body.blocks.iter().enumerate() {
        writeln!(out, "bb{index}:").unwrap();
        for statement in &block.statements {
            emit_statement(out, statement, &body, context, &function.package)?;
        }
        emit_terminator(out, &block.terminator, &body, ret, context)?;
    }
    out.push_str("}\n\n");
    Ok(())
}

fn is_boxable_capture_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Int
            | Ty::Bool
            | Ty::Float
            | Ty::String
            | Ty::Fun { .. }
            | Ty::Class(_)
            | Ty::ClassApp { .. }
            | Ty::Interface(_)
            | Ty::InterfaceApp { .. }
    ) || is_array_type(ty)
}

pub(super) fn is_boxed_local(local: &aura_ir::mir::Local) -> bool {
    local.name.starts_with("__aura_boxed_") || local.name.starts_with("__aura_borrowed_boxed_")
}

pub(super) fn is_borrowed_box_local(local: &aura_ir::mir::Local) -> bool {
    local.name.starts_with("__aura_borrowed_boxed_")
}

fn box_new_expression(ty: &Ty) -> Result<String, CodegenError> {
    let expression = match ty {
        Ty::Int => "call ptr @aura_box_i64_new(i64 0)",
        Ty::Bool => "call ptr @aura_box_bool_new(i1 false)",
        Ty::Float => "call ptr @aura_box_f64_new(double 0.0)",
        Ty::String => "call ptr @aura_box_str_new(ptr null)",
        Ty::Fun { .. } => "call ptr @aura_box_ptr_new(ptr null, ptr @aura_llvm_fun_box_drop)",
        ty if is_array_type(ty) => {
            "call ptr @aura_box_ptr_new(ptr null, ptr @aura_llvm_array_release)"
        }
        Ty::Class(_) | Ty::ClassApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. } => {
            "call ptr @aura_box_ptr_new(ptr null, ptr @aura_llvm_class_release)"
        }
        _ => return Err(unsupported("mutable capture type")),
    };
    Ok(expression.into())
}

fn box_release_helper(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Int => Ok("aura_box_i64_release"),
        Ty::Bool => Ok("aura_box_bool_release"),
        Ty::Float => Ok("aura_box_f64_release"),
        Ty::String => Ok("aura_box_str_release"),
        Ty::Fun { .. } | Ty::Class(_) | Ty::Interface(_) | Ty::InterfaceApp { .. } => {
            Ok("aura_box_ptr_release")
        }
        ty if is_array_type(ty) => Ok("aura_box_ptr_release"),
        Ty::ClassApp { .. } => Ok("aura_box_ptr_release"),
        _ => Err(unsupported("mutable capture type")),
    }
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
                let loaded = if body.locals[value.local].ty == Ty::Null {
                    if let Some(zero) = nullable_zero_value(Some(&body.return_ty)) {
                        zero.to_owned()
                    } else {
                        load_place(out, *value, body)?
                    }
                } else if body.locals[value.local].ty != body.return_ty {
                    emit_use_value(out, *value, body, Some(&body.return_ty))?
                } else {
                    load_place(out, *value, body)?
                };
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
            let handle = load_place(out, *task, body)?;
            let executor = next_temp(out);
            writeln!(out, "  {executor} = call ptr @aura_llvm_executor()").unwrap();
            if *payload_ty == Ty::Unit {
                let status = next_temp(out);
                writeln!(
                    out,
                    "  {status} = call i32 @aura_llvm_task_join_status(ptr {executor}, ptr {handle})"
                )
                .unwrap();
                let await_id = out.lines().count();
                writeln!(out, "  %await_ok{await_id} = icmp eq i32 {status}, 2").unwrap();
                writeln!(out, "  br i1 %await_ok{await_id}, label %await_resume{await_id}, label %await_fail{await_id}").unwrap();
                writeln!(out, "await_fail{await_id}:").unwrap();
                writeln!(
                    out,
                    "  call void @aura_llvm_task_raise_failure(ptr {handle})"
                )
                .unwrap();
                out.push_str("  unreachable\n");
                writeln!(out, "await_resume{await_id}:").unwrap();
            } else if *payload_ty == Ty::Int && body.locals[result.local].ty == Ty::Int {
                let slot = next_temp(out);
                let status = next_temp(out);
                writeln!(out, "  {slot} = alloca i64").unwrap();
                writeln!(
                    out,
                    "  call i32 @aura_llvm_task_join_i64(ptr {executor}, ptr {handle}, ptr {slot})"
                )
                .unwrap();
                writeln!(out, "  {status} = call i32 @aura_llvm_task_join_status(ptr {executor}, ptr {handle})").unwrap();
                let await_id = out.lines().count();
                writeln!(out, "  %await_ok{await_id} = icmp eq i32 {status}, 2").unwrap();
                writeln!(out, "  br i1 %await_ok{await_id}, label %await_resume{await_id}, label %await_fail{await_id}").unwrap();
                writeln!(out, "await_fail{await_id}:").unwrap();
                writeln!(
                    out,
                    "  call void @aura_llvm_task_raise_failure(ptr {handle})"
                )
                .unwrap();
                out.push_str("  unreachable\n");
                writeln!(out, "await_resume{await_id}:").unwrap();
                let value = next_temp(out);
                writeln!(out, "  {value} = load i64, ptr {slot}").unwrap();
                writeln!(out, "  store i64 {value}, ptr %slot{}", result.local).unwrap();
            } else if is_pointer_value_type(payload_ty)
                && body.locals[result.local].ty == *payload_ty
            {
                let slot = next_temp(out);
                let status = next_temp(out);
                writeln!(out, "  {slot} = alloca ptr").unwrap();
                writeln!(
                    out,
                    "  call i32 @aura_llvm_task_join_ptr(ptr {executor}, ptr {handle}, ptr {slot})"
                )
                .unwrap();
                writeln!(out, "  {status} = call i32 @aura_llvm_task_join_status(ptr {executor}, ptr {handle})").unwrap();
                let await_id = out.lines().count();
                writeln!(out, "  %await_ok{await_id} = icmp eq i32 {status}, 2").unwrap();
                writeln!(out, "  br i1 %await_ok{await_id}, label %await_resume{await_id}, label %await_fail{await_id}").unwrap();
                writeln!(out, "await_fail{await_id}:").unwrap();
                writeln!(
                    out,
                    "  call void @aura_llvm_task_raise_failure(ptr {handle})"
                )
                .unwrap();
                out.push_str("  unreachable\n");
                writeln!(out, "await_resume{await_id}:").unwrap();
                let value = next_temp(out);
                writeln!(out, "  {value} = load ptr, ptr {slot}").unwrap();
                retain_pointer_value(out, &value, payload_ty)?;
                writeln!(out, "  store ptr {value}, ptr %slot{}", result.local).unwrap();
            } else {
                return Err(unsupported("scheduler-backed await payload"));
            }
            writeln!(out, "  br label %bb{resume}").unwrap();
            let _ = unwind;
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
            // Cancellation is terminal in the immediate ABI. There is no
            // scheduler-owned frame to resume, so return the typed zero value.
            if ret == "void" {
                out.push_str("  ret void\n");
            } else {
                writeln!(out, "  ret {ret} {}", llvm_zero(&body.return_ty)?).unwrap();
            }
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
    let interface_base = match receiver_ty {
        Ty::Interface(name) => Some(name.split('@').next().unwrap_or(name)),
        Ty::InterfaceApp { name, .. } => Some(name.split('@').next().unwrap_or(name)),
        _ => None,
    };
    let base = match receiver_ty {
        Ty::Class(name) | Ty::ClassApp { name, .. } => name.split('@').next().unwrap_or(name),
        Ty::Interface(name) | Ty::InterfaceApp { name, .. } => {
            name.split('@').next().unwrap_or(name)
        }
        _ => return Vec::new(),
    };
    let receiver_args = match receiver_ty {
        Ty::ClassApp { args, .. } | Ty::InterfaceApp { args, .. } => args.as_slice(),
        _ => &[],
    };
    let mut descendants = if let Some(interface) = interface_base {
        context
            .class_interfaces
            .iter()
            .filter_map(|(class, interfaces)| {
                interfaces
                    .iter()
                    .any(|name| name == interface)
                    .then_some(class.clone())
            })
            .collect::<Vec<_>>()
    } else {
        context
            .class_superclasses
            .iter()
            .filter_map(|(class, parent)| (parent == base).then_some(class.clone()))
            .collect::<Vec<_>>()
    };
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
            let method_args = if target.type_args.is_empty() {
                receiver_args
            } else {
                target.type_args.as_slice()
            };
            let suffix = method_args
                .iter()
                .map(Ty::mono_suffix)
                .collect::<Vec<_>>()
                .join("_");
            let generic_prefix = format!("{class}_{}_", target.name);
            let (package, name) = context.signatures.keys().find(|(_, name)| {
                (!suffix.is_empty() && *name == format!("{generic_prefix}{suffix}"))
                    || (!suffix.is_empty() && name.starts_with(&generic_prefix))
                    || *name == method
            })?;
            let type_id = *context.class_type_ids.get(&class)?;
            Some((type_id, symbol_name(package, name)))
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
        let lazy_handle = name
            .split('@')
            .next()
            .is_some_and(|base| base == "std_sync_Lazy")
            .then(|| {
                fields
                    .iter()
                    .position(|(field, ty)| field == "runtimeHandle" && *ty == Ty::Int)
            })
            .flatten();
        if !class_has_pointer_fields(fields) && lazy_handle.is_none() {
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
            let Some(helper) = pointer_release_helper(ty, classes) else {
                continue;
            };
            writeln!(
                out,
                "  %field_ptr{index} = inttoptr i64 %field_raw{index} to ptr"
            )
            .unwrap();
            writeln!(out, "  call void @{helper}(ptr %field_ptr{index})").unwrap();
        }
        if let Some(index) = lazy_handle {
            writeln!(out, "  %lazy_address = getelementptr %AuraLlvmClass, ptr %value, i32 0, i32 1, i64 {index}").unwrap();
            out.push_str("  %lazy_raw = load i64, ptr %lazy_address\n  %lazy_cell = inttoptr i64 %lazy_raw to ptr\n  call void @aura_llvm_lazy_int_destroy(ptr %lazy_cell)\n");
        }
        out.push_str("  call void @free(ptr %value)\n");
        out.push_str("  br label %done\n");
        out.push_str("done:\n  ret void\n}\n\n");
    }
}

fn enum_destructor_symbol(
    variant: Option<&str>,
    fields: &[(String, Ty)],
    context: &mut EmitContext,
) -> String {
    let Some(variant) = variant else {
        return "null".into();
    };
    let pointer_fields = fields
        .iter()
        .enumerate()
        .filter_map(|(index, (_, ty))| is_pointer_value_type(ty).then(|| (index, ty.clone())))
        .collect::<Vec<_>>();
    if pointer_fields.is_empty() {
        return "null".into();
    }
    let suffix = pointer_fields
        .iter()
        .map(|(_, ty)| ty.mono_suffix())
        .collect::<Vec<_>>()
        .join("_");
    let symbol = format!(
        "aura_llvm_enum_drop_{}_{}",
        sanitize_symbol(variant),
        sanitize_symbol(&suffix)
    );
    context
        .enum_destructors
        .entry(symbol.clone())
        .or_insert(pointer_fields);
    format!("@{symbol}")
}

fn enum_payload_destructor_symbol(ty: &Ty) -> String {
    let ty = match ty {
        Ty::Nullable(inner) => inner.as_ref(),
        other => other,
    };
    let symbol = match ty {
        Ty::String => "@aura_llvm_enum_drop_string",
        Ty::Class(_) | Ty::ClassApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. } => {
            "@aura_llvm_enum_drop_class"
        }
        Ty::Enum(_) | Ty::EnumApp { .. } => "@aura_llvm_enum_drop_enum",
        Ty::ForeignHandle(_) => "@aura_llvm_enum_drop_class",
        ty if is_array_type(ty) => "@aura_llvm_enum_drop_array",
        _ => "null",
    };
    symbol.into()
}

fn emit_enum_destructors(
    out: &mut String,
    destructors: &HashMap<String, Vec<(usize, Ty)>>,
    classes: &HashMap<String, Vec<(String, Ty)>>,
) {
    for (symbol, fields) in destructors {
        writeln!(out, "define void @{symbol}(ptr %value) {{").unwrap();
        out.push_str("entry:\n");
        for (index, (field_index, ty)) in fields.iter().enumerate() {
            let address = format!("%field_address{index}");
            let raw = format!("%field_raw{index}");
            let value = format!("%field_value{index}");
            writeln!(
                out,
                "  {address} = getelementptr %AuraLlvmEnum, ptr %value, i32 0, i32 3, i64 {field_index}"
            )
            .unwrap();
            writeln!(out, "  {raw} = load i64, ptr {address}").unwrap();
            writeln!(out, "  {value} = inttoptr i64 {raw} to ptr").unwrap();
            let Some(helper) = pointer_release_helper(ty, classes) else {
                continue;
            };
            writeln!(out, "  call void @{helper}(ptr {value})").unwrap();
        }
        out.push_str("  ret void\n}\n\n");
    }
}

fn pointer_release_helper(ty: &Ty, classes: &HashMap<String, Vec<(String, Ty)>>) -> Option<String> {
    let ty = match ty {
        Ty::Nullable(inner) => inner.as_ref(),
        other => other,
    };
    if is_array_type(ty) {
        return Some("aura_llvm_array_release".into());
    }
    Some(match ty {
        Ty::String => "aura_llvm_str_release".into(),
        Ty::Class(name) | Ty::ClassApp { name, .. } => classes
            .get(name.split('@').next().unwrap_or(name))
            .filter(|fields| class_has_pointer_fields(fields))
            .map(|_| class_release_symbol(name.split('@').next().unwrap_or(name)))
            .unwrap_or_else(|| "aura_llvm_class_release".into()),
        Ty::Interface(_) | Ty::InterfaceApp { .. } | Ty::ForeignHandle(_) => {
            "aura_llvm_class_release".into()
        }
        Ty::Enum(_) | Ty::EnumApp { .. } => "aura_llvm_enum_release".into(),
        _ => return None,
    })
}

fn sanitize_symbol(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
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

fn closure_env_name(function: &str) -> String {
    let name = function
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    format!("AuraLlvmClosureEnv_{name}")
}

fn closure_drop_name(function: &str) -> String {
    format!("aura_llvm_closure_drop_{}", closure_env_name(function))
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
