//! Backend-neutral compilation pipeline.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aura_ast::File;
use aura_sema::{check_file, CheckedFile};

use crate::ctx::EmitOptions;
use crate::emit::emit_c_with;
use crate::error::CodegenError;
use crate::options::{
    Backend as BackendKind, CompileOptions, Lto, OutputKind, Profile, ProfileSettings, RuntimeAbi,
    Target,
};
use crate::validation::{compiler_command, validate_build};

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
        }
    }
}

impl std::fmt::Display for BuildIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let features = self.features.join(",");
        write!(
            f,
            "backend={:?}, target={:?}, profile={:?}, settings={:?}, runtime_abi={:?}/{:?}/{:?}, output={:?}, features=[{}]",
            self.backend,
            self.target,
            self.profile,
            self.profile_settings,
            self.runtime_abi,
            self.runtime_abi_version,
            self.runtime_abi_identity,
            self.output,
            features
        )
    }
}

/// The result reported by a backend after producing an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    path: PathBuf,
    identity: BuildIdentity,
}

impl Artifact {
    fn new(path: PathBuf, options: &CompileOptions) -> Self {
        Self {
            path,
            identity: BuildIdentity::from(options),
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
pub(crate) trait Backend {
    fn emit(&self, checked: &CheckedFile, opts: EmitOptions) -> String;

    fn compile(
        &self,
        checked: &CheckedFile,
        out_bin: &Path,
        runtime_c: &Path,
        options: &CompileOptions,
        opts: EmitOptions,
    ) -> Result<Artifact, CodegenError>;
}

/// Runs frontend/sema once, then delegates emission or compilation to a backend.
pub(crate) struct Driver<B> {
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
    Driver::new(CBackend).build(file, out_bin, runtime_c, options, opts)
}

impl<B: Backend> Driver<B> {
    pub(crate) fn new(backend: B) -> Self {
        Self { backend }
    }

    pub(crate) fn emit(&self, file: &File, opts: EmitOptions) -> Result<String, CodegenError> {
        let checked = check_file(file)?;
        Ok(self.backend.emit(&checked, opts))
    }

    pub(crate) fn build(
        &self,
        file: &File,
        out_bin: &Path,
        runtime_c: &Path,
        options: CompileOptions,
        opts: EmitOptions,
    ) -> Result<Artifact, CodegenError> {
        validate_build(&options, &compiler_command(), runtime_c)?;
        let checked = check_file(file)?;
        self.backend
            .compile(&checked, out_bin, runtime_c, &options, opts)
    }
}

pub(crate) struct CBackend;

impl Backend for CBackend {
    fn emit(&self, checked: &CheckedFile, opts: EmitOptions) -> String {
        emit_c_with(checked, opts)
    }

    fn compile(
        &self,
        checked: &CheckedFile,
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
        let c_src = self.emit(checked, emit_opts);
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
        fs::write(&c_path, c_src).map_err(|e| CodegenError::Io(e.to_string()))?;

        let mut command = Command::new(&compiler);
        command
            .arg(format!("-{}", options.profile_settings.optimization.flag()))
            .arg("-std=c11");
        if options.profile_settings.debug {
            command.arg("-g");
        }
        if options.profile_settings.lto != Lto::Off {
            command.arg("-flto");
        }
        if options.profile_settings.detector {
            command.arg("-fsanitize=address,undefined");
        }
        if let Some(linker) = &options.profile_settings.linker {
            command.arg(format!("-fuse-ld={linker}"));
        }
        for path in &options.foreign_library_paths {
            command.arg("-L").arg(path);
        }
        let mut foreign_link_args = Vec::new();
        for foreign in &checked.ast.foreign_functions {
            let (Some(link), Some(library)) = (&foreign.link, &foreign.library) else {
                continue;
            };
            match link.kind {
                aura_ast::ForeignLinkKind::Dynamic => {
                    foreign_link_args.push(format!("-l{}", library.name));
                }
                aura_ast::ForeignLinkKind::Static if cfg!(target_os = "linux") => {
                    foreign_link_args.push("-Wl,-Bstatic".into());
                    foreign_link_args.push(format!("-l{}", library.name));
                    foreign_link_args.push("-Wl,-Bdynamic".into());
                }
                aura_ast::ForeignLinkKind::Static if cfg!(target_os = "macos") => {
                    let archive_name = format!("lib{}.a", library.name);
                    let archive = options
                        .foreign_library_paths
                        .iter()
                        .map(|path| path.join(&archive_name))
                        .find(|path| path.is_file())
                        .unwrap_or_else(|| PathBuf::from(&archive_name));
                    foreign_link_args
                        .push(format!("-Wl,-force_load,{}", archive.to_string_lossy()));
                }
                aura_ast::ForeignLinkKind::Static => {}
            }
        }
        let status = command
            .arg(&c_path)
            .arg(runtime_c)
            .args(&foreign_link_args)
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

        Ok(Artifact::new(out_bin.to_path_buf(), options))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::Path;
    use std::rc::Rc;

    use aura_ast::{File, Ident, Path as AstPath, Span};

    use super::{Artifact, Backend, BuildIdentity, Driver};
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

    fn runtime_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/aura_rt.c")
    }

    struct FailingBackend {
        compile_calls: Rc<Cell<usize>>,
    }

    impl Backend for FailingBackend {
        fn emit(&self, _checked: &aura_sema::CheckedFile, _opts: EmitOptions) -> String {
            String::new()
        }

        fn compile(
            &self,
            _checked: &aura_sema::CheckedFile,
            _out_bin: &std::path::Path,
            _runtime_c: &std::path::Path,
            _options: &CompileOptions,
            _opts: EmitOptions,
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
                &runtime_path(),
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
                &runtime_path(),
                options,
                EmitOptions::default(),
            )
            .expect_err("invalid options should fail before compilation");

        assert!(
            matches!(error, CodegenError::Configuration(message) if message.contains("AuraRtC"))
        );
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
            "backend=C, target=Native, profile=Debug, settings=ProfileSettings { optimization: O0, debug: true, lto: Off, detector: true, panic: Unwind, backend: C, linker: None }, runtime_abi=Some(AuraRtC)/Some(1)/Some(\"aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1\"), output=Executable, features=[alpha,zeta]"
        );
    }

    struct IdentityBackend;

    impl Backend for IdentityBackend {
        fn emit(&self, _checked: &aura_sema::CheckedFile, _opts: EmitOptions) -> String {
            String::new()
        }

        fn compile(
            &self,
            _checked: &aura_sema::CheckedFile,
            out_bin: &std::path::Path,
            _runtime_c: &std::path::Path,
            options: &CompileOptions,
            _opts: EmitOptions,
        ) -> Result<Artifact, CodegenError> {
            Ok(Artifact::new(out_bin.to_path_buf(), options))
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
        let runtime = runtime_path();

        let first = Driver::new(IdentityBackend)
            .build(
                &file,
                Path::new("first.out"),
                &runtime,
                options.clone(),
                EmitOptions::default(),
            )
            .expect("first build");
        let second = Driver::new(IdentityBackend)
            .build(
                &file,
                Path::new("second.out"),
                &runtime,
                options,
                EmitOptions::default(),
            )
            .expect("second build");

        assert_eq!(first.identity(), second.identity());
        assert_eq!(first.identity().features, vec!["portable", "stable"]);
    }
}
