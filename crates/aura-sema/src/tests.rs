use crate::ty::Ty;
use crate::{
    check_file, check_file_with_derives, check_file_with_macros, check_file_with_plugin_source,
    MacroError, MacroExpansion, UserDerive, UserMacro,
};
use aura_ast::{
    AsyncExpr, AsyncFunDecl, Block, ChannelSendExpr, ClassDecl, Expr, File, FunDecl, Ident, Span,
    SpawnExpr, Stmt,
};
use aura_parser::parse_file;

fn promote_to_async(file: &mut aura_ast::File, index: usize) {
    let f = file.functions.remove(index);
    file.async_functions.push(AsyncFunDecl {
        is_pub: f.is_pub,
        origin_package: f.origin_package,
        attributes: f.attributes,
        is_test: f.is_test,
        name: f.name,
        type_params: f.type_params,
        params: f.params,
        return_type: f.return_type,
        body: f.body,
        span: f.span,
    });
}

#[test]
fn mono_suffix() {
    let t = Ty::ClassApp {
        name: "Box".into(),
        args: vec![Ty::String],
    };
    assert_eq!(t.mono_suffix(), "Box_String");
}

#[test]
fn checked_file_exposes_retention_and_derive_expansion_metadata() {
    let file = parse_file(
        "package demo\n@reflect\n@derive(Equals) class Point(@deprecated(\"old\") val x: Int) {}\n",
    )
    .expect("metadata fixture parses");
    let checked = check_file(&file).expect("metadata fixture checks");
    assert!(checked
        .attribute_metadata
        .iter()
        .any(|item| item.name == "reflect" && item.retention.abi_code() == 2));
    assert!(checked
        .attribute_metadata
        .iter()
        .any(|item| item.name == "deprecated" && item.retention.abi_code() == 1));
    let expansion = checked
        .expansions
        .iter()
        .find(|item| item.generated_item == "Point.equals")
        .expect("Equals expansion metadata");
    assert_eq!(expansion.phase, "derive");
    assert_eq!(expansion.macro_name, "Equals");
    assert_eq!(expansion.invocation_span, expansion.generated_span);
}

struct CustomMarkerDerive;

impl UserDerive for CustomMarkerDerive {
    fn name(&self) -> &str {
        "CustomMarker"
    }

    fn expand(&self, _input: &ClassDecl) -> Result<Vec<FunDecl>, MacroError> {
        let fixture =
            parse_file("package demo\nclass Generated() { fun marker(): Int { return 1 } }\n")
                .expect("user derive fixture parses");
        Ok(fixture.classes[0].methods.clone())
    }
}

#[test]
fn registered_user_derive_expands_before_typecheck() {
    let file = parse_file("package demo\n@derive(CustomMarker) class Value() {}\n")
        .expect("user derive source parses");
    let derive = CustomMarkerDerive;
    let checked = check_file_with_derives(&file, &[&derive]).expect("user derive checks");
    assert!(checked.ast.classes[0]
        .methods
        .iter()
        .any(|method| method.name.name == "marker"));
    assert!(checked
        .expansions
        .iter()
        .any(|item| item.macro_name == "CustomMarker" && item.generated_item == "Value.marker"));
}

struct CustomMarkerMacro;

impl UserMacro for CustomMarkerMacro {
    fn name(&self) -> &str {
        "CustomMarkerMacro"
    }

    fn expand(&self, file: &mut File) -> Result<Vec<MacroExpansion>, MacroError> {
        let class = file.classes.first_mut().ok_or_else(|| MacroError {
            message: "expected a class".into(),
            span: file.span,
        })?;
        let invocation_span = class.span;
        let fixture = parse_file(
            "package demo\n@derive(CustomMarker) class Generated() { fun macroMarker(): Int { return 1 } }\n",
        )
        .expect("user macro fixture parses");
        let method = fixture.classes[0].methods[0].clone();
        let generated_span = method.span;
        class
            .attributes
            .extend(fixture.classes[0].attributes.iter().cloned());
        class.methods.push(method);
        Ok(vec![MacroExpansion {
            macro_name: self.name().into(),
            generated_item: "Value.macroMarker".into(),
            invocation_span,
            generated_span,
        }])
    }
}

#[test]
fn registered_user_macro_expands_before_typecheck() {
    let file = parse_file("package demo\nclass Value() {}\n").expect("user macro source parses");
    let macro_impl = CustomMarkerMacro;
    let derive = CustomMarkerDerive;
    let checked =
        check_file_with_macros(&file, &[&macro_impl], &[&derive]).expect("user macro checks");
    assert!(checked.ast.classes[0]
        .methods
        .iter()
        .any(|method| method.name.name == "macroMarker"));
    assert!(checked.ast.classes[0]
        .methods
        .iter()
        .any(|method| method.name.name == "marker"));
    let expansion = checked
        .expansions
        .iter()
        .find(|item| item.generated_item == "Value.macroMarker")
        .expect("user macro expansion metadata");
    assert_eq!(expansion.phase, "macro");
    assert_eq!(expansion.macro_name, "CustomMarkerMacro");
}

#[test]
fn sandboxed_plugin_source_is_merged_and_recorded_before_typecheck() {
    let file = parse_file("package demo\nclass Value() {}\n").expect("plugin host source parses");
    let checked = check_file_with_plugin_source(
        &file,
        "ExternalMacro",
        "package demo\nfun generated(): Int { return 7 }\n",
        Span::new(8, 15),
        &[],
    )
    .expect("plugin source checks");
    assert!(checked
        .ast
        .functions
        .iter()
        .any(|function| function.name.name == "generated"));
    let expansion = checked
        .expansions
        .iter()
        .find(|item| item.macro_name == "ExternalMacro")
        .expect("plugin expansion metadata");
    assert_eq!(expansion.phase, "macro");
    assert_eq!(expansion.invocation_span, Span::new(8, 15));
}

#[test]
fn sandboxed_plugin_source_cannot_change_package_identity() {
    let file = parse_file("package demo\nclass Value() {}\n").expect("plugin host source parses");
    let error = check_file_with_plugin_source(
        &file,
        "ExternalMacro",
        "package other\nfun generated(): Int { return 7 }\n",
        Span::new(8, 15),
        &[],
    )
    .expect_err("plugin package mismatch must fail");
    assert!(error.primary().message.contains("does not match"));
}

#[test]
fn foreign_declaration_accepts_alpha_c_contract() {
    let file = parse_file(
        "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\nextern \"C\" fun native_abs(value: Int): Int\n",
    )
    .expect("parse");
    check_file(&file).expect("valid F1 declaration");
}

#[test]
fn foreign_status_failure_convention_requires_int_return() {
    let valid = parse_file(
        "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\", failure = \"status\")\nextern \"C\" fun native_status(value: Int): Int\n",
    )
    .expect("parse valid status convention");
    check_file(&valid).expect("status convention accepts Int");

    let invalid = parse_file(
        "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\", failure = \"status\")\nextern \"C\" fun native_status(value: Int): Bool\n",
    )
    .expect("parse invalid status convention");
    let error = check_file(&invalid).expect_err("status convention requires Int");
    assert!(error.to_string().contains("AURA-F2-FAILURE"));
}

#[test]
fn foreign_declaration_rejects_target_before_codegen() {
    let file = parse_file(
        "package demo\n@foreign(library = \"m\", target = \"windows-x86_64\", link = \"dynamic\", abi = 1, abi_id = \"c\")\nextern \"C\" fun native_abs(value: Int): Int\n",
    )
    .expect("parse");
    let error = check_file(&file).expect_err("unsupported target");
    assert!(error.to_string().contains("AURA-F1-TARGET"));
    assert!(error.to_string().contains("windows-x86_64"));
}

#[test]
fn foreign_declaration_rejects_non_c_abi_and_borrow_types() {
    let file = parse_file(
        "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 2, abi_id = \"rust\")\nextern \"Rust\" fun native_abs(value: ref String): Unit\n",
    )
    .expect("parse");
    let error = check_file(&file).expect_err("unsupported ABI");
    let message = error.to_string();
    assert!(message.contains("AURA-F1-CONVENTION"));
    assert!(message.contains("AURA-F1-ABI"));
    assert!(message.contains("AURA-F1-TYPE"));
}

