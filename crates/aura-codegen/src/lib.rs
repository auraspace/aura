use anyhow::Result;
use aura_mir::MirProgram;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use target_lexicon::{Architecture, BinaryFormat, Environment, OperatingSystem, Triple};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFormat {
    MachO,
    Elf,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetSupportStatus {
    Supported,
    PlaceholderOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetDescriptor {
    pub triple: String,
    pub object_format: ObjectFormat,
    pub support_status: TargetSupportStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Llvm,
    Clif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub supports_emit_llvm: bool,
    pub supports_emit_asm: bool,
}

impl BackendKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "llvm" => Ok(Self::Llvm),
            "clif" => Ok(Self::Clif),
            other => anyhow::bail!("unknown backend `{other}` (expected `llvm` or `clif`)"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Llvm => "llvm",
            Self::Clif => "clif",
        }
    }

    pub fn capabilities(self) -> BackendCapabilities {
        match self {
            Self::Llvm => BackendCapabilities {
                supports_emit_llvm: true,
                supports_emit_asm: true,
            },
            Self::Clif => BackendCapabilities {
                supports_emit_llvm: false,
                supports_emit_asm: false,
            },
        }
    }
}

pub trait Backend {
    /// Compiles the MIR program into an object file (`.o`).
    /// Returns the path to the produced object file.
    ///
    /// The MVP backend is LLVM; Cranelift remains a placeholder backend crate.
    fn compile(&self, program: &MirProgram, out_dir: &Path) -> Result<PathBuf>;

    /// Optinally emit intermediate representations (IR, Assembly, etc.).
    fn emit_llvm(&self, program: &MirProgram, out_path: &Path) -> Result<()>;
    fn emit_asm(&self, program: &MirProgram, out_path: &Path) -> Result<()>;
}

#[derive(Debug)]
pub struct Target {
    descriptor: TargetDescriptor,
}

impl Target {
    pub const AARCH64_APPLE_DARWIN: &'static str = "aarch64-apple-darwin";
    pub const X86_64_UNKNOWN_LINUX_GNU: &'static str = "x86_64-unknown-linux-gnu";

    pub fn new(triple: impl Into<String>) -> Self {
        Self::from_descriptor(TargetDescriptor {
            triple: triple.into(),
            object_format: ObjectFormat::Unknown,
            support_status: TargetSupportStatus::Supported,
        })
    }

    pub fn host() -> Self {
        Self::from_parsed_triple(target_lexicon::HOST.clone())
    }

    pub fn aarch64_apple_darwin() -> Self {
        Self::from_descriptor(TargetDescriptor {
            triple: Self::AARCH64_APPLE_DARWIN.to_string(),
            object_format: ObjectFormat::MachO,
            support_status: TargetSupportStatus::Supported,
        })
    }

    pub fn x86_64_unknown_linux_gnu() -> Self {
        Self::from_descriptor(TargetDescriptor {
            triple: Self::X86_64_UNKNOWN_LINUX_GNU.to_string(),
            object_format: ObjectFormat::Elf,
            support_status: TargetSupportStatus::PlaceholderOnly,
        })
    }

    pub fn descriptor(&self) -> &TargetDescriptor {
        &self.descriptor
    }

    pub fn triple(&self) -> &str {
        &self.descriptor.triple
    }

    pub fn linker_triple(&self) -> &str {
        self.triple()
    }

    pub fn object_format(&self) -> ObjectFormat {
        self.descriptor.object_format
    }

    pub fn support_status(&self) -> TargetSupportStatus {
        self.descriptor.support_status
    }

    pub fn is_placeholder_only(&self) -> bool {
        self.support_status() == TargetSupportStatus::PlaceholderOnly
    }

    pub fn ensure_codegen_supported(&self) -> Result<()> {
        match self.support_status() {
            TargetSupportStatus::Supported if self.object_format() != ObjectFormat::Unknown => {
                Ok(())
            }
            TargetSupportStatus::Supported => {
                anyhow::bail!(
                    "target `{}` has an unknown object format and cannot be used for code generation yet",
                    self.triple()
                )
            }
            TargetSupportStatus::PlaceholderOnly => {
                anyhow::bail!(
                    "target `{}` is placeholder-only and cannot be used for code generation yet",
                    self.triple()
                )
            }
        }
    }

    pub fn parse(triple: &str) -> Result<Self> {
        let parsed = Triple::from_str(triple)
            .map_err(|err| anyhow::anyhow!("invalid target triple `{triple}`: {err}"))?;
        Ok(Self::from_parsed_triple(parsed))
    }

    pub fn resolve(triple: &str) -> Result<Self> {
        let target = Self::parse(triple)?;
        target.ensure_codegen_supported()?;
        Ok(target)
    }

