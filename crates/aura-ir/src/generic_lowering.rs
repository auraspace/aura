//! Generic instance closure for the frontend IR boundary.
//!
//! This module owns the frontend AST substitution needed to materialize a
//! checked generic instance. Backends receive its result and do not implement
//! language substitution or inspect generic source declarations.

use std::collections::HashMap;

use aura_ast::*;
use aura_sema::{subst_ty, CheckedFile, Ty};

fn ty_ref(ty: &Ty, span: Span) -> TypeRef {
    let (name, args, nullable) = match ty {
        Ty::Nullable(inner) => {
            let mut value = ty_ref(inner, span);
            value.nullable = true;
            return value;
        }
        Ty::ClassApp { name, args }
        | Ty::EnumApp { name, args }
        | Ty::InterfaceApp { name, args } => (
            name.split('@').next().unwrap_or(name).to_string(),
            args.clone(),
            false,
        ),
        Ty::Class(name) | Ty::Enum(name) | Ty::Interface(name) => (
            name.split('@').next().unwrap_or(name).to_string(),
            Vec::new(),
            false,
        ),
        Ty::Unit => ("Unit".into(), Vec::new(), false),
        Ty::Int => ("Int".into(), Vec::new(), false),
        Ty::Bool => ("Bool".into(), Vec::new(), false),
        Ty::String => ("String".into(), Vec::new(), false),
        Ty::Task(inner) => ("Task".into(), vec![*inner.clone()], false),
        Ty::TaskHandle(inner) => ("TaskHandle".into(), vec![*inner.clone()], false),
        Ty::Channel(inner) => ("Channel".into(), vec![*inner.clone()], false),
        Ty::ForeignHandle(inner) => ("ForeignHandle".into(), vec![*inner.clone()], false),
        Ty::TypeParam(name) => (name.clone(), Vec::new(), false),
        Ty::Null => ("Null".into(), Vec::new(), false),
        Ty::Fun { params, ret } => {
            return TypeRef {
                qualifier: None,
                name: Ident {
                    name: "fn".into(),
                    span,
                },
                type_args: Vec::new(),
                nullable: false,
                reference: false,
                span,
                fun: Some(Box::new(FunTypeRef {
                    params: params.iter().map(|param| ty_ref(param, span)).collect(),
                    ret: ty_ref(ret, span),
                })),
            };
        }
    };
    TypeRef {
        qualifier: None,
        name: Ident { name, span },
        type_args: args.iter().map(|arg| ty_ref(arg, span)).collect(),
        nullable,
        reference: false,
        span,
        fun: None,
    }
}

fn sub_ref(value: &TypeRef, map: &HashMap<String, Ty>) -> TypeRef {
    if let Some(ty) = map.get(&value.name.name) {
        let mut result = ty_ref(ty, value.span);
        result.nullable |= value.nullable;
        result.reference = value.reference;
        return result;
    }
    let mut result = value.clone();
    result.type_args = value
        .type_args
        .iter()
        .map(|arg| sub_ref(arg, map))
        .collect();
    result
}

/// Substitute a checked type reference at the IR boundary. Backends should
/// consume the resulting concrete reference rather than reimplementing
/// generic type lowering.
pub fn substitute_type_ref(value: &TypeRef, params: &[String], args: &[Ty]) -> TypeRef {
    let map = params
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .collect::<HashMap<_, _>>();
    sub_ref(value, &map)
}

pub fn type_ref_from_ty(value: &Ty, span: Span) -> TypeRef {
    ty_ref(value, span)
}