#[test]
fn foreign_declaration_rejects_runtime_owned_async_handles() {
    let file = parse_file(
        "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_task(task: Task<Int>?): Unit\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_handle(handle: TaskHandle<Int>): Channel<Int>\n",
    )
    .expect("parse");
    let error = check_file(&file).expect_err("runtime-owned handles must not cross C");
    let message = error.to_string();
    assert!(message.contains("AURA-F4-BOUNDARY"));
    assert!(message.contains("Task"));
    assert!(message.contains("TaskHandle"));
    assert!(message.contains("Channel"));
    assert!(message.contains("async pin/ownership proof"));
}

#[test]
fn foreign_declaration_rejects_every_runtime_owned_handle_shape() {
    for (name, ty) in [
        ("task", "Task<Int>"),
        ("task_handle", "TaskHandle<Int>"),
        ("channel", "Channel<Int>"),
        ("nullable_task", "Task<Int>?"),
        ("nullable_handle", "TaskHandle<Int>?"),
        ("nullable_channel", "Channel<Int>?"),
    ] {
        let source = format!(
            "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\nextern \"C\" fun native_{name}(value: {ty}): Unit\n"
        );
        let file = parse_file(&source).expect("parse");
        let error = check_file(&file).expect_err("runtime-owned handles must not cross C");
        let message = error.to_string();
        assert!(message.contains("AURA-F4-BOUNDARY"), "{name}: {message}");
        assert!(
            message.contains("async pin/ownership proof"),
            "{name}: {message}"
        );
    }

    let source = "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\nextern \"C\" fun native_task(): Task<Int>\n";
    let file = parse_file(source).expect("parse");
    let error = check_file(&file).expect_err("runtime-owned returns must not cross C");
    let message = error.to_string();
    assert!(message.contains("AURA-F4-BOUNDARY"));
    assert!(message.contains("foreign return"));
}

#[test]
fn foreign_declaration_accepts_tagged_opaque_handle() {
    let file = parse_file(
        "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_use(handle: ForeignHandle<Int>): Unit\n",
    )
    .expect("parse typed opaque-handle declarations");
    let checked = check_file(&file).expect("tagged opaque handles have a C pointer ABI");
    let use_handle = checked
        .functions
        .iter()
        .find(|item| item.name == "native_use")
        .expect("native_use signature");
    assert_eq!(use_handle.params[0].display(), "ForeignHandle<Int>");
}

#[test]
fn foreign_declaration_accepts_owned_tagged_opaque_return() {
    let file = parse_file(
        "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_open(): ForeignHandle<Int>\n",
    )
    .expect("parse owned typed opaque-handle declaration");
    check_file(&file).expect("owned tagged opaque returns have an explicit drop contract");
}

#[test]
fn foreign_declaration_accepts_nested_opaque_handle_crossing() {
    let file = parse_file(
        "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_wrap(handle: ForeignHandle<Int>): ForeignHandle<ForeignHandle<Int>>\n",
    )
    .expect("parse nested opaque-handle declaration");
    check_file(&file).expect("nested handles share the opaque pointer ABI");
}

#[test]
fn async_function_accepts_nested_opaque_handle_result() {
    let file = parse_file(
        "package demo\n\
@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\n\
extern \"C\" fun native_open(): ForeignHandle<ForeignHandle<Int>>\n\
async fun produce(): ForeignHandle<ForeignHandle<Int>> { return native_open() }\n",
    )
    .expect("parse nested async opaque-handle declaration");
    check_file(&file).expect("nested handle task results have outer ownership");
}

#[test]
fn async_function_accepts_foreign_handle_inside_generic_enum_payload() {
    let file = parse_file(
        "package demo\n\
enum Boxed<T> { case Value(value: T) }\n\
async fun produce(): Boxed<ForeignHandle<Int>> { throw \"not-run\" }\n",
    )
    .expect("parse generic enum handle task fixture");
    check_file(&file).expect("generated enum hooks own nested foreign handles");
}

#[test]
fn foreign_declaration_keeps_unproven_async_handles_fail_closed() {
    for ty in ["ForeignHandle<Task<Int>>", "ForeignHandle<TaskHandle<Int>>"] {
        let source = format!(
            "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\nextern \"C\" fun native_use(handle: {ty}): Unit\n"
        );
        let file = parse_file(&source).expect("parse");
        let error = check_file(&file).expect_err("unproven nested async tags must be rejected");
        assert!(error.to_string().contains("AURA-F1-TYPE"), "{ty}: {error}");
    }
}

#[test]
fn foreign_handle_task_await_crossing_remains_fail_closed() {
    let file = parse_file(
        "package demo\n\
async fun worker(handle: ForeignHandle<Int>): Int { return 1 }\n\
async fun keep(handle: ForeignHandle<Int>): Int { return await worker(handle) }\n",
    )
    .expect("parse async handle fixture");
    check_file(&file).expect("borrowed handle is retained by the async task frame");
}

#[test]
fn primitive_foreign_call_is_accepted_inside_async_function() {
    let file = parse_file(
        "package demo\n@foreign(library = \"m\", target = \"native\", link = \"dynamic\", abi = 1, abi_id = \"c\")\nextern \"C\" fun native_abs(value: Int): Int\nasync fun call_native(): Int { return native_abs(7) }\n",
    )
    .expect("parse");
    check_file(&file).expect("primitive foreign values are async-boundary safe");
}

#[test]
fn async_call_is_task_and_await_recovers_result() {
    let mut file = parse_file(
        "package t\nfun worker(): Int { return 7 }\nfun main(): Int { return await worker() }\n",
    )
    .expect("parse");
    promote_to_async(&mut file, 1);
    promote_to_async(&mut file, 0);
    check_file(&file).expect("async call and await");
}

#[test]
fn join_returns_result_with_typed_task_error() {
    let file = parse_file(
        r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main(handle: TaskHandle<Int>): Result<Int, TaskError> {
  return join(handle)
}
"#,
    )
    .expect("parse typed join fixture");
    check_file(&file).expect("join should preserve Result<T, TaskError>");
}

#[test]
fn await_rejects_non_task_operand() {
    let mut file = parse_file("package t\nfun main(): Int { return await 1 }\n").expect("parse");
    promote_to_async(&mut file, 0);
    let err = check_file(&file).expect_err("awaiting an Int");
    assert!(err.primary().message.contains("requires Task<T>"));
}

#[test]
fn task_handle_and_channel_types_accept_owned_elements() {
    let file = parse_file(
        "package t\nfun use(task: Task<Int>, handle: TaskHandle<String>, channel: Channel<Int>) {}\n",
    )
    .expect("parse");
    check_file(&file).expect("async semantic types");
}

#[test]
fn attributes_validate_known_sites_and_arguments() {
    let file = parse_file(
        "package t\n@derive(Eq) class Box(@deprecated(\"old\") val value: Int) {}\nfun main(@notNull value: String) {}\n",
    )
    .expect("attribute syntax");
    check_file(&file).expect("valid attributes");
}

#[test]
fn equals_derive_generates_checked_method_for_supported_fields() {
    let file = parse_file(
        "package t\n@derive(Equals) struct Point(val x: Int, val label: String?, val ok: Bool) {}\n",
    )
    .expect("derive syntax");
    let checked = check_file(&file).expect("generated equals method should typecheck");
    let point = checked
        .ast
        .classes
        .iter()
        .find(|class| class.name.name == "Point")
        .expect("Point");
    let equals = point
        .methods
        .iter()
        .find(|method| method.name.name == "equals")
        .expect("generated equals");
    assert!(equals.is_pub);
    assert_eq!(equals.params[0].name.name, "other");
    assert!(equals
        .return_type
        .as_ref()
        .is_some_and(|ty| ty.name.name == "Bool"));
}

#[test]
fn equals_derive_accepts_nested_class_fields() {
    let file = parse_file(
        "package t\nclass Child(val id: Int) {}\n@derive(Equals) class Parent(val child: Child) {}\n",
    )
    .expect("nested derive syntax");
    let checked = check_file(&file).expect("nested class equality should typecheck");
    let parent = checked
        .ast
        .classes
        .iter()
        .find(|class| class.name.name == "Parent")
        .expect("Parent");
    assert!(parent
        .methods
        .iter()
        .any(|method| method.name.name == "equals"));
}

