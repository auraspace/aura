//! Deterministic compiler-host extension points for user derives.
//!
//! The callback API is intentionally AST-only. It is useful for compiler
//! integrations and tests without granting arbitrary source-process access;
//! RFC-010's out-of-process procedural macro ABI remains a separate boundary.

use aura_ast::{ClassDecl, File, FunDecl, Span};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const MACRO_PLUGIN_ABI_VERSION: u32 = 1;
const MACRO_PLUGIN_MAGIC: &[u8] = b"AURA-MACRO\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroError {
    pub message: String,
    pub span: Span,
}

/// A compiler-hosted derive implementation.
///
/// Implementations must return synthetic methods whose `span` is the derive
/// invocation span. This preserves expansion-origin metadata and diagnostics.
pub trait UserDerive {
    fn name(&self) -> &str;
    fn expand(&self, input: &ClassDecl) -> Result<Vec<FunDecl>, MacroError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroExpansion {
    pub macro_name: String,
    pub generated_item: String,
    pub invocation_span: Span,
    pub generated_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroPluginRequest {
    pub macro_name: String,
    pub package: String,
    pub source: String,
    pub invocation_span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroPluginResponse {
    Expanded { source: String },
    Failed { message: String, span: Span },
}

#[derive(Debug, Clone)]
pub struct MacroSandboxConfig {
    pub plugin: PathBuf,
    pub source_root: PathBuf,
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl Default for MacroSandboxConfig {
    fn default() -> Self {
        Self {
            plugin: PathBuf::new(),
            source_root: PathBuf::new(),
            timeout: Duration::from_secs(5),
            max_output_bytes: 4 * 1024 * 1024,
        }
    }
}

/// Invoke a procedural macro through an OS sandbox, using a versioned binary
/// protocol. Unsupported hosts fail closed instead of silently running a
/// plugin in-process or with ambient network/filesystem access.
pub fn run_sandboxed_macro(
    config: &MacroSandboxConfig,
    request: &MacroPluginRequest,
) -> Result<MacroPluginResponse, MacroError> {
    if config.plugin.as_os_str().is_empty() || !config.plugin.is_file() {
        return Err(MacroError {
            message: "macro plugin executable does not exist".into(),
            span: request.invocation_span,
        });
    }
    if !config.source_root.is_dir() {
        return Err(MacroError {
            message: "macro sandbox source root does not exist".into(),
            span: request.invocation_span,
        });
    }
    let mut command = sandbox_command(config).map_err(|message| MacroError {
        message,
        span: request.invocation_span,
    })?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .env(
            "AURA_MACRO_PLUGIN_ABI",
            MACRO_PLUGIN_ABI_VERSION.to_string(),
        );
    let mut child = command.spawn().map_err(|error| MacroError {
        message: format!("failed to start macro plugin: {error}"),
        span: request.invocation_span,
    })?;
    let payload = encode_request(request);
    let mut stdin = child.stdin.take().ok_or_else(|| MacroError {
        message: "macro plugin stdin was not available".into(),
        span: request.invocation_span,
    })?;
    let writer = std::thread::spawn(move || {
        let result = stdin.write_all(&payload);
        drop(stdin);
        result
    });
    let stdout = child.stdout.take().ok_or_else(|| MacroError {
        message: "macro plugin stdout was not available".into(),
        span: request.invocation_span,
    })?;
    let stderr = child.stderr.take().ok_or_else(|| MacroError {
        message: "macro plugin stderr was not available".into(),
        span: request.invocation_span,
    })?;
    let max_output = config.max_output_bytes;
    let stdout_reader = std::thread::spawn(move || read_capped(stdout, max_output));
    let stderr_reader = std::thread::spawn(move || read_capped(stderr, 64 * 1024));

    let deadline = Instant::now() + config.timeout;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| MacroError {
            message: format!("failed to poll macro plugin: {error}"),
            span: request.invocation_span,
        })? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = writer.join();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(MacroError {
                message: "macro plugin exceeded its sandbox timeout".into(),
                span: request.invocation_span,
            });
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let write_result = writer.join().map_err(|_| MacroError {
        message: "macro plugin request writer panicked".into(),
        span: request.invocation_span,
    })?;
    if let Err(error) = write_result {
        return Err(MacroError {
            message: format!("failed to send macro request: {error}"),
            span: request.invocation_span,
        });
    }
    let stdout = stdout_reader
        .join()
        .map_err(|_| MacroError {
            message: "macro plugin stdout reader panicked".into(),
            span: request.invocation_span,
        })?
        .map_err(|error| MacroError {
            message: format!("failed to collect macro plugin output: {error}"),
            span: request.invocation_span,
        })?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| MacroError {
            message: "macro plugin stderr reader panicked".into(),
            span: request.invocation_span,
        })?
        .map_err(|error| MacroError {
            message: format!("failed to collect macro plugin diagnostics: {error}"),
            span: request.invocation_span,
        })?;
    if stdout.len() > config.max_output_bytes {
        return Err(MacroError {
            message: "macro plugin response exceeded the output limit".into(),
            span: request.invocation_span,
        });
    }
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        return Err(MacroError {
            message: format!("macro plugin failed: {}", detail.trim()),
            span: request.invocation_span,
        });
    }
    decode_response(&stdout).map_err(|message| MacroError {
        message,
        span: request.invocation_span,
    })
}

/// Drain a plugin pipe even after the configured cap is reached. Keeping the
/// reader alive prevents a malicious plugin from blocking forever on a full
/// stdout/stderr pipe while the host waits for process termination.
fn read_capped<R: Read>(mut reader: R, cap: usize) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(cap.saturating_add(1).min(64 * 1024));
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = cap.saturating_add(1).saturating_sub(output.len());
        if remaining != 0 {
            output.extend_from_slice(&buffer[..count.min(remaining)]);
        }
    }
    Ok(output)
}

