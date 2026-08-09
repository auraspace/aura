//! Backend-neutral compilation pipeline.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aura_ast::File;
use aura_ir::{ForeignLinkIr, LoweredProgram};
use aura_mir::mir::Terminator;
use aura_sema::{check_file, CheckedFile};

use crate::backends::llvm::LlvmBackend;
use crate::ctx::EmitOptions;
use crate::emit::emit_c_with_program;
use crate::error::CodegenError;
use crate::options::{
    Backend as BackendKind, CompileOptions, Lto, NativeSource, OptimizationLevel, OutputKind,
    PanicStrategy, Profile, ProfileSettings, RuntimeAbi, Target,
};
use crate::validation::{compiler_command, validate_build};

fn compiler_process(compiler: &str) -> Command {
    if let Some(wrapper) = std::env::var_os("AURA_CC_WRAPPER") {
        let mut command = Command::new(wrapper);
        command.arg(compiler);
        command
    } else {
        Command::new(compiler)
    }
}

/// Stable identity of the backend build that produced an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildIdentity {
    pub backend: BackendKind,
    pub target: Target,
    pub profile: Profile,
    pub profile_settings: ProfileSettings,
    pub runtime_abi: Option<RuntimeAbi>,
    pub runtime_abi_version: Option<u32>,
    pub runtime_abi_identity: Option<&'static str>,
    pub output: OutputKind,
    pub features: Vec<String>,
    pub native_sources: Vec<String>,
}

impl From<&CompileOptions> for BuildIdentity {
    fn from(options: &CompileOptions) -> Self {
        Self {
            backend: options.backend,
            target: options.target,
            profile: options.profile,
            profile_settings: options.profile_settings.clone(),
            runtime_abi: options.runtime_abi,
            runtime_abi_version: options.runtime_abi.map(RuntimeAbi::version),
            runtime_abi_identity: options.runtime_abi.map(RuntimeAbi::identity),
            output: options.output,
            features: options.features.iter().cloned().collect(),
            native_sources: options
                .native_sources
                .iter()
                .map(native_source_identity)
                .collect(),
        }
    }
}

impl std::fmt::Display for BuildIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let features = self.features.join(",");
        write!(
            f,
            "backend={:?}, target={:?}, profile={:?}, settings={:?}, runtime_abi={:?}/{:?}/{:?}, output={:?}, features=[{}], native_sources={:?}",
            self.backend,
            self.target,
            self.profile,
            self.profile_settings,
            self.runtime_abi,
            self.runtime_abi_version,
            self.runtime_abi_identity,
            self.output,
            features,
            self.native_sources
        )
    }
}

/// The result reported by a backend after producing an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    path: PathBuf,
    identity: BuildIdentity,
}

/// Options visible at the target-neutral backend boundary.
#[derive(Debug, Clone, Copy, Default)]
pub struct BackendOptions {
    pub test: bool,
    pub instrumentation: bool,
}

/// Declares which inputs a backend is allowed to consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub requires_complete_mir: bool,
    pub supports_native_compile: bool,
}

/// Build settings shared by native MIR backends. C runtime/compiler details
/// intentionally stay outside this contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendBuildOptions {
    pub backend: BackendKind,
    pub target: Target,
    pub profile: Profile,
    pub optimization: OptimizationLevel,
    pub debug: bool,
    pub lto: Lto,
    pub panic: PanicStrategy,
    pub output: OutputKind,
    pub features: Vec<String>,
}

impl From<&CompileOptions> for BackendBuildOptions {
    fn from(options: &CompileOptions) -> Self {
        Self {
            backend: options.backend,
            target: options.target,
            profile: options.profile,
            optimization: options.profile_settings.optimization,
            debug: options.profile_settings.debug,
            lto: options.profile_settings.lto,
            panic: options.profile_settings.panic,
            output: options.output,
            features: options.features.iter().cloned().collect(),
        }
    }
}

impl From<EmitOptions> for BackendOptions {
    fn from(options: EmitOptions) -> Self {
        Self {
            test: options.test,
            instrumentation: options.detector,
        }
    }
}