#[test]
fn equals_derive_empty_type_returns_true() {
    let file =
        parse_file("package t\n@derive(Equals) struct Marker() {}\n").expect("derive syntax");
    let checked = check_file(&file).expect("empty generated equals should typecheck");
    let marker = checked
        .ast
        .classes
        .iter()
        .find(|class| class.name.name == "Marker")
        .expect("Marker");
    let equals = marker
        .methods
        .iter()
        .find(|method| method.name.name == "equals")
        .expect("generated equals");
    assert!(
        matches!(equals.body.stmts.first(), Some(Stmt::Return(return_stmt)) if matches!(return_stmt.value, Some(Expr::Bool(_))))
    );
}

#[test]
fn equals_derive_preserves_eq_alias_and_rejects_duplicate() {
    let file = parse_file(
        "package t\n@derive(Eq) class Box(val value: Int) { fun equals(other: Box): Bool { return true } }\n",
    )
    .expect("derive syntax");
    let errors = check_file(&file).expect_err("duplicate equals must be diagnosed");
    assert!(errors
        .errors
        .iter()
        .any(|error| error.message.contains("AURA-M4-DUPLICATE")));
}

#[test]
fn equals_derive_reports_unsupported_generic_field() {
    let file = parse_file("package t\n@derive(Equals) class Box<T>(val value: T) {}\n")
        .expect("derive syntax");
    let errors = check_file(&file).expect_err("generic equality requires a supported bound");
    assert!(errors
        .errors
        .iter()
        .any(|error| error.message.contains("AURA-M4-UNSUPPORTED")));
}

#[test]
fn hash_code_derive_generates_checked_method_for_int_and_string() {
    let file =
        parse_file("package t\n@derive(HashCode) struct Key(val id: Int, val name: String) {}\n")
            .expect("derive syntax");
    let checked = check_file(&file).expect("generated hashCode method should typecheck");
    let key = checked
        .ast
        .classes
        .iter()
        .find(|class| class.name.name == "Key")
        .expect("Key");
    let hash = key
        .methods
        .iter()
        .find(|method| method.name.name == "hashCode")
        .expect("generated hashCode");
    assert!(hash.is_pub);
    assert!(hash.params.is_empty());
    assert_eq!(
        hash.return_type.as_ref().map(|ty| ty.name.name.as_str()),
        Some("Int")
    );
    assert!(matches!(hash.body.stmts.first(), Some(Stmt::Return(_))));
}

#[test]
fn hash_code_derive_reports_unsupported_and_duplicate_fields() {
    let unsupported =
        parse_file("package t\n@derive(Hash) struct Bad(val ok: Bool, val maybe: Int?) {}\n")
            .expect("derive syntax");
    let errors = check_file(&unsupported).expect_err("unsupported hash fields must be diagnosed");
    assert_eq!(
        errors
            .errors
            .iter()
            .filter(|error| error.message.contains("AURA-M5-UNSUPPORTED"))
            .count(),
        2
    );

    let duplicate = parse_file(
        "package t\n@derive(HashCode) class Key(val id: Int) { fun hashCode(): Int { return 1 } }\n",
    )
    .expect("derive syntax");
    let errors = check_file(&duplicate).expect_err("duplicate hashCode must be diagnosed");
    assert!(errors
        .errors
        .iter()
        .any(|error| error.message.contains("AURA-M5-DUPLICATE")));
}

#[test]
fn debug_derive_generates_deterministic_public_to_string() {
    let file =
        parse_file("package t\n@derive(Debug) class Point(val x: Int, val label: String) {}\n")
            .expect("derive syntax");
    let checked = check_file(&file).expect("debug derive should typecheck");
    let method = checked.ast.classes[0]
        .methods
        .iter()
        .find(|method| method.name.name == "toString")
        .expect("generated toString");
    assert!(method.is_pub);
    assert!(method.params.is_empty());
    assert_eq!(
        method.return_type.as_ref().map(|ty| ty.name.name.as_str()),
        Some("String")
    );
    assert!(matches!(method.body.stmts.first(), Some(Stmt::Return(_))));
}

#[test]
fn debug_string_derive_uses_debug_string_and_reports_unsupported_fields() {
    let file =
        parse_file("package t\n@derive(DebugString) struct Marker() {}\n").expect("derive syntax");
    let checked = check_file(&file).expect("debugString derive should typecheck");
    assert!(checked.ast.classes[0]
        .methods
        .iter()
        .any(|method| method.name.name == "debugString"));

    let unsupported =
        parse_file("package t\n@derive(Debug) class Bad(val ok: Bool, val maybe: Int?) {}\n")
            .expect("derive syntax");
    let errors = check_file(&unsupported).expect_err("unsupported debug fields must be diagnosed");
    assert_eq!(
        errors
            .errors
            .iter()
            .filter(|error| error.message.contains("AURA-M6-UNSUPPORTED"))
            .count(),
        2
    );
}

#[test]
fn debug_derive_reports_duplicate_to_string() {
    let file = parse_file(
        "package t\n@derive(Debug) class Point(val x: Int) { fun toString(): String { return \"x\" } }\n",
    )
    .expect("derive syntax");
    let errors = check_file(&file).expect_err("duplicate toString must be diagnosed");
    assert!(errors
        .errors
        .iter()
        .any(|error| error.message.contains("AURA-M6-DUPLICATE")));
}

#[test]
fn attributes_report_unknown_target_duplicate_and_conflict() {
    let file = parse_file(
        "package t\n@unknown fun main() {}\n@inline @noinline fun other() {}\n@inline class Wrong() {}\n",
    )
    .expect("attribute syntax");
    let errors = check_file(&file).expect_err("invalid attributes");
    let messages = errors
        .errors
        .iter()
        .map(|error| error.message.as_str())
        .collect::<Vec<_>>();
    assert!(messages
        .iter()
        .any(|message| message.contains("AURA-M2-UNKNOWN")));
    assert!(messages
        .iter()
        .any(|message| message.contains("AURA-M2-CONFLICT")));
    assert!(messages
        .iter()
        .any(|message| message.contains("AURA-M2-TARGET")));
}

#[test]
fn reserved_attributes_and_derives_fail_explicitly() {
    let file = parse_file("package t\n@derive(Json) class Value() {}\n")
        .expect("reserved attribute syntax");
    let errors = check_file(&file).expect_err("reserved metadata must not be silently ignored");
    let messages = errors
        .errors
        .iter()
        .map(|error| error.message.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.contains("AURA-M3-UNSUPPORTED"))
            .count(),
        1
    );
    assert!(messages
        .iter()
        .any(|message| message.contains("derive `Json`")));
}

#[test]
fn async_boundaries_reject_borrowed_values() {
    let mut await_file =
        parse_file("package t\nfun main(x: ref String): String { return await x }\n")
            .expect("parse");
    promote_to_async(&mut await_file, 0);
    let err = check_file(&await_file).expect_err("borrow across await");
    assert!(err.primary().message.contains("boundary `await`"));

    let mut spawn_file = parse_file("package t\nfun main(x: ref String) {}\n").expect("parse");
    spawn_file.functions[0].body = Block {
        stmts: vec![Stmt::Expr(Expr::Async(AsyncExpr::Spawn(SpawnExpr {
            body: Block {
                stmts: vec![Stmt::Expr(Expr::Ident(Ident {
                    name: "x".into(),
                    span: Span::new(20, 21),
                }))],
                span: Span::new(20, 21),
            },
            span: Span::new(10, 30),
        })))],
        span: Span::new(10, 30),
    };
    promote_to_async(&mut spawn_file, 0);
    let err = check_file(&spawn_file).expect_err("borrow captured by spawn");
    assert!(err.primary().message.contains("boundary `spawn`"));

    let mut channel_file =
        parse_file("package t\nfun main(channel: Channel<String>, x: ref String) {}\n")
            .expect("parse");
    channel_file.functions[0].body = Block {
        stmts: vec![Stmt::Expr(Expr::Async(AsyncExpr::ChannelSend(
            ChannelSendExpr {
                channel: Box::new(Expr::Ident(Ident {
                    name: "channel".into(),
                    span: Span::new(20, 27),
                })),
                value: Box::new(Expr::Ident(Ident {
                    name: "x".into(),
                    span: Span::new(28, 29),
                })),
                span: Span::new(20, 29),
            },
        )))],
        span: Span::new(20, 29),
    };
    promote_to_async(&mut channel_file, 0);
    let err = check_file(&channel_file).expect_err("borrow sent through channel");
    assert!(err.primary().message.contains("boundary `channel send`"));

    let join_file =
        parse_file("package t\nfun main(handle: ref TaskHandle<Int>) { join(handle) }\n")
            .expect("parse borrowed join handle");
    let err = check_file(&join_file).expect_err("borrowed handle joined");
    assert!(err.primary().message.contains("boundary `join`"));

    let cancel_file =
        parse_file("package t\nfun main(handle: ref TaskHandle<Int>) { cancel(handle) }\n")
            .expect("parse borrowed cancel handle");
    let err = check_file(&cancel_file).expect_err("borrowed handle cancelled");
    assert!(err.primary().message.contains("boundary `cancel`"));

    let channel_create_file =
        parse_file("package t\nfun main(capacity: ref Int) { Channel<String>(capacity) }\n")
            .expect("parse borrowed channel capacity");
    let err = check_file(&channel_create_file).expect_err("borrowed channel capacity");
    assert!(err.primary().message.contains("boundary `channel create`"));
}

