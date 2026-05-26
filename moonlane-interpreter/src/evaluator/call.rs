use crate::ast::Span;
use crate::error::{MoonlaneError, RuntimeErrorCode};

use super::{ClosureBody, Signal, Value, attach_stack, eval_block, eval_untyped_block, pop_frame, push_frame};

/// Dispatch a function call to a `Value::Builtin` or `Value::Closure`.
/// Converts `Signal::Return` and `Signal::PropagateErr` at the function boundary.
pub(super) fn call_function(func: Value, args: Vec<Value>, span: &Span) -> Result<Signal, MoonlaneError> {
    match func {
        Value::Builtin(_, f) => f(args, span).map(Signal::Value).map_err(attach_stack),

        Value::Closure(rc) => {
            let closure = (*rc).clone();
            let fn_name = closure.name.clone().unwrap_or_else(|| "<closure>".to_string());
            push_frame(fn_name, span.clone());
            let mut call_env = closure.captured.clone();
            call_env.push_scope();
            for (param, val) in closure.params.iter().zip(args.iter()) {
                call_env.define(&param.name, val.clone());
            }
            let result = match &closure.body {
                ClosureBody::Typed(b)   => eval_block(b, &mut call_env),
                ClosureBody::Untyped(b) => eval_untyped_block(b, &mut call_env),
            };
            let result = result.map_err(attach_stack);
            pop_frame();
            let sig = result?;
            Ok(match sig {
                Signal::Return(v) => Signal::Value(v),
                Signal::PropagateErr(e) => Signal::Value(Value::Result(Err(Box::new(e)))),
                other => other,
            })
        }

        Value::Unit =>
            Err(attach_stack(MoonlaneError::panic(RuntimeErrorCode::R0002, "call: target is Unit, not a function", span))),

        other => Err(attach_stack(MoonlaneError::panic(
            RuntimeErrorCode::R0010,
            format!("call: expected a closure or builtin, got {:?}", std::mem::discriminant(&other)),
            span,
        ))),
    }
}

/// Like `call_function` but also returns the final value of the `self` parameter after the call.
///
/// This exists because the language currently has no mutable references. Structs are value types,
/// so when `next(&mut self)` mutates iterator state, those mutations are local to the call frame
/// and invisible to the caller. Returning `updated_self` lets the for-in loop thread the mutated
/// iterator forward to the next iteration without shared mutable state.
///
/// Once a memory model with mutable references is adopted (see RFC-0001 and related issues),
/// `next` can take `&mut self` and mutate in place. At that point this function becomes
/// unnecessary and `eval_for_in` can call `call_function` directly.
pub(super) fn call_function_mut_self(func: Value, args: Vec<Value>, span: &Span) -> Result<(Signal, Value), MoonlaneError> {
    match func {
        Value::Closure(rc) => {
            let closure = (*rc).clone();
            let self_param_name = closure.params.first().map(|p| p.name.clone());
            let fn_name = closure.name.clone().unwrap_or_else(|| "<closure>".to_string());
            push_frame(fn_name, span.clone());
            let mut call_env = closure.captured.clone();
            call_env.push_scope();
            for (param, val) in closure.params.iter().zip(args.iter()) {
                call_env.define(&param.name, val.clone());
            }
            let result = match &closure.body {
                ClosureBody::Typed(b)   => eval_block(b, &mut call_env),
                ClosureBody::Untyped(b) => eval_untyped_block(b, &mut call_env),
            };
            let updated_self = self_param_name
                .and_then(|n| call_env.get(&n))
                .unwrap_or(Value::Unit);
            let result = result.map_err(attach_stack);
            pop_frame();
            let sig = result?;
            let sig = match sig {
                Signal::Return(v) => Signal::Value(v),
                Signal::PropagateErr(e) => Signal::Value(Value::Result(Err(Box::new(e)))),
                other => other,
            };
            Ok((sig, updated_self))
        }
        other => {
            let sig = call_function(other, args, span)?;
            Ok((sig, Value::Unit))
        }
    }
}
