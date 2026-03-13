use crate::compiler::ir::instr::IrModule;

pub struct IrCodegen;

impl IrCodegen {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&mut self, _module: IrModule) -> String {
        unimplemented!("x86_64-unknown-linux-gnu backend is not yet implemented")
    }
}
