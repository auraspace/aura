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
    Alloc(u32, u32),              // dest, size
    Load(u32, Operand, u32),      // dest, base_ptr, offset
    Store(Operand, Operand, u32), // src_value, base_ptr, offset

    // Calls
    Call(u32, String, Vec<Operand>), // dest, function_name, args
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
}