impl Artifact {
    fn new(path: PathBuf, options: &CompileOptions) -> Self {
        Self {
            path,
            identity: BuildIdentity::from(options),
        }
    }

    /// Construct an artifact identity for a native MIR backend without C
    /// runtime ABI settings or a C-shaped compile option object.
    pub fn from_backend(path: PathBuf, options: &BackendBuildOptions) -> Self {
        Self {
            path,
            identity: BuildIdentity {
                backend: options.backend,
                target: options.target,
                profile: options.profile,
                profile_settings: ProfileSettings {
                    optimization: options.optimization,
                    debug: options.debug,
                    lto: options.lto,
                    detector: false,
                    panic: options.panic,
                    backend: options.backend,
                    linker: None,
                },
                runtime_abi: None,
                runtime_abi_version: None,
                runtime_abi_identity: None,
                output: options.output,
                features: options.features.clone(),
                native_sources: Vec::new(),
            },
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn identity(&self) -> &BuildIdentity {
        &self.identity
    }

    pub(crate) fn into_path(self) -> PathBuf {
        self.path
    }
}

/// Backend boundary after frontend and semantic checking have completed.
/// Target-neutral backend contract. Every implementation receives only the
/// fully lowered program; the C compatibility adapter is defined separately.
pub trait Backend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            requires_complete_mir: true,
            supports_native_compile: false,
        }
    }

    fn emit_ir(&self, program: &LoweredProgram, opts: BackendOptions) -> String;

    fn compile_ir(
        &self,
        _program: &LoweredProgram,
        _out_bin: &Path,
        _options: &BackendBuildOptions,
        _opts: BackendOptions,
    ) -> Result<Artifact, CodegenError> {
        Err(CodegenError::Configuration(
            "backend does not provide neutral native artifact compilation".into(),
        ))
    }
}

/// Compatibility-only adapter for the legacy C source/runtime pipeline.
pub trait CBackendCompatibility {
    fn emit_c(&self, program: &LoweredProgram, opts: EmitOptions) -> String;

    fn compile_c(
        &self,
        program: &LoweredProgram,
        out_bin: &Path,
        runtime_c: &Path,
        options: &CompileOptions,
        opts: EmitOptions,
    ) -> Result<Artifact, CodegenError>;
}

/// Runs frontend/sema once, then delegates emission or compilation to a backend.
pub struct Driver<B> {
    backend: B,
}

/// Build an artifact while retaining its inspectable backend identity.
pub fn build_artifact(
    file: &File,
    out_bin: &Path,
    runtime_c: &Path,
    options: CompileOptions,
    opts: EmitOptions,
) -> Result<Artifact, CodegenError> {
    CBackendDriver::build(file, out_bin, runtime_c, options, opts)
}

/// Compile a semantically checked file supplied by an external compiler host.
/// This preserves host-side macro/plugin expansion instead of re-running the
/// plain `check_file` path and discarding generated items.
pub fn build_artifact_from_checked(
    checked: &CheckedFile,
    out_bin: &Path,
    runtime_c: &Path,
    options: CompileOptions,
    opts: EmitOptions,
) -> Result<Artifact, CodegenError> {
    let program = LoweredProgram::from_checked(checked.clone());
    match options.backend {
        BackendKind::C => {
            validate_build(&options, &compiler_command(), runtime_c)?;
            CBackend.compile_c(&program, out_bin, runtime_c, &options, opts)
        }
        BackendKind::Llvm => {
            options
                .validate()
                .map_err(|error| CodegenError::Configuration(error.to_string()))?;
            if !program.mir_is_complete_for_entrypoint() {
                return Err(CodegenError::Configuration(format!(
                    "LLVM backend requires complete MIR; unsupported functions: {}",
                    program.reachable_lowering_gap_names().join(", ")
                )));
            }
            LlvmBackend.compile_ir(
                &program,
                out_bin,
                &BackendBuildOptions::from(&options),
                opts.into(),
            )
        }
        BackendKind::Cranelift => Err(CodegenError::Configuration(
            "Cranelift backend is not implemented".into(),
        )),
    }
}

