//! Expansion for source-level derives.

use aura_ast::{
    Attribute, AttributeArg, AttributeValue, BinOp, Block, CallExpr, ClassDecl, Expr, FieldExpr,
    FunDecl, Ident, IntLit, NominalKind, Param, ReturnStmt, Span, Stmt, StringLit, TypeRef,
};

use crate::error::SemaError;

const EQUALS: &str = "equals";
const HASH_CODE: &str = "hashCode";
const TO_STRING: &str = "toString";
const DEBUG_STRING: &str = "debugString";

fn error(code: &str, message: String, span: Span) -> SemaError {
    SemaError {
        message: format!("[{code}] {message}"),
        span,
    }
}

fn has_equals_derive(attributes: &[Attribute]) -> Option<Span> {
    attributes.iter().find_map(|attribute| {
        if attribute.name.name != "derive" {
            return None;
        }
        attribute.args.iter().find_map(|arg| match arg {
            AttributeArg::Positional(AttributeValue::Ident(name))
                if name.name == "Equals" || name.name == "Eq" =>
            {
                Some(attribute.span)
            }
            _ => None,
        })
    })
}

fn has_hash_derive(attributes: &[Attribute]) -> Option<Span> {
    attributes.iter().find_map(|attribute| {
        if attribute.name.name != "derive" {
            return None;
        }
        attribute.args.iter().find_map(|arg| match arg {
            AttributeArg::Positional(AttributeValue::Ident(name))
                if name.name == "HashCode" || name.name == "Hash" =>
            {
                Some(attribute.span)
            }
            _ => None,
        })
    })
}

fn debug_derive(attributes: &[Attribute]) -> Option<(&'static str, Span)> {
    attributes.iter().find_map(|attribute| {
        if attribute.name.name != "derive" {
            return None;
        }
        attribute.args.iter().find_map(|arg| match arg {
            AttributeArg::Positional(AttributeValue::Ident(name)) if name.name == "Debug" => {
                Some((TO_STRING, attribute.span))
            }
            AttributeArg::Positional(AttributeValue::Ident(name))
                if name.name == "DebugString" =>
            {
                Some((DEBUG_STRING, attribute.span))
            }
            _ => None,
        })
    })
}

fn type_ref_for_class(class: &ClassDecl) -> TypeRef {
    TypeRef {
        qualifier: None,
        name: class.name.clone(),
        type_args: class
            .type_params
            .iter()
            .map(|param| TypeRef {
                qualifier: None,
                name: param.name.clone(),
                type_args: Vec::new(),
                nullable: false,
                reference: false,
                span: param.name.span,
                fun: None,
            })
            .collect(),
        nullable: false,
        reference: false,
        span: class.name.span,
        fun: None,
    }
}

fn field_access(object: Expr, field: &aura_ast::FieldDecl, span: Span) -> Expr {
    Expr::Field(FieldExpr {
        object: Box::new(object),
        field: field.name.clone(),
        safe: false,
        span,
    })
}

fn equality_body(class: &ClassDecl, span: Span) -> Block {
    let mut comparisons = class.fields.iter().map(|field| {
        Expr::Binary(aura_ast::BinaryExpr {
            op: BinOp::Eq,
            left: Box::new(field_access(Expr::This(span), field, span)),
            right: Box::new(field_access(
                Expr::Ident(Ident {
                    name: "other".into(),
                    span,
                }),
                field,
                span,
            )),
            span,
        })
    });
    let value = comparisons.next().map(|first| {
        comparisons.fold(first, |left, right| {
            Expr::Binary(aura_ast::BinaryExpr {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
                span,
            })
        })
    });
    Block {
        stmts: vec![Stmt::Return(ReturnStmt {
            value: Some(value.unwrap_or(Expr::Bool(aura_ast::BoolLit { value: true, span }))),
            span,
        })],
        span,
    }
}

fn hash_call(object: Expr, field: &aura_ast::FieldDecl, span: Span) -> Expr {
    Expr::Call(CallExpr {
        callee: Box::new(Expr::Field(FieldExpr {
            object: Box::new(field_access(object, field, span)),
            field: Ident {
                name: "hash".into(),
                span,
            },
            safe: false,
            span,
        })),
        type_args: Vec::new(),
        args: Vec::new(),
        span,
    })
}

