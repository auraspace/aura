use crate::compiler::ir::instr::IrModule;

pub struct IrCodegen;

impl IrCodegen {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&mut self, _module: IrModule) -> String {
        // Placeholder for IR to x86_64 lowering
        String::new()
    }
}