impl<B: Backend> Driver<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub fn emit(&self, file: &File, opts: EmitOptions) -> Result<String, CodegenError> {
        let checked = check_file(file)?;
        let program = LoweredProgram::from_checked(checked);
        if self.backend.capabilities().requires_complete_mir
            && !program.mir_is_complete_for_entrypoint()
        {
            return Err(CodegenError::Configuration(format!(
                "backend requires complete MIR; unsupported functions: {}",
                program.reachable_lowering_gap_names().join(", ")
            )));
        }
        Ok(self.backend.emit_ir(&program, opts.into()))
    }

    pub fn build(
        &self,
        file: &File,
        out_bin: &Path,
        options: CompileOptions,
        opts: EmitOptions,
    ) -> Result<Artifact, CodegenError> {
        options
            .validate()
            .map_err(|error| CodegenError::Configuration(error.to_string()))?;
        let checked = check_file(file)?;
        let program = LoweredProgram::from_checked(checked);
        if self.backend.capabilities().requires_complete_mir
            && !program.mir_is_complete_for_entrypoint()
        {
            return Err(CodegenError::Configuration(format!(
                "backend requires complete MIR; unsupported functions: {}",
                program.reachable_lowering_gap_names().join(", ")
            )));
        }
        self.backend.compile_ir(
            &program,
            out_bin,
            &BackendBuildOptions::from(&options),
            opts.into(),
        )
    }
}

/// Driver for the C compatibility path. It is intentionally separate from
/// the target-neutral `Driver` so runtime source/compiler inputs cannot leak
/// into the Backend trait.
pub struct CBackendDriver;

impl CBackendDriver {
    pub fn emit(file: &File, opts: EmitOptions) -> Result<String, CodegenError> {
        Driver::new(CBackend).emit(file, opts)
    }

    pub fn build(
        file: &File,
        out_bin: &Path,
        runtime_c: &Path,
        options: CompileOptions,
        opts: EmitOptions,
    ) -> Result<Artifact, CodegenError> {
        validate_build(&options, &compiler_command(), runtime_c)?;
        let checked = check_file(file)?;
        let program = LoweredProgram::from_checked(checked);
        CBackend.compile_c(&program, out_bin, runtime_c, &options, opts)
    }
}

pub use crate::backends::c::CBackend;

/// A backend probe that consumes only target-neutral IR.
///
/// This is intentionally a textual backend rather than a second native
/// compiler: it proves that backend discovery, validation, and emission do
/// not require C source or the C runtime ABI.
pub struct MirBackend;

impl Backend for MirBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            requires_complete_mir: true,
            supports_native_compile: false,
        }
    }

    fn emit_ir(&self, program: &LoweredProgram, _opts: BackendOptions) -> String {
        let ir = program.checked();
        let mut out = String::new();
        out.push_str("aura-mir version=1\n");
        out.push_str(&format!("package {}\n", ir.package));
        for function in ir.functions.iter().chain(ir.generic_functions.iter()) {
            let Some(body) = function.body.as_ref() else {
                continue;
            };
            out.push_str(&format!(
                "function {} blocks={} locals={} effect={:?}\n",
                function.name,
                body.blocks.len(),
                body.locals.len(),
                body.effect
            ));
            for (index, block) in body.blocks.iter().enumerate() {
                let terminator = match &block.terminator {
                    Terminator::Goto { .. } => "goto",
                    Terminator::SwitchInt { .. } => "switch",
                    Terminator::SwitchTag { .. } => "switch-tag",
                    Terminator::Await { .. } => "await",
                    Terminator::Return { .. } => "return",
                    Terminator::Throw { .. } => "throw",
                    Terminator::Cancel => "cancel",
                    Terminator::Unreachable => "unreachable",
                };
                out.push_str(&format!(
                    "  block {} statements={} terminator={}\n",
                    index,
                    block.statements.len(),
                    terminator
                ));
            }
        }
        for body in ir
            .async_mir
            .iter()
            .chain(ir.open_generic_async_mir.iter())
            .chain(ir.generic_async_mir.iter())
            .chain(ir.generic_async_method_mir.iter())
        {
            out.push_str(&format!(
                "async-body {} blocks={} locals={} effect={:?}\n",
                body.name,
                body.blocks.len(),
                body.locals.len(),
                body.effect
            ));
        }
        for machine in ir
            .async_state_machines
            .iter()
            .chain(ir.open_generic_async_state_machines.iter())
            .chain(ir.generic_async_state_machines.iter())
            .chain(ir.generic_async_method_state_machines.iter())
            .chain(ir.spawn_state_machines.iter())
        {
            out.push_str(&format!(
                "state-machine {} states={} frame-locals={}\n",
                machine.function,
                machine.states.len(),
                machine.frame_locals.len()
            ));
        }
        out
    }
}

