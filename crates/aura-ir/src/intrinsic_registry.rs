//! Shared source-level intrinsic to runtime-ABI registry.
//!
//! Backends should dispatch on these stable identities instead of embedding
//! `std.*` package names and runtime symbol names in separate branches.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intrinsic {
    SyncLazy,
    TaskScope,
    TaskSpawnBlocking,
    TaskSelect,
    TaskCancellation,
    TaskErrorMetadata,
    Time,
    Encoding,
    Json,
    CryptoRandomBytes,
    Compress,
    IoOpenFile,
    Url,
    Mime,
    Os,
    Signal,
    Bytes,
    Dns,
    Error,
    Log,
    Assert,
    Test,
    Io,
    Crypto,
    Tls,
    Reflect,
    Websocket,
    HttpAccessor,
    HttpServe,
    Udp,
    Net,
    IoFd,
    Fs,
}

/// Version of the source-to-runtime intrinsic contract.
pub const ABI_VERSION: u32 = 1;
/// Runtime ABI identity consumed by generated artifacts and the native runtime.
pub const RUNTIME_ABI_VERSION: u32 = 1;
pub const RUNTIME_ABI_ID: &str =
    "aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1;type=1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiSpec {
    pub intrinsic: Intrinsic,
    pub symbol: &'static str,
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeIntrinsic {
    SyncLazy,
    TaskSelect,
    SyncAtomicInt,
    SyncMutex,
    SyncRwLock,
    SyncOnce,
    MetricsCounter,
    HttpRequestBody,
    HttpResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumIntrinsic {
    IoResult,
    IoTaskError,
    ErrorOutcome,
}

pub fn lookup(package: &str, name: &str) -> Option<AbiSpec> {
    let (intrinsic, symbol) = match (package, name) {
        ("std.sync", "lazy" | "get" | "isInitialized") => {
            (Intrinsic::SyncLazy, "aura_llvm_lazy_int")
        }
        ("std.task", "taskScope") => (Intrinsic::TaskScope, "aura_task_scope_begin"),
        ("std.task", "spawnBlocking") => {
            (Intrinsic::TaskSpawnBlocking, "aura_llvm_spawn_blocking_i64")
        }
        ("std.task", "select") => (Intrinsic::TaskSelect, "aura_task_select_new"),
        ("std.task", "cancelAfter" | "linkCancellation" | "isCancelled") => (
            Intrinsic::TaskCancellation,
            "aura_task_frame_set_cancel_deadline",
        ),
        (
            "std.io",
            "taskErrorTypeName" | "taskErrorSourceId" | "taskErrorSpanStart" | "taskErrorSpanEnd",
        ) => (Intrinsic::TaskErrorMetadata, "aura_ex_cause_count"),
        ("std.time", "nowMillis" | "sleep") => (Intrinsic::Time, "aura_time_monotonic_millis"),
        (
            "std.encoding",
            "hexEncode" | "hexDecode" | "base64Encode" | "base64Decode" | "percentEncode"
            | "percentDecode" | "isValidUtf8",
        ) => (Intrinsic::Encoding, "aura_encoding_hex_encode"),
        ("std.bytes", "readFileBytes") => (Intrinsic::Bytes, "aura_read_file_bytes"),
        ("std.bytes", "tryWriteFileBytesAtomic") => {
            (Intrinsic::Bytes, "aura_try_write_file_bytes_atomic")
        }
        (
            "std.json",
            "isValid" | "errorOffset" | "escapeString" | "jsonArrayCount" | "jsonObjectGet"
            | "jsonArrayAt" | "jsonObjectKeys" | "jsonDecodeString" | "jsonDuplicateKey" | "encode"
            | "stringify" | "decode",
        ) => (Intrinsic::Json, "aura_json_object_get"),
        ("std.crypto", "randomBytes") => (Intrinsic::CryptoRandomBytes, "aura_crypto_random_bytes"),
        ("std.compress", "compress" | "decompress") => (Intrinsic::Compress, "aura_compress"),
        ("std.io", "openFile") => (Intrinsic::IoOpenFile, "aura_file_open"),
        (
            "std.io",
            "args" | "readLine" | "readLineResult" | "readAllStdin" | "readAllStdinResult" | "exit"
            | "print" | "println" | "eprint" | "eprintln" | "readFile" | "tryReadFile"
            | "writeFile" | "tryWriteFile" | "readFileResult" | "writeFileResult" | "appendFile"
            | "tryWriteFileAtomic" | "fileExists" | "fileExistsResult" | "fileSize"
            | "fileSizeResult",
        ) => (Intrinsic::Io, "aura_print"),
        (
            "std.crypto",
            "randomBytesBuffer" | "sha256" | "hmacSha256" | "md5Bytes" | "sha256Bytes"
            | "hmacSha256Bytes" | "pbkdf2Sha256" | "constantTimeEquals" | "tlsConfig",
        ) => (Intrinsic::Crypto, "aura_crypto_sha256"),
        (
            "std.crypto",
            "read"
            | "write"
            | "readBytes"
            | "readBytesWithTimeout"
            | "writeBytes"
            | "writeBytesWithTimeout"
            | "connectTls",
        ) => (Intrinsic::Tls, "aura_tls_read"),
        ("std.tls", "wrapStream" | "config" | "loadCertificate" | "connect") => {
            (Intrinsic::Tls, "aura_tls_wrap_stream")
        }
        (
            "std.tls",
            "read"
            | "write"
            | "readBytes"
            | "readBytesWithTimeout"
            | "writeBytes"
            | "writeBytesWithTimeout",
        ) => (Intrinsic::Tls, "aura_tls_read"),
        ("std.tls", "close") | ("std.crypto", "close") => (Intrinsic::Tls, "aura_tls_close"),
        (
            "std.reflect",
            "typeOf" | "typeIdOf" | "typeInfo" | "fields" | "methods" | "fieldMetadata"
            | "methodMetadata" | "isReflectable",
        ) => (Intrinsic::Reflect, "aura_reflect_type_info"),
        (
            "std.url",
            "isOriginForm" | "path" | "normalizePath" | "query" | "isAbsolute" | "authority"
            | "authorityHost" | "authorityPort" | "queryValue",
        ) => (Intrinsic::Url, "aura_url_path"),
        ("std.mime", "isValidType" | "sanitizeFilename" | "dispositionFilename") => {
            (Intrinsic::Mime, "aura_mime_is_valid_type")
        }
        ("std.os", "getEnv" | "setEnv" | "unsetEnv" | "cwd" | "pid" | "platform") => {
            (Intrinsic::Os, "aura_os_get_env")
        }
        ("std.signal", "installShutdown" | "shutdownRequested" | "clearShutdown") => {
            (Intrinsic::Signal, "aura_signal_install_shutdown")
        }
        ("std.bytes", "copy" | "bufferToString" | "concat" | "slice" | "equals") => {
            (Intrinsic::Bytes, "aura_bytes_copy")
        }
        ("std.dns", "resolveHost" | "resolveHostList") => (Intrinsic::Dns, "aura_dns_resolve_host"),
        ("std.error", "kindCode") => (Intrinsic::Error, "aura_error_kind_code"),
        ("std.log", "debug" | "info" | "warn" | "error" | "setMinLevel" | "minLevel") => {
            (Intrinsic::Log, "aura_log")
        }
        ("std.assert", "assert") => (Intrinsic::Assert, "aura_assert"),
        (
            "std.test",
            "assert" | "assertEqInt" | "assertEqString" | "assertEqBool" | "assertEqFloat",
        ) => (Intrinsic::Test, "aura_assert"),
        ("std.websocket", "connect" | "receive" | "send" | "ping" | "close") => {
            (Intrinsic::Websocket, "aura_ws_connect")
        }
        (
            "std.http",
            "requestMethod"
            | "requestTarget"
            | "requestVersion"
            | "requestHeaderCount"
            | "requestHeaderName"
            | "requestHeaderValue"
            | "requestBody"
            | "responseStatus"
            | "responseKeepAlive"
            | "responseSetStatus"
            | "responseSetKeepAlive"
            | "responseSetBody"
            | "responseSetBodyBytes"
            | "responseAddHeader",
        ) => (Intrinsic::HttpAccessor, "aura_http_request_method"),
        ("std.http", "serve" | "serveConnection" | "readChunk" | "getBytes") => {
            (Intrinsic::HttpServe, "aura_llvm_http_serve_task")
        }
        ("std.udp", "bind" | "send" | "receive" | "close") => (Intrinsic::Udp, "aura_udp_bind"),
        (
            "std.net",
            "listen"
            | "connect"
            | "accept"
            | "closeListener"
            | "closeStream"
            | "readStream"
            | "readStreamWithTimeout"
            | "readExact"
            | "readExactly"
            | "readExactlyWithTimeout"
            | "writeStream"
            | "writeStreamWithTimeout"
            | "writeAll"
            | "writeAllWithTimeout",
        ) => (Intrinsic::Net, "aura_llvm_net_connect"),
        ("std.io", "readFd" | "readFdResult" | "writeFd" | "writeFdResult") => {
            (Intrinsic::IoFd, "aura_llvm_io_read_fd_task")
        }
        (
            "std.fs",
            "join" | "basename" | "dirname" | "extension" | "isAbsolute" | "isDirectory"
            | "ensureDirectory" | "fileMode" | "permissions" | "modifiedMillis" | "listNames"
            | "isSymlink",
        ) => (Intrinsic::Fs, "aura_fs_join"),
        _ => return None,
    };
    Some(AbiSpec {
        intrinsic,
        symbol,
        version: ABI_VERSION,
    })
}

pub fn lookup_type(package: &str, name: &str) -> Option<TypeIntrinsic> {
    match (package, name) {
        ("std.sync", "Lazy") => Some(TypeIntrinsic::SyncLazy),
        ("std.task", "Select") => Some(TypeIntrinsic::TaskSelect),
        ("std.sync", "AtomicInt") => Some(TypeIntrinsic::SyncAtomicInt),
        ("std.sync", "Mutex") => Some(TypeIntrinsic::SyncMutex),
        ("std.sync", "RwLock") => Some(TypeIntrinsic::SyncRwLock),
        ("std.sync", "Once") => Some(TypeIntrinsic::SyncOnce),
        ("std.metrics", "Counter") => Some(TypeIntrinsic::MetricsCounter),
        ("std.http", "RequestBody") => Some(TypeIntrinsic::HttpRequestBody),
        ("std.http", "Response") => Some(TypeIntrinsic::HttpResponse),
        _ => None,
    }
}

pub fn lookup_enum(package: &str, name: &str) -> Option<EnumIntrinsic> {
    match (package, name) {
        ("std.io", "Result") => Some(EnumIntrinsic::IoResult),
        ("std.io", "TaskError") => Some(EnumIntrinsic::IoTaskError),
        ("std.error", "Outcome") => Some(EnumIntrinsic::ErrorOutcome),
        _ => None,
    }
}

/// Selects the constructor for the two source-level TLS compatibility types.
/// Keeping this distinction here prevents backend emitters from branching on
/// stdlib package names while preserving their historical layouts.
pub fn tls_connection_constructor(package: &str) -> Option<&'static str> {
    match package {
        "std.tls" => Some("aura_new_std_tls_Connection"),
        "std.crypto" => Some("aura_new_std_crypto_TlsConnection"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        lookup, lookup_enum, lookup_type, tls_connection_constructor, EnumIntrinsic, Intrinsic,
        TypeIntrinsic, ABI_VERSION,
    };

    #[test]
    fn resolves_stdlib_identity_without_backend_package_branches() {
        assert_eq!(
            lookup("std.http", "serve").unwrap().intrinsic,
            Intrinsic::HttpServe
        );
        assert_eq!(
            lookup("std.sync", "lazy").unwrap().symbol,
            "aura_llvm_lazy_int"
        );
        assert_eq!(lookup("std.sync", "lazy").unwrap().version, ABI_VERSION);
        assert_eq!(
            lookup("std.task", "select").unwrap().intrinsic,
            Intrinsic::TaskSelect
        );
        assert_eq!(
            lookup("std.encoding", "base64Encode").unwrap().intrinsic,
            Intrinsic::Encoding
        );
        assert_eq!(
            lookup("std.task", "cancelAfter").unwrap().intrinsic,
            Intrinsic::TaskCancellation
        );
        assert_eq!(
            lookup_type("std.sync", "Lazy"),
            Some(TypeIntrinsic::SyncLazy)
        );
        assert_eq!(
            lookup("std.url", "queryValue").unwrap().intrinsic,
            Intrinsic::Url
        );
        assert_eq!(lookup("std.os", "cwd").unwrap().intrinsic, Intrinsic::Os);
        assert_eq!(
            lookup("std.dns", "resolveHostList").unwrap().intrinsic,
            Intrinsic::Dns
        );
        assert_eq!(
            lookup("std.test", "assertEqString").unwrap().intrinsic,
            Intrinsic::Test
        );
        assert_eq!(
            lookup_enum("std.io", "TaskError"),
            Some(EnumIntrinsic::IoTaskError)
        );
        assert_eq!(
            tls_connection_constructor("std.tls"),
            Some("aura_new_std_tls_Connection")
        );
        assert!(lookup("demo", "serve").is_none());
    }
}
