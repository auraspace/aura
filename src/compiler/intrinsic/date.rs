use crate::compiler::interp::Value;
use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use std::rc::Rc;

pub fn register_date_intrinsics(interp_env: &mut dyn FnMut(String, Value)) {
    // __date_now() -> i64
    interp_env(
        "__date_now".to_string(),
        Value::NativeFunction(Rc::new(|_| Value::Int64(Utc::now().timestamp_millis()))),
    );

    // __date_get_part(ts: i64, part: string) -> i32
    interp_env(
        "__date_get_part".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                return Value::Int(0);
            }
            let ts = match &args[0] {
                Value::Int64(i) => *i,
                _ => return Value::Int(0),
            };
            let part = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => return Value::Int(0),
            };

            let dt = match Utc.timestamp_millis_opt(ts) {
                chrono::LocalResult::Single(dt) => dt,
                _ => return Value::Int(0),
            };

            match part {
                "year" => Value::Int(dt.year()),
                "month" => Value::Int(dt.month() as i32 - 1), // 0-based
                "day" => Value::Int(dt.day() as i32),
                "hours" => Value::Int(dt.hour() as i32),
                "minutes" => Value::Int(dt.minute() as i32),
                "seconds" => Value::Int(dt.second() as i32),
                "milli" => Value::Int((dt.timestamp_subsec_millis()) as i32),
                "weekday" => Value::Int(dt.weekday().num_days_from_sunday() as i32),
                _ => Value::Int(0),
            }
        })),
    );

    // __date_format(ts: i64, format: string) -> string
    interp_env(
        "__date_format".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.len() != 2 {
                return Value::String("".to_string());
            }
            let ts = match &args[0] {
                Value::Int64(i) => *i,
                _ => return Value::String("".to_string()),
            };
            let fmt = match &args[1] {
                Value::String(s) => s.as_str(),
                _ => return Value::String("".to_string()),
            };

            let dt = match Utc.timestamp_millis_opt(ts) {
                chrono::LocalResult::Single(dt) => dt,
                _ => return Value::String("".to_string()),
            };

            Value::String(dt.format(fmt).to_string())
        })),
    );

    // __date_parse(str: string) -> i64
    interp_env(
        "__date_parse".to_string(),
        Value::NativeFunction(Rc::new(|args| {
            if args.is_empty() {
                return Value::Int64(0);
            }
            let s = match &args[0] {
                Value::String(s) => s,
                _ => return Value::Int64(0),
            };

            // Try common formats
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                return Value::Int64(dt.timestamp_millis());
            }

            // Fallback: parse as naive date time if needed or return 0
            Value::Int64(0)
        })),
    );
}
