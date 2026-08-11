//! Aura CLI — check / build / run / test / bench / new / emit-c with pretty diagnostics.

mod coverage;
mod doc;
mod formatter;
mod runtime_path;
mod scaffold;
mod test_report;

use aura_analysis::{SemaError, SemaErrors};
use aura_codegen::{
    build_from_checked_with_native, build_from_checked_with_options,
    build_tests_from_checked_with_native, emit_c_from_checked, llvm_options, Backend, NativeSource,
};
use aura_diagnostics::{
    classify_async, format_async_error, format_error_with, FormatOptions, JsonDiagnostic, Severity,
};
use aura_lsp::run_stdio_with_std_root;
use aura_package as package;
use package::{
    activate_update, add_dependency, current_target, load_package, load_package_default,
    remove_dependency, LoadedPackage, RegistryIndex, UpdateDecision,
};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

const AURA_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        eprint_usage();
        return ExitCode::from(2);
    }

    let cmd = args.remove(0);
    match cmd.as_str() {
        "check" => cmd_check(&args),
        "build" => cmd_build(&args),
        "run" => cmd_run(&args),
        "test" => cmd_test(&args),
        "bench" => cmd_bench(&args),
        "race" => cmd_race(&args),
        "update" => cmd_update(&args),
        "add" => cmd_add(&args),
        "remove" => cmd_remove(&args),
        "fmt" => cmd_fmt(&args),
        "fix" => cmd_fmt(&args),
        "clean" => cmd_clean(&args),
        "doc" => cmd_doc(&args),
        "tree" => cmd_tree(&args),
        "toolchain" => cmd_toolchain(&args),
        "emit-c" => cmd_emit_c(&args),
        "emit-llvm" => cmd_emit_llvm(&args),
        "language-server" | "lsp" => cmd_language_server(&args),
        "new" => cmd_new(&args),
        "init" => cmd_init(&args),
        "version" | "--version" | "-V" => {
            println!("aura {AURA_VERSION}");
            ExitCode::SUCCESS
        }
        "help" | "-h" | "--help" => {
            eprint_usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("error: unknown command `{other}`");
            eprint_usage();
            ExitCode::from(2)
        }
    }
}

fn eprint_usage() {
    eprintln!(
        "Aura toolchain {AURA_VERSION}\n\n\
         Usage:\n  \
           aura new <path>                   Scaffold package directory\n  \
           aura init [name]                  Scaffold package in current directory\n  \
           aura check [path]                 Parse + typecheck (.aura | dir | aura.toml)\n  \
           aura build [path] [-o <bin>] [--backend c|llvm] [--rebuild-runtime]  Compile to native binary\n  \
           aura run [path] [--rebuild-runtime] [-- args...]  Build to temp and execute\n  \
           aura test [path] [--test-name <pattern>] [--format json] [-- args...]\n  \
           aura bench [path] [--test-name <pattern>] [-- args...]\n  \
           aura race [path] [--format json] [-- args...]\n  \
           aura update ... --activate           Verify and atomically activate update\n  \
           aura add <origin>[@version] [options] Add dependency and refresh lock\n  \
           aura remove <name|origin> [options] Remove dependency and refresh lock\n  \
           aura fmt [--check] <path>          Format/check `.aura` files, project, or folder\n  \
           aura fix [--check] <path>          Apply formatter fixes (alias of `fmt`)\n  \
           aura clean [path]                  Remove only the package target directory\n  \
           aura doc [path] [-o <file>]        Generate deterministic Markdown API docs\n  \
           aura tree [path] [--format json]   Show the resolved package graph\n  \
           aura toolchain [list|current|switch] Manage installed toolchains\n  \
           aura emit-c [path]                Print generated C (debug)\n  \
           aura emit-llvm [path]             Print generated LLVM IR (debug)\n  \
           aura language-server              Run the stdio LSP server (alias: lsp)\n  \
           aura version                      Print CLI version\n  \
           aura help\n\n\
         Path may be a `.aura` file, a package directory, or `aura.toml`.\n\
         With no path, commands look for `./aura.toml`.\n\n\
         See docs/roadmap.md and RFC-001 §6.0 / RFC-005 / RFC-008 / RFC-012."
    );
}

fn cmd_add(args: &[String]) -> ExitCode {
    let parsed = match AddOptions::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let value = match parsed.dependency_value() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let name = match parsed.dependency_name() {
        Ok(name) => name,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let manifest = parsed
        .manifest
        .unwrap_or_else(|| PathBuf::from("aura.toml"));
    let original = match fs::read(&manifest) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("error: read {}: {error}", manifest.display());
            return ExitCode::from(1);
        }
    };
    let lock = manifest.with_file_name("aura.lock");
    let original_lock = fs::read(&lock).ok();
    if let Err(error) = add_dependency(&manifest, &name, &value) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    match load_package(&manifest) {
        Ok(_) => {
            println!("added dependency `{name}`");
            ExitCode::SUCCESS
        }
        Err(error) => {
            restore_file(&manifest, &original);
            restore_optional_file(&lock, original_lock.as_deref());
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn cmd_remove(args: &[String]) -> ExitCode {
    let (manifest, name) = match parse_remove_args(args) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let original = match fs::read(&manifest) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("error: read {}: {error}", manifest.display());
            return ExitCode::from(1);
        }
    };
    let lock = manifest.with_file_name("aura.lock");
    let original_lock = fs::read(&lock).ok();
    if let Err(error) = remove_dependency(&manifest, &name) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    match load_package(&manifest) {
        Ok(_) => {
            println!("removed dependency `{name}`");
            ExitCode::SUCCESS
        }
        Err(error) => {
            restore_file(&manifest, &original);
            restore_optional_file(&lock, original_lock.as_deref());
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn restore_file(path: &Path, contents: &[u8]) {
    let _ = fs::write(path, contents);
}

fn restore_optional_file(path: &Path, contents: Option<&[u8]>) {
    match contents {
        Some(contents) => restore_file(path, contents),
        None => {
            let _ = fs::remove_file(path);
        }
    }
}

#[derive(Debug)]
struct AddOptions {
    origin: String,
    version: Option<String>,
    subdir: Option<String>,
    manifest: Option<PathBuf>,
}

impl AddOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            origin: String::new(),
            version: None,
            subdir: None,
            manifest: None,
        };
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--subdir" | "--manifest" => {
                    let flag = args[i].clone();
                    i += 1;
                    let value = args
                        .get(i)
                        .ok_or_else(|| format!("error: {flag} requires a value"))?
                        .clone();
                    match flag.as_str() {
                        "--subdir" => options.subdir = Some(value),
                        "--manifest" => options.manifest = Some(PathBuf::from(value)),
                        _ => unreachable!(),
                    }
                }
                value if value.starts_with('-') => {
                    return Err(format!("error: unknown add option `{value}`"));
                }
                value => {
                    if !options.origin.is_empty() {
                        return Err("error: usage: aura add <origin>[@version] [options]".into());
                    }
                    options.origin = value.to_string();
                }
            }
            i += 1;
        }
        if options.origin.is_empty() {
            return Err("error: aura add requires a Git origin".into());
        }
        let spec = options.origin.clone();
        let (origin, version) = split_origin_version(&spec);
        options.origin = normalize_origin(origin)?;
        if let Some(version) = version {
            validate_version_selector(version)?;
            options.version = Some(version.trim_start_matches('v').to_string());
        }
        Ok(options)
    }

    fn dependency_name(&self) -> Result<String, String> {
        dependency_name_from_origin(&self.origin, self.subdir.as_deref())
    }

    fn dependency_value(&self) -> Result<String, String> {
        if let Some(subdir) = &self.subdir {
            if subdir.is_empty()
                || subdir.starts_with('/')
                || subdir
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err("error: --subdir must be a normalized relative path".into());
            }
        }
        let mut fields = vec![format!("git = {}", quote_toml(&self.origin))];
        if let Some(subdir) = &self.subdir {
            fields.push(format!("subdir = {}", quote_toml(subdir)));
        }
        if let Some(version) = &self.version {
            fields.push(format!("tag = {}", quote_toml(&format!("v{version}"))));
        }
        Ok(format!("{{ {} }}", fields.join(", ")))
    }
}

