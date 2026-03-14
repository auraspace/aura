use crate::compiler::interp::Value;
use crate::runtime::ffi::io;
use std::rc::Rc;

pub fn register_fs_intrinsics(register: &mut dyn FnMut(String, Value)) {
    // __fs_open(path: string, flags: i32, mode: i32) -> i32
    register(
        "__fs_open".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 3 {
                panic!("__fs_open expects 3 arguments");
            }
            let path = match &args[0] {
                Value::String(s) => s,
                _ => panic!("__fs_open: arg 0 must be string"),
            };
            let flags = match args[1] {
                Value::Int(i) => i,
                _ => panic!("__fs_open: arg 1 must be i32"),
            };
            let mode = match args[2] {
                Value::Int(i) => i,
                _ => panic!("__fs_open: arg 2 must be i32"),
            };

            match io::open_file(path, flags, mode as libc::mode_t) {
                Ok(fd) => Value::Int(fd),
                Err(e) => {
                    eprintln!("__fs_open error: {}", e);
                    Value::Int(-1)
                }
            }
        })),
    );

    // __fs_close(fd: i32) -> void
    register(
        "__fs_close".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.is_empty() {
                panic!("__fs_close expects 1 argument");
            }
            let fd = match args[0] {
                Value::Int(i) => i,
                _ => panic!("__fs_close: arg 0 must be i32"),
            };

            match io::close_file(fd) {
                Ok(_) => Value::Void,
                Err(e) => {
                    eprintln!("__fs_close error: {}", e);
                    Value::Void
                }
            }
        })),
    );

    // __fs_read(fd: i32, size: i32) -> string
    register(
        "__fs_read".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                panic!("__fs_read expects 2 arguments");
            }
            let fd = match args[0] {
                Value::Int(i) => i,
                _ => panic!("__fs_read: arg 0 must be i32"),
            };
            let size = match args[1] {
                Value::Int(i) => i,
                _ => panic!("__fs_read: arg 1 must be i32"),
            };

            let mut buf = vec![0u8; size as usize];
            match io::read_file(fd, &mut buf) {
                Ok(bytes_read) => {
                    let content = String::from_utf8_lossy(&buf[..bytes_read]).into_owned();
                    Value::String(content)
                }
                Err(e) => {
                    eprintln!("__fs_read error: {}", e);
                    Value::String("".to_string())
                }
            }
        })),
    );

    // __fs_write(fd: i32, content: string) -> i32
    register(
        "__fs_write".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                panic!("__fs_write expects 2 arguments");
            }
            let fd = match args[0] {
                Value::Int(i) => i,
                _ => panic!("__fs_write: arg 0 must be i32"),
            };
            let content = match &args[1] {
                Value::String(s) => s,
                _ => panic!("__fs_write: arg 1 must be string"),
            };

            match io::write_file(fd, content.as_bytes()) {
                Ok(bytes_written) => Value::Int(bytes_written as i32),
                Err(e) => {
                    eprintln!("__fs_write error: {}", e);
                    Value::Int(-1)
                }
            }
        })),
    );
}
