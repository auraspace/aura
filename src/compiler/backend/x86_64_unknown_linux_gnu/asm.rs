use crate::compiler::backend::x86_64_unknown_linux_gnu::reg::Register;

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
        self.output.push_str(".text\n");
        self.output.push_str(".global main\n");
        self.output.push_str(".type main, @function\n");
        self.output.push_str("main:\n");
        // Function prologue
        self.output.push_str("    push %rbp\n");
        self.output.push_str("    mov %rsp, %rbp\n");
        self.output.push_str("    sub $256, %rsp\n"); // Space for local variables
    }

    pub fn emit_footer(&mut self) {
        // Function epilogue
        self.output.push_str("    mov $0, %rax\n"); // return 0
        self.output.push_str("    leave\n");
        self.output.push_str("    ret\n");
    }

    pub fn mov_imm(&mut self, reg: Register, val: i64) {
        self.output
            .push_str(&format!("    mov ${}, %{}\n", val, reg.name()));
    }

    pub fn mov_reg(&mut self, dst: Register, src: Register) {
        self.output
            .push_str(&format!("    mov %{}, %{}\n", src.name(), dst.name()));
    }

    pub fn add(&mut self, dst: Register, src1: Register, src2: Register) {
        if dst == src1 {
            self.output
                .push_str(&format!("    add %{}, %{}\n", src2.name(), dst.name()));
        } else if dst == src2 {
            self.output
                .push_str(&format!("    add %{}, %{}\n", src1.name(), dst.name()));
        } else {
            self.output
                .push_str(&format!("    mov %{}, %{}\n", src1.name(), dst.name()));
            self.output
                .push_str(&format!("    add %{}, %{}\n", src2.name(), dst.name()));
        }
    }

    pub fn sub(&mut self, dst: Register, src1: Register, src2: Register) {
        if dst == src1 {
            self.output
                .push_str(&format!("    sub %{}, %{}\n", src2.name(), dst.name()));
        } else {
            self.output
                .push_str(&format!("    mov %{}, %{}\n", src1.name(), dst.name()));
            self.output
                .push_str(&format!("    sub %{}, %{}\n", src2.name(), dst.name()));
        }
    }

    pub fn mul(&mut self, dst: Register, src1: Register, src2: Register) {
        if dst == src1 {
            self.output
                .push_str(&format!("    imul %{}, %{}\n", src2.name(), dst.name()));
        } else if dst == src2 {
            self.output
                .push_str(&format!("    imul %{}, %{}\n", src1.name(), dst.name()));
        } else {
            self.output
                .push_str(&format!("    mov %{}, %{}\n", src1.name(), dst.name()));
            self.output
                .push_str(&format!("    imul %{}, %{}\n", src2.name(), dst.name()));
        }
    }

    pub fn sdiv(&mut self, dst: Register, src1: Register, src2: Register) {
        // x86_64 idiv uses %rax and %rdx
        // We'll move src1 to %rax, sign-extend to %rdx, then idiv src2
        self.output
            .push_str(&format!("    mov %{}, %rax\n", src1.name()));
        self.output.push_str("    cqo\n");
        self.output
            .push_str(&format!("    idiv %{}\n", src2.name()));
        if dst != Register::RAX {
            self.output
                .push_str(&format!("    mov %rax, %{}\n", dst.name()));
        }
    }

    pub fn call(&mut self, label: &str) {
        self.output.push_str(&format!("    call {}\n", label));
    }

    pub fn push(&mut self, reg: Register) {
        self.output.push_str(&format!("    push %{}\n", reg.name()));
    }

    pub fn pop(&mut self, reg: Register) {
        self.output.push_str(&format!("    pop %{}\n", reg.name()));
    }

    pub fn finalize(self) -> String {
        self.output
    }
}
