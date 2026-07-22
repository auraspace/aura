//! Expansion for source-level derives.

use aura_ast::{
    Attribute, AttributeArg, AttributeValue, BinOp, Block, ClassDecl, Expr, FieldExpr, FunDecl,
    Ident, NominalKind, Param, ReturnStmt, Span, Stmt, TypeRef,
};

use crate::error::SemaError;

const EQUALS: &str = "equals";

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