fn hash_body(class: &ClassDecl, span: Span) -> Block {
    let value = class.fields.iter().fold(
        Expr::Int(IntLit { value: 17, span }),
        |seed, field| {
            let multiplied = Expr::Binary(aura_ast::BinaryExpr {
                op: BinOp::Mul,
                left: Box::new(seed),
                right: Box::new(Expr::Int(IntLit { value: 31, span })),
                span,
            });
            Expr::Binary(aura_ast::BinaryExpr {
                op: BinOp::Add,
                left: Box::new(multiplied),
                right: Box::new(hash_call(Expr::This(span), field, span)),
                span,
            })
        },
    );
    Block {
        stmts: vec![Stmt::Return(ReturnStmt {
            value: Some(value),
            span,
        })],
        span,
    }
}

fn supports_field(field: &aura_ast::FieldDecl, classes: &[ClassDecl]) -> bool {
    fn supported(ty: &TypeRef, classes: &[ClassDecl]) -> bool {
        if ty.reference || ty.fun.is_some() || !ty.type_args.is_empty() {
            return false;
        }
        if ty.nullable {
            let mut inner = ty.clone();
            inner.nullable = false;
            return supported(&inner, classes);
        }
        matches!(ty.name.name.as_str(), "Int" | "Bool" | "String")
            || classes.iter().any(|class| {
                class.name.name == ty.name.name && class.kind == NominalKind::Class
            })
    }
    supported(&field.ty, classes)
}

fn supports_hash_field(field: &aura_ast::FieldDecl) -> bool {
    !field.ty.reference
        && field.ty.fun.is_none()
        && field.ty.type_args.is_empty()
        && !field.ty.nullable
        && matches!(field.ty.name.name.as_str(), "Int" | "String")
}

fn supports_debug_field(field: &aura_ast::FieldDecl) -> bool {
    !field.ty.reference
        && field.ty.fun.is_none()
        && field.ty.type_args.is_empty()
        && !field.ty.nullable
        && matches!(field.ty.name.name.as_str(), "Int" | "String")
}

fn int_to_string(object: Expr, span: Span) -> Expr {
    Expr::Call(CallExpr {
        callee: Box::new(Expr::Field(FieldExpr {
            object: Box::new(object),
            field: Ident { name: "toString".into(), span },
            safe: false,
            span,
        })),
        type_args: Vec::new(),
        args: Vec::new(),
        span,
    })
}

fn debug_value(field: &aura_ast::FieldDecl, span: Span) -> Expr {
    let value = field_access(Expr::This(span), field, span);
    if field.ty.name.name == "Int" {
        int_to_string(value, span)
    } else {
        value
    }
}

fn string_add(left: Expr, right: Expr, span: Span) -> Expr {
    Expr::Binary(aura_ast::BinaryExpr {
        op: BinOp::Add,
        left: Box::new(left),
        right: Box::new(right),
        span,
    })
}

fn debug_body(class: &ClassDecl, span: Span) -> Block {
    let mut value = Expr::String(StringLit {
        value: format!("{}(", class.name.name),
        span,
    });
    for (index, field) in class.fields.iter().enumerate() {
        if index != 0 {
            value = string_add(
                value,
                Expr::String(StringLit { value: ", ".into(), span }),
                span,
            );
        }
        value = string_add(
            value,
            Expr::String(StringLit {
                value: format!("{}=", field.name.name),
                span,
            }),
            span,
        );
        value = string_add(value, debug_value(field, span), span);
    }
    value = string_add(
        value,
        Expr::String(StringLit { value: ")".into(), span }),
        span,
    );
    Block {
        stmts: vec![Stmt::Return(ReturnStmt { value: Some(value), span })],
        span,
    }
}

/// Expand the conservative MVP `@derive(Debug)` / `@derive(DebugString)` implementation.
pub(crate) fn expand_debug(file: &mut aura_ast::File) -> Vec<SemaError> {
    let mut errors = Vec::new();
    for class in &mut file.classes {
        let Some((method_name, derive_span)) = debug_derive(&class.attributes) else {
            continue;
        };
        if class.methods.iter().any(|method| method.name.name == method_name) {
            errors.push(error(
                "AURA-M6-DUPLICATE",
                format!(
                    "cannot derive `{}` for `{}`: method `{method_name}` already exists",
                    if method_name == TO_STRING { "Debug" } else { "DebugString" },
                    class.name.name
                ),
                derive_span,
            ));
            continue;
        }
        let mut unsupported = false;
        for field in &class.fields {
            if !supports_debug_field(field) {
                unsupported = true;
                errors.push(error(
                    "AURA-M6-UNSUPPORTED",
                    format!(
                        "cannot derive `{}` for `{}`: field `{}` has unsupported type `{}`",
                        if method_name == TO_STRING { "Debug" } else { "DebugString" },
                        class.name.name,
                        field.name.name,
                        field.ty.name.name
                    ),
                    field.span,
                ));
            }
        }
        if unsupported {
            continue;
        }
        class.methods.push(FunDecl {
            is_pub: true,
            origin_package: class.origin_package.clone(),
            attributes: Vec::new(),
            is_test: false,
            name: Ident { name: method_name.into(), span: derive_span },
            type_params: Vec::new(),
            params: Vec::new(),
            return_type: Some(TypeRef {
                qualifier: None,
                name: Ident { name: "String".into(), span: derive_span },
                type_args: Vec::new(),
                nullable: false,
                reference: false,
                span: derive_span,
                fun: None,
            }),
            body: debug_body(class, derive_span),
            span: derive_span,
        });
    }
    errors
}