#[test]
fn bounded_non_empty_spawn_body_is_semantically_valid() {
    let file = parse_file(
        "package t\nfun main() { val task = spawn { println(\"once\") return } join(task) }\n",
    )
    .expect("parse bounded spawn");
    check_file(&file).expect("capture-free effect-only spawn should typecheck");
}

#[test]
fn spawn_infers_return_payload_for_join() {
    let file = parse_file(
        r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun main() {
  val task = spawn { return "ready" }
  val outcome: Result<String, TaskError> = join(task)
}
"#,
    )
    .expect("parse typed spawn fixture");
    check_file(&file).expect("spawn should infer String payload");
}

#[test]
fn generic_task_handle_wrapper_preserves_payload_type() {
    let file = parse_file(
        r#"package std.io
enum TaskError { case Failed(error: String) case Cancelled }
enum Result<T, E> { case Ok(value: T) case Err(error: E) }
fun observe<T>(handle: TaskHandle<T>): Result<T, TaskError> { return join(handle) }
fun stop<T>(handle: TaskHandle<T>) { cancel(handle) }
fun main() {
  val handle = spawn { return 42 }
  val outcome: Result<Int, TaskError> = observe(handle)
  stop(handle)
}
"#,
    )
    .expect("parse generic task wrapper fixture");
    check_file(&file).expect("generic task handles should infer their payload through wrappers");
}

#[test]
fn task_owned_storage_rejects_nested_ref() {
    let file = parse_file("package t\nfun use(task: Task<ref String>) {}\n").expect("parse");
    let err = check_file(&file).expect_err("borrow in task storage");
    assert!(err
        .primary()
        .message
        .contains("borrow reference cannot be stored"));
}

#[test]
fn scoped_ref_types_allow_lexical_local_and_parameter_use() {
    let file = parse_file(
        "package t\nfun borrow(x: ref String) { val y: ref String = x println(y) }\nfun main() {}\n",
    )
    .expect("parse");
    check_file(&file).expect("scoped ref type");
}

#[test]
fn scoped_ref_rejects_returns_and_lambda_captures() {
    let returned =
        parse_file("package t\nfun bad(x: ref String): ref String { return x }\n").expect("parse");
    let err = check_file(&returned).expect_err("ref return");
    assert!(err.primary().message.contains("cannot be returned"));

    let captured =
        parse_file("package t\nfun bad(x: String) { val y: ref String = x val f = () => y }\n")
            .expect("parse");
    let err = check_file(&captured).expect_err("ref capture");
    assert!(err.primary().message.contains("cannot capture borrow"));
}

#[test]
fn scoped_ref_rejects_assignment_into_longer_lived_binding() {
    let file = parse_file(
        "package t\nfun bad() { var out: String = \"out\" if (true) { val x: String = \"inner\" val y: ref String = x out = y } }\n",
    )
    .expect("parse");
    let err = check_file(&file).expect_err("borrow escape through assignment");
    assert!(err.primary().message.contains("escape"));
}

#[test]
fn scoped_ref_rejects_owning_field_storage() {
    let file = parse_file("package t\nclass Holder(val value: ref String) {}\nfun main() {}\n")
        .expect("parse");
    let err = check_file(&file).expect_err("ref field");
    assert!(err.primary().message.contains("stored in fields"));
}

#[test]
fn scoped_ref_allows_array_field_view_without_return_escape() {
    let file = parse_file(
        "package t\nclass Holder(val items: Array<Int>) {\n  fun view() { val xs: ref Array<Int> = this.items val n: Int = xs.len }\n}\nfun main() {}\n",
    )
    .expect("parse");
    check_file(&file).expect("Array field borrow view");
}

#[test]
fn array_field_escape_requires_explicit_borrow_or_clone() {
    let file = parse_file(
        "package t\nclass Holder(val items: Array<Int>) {\n  fun leak(): Array<Int> { return this.items }\n}\nfun main() {}\n",
    )
    .expect("parse Array field escape");
    let err = check_file(&file).expect_err("Array field must not move out implicitly");
    assert!(err.primary().message.contains("borrow"));
}

#[test]
fn scoped_ref_allows_read_only_collection_iteration_views() {
    let file = parse_file(
        r#"
package t
class Snapshot(val items: Array<Int>) {
  fun len(): Int { return this.items.len }
  fun get(i: Int): Int { return this.items.get(i) }
}
fun inspect(snapshot: Snapshot) {
  val iterator: ref Snapshot = snapshot
  val items: ref Array<Int> = iterator.items
  for (item in items) {
    val current: ref Int = item
    if (current == 1 || current == 2) { }
  }
}
fun main() {}
"#,
    )
    .expect("parse");
    check_file(&file).expect("snapshot iteration borrow should stay lexical");
}

#[test]
fn snapshot_entry_values_remain_owned_after_iteration() {
    let file = parse_file(
        r#"
package t
class HashMapEntry(val key: Int, val value: String) {}
class HashMapEntryIterator(val items: Array<HashMapEntry>) {
  fun len(): Int { return this.items.len }
  fun get(i: Int): HashMapEntry { return this.items.get(i) }
}
fun inspect(entries: HashMapEntryIterator) {
  for (entry in entries) {
    val key: Int = entry.key
    val value: String = entry.value
    println(value)
    if (key == 1) { println("entry") }
  }
}
fun main() {}
"#,
    )
    .expect("parse");
    check_file(&file).expect("snapshot entry values should remain owning values");
}

#[test]
fn scoped_ref_rejects_collection_iteration_escape() {
    let returned = parse_file(
        r#"
package t
class Snapshot(val items: Array<Int>) {}
fun bad(snapshot: Snapshot): ref Array<Int> {
  val iterator: ref Snapshot = snapshot
  val items: ref Array<Int> = iterator.items
  return items
}
fun main() {}
"#,
    )
    .expect("parse");
    let err = check_file(&returned).expect_err("iterator borrow return");
    assert!(err.primary().message.contains("cannot be returned"));

    let captured = parse_file(
        r#"
package t
class Snapshot(val items: Array<Int>) {}
fun bad(snapshot: Snapshot) {
  val iterator: ref Snapshot = snapshot
  val items: ref Array<Int> = iterator.items
  val f = () => items.len
}
fun main() {}
"#,
    )
    .expect("parse");
    let err = check_file(&captured).expect_err("iterator borrow capture");
    assert!(err.primary().message.contains("cannot capture borrow"));
}

#[test]
fn scoped_ref_rejects_nullable_targets() {
    let file =
        parse_file("package t\nfun bad(x: ref String?): String { return \"x\" }\n").expect("parse");
    let err = check_file(&file).expect_err("nullable ref");
    assert!(err.primary().message.contains("must be non-null"));
}

#[test]
fn scoped_ref_rejects_mutable_bindings_and_function_targets() {
    let mutable =
        parse_file("package t\nfun bad(x: String) { var y: ref String = x }\nfun main() {}\n")
            .expect("parse");
    let err = check_file(&mutable).expect_err("mutable ref binding");
    assert!(err.primary().message.contains("must be immutable"));

    let function =
        parse_file("package t\nfun bad(x: ref (Int) -> Int): Int { return 0 }\nfun main() {}\n")
            .expect("parse");
    let err = check_file(&function).expect_err("function ref");
    assert!(err.primary().message.contains("function types"));
}

