use crate::*;
use std::fmt::{self, Write};

pub fn dump_mir(mir: &MirProgram) -> String {
    let mut out = String::new();
    for func in &mir.functions {
        let _ = dump_function(&mut out, func);
        out.push_str("\n");
    }
    for class in mir.classes.values() {
        let _ = writeln!(out, "class {} {{", class.name);
        for (name, ty) in &class.fields {
            let _ = writeln!(out, "  {}: {};", name, ty.name());
        }
        for method in class.methods.values() {
            let _ = dump_function(&mut out, method);
        }
        let _ = writeln!(out, "}}");
        out.push_str("\n");
    }
    out
}

fn dump_function(out: &mut String, func: &MirFunction) -> fmt::Result {
    writeln!(out, "fn {}() {{", func.name)?;
    for (i, local) in func.locals.iter().enumerate() {
        let name = local.name.as_deref().unwrap_or("_");
        writeln!(
            out,
            "  let _{}: {} // {} ({:?})",
            i,
            local.ty.name(),
            name,
            local.kind
        )?;
    }
    writeln!(out)?;
    for block in &func.blocks {
        writeln!(out, "  bb{}:", block.id)?;
        for stmt in &block.statements {
            write!(out, "    ")?;
            dump_stmt(out, stmt)?;
            writeln!(out)?;
        }
        write!(out, "    ")?;
        if let Some(term) = &block.terminator {
            dump_terminator(out, term)?;
        } else {
            write!(out, "unreachable")?;
        }
        writeln!(out)?;
    }
    writeln!(out, "}}")?;
    Ok(())
}

fn dump_stmt(out: &mut String, stmt: &Statement) -> fmt::Result {
    match stmt {
        Statement::Assign(lval, rval) => {
            dump_lvalue(out, lval)?;
            write!(out, " = ")?;
            dump_rvalue(out, rval)?;
        }
    }
    Ok(())
}

fn dump_terminator(out: &mut String, term: &Terminator) -> fmt::Result {
    match term {
        Terminator::Goto(id) => write!(out, "goto bb{}", id),
        Terminator::SwitchInt {
            discr,
            targets,
            otherwise,
        } => {
            write!(out, "switch(")?;
            dump_operand(out, discr)?;
            write!(out, ") {{ ")?;
            for (val, id) in targets {
                write!(out, "{} => bb{}, ", val, id)?;
            }
            write!(out, "_ => bb{} }}", otherwise)
        }
        Terminator::Return(op) => {
            write!(out, "return")?;
            if let Some(op) = op {
                write!(out, " ")?;
                dump_operand(out, op)?;
            }
            Ok(())
        }
        Terminator::Call {
            callee,
            args,
            destination,
            target,
        } => {
            dump_lvalue(out, destination)?;
            write!(out, " = ")?;
            dump_operand(out, callee)?;
            write!(out, "(")?;
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    write!(out, ", ")?;
                }
                dump_operand(out, arg)?;
            }
            write!(out, ") -> bb{}", target)
        }
        Terminator::Unreachable => write!(out, "unreachable"),
    }
}

fn dump_lvalue(out: &mut String, lval: &Lvalue) -> fmt::Result {
    match lval {
        Lvalue::Local(id) => write!(out, "_{}", id),
        Lvalue::Field(base, field) => {
            dump_lvalue(out, base)?;
            write!(out, ".{}", field)
        }
    }
}

fn dump_rvalue(out: &mut String, rval: &Rvalue) -> fmt::Result {
    match rval {
        Rvalue::Use(op) => dump_operand(out, op),
        Rvalue::BinaryOp(op, left, right) => {
            dump_operand(out, left)?;
            write!(out, " {:?} ", op)?;
            dump_operand(out, right)
        }
        Rvalue::UnaryOp(op, op_val) => {
            write!(out, "{:?}(", op)?;
            dump_operand(out, op_val)?;
            write!(out, ")")
        }
        Rvalue::Ref(lval) => {
            write!(out, "&")?;
            dump_lvalue(out, lval)
        }
    }
}

fn dump_operand(out: &mut String, op: &Operand) -> fmt::Result {
    match op {
        Operand::Copy(lval) => dump_lvalue(out, lval),
        Operand::Move(lval) => {
            write!(out, "move ")?;
            dump_lvalue(out, lval)
        }
        Operand::Constant(c) => match c {
            Constant::Int(v) => write!(out, "{}", v),
            Constant::Float(v) => write!(out, "{:?}", v),
            Constant::String(v) => write!(out, "{:?}", v),
            Constant::Bool(v) => write!(out, "{}", v),
        },
    }
}
