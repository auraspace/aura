//! Aura CLI — check / build / run / test / new / emit-c with pretty diagnostics.

mod formatter;
mod package;
mod runtime_path;
mod scaffold;
mod std_path;
mod test_report;

use aura_codegen::{build_from_file, build_tests_from_file, emit_c_from_ast};
use aura_diagnostics::{
    classify_async, format_async_error, format_error_with, FormatOptions, JsonDiagnostic, Severity,
};
use aura_sema::{check_file, SemaError, SemaErrors};
use package::{
    activate_update, current_target, load_package, load_package_default, publish_dry_run, publish_package,
    LoadedPackage, RegistryIndex, UpdateDecision, ENV_REGISTRY_TOKEN,
};
use std::env;
use std::path::{Path, PathBuf};
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
        "race" => cmd_race(&args),
        "publish" => cmd_publish(&args),
        "update" => cmd_update(&args),
        "fmt" => cmd_fmt(&args),
        "emit-c" => cmd_emit_c(&args),
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
           aura build [path] [-o <bin>]      Compile to native binary (C backend)\n  \
           aura run [path] [-- args...]      Build to temp and execute\n  \
           aura test [path] [--test-name <pattern>] [--format json] [-- args...]\n  \
           aura race [path] [--format json] [-- args...]\n  \
           aura publish --dry-run [path]    Validate and preview without upload\n  \
           aura publish --registry <url> [path]  Validate and upload package\n  \
           aura update ... --activate           Verify and atomically activate update\n  \
           aura fmt <path>                   Format an Aura source file\n  \
           aura emit-c [path]                Print generated C (debug)\n  \
           aura version                      Print CLI version\n  \
           aura help\n\n\
         Path may be a `.aura` file, a package directory, or `aura.toml`.\n\
         With no path, commands look for `./aura.toml`.\n\n\
         See docs/roadmap.md and RFC-001 §6.0 / RFC-005 / RFC-008 / RFC-012."
    );
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
                current,
                candidate.meta.vers,
                candidate.target,
                candidate.reason
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

fn cmd_publish(args: &[String]) -> ExitCode {
    let mut dry_run = false;
    let mut path = None;
    let mut registry = None;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--dry-run" {
            dry_run = true;
        } else if arg == "--registry" {
            i += 1;
            let Some(value) = args.get(i) else {
                eprintln!("error: --registry requires a URL");
                return ExitCode::from(2);
            };
            registry = Some(value.clone());
        } else if arg.starts_with('-') {
            eprintln!("error: unknown publish option `{arg}`");
            return ExitCode::from(2);
        } else if path.replace(PathBuf::from(arg)).is_some() {
            eprintln!("error: unexpected extra package argument `{arg}`");
            return ExitCode::from(2);
        }
        i += 1;
    }
    let path = path.unwrap_or_else(|| PathBuf::from("aura.toml"));
    if dry_run {
        if registry.is_some() {
            eprintln!("error: --registry cannot be combined with --dry-run");
            return ExitCode::from(2);
        }
        return match publish_dry_run(path) {
            Ok(preview) => {
                println!("{}", preview.render());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(1)
            }
        };
    }
    let registry = registry.or_else(|| std::env::var("AURA_REGISTRY_URL").ok());
    let Some(registry) = registry else {
        eprintln!("error: publish upload requires --registry <url> or AURA_REGISTRY_URL");
        return ExitCode::from(2);
    };
    match publish_package(path, &registry, std::env::var(ENV_REGISTRY_TOKEN).ok().as_deref()) {
        Ok(receipt) => {
            println!("{}", receipt.render_json());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", error.render_json());
            if error.kind == package::PublishErrorKind::Indeterminate {
                ExitCode::from(3)
            } else {
                ExitCode::from(1)
            }
        }
    }
}

fn cmd_fmt(args: &[String]) -> ExitCode {
    if args.len() != 1 {
        eprintln!("error: usage: aura fmt <path>");
        return ExitCode::from(2);
    }
    let path = Path::new(&args[0]);
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", path.display());
            return ExitCode::from(1);
        }
    };
    match formatter::format_source(&source) {
        Ok(formatted) => match std::fs::write(path, formatted) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: cannot write {}: {error}", path.display());
                ExitCode::from(1)
            }
        },
        Err(error) => {
            eprintln!("error: cannot format {}: {error}", path.display());
            ExitCode::from(1)
        }
    }
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
    match resolve_package(&package_args).and_then(|pkg| check_package_mode(pkg, json)) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
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

