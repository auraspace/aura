//! LLVM IR backend for the complete scalar MIR subset.
//!
//! The backend emits textual LLVM IR and delegates object generation/linking
//! to the host Clang driver. It deliberately consumes MIR only; the C source
//! compatibility view is never consulted here.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use aura_ir::mir::{BinaryOp, MirBody, Place, Rvalue, Statement, Terminator, UnaryOp};
use aura_ir::{FunctionIr, LoweredProgram};
use aura_sema::Ty;

use crate::ctx::EmitOptions;
use crate::driver::{Artifact, Backend, BackendBuildOptions, BackendCapabilities};
use crate::error::CodegenError;

#[derive(Debug, Clone, Copy, Default)]
pub struct LlvmBackend;

impl LlvmBackend {
    pub fn emit_module(program: &LoweredProgram) -> Result<String, CodegenError> {
        let mut module = String::from("; ModuleID = 'aura'\nsource_filename = \"aura\"\n\n");
        for function in program
            .checked()
            .functions
            .iter()
            .chain(program.checked().generic_functions.iter())
        {
            let Some(body) = &function.body else {
                continue;
            };
            emit_function(&mut module, function, body)?;
        }
        if let Some(function) = program
            .checked()
            .functions
            .iter()
            .find(|function| function.name == "main" && function.params.is_empty())
        {
            let symbol = symbol_name(&function.package, &function.name);
            module.push_str(&format!(
                "define i32 @main() {{\nentry:\n  call void @{symbol}()\n  ret i32 0\n}}\n"
            ));
        }
        Ok(module)
    }

    pub fn compile(
        program: &LoweredProgram,
        out_bin: &Path,
        options: &BackendBuildOptions,
    ) -> Result<Artifact, CodegenError> {
        let module = Self::emit_module(program)?;
        let parent = out_bin.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| CodegenError::Io(error.to_string()))?;
        let ir_path = parent.join(format!(
            "{}.aura.ll",
            out_bin
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("out")
        ));
        fs::write(&ir_path, module).map_err(|error| CodegenError::Io(error.to_string()))?;

        let clang = std::env::var("AURA_LLVM_CC").unwrap_or_else(|_| "clang".into());
        let mut command = Command::new(&clang);
        command.arg("-x").arg("ir");
        command.arg(format!("-{}", options.optimization.flag()));
        if options.debug {
            command.arg("-g");
        }
        command.arg(&ir_path).arg("-o").arg(out_bin);
        let output = command
            .output()
            .map_err(|error| CodegenError::Compile(format!("failed to spawn {clang}: {error}")))?;
        if !output.status.success() {
            return Err(CodegenError::Compile(format!(
                "{clang} failed to compile LLVM IR: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(Artifact::from_backend(out_bin.to_path_buf(), options))
    }
}

impl Backend for LlvmBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            requires_complete_mir: true,
            accepts_alpha_source: false,
            requires_c_runtime: false,
            supports_native_compile: true,
        }
    }

    fn emit(&self, program: &LoweredProgram, _opts: EmitOptions) -> String {
        Self::emit_module(program).unwrap_or_else(|error| format!("; LLVM emission error: {error}"))
    }

    fn compile_ir(
        &self,
        program: &LoweredProgram,
        out_bin: &Path,
        options: &BackendBuildOptions,
        _opts: crate::driver::BackendOptions,
    ) -> Result<Artifact, CodegenError> {
        Self::compile(program, out_bin, options)
    }
}