impl Backend for CBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            requires_complete_mir: false,
            supports_native_compile: false,
        }
    }

    fn emit_ir(&self, program: &LoweredProgram, opts: BackendOptions) -> String {
        let opts = EmitOptions {
            test: opts.test,
            detector: opts.instrumentation,
        };
        emit_c_with_program(program, opts)
    }
}

impl CBackendCompatibility for CBackend {
    fn emit_c(&self, program: &LoweredProgram, opts: EmitOptions) -> String {
        emit_c_with_program(program, opts)
    }

    fn compile_c(
        &self,
        program: &LoweredProgram,
        out_bin: &Path,
        runtime_c: &Path,
        options: &CompileOptions,
        opts: EmitOptions,
    ) -> Result<Artifact, CodegenError> {
        let compiler = compiler_command();
        validate_build(options, &compiler, runtime_c)?;

        // Keep these identities at the backend boundary even while C is the
        // only supported implementation. They become meaningful when other
        // backend/target/runtime combinations are added.
        let _backend = options.backend;
        let _target = options.target;
        let _runtime_abi = options.runtime_abi;
        let _output = options.output;
        let mut emit_opts = opts;
        emit_opts.detector = options.profile_settings.detector;
        let c_src = self.emit_c(program, emit_opts);
        let parent = out_bin
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&parent).map_err(|e| CodegenError::Io(e.to_string()))?;

        let c_path = parent.join(format!(
            "{}.aura.c",
            out_bin
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("out")
        ));
        fs::write(&c_path, &c_src).map_err(|e| CodegenError::Io(e.to_string()))?;

        let mut compile_flags = vec![
            format!("-{}", options.profile_settings.optimization.flag()),
            "-std=c11".to_owned(),
        ];
        if options.profile_settings.debug {
            compile_flags.push("-g".into());
        }
        if options.profile_settings.lto != Lto::Off {
            compile_flags.push("-flto".into());
        }
        if options.profile_settings.detector {
            compile_flags.push("-fsanitize=address,undefined".into());
        }
        for native in &options.native_sources {
            for include in &native.include_dirs {
                compile_flags.push("-I".into());
                compile_flags.push(include.to_string_lossy().into_owned());
            }
            for define in &native.defines {
                compile_flags.push(format!("-D{define}"));
            }
        }
        if c_src.contains("AURA_TLS_REQUIRED") {
            compile_flags.push("-DAURA_TLS_ENABLE=1".into());
            for include in ["/opt/homebrew/include", "/usr/local/include"] {
                if Path::new(include).join("openssl/ssl.h").is_file() {
                    compile_flags.push("-I".into());
                    compile_flags.push(include.into());
                    break;
                }
            }
        }

        let c_object = parent.join(format!(
            "{}.aura.o",
            out_bin
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("out")
        ));
        let runtime_object = parent.join(format!(
            "{}.runtime.o",
            out_bin
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("out")
        ));
        let (source, object) = (c_path.as_path(), c_object.as_path());
        let status = compiler_process(&compiler)
            .args(&compile_flags)
            .arg("-c")
            .arg(source)
            .arg("-o")
            .arg(object)
            .status()
            .map_err(|e| CodegenError::Compile(format!("failed to spawn {compiler}: {e}")))?;
        if !status.success() {
            return Err(CodegenError::Compile(format!(
                "{compiler} failed compiling source {} with status {status}",
                source.display()
            )));
        }