fn check_package(pkg: LoadedPackage) -> Result<String, String> {
    let checked = check_file(&pkg.ast).map_err(|e| diag_sema_errors(&pkg, e))?;

    let mut lines = Vec::new();
    if pkg.sources.len() == 1 {
        lines.push(format!("ok  {}", pkg.sources[0].path.display()));
    } else {
        lines.push(format!(
            "ok  {} ({} files)",
            pkg.root.display(),
            pkg.sources.len()
        ));
        for s in &pkg.sources {
            lines.push(format!("  file {}", s.path.display()));
        }
    }
    lines.push(format!("package {}", checked.package));
    if !checked.interfaces.is_empty() {
        lines.push(format!("{} interface(s)", checked.interfaces.len()));
        for i in &checked.interfaces {
            lines.push(format!(
                "  interface {} ({} method(s))",
                i.name,
                i.methods.len()
            ));
        }
    }
    if !checked.enums.is_empty() {
        lines.push(format!("{} enum(s)", checked.enums.len()));
        for e in &checked.enums {
            lines.push(format!(
                "  enum {} ({} variant(s))",
                e.name,
                e.variants.len()
            ));
        }
    }
    if !checked.classes.is_empty() {
        let n_cls = checked.classes.iter().filter(|c| !c.is_struct).count();
        let n_st = checked.classes.iter().filter(|c| c.is_struct).count();
        if n_cls > 0 {
            lines.push(format!("{n_cls} class(es)"));
        }
        if n_st > 0 {
            lines.push(format!("{n_st} struct(s)"));
        }
        for c in &checked.classes {
            let kind = if c.is_struct { "struct" } else { "class" };
            let impls = if c.implements.is_empty() {
                String::new()
            } else {
                format!(
                    " : {}",
                    c.implements
                        .iter()
                        .map(|t| t.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            lines.push(format!(
                "  {kind} {}{} ({} field(s), {} method(s))",
                c.name,
                impls,
                c.fields.len(),
                c.methods.len()
            ));
        }
    }
    lines.push(format!(
        "{} function(s) typechecked",
        checked.functions.len()
    ));
    let n_tests = checked.functions.iter().filter(|f| f.is_test).count();
    if n_tests > 0 {
        lines.push(format!("{n_tests} @test function(s)"));
    }
    for f in &checked.functions {
        let mark = if f.is_test { " @test" } else { "" };
        lines.push(format!(
            "  fun{} {}({}) -> {}",
            mark,
            f.name,
            f.params
                .iter()
                .map(|t| t.display())
                .collect::<Vec<_>>()
                .join(", "),
            f.ret.display()
        ));
    }
    Ok(lines.join("\n"))
}

fn check_package_mode(pkg: LoadedPackage, json: bool) -> Result<String, String> {
    match check_file(&pkg.ast) {
        Ok(_) => check_package(pkg),
        Err(errors) if json => {
            let diagnostics = errors
                .errors
                .iter()
                .map(|e| diag_sema_json(&pkg, e).to_json())
                .collect::<Vec<_>>();
            Err(format!("{{\"diagnostics\":[{}]}}", diagnostics.join(",")))
        }
        Err(errors) => Err(diag_sema_errors(&pkg, errors)),
    }
}

fn cmd_emit_c(args: &[String]) -> ExitCode {
    match resolve_package(args).and_then(|pkg| {
        emit_c_from_ast(&pkg.ast).map_err(|e| match e {
            aura_codegen::CodegenError::Sema(se) => diag_sema_errors(&pkg, se),
            other => format!("error: {other}"),
        })
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

fn cmd_build(args: &[String]) -> ExitCode {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
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

    let out = output.unwrap_or_else(|| PathBuf::from(format!("target/aura/{}", pkg.bin_name)));
    match build_package(&pkg, &out) {
        Ok(bin) => {
            println!("ok  {}", bin.display());
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

fn runtime_c_path() -> Result<PathBuf, String> {
    // Dev monorepo path, AURA_RUNTIME, binary-adjacent, or embedded cache (install).
    runtime_path::resolve_runtime_c()
}

fn build_package(pkg: &LoadedPackage, out: &Path) -> Result<PathBuf, String> {
    let rt = runtime_c_path()?;
    build_from_file(&pkg.ast, out, &rt).map_err(|e| match e {
        aura_codegen::CodegenError::Sema(se) => diag_sema_errors(pkg, se),
        other => format!("error: {other}"),
    })
}

fn cmd_run(args: &[String]) -> ExitCode {
    // C12c: `aura run [path] -- arg1 arg2 …` forwards args after `--` to the binary.
    let (cli_args, program_args) = split_pass_through(args);
    let pkg = match resolve_package(cli_args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let out = PathBuf::from(format!("target/aura/run-{}", pkg.bin_name));
    match build_package(&pkg, &out) {
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
    let started = Instant::now();
    match build_test_package(&test_pkg, &out) {
        Ok(bin) => {
            let output = Command::new(&bin).args(program_args).output();
            match output {
                Ok(output) => {
                    let elapsed = started.elapsed().as_millis();
                    let status = output.status.success();
                    if options.json {
                        let cases = test_report::cases_from_output(
                            &pkg.package,
                            &all_tests,
                            &selected,
                            &output.stdout,
                            &output.stderr,
                            status,
                        );
                        let report = test_report::TestReport {
                                package: pkg.package.clone(),
                                duration_ms: elapsed,
                                tests: cases
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
                        print!("{}", String::from_utf8_lossy(&output.stdout));
                        eprint!("{}", String::from_utf8_lossy(&output.stderr));
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
}

impl TestOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut package_args = Vec::new();
        let mut test_name = None;
        let mut json = false;
        let mut race = false;
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
        })
    }
}

fn build_test_package(pkg: &LoadedPackage, out: &Path) -> Result<PathBuf, String> {
    let rt = runtime_c_path()?;
    build_tests_from_file(&pkg.ast, out, &rt).map_err(|e| match e {
        aura_codegen::CodegenError::Sema(se) => diag_sema_errors(pkg, se),
        other => format!("error: {other}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{build_package, split_pass_through, TestOptions};
    use crate::package::load_package;
    use std::path::PathBuf;
    use std::process::Command;

    fn s(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| (*x).to_string()).collect()
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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
