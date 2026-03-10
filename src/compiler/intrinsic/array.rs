use crate::compiler::interp::value::Value;
use std::rc::Rc;

pub fn register_array_intrinsics(register: &mut dyn FnMut(String, Value)) {
    // len(a: array) -> i32
    register(
        "__arr_len".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 1 {
                panic!("__arr_len expects 1 argument");
            }
            if let Value::Array(a) = &args[0] {
                Value::Int(a.borrow().len() as i32)
            } else {
                panic!("__arr_len: arg 0 must be array");
            }
        })),
    );

    // push(a: array, item: any) -> void
    register(
        "__arr_push".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                panic!("__arr_push expects 2 arguments");
            }
            if let Value::Array(a) = &args[0] {
                a.borrow_mut().push(args[1].clone());
            } else {
                panic!("__arr_push: arg 0 must be array");
            }
            Value::Void
        })),
    );

    // pop(a: array) -> any
    register(
        "__arr_pop".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 1 {
                panic!("__arr_pop expects 1 argument");
            }
            if let Value::Array(a) = &args[0] {
                a.borrow_mut().pop().unwrap_or(Value::Null)
            } else {
                panic!("__arr_pop: arg 0 must be array");
            }
        })),
    );

    // join(a: array, sep: string) -> string
    register(
        "__arr_join".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                panic!("__arr_join expects 2 arguments");
            }
            let a = if let Value::Array(a) = &args[0] {
                a.borrow()
            } else {
                panic!("__arr_join: arg 0 must be array");
            };
            let sep = if let Value::String(s) = &args[1] {
                s
            } else {
                panic!("__arr_join: arg 1 must be string");
            };

            let mut res = String::new();
            for (i, val) in a.iter().enumerate() {
                if i > 0 {
                    res.push_str(sep);
                }
                // Basic stringify for join
                let val_str = match val {
                    Value::String(s) => s.clone(),
                    Value::Int(i) => i.to_string(),
                    Value::Boolean(b) => b.to_string(),
                    _ => format!("{:?}", val),
                };
                res.push_str(&val_str);
            }
            Value::String(res)
        })),
    );

    // get(a: array, i: i32) -> any
    register(
        "__arr_get".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                panic!("__arr_get expects 2 arguments");
            }
            let a = if let Value::Array(a) = &args[0] {
                a.borrow()
            } else {
                panic!("__arr_get: arg 0 must be array");
            };
            let i = if let Value::Int(i) = args[1] {
                i as usize
            } else {
                panic!("__arr_get: arg 1 must be i32");
            };

            if i < a.len() {
                a[i].clone()
            } else {
                Value::Null
            }
        })),
    );
}