        let runtime_link = if is_runtime_archive(runtime_c) {
            runtime_c.to_path_buf()
        } else {
            let status = compiler_process(&compiler)
                .args(&compile_flags)
                .arg("-c")
                .arg(runtime_c)
                .arg("-o")
                .arg(&runtime_object)
                .status()
                .map_err(|e| CodegenError::Compile(format!("failed to spawn {compiler}: {e}")))?;
            if !status.success() {
                return Err(CodegenError::Compile(format!(
                    "{compiler} failed compiling runtime input {} with status {status}",
                    runtime_c.display()
                )));
            }
            runtime_object
        };

        let mut native_objects = Vec::new();
        for (index, native) in options.native_sources.iter().enumerate() {
            validate_native_source(native)?;
            let object = parent.join(format!(
                "{}.native-{index}.o",
                out_bin
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("out")
            ));
            let status = compiler_process(&compiler)
                .args(&compile_flags)
                .arg("-c")
                .arg(&native.source)
                .arg("-o")
                .arg(&object)
                .status()
                .map_err(|e| {
                    CodegenError::Compile(format!(
                        "failed to spawn {compiler} for native source {}: {e}",
                        native.source.display()
                    ))
                })?;
            if !status.success() {
                return Err(CodegenError::Compile(format!(
                    "{compiler} failed compiling native source {} with status {status}; {}",
                    native.source.display(),
                    native_context(native)
                )));
            }
            native_objects.push(object);
        }

        let mut command = compiler_process(&compiler);
        if c_src.contains("AURA_TLS_REQUIRED") {
            for library in ["/opt/homebrew/lib", "/usr/local/lib"] {
                if Path::new(library).join("libssl.dylib").is_file()
                    || Path::new(library).join("libssl.so").is_file()
                {
                    command.arg("-L").arg(library);
                    break;
                }
            }
        }
        command.args(&compile_flags);
        if let Some(linker) = &options.profile_settings.linker {
            command.arg(format!("-fuse-ld={linker}"));
        }
        for path in &options.foreign_library_paths {
            command.arg("-L").arg(path);
        }
        let mut foreign_link_args = Vec::new();
        for foreign in program.foreign_libraries() {
            // Package-owned native sources are already present as objects.
            // Do not ask the system linker to find a second archive.
            if options
                .native_sources
                .iter()
                .any(|source| source.name == foreign.library)
            {
                continue;
            }
            match foreign.link {
                ForeignLinkIr::Dynamic => {
                    foreign_link_args.push(format!("-l{}", foreign.library));
                }
                ForeignLinkIr::Static if cfg!(target_os = "linux") => {
                    foreign_link_args.push("-Wl,-Bstatic".into());
                    foreign_link_args.push(format!("-l{}", foreign.library));
                    foreign_link_args.push("-Wl,-Bdynamic".into());
                }
                ForeignLinkIr::Static if cfg!(target_os = "macos") => {
                    let archive_name = format!("lib{}.a", foreign.library);
                    let archive = options
                        .foreign_library_paths
                        .iter()
                        .map(|path| path.join(&archive_name))
                        .find(|path| path.is_file())
                        .unwrap_or_else(|| PathBuf::from(&archive_name));
                    foreign_link_args
                        .push(format!("-Wl,-force_load,{}", archive.to_string_lossy()));
                }
                ForeignLinkIr::Static => {}
            }
        }
        command
            .arg(&c_object)
            .arg(&runtime_link)
            .args(&native_objects)
            .args(&foreign_link_args)
            .arg("-lz");
        for native in &options.native_sources {
            command.args(&native.linker_args);
        }
        if c_src.contains("AURA_TLS_REQUIRED") {
            command.arg("-lssl").arg("-lcrypto");
        }
        let status = command
            .arg("-o")
            .arg(out_bin)
            .status()
            .map_err(|e| CodegenError::Compile(format!("failed to spawn {compiler}: {e}")))?;