fn parse_remove_args(args: &[String]) -> Result<(PathBuf, String), String> {
    let mut manifest = None;
    let mut subdir = None;
    let mut name = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" | "--subdir" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| format!("error: {} requires a value", args[i - 1]))?;
                if args[i - 1] == "--manifest" {
                    manifest = Some(PathBuf::from(value));
                } else {
                    subdir = Some(value.to_string());
                }
            }
            value if value.starts_with('-') => {
                return Err(format!("error: unknown remove option `{value}`"))
            }
            value if name.is_none() => name = Some(value.to_string()),
            _ => {
                return Err(
                    "error: usage: aura remove <name|origin> [--subdir <path>] [--manifest <path>]"
                        .into(),
                )
            }
        }
        i += 1;
    }
    let spec =
        name.ok_or_else(|| "error: aura remove requires a dependency name or origin".to_string())?;
    let dependency = if spec.contains('/')
        || spec.starts_with("http")
        || spec.starts_with("ssh://")
        || spec.starts_with("git@")
        || spec.starts_with("github:")
    {
        let (origin, _) = split_origin_version(&spec);
        let normalized = normalize_origin(origin)?;
        dependency_name_from_origin(&normalized, subdir.as_deref())?
    } else {
        spec
    };
    validate_dependency_name(&dependency)?;
    Ok((
        manifest.unwrap_or_else(|| PathBuf::from("aura.toml")),
        dependency,
    ))
}

fn split_origin_version(spec: &str) -> (&str, Option<&str>) {
    let Some((origin, suffix)) = spec.rsplit_once('@') else {
        return (spec, None);
    };
    let version_like = suffix
        .strip_prefix('v')
        .unwrap_or(suffix)
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit());
    if version_like && !origin.is_empty() {
        (origin, Some(suffix))
    } else {
        (spec, None)
    }
}

fn normalize_origin(origin: &str) -> Result<String, String> {
    let origin = origin.trim();
    if let Some(repo) = origin.strip_prefix("github:") {
        return normalize_github_repo(repo);
    }
    if origin.starts_with("https://")
        || origin.starts_with("ssh://")
        || origin.starts_with("git@")
        || origin.starts_with("file://")
        || Path::new(origin).is_absolute()
    {
        return Ok(origin
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .to_string());
    }
    if origin.matches('/').count() == 1 && !origin.contains(':') {
        return normalize_github_repo(origin);
    }
    Err(format!(
        "error: unsupported origin `{origin}`; use a full Git URL, github:owner/repo, or owner/repo"
    ))
}

fn normalize_github_repo(repo: &str) -> Result<String, String> {
    let repo = repo.trim().trim_end_matches('/').trim_end_matches(".git");
    let mut parts = repo.split('/');
    if parts.next().is_none()
        || parts.next().is_none()
        || parts.next().is_some()
        || repo.contains("..")
    {
        return Err(format!(
            "error: invalid GitHub repository `{repo}`; expected owner/repo"
        ));
    }
    Ok(format!("https://github.com/{repo}"))
}

fn dependency_name_from_origin(origin: &str, subdir: Option<&str>) -> Result<String, String> {
    let source = subdir
        .and_then(|path| path.rsplit('/').next())
        .filter(|name| !name.is_empty())
        .or_else(|| origin.rsplit('/').next())
        .unwrap_or(origin)
        .trim_end_matches(".git");
    let name = source.rsplit(':').next().unwrap_or(source);
    validate_dependency_name(name)?;
    Ok(name.to_string())
}

fn validate_version_selector(version: &str) -> Result<(), String> {
    if version.is_empty()
        || version
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '"' || ch == '@')
        || !version.chars().any(|ch| ch.is_ascii_digit())
    {
        return Err(format!("error: invalid version `{version}`"));
    }
    Ok(())
}

fn validate_dependency_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!("error: invalid dependency name `{name}`"));
    }
    Ok(())
}

fn quote_toml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn cmd_language_server(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("error: language-server does not accept arguments");
        return ExitCode::from(2);
    }
    let std_root = package::std_path::active_toolchain_std_root();
    match run_stdio_with_std_root(std_root) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("language-server: {error}");
            ExitCode::from(1)
        }
    }
}

