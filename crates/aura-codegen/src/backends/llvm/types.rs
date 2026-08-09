use std::collections::HashMap;

use aura_ir::{mir::MirBody, LoweredProgram};
use aura_sema::Ty;

use super::{symbol_name, unsupported, CodegenError, EnumVariantInfo, Signatures};

pub(super) fn llvm_zero(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Unit => Err(unsupported("unit local")),
        Ty::Bool => Ok("false"),
        Ty::Float => Ok("0.0"),
        Ty::Int => Ok("0"),
        Ty::Fun { .. } => Ok("{ ptr null, ptr null }"),
        Ty::String | Ty::Null => Ok("null"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::String) => Ok("null"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Int) => Ok("{ i1 false, i64 0 }"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Bool) => Ok("{ i1 false, i1 false }"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Float) => {
            Ok("{ i1 false, double 0.0 }")
        }
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::TypeParam(_)) => Ok("null"),
        Ty::Nullable(inner)
            if matches!(
                inner.as_ref(),
                Ty::Class(_)
                    | Ty::ClassApp { .. }
                    | Ty::Enum(_)
                    | Ty::EnumApp { .. }
                    | Ty::Interface(_)
                    | Ty::InterfaceApp { .. }
                    | Ty::Fun { .. }
                    | Ty::Channel(_)
                    | Ty::ForeignHandle(_)
            ) =>
        {
            Ok("null")
        }
        Ty::Interface(_) | Ty::InterfaceApp { .. } => Ok("null"),
        Ty::Enum(_)
        | Ty::EnumApp { .. }
        | Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::ForeignHandle(_) => Ok("null"),
        Ty::Task(_) | Ty::TaskHandle(_) => Ok("null"),
        Ty::Channel(_) => Ok("null"),
        _ => Err(unsupported(&format!("type {}", ty.display()))),
    }
}

pub(crate) fn llvm_type(ty: &Ty) -> Result<&'static str, CodegenError> {
    match ty {
        Ty::Unit => Ok("void"),
        Ty::Int => Ok("i64"),
        Ty::Bool => Ok("i1"),
        Ty::Float => Ok("double"),
        Ty::Fun { .. } => Ok("%AuraLlvmFun"),
        Ty::String | Ty::Null => Ok("ptr"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::String) => Ok("ptr"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Int) => Ok("%AuraLlvmOptInt"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Bool) => Ok("%AuraLlvmOptBool"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Float) => Ok("%AuraLlvmOptFloat"),
        Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::TypeParam(_)) => Ok("ptr"),
        Ty::Nullable(inner)
            if matches!(
                inner.as_ref(),
                Ty::Class(_)
                    | Ty::ClassApp { .. }
                    | Ty::Enum(_)
                    | Ty::EnumApp { .. }
                    | Ty::Interface(_)
                    | Ty::InterfaceApp { .. }
                    | Ty::Fun { .. }
                    | Ty::Channel(_)
                    | Ty::ForeignHandle(_)
            ) =>
        {
            Ok("ptr")
        }
        Ty::Interface(_) | Ty::InterfaceApp { .. } => Ok("ptr"),
        Ty::Enum(_)
        | Ty::EnumApp { .. }
        | Ty::Class(_)
        | Ty::ClassApp { .. }
        | Ty::ForeignHandle(_) => Ok("ptr"),
        // Task values are executor-owned frame handles regardless of payload.
        Ty::Task(_) | Ty::TaskHandle(_) => Ok("ptr"),
        Ty::Channel(_) => Ok("ptr"),
        _ => Err(unsupported(&format!("type {}", ty.display()))),
    }
}

pub(crate) fn task_payload_type(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Task(payload) | Ty::TaskHandle(payload) => Some(payload),
        _ => None,
    }
}

