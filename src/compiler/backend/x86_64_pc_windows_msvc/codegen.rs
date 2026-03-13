use crate::compiler::ast::Program;

pub struct Codegen;

impl Codegen {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&mut self, _program: Program) -> String {
        unimplemented!("x86_64-pc-windows-msvc codegen is not yet implemented")
    }
}
