#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    RAX,
    RBX,
    RCX,
    RDX,
    RSI,
    RDI,
    RBP,
    RSP,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

impl Register {
    pub fn name(&self) -> &'static str {
        match self {
            Self::RAX => "rax",
            Self::RBX => "rbx",
            Self::RCX => "rcx",
            Self::RDX => "rdx",
            Self::RSI => "rsi",
            Self::RDI => "rdi",
            Self::RBP => "rbp",
            Self::RSP => "rsp",
            Self::R8 => "r8",
            Self::R9 => "r9",
            Self::R10 => "r10",
            Self::R11 => "r11",
            Self::R12 => "r12",
            Self::R13 => "r13",
            Self::R14 => "r14",
            Self::R15 => "r15",
        }
    }

    pub fn arg_index(idx: usize) -> Self {
        match idx {
            0 => Self::RDI,
            1 => Self::RSI,
            2 => Self::RDX,
            3 => Self::RCX,
            4 => Self::R8,
            5 => Self::R9,
            _ => panic!("More than 6 arguments not supported in System V AMD64 ABI yet"),
        }
    }
}

pub struct Emitter {
    pub output: String,
}

impl Emitter {
    pub fn new() -> Self {
        Self {
            output: String::new(),
        }
    }

    pub fn emit_header(&mut self) {
        self.output.push_str(".intel_syntax noprefix\n");
        // We do not emit _main manually here, to prevent collision with the semantic main
    }

    pub fn emit_footer(&mut self) {
        // Empty footer, let the semantic main handle return
    }

    pub fn mov_imm(&mut self, reg: Register, val: i64) {
        self.output
            .push_str(&format!("    mov {}, {}\n", reg.name(), val));
    }

    pub fn mov_reg(&mut self, dst: Register, src: Register) {
        self.output
            .push_str(&format!("    mov {}, {}\n", dst.name(), src.name()));
    }

    pub fn add(&mut self, dst: Register, src: Register) {
        self.output
            .push_str(&format!("    add {}, {}\n", dst.name(), src.name()));
    }

    pub fn sub(&mut self, dst: Register, src: Register) {
        self.output
            .push_str(&format!("    sub {}, {}\n", dst.name(), src.name()));
    }

    pub fn call(&mut self, label: &str) {
        self.output.push_str(&format!("    call {}\n", label));
    }

    pub fn push(&mut self, reg: Register) {
        self.output.push_str(&format!("    push {}\n", reg.name()));
    }

    pub fn pop(&mut self, reg: Register) {
        self.output.push_str(&format!("    pop {}\n", reg.name()));
    }

    pub fn finalize(self) -> String {
        self.output
    }
}