pub(crate) fn signatures(program: &LoweredProgram) -> Signatures {
    let mut signatures = program
        .checked()
        .functions
        .iter()
        .chain(program.checked().generic_functions.iter())
        .map(|function| {
            (
                (function.package.clone(), function.name.clone()),
                (
                    function.ret.ty.clone(),
                    function
                        .params
                        .iter()
                        .map(|param| param.ty.clone())
                        .collect(),
                ),
            )
        })
        .collect::<Signatures>();
    for (name, params, ret) in &program.checked().generic_method_signatures {
        let owner_package = program
            .checked()
            .class_layouts
            .iter()
            .find(|class| name.starts_with(&format!("{}_", class.name)))
            .map(|class| class.package.clone())
            .unwrap_or_else(|| program.checked().package.clone());
        signatures.insert((owner_package, name.clone()), (ret.clone(), params.clone()));
    }
    // Async declarations are callable through their task-returning public
    // wrapper, while their MIR body is emitted under a private symbol.
    for function in &program.checked().async_signatures {
        signatures.insert(
            (function.package.clone(), function.name.clone()),
            (function.ret.clone(), function.params.clone()),
        );
    }
    signatures
}

pub(crate) fn signature_for<'a>(
    signatures: &'a Signatures,
    package: &str,
    target: &aura_ir::mir::CallTarget,
) -> Option<&'a (Ty, Vec<Ty>)> {
    signatures
        .get(&(target.package.clone(), target.name.clone()))
        .or_else(|| signatures.get(&(package.to_owned(), target.name.clone())))
        .or_else(|| {
            signatures
                .iter()
                .find(|((_, name), _)| name == &target.name)
                .map(|(_, signature)| signature)
        })
        .or_else(|| {
            signatures
                .iter()
                .find(|((_, name), _)| name.rsplit("::").next() == Some(target.name.as_str()))
                .map(|(_, signature)| signature)
        })
}

pub(crate) fn method_symbol_for(
    signatures: &Signatures,
    target: &aura_ir::mir::CallTarget,
    args: &[aura_ir::mir::Place],
    body: &MirBody,
    package: &str,
    result_ty: Option<&Ty>,
) -> Option<String> {
    let receiver_ty = args
        .first()
        .and_then(|place| body.locals.get(place.local))
        .map(|local| &local.ty)?;
    let interface_receiver = matches!(receiver_ty, Ty::Interface(_) | Ty::InterfaceApp { .. });
    if target.is_static && !args.is_empty() {
        let suffix = body.locals[args[0].local].ty.mono_suffix();
        let suffix = format!("_{}_{}", target.name, suffix);
        if let Some(((owner_package, name), _)) =
            signatures.iter().find(|((owner_package, name), _)| {
                name.ends_with(&suffix)
                    && (owner_package == package || owner_package == &target.package)
            })
        {
            return Some(symbol_name(owner_package, name));
        }
    }
    if let Some(owner) = class_type_name(receiver_ty) {
        let owner = owner.rsplit('.').next().unwrap_or(owner);
        let owner_method = format!("{owner}__{}", target.name);
        let owner_decl = format!("{owner}::{}", target.name);
        if let Some(((owner_package, name), (_ret, _))) =
            signatures.iter().find(|((_, name), (ret, _))| {
                (name == &owner_method || name == &owner_decl)
                    && result_ty.is_none_or(|expected| return_compatible(ret, expected))
            })
        {
            return Some(symbol_name(owner_package, name));
        }
        let receiver_args = class_type_args(receiver_ty).to_vec();
        let receiver_arg_count = receiver_args.len();
        let mut suffix_args = receiver_args;
        suffix_args.extend(target.method_type_args.iter().cloned());
        let suffix = suffix_args.iter().map(Ty::mono_suffix).collect::<Vec<_>>();
        let expected = if suffix.is_empty() {
            format!("{owner}_{}", target.name)
        } else {
            format!("{owner}_{}_{}", target.name, suffix.join("_"))
        };
        if let Some(((owner_package, name), _)) =
            signatures.iter().find(|((_, name), _)| name == &expected)
        {
            return Some(symbol_name(owner_package, name));
        }
        if !target.method_type_args.is_empty() {
            let receiver_suffix = suffix_args
                .into_iter()
                .take(receiver_arg_count)
                .map(|ty| ty.mono_suffix())
                .collect::<Vec<_>>();
            let receiver_only = format!("{owner}_{}_{}", target.name, receiver_suffix.join("_"));
            if let Some(((owner_package, name), _)) = signatures
                .iter()
                .find(|((_, name), _)| name == &receiver_only)
            {
                return Some(symbol_name(owner_package, name));
            }
        }
    }
    let matches_receiver = |params: &[Ty]| {
        params.first().is_some_and(|candidate| {
            compatible_receiver(candidate, receiver_ty)
                || (interface_receiver && is_class_type(candidate))
        })
    };
    signatures
        .iter()
        .find_map(|((owner_package, name), (ret, params))| {
            let concrete_method = name.contains(&format!("_{}_", target.name));
            (concrete_method
                && (owner_package == package || owner_package == &target.package)
                && result_ty.is_none_or(|expected| return_compatible(ret, expected))
                && matches_receiver(params))
            .then(|| symbol_name(owner_package, name))
        })
        .or_else(|| {
            signatures
                .iter()
                .find_map(|((owner_package, name), (ret, params))| {
                    let method_name = name.rsplit("::").next()?;
                    if method_name != target.name
                        || (owner_package != package && owner_package != &target.package)
                        || !result_ty.is_none_or(|expected| types_compatible(ret, expected))
                        || !params.first().is_some_and(|candidate| {
                            compatible_receiver(candidate, receiver_ty)
                                || (interface_receiver && is_class_type(candidate))
                        })
                    {
                        return None;
                    }
                    Some(symbol_name(owner_package, name))
                })
        })
        .or_else(|| {
            // Imported class methods may arrive without a package on the call
            // target. Match the receiver's nominal class before falling back
            // to the generic ABI-compatible lookup.
            signatures
                .iter()
                .find_map(|((owner_package, name), (ret, params))| {
                    let method_name = name.rsplit("::").next()?;
                    if method_name != target.name {
                        return None;
                    }
                    let candidate = params.first()?;
                    (result_ty.is_none_or(|expected| return_compatible(ret, expected))
                        && class_type_name(candidate) == class_type_name(receiver_ty))
                    .then(|| symbol_name(owner_package, name))
                })
        })
}