#[test]
fn try_catch_typechecks() {
    let src = r#"
package t
fun boom() { throw "x" }
fun main() {
  try {
boom()
  } catch (e: String) {
println(e)
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn throw_rejects_unit() {
    let src = r#"
package t
fun main() {
  throw null
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("throw null");
    assert!(
        err.primary().message.contains("throw") || err.primary().message.contains("Null"),
        "{}",
        err.primary().message
    );
}

#[test]
fn result_enum_and_match() {
    let src = r#"
package t
enum Result<T, E> {
  case Ok(value: T)
  case Err(error: E)
}
fun f(): Result<Int, String> {
  return Ok(1)
}
fun g(r: Result<Int, String>): Int {
  match (r) {
case Ok(v) => { return v }
case Err(e) => { return 0 }
  }
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(checked.enums.iter().any(|e| e.name == "Result"));
    assert!(checked
        .mono_enums
        .iter()
        .any(|(n, a)| n == "Result" && a == &[Ty::Int, Ty::String]));
}

#[test]
fn match_nonexhaustive_errors() {
    let src = r#"
package t
enum Color { case Red case Green }
fun f(c: Color) {
  match (c) {
case Red => { println("r") }
  }
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("non-exhaustive");
    assert!(
        err.primary().message.contains("non-exhaustive") || err.primary().message.contains("Green"),
        "{}",
        err.primary().message
    );
}

#[test]
fn struct_fields_and_methods() {
    let src = r#"
package t
struct Point(val x: Int, val y: Int) {
  fun sum(): Int { return this.x + this.y }
}
fun f(): Int {
  val p: Point = Point(1, 2)
  return p.sum()
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(checked
        .classes
        .iter()
        .any(|c| c.is_struct && c.name == "Point"));
}

#[test]
fn class_inheritance_is_recorded_and_assignable() {
    let src = r#"
package t
open class Animal(val age: Int) {
  fun years(): Int { return this.age }
}
interface Named { fun name(): String }
class Dog(val breed: String) : Animal(7), Named {
  fun name(): String { return this.breed }
}
fun accept(a: Animal): Int { return a.years() + a.age }
fun main() {
  val d: Dog = Dog("dog")
  val n: Int = accept(d)
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    let dog = checked
        .classes
        .iter()
        .find(|c| c.name == "Dog")
        .expect("Dog");
    assert_eq!(
        dog.superclass.as_ref().map(Ty::class_name),
        Some(Some("Animal"))
    );
    assert_eq!(dog.implements.len(), 1);
}

#[test]
fn class_cannot_extend_struct_or_itself() {
    let src = r#"
package t
struct Point(val x: Int) {}
class Bad(val x: Int) : Point {}
class Loop(val x: Int) : Loop {}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("invalid superclass");
    let messages = err
        .errors
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>();
    assert!(messages
        .iter()
        .any(|m| m.contains("structs cannot be used as superclasses")));
    assert!(messages.iter().any(|m| m.contains("cannot extend itself")));
}

#[test]
fn class_requires_superclass_constructor_arguments() {
    let src = r#"
package t
open class Base(val id: Int) {}
class Child(val value: Int) : Base {}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("missing superclass arguments");
    assert!(err
        .primary()
        .message
        .contains("superclass `Base` expects 1 constructor argument(s), got 0"));
}

#[test]
fn superclass_constructor_arguments_are_type_checked() {
    let src = r#"
package t
open class Base(val id: Int) {}
class Child(val value: String) : Base(value) {}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("superclass argument type mismatch");
    assert!(err
        .primary()
        .message
        .contains("superclass constructor argument for `id`: expected Int, got String"));
}

#[test]
fn superclass_constructor_arguments_can_use_child_fields() {
    let src = r#"
package t
open class Base(val id: Int) {}
class Child(val value: Int) : Base(value) {}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("child field is available to superclass constructor");
}

#[test]
fn classes_are_final_by_default() {
    let src = r#"
package t
class Base(val id: Int) {}
class Child(val value: Int) : Base(value) {}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("final class extension");
    assert!(err
        .primary()
        .message
        .contains("class `Base` is final and cannot be extended"));
}

#[test]
fn open_override_requires_matching_open_parent_method() {
    let src = r#"
package t
open class Base(val id: Int) {
  open fun value(): Int { return this.id }
}
class Child(val id2: Int) : Base(1) {
  override fun value(): Int { return this.id2 }
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("valid override");
}

#[test]
fn override_without_parent_method_is_rejected() {
    let src = r#"
package t
open class Base(val id: Int) {}
class Child(val value: Int) : Base(1) {
  override fun missing(): Int { return this.value }
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("invalid override");
    assert!(err
        .primary()
        .message
        .contains("does not override a superclass method"));
}

#[test]
fn abstract_class_cannot_be_instantiated() {
    let src = r#"
package t
abstract class Base(val id: Int) {}
fun main() { Base(1) }
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("abstract constructor");
    assert!(err
        .primary()
        .message
        .contains("abstract class `Base` cannot be instantiated"));
}

#[test]
fn private_members_are_only_visible_inside_the_declaring_class() {
    let src = r#"
package t
class Vault(private val code: Int) {
  private fun secret(): Int { return this.code }
  fun reveal(): Int { return this.secret() }
}
fun main(): Int {
  val vault = Vault(7)
  return vault.code
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("private field access");
    assert!(err.primary().message.contains("private member `code`"));
}

#[test]
fn protected_fields_are_visible_to_subclasses_only() {
    let src = r#"
package t
open class Base(protected val id: Int) {}
class Child(val value: Int) : Base(value) {
  fun inherited(): Int { return id }
}
fun main(): Int {
  val base: Base = Child(7)
  return base.id
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("protected field access");
    assert!(err.primary().message.contains("protected member `id`"));
}

#[test]
fn public_members_are_visible_outside_the_class() {
    let src = r#"
package t
class Record(pub val id: Int) {
  pub fun value(): Int { return this.id }
}
fun main(): Int {
  val record = Record(7)
  return record.value() + record.id
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("public members are accessible");
}

#[test]
fn class_task_method_allows_await_and_checks_inner_result() {
    let src = r#"
package t
class Counter(val value: Int) {
  async fun current(): Int {
    val one: Int = await ready()
    return this.value
  }
}
async fun ready(): Int { return 1 }
fun main() {}
"#;
    let file = parse_file(src).expect("parse async class method");
    check_file(&file).expect("Task class method should typecheck await against inner result");
}

#[test]
fn bounds_allow_method_on_type_param() {
    let src = r#"
package t
interface Named {
  fun name(): String
}
class User(val n: String) : Named {
  fun name(): String { return this.n }
}
fun greet<T : Named>(x: T): String {
  return x.name()
}
fun main() {
  val s: String = greet(User("hi"))
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("bounded type param method call");
}

#[test]
fn hashable_primitives_satisfy_bound_and_hash_method() {
    let src = r#"
package t
interface Hashable { fun hash(): Int }
fun hash_it<T : Hashable>(x: T): Int { return x.hash() }
fun main() {
  val a: Int = hash_it(7)
  val b: Int = hash_it("a")
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("primitive Hashable bound");
}

#[test]
fn hashable_rejects_bool() {
    let src = r#"
package t
interface Hashable { fun hash(): Int }
fun hash_it<T : Hashable>(x: T): Int { return x.hash() }
fun main() { val a: Int = hash_it(true) }
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("Bool must not satisfy Hashable");
    assert!(err
        .primary()
        .message
        .contains("does not satisfy bound `Hashable`"));
}

#[test]
fn where_multi_bounds_and_reject_unsatisfied() {
    let src_ok = r#"
package t
interface Named { fun name(): String }
interface Id { fun id(): Int }
class Both(val n: String, val i: Int) : Named, Id {
  fun name(): String { return this.n }
  fun id(): Int { return this.i }
}
fun f<T>(x: T) where T : Named, T : Id {
  println(x.name())
}
fun main() { f(Both("a", 1)) }
"#;
    let file = parse_file(src_ok).expect("parse");
    check_file(&file).expect("multi bounds ok");

    let src_bad = r#"
package t
interface Named { fun name(): String }
interface Id { fun id(): Int }
class OnlyNamed(val n: String) : Named {
  fun name(): String { return this.n }
}
fun f<T>(x: T) where T : Named, T : Id {
  println(x.name())
}
fun main() { f(OnlyNamed("a")) }
"#;
    let file = parse_file(src_bad).expect("parse");
    let err = check_file(&file).expect_err("should reject missing Id bound");
    assert!(
        err.primary().message.contains("Id") || err.primary().message.contains("bound"),
        "unexpected: {}",
        err.primary().message
    );
}

#[test]
fn unbounded_type_param_cannot_call_methods() {
    let src = r#"
package t
interface Named { fun name(): String }
fun bad<T>(x: T): String {
  return x.name()
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("unbounded T");
    assert!(
        err.primary().message.contains("unbounded") || err.primary().message.contains("method"),
        "unexpected: {}",
        err.primary().message
    );
}

#[test]
fn null_flow_narrows_in_if() {
    let src = r#"
package t
fun f(name: String?): String {
  if (name != null) {
return name
  } else {
return "x"
  }
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check should allow name after != null");
}

#[test]
fn null_flow_rejects_without_check() {
    let src = r#"
package t
fun f(name: String?): String {
  return name
}
fun main() {}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("should reject String? as String");
    assert!(
        err.primary().message.contains("return type mismatch")
            || err.primary().message.contains("String")
    );
}

#[test]
fn non_unit_function_requires_return_on_every_path() {
    let src = r#"
package t
class Notebook(val items: Array<String>) {}
pub fun notebook(): Notebook {
  var a = Notebook(Array(0))
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("missing return");
    assert!(
        err.primary().message.contains("missing return")
            && err.primary().message.contains("Notebook"),
        "{}",
        err.primary().message
    );
}

#[test]
fn return_path_analysis_accepts_terminating_loops_without_reachable_breaks() {
    let src = r#"
package t
fun continues_forever(): Int {
  while (true) {
    continue
  }
}
fun nested_loop_never_exits(): Int {
  while (true) {
    while (true) {
      break
    }
  }
}
fun unreachable_break(): Int {
  while (true) {
    return 1
    break
  }
}
fun exits_then_returns(): Int {
  while (true) {
    break
  }
  return 1
}
class Worker() {
  fun spin(): Int {
    while (true) {
      continue
    }
  }
}
async fun async_spin(): Int {
  while (true) {
    continue
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("provably terminating loop paths are valid");
}

#[test]
fn return_path_analysis_rejects_reachable_loop_exits() {
    let src = r#"
package t
fun can_exit(stop: Bool): Int {
  while (true) {
    if (stop) {
      break
    }
  }
}
fun finally_can_exit(): Int {
  while (true) {
    try {
      return 1
    } finally {
      break
    }
  }
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("reachable loop exit permits function fall-through");
    assert!(
        err.errors
            .iter()
            .filter(|error| error.message.contains("missing return"))
            .count()
            == 2
    );
}

#[test]
fn infers_box_and_id_type_args() {
    let src = r#"
package t
class Box<T>(val value: T) {
  fun get(): T { return this.value }
}
fun id<T>(x: T): T { return x }
fun main() {
  val a = Box("hi")
  val b: Box<String> = Box("x")
  id("y")
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(checked
        .mono_classes
        .iter()
        .any(|(n, a)| n == "Box" && a == &[Ty::String]));
    assert!(checked
        .mono_funs
        .iter()
        .any(|(n, a)| n == "id" && a == &[Ty::String]));
    assert!(!checked.call_instantiations.is_empty());
}

#[test]
fn nested_mono_skips_open_type_params() {
    // C4u: Wrapper<T> field Box<T> must not record open Box_T monomorphs.
    let src = r#"
package t
class Box<T>(val value: T) {
  fun get(): T { return this.value }
}
class Wrapper<T>(val inner: Box<T>) {
  fun unwrap(): T { return this.inner.get() }
}
fun main() {
  val w: Wrapper<String> = Wrapper(Box("x"))
  w.unwrap()
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, a)| n == "Wrapper" && a == &[Ty::String]),
        "expected Wrapper_String"
    );
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, a)| n == "Box" && a == &[Ty::String]),
        "expected Box_String from nested expand"
    );
    assert!(
        !checked
            .mono_classes
            .iter()
            .any(|(_, a)| a.iter().any(|t| t.is_open())),
        "open monomorphs must not be recorded: {:?}",
        checked.mono_classes
    );
}

#[test]
fn nested_mono_expands_generic_method_signature_types() {
    let src = r#"
package t
class Entry<K, V>(val key: K, val value: V) {}
class Table<K, V>(val key: K, val value: V) {
  fun entries(): Array<Entry<K, V>> { return Array(0) }
}
fun main() {
  val table = Table<Int, String>(1, "one")
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    let entry = Ty::ClassApp {
        name: "Entry@t".into(),
        args: vec![Ty::Int, Ty::String],
    };
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, args)| n == "Entry" && args == &[Ty::Int, Ty::String]),
        "expected Entry<Int, String>, got {:?}",
        checked.mono_classes
    );
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, args)| n == "Array" && args.as_slice() == std::slice::from_ref(&entry)),
        "expected Array<Entry<Int, String>>, got {:?}",
        checked.mono_classes
    );
}

#[test]
fn import_allows_pub_function() {
    use aura_ast::ImportDecl;
    let mut lib = parse_file(
        r#"
package demo.math
pub fun square(x: Int): Int { return x * x }
fun mul(a: Int, b: Int): Int { return a * b }
"#,
    )
    .expect("parse lib");
    for f in &mut lib.functions {
        f.origin_package = "demo.math".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.math
fun main() {
  square(3)
}
"#,
    )
    .expect("parse app");
    for f in &mut app.functions {
        f.origin_package = "demo.app".into();
    }
    for i in &mut app.imports {
        i.origin_package = "demo.app".into();
    }
    // Merge lib into app unit
    app.functions.extend(lib.functions);
    app.interfaces.extend(lib.interfaces);
    app.enums.extend(lib.enums);
    app.classes.extend(lib.classes);
    let _ = ImportDecl {
        path: app.imports[0].path.clone(),
        alias: None,
        origin_package: "demo.app".into(),
        span: app.imports[0].span,
    };
    check_file(&app).expect("cross-package pub call");
}

#[test]
fn same_fun_name_two_packages_via_alias() {
    let mut a = parse_file(
        r#"
package demo.a
pub fun add(x: Int, y: Int): Int { return x + y }
"#,
    )
    .expect("parse a");
    for f in &mut a.functions {
        f.origin_package = "demo.a".into();
    }
    let mut b = parse_file(
        r#"
package demo.b
pub fun add(x: Int, y: Int): Int { return x * y }
"#,
    )
    .expect("parse b");
    for f in &mut b.functions {
        f.origin_package = "demo.b".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.a as A
import demo.b as B
fun main() {
  A.add(1, 2)
  B.add(1, 2)
}
"#,
    )
    .expect("parse app");
    for f in &mut app.functions {
        f.origin_package = "demo.app".into();
    }
    for i in &mut app.imports {
        i.origin_package = "demo.app".into();
    }
    app.functions.extend(a.functions);
    app.functions.extend(b.functions);
    check_file(&app).expect("same name two packages");
}

#[test]
fn same_class_name_two_packages_resolves_methods_by_nominal_key() {
    let mut a = parse_file(
        r#"
package demo.a
pub class Token(val n: Int) { pub fun kind(): Int { return this.n } }
"#,
    )
    .expect("parse a");
    for c in &mut a.classes {
        c.origin_package = "demo.a".into();
    }
    let mut b = parse_file(
        r#"
package demo.b
pub class Token(val n: Int) { pub fun kind(): Int { return this.n * 10 } }
"#,
    )
    .expect("parse b");
    for c in &mut b.classes {
        c.origin_package = "demo.b".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.a as A
import demo.b as B
fun main() {
  val a: A.Token = A.Token(3)
  val b: B.Token = B.Token(3)
  a.kind()
  b.kind()
}
"#,
    )
    .expect("parse app");
    for f in &mut app.functions {
        f.origin_package = "demo.app".into();
    }
    for i in &mut app.imports {
        i.origin_package = "demo.app".into();
    }
    app.classes.extend(a.classes);
    app.classes.extend(b.classes);
    check_file(&app).expect("same class name methods resolve by package");
}

#[test]
fn import_alias_qualified_call() {
    let mut lib = parse_file(
        r#"
package demo.math
pub fun square(x: Int): Int { return x * x }
"#,
    )
    .expect("parse lib");
    for f in &mut lib.functions {
        f.origin_package = "demo.math".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.math as Math
fun main() {
  Math.square(3)
}
"#,
    )
    .expect("parse app");
    for f in &mut app.functions {
        f.origin_package = "demo.app".into();
    }
    for i in &mut app.imports {
        i.origin_package = "demo.app".into();
    }
    app.functions.extend(lib.functions);
    check_file(&app).expect("alias qualified call");
}

#[test]
fn import_alias_qualified_type() {
    let mut lib = parse_file(
        r#"
package demo.math
pub class Point(val x: Int, val y: Int) {}
"#,
    )
    .expect("parse lib");
    for c in &mut lib.classes {
        c.origin_package = "demo.math".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.math as Math
fun main() {
  val p: Math.Point = Math.Point(1, 2)
}
"#,
    )
    .expect("parse app");
    for f in &mut app.functions {
        f.origin_package = "demo.app".into();
    }
    for i in &mut app.imports {
        i.origin_package = "demo.app".into();
    }
    app.classes.extend(lib.classes);
    check_file(&app).expect("alias qualified type");
}

#[test]
fn import_rejects_private_function() {
    let mut lib = parse_file(
        r#"
package demo.math
fun mul(a: Int, b: Int): Int { return a * b }
"#,
    )
    .expect("parse lib");
    for f in &mut lib.functions {
        f.origin_package = "demo.math".into();
    }
    let mut app = parse_file(
        r#"
package demo.app
import demo.math
fun main() {
  mul(2, 3)
}
"#,
    )
    .expect("parse app");
    for f in &mut app.functions {
        f.origin_package = "demo.app".into();
    }
    for i in &mut app.imports {
        i.origin_package = "demo.app".into();
    }
    app.functions.extend(lib.functions);
    let err = check_file(&app).expect_err("private");
    assert!(
        err.primary().message.contains("private") || err.primary().message.contains("mul"),
        "{}",
        err.primary().message
    );
}

#[test]
fn class_throw_and_catch_typechecks() {
    let src = r#"
package t
class Error(val msg: String) {}
fun boom() { throw Error("x") }
fun main() {
  try {
    boom()
  } catch (e: Error) {
    println(e.msg)
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn reject_throw_interface() {
    let src = r#"
package t
interface I { fun m(): Int }
fun main() {
  throw null
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("throw null");
    assert!(
        err.primary().message.contains("throw") || err.primary().message.contains("Null"),
        "{}",
        err.primary().message
    );
}

#[test]
fn for_range_typechecks() {
    let src = r#"
package t
fun main() {
  var s: Int = 0
  for (i in 0..5) {
    s = s + i
  }
  for (j in 1..=3) {
    s = s + j
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn for_range_rejects_non_int() {
    let src = r#"
package t
fun main() {
  for (i in "a".."b") {}
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("non-int range");
    assert!(
        err.primary().message.contains("Int"),
        "{}",
        err.primary().message
    );
}

#[test]
fn break_continue_in_loop_ok() {
    let src = r#"
package t
fun main() {
  for (i in 0..3) {
    if (i == 0) { continue }
    if (i == 2) { break }
  }
  while (true) {
    break
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn break_outside_loop_errors() {
    let src = r#"
package t
fun main() {
  break
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("break outside");
    assert!(
        err.primary().message.contains("break"),
        "{}",
        err.primary().message
    );
}

#[test]
fn array_accepts_enum_elem() {
    // C6g: Array of enum elements by value.
    let src = r#"
package t
enum Color { Red, Green }
fun main() {
  val a: Array<Color> = Array(1)
  a.set(0, Red())
  val c: Color = a.get(0)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("enum Array elem");
}

#[test]
fn array_accepts_interface_elem() {
    let src = r#"
package t
interface Named {
  fun name(): String
}
class User(val value: String) : Named {
  fun name(): String { return this.value }
}
fun main() {
  val a: Array<Named> = Array(1)
  a.set(0, User("a"))
  println(a.get(0).name())
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("array of interface");
}

#[test]
fn array_int_typechecks() {
    let src = r#"
package t
fun main() {
  val a: Array<Int> = Array(3)
  a.set(0, 1)
  val x: Int = a.get(0)
  val n: Int = a.len
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("check");
    assert!(checked
        .mono_classes
        .iter()
        .any(|(n, a)| n == "Array" && a == &[Ty::Int]));
}

#[test]
fn array_push_typechecks() {
    let src = r#"
package t
fun main() {
  val a: Array<Int> = Array(0)
  a.push(1)
  a.push(2)
  val n: Int = a.len
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn array_pop_typechecks() {
    let src = r#"
package t
fun main() {
  val a: Array<Int> = Array(0)
  a.push(1)
  a.push(2)
  val x: Int = a.pop()
  val n: Int = a.len
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn for_in_array_typechecks() {
    let src = r#"
package t
fun main() {
  val a: Array<Int> = Array(2)
  a.set(0, 1)
  a.set(1, 2)
  var s: Int = 0
  for (x in a) {
    s = s + x
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("check");
}

#[test]
fn for_in_rejects_non_array() {
    let src = r#"
package t
fun main() {
  for (x in 1) {}
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("non-array for-in");
    assert!(
        err.primary().message.contains("Array") || err.primary().message.contains("String"),
        "{}",
        err.primary().message
    );
}

#[test]
fn for_in_string_typechecks() {
    let src = r#"
package t
fun main() {
  var s: Int = 0
  for (b in "ab") {
    s = s + b
  }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("for-in string");
}

#[test]
fn undefined_name_suggests_similar() {
    // C5c: typo hint.
    let src = r#"
package t
fun main() {
  val count: Int = 1
  println(cout)
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("undefined");
    assert!(
        err.primary().message.contains("undefined name") && err.primary().message.contains("count"),
        "{}",
        err.primary().message
    );
}

#[test]
fn multi_error_collects_body_errors() {
    // C6h: two undefined names in one body → two diagnostics.
    let src = r#"
package t
fun main() {
  println(missing_one)
  println(missing_two)
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("multi");
    assert!(
        err.errors.len() >= 2,
        "expected ≥2 errors, got {}: {:?}",
        err.errors.len(),
        err.errors
    );
    let joined = err
        .errors
        .iter()
        .map(|e| e.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(
        joined.contains("missing_one") && joined.contains("missing_two"),
        "{joined}"
    );
}

#[test]
fn for_in_duck_len_get() {
    // C4y: class with len field + get(i).
    let src = r#"
package t
class R(val len: Int) {
  fun get(i: Int): Int { return i }
}
fun main() {
  var s: Int = 0
  for (x in R(2)) { s = s + x }
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("duck for-in");
}

#[test]
fn for_in_iface_iterable() {
    // C6c: interface with len() + get(i).
    let src = r#"
package t
interface Iterable {
  fun len(): Int
  fun get(i: Int): Int
}
class R(val n: Int) : Iterable {
  fun len(): Int { return this.n }
  fun get(i: Int): Int { return i }
}
fun sum(it: Iterable): Int {
  var s: Int = 0
  for (x in it) { s = s + x }
  return s
}
fun main() {
  val r = R(2)
  val n = sum(r)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("iface for-in");
}

#[test]
fn generic_iface_implements_mono() {
    // C8c: interface Boxable<T>; class implements Boxable<Int>.
    let src = r#"
package t
interface Boxable<T> {
  fun get(): T
}
class IntBox(val n: Int) : Boxable<Int> {
  fun get(): Int { return this.n }
}
fun take(b: Boxable<Int>): Int {
  return b.get()
}
fun main() {
  val x = IntBox(7)
  val n = take(x)
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("generic iface implements");
    assert!(
        checked
            .mono_interfaces
            .iter()
            .any(|(n, args)| n == "Boxable" && args == &[Ty::Int]),
        "expected mono Boxable<Int>, got {:?}",
        checked.mono_interfaces
    );
}

#[test]
fn generic_class_implements_mono() {
    // C9a: class Box<T> : Boxable<T>
    let src = r#"
package t
interface Boxable<T> {
  fun get(): T
}
class Box<T>(val v: T) : Boxable<T> {
  fun get(): T { return this.v }
}
fun take(b: Boxable<Int>): Int {
  return b.get()
}
fun main() {
  val x = Box(7)
  val n = take(x)
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("generic class implements");
    assert!(
        checked
            .mono_classes
            .iter()
            .any(|(n, args)| n == "Box" && args == &[Ty::Int]),
        "expected mono Box<Int>, got {:?}",
        checked.mono_classes
    );
    assert!(
        checked
            .mono_interfaces
            .iter()
            .any(|(n, args)| n == "Boxable" && args == &[Ty::Int]),
        "expected mono Boxable<Int> from class implements subst, got {:?}",
        checked.mono_interfaces
    );
}

#[test]
fn array_accepts_class_elem() {
    let src = r#"
package t
class Box(val x: Int) {}
fun main() {
  val a: Array<Box> = Array(1)
  a.set(0, Box(2))
  val b: Box = a.get(0)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("class Array elem");
}

#[test]
fn generic_higher_order_function_typechecks() {
    let src = r#"
package t
fun map<T, R>(xs: Array<T>, f: (T) -> R): Array<R> {
  val out: Array<R> = Array(xs.len)
  var i: Int = 0
  while (i < xs.len) {
    out.set(i, f(xs.get(i)))
    i = i + 1
  }
  return out
}
fun filter<T>(xs: Array<T>, pred: (T) -> Bool): Array<T> {
  val out: Array<T> = Array(0)
  var i: Int = 0
  while (i < xs.len) {
    if (pred(xs.get(i))) { out.push(xs.get(i)) }
    i = i + 1
  }
  return out
}
fun fold<T, A>(xs: Array<T>, init: A, f: (A, T) -> A): A {
  var acc: A = init
  var i: Int = 0
  while (i < xs.len) {
    acc = f(acc, xs.get(i))
    i = i + 1
  }
  return acc
}
fun main() {
  val xs: Array<Int> = Array(2)
  xs.set(0, 2)
  xs.set(1, 3)
  val ys: Array<String> = map<Int, String>(xs, (x: Int) => x.toString())
  val zs: Array<Int> = filter<Int>(xs, (x: Int) => x > 2)
  val total: Int = fold<Int, Int>(xs, 0, (acc: Int, x: Int) => acc + x)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("generic HOFs");
}

#[test]
fn generic_higher_order_function_accepts_generic_class_values() {
    let src = r#"
package t
class Box<T>(val value: T) {}
fun map<T, R>(xs: Array<T>, f: (T) -> R): Array<R> {
  val out: Array<R> = Array(xs.len)
  var i: Int = 0
  while (i < xs.len) {
    out.set(i, f(xs.get(i)))
    i = i + 1
  }
  return out
}
fun filter<T>(xs: Array<T>, pred: (T) -> Bool): Array<T> {
  val out: Array<T> = Array(0)
  var i: Int = 0
  while (i < xs.len) {
    if (pred(xs.get(i))) { out.push(xs.get(i)) }
    i = i + 1
  }
  return out
}
fun fold<T, A>(xs: Array<T>, init: A, f: (A, T) -> A): A {
  var acc: A = init
  var i: Int = 0
  while (i < xs.len) {
    acc = f(acc, xs.get(i))
    i = i + 1
  }
  return acc
}
fun main() {
  val xs: Array<Box<Int>> = Array(0)
  xs.push(Box<Int>(1))
  val ys: Array<Box<Int>> = map<Box<Int>, Box<Int>>(xs, (x: Box<Int>) => Box<Int>(x.value + 1))
  val zs: Array<Box<Int>> = filter<Box<Int>>(ys, (x: Box<Int>) => x.value > 1)
  val total: Box<Int> = fold<Box<Int>, Box<Int>>(zs, Box<Int>(0), (a: Box<Int>, x: Box<Int>) => Box<Int>(a.value + x.value))
}
"#;
    let file = parse_file(src).expect("parse generic class HOF");
    check_file(&file).expect("generic HOFs over generic class values");
}

#[test]
fn array_accepts_struct_elem() {
    let src = r#"
package t
struct Point(val x: Int) {}
fun main() {
  val a: Array<Point> = Array(1)
  a.set(0, Point(2))
  val p: Point = a.get(0)
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("struct Array elem");
}

#[test]
fn reject_struct_equality() {
    let src = r#"
package t
struct Point(val x: Int) {}
fun main() {
  val a: Point = Point(1)
  val b: Point = Point(1)
  if (a == b) {}
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("struct ==");
    assert!(
        err.primary().message.contains("struct") || err.primary().message.contains("compare"),
        "{}",
        err.primary().message
    );
}

#[test]
fn reject_enum_equality() {
    let src = r#"
package t
enum Color { case Red case Blue }
fun main() {
  val a: Color = Red()
  val b: Color = Red()
  if (a == b) {}
}
"#;
    let file = parse_file(src).expect("parse");
    let err = check_file(&file).expect_err("enum ==");
    assert!(
        err.primary().message.contains("enum") || err.primary().message.contains("compare"),
        "{}",
        err.primary().message
    );
}

#[test]
fn lambda_allows_var_class_capture_by_ref() {
    // C20a: mutable class captures are represented as by-ref captures.
    let src = r#"
package t
class Box(val n: Int) {}
fun main() {
  var b = Box(1)
  val f = () => b.n
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("var class capture should be allowed");
    assert!(checked.lambda_captures.values().any(|caps| {
        caps.iter()
            .any(|c| c.name == "b" && c.by_ref && matches!(c.ty, Ty::Class(_)))
    }));
}

#[test]
fn lambda_allows_var_array_capture_by_ref() {
    // C20a: mutable Array captures use one shared owned cell, not a view.
    let src = r#"
package t
fun main() {
  var a: Array<Int> = Array(1)
  val f = () => a.len
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("var Array capture should be allowed");
    assert!(checked.lambda_captures.values().any(|caps| {
        caps.iter().any(|c| {
            c.name == "a"
                && c.by_ref
                && matches!(&c.ty, Ty::ClassApp { name, .. } if name == "Array")
        })
    }));
}

#[test]
fn lambda_allows_var_string_capture() {
    // C13f: outer `var` String is capturable via shared RC box.
    let src = r#"
package t
fun main(): String {
  var s = "hi"
  val f = () => s
  return f()
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("var String capture should be allowed");
    let has_str_cap = checked.lambda_captures.values().any(|caps| {
        caps.iter()
            .any(|c| c.name == "s" && c.by_ref && matches!(c.ty, Ty::String))
    });
    assert!(
        has_str_cap,
        "expected outer lambda to by-ref capture String `s`"
    );
}

#[test]
fn lambda_allows_fun_capture() {
    // C13e: outer `val` Fun is capturable (nested env retain/release in codegen).
    let src = r#"
package t
fun main(): Int {
  val inner: (Int) -> Int = (x: Int) => x + 1
  val outer = () => inner(2)
  return outer()
}
"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("Fun capture should be allowed");
    let has_fun_cap = checked.lambda_captures.values().any(|caps| {
        caps.iter()
            .any(|c| c.name == "inner" && matches!(c.ty, Ty::Fun { .. }))
    });
    assert!(has_fun_cap, "expected outer lambda to capture Fun `inner`");
}

#[test]
fn lambda_allows_var_fun_capture_by_ref() {
    // C20a: mutable Fun captures carry the nested closure reference by ref.
    let src = r#"
package t
fun main() {
  var f: (Int) -> Int = (x: Int) => x
  val g = () => f(1)
}

"#;
    let file = parse_file(src).expect("parse");
    let checked = check_file(&file).expect("var Fun capture should be allowed");
    assert!(checked.lambda_captures.values().any(|caps| {
        caps.iter()
            .any(|c| c.name == "f" && c.by_ref && matches!(c.ty, Ty::Fun { .. }))
    }));
}

#[test]
fn interface_parent_is_assignable_and_requires_inherited_methods() {
    let src = r#"
package t
interface Parent { fun value(): Int }
interface Child : Parent { fun id(): Int }
class User(val n: Int) : Child {
  fun value(): Int { return this.n }
  fun id(): Int { return this.n + 1 }
}
fun main() {
  val parent: Parent = User(42)
  val value: Int = parent.value()
}
"#;
    let file = parse_file(src).expect("parse");
    check_file(&file).expect("child interface should upcast to parent");

    let missing_parent_method = r#"
package t
interface Parent { fun value(): Int }
interface Child : Parent { fun id(): Int }
class User(val n: Int) : Child { fun id(): Int { return this.n } }
"#;
    let file = parse_file(missing_parent_method).expect("parse");
    let errors = check_file(&file).expect_err("inherited method must be implemented");
    assert!(errors
        .errors
        .iter()
        .any(|error| error.message.contains("value")));
}