        if !status.success() {
            return Err(CodegenError::Compile(format!(
                "{compiler} failed with status {status} (source {})",
                c_path.display()
            )));
        }

        write_native_metadata(out_bin, &options.native_sources)
            .map_err(|e| CodegenError::Io(format!("write native build metadata: {e}")))?;

        Ok(Artifact::new(out_bin.to_path_buf(), options))
    }
}

fn is_runtime_archive(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("a" | "lib")
    )
}

fn validate_native_source(source: &NativeSource) -> Result<(), CodegenError> {
    if !source.source.is_file() {
        return Err(CodegenError::Configuration(format!(
            "native source not found: {} (check [native.*].sources)",
            source.source.display()
        )));
    }
    for include in &source.include_dirs {
        if !include.is_dir() {
            return Err(CodegenError::Configuration(format!(
                "native include directory not found: {} (source {})",
                include.display(),
                source.source.display()
            )));
        }
    }
    Ok(())
}

fn native_context(source: &NativeSource) -> String {
    format!(
        "name={}, include_dirs={:?}, defines={:?}, linker_args={:?}, static={}",
        source.name, source.include_dirs, source.defines, source.linker_args, source.static_link
    )
}

fn write_native_metadata(path: &Path, sources: &[NativeSource]) -> Result<(), String> {
    if sources.is_empty() {
        return Ok(());
    }
    let mut text = String::from("schema=aura-native-build-v1\n");
    for source in sources {
        let bytes =
            fs::read(&source.source).map_err(|e| format!("{}: {e}", source.source.display()))?;
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        text.push_str(&format!(
            "name={}\nsource={}\nsha256={digest}\n",
            source.name,
            source.source.display()
        ));
        for include in &source.include_dirs {
            text.push_str(&format!("include={}\n", include.display()));
        }
        for define in &source.defines {
            text.push_str(&format!("define={define}\n"));
        }
        text.push_str(&format!("static={}\n", source.static_link));
    }
    fs::write(path.with_extension("native.meta"), text).map_err(|e| e.to_string())
}