fn sandbox_command(config: &MacroSandboxConfig) -> Result<Command, String> {
    let plugin = canonical(config.plugin.as_path())?;
    let source_root = canonical(config.source_root.as_path())?;
    #[cfg(target_os = "macos")]
    {
        let profile = format!(
            "(version 1) (deny default) (allow process-exec) (allow sysctl-read) (allow file-read* (subpath \"{}\") (subpath \"{}\") (subpath \"/usr/bin\") (subpath \"/usr/lib\") (subpath \"/System/Library\")) (allow file-write* (subpath \"/tmp\"))",
            profile_path(&source_root),
            profile_path(plugin.parent().unwrap_or(Path::new("/")))
        );
        let mut command = Command::new("sandbox-exec");
        command.arg("-p").arg(profile).arg(plugin);
        return Ok(command);
    }
    #[cfg(target_os = "linux")]
    {
        let _ = (plugin, source_root);
        return Err("macro sandbox requires bubblewrap on Linux".into());
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (plugin, source_root);
        Err("macro sandbox is unsupported on this host".into())
    }
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|error| format!("cannot resolve sandbox path {}: {error}", path.display()))
}

fn profile_path(path: &Path) -> String {
    path.to_string_lossy().replace('"', "\\\"")
}

fn encode_request(request: &MacroPluginRequest) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MACRO_PLUGIN_MAGIC);
    out.extend_from_slice(&MACRO_PLUGIN_ABI_VERSION.to_le_bytes());
    write_string(&mut out, &request.macro_name);
    write_string(&mut out, &request.package);
    write_string(&mut out, &request.source);
    out.extend_from_slice(&request.invocation_span.start.to_le_bytes());
    out.extend_from_slice(&request.invocation_span.end.to_le_bytes());
    out
}

/// Decode the host request from the stable plugin wire format.
pub fn decode_plugin_request(bytes: &[u8]) -> Result<MacroPluginRequest, String> {
    let mut cursor = 0;
    if bytes.get(..MACRO_PLUGIN_MAGIC.len()) != Some(MACRO_PLUGIN_MAGIC) {
        return Err("invalid macro plugin request magic".into());
    }
    cursor += MACRO_PLUGIN_MAGIC.len();
    if read_u32(bytes, &mut cursor)? != MACRO_PLUGIN_ABI_VERSION {
        return Err("unsupported macro plugin ABI version".into());
    }
    let macro_name = read_string(bytes, &mut cursor)?;
    let package = read_string(bytes, &mut cursor)?;
    let source = read_string(bytes, &mut cursor)?;
    let start = read_u32(bytes, &mut cursor)?;
    let end = read_u32(bytes, &mut cursor)?;
    if cursor != bytes.len() {
        return Err("trailing bytes in macro plugin request".into());
    }
    Ok(MacroPluginRequest {
        macro_name,
        package,
        source,
        invocation_span: Span { start, end },
    })
}