pub(crate) fn signature_for_symbol<'a>(
    signatures: &'a Signatures,
    symbol: &str,
) -> Option<&'a (Ty, Vec<Ty>)> {
    signatures
        .iter()
        .find(|((package, name), _)| symbol_name(package, name) == symbol)
        .map(|(_, signature)| signature)
}

pub(crate) fn monomorphized_symbol_for(
    signatures: &Signatures,
    target: &aura_ir::mir::CallTarget,
    package: &str,
    argument_tys: &[Ty],
) -> Option<String> {
    if target.type_args.is_empty() {
        return None;
    }
    let suffix = target
        .type_args
        .iter()
        .map(Ty::mono_suffix)
        .collect::<Vec<_>>()
        .join("_");
    let name = format!("{}_{}", target.name, suffix);
    signatures
        .iter()
        .find(|((owner, candidate), _)| {
            candidate == &name && (owner == package || owner == &target.package)
        })
        .or_else(|| {
            signatures.iter().find(|((owner, candidate), (_, params))| {
                candidate.starts_with(&format!("{}_", target.name))
                    && (owner == package || owner == &target.package)
                    && params.len() == argument_tys.len()
                    && params
                        .iter()
                        .zip(argument_tys)
                        .all(|(expected, actual)| types_compatible(expected, actual))
            })
        })
        .or_else(|| {
            signatures.iter().find(|((owner, candidate), _)| {
                candidate.starts_with(&format!("{}_", target.name))
                    && candidate.ends_with(&format!("_{suffix}"))
                    && (owner == package || owner == &target.package)
            })
        })
        .or_else(|| {
            signatures
                .iter()
                .find(|((_, candidate), _)| candidate == &name)
        })
        .map(|((owner, candidate), _)| symbol_name(owner, candidate))
}

pub(crate) fn compatible_receiver(left: &Ty, right: &Ty) -> bool {
    if let Ty::Nullable(inner) = left {
        return compatible_receiver(inner, right);
    }
    if let Ty::Nullable(inner) = right {
        return compatible_receiver(left, inner);
    }
    match (left, right) {
        (left, right) if is_class_type(left) && is_class_type(right) => true,
        _ => left == right,
    }
}

