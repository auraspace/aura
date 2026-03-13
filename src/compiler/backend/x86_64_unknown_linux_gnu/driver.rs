use std::process::Command;

pub struct Driver;

impl Driver {
    pub fn build(asm_path: &str, output_path: &str, runtime_code: &str) -> std::io::Result<()> {
        let obj_path = format!("{}.o", asm_path);
        let runtime_src = format!("{}_runtime.c", asm_path);
        let runtime_obj = format!("{}_runtime.o", asm_path);

        // 1. Assemble Aura code
        println!("Assembling {}...", asm_path);
        let status = Command::new("gcc")
            .arg("-c")
            .arg("-o")
            .arg(&obj_path)
            .arg(asm_path)
            .status()?;
        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Assembly failed",
            ));
        }

        // 2. Compile Runtime
        println!("Compiling runtime for {}...", asm_path);
        std::fs::write(&runtime_src, runtime_code)?;
        let status = Command::new("gcc")
            .arg("-c")
            .arg("-o")
            .arg(&runtime_obj)
            .arg(&runtime_src)
            .status()?;

        // Cleanup runtime source immediately after compilation
        let _ = std::fs::remove_file(&runtime_src);

        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Runtime compilation failed",
            ));
        }

        // 3. Link
        println!("Linking {}...", output_path);
        let status = Command::new("gcc")
            .arg("-o")
            .arg(output_path)
            .arg(&obj_path)
            .arg(&runtime_obj)
            .arg("-lpthread") // Common linux link
            .arg("-ldl")
            .status()?;

        if !status.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Linking failed",
            ));
        }

        // Cleanup temporary files
        let _ = std::fs::remove_file(&obj_path);
        let _ = std::fs::remove_file(&runtime_obj);

        Ok(())
    }
}
