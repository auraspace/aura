use crate::compiler::ir::instr::IrModule;

pub struct IrCodegen;

impl IrCodegen {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&mut self, _module: IrModule) -> String {
        todo!("x86_64-pc-windows-msvc backend not yet implemented")
    }
}
