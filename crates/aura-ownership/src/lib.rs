//! Backend-neutral ownership policy.
//!
//! Ownership is a compiler contract shared by HIR/MIR lowering and native
//! backends; it is intentionally independent from the aggregate IR container.

use aura_sema::Ty;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    Copy,
    Unique,
    GcReference,
    TaskHandle,
    Channel,
    FunctionEnvironment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Copy,
    Move,
    Clone,
    Retain,
    Drop,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub storage: Storage,
    pub bind: Action,
    pub assign: Action,
    pub across_suspend: Action,
    pub scope_exit: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipMode {
    Borrowed,
    Owned,
    Move,
    Shared,
    Mutable,
}

pub fn plan_for_ty(ty: &Ty) -> Plan {
    match ty {
        Ty::Unit | Ty::Int | Ty::Float | Ty::Bool | Ty::Null => Plan {
            storage: Storage::Copy,
            bind: Action::Copy,
            assign: Action::Copy,
            across_suspend: Action::Copy,
            scope_exit: Action::Noop,
        },
        Ty::String | Ty::ForeignHandle(_) => Plan {
            storage: Storage::Unique,
            bind: Action::Move,
            assign: Action::Move,
            across_suspend: Action::Clone,
            scope_exit: Action::Drop,
        },
        Ty::Class(_) | Ty::Interface(_) | Ty::InterfaceApp { .. } | Ty::Nullable(_) => Plan {
            storage: Storage::GcReference,
            bind: Action::Retain,
            assign: Action::Retain,
            across_suspend: Action::Retain,
            scope_exit: Action::Drop,
        },
        Ty::ClassApp { name, .. } if name == "Array" => Plan {
            storage: Storage::Unique,
            bind: Action::Move,
            assign: Action::Move,
            across_suspend: Action::Clone,
            scope_exit: Action::Drop,
        },
        Ty::ClassApp { .. } | Ty::Enum(_) | Ty::EnumApp { .. } => Plan {
            storage: Storage::Unique,
            bind: Action::Move,
            assign: Action::Move,
            across_suspend: Action::Clone,
            scope_exit: Action::Drop,
        },
        Ty::Fun { .. } => Plan {
            storage: Storage::FunctionEnvironment,
            bind: Action::Move,
            assign: Action::Move,
            across_suspend: Action::Clone,
            scope_exit: Action::Drop,
        },
        Ty::Task(_) | Ty::TaskHandle(_) => Plan {
            storage: Storage::TaskHandle,
            bind: Action::Move,
            assign: Action::Move,
            across_suspend: Action::Retain,
            scope_exit: Action::Drop,
        },
        Ty::Channel(_) => Plan {
            storage: Storage::Channel,
            bind: Action::Move,
            assign: Action::Move,
            across_suspend: Action::Retain,
            scope_exit: Action::Drop,
        },
        Ty::TypeParam(_) => Plan {
            storage: Storage::Unique,
            bind: Action::Move,
            assign: Action::Move,
            across_suspend: Action::Clone,
            scope_exit: Action::Drop,
        },
    }
}

pub fn mode_for_ty(ty: &Ty) -> OwnershipMode {
    match plan_for_ty(ty).storage {
        Storage::Copy => OwnershipMode::Borrowed,
        Storage::Unique | Storage::TaskHandle | Storage::Channel | Storage::FunctionEnvironment => {
            OwnershipMode::Owned
        }
        Storage::GcReference => OwnershipMode::Shared,
    }
}