pub(crate) fn types_compatible(left: &Ty, right: &Ty) -> bool {
    match (left, right) {
        (Ty::Class(left), Ty::Class(right)) => nominal_tail(left) == nominal_tail(right),
        (
            Ty::ClassApp {
                name: left,
                args: left_args,
            },
            Ty::ClassApp {
                name: right,
                args: right_args,
            },
        ) => {
            nominal_tail(left) == nominal_tail(right)
                && left_args.len() == right_args.len()
                && left_args
                    .iter()
                    .zip(right_args)
                    .all(|(left, right)| types_compatible(left, right))
        }
        (Ty::Enum(left), Ty::Enum(right)) => nominal_tail(left) == nominal_tail(right),
        (
            Ty::EnumApp {
                name: left,
                args: left_args,
            },
            Ty::EnumApp {
                name: right,
                args: right_args,
            },
        ) => {
            nominal_tail(left) == nominal_tail(right)
                && left_args.len() == right_args.len()
                && left_args
                    .iter()
                    .zip(right_args)
                    .all(|(left, right)| types_compatible(left, right))
        }
        (Ty::Interface(left), Ty::Interface(right)) => nominal_tail(left) == nominal_tail(right),
        (
            Ty::InterfaceApp {
                name: left,
                args: left_args,
            },
            Ty::InterfaceApp {
                name: right,
                args: right_args,
            },
        ) => {
            nominal_tail(left) == nominal_tail(right)
                && left_args.len() == right_args.len()
                && left_args
                    .iter()
                    .zip(right_args)
                    .all(|(left, right)| types_compatible(left, right))
        }
        (
            Ty::Fun {
                params: left_params,
                ret: left_ret,
            },
            Ty::Fun {
                params: right_params,
                ret: right_ret,
            },
        ) => {
            left_params.len() == right_params.len()
                && left_params
                    .iter()
                    .zip(right_params)
                    .all(|(left, right)| types_compatible(left, right))
                && types_compatible(left_ret, right_ret)
        }
        _ => left == right,
    }
}

fn return_compatible(actual: &Ty, expected: &Ty) -> bool {
    types_compatible(actual, expected)
        || matches!(expected, Ty::Nullable(inner) if types_compatible(actual, inner))
}

pub(crate) fn nominal_tail(name: &str) -> &str {
    name.split('@')
        .next()
        .unwrap_or(name)
        .rsplit('.')
        .next()
        .unwrap_or(name)
}

pub(crate) fn is_string_type(ty: &Ty) -> bool {
    match ty {
        Ty::String => true,
        Ty::Nullable(inner) => matches!(inner.as_ref(), Ty::String),
        _ => false,
    }
}

pub(crate) fn is_tagged_nullable(ty: &Ty) -> bool {
    matches!(ty, Ty::Nullable(inner) if matches!(inner.as_ref(), Ty::Int | Ty::Bool | Ty::Float))
}

pub(crate) fn is_enum_type(ty: &Ty) -> bool {
    match ty {
        Ty::Enum(_) | Ty::EnumApp { .. } => true,
        Ty::Nullable(inner) => is_enum_type(inner),
        _ => false,
    }
}

pub(crate) fn is_class_type(ty: &Ty) -> bool {
    match ty {
        Ty::Class(_) => true,
        Ty::ClassApp { name, .. } => name != "Array",
        Ty::Nullable(inner) => is_class_type(inner),
        _ => false,
    }
}

pub(crate) fn class_type_name(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Class(name) => Some(name.split('@').next().unwrap_or(name)),
        Ty::ClassApp { name, .. } if name != "Array" => {
            Some(name.split('@').next().unwrap_or(name))
        }
        Ty::Nullable(inner) => class_type_name(inner),
        _ => None,
    }
}

pub(crate) fn class_type_args(ty: &Ty) -> &[Ty] {
    match ty {
        Ty::ClassApp { args, .. } => args,
        Ty::Nullable(inner) => class_type_args(inner),
        _ => &[],
    }
}

