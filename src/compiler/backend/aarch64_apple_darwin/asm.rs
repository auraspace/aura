#[derive(Debug, Clone, Copy)]
pub enum Register {
    X0,
    X1,
    X2,
    X3,
    X4,
    X5,
    X6,
    X7,
    X8,
    X9,
    X10,
    X11,
    X12,
    X13,
    X14,
    X15,
    X16,
    X17,
    X18,
    X19,
    X20,
    X21,
    X22,
    X23,
    X24,
    X25,
    X26,
    X27,
    X28,
    X29,
    X30,
    SP,
    XZR,
    D0,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
    D31,
}

impl Register {
    pub fn name(&self) -> &'static str {
        match self {
            Self::X0 => "x0",
            Self::X1 => "x1",
            Self::X2 => "x2",
            Self::X3 => "x3",
            Self::X4 => "x4",
            Self::X5 => "x5",
            Self::X6 => "x6",
            Self::X7 => "x7",
            Self::X8 => "x8",
            Self::X9 => "x9",
            Self::X10 => "x10",
            Self::X11 => "x11",
            Self::X12 => "x12",
            Self::X13 => "x13",
            Self::X14 => "x14",
            Self::X15 => "x15",
            Self::X16 => "x16",
            Self::X17 => "x17",
            Self::X18 => "x18",
            Self::X19 => "x19",
            Self::X20 => "x20",
            Self::X21 => "x21",
            Self::X22 => "x22",
            Self::X23 => "x23",
            Self::X24 => "x24",
            Self::X25 => "x25",
            Self::X26 => "x26",
            Self::X27 => "x27",
            Self::X28 => "x28",
            Self::X29 => "x29",
            Self::X30 => "x30",
            Self::SP => "sp",
            Self::XZR => "xzr",
            Self::D0 => "d0",
            Self::D1 => "d1",
            Self::D2 => "d2",
            Self::D3 => "d3",
            Self::D4 => "d4",
            Self::D5 => "d5",
            Self::D6 => "d6",
            Self::D7 => "d7",
            Self::D8 => "d8",
            Self::D9 => "d9",
            Self::D10 => "d10",
            Self::D11 => "d11",
            Self::D12 => "d12",
            Self::D13 => "d13",
            Self::D14 => "d14",
            Self::D15 => "d15",
            Self::D16 => "d16",
            Self::D17 => "d17",
            Self::D18 => "d18",
            Self::D19 => "d19",
            Self::D20 => "d20",
            Self::D21 => "d21",
            Self::D22 => "d22",
            Self::D23 => "d23",
            Self::D24 => "d24",
            Self::D25 => "d25",
            Self::D26 => "d26",
            Self::D27 => "d27",
            Self::D28 => "d28",
            Self::D29 => "d29",
            Self::D30 => "d30",
            Self::D31 => "d31",
        }
    }

    pub fn index(&self) -> u8 {
        match self {
            Self::X0 => 0,
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X3 => 3,
            Self::X4 => 4,
            Self::X5 => 5,
            Self::X6 => 6,
            Self::X7 => 7,
            Self::X8 => 8,
            Self::X9 => 9,
            Self::X10 => 10,
            Self::X11 => 11,
            Self::X12 => 12,
            Self::X13 => 13,
            Self::X14 => 14,
            Self::X15 => 15,
            Self::X16 => 16,
            Self::X17 => 17,
            Self::X18 => 18,
            Self::X19 => 19,
            Self::X20 => 20,
            Self::X21 => 21,
            Self::X22 => 22,
            Self::X23 => 23,
            Self::X24 => 24,
            Self::X25 => 25,
            Self::X26 => 26,
            Self::X27 => 27,
            Self::X28 => 28,
            Self::X29 => 29,
            Self::X30 => 30,
            Self::D0 => 0,
            Self::D1 => 1,
            Self::D2 => 2,
            Self::D3 => 3,
            Self::D4 => 4,
            Self::D5 => 5,
            Self::D6 => 6,
            Self::D7 => 7,
            Self::D8 => 8,
            Self::D9 => 9,
            Self::D10 => 10,
            Self::D11 => 11,
            Self::D12 => 12,
            Self::D13 => 13,
            Self::D14 => 14,
            Self::D15 => 15,
            Self::D16 => 16,
            Self::D17 => 17,
            Self::D18 => 18,
            Self::D19 => 19,
            Self::D20 => 20,
            Self::D21 => 21,
            Self::D22 => 22,
            Self::D23 => 23,
            Self::D24 => 24,
            Self::D25 => 25,
            Self::D26 => 26,
            Self::D27 => 27,
            Self::D28 => 28,
            Self::D29 => 29,
            Self::D30 => 30,
            Self::D31 => 31,
            _ => panic!("Not a general purpose or floating point register"),
        }
    }

    pub fn from_u8(n: u8) -> Self {
        match n {
            0 => Self::X0,
            1 => Self::X1,
            2 => Self::X2,
            3 => Self::X3,
            4 => Self::X4,
            5 => Self::X5,
            6 => Self::X6,
            7 => Self::X7,
            8 => Self::X8,
            9 => Self::X9,
            10 => Self::X10,
            11 => Self::X11,
            12 => Self::X12,
            13 => Self::X13,
            14 => Self::X14,
            15 => Self::X15,
            16 => Self::X16,
            17 => Self::X17,
            18 => Self::X18,
            19 => Self::X19,
            20 => Self::X20,
            21 => Self::X21,
            22 => Self::X22,
            23 => Self::X23,
            24 => Self::X24,
            25 => Self::X25,
            26 => Self::X26,
            27 => Self::X27,
            28 => Self::X28,
            30 => Self::X30,
            _ => panic!("Invalid register number"),
        }
    }

    pub fn from_d_u8(n: u8) -> Self {
        match n {
            0 => Self::D0,
            1 => Self::D1,
            2 => Self::D2,
            3 => Self::D3,
            4 => Self::D4,
            5 => Self::D5,
            6 => Self::D6,
            7 => Self::D7,
            8 => Self::D8,
            9 => Self::D9,
            10 => Self::D10,
            11 => Self::D11,
            12 => Self::D12,
            13 => Self::D13,
            14 => Self::D14,
            15 => Self::D15,
            16 => Self::D16,
            17 => Self::D17,
            18 => Self::D18,
            19 => Self::D19,
            20 => Self::D20,
            21 => Self::D21,
            22 => Self::D22,
            23 => Self::D23,
            24 => Self::D24,
            25 => Self::D25,
            26 => Self::D26,
            27 => Self::D27,
            28 => Self::D28,
            29 => Self::D29,
            30 => Self::D30,
            31 => Self::D31,
            _ => panic!("Invalid float register number"),
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
        self.output.push_str(".text\n");
        self.output.push_str(".global _main\n");
        self.output.push_str(".align 4\n");
        self.output.push_str("_main:\n");
        // Function prologue
        self.output.push_str("    stp x29, x30, [sp, -16]!\n");
        self.output.push_str("    mov x29, sp\n");
        self.output.push_str("    sub sp, sp, #256\n"); // Space for local variables
    }

    pub fn emit_footer(&mut self) {
        // Function epilogue
        self.output.push_str("    mov w0, #0\n"); // return 0
        self.output.push_str("    add sp, sp, #256\n");
        self.output.push_str("    ldp x29, x30, [sp], 16\n");
        self.output.push_str("    ret\n");
    }

    pub fn mov_imm(&mut self, reg: Register, val: i64) {
        if val >= -4096 && val <= 4095 {
            self.output
                .push_str(&format!("    mov {}, #{}\n", reg.name(), val));
        } else {
            self.output
                .push_str(&format!("    ldr {}, ={}\n", reg.name(), val));
        }
    }

    pub fn mov_reg(&mut self, dst: Register, src: Register) {
        self.output
            .push_str(&format!("    mov {}, {}\n", dst.name(), src.name()));
    }

    pub fn add(&mut self, dst: Register, src1: Register, src2: Register) {
        self.output.push_str(&format!(
            "    add {}, {}, {}\n",
            dst.name(),
            src1.name(),
            src2.name()
        ));
    }

    pub fn sub(&mut self, dst: Register, src1: Register, src2: Register) {
        self.output.push_str(&format!(
            "    sub {}, {}, {}\n",
            dst.name(),
            src1.name(),
            src2.name()
        ));
    }

    pub fn mul(&mut self, dst: Register, src1: Register, src2: Register) {
        self.output.push_str(&format!(
            "    mul {}, {}, {}\n",
            dst.name(),
            src1.name(),
            src2.name()
        ));
    }

    pub fn sdiv(&mut self, dst: Register, src1: Register, src2: Register) {
        self.output.push_str(&format!(
            "    sdiv {}, {}, {}\n",
            dst.name(),
            src1.name(),
            src2.name()
        ));
    }

    pub fn fadd(&mut self, dst: Register, src1: Register, src2: Register) {
        self.output.push_str(&format!(
            "    fadd {}, {}, {}\n",
            dst.name(),
            src1.name(),
            src2.name()
        ));
    }

    pub fn fsub(&mut self, dst: Register, src1: Register, src2: Register) {
        self.output.push_str(&format!(
            "    fsub {}, {}, {}\n",
            dst.name(),
            src1.name(),
            src2.name()
        ));
    }

    pub fn fmul(&mut self, dst: Register, src1: Register, src2: Register) {
        self.output.push_str(&format!(
            "    fmul {}, {}, {}\n",
            dst.name(),
            src1.name(),
            src2.name()
        ));
    }

    pub fn fdiv(&mut self, dst: Register, src1: Register, src2: Register) {
        self.output.push_str(&format!(
            "    fdiv {}, {}, {}\n",
            dst.name(),
            src1.name(),
            src2.name()
        ));
    }

    pub fn fmov(&mut self, dst: Register, src: Register) {
        self.output
            .push_str(&format!("    fmov {}, {}\n", dst.name(), src.name()));
    }

    pub fn fcmp(&mut self, src1: Register, src2: Register) {
        self.output
            .push_str(&format!("    fcmp {}, {}\n", src1.name(), src2.name()));
    }

    pub fn scvtf(&mut self, dst: Register, src: Register) {
        // Integer to floating-point
        self.output
            .push_str(&format!("    scvtf {}, {}\n", dst.name(), src.name()));
    }

    pub fn fcvtzs(&mut self, dst: Register, src: Register) {
        // Floating-point to integer (signed, towards zero)
        self.output
            .push_str(&format!("    fcvtzs {}, {}\n", dst.name(), src.name()));
    }

    pub fn fmov_imm(&mut self, reg: Register, val: f64) {
        // Simple heuristic: if it's 0.0, use fmov reg, #0.0
        if val == 0.0 {
            self.output
                .push_str(&format!("    fmov {}, #0.0\n", reg.name()));
        } else {
            // Otherwise, load via ldr from literal pool or similar
            // For now, let's use a trick: load as i64 then fmov
            let bits = val.to_bits();
            self.output.push_str(&format!("    ldr x16, ={}\n", bits));
            self.output
                .push_str(&format!("    fmov {}, x16\n", reg.name()));
        }
    }

    pub fn call(&mut self, label: &str) {
        self.output.push_str(&format!("    bl {}\n", label));
    }

    pub fn push(&mut self, reg: Register) {
        self.output
            .push_str(&format!("    str {}, [sp, -16]!\n", reg.name()));
    }

    pub fn pop(&mut self, reg: Register) {
        self.output
            .push_str(&format!("    ldr {}, [sp], 16\n", reg.name()));
    }

    pub fn finalize(self) -> String {
        self.output
    }
}