fn expr(value: &mut Expr, checked: Option<&CheckedFile>, map: &HashMap<String, Ty>) {
    match value {
        Expr::Call(call) => {
            if call.type_args.is_empty() {
                if let Some(inst) =
                    checked.and_then(|checked| checked.call_instantiations.get(&call.span.start))
                {
                    call.type_args = inst
                        .type_args
                        .iter()
                        .map(|ty| ty_ref(&subst_ty(ty, map), call.span))
                        .collect();
                }
            } else {
                call.type_args = call.type_args.iter().map(|ty| sub_ref(ty, map)).collect();
            }
            expr(&mut call.callee, checked, map);
            for arg in &mut call.args {
                expr(arg, checked, map);
            }
        }
        Expr::Field(v) => expr(&mut v.object, checked, map),
        Expr::Assign(v) => expr(&mut v.value, checked, map),
        Expr::Binary(v) => {
            expr(&mut v.left, checked, map);
            expr(&mut v.right, checked, map);
        }
        Expr::Unary(v) => expr(&mut v.expr, checked, map),
        Expr::ForceUnwrap(v) => expr(&mut v.expr, checked, map),
        Expr::Is(v) => {
            expr(&mut v.expr, checked, map);
            v.ty = sub_ref(&v.ty, map);
        }
        Expr::Group(v, _) => expr(v, checked, map),
        Expr::If(v) => {
            expr(&mut v.cond, checked, map);
            block(&mut v.then_block, checked, map);
            block(&mut v.else_block, checked, map);
        }
        Expr::Lambda(v) => match &mut v.body {
            LambdaBody::Expr(e) => expr(e, checked, map),
            LambdaBody::Block(b) => block(b, checked, map),
        },
        Expr::Async(v) => match v {
            AsyncExpr::Await(x) => expr(&mut x.operand, checked, map),
            AsyncExpr::Spawn(x) => block(&mut x.body, checked, map),
            AsyncExpr::Join(x) => expr(&mut x.handle, checked, map),
            AsyncExpr::Cancel(x) => expr(&mut x.handle, checked, map),
            AsyncExpr::ChannelCreate(x) => {
                x.element_type = sub_ref(&x.element_type, map);
                expr(&mut x.capacity, checked, map);
            }
            AsyncExpr::ChannelSend(x) => {
                expr(&mut x.channel, checked, map);
                expr(&mut x.value, checked, map);
            }
            AsyncExpr::ChannelReceive(x) => expr(&mut x.channel, checked, map),
            AsyncExpr::ChannelClose(x) => expr(&mut x.channel, checked, map),
        },
        Expr::Ident(_)
        | Expr::This(_)
        | Expr::Int(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::Null(_) => {}
    }
}

fn block(value: &mut Block, checked: Option<&CheckedFile>, map: &HashMap<String, Ty>) {
    for stmt in &mut value.stmts {
        match stmt {
            Stmt::Var(v) => {
                if let Some(ty) = &mut v.ty {
                    *ty = sub_ref(ty, map);
                }
                expr(&mut v.init, checked, map);
            }
            Stmt::If(v) => {
                expr(&mut v.cond, checked, map);
                block(&mut v.then_block, checked, map);
                if let Some(b) = &mut v.else_block {
                    block(b, checked, map);
                }
            }
            Stmt::While(v) => {
                expr(&mut v.cond, checked, map);
                block(&mut v.body, checked, map);
            }
            Stmt::ForRange(v) => {
                expr(&mut v.start, checked, map);
                expr(&mut v.end, checked, map);
                block(&mut v.body, checked, map);
            }
            Stmt::ForIn(v) => {
                expr(&mut v.iterable, checked, map);
                block(&mut v.body, checked, map);
            }
            Stmt::Match(v) => {
                expr(&mut v.scrutinee, checked, map);
                for arm in &mut v.arms {
                    block(&mut arm.body, checked, map);
                }
            }
            Stmt::Try(v) => {
                block(&mut v.try_block, checked, map);
                if let Some(c) = &mut v.catch {
                    c.ty = sub_ref(&c.ty, map);
                    block(&mut c.body, checked, map);
                }
                if let Some(b) = &mut v.finally {
                    block(b, checked, map);
                }
            }
            Stmt::Throw(v) => expr(&mut v.value, checked, map),
            Stmt::Return(v) => {
                if let Some(e) = &mut v.value {
                    expr(e, checked, map);
                }
            }
            Stmt::Expr(e) => expr(e, checked, map),
            Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
}

pub fn close_function(f: &FunDecl, args: &[Ty], checked: &CheckedFile) -> FunDecl {
    let names = f
        .type_params
        .iter()
        .map(|p| p.name.name.clone())
        .collect::<Vec<_>>();
    let map = names
        .into_iter()
        .zip(args.iter().cloned())
        .collect::<HashMap<_, _>>();
    let mut result = f.clone();
    result.type_params.clear();
    for param in &mut result.params {
        param.ty = sub_ref(&param.ty, &map);
    }
    result.return_type = result.return_type.as_ref().map(|ty| sub_ref(ty, &map));
    block(&mut result.body, Some(checked), &map);
    result
}

pub fn close_async_function(f: &AsyncFunDecl, args: &[Ty], checked: &CheckedFile) -> AsyncFunDecl {
    let names = f
        .type_params
        .iter()
        .map(|p| p.name.name.clone())
        .collect::<Vec<_>>();
    let map = names
        .into_iter()
        .zip(args.iter().cloned())
        .collect::<HashMap<_, _>>();
    let mut result = f.clone();
    result.name.name = format!(
        "{}_{}",
        f.name.name,
        args.iter()
            .map(Ty::mono_suffix)
            .collect::<Vec<_>>()
            .join("_")
    );
    result.type_params.clear();
    for param in &mut result.params {
        param.ty = sub_ref(&param.ty, &map);
    }
    result.return_type = result.return_type.as_ref().map(|ty| sub_ref(ty, &map));
    block(&mut result.body, Some(checked), &map);
    result
}

pub fn substitute_async_body(block_value: &mut Block, params: &[String], args: &[Ty]) {
    let map = params
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .collect::<HashMap<_, _>>();
    block(block_value, None, &map);
}

/// Close a generic async class method into the backend-neutral declaration
/// used by the alpha C adapter. The adapter still owns the C ABI wrapper, but
/// generic substitution and synthetic method materialization live here.
pub fn close_async_method(
    class_name: &Ident,
    method: &FunDecl,
    origin_package: String,
    synthetic_name: String,
    class_params: &[String],
    class_args: &[Ty],
    method_args: &[Ty],
) -> Option<AsyncFunDecl> {
    let task_ty = method.return_type.as_ref()?;
    let result_ty = task_ty.type_args.first()?;
    let concrete_this = TypeRef {
        qualifier: None,
        name: class_name.clone(),
        type_args: class_args
            .iter()
            .map(|ty| ty_ref(ty, class_name.span))
            .collect(),
        nullable: false,
        reference: false,
        span: class_name.span,
        fun: None,
    };
    let this_param = Param {
        attributes: Vec::new(),
        name: Ident {
            name: "this".into(),
            span: class_name.span,
        },
        ty: concrete_this,
        span: class_name.span,
    };
    let method_params = method
        .type_params
        .iter()
        .map(|param| param.name.name.clone())
        .collect::<Vec<_>>();
    let mut all_params = class_params.to_vec();
    all_params.extend(method_params);
    let mut all_args = class_args.to_vec();
    all_args.extend_from_slice(method_args);
    let mut params = vec![this_param];
    params.extend(method.params.iter().cloned().map(|mut param| {
        param.ty = substitute_type_ref(&param.ty, &all_params, &all_args);
        param
    }));
    let mut body = method.body.clone();
    substitute_async_body(&mut body, &all_params, &all_args);
    Some(AsyncFunDecl {
        is_pub: method.is_pub,
        origin_package,
        attributes: method.attributes.clone(),
        is_test: false,
        name: Ident {
            name: synthetic_name,
            span: method.name.span,
        },
        type_params: Vec::new(),
        params,
        return_type: Some(substitute_type_ref(result_ty, &all_params, &all_args)),
        body,
        span: method.span,
    })
}

/// Close a generic synchronous class method before backend selection. The
/// method receiver is made explicit so MIR consumers do not rediscover class
/// substitution or receiver layout from AST source.
pub fn close_method(
    class_name: &Ident,
    method: &FunDecl,
    origin_package: String,
    synthetic_name: String,
    class_params: &[String],
    class_args: &[Ty],
    method_args: &[Ty],
) -> FunDecl {
    let concrete_this = TypeRef {
        qualifier: None,
        name: class_name.clone(),
        type_args: class_args
            .iter()
            .map(|ty| ty_ref(ty, class_name.span))
            .collect(),
        nullable: false,
        reference: false,
        span: class_name.span,
        fun: None,
    };
    let this_param = Param {
        attributes: Vec::new(),
        name: Ident {
            name: "this".into(),
            span: class_name.span,
        },
        ty: concrete_this,
        span: class_name.span,
    };
    let method_params = method
        .type_params
        .iter()
        .map(|param| param.name.name.clone())
        .collect::<Vec<_>>();
    let mut all_params = class_params.to_vec();
    all_params.extend(method_params);
    let mut all_args = class_args.to_vec();
    all_args.extend_from_slice(method_args);
    let mut params = vec![this_param];
    params.extend(method.params.iter().cloned().map(|mut param| {
        param.ty = substitute_type_ref(&param.ty, &all_params, &all_args);
        param
    }));
    let mut body = method.body.clone();
    substitute_async_body(&mut body, &all_params, &all_args);
    FunDecl {
        is_pub: method.is_pub,
        origin_package,
        attributes: method.attributes.clone(),
        modifiers: method.modifiers.clone(),
        visibility: method.visibility,
        is_test: false,
        name: Ident {
            name: synthetic_name,
            span: method.name.span,
        },
        type_params: Vec::new(),
        params,
        return_type: method
            .return_type
            .as_ref()
            .map(|ty| substitute_type_ref(ty, &all_params, &all_args)),
        body,
        span: method.span,
    }
}
