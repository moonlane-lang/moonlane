use std::collections::HashMap;

use crate::ast::{BinOp, Literal, Span};
use crate::error::{MoonlaneError, RuntimeErrorCode};

use super::{Environment, Signal, Value};

pub(super) fn eval_untyped_index(
    expr: &crate::ast::Expr,
    env: &Environment,
    _span: &Span,
) -> Result<i64, MoonlaneError> {
    use crate::ast::Expr;
    match expr {
        Expr::Literal(Literal::Int(n), _) => Ok(*n),
        Expr::Ident(name, _) => match env.get(name) {
            Some(Value::Int(n)) => Ok(n),
            Some(_) => Err(MoonlaneError::internal(format!("`{name}` is not an Int"))),
            None    => Err(MoonlaneError::internal(format!("eval_untyped_index: undefined `{name}`"))),
        },
        _ => Err(MoonlaneError::internal(
            "index expression too complex; assign the index to a variable first",
        )),
    }
}

pub(super) fn eval_untyped_lvalue_value(
    expr: &crate::ast::Expr,
    env: &Environment,
    span: &Span,
) -> Result<Value, MoonlaneError> {
    use crate::ast::Expr;
    match expr {
        Expr::Ident(name, _) => env.get(name).ok_or_else(|| {
            MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: `{name}` not found"), span)
        }),
        Expr::FieldAccess { object, field, span: fspan } => {
            let parent = eval_untyped_lvalue_value(object, env, fspan)?;
            match parent {
                Value::Struct { fields, .. } | Value::Enum { fields, .. } => {
                    fields.get(field).cloned().ok_or_else(|| {
                        MoonlaneError::panic(RuntimeErrorCode::R0008,
                            format!("field access: no field `{field}`"), fspan)
                    })
                }
                _ => Err(MoonlaneError::internal(format!(
                    "field access: `{field}` receiver is not a struct/enum"
                ))),
            }
        }
        _ => Err(MoonlaneError::internal(
            "assign receiver too complex; assign to a variable first",
        )),
    }
}

pub(super) fn extract_lvalue_path<'a>(
    expr: &'a crate::ast::Expr,
    span: &Span,
) -> Result<(&'a str, Vec<&'a str>), MoonlaneError> {
    use crate::ast::Expr;
    fn walk<'a>(expr: &'a Expr, path: &mut Vec<&'a str>, span: &Span) -> Result<&'a str, MoonlaneError> {
        match expr {
            Expr::Ident(name, _) => Ok(name.as_str()),
            Expr::FieldAccess { object, field, .. } => {
                let root = walk(object, path, span)?;
                path.push(field.as_str());
                Ok(root)
            }
            _ => Err(MoonlaneError::panic(
                RuntimeErrorCode::R0003,
                "field assign: receiver must be a variable or field access chain",
                span,
            )),
        }
    }
    let mut path = Vec::new();
    let root = walk(expr, &mut path, span)?;
    Ok((root, path))
}

pub(super) fn apply_assign_op(
    op: &crate::ast::AssignOp,
    cur: Value,
    rhs: Value,
    span: &Span,
) -> Result<Value, MoonlaneError> {
    use crate::ast::AssignOp;
    let fake_binop = match op {
        AssignOp::AddAssign => BinOp::Add,
        AssignOp::SubAssign => BinOp::Sub,
        AssignOp::MulAssign => BinOp::Mul,
        AssignOp::DivAssign => BinOp::Div,
        AssignOp::RemAssign => BinOp::Rem,
        AssignOp::Assign    => unreachable!("plain Assign handled before apply_assign_op"),
    };
    eval_binop(&fake_binop, cur, rhs, span).map(Signal::into_value)
}