    pub fn ensure_linking_supported(&self) -> Result<()> {
        match self.support_status() {
            TargetSupportStatus::Supported if self.object_format() != ObjectFormat::Unknown => {
                Ok(())
            }
            TargetSupportStatus::Supported => {
                anyhow::bail!(
                    "target `{}` has an unknown object format and cannot be linked yet",
                    self.triple()
                )
            }
            TargetSupportStatus::PlaceholderOnly => {
                anyhow::bail!(
                    "target `{}` is placeholder-only and cannot be linked yet",
                    self.triple()
                )
            }
        }
    }

    fn from_descriptor(descriptor: TargetDescriptor) -> Self {
        Self { descriptor }
    }

    fn from_parsed_triple(triple: Triple) -> Self {
        let triple_string = triple.to_string();

        match (
            triple.architecture,
            triple.operating_system,
            triple.environment,
            triple.binary_format,
        ) {
            (Architecture::Aarch64(_), OperatingSystem::Darwin, _, BinaryFormat::Macho) => {
                Self::aarch64_apple_darwin()
            }
            (Architecture::X86_64, OperatingSystem::Linux, Environment::Gnu, BinaryFormat::Elf) => {
                Self::x86_64_unknown_linux_gnu()
            }
            (_, _, _, binary_format) => Self::from_descriptor(TargetDescriptor {
                triple: triple_string,
                object_format: match binary_format {
                    BinaryFormat::Macho => ObjectFormat::MachO,
                    BinaryFormat::Elf => ObjectFormat::Elf,
                    _ => ObjectFormat::Unknown,
                },
                support_status: TargetSupportStatus::Supported,
            }),
        }
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::host()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backend_kind_and_reports_name() {
        assert_eq!(BackendKind::parse("llvm").unwrap(), BackendKind::Llvm);
        assert_eq!(BackendKind::parse("clif").unwrap(), BackendKind::Clif);
        assert!(BackendKind::parse("something-else").is_err());
        assert_eq!(BackendKind::Llvm.name(), "llvm");
        assert_eq!(BackendKind::Clif.name(), "clif");
    }

    #[test]
    fn target_helpers_return_expected_triples() {
        let aarch64 = Target::aarch64_apple_darwin();
        assert_eq!(aarch64.triple(), Target::AARCH64_APPLE_DARWIN);
        assert_eq!(aarch64.linker_triple(), Target::AARCH64_APPLE_DARWIN);
        assert_eq!(aarch64.object_format(), ObjectFormat::MachO);
        assert_eq!(aarch64.support_status(), TargetSupportStatus::Supported);

        let linux = Target::x86_64_unknown_linux_gnu();
        assert_eq!(linux.triple(), Target::X86_64_UNKNOWN_LINUX_GNU);
        assert_eq!(linux.linker_triple(), Target::X86_64_UNKNOWN_LINUX_GNU);
        assert_eq!(linux.object_format(), ObjectFormat::Elf);
        assert_eq!(linux.support_status(), TargetSupportStatus::PlaceholderOnly);
        assert!(linux
            .ensure_codegen_supported()
            .expect_err("placeholder target should fail codegen")
            .to_string()
            .contains("placeholder-only"));
        assert!(linux
            .ensure_linking_supported()
            .expect_err("placeholder target should fail linking")
            .to_string()
            .contains("placeholder-only"));

        let custom = Target::new("custom-triple");
        assert_eq!(custom.triple(), "custom-triple");
        assert_eq!(custom.linker_triple(), "custom-triple");
        assert_eq!(custom.object_format(), ObjectFormat::Unknown);
        assert_eq!(custom.support_status(), TargetSupportStatus::Supported);
        assert!(custom.ensure_codegen_supported().is_err());
        assert!(custom.ensure_linking_supported().is_err());
    }

    #[test]
    fn parses_known_triples_and_rejects_invalid_ones() {
        let parsed = Target::parse("aarch64-apple-darwin").unwrap();
        assert_eq!(parsed.triple(), Target::AARCH64_APPLE_DARWIN);
        assert_eq!(parsed.object_format(), ObjectFormat::MachO);

        let parsed_placeholder = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(
            parsed_placeholder.triple(),
            Target::X86_64_UNKNOWN_LINUX_GNU
        );
        assert_eq!(
            parsed_placeholder.support_status(),
            TargetSupportStatus::PlaceholderOnly
        );

        assert!(Target::parse("not-a-real-triple").is_err());
    }

    #[test]
    fn resolves_supported_targets_through_one_api() {
        let resolved = Target::resolve("aarch64-apple-darwin").unwrap();
        assert_eq!(resolved.triple(), Target::AARCH64_APPLE_DARWIN);
        assert_eq!(resolved.object_format(), ObjectFormat::MachO);
        assert_eq!(resolved.support_status(), TargetSupportStatus::Supported);

        let err = Target::resolve("x86_64-unknown-linux-gnu").unwrap_err();
        assert!(err.to_string().contains("placeholder-only"));
    }
}
