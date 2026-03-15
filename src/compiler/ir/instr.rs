#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrType {
    I32,
    I64,
    Pointer,
    Any, // Tagged union (16 bytes: 8-byte tag + 8-byte value)
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Value(u32), // SSA Register number
    Constant(i64),
    Parameter(u32), // Function parameter index
}

#[derive(Debug, Clone)]
pub enum Instruction {
    Add(u32, Operand, Operand), // dest, lhs, rhs
    Sub(u32, Operand, Operand),
    Mul(u32, Operand, Operand),
    Div(u32, Operand, Operand),
    Rem(u32, Operand, Operand),

    BitAnd(u32, Operand, Operand),
    BitOr(u32, Operand, Operand),
    BitXor(u32, Operand, Operand),
    Shl(u32, Operand, Operand),
    Shr(u32, Operand, Operand),

    BitNot(u32, Operand),

    // Comparison
    Eq(u32, Operand, Operand),
    Ne(u32, Operand, Operand),
    Lt(u32, Operand, Operand),
    Le(u32, Operand, Operand),
    Gt(u32, Operand, Operand),
    Ge(u32, Operand, Operand),

    // Control Flow
    Jump(String),                    // target block label
    Branch(Operand, String, String), // condition, then_block, else_block
    Return(Option<Operand>),

    // Memory
    Alloc(u32, u32),                // dest, size
    Load(u32, Operand, u32),        // dest, base_ptr, offset
    Store(Operand, Operand, u32),   // src_value, base_ptr, offset
    WriteBarrier(Operand, Operand), // object_ptr, field_ptr

    Call(u32, String, Vec<Operand>), // dest, function_name, args
    CallVirtual(u32, Operand, u32, Vec<Operand>), // dest, object_ptr, vtable_index, args
    SetVTable(Operand, String),      // object_ptr, class_name
    Move(u32, Operand),              // dest, src
    StackAlloc(u32, u32),            // dest, size
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub label: String,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<IrType>,
    pub return_type: IrType,
    pub blocks: Vec<BasicBlock>,
}

#[derive(Debug, Clone)]
pub struct IrModule {
    pub functions: Vec<IrFunction>,
    pub globals: Vec<(String, String)>, // (name, content)
    pub vtables: std::collections::HashMap<String, Vec<String>>, // class_name -> list of function names
}

impl std::fmt::Display for IrType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IrType::I32 => write!(f, "i32"),
            IrType::I64 => write!(f, "i64"),
            IrType::Pointer => write!(f, "ptr"),
            IrType::Any => write!(f, "any"),
            IrType::Void => write!(f, "void"),
        }
    }
}

impl std::fmt::Display for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operand::Value(v) => write!(f, "%{}", v),
            Operand::Constant(c) => write!(f, "const({})", c),
            Operand::Parameter(p) => write!(f, "param({})", p),
        }
    }
}

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instruction::Add(d, l, r) => write!(f, "  %{} = add {}, {}", d, l, r),
            Instruction::Sub(d, l, r) => write!(f, "  %{} = sub {}, {}", d, l, r),
            Instruction::Mul(d, l, r) => write!(f, "  %{} = mul {}, {}", d, l, r),
            Instruction::Div(d, l, r) => write!(f, "  %{} = div {}, {}", d, l, r),
            Instruction::Rem(d, l, r) => write!(f, "  %{} = rem {}, {}", d, l, r),
            Instruction::BitAnd(d, l, r) => write!(f, "  %{} = and {}, {}", d, l, r),
            Instruction::BitOr(d, l, r) => write!(f, "  %{} = or {}, {}", d, l, r),
            Instruction::BitXor(d, l, r) => write!(f, "  %{} = xor {}, {}", d, l, r),
            Instruction::Shl(d, l, r) => write!(f, "  %{} = shl {}, {}", d, l, r),
            Instruction::Shr(d, l, r) => write!(f, "  %{} = shr {}, {}", d, l, r),
            Instruction::BitNot(d, s) => write!(f, "  %{} = not {}", d, s),
            Instruction::Eq(d, l, r) => write!(f, "  %{} = eq {}, {}", d, l, r),
            Instruction::Ne(d, l, r) => write!(f, "  %{} = ne {}, {}", d, l, r),
            Instruction::Lt(d, l, r) => write!(f, "  %{} = lt {}, {}", d, l, r),
            Instruction::Le(d, l, r) => write!(f, "  %{} = le {}, {}", d, l, r),
            Instruction::Gt(d, l, r) => write!(f, "  %{} = gt {}, {}", d, l, r),
            Instruction::Ge(d, l, r) => write!(f, "  %{} = ge {}, {}", d, l, r),
            Instruction::Jump(lbl) => write!(f, "  jump {}", lbl),
            Instruction::Branch(c, t, e) => write!(f, "  br {}, {}, {}", c, t, e),
            Instruction::Return(Some(op)) => write!(f, "  ret {}", op),
            Instruction::Return(None) => write!(f, "  ret"),
            Instruction::Alloc(d, s) => write!(f, "  %{} = alloc {}", d, s),
            Instruction::Load(d, b, off) => write!(f, "  %{} = load {}, {}", d, b, off),
            Instruction::Store(v, b, off) => write!(f, "  store {}, {}, {}", v, b, off),
            Instruction::WriteBarrier(o, v) => write!(f, "  write_barrier {}, {}", o, v),
            Instruction::Call(d, func, args) => {
                let args_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "  %{} = call {} {}", d, func, args_str)
            }
            Instruction::CallVirtual(d, obj, idx, args) => {
                let args_str = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "  %{} = call_virtual {}, {}, {}", d, obj, idx, args_str)
            }
            Instruction::SetVTable(obj, class) => write!(f, "  set_vtable {}, {}", obj, class),
            Instruction::Move(d, s) => write!(f, "  %{} = move {}", d, s),
            Instruction::StackAlloc(d, s) => write!(f, "  %{} = salloc {}", d, s),
        }
    }
}

impl std::fmt::Display for BasicBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}:", self.label)?;
        for instr in &self.instructions {
            writeln!(f, "{}", instr)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for IrFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params_str = self
            .params
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            f,
            "func {}({}) -> {} {{",
            self.name, params_str, self.return_type
        )?;
        for block in &self.blocks {
            write!(f, "{}", block)?;
        }
        writeln!(f, "}}")
    }
}

impl std::fmt::Display for IrModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, content) in &self.globals {
            writeln!(f, "global {} = \"{}\"", name, content)?;
        }
        if !self.globals.is_empty() {
            writeln!(f)?;
        }
        for func in &self.functions {
            writeln!(f, "{}", func)?;
        }
        for (class, methods) in &self.vtables {
            writeln!(f, "vtable {} {{", class)?;
            for (i, method) in methods.iter().enumerate() {
                writeln!(f, "  {}: {}", i, method)?;
            }
            writeln!(f, "}}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ir_text_format() {
        let module = IrModule {
            globals: vec![("msg".to_string(), "Hello World".to_string())],
            vtables: std::collections::HashMap::new(),
            functions: vec![IrFunction {
                name: "main".to_string(),
                params: vec![IrType::I32, IrType::I32],
                return_type: IrType::I32,
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    instructions: vec![
                        Instruction::Add(1, Operand::Parameter(0), Operand::Parameter(1)),
                        Instruction::Return(Some(Operand::Value(1))),
                    ],
                }],
            }],
        };

        let output = format!("{}", module);
        let expected = "global msg = \"Hello World\"

func main(i32, i32) -> i32 {
entry:
  %1 = add param(0), param(1)
  ret %1
}

";
        assert_eq!(output, expected);
    }
}