/// Expand supported `@derive(Equals)` declarations and return stable M4 errors.
pub(crate) fn expand_equals(file: &mut aura_ast::File) -> Vec<SemaError> {
    let classes = file.classes.clone();
    let mut errors = Vec::new();
    for class in &mut file.classes {
        let Some(derive_span) = has_equals_derive(&class.attributes) else {
            continue;
        };
        if class.methods.iter().any(|method| method.name.name == EQUALS) {
            errors.push(error(
                "AURA-M4-DUPLICATE",
                format!("cannot derive `Equals` for `{}`: method `equals` already exists", class.name.name),
                derive_span,
            ));
            continue;
        }
        let mut unsupported = false;
        for field in &class.fields {
            if !supports_field(field, &classes) {
                unsupported = true;
                errors.push(error(
                    "AURA-M4-UNSUPPORTED",
                    format!(
                        "cannot derive `Equals` for `{}`: field `{}` has unsupported type `{}`",
                        class.name.name,
                        field.name.name,
                        field.ty.name.name
                    ),
                    field.span,
                ));
            }
        }
        if unsupported {
            continue;
        }
        class.methods.push(FunDecl {
            is_pub: true,
            origin_package: class.origin_package.clone(),
            attributes: Vec::new(),
            is_test: false,
            name: Ident { name: EQUALS.into(), span: derive_span },
            type_params: Vec::new(),
            params: vec![Param {
                attributes: Vec::new(),
                name: Ident { name: "other".into(), span: derive_span },
                ty: type_ref_for_class(class),
                span: derive_span,
            }],
            return_type: Some(TypeRef {
                qualifier: None,
                name: Ident { name: "Bool".into(), span: derive_span },
                type_args: Vec::new(),
                nullable: false,
                reference: false,
                span: derive_span,
                fun: None,
            }),
            body: equality_body(class, derive_span),
            span: derive_span,
        });
    }
    errors
}

/// Expand the conservative MVP `@derive(HashCode)` implementation.
pub(crate) fn expand_hash(file: &mut aura_ast::File) -> Vec<SemaError> {
    let mut errors = Vec::new();
    for class in &mut file.classes {
        let Some(derive_span) = has_hash_derive(&class.attributes) else {
            continue;
        };
        if class.methods.iter().any(|method| method.name.name == HASH_CODE) {
            errors.push(error(
                "AURA-M5-DUPLICATE",
                format!(
                    "cannot derive `HashCode` for `{}`: method `hashCode` already exists",
                    class.name.name
                ),
                derive_span,
            ));
            continue;
        }
        let mut unsupported = false;
        for field in &class.fields {
            if !supports_hash_field(field) {
                unsupported = true;
                errors.push(error(
                    "AURA-M5-UNSUPPORTED",
                    format!(
                        "cannot derive `HashCode` for `{}`: field `{}` has unsupported type `{}`",
                        class.name.name, field.name.name, field.ty.name.name
                    ),
                    field.span,
                ));
            }
        }
        if unsupported {
            continue;
        }
        class.methods.push(FunDecl {
            is_pub: true,
            origin_package: class.origin_package.clone(),
            attributes: Vec::new(),
            is_test: false,
            name: Ident {
                name: HASH_CODE.into(),
                span: derive_span,
            },
            type_params: Vec::new(),
            params: Vec::new(),
            return_type: Some(TypeRef {
                qualifier: None,
                name: Ident {
                    name: "Int".into(),
                    span: derive_span,
                },
                type_args: Vec::new(),
                nullable: false,
                reference: false,
                span: derive_span,
                fun: None,
            }),
            body: hash_body(class, derive_span),
            span: derive_span,
        });
    }
    errors
}