pub(crate) fn is_array_type(ty: &Ty) -> bool {
    match ty {
        Ty::ClassApp { name, args } => name == "Array" && args.len() == 1,
        Ty::Nullable(inner) => is_array_type(inner),
        _ => false,
    }
}

pub(crate) fn array_element_type(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::ClassApp { name, args } if name == "Array" && args.len() == 1 => args.first(),
        Ty::Nullable(inner) => array_element_type(inner),
        _ => None,
    }
}

pub(crate) fn contains_type_param(ty: &Ty) -> bool {
    match ty {
        Ty::TypeParam(_) => true,
        Ty::Nullable(inner)
        | Ty::Task(inner)
        | Ty::TaskHandle(inner)
        | Ty::Channel(inner)
        | Ty::ForeignHandle(inner) => contains_type_param(inner),
        Ty::ClassApp { args, .. } | Ty::EnumApp { args, .. } | Ty::InterfaceApp { args, .. } => {
            args.iter().any(contains_type_param)
        }
        Ty::Fun { params, ret } => {
            params.iter().any(contains_type_param) || contains_type_param(ret)
        }
        _ => false,
    }
}

pub(crate) fn array_kind(ty: &Ty) -> Result<i64, CodegenError> {
    match ty {
        Ty::ClassApp { name, args } if name == "Array" && args.len() == 1 => Ok(4),
        Ty::String => Ok(1),
        Ty::Class(_) | Ty::ClassApp { .. } | Ty::Interface(_) | Ty::InterfaceApp { .. } => Ok(2),
        Ty::Enum(_) | Ty::EnumApp { .. } => Ok(3),
        Ty::Int | Ty::Bool | Ty::Float => Ok(0),
        _ => Err(super::unsupported(&format!(
            "Array element type {}",
            ty.display()
        ))),
    }
}

pub(crate) fn enum_variants(program: &LoweredProgram) -> HashMap<String, EnumVariantInfo> {
    program
        .checked()
        .enum_layouts
        .iter()
        .flat_map(|enum_decl| {
            enum_decl.variants.iter().map(|variant| {
                (
                    variant.name.clone(),
                    EnumVariantInfo {
                        tag: variant.tag as i64,
                        type_params: enum_decl.type_params.clone(),
                        fields: variant.fields.clone(),
                    },
                )
            })
        })
        .collect()
}

pub(crate) fn resolved_variant_fields(
    info: &EnumVariantInfo,
    owner_ty: &Ty,
    fallback_args: &[Ty],
) -> Vec<(String, Ty)> {
    let args = match owner_ty {
        Ty::EnumApp { args, .. } => args.as_slice(),
        _ => fallback_args,
    };
    let substitutions = info
        .type_params
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .collect::<HashMap<_, _>>();
    info.fields
        .iter()
        .map(|(name, ty)| (name.clone(), substitute_ty(ty, &substitutions)))
        .collect()
}

pub(crate) fn substitute_ty(ty: &Ty, substitutions: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::TypeParam(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        Ty::Nullable(inner) => Ty::Nullable(Box::new(substitute_ty(inner, substitutions))),
        Ty::ClassApp { name, args } => Ty::ClassApp {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_ty(arg, substitutions))
                .collect(),
        },
        Ty::EnumApp { name, args } => Ty::EnumApp {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| substitute_ty(arg, substitutions))
                .collect(),
        },
        Ty::Task(inner) => Ty::Task(Box::new(substitute_ty(inner, substitutions))),
        Ty::TaskHandle(inner) => Ty::TaskHandle(Box::new(substitute_ty(inner, substitutions))),
        Ty::Channel(inner) => Ty::Channel(Box::new(substitute_ty(inner, substitutions))),
        Ty::ForeignHandle(inner) => {
            Ty::ForeignHandle(Box::new(substitute_ty(inner, substitutions)))
        }
        Ty::Fun { params, ret } => Ty::Fun {
            params: params
                .iter()
                .map(|param| substitute_ty(param, substitutions))
                .collect(),
            ret: Box::new(substitute_ty(ret, substitutions)),
        },
        _ => ty.clone(),
    }
}
