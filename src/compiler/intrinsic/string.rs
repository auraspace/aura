use crate::compiler::interp::value::Value;
use std::rc::Rc;

pub fn register_string_intrinsics(register: &mut dyn FnMut(String, Value)) {
    // len(s: string) -> i32
    register(
        "__str_len".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 1 {
                panic!("__str_len expects 1 argument");
            }
            match &args[0] {
                Value::String(s) => Value::Int(s.len() as i32),
                _ => panic!("__str_len: arg 0 must be string"),
            }
        })),
    );

    // charAt(s: string, i: i32) -> string
    register(
        "__str_charAt".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                panic!("__str_charAt expects 2 arguments");
            }
            let s = match &args[0] {
                Value::String(s) => s,
                _ => panic!("__str_charAt: arg 0 must be string"),
            };
            let i = match args[1] {
                Value::Int(i) => i,
                _ => panic!("__str_charAt: arg 1 must be i32"),
            };
            if i < 0 || i as usize >= s.len() {
                return Value::String("".to_string());
            }
            Value::String(s.chars().nth(i as usize).unwrap().to_string())
        })),
    );

    // substring(s: string, start: i32, end: i32) -> string
    register(
        "__str_substring".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 3 {
                panic!("__str_substring expects 3 arguments");
            }
            let s = match &args[0] {
                Value::String(s) => s,
                _ => panic!("__str_substring: arg 0 must be string"),
            };
            let start = match args[1] {
                Value::Int(i) => i,
                _ => panic!("__str_substring: arg 1 must be i32"),
            };
            let end = match args[2] {
                Value::Int(i) => i,
                _ => panic!("__str_substring: arg 2 must be i32"),
            };

            let start = start.max(0).min(s.len() as i32) as usize;
            let end = end.max(0).min(s.len() as i32) as usize;

            if start >= end {
                return Value::String("".to_string());
            }

            Value::String(s[start..end].to_string())
        })),
    );

    // indexOf(s: string, target: string) -> i32
    register(
        "__str_indexOf".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                panic!("__str_indexOf expects 2 arguments");
            }
            let s = match &args[0] {
                Value::String(s) => s,
                _ => panic!("__str_indexOf: arg 0 must be string"),
            };
            let target = match &args[1] {
                Value::String(s) => s,
                _ => panic!("__str_indexOf: arg 1 must be string"),
            };

            match s.find(target) {
                Some(idx) => Value::Int(idx as i32),
                None => Value::Int(-1),
            }
        })),
    );

    // toUpper(s: string) -> string
    register(
        "__str_toUpper".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 1 {
                panic!("__str_toUpper expects 1 argument");
            }
            match &args[0] {
                Value::String(s) => Value::String(s.to_uppercase()),
                _ => panic!("__str_toUpper: arg 0 must be string"),
            }
        })),
    );

    // toLower(s: string) -> string
    register(
        "__str_toLower".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 1 {
                panic!("__str_toLower expects 1 argument");
            }
            match &args[0] {
                Value::String(s) => Value::String(s.to_lowercase()),
                _ => panic!("__str_toLower: arg 0 must be string"),
            }
        })),
    );

    // trim(s: string) -> string
    register(
        "__str_trim".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 1 {
                panic!("__str_trim expects 1 argument");
            }
            match &args[0] {
                Value::String(s) => Value::String(s.trim().to_string()),
                _ => panic!("__str_trim: arg 0 must be string"),
            }
        })),
    );
}
