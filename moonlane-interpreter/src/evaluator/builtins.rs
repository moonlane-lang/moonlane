use crate::error::{MoonlaneError, RuntimeErrorCode};

use super::{Environment, Value};
use super::display::{format_float, format_value, value_to_display_string};

pub(super) fn register_builtins(env: &mut Environment) {
    // print/println dispatch through Display (to_string) for any type.
    env.define("print", Value::Builtin("print".to_string(), |args, span| {
        let s = match args.first() {
            Some(v) => value_to_display_string(v).ok_or_else(|| {
                MoonlaneError::panic(RuntimeErrorCode::R0009, "print: value does not implement Display", span)
            })?,
            None => return Err(MoonlaneError::internal("print: expected one argument")),
        };
        print!("{s}");
        Ok(Value::Unit)
    }));

    env.define("println", Value::Builtin("println".to_string(), |args, span| {
        let s = match args.first() {
            Some(v) => value_to_display_string(v).ok_or_else(|| {
                MoonlaneError::panic(RuntimeErrorCode::R0009, "println: value does not implement Display", span)
            })?,
            None => return Err(MoonlaneError::internal("println: expected one argument")),
        };
        println!("{s}");
        Ok(Value::Unit)
    }));

    // to_string() methods for built-in Display types.
    env.define("Int::to_string", Value::Builtin("Int::to_string".to_string(), |args, _span| {
        match args.first() {
            Some(Value::Int(n)) => Ok(Value::Str(n.to_string())),
            _ => Err(MoonlaneError::internal("Int::to_string: expected Int")),
        }
    }));
    env.define("Float::to_string", Value::Builtin("Float::to_string".to_string(), |args, _span| {
        match args.first() {
            Some(Value::Float(f)) => Ok(Value::Str(format_float(*f))),
            _ => Err(MoonlaneError::internal("Float::to_string: expected Float")),
        }
    }));
    env.define("Bool::to_string", Value::Builtin("Bool::to_string".to_string(), |args, _span| {
        match args.first() {
            Some(Value::Bool(b)) => Ok(Value::Str(if *b { "true" } else { "false" }.to_string())),
            _ => Err(MoonlaneError::internal("Bool::to_string: expected Bool")),
        }
    }));
    env.define("String::to_string", Value::Builtin("String::to_string".to_string(), |args, _span| {
        match args.first() {
            Some(Value::Str(s)) => Ok(Value::Str(s.clone())),
            _ => Err(MoonlaneError::internal("String::to_string: expected String")),
        }
    }));

    // From impls for numeric conversions.
    env.define("Int::from", Value::Builtin("Int::from".to_string(), |args, _span| {
        match args.first() {
            Some(Value::Float(f)) => Ok(Value::Int(*f as i64)),
            Some(Value::Int(n))   => Ok(Value::Int(*n)),
            _ => Err(MoonlaneError::internal("Int::from: expected Float")),
        }
    }));
    env.define("Float::from", Value::Builtin("Float::from".to_string(), |args, _span| {
        match args.first() {
            Some(Value::Int(n))   => Ok(Value::Float(*n as f64)),
            Some(Value::Float(f)) => Ok(Value::Float(*f)),
            _ => Err(MoonlaneError::internal("Float::from: expected Int")),
        }
    }));

    env.define("string_len", Value::Builtin("string_len".to_string(), |args, _span| {
        if let Some(Value::Str(s)) = args.first() {
            Ok(Value::Int(s.chars().count() as i64))
        } else {
            Err(MoonlaneError::internal("string_len: expected String argument"))
        }
    }));

    env.define("string_concat", Value::Builtin("string_concat".to_string(), |args, _span| {
        match (args.first(), args.get(1)) {
            (Some(Value::Str(a)), Some(Value::Str(b))) => Ok(Value::Str(a.clone() + b)),
            _ => Err(MoonlaneError::internal("string_concat: expected two String arguments")),
        }
    }));

    env.define("array_push", Value::Builtin("array_push".to_string(), |args, _span| {
        if let Some(Value::Array(arr)) = args.first() {
            if let Some(val) = args.get(1) {
                arr.borrow_mut().push(val.clone());
                Ok(Value::Unit)
            } else {
                Err(MoonlaneError::internal("array_push: missing value argument"))
            }
        } else {
            Err(MoonlaneError::internal("array_push: expected Array as first argument"))
        }
    }));

    env.define("array_len", Value::Builtin("array_len".to_string(), |args, _span| {
        if let Some(Value::Array(arr)) = args.first() {
            Ok(Value::Int(arr.borrow().len() as i64))
        } else {
            Err(MoonlaneError::internal("array_len: expected Array argument"))
        }
    }));

    env.define("clock", Value::Builtin("clock".to_string(), |_args, _span| {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        Ok(Value::Int(ms))
    }));

    env.define("assert", Value::Builtin("assert".to_string(), |args, span| {
        match args.first() {
            Some(Value::Bool(true)) => Ok(Value::Unit),
            Some(Value::Bool(false)) => Err(MoonlaneError::panic(
                RuntimeErrorCode::R0013,
                "assertion failed",
                span,
            )),
            _ => Err(MoonlaneError::internal("assert: expected Bool argument")),
        }
    }));

    env.define("assert_msg", Value::Builtin("assert_msg".to_string(), |args, span| {
        match (args.first(), args.get(1)) {
            (Some(Value::Bool(true)), _) => Ok(Value::Unit),
            (Some(Value::Bool(false)), Some(Value::Str(msg))) => Err(MoonlaneError::panic(
                RuntimeErrorCode::R0013,
                msg.clone(),
                span,
            )),
            (Some(Value::Bool(false)), _) => Err(MoonlaneError::panic(
                RuntimeErrorCode::R0013,
                "assertion failed",
                span,
            )),
            _ => Err(MoonlaneError::internal("assert_msg: expected (Bool, String) arguments")),
        }
    }));

    env.define("dbg", Value::Builtin("dbg".to_string(), |args, _span| {
        if let Some(val) = args.first() {
            eprintln!("[dbg] {}", format_value(val));
            Ok(val.clone())
        } else {
            Err(MoonlaneError::internal("dbg: expected one argument"))
        }
    }));
}