pub(super) fn eval_binop(op: &BinOp, lv: Value, rv: Value, span: &Span) -> Result<Signal, MoonlaneError> {
    let result = match (op, lv, rv) {
        // Int arithmetic
        (BinOp::Add, Value::Int(a), Value::Int(b)) => Value::Int(a + b),
        (BinOp::Sub, Value::Int(a), Value::Int(b)) => Value::Int(a - b),
        (BinOp::Mul, Value::Int(a), Value::Int(b)) => Value::Int(a * b),
        (BinOp::Div, Value::Int(a), Value::Int(b)) => {
            if b == 0 { return Err(MoonlaneError::panic(RuntimeErrorCode::R0007, "division by zero", span)); }
            Value::Int(a / b)
        }
        (BinOp::Rem, Value::Int(a), Value::Int(b)) => {
            if b == 0 { return Err(MoonlaneError::panic(RuntimeErrorCode::R0007, "remainder by zero", span)); }
            Value::Int(a % b)
        }

        // Float arithmetic
        (BinOp::Add, Value::Float(a), Value::Float(b)) => Value::Float(a + b),
        (BinOp::Sub, Value::Float(a), Value::Float(b)) => Value::Float(a - b),
        (BinOp::Mul, Value::Float(a), Value::Float(b)) => Value::Float(a * b),
        (BinOp::Div, Value::Float(a), Value::Float(b)) => Value::Float(a / b),
        (BinOp::Rem, Value::Float(a), Value::Float(b)) => Value::Float(a % b),

        // Int comparison
        (BinOp::Eq, Value::Int(a), Value::Int(b)) => Value::Bool(a == b),
        (BinOp::Ne, Value::Int(a), Value::Int(b)) => Value::Bool(a != b),
        (BinOp::Lt, Value::Int(a), Value::Int(b)) => Value::Bool(a <  b),
        (BinOp::Le, Value::Int(a), Value::Int(b)) => Value::Bool(a <= b),
        (BinOp::Gt, Value::Int(a), Value::Int(b)) => Value::Bool(a >  b),
        (BinOp::Ge, Value::Int(a), Value::Int(b)) => Value::Bool(a >= b),

        // Float comparison
        (BinOp::Eq, Value::Float(a), Value::Float(b)) => Value::Bool(a == b),
        (BinOp::Ne, Value::Float(a), Value::Float(b)) => Value::Bool(a != b),
        (BinOp::Lt, Value::Float(a), Value::Float(b)) => Value::Bool(a <  b),
        (BinOp::Le, Value::Float(a), Value::Float(b)) => Value::Bool(a <= b),
        (BinOp::Gt, Value::Float(a), Value::Float(b)) => Value::Bool(a >  b),
        (BinOp::Ge, Value::Float(a), Value::Float(b)) => Value::Bool(a >= b),

        // Bool equality
        (BinOp::Eq, Value::Bool(a), Value::Bool(b)) => Value::Bool(a == b),
        (BinOp::Ne, Value::Bool(a), Value::Bool(b)) => Value::Bool(a != b),

        // String equality
        (BinOp::Eq, Value::Str(a), Value::Str(b)) => Value::Bool(a == b),
        (BinOp::Ne, Value::Str(a), Value::Str(b)) => Value::Bool(a != b),

        // Range — produce a Struct value understood by for-in (issue #55)
        (BinOp::Range, Value::Int(a), Value::Int(b)) => Value::Struct {
            name: "Range".to_string(),
            fields: {
                let mut m = HashMap::new();
                m.insert("start".to_string(), Value::Int(a));
                m.insert("end".to_string(),   Value::Int(b));
                m
            },
        },
        (BinOp::RangeInclusive, Value::Int(a), Value::Int(b)) => Value::Struct {
            name: "RangeInclusive".to_string(),
            fields: {
                let mut m = HashMap::new();
                m.insert("start".to_string(), Value::Int(a));
                m.insert("end".to_string(),   Value::Int(b));
                m
            },
        },

        (_, lv, rv) => return Err(MoonlaneError::internal(
            format!("binop: unsupported operand types ({lv:?}, {rv:?}) (typechecker should have caught this)"),
        )),
    };
    Ok(Signal::Value(result))
}