fn emit_function(
    out: &mut String,
    function: &FunctionIr,
    body: &MirBody,
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
    writeln!(out, "bb{}:", body.entry).unwrap();
    for (index, local) in body.locals.iter().enumerate() {
        if local.ty != Ty::Unit {
            writeln!(out, "  %slot{index} = alloca {}", llvm_type(&local.ty)?).unwrap();
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
    for (index, block) in body.blocks.iter().enumerate() {
        if index != body.entry {
            writeln!(out, "bb{index}:").unwrap();
        }
        for statement in &block.statements {
            emit_statement(out, statement, body)?;
        }
        emit_terminator(out, &block.terminator, body, ret)?;
    }
    out.push_str("}\n\n");
    Ok(())
}

fn emit_statement(
    out: &mut String,
    statement: &Statement,
    body: &MirBody,
) -> Result<(), CodegenError> {
    match statement {
        Statement::Assign { place, value } => {
            let value = emit_rvalue(out, value, body, Some(&body.locals[place.local].ty))?;
            let ty = llvm_type(&body.locals[place.local].ty)?;
            writeln!(out, "  store {ty} {value}, ptr %slot{}", place.local).unwrap();
        }
        Statement::Move { from, to }
        | Statement::Clone { from, to }
        | Statement::Retain { from, to } => {
            copy_place(out, *from, *to, body)?;
        }
        Statement::Evaluate(value) => {
            let _ = emit_rvalue(out, value, body, None)?;
        }
        Statement::Drop(_) | Statement::EnterTry { .. } | Statement::LeaveTry => {}
        Statement::ExtractVariantField { .. } | Statement::LoadIndex { .. } => {
            return Err(unsupported("aggregate extraction/indexing"));
        }
    }
    Ok(())
}

fn copy_place(
    out: &mut String,
    from: Place,
    to: Place,
    body: &MirBody,
) -> Result<(), CodegenError> {
    let ty = llvm_type(&body.locals[from.local].ty)?;
    let value = load_place(out, from, body)?;
    writeln!(out, "  store {ty} {value}, ptr %slot{}", to.local).unwrap();
    Ok(())
}

fn emit_terminator(
    out: &mut String,
    term: &Terminator,
    body: &MirBody,
    ret: &str,
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
                let value = value.ok_or_else(|| unsupported("missing return value"))?;
                let loaded = load_place(out, value, body)?;
                writeln!(out, "  ret {ret} {loaded}").unwrap();
            }
        }
        Terminator::Unreachable => out.push_str("  unreachable\n"),
        Terminator::SwitchTag { .. }
        | Terminator::Await { .. }
        | Terminator::Throw { .. }
        | Terminator::Cancel => {
            return Err(unsupported(
                "tag switch, async, exception, or cancellation control flow",
            ));
        }
    }
    Ok(())
}

