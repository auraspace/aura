use crate::compiler::interp::value::Value;
use crate::runtime::ffi::net;
use std::rc::Rc;

pub fn register_net_intrinsics(register: &mut dyn FnMut(String, Value)) {
    // __net_listen(port: i32) -> i32
    register(
        "__net_listen".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.is_empty() {
                panic!("__net_listen expects 1 argument");
            }
            let port = match args[0] {
                Value::Int(i) => i as u16,
                _ => panic!("__net_listen: arg 0 must be i32"),
            };

            match net::listen_tcp(port) {
                Ok(fd) => Value::Int(fd),
                Err(e) => {
                    eprintln!("__net_listen error: {}", e);
                    Value::Int(-1)
                }
            }
        })),
    );

    // __net_accept(fd: i32) -> i32
    register(
        "__net_accept".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.is_empty() {
                panic!("__net_accept expects 1 argument");
            }
            let fd = match args[0] {
                Value::Int(i) => i,
                _ => panic!("__net_accept: arg 0 must be i32"),
            };

            match net::accept_tcp(fd) {
                Ok(conn_fd) => Value::Int(conn_fd),
                Err(e) => {
                    eprintln!("__net_accept error: {}", e);
                    Value::Int(-1)
                }
            }
        })),
    );

    // __net_connect(host: string, port: i32) -> i32
    register(
        "__net_connect".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                panic!("__net_connect expects 2 arguments");
            }
            let host = match &args[0] {
                Value::String(s) => s,
                _ => panic!("__net_connect: arg 0 must be string"),
            };
            let port = match args[1] {
                Value::Int(i) => i as u16,
                _ => panic!("__net_connect: arg 1 must be i32"),
            };

            match net::connect_tcp(host, port) {
                Ok(fd) => Value::Int(fd),
                Err(e) => {
                    eprintln!("__net_connect error: {}", e);
                    Value::Int(-1)
                }
            }
        })),
    );

    // Reuse __fs_read for sockets
    // Reuse __fs_write for sockets
    // Reuse __fs_close for sockets (aliased as __net_close if needed)
}