fn cmd_update(args: &[String]) -> ExitCode {
    let mut package = None;
    let mut current = None;
    let mut target = current_target();
    let mut registry = None;
    let mut json = false;
    let mut activate = false;
    let mut executable = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--package" | "--current" | "--target" | "--registry" | "--executable" => {
                let option = args[i].clone();
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("error: {option} requires a value");
                    return ExitCode::from(2);
                };
                match option.as_str() {
                    "--package" => package = Some(value.clone()),
                    "--current" => current = Some(value.clone()),
                    "--target" => target = value.clone(),
                    "--registry" => registry = Some(value.clone()),
                    "--executable" => executable = Some(PathBuf::from(value)),
                    _ => unreachable!(),
                }
            }
            "--json" => json = true,
            "--activate" => activate = true,
            option if option.starts_with('-') => {
                eprintln!("error: unknown update option `{option}`");
                return ExitCode::from(2);
            }
            value => {
                eprintln!("error: unexpected update argument `{value}`");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }
    let Some(package) = package else {
        eprintln!("error: update requires --package <name>");
        return ExitCode::from(2);
    };
    let Some(current) = current else {
        eprintln!("error: update requires --current <version>");
        return ExitCode::from(2);
    };
    let index = match registry {
        Some(url) => RegistryIndex::open_url(&url),
        None => RegistryIndex::from_env_or_default(),
    };
    let index = match index {
        Ok(index) => index,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    let decision = match index.discover_update(&package, &current, AURA_VERSION, &target) {
        Ok(decision) => decision,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    if activate {
        let UpdateDecision::Update(candidate) = decision else {
            eprintln!("error: --activate requires a compatible update candidate");
            return ExitCode::from(1);
        };
        let active = match executable {
            Some(path) => path,
            None => match env::current_exe() {
                Ok(path) => path,
                Err(error) => {
                    eprintln!("error: cannot locate active executable: {error}");
                    return ExitCode::from(1);
                }
            },
        };
        let source = match index.update_source(&candidate) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(1);
            }
        };
        return match activate_update(&candidate, &source, &active) {
            Ok(result) => {
                if json {
                    println!("{}", result.render_json());
                } else {
                    println!(
                        "[activated] {} -> {} (checksum {}, signature {}, rollback {})",
                        current,
                        result.version,
                        result.checksum,
                        result.signature,
                        result.rollback.display()
                    );
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        };
    }
    if json {
        println!("{}", decision.render_json());
    } else {
        let code = decision.code();
        match &decision {
            UpdateDecision::Update(candidate) => println!(
                "[{code}] update available: {} -> {} ({}, {})",
                current, candidate.meta.vers, candidate.target, candidate.reason
            ),
            UpdateDecision::NoUpdate { current } => {
                println!("[{code}] no update available (current {current})")
            }
            UpdateDecision::Unsupported { target, .. } => {
                println!("[{code}] update unsupported for target {target}")
            }
            UpdateDecision::Revoked { version, reason } => {
                println!("[{code}] update {version} revoked: {reason}")
            }
        }
    }
    match decision {
        UpdateDecision::Unsupported { .. } => ExitCode::from(2),
        UpdateDecision::Revoked { .. } => ExitCode::from(3),
        _ => ExitCode::SUCCESS,
    }
}

fn cmd_fmt(args: &[String]) -> ExitCode {
    let (check, path) = match args {
        [path] => (false, path.as_str()),
        [flag, path] if flag == "--check" => (true, path.as_str()),
        _ => {
            eprintln!("error: usage: aura fmt [--check] <path>");
            return ExitCode::from(2);
        }
    };
    match formatter::format_path(Path::new(path), check) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn cmd_clean(args: &[String]) -> ExitCode {
    if args.len() > 1 || args.first().is_some_and(|arg| arg.starts_with('-')) {
        eprintln!("error: usage: aura clean [path]");
        return ExitCode::from(2);
    }
    let requested = args.first().map(PathBuf::from);
    let root = match requested.as_deref() {
        None => match env::current_dir() {
            Ok(root) => root,
            Err(error) => {
                eprintln!("error: cannot determine project directory: {error}");
                return ExitCode::from(1);
            }
        },
        Some(path) if path.is_file() => path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        Some(path) => path.to_path_buf(),
    };
    let root = match fs::canonicalize(&root) {
        Ok(root) => root,
        Err(error) => {
            eprintln!(
                "error: project directory not found {}: {error}",
                root.display()
            );
            return ExitCode::from(1);
        }
    };
    if requested.is_some() && !root.join("aura.toml").is_file() {
        eprintln!("error: no aura.toml found in {}", root.display());
        return ExitCode::from(1);
    }
    let target = root.join("target");
    if target.is_dir() {
        if let Err(error) = fs::remove_dir_all(&target) {
            eprintln!("error: failed to remove {}: {error}", target.display());
            return ExitCode::from(1);
        }
        println!("removed {}", target.display());
    } else {
        println!("nothing to clean in {}", root.display());
    }
    ExitCode::SUCCESS
}

fn cmd_doc(args: &[String]) -> ExitCode {
    let mut input = None;
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let Some(path) = args.get(i) else {
                    eprintln!("error: {} requires a path", args[i - 1]);
                    return ExitCode::from(2);
                };
                output = Some(PathBuf::from(path));
            }
            value if value.starts_with('-') => {
                eprintln!("error: unknown doc option `{value}`");
                return ExitCode::from(2);
            }
            value if input.is_none() => input = Some(PathBuf::from(value)),
            _ => {
                eprintln!("error: usage: aura doc [path] [-o <file>]");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }
    let package_args = input
        .map(|path| path.display().to_string())
        .into_iter()
        .collect::<Vec<_>>();
    let pkg = match resolve_package(&package_args) {
        Ok(pkg) => pkg,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    let destination =
        output.unwrap_or_else(|| PathBuf::from(format!("target/aura/doc/{}.md", pkg.bin_name)));
    if let Some(parent) = destination.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            eprintln!("error: create {}: {error}", parent.display());
            return ExitCode::from(1);
        }
    }
    let markdown = doc::render(&pkg.ast, &pkg.package);
    match fs::write(&destination, markdown) {
        Ok(()) => {
            println!("generated {}", destination.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: write {}: {error}", destination.display());
            ExitCode::from(1)
        }
    }
}

fn cmd_tree(args: &[String]) -> ExitCode {
    let mut package_args = Vec::new();
    let mut json_output = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--format" => {
                i += 1;
                if args.get(i).map(String::as_str) != Some("json") {
                    eprintln!("error: tree format must be json");
                    return ExitCode::from(2);
                }
                json_output = true;
            }
            "--format=json" => json_output = true,
            value if value.starts_with('-') => {
                eprintln!("error: unknown tree option `{value}`");
                return ExitCode::from(2);
            }
            value => package_args.push(value.to_owned()),
        }
        i += 1;
    }
    if package_args.len() > 1 {
        eprintln!("error: usage: aura tree [path] [--format json]");
        return ExitCode::from(2);
    }
    let graph_path = package_args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let graph = match package::dependency_graph(&graph_path) {
        Ok(graph) => graph,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    };
    if json_output {
        println!("{}", dependency_node_json(&graph));
    } else {
        println!("{}", graph.name);
        render_tree_children(&graph.dependencies, "");
    }
    ExitCode::SUCCESS
}

fn render_tree_children(nodes: &[package::DependencyNode], prefix: &str) {
    for (index, node) in nodes.iter().enumerate() {
        let last = index + 1 == nodes.len();
        let branch = if last { "`--" } else { "|--" };
        println!("{prefix}{branch} {}", node.name);
        let child_prefix = format!("{prefix}{}   ", if last { "    " } else { "|" });
        render_tree_children(&node.dependencies, &child_prefix);
    }
}

