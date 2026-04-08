use std::collections::{HashMap, HashSet};

use aura_ast::{Program, Stmt, TopLevel};
use aura_diagnostics::Diagnostic;

mod check;
mod env;
mod lib_utils;
mod types;

pub use env::VarInfo;
pub use types::{ClassInfo, InterfaceInfo, MethodSig, Ty, TypedProgram};

use crate::check::{typeck_block, typeck_stmt};
use crate::env::Env;
use crate::lib_utils::ident_text;
use crate::types::{ty_from_type_ref, MethodSig as TypeMethodSig, TyDefKind, TypePosition};

pub fn typeck_program(source: &str, program: &Program) -> (Vec<Diagnostic>, TypedProgram) {
    let mut diags = Vec::new();

    let mut type_defs = HashMap::<String, TyDefKind>::new();
    for item in &program.items {
        match item {
            TopLevel::Class(class_decl) => {
                if let Some(name) = ident_text(source, &class_decl.name) {
                    type_defs.insert(name.to_string(), TyDefKind::Class);
                }
            }
            TopLevel::Interface(iface_decl) => {
                if let Some(name) = ident_text(source, &iface_decl.name) {
                    type_defs.insert(name.to_string(), TyDefKind::Interface);
                }
            }
            _ => {}
        }
    }

    let mut interfaces = HashMap::<String, InterfaceInfo>::new();
    for item in &program.items {
        let TopLevel::Interface(iface_decl) = item else {
            continue;
        };
        let Some(name) = ident_text(source, &iface_decl.name) else {
            continue;
        };
        let mut methods = HashMap::<String, TypeMethodSig>::new();
        for method in &iface_decl.methods {
            let Some(method_name) = ident_text(source, &method.name) else {
                continue;
            };
            let mut params = Vec::new();
            for param in &method.params {
                let ty = ty_from_type_ref(
                    source,
                    &param.ty,
                    TypePosition::Value,
                    &type_defs,
                    &mut diags,
                );
                params.push(ty);
            }
            let return_ty = method
                .return_type
                .as_ref()
                .map(|t| ty_from_type_ref(source, t, TypePosition::Return, &type_defs, &mut diags))
                .unwrap_or(Ty::Void);
            methods
                .entry(method_name.to_string())
                .or_insert(TypeMethodSig { params, return_ty });
        }
        interfaces
            .entry(name.to_string())
            .or_insert(InterfaceInfo { methods });
    }

    #[derive(Clone)]
    struct RawClassInfo {
        extends: Option<(String, aura_span::Span)>,
        fields: Vec<(String, Ty, aura_span::Span)>,
        methods: HashMap<String, (TypeMethodSig, aura_span::Span)>,
        implements: HashSet<String>,
        span: aura_span::Span,
    }

    let mut raw_classes = HashMap::<String, RawClassInfo>::new();
    for item in &program.items {
        let TopLevel::Class(class_decl) = item else {
            continue;
        };
        let Some(name) = ident_text(source, &class_decl.name) else {
            continue;
        };
        let extends = class_decl
            .extends
            .as_ref()
            .and_then(|ty| ident_text(source, &ty.name).map(|name| (name.to_string(), ty.span)));

        let mut fields = Vec::<(String, Ty, aura_span::Span)>::new();
        for field in &class_decl.fields {
            let Some(field_name) = ident_text(source, &field.name) else {
                continue;
            };
            let ty = ty_from_type_ref(
                source,
                &field.ty,
                TypePosition::Value,
                &type_defs,
                &mut diags,
            );
            fields.push((field_name.to_string(), ty, field.name.span));
        }
        let mut methods = HashMap::<String, (TypeMethodSig, aura_span::Span)>::new();
        for method in &class_decl.methods {
            let Some(method_name) = ident_text(source, &method.name) else {
                continue;
            };
            let mut params = Vec::new();
            for param in &method.params {
                let ty = ty_from_type_ref(
                    source,
                    &param.ty,
                    TypePosition::Value,
                    &type_defs,
                    &mut diags,
                );
                params.push(ty);
            }
            let return_ty = method
                .return_type
                .as_ref()
                .map(|t| ty_from_type_ref(source, t, TypePosition::Return, &type_defs, &mut diags))
                .unwrap_or(Ty::Void);
            methods.insert(
                method_name.to_string(),
                (TypeMethodSig { params, return_ty }, method.span),
            );
        }
        let mut implements = HashSet::new();
        for impl_ref in &class_decl.implements {
            let ty = ty_from_type_ref(
                source,
                impl_ref,
                TypePosition::Value,
                &type_defs,
                &mut diags,
            );
            match ty {
                Ty::Interface(iname) => {
                    implements.insert(iname);
                }
                Ty::Class(cname) => {
                    diags.push(
                        Diagnostic::error(
                            impl_ref.span,
                            format!("cannot implement class `{cname}`"),
                        )
                        .with_help("use `extends` for class inheritance"),
                    );
                }
                Ty::Unknown => {}
                _ => {
                    diags.push(Diagnostic::error(
                        impl_ref.span,
                        format!("cannot implement non-interface type `{}`", ty.name()),
                    ));
                }
            }
        }
        raw_classes.entry(name.to_string()).or_insert(RawClassInfo {
            extends,
            fields,
            methods,
            implements,
            span: class_decl.span,
        });
    }

    fn resolve_class_info(
        class_name: &str,
        raw_classes: &HashMap<String, RawClassInfo>,
        type_defs: &HashMap<String, TyDefKind>,
        diags: &mut Vec<Diagnostic>,
        cache: &mut HashMap<String, ClassInfo>,
        stack: &mut Vec<String>,
    ) -> ClassInfo {
        if let Some(info) = cache.get(class_name) {
            return info.clone();
        }
        if stack.iter().any(|n| n == class_name) {
            diags.push(Diagnostic::error(
                raw_classes
                    .get(class_name)
                    .map(|c| c.span)
                    .unwrap_or(aura_span::Span::empty(aura_span::BytePos::new(0))),
                format!("cyclic inheritance involving `{class_name}`"),
            ));
            return ClassInfo {
                extends: None,
                fields: HashMap::new(),
                field_order: Vec::new(),
                methods: HashMap::new(),
                implements: HashSet::new(),
            };
        }

        let Some(raw) = raw_classes.get(class_name) else {
            return ClassInfo {
                extends: None,
                fields: HashMap::new(),
                field_order: Vec::new(),
                methods: HashMap::new(),
                implements: HashSet::new(),
            };
        };

        stack.push(class_name.to_string());

        let mut extends = None;
        let mut fields: HashMap<String, Ty> = HashMap::new();
        let mut field_order = Vec::new();
        let mut methods: HashMap<String, TypeMethodSig> = HashMap::new();
        let mut implements = raw.implements.clone();

        if let Some((parent_name, parent_span)) = &raw.extends {
            match type_defs.get(parent_name) {
                Some(TyDefKind::Class) => {
                    let parent = resolve_class_info(
                        parent_name,
                        raw_classes,
                        type_defs,
                        diags,
                        cache,
                        stack,
                    );
                    extends = Some(parent_name.clone());
                    for field_name in &parent.field_order {
                        if let Some(ty) = parent.fields.get(field_name) {
                            fields.insert(field_name.clone(), ty.clone());
                            field_order.push(field_name.clone());
                        }
                    }
                    for (mname, msig) in &parent.methods {
                        methods.insert(mname.clone(), msig.clone());
                    }
                    implements.extend(parent.implements.iter().cloned());
                }
                Some(TyDefKind::Interface) => {
                    diags.push(Diagnostic::error(
                        *parent_span,
                        format!("class `{class_name}` cannot extend interface `{parent_name}`"),
                    ));
                }
                None => {
                    diags.push(Diagnostic::error(
                        *parent_span,
                        format!("unknown class `{parent_name}`"),
                    ));
                }
            }
        }

        for (field_name, ty, span) in &raw.fields {
            if fields.contains_key(field_name.as_str()) {
                diags.push(Diagnostic::error(
                    *span,
                    format!("field `{field_name}` is already defined in an ancestor class"),
                ));
                continue;
            }
            fields.insert(field_name.clone(), ty.clone());
            field_order.push(field_name.clone());
        }

        for (method_name, (sig, method_span)) in &raw.methods {
            if let Some(parent_sig) = methods.get(method_name) {
                if parent_sig != sig {
                    diags.push(Diagnostic::error(
                        *method_span,
                        format!(
                            "method `{method_name}` in class `{class_name}` does not match inherited signature"
                        ),
                    )
                    .with_help("keep overridden method signatures identical"));
                }
            }
            methods.insert(method_name.clone(), sig.clone());
        }

        let resolved = ClassInfo {
            extends,
            fields,
            field_order,
            methods,
            implements,
        };
        cache.insert(class_name.to_string(), resolved.clone());
        stack.pop();
        resolved
    }

    let mut classes = HashMap::<String, ClassInfo>::new();
    let mut resolving_stack = Vec::new();
    for class_name in raw_classes.keys().cloned().collect::<Vec<_>>() {
        let info = resolve_class_info(
            &class_name,
            &raw_classes,
            &type_defs,
            &mut diags,
            &mut classes,
            &mut resolving_stack,
        );
        classes.insert(class_name, info);
    }

    // Pass 1.5: Verify class implements all interface methods
    for (class_name, class_info) in &classes {
        for iname in &class_info.implements {
            let Some(iface_info) = interfaces.get(iname) else {
                continue;
            };
            for (mname, isig) in &iface_info.methods {
                let Some(csig) = class_info.methods.get(mname) else {
                    diags.push(
                        Diagnostic::error(
                            program
                                .items
                                .iter()
                                .find_map(|item| {
                                    if let TopLevel::Class(c) = item {
                                        if ident_text(source, &c.name) == Some(class_name) {
                                            return Some(c.name.span);
                                        }
                                    }
                                    None
                                })
                                .unwrap_or(aura_span::Span::empty(aura_span::BytePos::new(0))),
                            format!(
                                "class `{class_name}` misses method `{mname}` from interface `{iname}`"
                            ),
                        )
                        .with_help(format!(
                            "implement `function {mname}(...): {}`",
                            isig.return_ty.name()
                        )),
                    );
                    continue;
                };

                if csig.params != isig.params || csig.return_ty != isig.return_ty {
                    diags.push(
                        Diagnostic::error(
                            program
                                .items
                                .iter()
                                .find_map(|item| {
                                    if let TopLevel::Class(c) = item {
                                        if ident_text(source, &c.name) == Some(class_name) {
                                            return c.methods.iter().find(|m| {
                                                ident_text(source, &m.name) == Some(mname)
                                            }).map(|m| m.span);
                                        }
                                    }
                                    None
                                })
                                .unwrap_or(aura_span::Span::empty(aura_span::BytePos::new(0))),
                            format!(
                                "method `{mname}` in class `{class_name}` does not match interface `{iname}`"
                            ),
                        )
                        .with_help("ensure parameter and return types match exactly"),
                    );
                }
            }
        }
    }

    let mut globals = HashMap::<String, VarInfo>::new();
    let mut top_level_functions = HashMap::<String, MethodSig>::new();

    // Register built-in functions
    let println_sig = MethodSig {
        params: vec![Ty::String],
        return_ty: Ty::Void,
    };
    globals.insert(
        "println".to_string(),
        VarInfo {
            ty: Ty::Function(Box::new(println_sig.clone())),
            mutable: false,
            decl_span: aura_span::Span::empty(aura_span::BytePos::new(0)),
        },
    );
    top_level_functions.insert("println".to_string(), println_sig);

    for item in &program.items {
        match item {
            TopLevel::Stmt(Stmt::Let(s) | Stmt::Const(s)) => {
                let Some(name) = ident_text(source, &s.name) else {
                    continue;
                };
                globals.entry(name.to_string()).or_insert_with(|| VarInfo {
                    ty: Ty::Unknown,
                    mutable: matches!(item, TopLevel::Stmt(Stmt::Let(_))),
                    decl_span: s.name.span,
                });
            }
            TopLevel::Function(func) => {
                let Some(name) = ident_text(source, &func.name) else {
                    continue;
                };
                let mut params = Vec::new();
                for param in &func.params {
                    let ty = ty_from_type_ref(
                        source,
                        &param.ty,
                        TypePosition::Value,
                        &type_defs,
                        &mut diags,
                    );
                    params.push(ty);
                }
                let return_ty = func
                    .return_type
                    .as_ref()
                    .map(|t| {
                        ty_from_type_ref(source, t, TypePosition::Return, &type_defs, &mut diags)
                    })
                    .unwrap_or(Ty::Void);
                let sig = TypeMethodSig { params, return_ty };
                globals.insert(
                    name.to_string(),
                    VarInfo {
                        ty: Ty::Function(Box::new(sig.clone())),
                        mutable: false,
                        decl_span: func.name.span,
                    },
                );
                top_level_functions.insert(name.to_string(), sig);
            }
            _ => {}
        }
    }

    let mut global_env = Env::new(globals);

    let mut expression_types = HashMap::new();
    // Pass 2: type-check all top-level statements (including global var initializers).
    for item in &program.items {
        let TopLevel::Stmt(stmt) = item else { continue };
        let expected = Ty::Void;
        typeck_stmt(
            source,
            &mut global_env,
            &expected,
            &type_defs,
            &classes,
            &interfaces,
            None,
            false,
            stmt,
            &mut expression_types,
            &mut diags,
        );
    }

    // Pass 3: type-check function bodies.
    for item in &program.items {
        let TopLevel::Function(func) = item else {
            continue;
        };

        let mut env = global_env.child();
        for param in &func.params {
            let Some(name) = ident_text(source, &param.name) else {
                continue;
            };
            let ty = ty_from_type_ref(
                source,
                &param.ty,
                TypePosition::Value,
                &type_defs,
                &mut diags,
            );
            expression_types.insert(param.span, ty.clone());
            expression_types.insert(param.name.span, ty.clone());

            env.declare(
                name.to_string(),
                VarInfo {
                    ty,
                    mutable: true,
                    decl_span: param.name.span,
                },
            );
        }

        let expected_return = func
            .return_type
            .as_ref()
            .map(|t| ty_from_type_ref(source, t, TypePosition::Return, &type_defs, &mut diags))
            .unwrap_or(Ty::Void);

        typeck_block(
            source,
            &mut env,
            &expected_return,
            &type_defs,
            &classes,
            &interfaces,
            None,
            false,
            &func.body,
            &mut expression_types,
            &mut diags,
        );

        if expected_return != Ty::Void && !crate::check::block_guarantees_return(&func.body.stmts) {
            diags.push(
                Diagnostic::error(func.span, "missing return on some paths")
                    .with_help("add a `return` statement on all control-flow paths"),
            );
        }
    }

    // Pass 4: type-check class methods (field access via `this`).
    for item in &program.items {
        let TopLevel::Class(class_decl) = item else {
            continue;
        };
        let class_name = ident_text(source, &class_decl.name);
        for method in &class_decl.methods {
            let is_constructor = matches!(ident_text(source, &method.name), Some("constructor"));

            let mut env = global_env.child();
            for param in &method.params {
                let Some(name) = ident_text(source, &param.name) else {
                    continue;
                };
                let ty = ty_from_type_ref(
                    source,
                    &param.ty,
                    TypePosition::Value,
                    &type_defs,
                    &mut diags,
                );
                expression_types.insert(param.span, ty.clone());
                expression_types.insert(param.name.span, ty.clone());

                env.declare(
                    name.to_string(),
                    VarInfo {
                        ty,
                        mutable: true,
                        decl_span: param.name.span,
                    },
                );
            }

            if let Some(cname) = class_name {
                expression_types.insert(method.span, Ty::Class(cname.to_string()));
            }

            let mut expected_return = method
                .return_type
                .as_ref()
                .map(|t| ty_from_type_ref(source, t, TypePosition::Return, &type_defs, &mut diags))
                .unwrap_or(Ty::Void);

            if is_constructor && expected_return != Ty::Void {
                let span = method
                    .return_type
                    .as_ref()
                    .map(|t| t.span)
                    .unwrap_or(method.span);
                diags.push(
                    Diagnostic::error(span, "constructor must return `void`")
                        .with_help("use `void` or omit the annotation"),
                );
                expected_return = Ty::Void;
            }

            typeck_block(
                source,
                &mut env,
                &expected_return,
                &type_defs,
                &classes,
                &interfaces,
                class_name,
                true, // Relaxed: fields can be mutated in any method
                &method.body,
                &mut expression_types,
                &mut diags,
            );

            if expected_return != Ty::Void
                && !crate::check::block_guarantees_return(&method.body.stmts)
            {
                diags.push(
                    Diagnostic::error(method.span, "missing return on some paths")
                        .with_help("add a `return` statement on all control-flow paths"),
                );
            }
        }
    }

    // De-duplicate diagnostics by span and message
    let mut unique_diags = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for diag in diags {
        let key = (
            diag.span.start.raw(),
            diag.span.end.raw(),
            diag.message.clone(),
        );
        if seen.insert(key) {
            unique_diags.push(diag);
        }
    }

    (
        unique_diags,
        TypedProgram {
            classes,
            interfaces,
            functions: top_level_functions,
            expression_types,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_builtin_types() {
        let src = r#"
function f(a: i32, b: string): void { return; }
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn reports_unknown_type() {
        let src = r#"
function f(a: Foo): i32 { return 0; }
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unknown type"));
    }

    #[test]
    fn rejects_void_outside_return_position() {
        let src = r#"
function f(a: void): i32 { return 0; }
let x: void = 0;
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 2, "{diags:#?}");
        assert!(diags
            .iter()
            .all(|d| d.message.contains("only valid as a function return type")));
    }

    #[test]
    fn infers_let_type_from_initializer_and_allows_assignment() {
        let src = r#"
function main(): void {
  let x = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_missing_type_and_initializer() {
        let src = r#"
function main(): void {
  let x;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0]
            .message
            .contains("type annotation or an initializer"));
    }

    #[test]
    fn allows_widening_assignment_to_annotated_type() {
        let src = r#"
function main(): void {
  let x: i64 = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_type_mismatch_in_initializer() {
        let src = r#"
function main(): void {
  let x: i32 = 1.0;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("type mismatch"));
    }

    #[test]
    fn rejects_assignment_to_const() {
        let src = r#"
function main(): void {
  const x: i32 = 1;
  x = 2;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("cannot assign to `const`"));
    }

    #[test]
    fn rejects_missing_return_on_some_paths() {
        let src = r#"
function f(): i32 {
  if (true) { return 1; }
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("missing return"));
    }

    #[test]
    fn rejects_return_type_mismatch() {
        let src = r#"
function f(): i32 {
  return true;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("type mismatch"));
    }

    #[test]
    fn rejects_return_value_in_void_function() {
        let src = r#"
function f(): void {
  return 1;
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert_eq!(diags.len(), 1, "{diags:#?}");
        assert!(diags[0].message.contains("cannot return a value"));
    }

    #[test]
    fn constructor_assigns_fields_without_diag() {
        let src = r#"
class Point {
  x: i32;
  y: i32;

  function constructor(x: i32, y: i32): void {
    this.x = x;
    this.y = y;
  }
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn allows_this_assignment_outside_constructor() {
        let src = r#"
class Point {
  x: i32;

  function mutate(): void {
    this.x = 1;
  }
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_non_void_constructor_return_type() {
        let src = r#"
class Foo {
  x: i32;

  function constructor(): i32 {
    this.x = 0;
    return 1;
  }
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags
            .iter()
            .any(|d| d.message.contains("constructor must return `void`")));
    }

    #[test]
    fn accepts_interface_implementation() {
        let src = r#"
interface Logger {
    function log(msg: string): void;
}

class MyLogger implements Logger {
    function log(msg: string): void {
        // ...
    }
}

let l: Logger = new MyLogger();
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_missing_interface_method() {
        let src = r#"
interface Logger {
    function log(msg: string): void;
}

class MyLogger implements Logger {
    // Missing log
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(
            !diags.is_empty(),
            "Should have diagnostics for missing method"
        );
        assert!(diags
            .iter()
            .any(|d| d.message.contains("misses method `log`")));
    }

    #[test]
    fn rejects_interface_method_signature_mismatch() {
        let src = r#"
interface Logger {
    function log(msg: string): void;
}

class MyLogger implements Logger {
    function log(msg: i32): void { }
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(
            !diags.is_empty(),
            "Should have diagnostics for signature mismatch"
        );
        assert!(diags
            .iter()
            .any(|d| d.message.contains("does not match interface")));
    }

    #[test]
    fn accepts_interface_method_calls() {
        let src = r#"
interface Logger {
    function log(msg: string): void;
}

class MyLogger implements Logger {
    function log(msg: string): void { }
}

function t(l: Logger) {
    l.log("hello");
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn accepts_inherited_method_calls_and_subclass_assignment() {
        let src = r#"
class Animal {
    function speak(): string { return "..."; }
}

class Dog extends Animal {
    function speak(): string { return "woof"; }
}

function t(): void {
    let a: Animal = new Dog();
    println(a.speak());
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }

    #[test]
    fn rejects_interface_instantiation() {
        let src = r#"
interface Logger {
    function log(msg: string): void;
}

let l = new Logger();
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(
            !diags.is_empty(),
            "Should have diagnostics for interface instantiation"
        );
        assert!(diags
            .iter()
            .any(|d| d.message.contains("cannot instantiate interface")));
    }

    #[test]
    fn accepts_try_catch_finally_with_throw() {
        let src = r#"
function f(): void {
    try {
        throw 1;
    } catch (e) {
        return;
    } finally {
        let done = 1;
    }
}
"#;
        let out = aura_parser::parse_program(src);
        assert!(out.errors.is_empty(), "{:#?}", out.errors);

        let (diags, _) = typeck_program(src, &out.value);
        assert!(diags.is_empty(), "{diags:#?}");
    }
}
