//! LLVM backend facade.

mod compile;
mod emit;
mod options;

#[cfg(test)]
mod tests;

pub use compile::LlvmBackend;
pub use emit::emit_module;
pub use options::options;