/// Encode the plugin response for stdout. Plugin implementations should write
/// exactly this frame and no human-readable bytes to stdout.
pub fn encode_plugin_response(response: &MacroPluginResponse) -> Vec<u8> {
    let (status, text, span) = match response {
        MacroPluginResponse::Expanded { source } => (0u8, source.as_str(), Span::new(0, 0)),
        MacroPluginResponse::Failed { message, span } => (1u8, message.as_str(), *span),
    };
    let mut out = Vec::new();
    out.extend_from_slice(MACRO_PLUGIN_MAGIC);
    out.extend_from_slice(&MACRO_PLUGIN_ABI_VERSION.to_le_bytes());
    out.push(status);
    write_string(&mut out, text);
    out.extend_from_slice(&span.start.to_le_bytes());
    out.extend_from_slice(&span.end.to_le_bytes());
    out
}

fn decode_response(bytes: &[u8]) -> Result<MacroPluginResponse, String> {
    let mut cursor = 0;
    if bytes.get(..MACRO_PLUGIN_MAGIC.len()) != Some(MACRO_PLUGIN_MAGIC) {
        return Err("invalid macro plugin response magic".into());
    }
    cursor += MACRO_PLUGIN_MAGIC.len();
    if read_u32(bytes, &mut cursor)? != MACRO_PLUGIN_ABI_VERSION {
        return Err("unsupported macro plugin ABI version".into());
    }
    let status = *bytes
        .get(cursor)
        .ok_or_else(|| "truncated macro plugin response".to_string())?;
    cursor += 1;
    let text = read_string(bytes, &mut cursor)?;
    let start = read_u32(bytes, &mut cursor)?;
    let end = read_u32(bytes, &mut cursor)?;
    match status {
        0 => Ok(MacroPluginResponse::Expanded { source: text }),
        1 => Ok(MacroPluginResponse::Failed {
            message: text,
            span: Span { start, end },
        }),
        _ => Err("unknown macro plugin response status".into()),
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let end = cursor.saturating_add(4);
    let raw = bytes
        .get(*cursor..end)
        .ok_or_else(|| "truncated macro plugin response".to_string())?;
    *cursor = end;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_string(bytes: &[u8], cursor: &mut usize) -> Result<String, String> {
    let length = read_u32(bytes, cursor)? as usize;
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| "macro plugin response length overflow".to_string())?;
    let raw = bytes
        .get(*cursor..end)
        .ok_or_else(|| "truncated macro plugin response string".to_string())?;
    *cursor = end;
    String::from_utf8(raw.to_vec()).map_err(|_| "macro plugin response is not UTF-8".into())
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    #[test]
    fn plugin_protocol_round_trips_request_and_response() {
        let request = MacroPluginRequest {
            macro_name: "deriveThing".into(),
            package: "demo".into(),
            source: "class Value() {}".into(),
            invocation_span: Span::new(4, 12),
        };
        let decoded = decode_plugin_request(&encode_request(&request)).expect("decode request");
        assert_eq!(decoded, request);

        let response = MacroPluginResponse::Failed {
            message: "bad input".into(),
            span: Span::new(10, 15),
        };
        let decoded = decode_response(&encode_plugin_response(&response)).expect("decode response");
        assert_eq!(decoded, response);
    }

    #[test]
    fn plugin_protocol_rejects_wrong_version() {
        let mut bytes = encode_plugin_response(&MacroPluginResponse::Expanded {
            source: "ok".into(),
        });
        let version_start = MACRO_PLUGIN_MAGIC.len();
        bytes[version_start] = 99;
        assert!(decode_response(&bytes)
            .expect_err("wrong ABI version must fail")
            .contains("version"));
    }
}

/// A deterministic AST macro hook executed before derive expansion.
///
/// This is the compiler-host half of the macro boundary. Package-level token
/// parsing and sandboxed process execution remain outside this trait.
pub trait UserMacro {
    fn name(&self) -> &str;
    fn expand(&self, file: &mut File) -> Result<Vec<MacroExpansion>, MacroError>;
}