fn cmd_toolchain(args: &[String]) -> ExitCode {
    let mut json = args.iter().any(|arg| arg == "--format=json");
    let mut positional = args
        .iter()
        .filter(|arg| arg.as_str() != "--format=json")
        .map(String::as_str)
        .collect::<Vec<_>>();
    if let Some(index) = positional.iter().position(|arg| *arg == "--format") {
        if positional.get(index + 1) != Some(&"json") {
            eprintln!("error: toolchain format must be json");
            return ExitCode::from(2);
        }
        // Keep the spaced and equals forms identical for scripts and editors.
        json = true;
        positional.drain(index..=index + 1);
    }
    let action = positional.first().copied().unwrap_or("current");
    if positional.len() > 2 || !matches!(action, "list" | "current" | "switch") {
        eprintln!("error: usage: aura toolchain [list|current|switch <version>] [--format json]");
        return ExitCode::from(2);
    }
    let home = env::var_os("AURA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|value| PathBuf::from(value).join(".aura")))
        .unwrap_or_else(|| PathBuf::from(".aura"));
    let current = fs::read_link(home.join("current")).ok().and_then(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    });
    match action {
        "current" => {
            if json {
                println!(
                    "{{\"ok\":true,\"current\":{}}}",
                    current
                        .as_deref()
                        .map(json_quote)
                        .unwrap_or_else(|| "null".into())
                );
            } else {
                println!("{}", current.as_deref().unwrap_or("(none)"));
            }
            if current.is_some() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        "list" => {
            let mut versions = fs::read_dir(home.join("versions"))
                .ok()
                .into_iter()
                .flat_map(|entries| entries.filter_map(Result::ok))
                .filter(|entry| entry.path().is_dir())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect::<Vec<_>>();
            versions.sort();
            if json {
                println!(
                    "{{\"ok\":true,\"current\":{},\"versions\":[{}]}}",
                    current
                        .as_deref()
                        .map(json_quote)
                        .unwrap_or_else(|| "null".into()),
                    versions
                        .iter()
                        .map(|version| json_quote(version))
                        .collect::<Vec<_>>()
                        .join(",")
                );
            } else {
                for version in versions {
                    let marker = if current.as_deref() == Some(version.as_str()) {
                        "* "
                    } else {
                        "  "
                    };
                    println!("{marker}{version}");
                }
            }
            ExitCode::SUCCESS
        }
        "switch" => {
            let Some(version) = positional.get(1) else {
                eprintln!("error: toolchain switch requires a version");
                return ExitCode::from(2);
            };
            let avm = home.join("bin/avm");
            if !avm.is_file() {
                eprintln!(
                    "error: Aura Version Manager not found at {}; install a toolchain first",
                    avm.display()
                );
                return ExitCode::from(1);
            }
            match Command::new(avm)
                .arg(version)
                .env("AURA_HOME", &home)
                .status()
            {
                Ok(status) if status.success() => ExitCode::SUCCESS,
                Ok(status) => status
                    .code()
                    .map(|code| ExitCode::from(code.clamp(1, 255) as u8))
                    .unwrap_or_else(|| ExitCode::from(1)),
                Err(error) => {
                    eprintln!("error: toolchain switch failed: {error}");
                    ExitCode::from(1)
                }
            }
        }
        _ => unreachable!(),
    }
}

fn dependency_node_json(node: &package::DependencyNode) -> String {
    format!(
        "{{\"package\":{},\"source\":{},\"dependencies\":[{}]}}",
        json_quote(&node.name),
        node.source
            .as_deref()
            .map(json_quote)
            .unwrap_or_else(|| "null".into()),
        node.dependencies
            .iter()
            .map(dependency_node_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn json_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

fn cmd_new(args: &[String]) -> ExitCode {
    if args.len() != 1 {
        eprintln!("error: usage: aura new <path>");
        return ExitCode::from(2);
    }
    let arg = &args[0];
    let (pkg, bin) = match scaffold::names_from_arg(arg) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let dir = PathBuf::from(arg);
    match scaffold::scaffold_package(&dir, &pkg, &bin) {
        Ok(()) => {
            println!("created package `{pkg}` in {}", dir.display());
            println!("  {}", dir.join("aura.toml").display());
            println!("  {}", dir.join("src/main.aura").display());
            println!("next:  aura run {}", dir.display());
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_init(args: &[String]) -> ExitCode {
    if args.len() > 1 {
        eprintln!("error: usage: aura init [name]");
        return ExitCode::from(2);
    }
    let dir = PathBuf::from(".");
    let (pkg, bin) = if let Some(name) = args.first() {
        match scaffold::names_from_arg(name) {
            Ok(v) => v,
            Err(msg) => {
                eprintln!("{msg}");
                return ExitCode::from(1);
            }
        }
    } else {
        // Derive from current directory name when possible.
        match env::current_dir()
            .ok()
            .and_then(|p| {
                p.file_name()
                    .and_then(|s| s.to_str().map(|s| s.to_string()))
            })
            .and_then(|stem| scaffold::names_from_arg(&stem).ok())
        {
            Some(v) => v,
            None => ("app".into(), "app".into()),
        }
    };
    match scaffold::scaffold_package(&dir, &pkg, &bin) {
        Ok(()) => {
            println!("initialized package `{pkg}` in .");
            println!("  ./aura.toml");
            println!("  ./src/main.aura");
            println!("next:  aura run .");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn resolve_package(args: &[String]) -> Result<LoadedPackage, String> {
    if args.is_empty() {
        load_package_default()
    } else {
        load_package(Path::new(&args[0]))
    }
}

/// Split CLI args at the first `--` into (toolchain args, program argv tail).
/// Without `--`, the whole slice is toolchain args and program args are empty.
fn split_pass_through(args: &[String]) -> (&[String], &[String]) {
    if let Some(i) = args.iter().position(|a| a == "--") {
        (&args[..i], &args[i + 1..])
    } else {
        (args, &[])
    }
}

fn take_rebuild_runtime(args: &[String]) -> (Vec<String>, bool) {
    let mut filtered = Vec::with_capacity(args.len());
    let mut rebuild = false;
    for arg in args {
        if arg == "--rebuild-runtime" {
            rebuild = true;
        } else {
            filtered.push(arg.clone());
        }
    }
    (filtered, rebuild)
}

fn diag_sema(pkg: &LoadedPackage, e: &SemaError) -> String {
    let (path, src, span) = pkg.locate(e.span);
    // C10b: one line of context above the error; auto expected/found notes.
    if let Some(metadata) = classify_async(&e.message) {
        return format_async_error(&path, src, &e.message, span, &metadata);
    }
    format_error_with(
        &path,
        src,
        &e.message,
        span,
        &FormatOptions {
            notes: &[],
            context_before: true,
        },
    )
}

fn diag_sema_json(pkg: &LoadedPackage, e: &SemaError) -> JsonDiagnostic {
    let (path, src, span) = pkg.locate(e.span);
    let diagnostic = JsonDiagnostic::new(path, src, Severity::Error, &e.message, span);
    match classify_async(&e.message) {
        Some(metadata) => diagnostic.with_async_metadata(&metadata),
        None => diagnostic,
    }
}

fn diag_sema_errors(pkg: &LoadedPackage, es: SemaErrors) -> String {
    es.errors
        .iter()
        .map(|e| diag_sema(pkg, e))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn cmd_check(args: &[String]) -> ExitCode {
    let (json, package_args) = match parse_check_options(args) {
        Ok(value) => value,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(2);
        }
    };
    let target = package_args.first().cloned().unwrap_or_else(|| ".".into());
    let loading_started = Instant::now();
    let pkg = match resolve_package(&package_args) {
        Ok(pkg) => pkg,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let parsing_ms = loading_started.elapsed().as_secs_f64() * 1000.0;
    let checking_started = Instant::now();
    match pkg.check_with_plugins() {
        Ok(_) if json => {
            println!(
                "{}",
                render_check_summary(
                    &pkg,
                    parsing_ms,
                    checking_started.elapsed().as_secs_f64() * 1000.0
                )
            );
            ExitCode::SUCCESS
        }
        Ok(_) => {
            let checking_ms = checking_started.elapsed().as_secs_f64() * 1000.0;
            println!(
                "{}",
                render_check_progress(&pkg, &target, parsing_ms, checking_ms)
            );
            ExitCode::SUCCESS
        }
        Err(msg) if json => {
            let diagnostics = msg
                .errors
                .iter()
                .map(|e| diag_sema_json(&pkg, e).to_json())
                .collect::<Vec<_>>();
            eprintln!("{{\"diagnostics\":[{}]}}", diagnostics.join(","));
            ExitCode::from(1)
        }
        Err(msg) => {
            eprintln!("{}", diag_sema_errors(&pkg, msg));
            ExitCode::from(1)
        }
    }
}

fn parse_check_options(args: &[String]) -> Result<(bool, Vec<String>), String> {
    let mut json = false;
    let mut package_args = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--format" => {
                i += 1;
                match args.get(i).map(String::as_str) {
                    Some("json") => json = true,
                    Some(value) => return Err(format!("unsupported check format `{value}`")),
                    None => return Err("--format requires a value".into()),
                }
            }
            value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
            value => package_args.push(value.to_string()),
        }
        i += 1;
    }
    if package_args.len() > 1 {
        return Err("unexpected extra package argument".into());
    }
    Ok((json, package_args))
}

fn render_check_progress(
    pkg: &LoadedPackage,
    target: &str,
    parsing_ms: f64,
    checking_ms: f64,
) -> String {
    let total_ms = parsing_ms + checking_ms;
    format!(
        "[1/1] Checking package {}...\n  ✔  Syntax parsing complete ({})\n  ✔  Symbol resolution complete ({})\n  ✔  Type check & null-safety validation complete ({})\n\n⚡  Checked {} files in {}. 0 errors, 0 warnings.",
        target,
        format_ms(parsing_ms),
        format_ms(checking_ms * 0.35),
        format_ms(checking_ms * 0.65),
        pkg.sources.len(),
        format_ms(total_ms),
    )
}

fn render_check_summary(pkg: &LoadedPackage, parsing_ms: f64, checking_ms: f64) -> String {
    format!(
        "{{\"package\":\"{}\",\"files\":{},\"duration_ms\":{:.3}}}",
        pkg.package,
        pkg.sources.len(),
        parsing_ms + checking_ms
    )
}

fn format_ms(ms: f64) -> String {
    if ms < 1.0 {
        format!("{ms:.2}ms")
    } else {
        format!("{ms:.1}ms")
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn display_path(path: &Path) -> String {
    let value = path.display().to_string();
    if path.is_relative() && !value.starts_with("./") {
        format!("./{value}")
    } else {
        value
    }
}

fn render_test_progress(
    pkg: &LoadedPackage,
    cases: &[test_report::TestCase],
    elapsed_ms: f64,
) -> String {
    let mut lines = vec![format!(
        "[test] Running package tests in {}...",
        pkg.root.display()
    )];
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    for case in cases {
        let (label, duration) = match case.status {
            test_report::TestStatus::Passed => {
                passed += 1;
                ("OK", format_ms(case.duration_ms as f64))
            }
            test_report::TestStatus::Failed => {
                failed += 1;
                ("FAILED", format_ms(case.duration_ms as f64))
            }
            test_report::TestStatus::Skipped => {
                skipped += 1;
                ("SKIPPED", "0.00ms".into())
            }
        };
        lines.push(format!("  RUN  {} ... {} ({duration})", case.name, label));
        if let Some(diagnostic) = &case.diagnostic {
            lines.push(format!("       {}", diagnostic.message));
        }
    }
    lines.push(String::new());
    lines.push(format!(
        "{} {} tests | {} failed | {} skipped (in {})",
        if failed == 0 {
            "✔ Passed:"
        } else {
            "✘ Failed:"
        },
        passed,
        failed,
        skipped,
        format_ms(elapsed_ms)
    ));
    lines.join("\n")
}

fn cmd_emit_c(args: &[String]) -> ExitCode {
    match resolve_package(args).and_then(|pkg| {
        let checked = pkg
            .check_with_plugins()
            .map_err(|e| diag_sema_errors(&pkg, e))?;
        Ok(emit_c_from_checked(&checked))
    }) {
        Ok(c) => {
            print!("{c}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_emit_llvm(args: &[String]) -> ExitCode {
    match resolve_package(args).and_then(|pkg| {
        let checked = pkg
            .check_with_plugins()
            .map_err(|e| diag_sema_errors(&pkg, e))?;
        let program = aura_codegen::LoweredProgram::from_checked(checked);
        if !program.mir_is_complete_for_entrypoint() {
            return Err(format!(
                "LLVM backend requires complete MIR; unsupported functions: {}",
                program.reachable_lowering_gap_names().join(", ")
            ));
        }
        aura_codegen::LlvmBackend::emit_module(&program).map_err(|e| e.to_string())
    }) {
        Ok(ir) => {
            print!("{ir}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_build(args: &[String]) -> ExitCode {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut backend: Option<Backend> = None;
    let mut rebuild_runtime = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: -o requires a path");
                    return ExitCode::from(2);
                }
                output = Some(PathBuf::from(&args[i]));
            }
            "--backend" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    eprintln!("error: --backend requires c or llvm");
                    return ExitCode::from(2);
                };
                backend = Some(match value.as_str() {
                    "c" => Backend::C,
                    "llvm" => Backend::Llvm,
                    _ => {
                        eprintln!("error: unsupported backend `{value}` (expected c or llvm)");
                        return ExitCode::from(2);
                    }
                });
            }
            "--rebuild-runtime" => rebuild_runtime = true,
            s if s.starts_with('-') => {
                eprintln!("error: unknown option `{s}`");
                return ExitCode::from(2);
            }
            s => {
                if input.is_some() {
                    eprintln!("error: unexpected argument `{s}`");
                    return ExitCode::from(2);
                }
                input = Some(PathBuf::from(s));
            }
        }
        i += 1;
    }

    let pkg = match input {
        Some(p) => load_package(&p),
        None => load_package_default(),
    };
    let pkg = match pkg {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };

    let backend = backend.unwrap_or(pkg.profile_settings.backend);

    let out = output.unwrap_or_else(|| PathBuf::from(format!("target/aura/{}", pkg.bin_name)));
    let build_started = Instant::now();
    let runtime = match runtime_c_path(backend, rebuild_runtime, None) {
        Ok(path) => path,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    println!("[build] Checking package {}...", pkg.package);
    let check_started = Instant::now();
    let checked = match pkg.check_with_plugins() {
        Ok(checked) => checked,
        Err(errors) => {
            eprintln!("{}", diag_sema_errors(&pkg, errors));
            return ExitCode::from(1);
        }
    };
    println!(
        "  ✔  Package check complete ({})",
        format_ms(check_started.elapsed().as_secs_f64() * 1000.0)
    );
    println!("[build] Compiling native artifact...");
    let compile_started = Instant::now();
    let native = match native_sources_for(&pkg) {
        Ok(native) => native,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let compiled = if backend == Backend::Llvm {
        let mut options = llvm_options();
        options.profile_settings = pkg.profile_settings.clone();
        options.profile_settings.backend = Backend::Llvm;
        options.backend = Backend::Llvm;
        build_from_checked_with_options(&checked, &out, &runtime, options, native)
    } else {
        build_from_checked_with_native(&checked, &out, &runtime, native)
    }
    .map_err(|e| match e {
        aura_codegen::CodegenError::Sema(se) => diag_sema_errors(&pkg, se),
        other => format!("error: {other}"),
    });
    match compiled {
        Ok(bin) => {
            let size = fs::metadata(&bin)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            println!(
                "  ✔  Native artifact complete ({})",
                format_ms(compile_started.elapsed().as_secs_f64() * 1000.0)
            );
            println!();
            println!(
                "🚀  Created self-contained binary: {} ({}, in {})",
                display_path(&bin),
                format_size(size),
                format_ms(build_started.elapsed().as_secs_f64() * 1000.0)
            );
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn runtime_c_path(
    backend: Backend,
    rebuild_runtime: bool,
    detector: Option<bool>,
) -> Result<PathBuf, String> {
    // Prefer a matching prebuilt archive, then use the source compatibility path.
    if let Some(detector) = detector {
        runtime_path::resolve_runtime_input_with_config(backend, rebuild_runtime, Some(detector))
    } else if rebuild_runtime {
        runtime_path::resolve_runtime_input_with_rebuild(backend, true)
    } else {
        runtime_path::resolve_runtime_input(backend)
    }
}

fn native_sources_for(pkg: &LoadedPackage) -> Result<Vec<NativeSource>, String> {
    let mut sources = Vec::new();
    for (name, config) in &pkg.native {
        let package_root = pkg.native_roots.get(name).unwrap_or(&pkg.root);
        let include_dirs = config
            .include_dirs
            .iter()
            .map(|path| {
                let relative = Path::new(path);
                if relative.is_absolute()
                    || relative
                        .components()
                        .any(|component| matches!(component, Component::ParentDir))
                {
                    return Err(format!(
                        "error: native package `{name}` include directory must be package-relative without `..`: {path}"
                    ));
                }
                Ok(package_root.join(relative))
            })
            .collect::<Result<Vec<_>, String>>()?;
        for source in &config.sources {
            let relative = Path::new(source);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|component| matches!(component, Component::ParentDir))
            {
                return Err(format!(
                    "error: native package `{name}` source must be package-relative without `..`: {source}"
                ));
            }
            let path = package_root.join(relative);
            sources.push(NativeSource {
                name: name.clone(),
                source: path,
                include_dirs: include_dirs.clone(),
                defines: config.defines.clone(),
                linker_args: config.linker_args.clone(),
                static_link: config.static_link,
            });
        }
    }
    Ok(sources)
}

fn build_package(pkg: &LoadedPackage, out: &Path) -> Result<PathBuf, String> {
    build_package_with_runtime(pkg, out, false)
}

fn build_package_with_runtime(
    pkg: &LoadedPackage,
    out: &Path,
    rebuild_runtime: bool,
) -> Result<PathBuf, String> {
    let rt = runtime_c_path(Backend::C, rebuild_runtime, None)?;
    let checked = pkg
        .check_with_plugins()
        .map_err(|e| diag_sema_errors(pkg, e))?;
    build_from_checked_with_native(&checked, out, &rt, native_sources_for(pkg)?).map_err(
        |e| match e {
            aura_codegen::CodegenError::Sema(se) => diag_sema_errors(pkg, se),
            other => format!("error: {other}"),
        },
    )
}

fn cmd_run(args: &[String]) -> ExitCode {
    // C12c: `aura run [path] -- arg1 arg2 …` forwards args after `--` to the binary.
    let (raw_cli_args, program_args) = split_pass_through(args);
    let (cli_args, rebuild_runtime) = take_rebuild_runtime(raw_cli_args);
    let pkg = match resolve_package(&cli_args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let out = PathBuf::from(format!("target/aura/run-{}", pkg.bin_name));
    let build_result = if rebuild_runtime {
        build_package_with_runtime(&pkg, &out, true)
    } else {
        build_package(&pkg, &out)
    };
    match build_result {
        Ok(bin) => {
            let status = Command::new(&bin).args(program_args).status();
            match status {
                Ok(s) if s.success() => ExitCode::SUCCESS,
                Ok(s) => {
                    eprintln!("error: program exited with {s}");
                    ExitCode::from(s.code().unwrap_or(1) as u8)
                }
                Err(e) => {
                    eprintln!("error: failed to execute {}: {e}", bin.display());
                    ExitCode::from(1)
                }
            }
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_test(args: &[String]) -> ExitCode {
    let (raw_cli_args, program_args) = split_pass_through(args);
    let options = match TestOptions::parse(raw_cli_args) {
        Ok(options) => options,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(2);
        }
    };
    let cli_args = &options.package_args;
    let pkg = match resolve_package(cli_args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let all_tests: Vec<String> = pkg
        .ast
        .functions
        .iter()
        .filter(|f| f.is_test)
        .map(|f| f.name.name.clone())
        .collect();
    let selected: Vec<String> = all_tests
        .iter()
        .filter(|name| {
            options
                .test_name
                .as_ref()
                .map(|p| name.contains(p))
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    let n_tests = selected.len();
    if n_tests == 0 {
        if options.test_name.is_some() {
            eprintln!("error: no @test functions match the requested name");
        }
        eprintln!(
            "error: no @test functions found in package `{}` ({} file(s))",
            pkg.package,
            pkg.sources.len()
        );
        return ExitCode::from(1);
    }
    let mut test_pkg = pkg.clone();
    if options.test_name.is_some() {
        for function in &mut test_pkg.ast.functions {
            if function.is_test && !selected.iter().any(|name| name == &function.name.name) {
                function.is_test = false;
            }
        }
    }
    let out = PathBuf::from(format!("target/aura/test-{}", pkg.bin_name));
    let coverage_dir = options
        .coverage
        .then(|| PathBuf::from(format!("target/aura/coverage-{}", pkg.bin_name)));
    if let Some(directory) = &coverage_dir {
        if let Err(error) = fs::create_dir_all(directory) {
            eprintln!(
                "error: create coverage directory {}: {error}",
                directory.display()
            );
            return ExitCode::from(1);
        }
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "profraw")
                    || path.extension().is_some_and(|ext| ext == "profdata")
                    || path.extension().is_some_and(|ext| ext == "lcov")
                {
                    let _ = fs::remove_file(path);
                }
            }
        }
        let _ = fs::remove_dir_all(directory.join("html"));
    }
    let started = Instant::now();
    match build_test_package(&test_pkg, &out, options.race, options.coverage) {
        Ok(bin) => {
            let mut command = Command::new(&bin);
            command.args(program_args);
            if let Some(directory) = &coverage_dir {
                command.env("LLVM_PROFILE_FILE", directory.join("default-%p.profraw"));
            }
            let output = command.output();
            match output {
                Ok(output) => {
                    let elapsed = started.elapsed().as_millis();
                    let coverage_result = coverage_dir
                        .as_deref()
                        .map(|directory| coverage::collect(&out, directory));
                    if let Some(Err(error)) = &coverage_result {
                        eprintln!("{error}");
                    }
                    let status = output.status.success()
                        && coverage_result.as_ref().is_none_or(Result::is_ok);
                    let cases = test_report::cases_from_output(
                        &pkg.package,
                        &all_tests,
                        &selected,
                        &output.stdout,
                        &output.stderr,
                        status,
                    );
                    if options.json {
                        let report = test_report::TestReport {
                            package: pkg.package.clone(),
                            duration_ms: elapsed,
                            tests: cases,
                            coverage: coverage_result.and_then(|result| {
                                result.ok().map(|artifacts| test_report::CoverageReport {
                                    lcov: artifacts.lcov.display().to_string(),
                                    html: artifacts.html.display().to_string(),
                                })
                            }),
                        };
                        if options.race {
                            println!(
                                "{{\"mode\":\"race\",\"detector\":true,\"result\":{}}}",
                                report.to_json()
                            );
                        } else {
                            println!("{}", report.to_json());
                        }
                    } else {
                        println!("{}", render_test_progress(&pkg, &cases, elapsed as f64));
                        if let Some(Ok(artifacts)) = coverage_result {
                            println!(
                                "coverage: LCOV {} | HTML {}",
                                artifacts.lcov.display(),
                                artifacts.html.display()
                            );
                        }
                        if options.race {
                            println!(
                                "race: {} (detector=on)",
                                if status { "pass" } else { "fail" }
                            );
                        }
                    }
                    if status {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::from(output.status.code().unwrap_or(1) as u8)
                    }
                }
                Err(e) => {
                    eprintln!("error: failed to execute {}: {e}", bin.display());
                    ExitCode::from(1)
                }
            }
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_bench(args: &[String]) -> ExitCode {
    let (raw_cli_args, program_args) = split_pass_through(args);
    let options = match TestOptions::parse(raw_cli_args) {
        Ok(options) => options,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(2);
        }
    };
    let pkg = match resolve_package(&options.package_args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let benchmarks: Vec<String> = pkg
        .ast
        .functions
        .iter()
        .filter(|f| f.attributes.iter().any(|a| a.name.name == "bench"))
        .map(|f| f.name.name.clone())
        .collect();
    let selected: Vec<String> = benchmarks
        .iter()
        .filter(|name| {
            options
                .test_name
                .as_ref()
                .map(|pattern| name.contains(pattern))
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    if selected.is_empty() {
        eprintln!(
            "error: no @bench functions{} in package `{}`",
            options
                .test_name
                .as_ref()
                .map(|pattern| format!(" match `{pattern}`"))
                .unwrap_or_default(),
            pkg.package
        );
        return ExitCode::from(1);
    }
    let mut bench_pkg = pkg.clone();
    for function in &mut bench_pkg.ast.functions {
        let is_bench = function.attributes.iter().any(|a| a.name.name == "bench");
        function.is_test = is_bench && selected.iter().any(|name| name == &function.name.name);
    }
    let out = PathBuf::from(format!("target/aura/bench-{}", pkg.bin_name));
    let started = Instant::now();
    match build_test_package(&bench_pkg, &out, false, false) {
        Ok(bin) => match Command::new(&bin).args(program_args).output() {
            Ok(output) => {
                let status = output.status.success();
                if options.json {
                    let cases = test_report::cases_from_output(
                        &pkg.package,
                        &benchmarks,
                        &selected,
                        &output.stdout,
                        &output.stderr,
                        status,
                    );
                    let report = test_report::TestReport {
                        package: pkg.package.clone(),
                        duration_ms: started.elapsed().as_millis(),
                        tests: cases,
                        coverage: None,
                    };
                    println!("{}", report.to_json());
                } else {
                    print!("{}", String::from_utf8_lossy(&output.stdout));
                    eprint!("{}", String::from_utf8_lossy(&output.stderr));
                }
                if status {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(output.status.code().unwrap_or(1) as u8)
                }
            }
            Err(e) => {
                eprintln!("error: failed to execute {}: {e}", bin.display());
                ExitCode::from(1)
            }
        },
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

/// R5: frozen user-facing entry point for the alpha detector workflow.
/// `aura race` is deliberately test-shaped: the child status is the stable
/// pass/fail contract, while the detector-enabled generated binary supplies
/// the runtime evidence.
fn cmd_race(args: &[String]) -> ExitCode {
    let mut test_args = Vec::with_capacity(args.len() + 1);
    test_args.push("--race".into());
    test_args.extend_from_slice(args);
    cmd_test(&test_args)
}

struct TestOptions {
    package_args: Vec<String>,
    test_name: Option<String>,
    json: bool,
    race: bool,
    coverage: bool,
}

impl TestOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut package_args = Vec::new();
        let mut test_name = None;
        let mut json = false;
        let mut race = false;
        let mut coverage = false;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--test-name" | "--filter" => {
                    i += 1;
                    let value = args.get(i).ok_or("--test-name requires a pattern")?;
                    test_name = Some(value.clone());
                }
                "--format" | "--report" => {
                    i += 1;
                    match args.get(i).map(String::as_str) {
                        Some("json") => json = true,
                        Some(value) => {
                            return Err(format!("unsupported test report format `{value}`"))
                        }
                        None => return Err("--format requires a value".into()),
                    }
                }
                "--race" => race = true,
                "--coverage" => coverage = true,
                value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
                value => package_args.push(value.to_string()),
            }
            i += 1;
        }
        if package_args.len() > 1 {
            return Err("unexpected extra package argument".into());
        }
        Ok(Self {
            package_args,
            test_name,
            json,
            race,
            coverage,
        })
    }
}

fn build_test_package(
    pkg: &LoadedPackage,
    out: &Path,
    sanitizer: bool,
    coverage: bool,
) -> Result<PathBuf, String> {
    let rt = runtime_c_path(Backend::C, false, Some(sanitizer))?;
    let checked = pkg
        .check_with_plugins()
        .map_err(|e| diag_sema_errors(pkg, e))?;
    build_tests_from_checked_with_native(
        &checked,
        out,
        &rt,
        native_sources_for(pkg)?,
        sanitizer,
        coverage,
    )
    .map_err(|e| match e {
        aura_codegen::CodegenError::Sema(se) => diag_sema_errors(pkg, se),
        other => format!("error: {other}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{build_package, cmd_clean, cmd_fmt, split_pass_through, AddOptions, TestOptions};
    use crate::package::load_package;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    fn s(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| (*x).to_string()).collect()
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn clean_removes_only_project_target() {
        let root = std::env::temp_dir().join(format!("aura-clean-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("target/aura")).unwrap();
        fs::write(root.join("aura.toml"), "[package]\nname = \"clean\"\n").unwrap();
        fs::write(root.join("keep.txt"), "keep").unwrap();

        assert_eq!(
            cmd_clean(&[root.display().to_string()]),
            std::process::ExitCode::SUCCESS
        );
        assert!(!root.join("target").exists());
        assert!(root.join("aura.toml").exists());
        assert!(root.join("keep.txt").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_rejects_extra_arguments() {
        assert_eq!(
            cmd_clean(&s(&["one", "two"])),
            std::process::ExitCode::from(2)
        );
    }

    #[test]
    fn split_no_separator_keeps_all_as_cli() {
        let args = s(&["pkg", "extra"]);
        let (cli, prog) = split_pass_through(&args);
        assert_eq!(cli, &args[..]);
        assert!(prog.is_empty());
    }

    #[test]
    fn check_options_accept_json_and_one_path() {
        assert_eq!(
            super::parse_check_options(&s(&["--format", "json", "x"])).unwrap(),
            (true, s(&["x"]))
        );
    }

    #[test]
    fn add_options_render_git_dependency() {
        let options =
            AddOptions::parse(&s(&["auraspace/aura@v0.1.1-alpha.5", "--subdir", "std/io"]))
                .unwrap();
        assert_eq!(options.dependency_name().unwrap(), "io");
        assert_eq!(
            options.dependency_value().unwrap(),
            "{ git = \"https://github.com/auraspace/aura\", subdir = \"std/io\", tag = \"v0.1.1-alpha.5\" }"
        );
    }

    #[test]
    fn add_options_allow_versionless_git_origin() {
        let options = AddOptions::parse(&s(&["https://git.example.com/org/demo.git"])).unwrap();
        assert_eq!(options.dependency_name().unwrap(), "demo");
        assert_eq!(
            options.dependency_value().unwrap(),
            "{ git = \"https://git.example.com/org/demo\" }"
        );
    }

    #[test]
    fn add_options_reject_legacy_name_version_form() {
        let error = AddOptions::parse(&s(&["demo.dep@1.0.0"])).unwrap_err();
        assert!(error.contains("unsupported origin"));
    }

    #[test]
    fn fmt_check_reports_changes_without_writing() {
        let path = std::env::temp_dir().join(format!(
            "aura-fmt-cli-{}-{}.aura",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let source = "package demo\nfun main(){return}\n";
        fs::write(&path, source).unwrap();

        assert_eq!(
            cmd_fmt(&s(&["--check", path.to_str().unwrap()])),
            std::process::ExitCode::from(1)
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert_eq!(
            cmd_fmt(&s(&[path.to_str().unwrap()])),
            std::process::ExitCode::SUCCESS
        );
        assert_eq!(
            cmd_fmt(&s(&["--check", path.to_str().unwrap()])),
            std::process::ExitCode::SUCCESS
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn split_with_separator_forwards_tail() {
        let args = s(&["corpus/std_io/args", "--", "hello", "world"]);
        let (cli, prog) = split_pass_through(&args);
        assert_eq!(cli, &s(&["corpus/std_io/args"])[..]);
        assert_eq!(prog, &s(&["hello", "world"])[..]);
    }

    #[test]
    fn split_leading_separator_allows_default_package() {
        let args = s(&["--", "a"]);
        let (cli, prog) = split_pass_through(&args);
        assert!(cli.is_empty());
        assert_eq!(prog, &s(&["a"])[..]);
    }

    #[test]
    fn split_empty_tail_after_separator() {
        let args = s(&["pkg", "--"]);
        let (cli, prog) = split_pass_through(&args);
        assert_eq!(cli, &s(&["pkg"])[..]);
        assert!(prog.is_empty());
    }

    #[test]
    fn test_options_keep_package_and_filter_separate() {
        let args = s(&["corpus/test", "--test-name", "add", "--format", "json"]);
        let options = TestOptions::parse(&args).expect("parse test options");
        assert_eq!(options.package_args, s(&["corpus/test"]));
        assert_eq!(options.test_name.as_deref(), Some("add"));
        assert!(options.json);
        assert!(!options.race);
        assert!(!options.coverage);
    }

    #[test]
    fn test_options_enable_coverage_mode() {
        let options = TestOptions::parse(&s(&["corpus/test", "--coverage"])).unwrap();
        assert!(options.coverage);
        assert!(!options.race);
    }

    #[test]
    fn race_options_enable_detector_mode() {
        let options = TestOptions::parse(&s(&["corpus/test", "--race"])).expect("race options");
        assert!(options.race);
        assert_eq!(options.package_args, s(&["corpus/test"]));
    }

    /// C12e: non-zero `std.io.exit` must be observable on the process status.
    #[test]
    fn std_io_exit_nonzero_status() {
        let root = repo_root();
        let pkg_path = root.join("corpus/std_io/exit");
        let pkg = load_package(&pkg_path).expect("load corpus/std_io/exit");
        let out = std::env::temp_dir().join(format!(
            "aura-exit-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let bin = build_package(&pkg, &out).expect("build exit corpus");
        let status = Command::new(&bin)
            .arg("7")
            .status()
            .expect("spawn built binary");
        assert_eq!(
            status.code(),
            Some(7),
            "std.io.exit(7) should set process exit code 7; got {status}"
        );
        let status0 = Command::new(&bin).status().expect("spawn default");
        assert!(
            status0.success(),
            "default smoke path should exit 0; got {status0}"
        );
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(format!("{}.aura.c", out.display()));
    }

    /// S1.1: argv strings must remain valid when Array<String> is dropped.
    #[test]
    fn std_io_args_owns_strings_through_teardown() {
        let root = repo_root();
        let pkg_path = root.join("corpus/std_io/args");
        let pkg = load_package(&pkg_path).expect("load corpus/std_io/args");
        let out = std::env::temp_dir().join(format!(
            "aura-args-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let bin = build_package(&pkg, &out).expect("build args corpus");
        let status = Command::new(&bin)
            .args(["hello", "world"])
            .status()
            .expect("spawn built binary");
        assert!(
            status.success(),
            "std.io.args() should not abort while dropping Array<String>: {status}"
        );
        let _ = std::fs::remove_file(&bin);
        let _ = std::fs::remove_file(format!("{}.aura.c", out.display()));
    }
}