fn emit_rvalue(
    out: &mut String,
    value: &Rvalue,
    body: &MirBody,
    result_ty: Option<&Ty>,
) -> Result<String, CodegenError> {
    match value {
        Rvalue::Use(place) => load_place(out, *place, body),
        Rvalue::ConstInt(value) => Ok(value.to_string()),
        Rvalue::ConstBool(value) => Ok(if *value { "true" } else { "false" }.into()),
        Rvalue::ConstNull => Ok("null".into()),
        Rvalue::ConstString(_) => Err(unsupported("String constants")),
        Rvalue::Unary { op, operand } => {
            let value = load_place(out, *operand, body)?;
            match op {
                UnaryOp::Neg => Ok(format!("sub i64 0, {value}")),
                UnaryOp::Not => Ok(format!("xor i1 {value}, true")),
            }
        }
        Rvalue::Binary { op, left, right } => {
            let operand_ty = llvm_type(&body.locals[left.local].ty)?;
            let left = load_place(out, *left, body)?;
            let right = load_place(out, *right, body)?;
            let instruction = match op {
                BinaryOp::Add => "add",
                BinaryOp::Sub => "sub",
                BinaryOp::Mul => "mul",
                BinaryOp::Div => "sdiv",
                BinaryOp::Rem => "srem",
                BinaryOp::And => "and",
                BinaryOp::Or => "or",
                BinaryOp::Eq => "icmp eq",
                BinaryOp::Ne => "icmp ne",
                BinaryOp::Lt => "icmp slt",
                BinaryOp::Le => "icmp sle",
                BinaryOp::Gt => "icmp sgt",
                BinaryOp::Ge => "icmp sge",
                BinaryOp::Coalesce => return Err(unsupported("coalesce")),
            };
            let temp = next_temp(out);
            writeln!(out, "  {temp} = {instruction} {operand_ty} {left}, {right}").unwrap();
            Ok(temp)
        }
        Rvalue::Select {
            condition,
            then_value,
            else_value,
        } => {
            let condition = load_place(out, *condition, body)?;
            let then_value = load_place(out, *then_value, body)?;
            let else_value = load_place(out, *else_value, body)?;
            let selected_ty = llvm_type(result_ty.unwrap_or(&Ty::Int))?;
            let temp = next_temp(out);
            writeln!(
                out,
                "  {temp} = select i1 {condition}, {selected_ty} {then_value}, {selected_ty} {else_value}"
            )
            .unwrap();
            Ok(temp)
        }
        Rvalue::Call { target, args } => {
            let values = args
                .iter()
                .map(|place| load_place(out, *place, body))
                .collect::<Result<Vec<_>, _>>()?;
            let temp = next_temp(out);
            let name = symbol_name(&target.package, &target.name);
            writeln!(
                out,
                "  {temp} = call i64 @{name}({})",
                values
                    .iter()
                    .map(|value| format!("i64 {value}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .unwrap();
            Ok(temp)
        }
        Rvalue::Intrinsic(_)
        | Rvalue::Unwrap { .. }
        | Rvalue::TypeTest { .. }
        | Rvalue::VariantTag { .. }
        | Rvalue::Length(_)
        | Rvalue::Index { .. }
        | Rvalue::Field { .. }
        | Rvalue::AsyncOp(_) => Err(unsupported("non-scalar MIR operation")),
    }
}

fn load_place(out: &mut String, place: Place, body: &MirBody) -> Result<String, CodegenError> {
    let ty = llvm_type(&body.locals[place.local].ty)?;
    let temp = next_temp(out);
    writeln!(out, "  {temp} = load {ty}, ptr %slot{}", place.local).unwrap();
    Ok(temp)
}

fn next_temp(out: &str) -> String {
    format!(
        "%t{}",
        out.lines().filter(|line| line.contains(" = ")).count()
    )
}

fn llvm_type(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Unit => Ok("void"),
        Ty::Int => Ok("i64"),
        Ty::Bool => Ok("i1"),
        Ty::Float => Ok("double"),
        Ty::Null => Ok("ptr"),
        _ => Err(unsupported(&format!("type {}", ty.display()))),
    }
}

fn symbol_name(package: &str, name: &str) -> String {
    let package = package
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    format!("aura_{}_{}", package, name)
}

fn unsupported(feature: &str) -> CodegenError {
    CodegenError::Configuration(format!(
        "LLVM backend does not support {feature} in the current MIR contract"
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::LlvmBackend;
    use crate::driver::BackendBuildOptions;
    use crate::options::{
        Backend, Lto, OptimizationLevel, OutputKind, PanicStrategy, Profile, Target,
    };
    use aura_ir::LoweredProgram;
    use aura_parser::parse_file;
    use aura_sema::check_file;

    #[test]
    fn emits_valid_scalar_llvm_module() {
        let file =
            parse_file("package demo\nfun add(a: Int, b: Int): Int { return a + b }\n").unwrap();
        let checked = check_file(&file).unwrap();
        let module = LlvmBackend::emit_module(&LoweredProgram::from_checked(checked)).unwrap();
        assert!(module.contains("define i64 @aura_demo_add"));
        assert!(module.contains("add i64"));
        let ir_path =
            std::path::PathBuf::from(format!("/tmp/aura-llvm-module-{}.ll", std::process::id()));
        let object_path = ir_path.with_extension("o");
        std::fs::write(&ir_path, module).unwrap();
        let status = std::process::Command::new("clang")
            .args(["-x", "ir", "-c"])
            .arg(&ir_path)
            .arg("-o")
            .arg(&object_path)
            .status()
            .unwrap();
        assert!(status.success());
        let _ = std::fs::remove_file(ir_path);
        let _ = std::fs::remove_file(object_path);
    }

    #[test]
    fn compiles_empty_main_with_clang_when_available() {
        if std::process::Command::new("clang")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let file = parse_file("package demo\nfun main() {}\n").unwrap();
        let checked = check_file(&file).unwrap();
        let program = LoweredProgram::from_checked(checked);
        assert!(program.mir_is_complete());
        let out = PathBuf::from(format!("/tmp/aura-llvm-test-{}", std::process::id()));
        let options = BackendBuildOptions {
            backend: Backend::Llvm,
            target: Target::Native,
            profile: Profile::Debug,
            optimization: OptimizationLevel::O0,
            debug: false,
            lto: Lto::Off,
            panic: PanicStrategy::Abort,
            output: OutputKind::Executable,
            features: Vec::new(),
        };
        LlvmBackend::compile(&program, &out, &options).unwrap();
        assert!(std::process::Command::new(&out).status().unwrap().success());
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(out.with_file_name(format!(
            "{}.aura.ll",
            out.file_name().unwrap().to_string_lossy()
        )));
    }
}
