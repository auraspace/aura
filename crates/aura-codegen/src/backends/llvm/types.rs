use std::collections::HashMap;

use aura_ir::{mir::MirBody, LoweredProgram};
use aura_sema::Ty;

use super::{symbol_name, CodegenError, EnumVariantInfo, Signatures};

pub(crate) fn task_payload_type(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Task(payload) | Ty::TaskHandle(payload) => Some(payload),
        _ => None,
    }
}

pub(crate) fn signatures(program: &LoweredProgram) -> Signatures {
    program
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
        .collect()
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
) -> Option<String> {
    let receiver_ty = args
        .first()
        .and_then(|place| body.locals.get(place.local))
        .map(|local| &local.ty)?;
    let interface_receiver = matches!(receiver_ty, Ty::Interface(_) | Ty::InterfaceApp { .. });
    signatures
        .iter()
        .find_map(|((owner_package, name), (_, params))| {
            let method_name = name.rsplit("::").next()?;
            if method_name != target.name
                || (owner_package != package && owner_package != &target.package)
                || !params.first().is_some_and(|candidate| {
                    compatible_receiver(candidate, receiver_ty)
                        || (interface_receiver && is_class_type(candidate))
                })
            {
                return None;
            }
            Some(symbol_name(owner_package, name))
        })
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
        _ => left == right,
    }
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

pub(crate) fn array_kind(ty: &Ty) -> Result<i64, CodegenError> {
    match ty {
        Ty::String => Ok(1),
        Ty::Class(_) | Ty::ClassApp { .. } => Ok(2),
        Ty::Enum(_) | Ty::EnumApp { .. } => Ok(3),
        Ty::Int | Ty::Bool | Ty::Float => Ok(0),
        _ => Err(super::unsupported("Array element type")),
    }
}

pub(crate) fn enum_variants(program: &LoweredProgram) -> HashMap<String, EnumVariantInfo> {
    program
        .source()
        .enums
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
        _ => ty.clone(),
    }
}
