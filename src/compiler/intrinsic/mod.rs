pub mod array;
pub mod fs;
pub mod net;
pub mod string;

use crate::compiler::ast::Span;
use crate::compiler::interp::value::Value;
use crate::compiler::sema::checker::SemanticAnalyzer;
use crate::compiler::sema::ty::Type;

pub fn register_interpreter_intrinsics(interp_env: &mut dyn FnMut(String, Value)) {
    fs::register_fs_intrinsics(interp_env);
    string::register_string_intrinsics(interp_env);
    array::register_array_intrinsics(interp_env);
    net::register_net_intrinsics(interp_env);

    // Register common constants (macOS values for now)
    let constants = vec![
        ("O_RDONLY", 0),
        ("O_WRONLY", 1),
        ("O_RDWR", 2),
        ("O_CREAT", 512),  // 0x0200 on macOS
        ("O_TRUNC", 1024), // 0x0400 on macOS
        ("O_APPEND", 8),
    ];

    for (name, val) in constants {
        interp_env(name.to_string(), Value::Int(val));
    }
}

pub fn register_analyzer_intrinsics(sema_analyzer: &mut SemanticAnalyzer) {
    // __fs_open(path: string, flags: i32, mode: i32) -> i32
    sema_analyzer.scope.insert(
        "__fs_open".to_string(),
        Type::Function(
            vec![Type::String, Type::Int32, Type::Int32],
            Box::new(Type::Int32),
        ),
        false,
        Span::new(0, 0),
        Some("Open a file".to_string()),
    );

    // __fs_close(fd: i32) -> void
    sema_analyzer.scope.insert(
        "__fs_close".to_string(),
        Type::Function(vec![Type::Int32], Box::new(Type::Void)),
        false,
        Span::new(0, 0),
        Some("Close a file".to_string()),
    );

    // __fs_read(fd: i32, size: i32) -> string
    sema_analyzer.scope.insert(
        "__fs_read".to_string(),
        Type::Function(vec![Type::Int32, Type::Int32], Box::new(Type::String)),
        false,
        Span::new(0, 0),
        Some("Read from a file".to_string()),
    );

    // __fs_write(fd: i32, content: string) -> i32
    sema_analyzer.scope.insert(
        "__fs_write".to_string(),
        Type::Function(vec![Type::Int32, Type::String], Box::new(Type::Int32)),
        false,
        Span::new(0, 0),
        Some("Write to a file".to_string()),
    );

    // __net_listen(port: i32) -> i32
    sema_analyzer.scope.insert(
        "__net_listen".to_string(),
        Type::Function(vec![Type::Int32], Box::new(Type::Int32)),
        false,
        Span::new(0, 0),
        Some("Listen on a TCP port".to_string()),
    );

    // __net_accept(fd: i32) -> i32
    sema_analyzer.scope.insert(
        "__net_accept".to_string(),
        Type::Function(vec![Type::Int32], Box::new(Type::Int32)),
        false,
        Span::new(0, 0),
        Some("Accept a new TCP connection".to_string()),
    );

    // __net_connect(host: string, port: i32) -> i32
    sema_analyzer.scope.insert(
        "__net_connect".to_string(),
        Type::Function(vec![Type::String, Type::Int32], Box::new(Type::Int32)),
        false,
        Span::new(0, 0),
        Some("Connect to a TCP host".to_string()),
    );

    // Register common constants
    let constants = vec![
        "O_RDONLY", "O_WRONLY", "O_RDWR", "O_CREAT", "O_TRUNC", "O_APPEND",
    ];

    for name in constants {
        sema_analyzer.scope.insert(
            name.to_string(),
            Type::Int32,
            false,
            Span::new(0, 0),
            Some(format!("libc constant {}", name)),
        );
    }
}