fn native_source_identity(source: &NativeSource) -> String {
    let digest = fs::read(&source.source)
        .map(|bytes| {
            Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        })
        .unwrap_or_else(|_| "missing".into());
    format!("{}:{}#{}", source.name, source.source.display(), digest)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use aura_ast::{File, Ident, Path as AstPath, Span};
    use aura_ir::LoweredProgram;
    use aura_parser::parse_file;

    use super::{
        Artifact, Backend, BackendBuildOptions, BackendCapabilities, BackendOptions, BuildIdentity,
        Driver, MirBackend,
    };
    use crate::ctx::EmitOptions;
    use crate::error::CodegenError;
    use crate::options::CompileOptions;

    fn empty_file() -> File {
        let span = Span::new(0, 0);
        File {
            package: AstPath {
                segments: vec![Ident {
                    name: "demo".into(),
                    span,
                }],
                span,
            },
            imports: vec![],
            interfaces: vec![],
            enums: vec![],
            classes: vec![],
            type_aliases: vec![],
            consts: vec![],
            functions: vec![],
            foreign_functions: vec![],
            async_functions: vec![],
            span,
        }
    }

    struct FailingBackend {
        compile_calls: Rc<Cell<usize>>,
    }

    impl Backend for FailingBackend {
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities {
                requires_complete_mir: true,
                supports_native_compile: true,
            }
        }

        fn emit_ir(&self, _program: &LoweredProgram, _opts: BackendOptions) -> String {
            String::new()
        }

        fn compile_ir(
            &self,
            _program: &LoweredProgram,
            _out_bin: &std::path::Path,
            _options: &BackendBuildOptions,
            _opts: BackendOptions,
        ) -> Result<Artifact, CodegenError> {
            self.compile_calls.set(self.compile_calls.get() + 1);
            Err(CodegenError::Compile("backend failed".into()))
        }
    }

    #[test]
    fn checks_once_before_propagating_backend_error() {
        let compile_calls = Rc::new(Cell::new(0));
        let driver = Driver::new(FailingBackend {
            compile_calls: Rc::clone(&compile_calls),
        });

        let error = driver
            .build(
                &empty_file(),
                std::path::Path::new("out"),
                CompileOptions::default(),
                EmitOptions::default(),
            )
            .expect_err("backend error should propagate");

        assert!(matches!(error, CodegenError::Compile(message) if message == "backend failed"));
        assert_eq!(compile_calls.get(), 1);
    }

    #[test]
    fn invalid_options_fail_before_backend_compile() {
        let compile_calls = Rc::new(Cell::new(0));
        let driver = Driver::new(FailingBackend {
            compile_calls: Rc::clone(&compile_calls),
        });
        let options = CompileOptions {
            runtime_abi: None,
            ..CompileOptions::default()
        };

        let error = driver
            .build(
                &empty_file(),
                std::path::Path::new("out"),
                options,
                EmitOptions::default(),
            )
            .expect_err("invalid options should fail before compilation");

        assert!(matches!(error, CodegenError::Configuration(_)));
        assert_eq!(compile_calls.get(), 0);

        // Keep the invalid case explicit: this is the same validation that
        // protects CBackend from invoking CC.
        assert!(CompileOptions {
            runtime_abi: None,
            ..CompileOptions::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn strict_backend_accepts_lowered_async_mir_before_emission() {
        let source =
            parse_file("package demo\nasync fun work(): Int { while (true) { break } return 1 }\n")
                .expect("parse");
        let driver = Driver::new(FailingBackend {
            compile_calls: Rc::new(Cell::new(0)),
        });
        let output = driver
            .emit(&source, EmitOptions::default())
            .expect("lowered async MIR should reach the strict backend");
        assert!(output.is_empty());
    }

    #[test]
    fn mir_backend_emits_without_c_source_or_runtime() {
        let source = parse_file(
            "package demo\nfun touch() { }\nfun spin(flag: Bool) { while (flag) { touch() } }\n",
        )
        .expect("parse");
        let output = Driver::new(MirBackend)
            .emit(&source, EmitOptions::default())
            .expect("MIR backend should emit");
        assert!(output.starts_with("aura-mir version=1\npackage demo\n"));
        assert!(output.contains("function spin"));
        assert!(output.contains("terminator=switch"));
        assert!(!output.contains("#include"));
        assert!(!output.contains("aura_rt"));
    }

    #[test]
    fn mir_backend_output_is_deterministic_for_repeated_lowering() {
        let source = parse_file(
            "package demo\nfun touch() { }\nfun spin(flag: Bool) { while (flag) { touch() } }\n",
        )
        .expect("parse");
        let driver = Driver::new(MirBackend);
        let first = driver
            .emit(&source, EmitOptions::default())
            .expect("first MIR emission");
        let second = driver
            .emit(&source, EmitOptions::default())
            .expect("second MIR emission");
        assert_eq!(first, second);
    }

    #[test]
    fn mir_backend_publishes_open_generic_async_ir_without_c_inputs() {
        let source =
            parse_file("package demo\nasync fun identity<T>(value: T): T { return value }\n")
                .expect("parse");
        let output = Driver::new(MirBackend)
            .emit(&source, EmitOptions::default())
            .expect("open generic MIR emission");
        assert!(output.contains("async-body identity"));
        assert!(output.contains("state-machine identity"));
        assert!(!output.contains("#include"));
        assert!(!output.contains("aura_type_erased"));
    }

    #[test]
    fn backend_capabilities_separate_mir_and_alpha_c_requirements() {
        let mir = MirBackend.capabilities();
        assert!(mir.requires_complete_mir);
        assert!(!mir.supports_native_compile);

        let c = super::CBackend.capabilities();
        assert!(!c.requires_complete_mir);
        assert!(!c.supports_native_compile);
    }

    #[test]
    fn mir_backend_build_does_not_validate_c_runtime_or_compiler() {
        let error = Driver::new(MirBackend)
            .build(
                &empty_file(),
                Path::new("out"),
                CompileOptions::default(),
                EmitOptions::default(),
            )
            .expect_err("MIR-only backend has no native artifact path");
        assert!(matches!(
            error,
                CodegenError::Configuration(message)
                if message.contains("does not provide neutral native artifact")
        ));
    }

    #[test]
    fn native_backend_artifact_identity_needs_no_runtime_abi() {
        let options = BackendBuildOptions {
            backend: crate::options::Backend::Llvm,
            target: crate::options::Target::Native,
            profile: crate::options::Profile::Release,
            optimization: crate::options::OptimizationLevel::O2,
            debug: false,
            lto: crate::options::Lto::Thin,
            panic: crate::options::PanicStrategy::Abort,
            output: crate::options::OutputKind::Executable,
            features: vec!["mir".into()],
        };
        let artifact = Artifact::from_backend(PathBuf::from("aura.llvm"), &options);
        assert_eq!(artifact.identity().backend, crate::options::Backend::Llvm);
        assert_eq!(artifact.identity().runtime_abi, None);
        assert_eq!(artifact.identity().features, vec!["mir"]);
    }

    #[test]
    fn identity_has_deterministic_equality_and_display() {
        let options = CompileOptions::builder()
            .backend(crate::options::Backend::C)
            .target(crate::options::Target::Native)
            .profile(crate::options::Profile::Debug)
            .runtime_abi(crate::options::RuntimeAbi::AuraRtC)
            .output(crate::options::OutputKind::Executable)
            .diagnostics(crate::options::DiagnosticMode::Human)
            .feature("zeta")
            .feature("alpha")
            .build()
            .expect("complete options");

        let first = BuildIdentity::from(&options);
        let second = BuildIdentity::from(&options);

        assert_eq!(first, second);
        assert_eq!(first.features, vec!["alpha", "zeta"]);
        assert_eq!(first.runtime_abi_version, Some(1));
        assert_eq!(first.runtime_abi_identity, Some(crate::runtime_abi::ID));
        assert_eq!(
            first.to_string(),
            "backend=C, target=Native, profile=Debug, settings=ProfileSettings { optimization: O0, debug: true, lto: Off, detector: true, panic: Unwind, backend: C, linker: None }, runtime_abi=Some(AuraRtC)/Some(1)/Some(\"aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1;type=1\"), output=Executable, features=[alpha,zeta], native_sources=[]"
        );
    }

    struct IdentityBackend;

    impl Backend for IdentityBackend {
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities {
                requires_complete_mir: true,
                supports_native_compile: true,
            }
        }

        fn emit_ir(&self, _program: &LoweredProgram, _opts: BackendOptions) -> String {
            String::new()
        }

        fn compile_ir(
            &self,
            _program: &LoweredProgram,
            out_bin: &std::path::Path,
            options: &BackendBuildOptions,
            _opts: BackendOptions,
        ) -> Result<Artifact, CodegenError> {
            Ok(Artifact::from_backend(out_bin.to_path_buf(), options))
        }
    }

    #[test]
    fn repeated_driver_builds_have_equal_identity() {
        let options = CompileOptions::builder()
            .backend(crate::options::Backend::C)
            .target(crate::options::Target::Native)
            .profile(crate::options::Profile::Debug)
            .runtime_abi(crate::options::RuntimeAbi::AuraRtC)
            .output(crate::options::OutputKind::Executable)
            .diagnostics(crate::options::DiagnosticMode::Human)
            .feature("stable")
            .feature("portable")
            .build()
            .expect("complete options");
        let file = empty_file();
        let first = Driver::new(IdentityBackend)
            .build(
                &file,
                Path::new("first.out"),
                options.clone(),
                EmitOptions::default(),
            )
            .expect("first build");
        let second = Driver::new(IdentityBackend)
            .build(
                &file,
                Path::new("second.out"),
                options,
                EmitOptions::default(),
            )
            .expect("second build");

        assert_eq!(first.identity(), second.identity());
        assert_eq!(first.identity().features, vec!["portable", "stable"]);
    }
}
