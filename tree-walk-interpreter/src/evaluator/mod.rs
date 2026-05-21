// PoC evaluator — this implementation will almost certainly be rewritten.
// Implement the simplest correct thing; do not over-engineer.

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::{BinOp, Literal, Span, UnaryOp};
use crate::error::YoloscriptError;
use crate::typed_ast::{TypedExpr, TypedProgram};

// ── Runtime values ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Unit,
    Tuple(Vec<Value>),
    Array(Rc<RefCell<Vec<Value>>>),
    Struct { name: String, fields: HashMap<String, Value> },
    Enum { name: String, variant: String, fields: HashMap<String, Value> },
    Closure(Rc<ClosureValue>),
    Builtin(String, fn(Vec<Value>, &Span) -> Result<Value, YoloscriptError>),
    Perhaps(Option<Box<Value>>),
    YoloResult(Result<Box<Value>, Box<Value>>),
}

#[derive(Debug, Clone)]
pub struct ClosureValue {
    pub params: Vec<String>,
    // body and captured env — filled in when closures are implemented
}

// ── Control flow signals ──────────────────────────────────────────────────────

/// Returned by evaluation functions to handle non-local control flow.
/// Regular expression evaluation returns Signal::Value.
#[derive(Debug)]
pub enum Signal {
    Value(Value),
    Return(Value),
    Break(Value),       // carries value for `loop { break expr; }`
    Continue,
    PropagateErr(Value), // the ? operator
}

impl Signal {
    /// Extract the inner `Value`, consuming the signal.
    /// Panics for non-Value signals — callers that need the full signal must match directly.
    pub fn into_value(self) -> Value {
        match self {
            Signal::Value(v) => v,
            other => panic!("Signal::into_value called on non-Value signal: {other:?}"),
        }
    }
}

// ── Environment ───────────────────────────────────────────────────────────────

/// Lexically-scoped environment — a stack of hashmaps.
/// All values are Rc<RefCell<Value>> so that closures can share mutable bindings
/// with their enclosing scope.
#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<HashMap<String, Rc<RefCell<Value>>>>,
}

impl Environment {
    pub fn new() -> Self {
        Self { scopes: vec![HashMap::new()] }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Define a new binding in the current scope.
    pub fn define(&mut self, name: &str, value: Value) {
        let cell = Rc::new(RefCell::new(value));
        self.scopes.last_mut().unwrap().insert(name.to_string(), cell);
    }

    /// Look up a binding, searching from innermost to outermost scope.
    pub fn get(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(cell) = scope.get(name) {
                return Some(cell.borrow().clone());
            }
        }
        None
    }

    /// Assign to an existing binding anywhere in the scope chain.
    pub fn set(&self, name: &str, value: Value) -> bool {
        for scope in self.scopes.iter().rev() {
            if let Some(cell) = scope.get(name) {
                *cell.borrow_mut() = value;
                return true;
            }
        }
        false
    }

    /// Return the Rc for a binding (used by closures to share mutable state).
    pub fn get_rc(&self, name: &str) -> Option<Rc<RefCell<Value>>> {
        for scope in self.scopes.iter().rev() {
            if let Some(cell) = scope.get(name) {
                return Some(Rc::clone(cell));
            }
        }
        None
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn evaluate(program: TypedProgram) -> Result<(), YoloscriptError> {
    let mut env = Environment::new();
    register_builtins(&mut env);

    // TODO: evaluate each declaration, then call main()
    let _ = program;
    Ok(())
}

// ── Expression evaluation ─────────────────────────────────────────────────────

pub fn eval_expr(expr: &TypedExpr, env: &mut Environment) -> Result<Signal, YoloscriptError> {
    match expr {
        TypedExpr::Literal(lit, _, _) => {
            let val = match lit {
                Literal::Int(n)   => Value::Int(*n),
                Literal::Float(f) => Value::Float(*f),
                Literal::Bool(b)  => Value::Bool(*b),
                Literal::Str(s)   => Value::Str(s.clone()),
                Literal::Nope     => Value::Perhaps(None),
                Literal::Unit     => Value::Unit,
            };
            Ok(Signal::Value(val))
        }

        TypedExpr::Ident(name, _, span) => {
            match env.get(name) {
                Some(val) => Ok(Signal::Value(val)),
                None => Err(YoloscriptError::panic(
                    format!("undefined variable `{name}`"),
                    span,
                )),
            }
        }

        TypedExpr::Path(segments, _, _) => {
            // Unit enum variant: `Colour::Red` → Value::Enum { name: "Colour", variant: "Red", fields: {} }
            // A single-segment path is treated as an ident lookup.
            if segments.len() == 1 {
                let name = &segments[0];
                let span = expr.span();
                match env.get(name) {
                    Some(val) => Ok(Signal::Value(val)),
                    None => Err(YoloscriptError::panic(
                        format!("undefined variable `{name}`"),
                        span,
                    )),
                }
            } else {
                let name    = segments[segments.len() - 2].clone();
                let variant = segments[segments.len() - 1].clone();
                Ok(Signal::Value(Value::Enum {
                    name,
                    variant,
                    fields: HashMap::new(),
                }))
            }
        }

        TypedExpr::Tuple(elems, _, _) => {
            let mut vals = Vec::with_capacity(elems.len());
            for e in elems {
                vals.push(eval_expr(e, env)?.into_value());
            }
            Ok(Signal::Value(Value::Tuple(vals)))
        }

        TypedExpr::Array(elems, _, _) => {
            let mut vals = Vec::with_capacity(elems.len());
            for e in elems {
                vals.push(eval_expr(e, env)?.into_value());
            }
            Ok(Signal::Value(Value::Array(Rc::new(RefCell::new(vals)))))
        }

        TypedExpr::BinOp(lhs, op, rhs, _, span) => {
            // Short-circuit logical ops before evaluating rhs.
            if matches!(op, BinOp::And) {
                let l = eval_expr(lhs, env)?.into_value();
                return match l {
                    Value::Bool(false) => Ok(Signal::Value(Value::Bool(false))),
                    Value::Bool(true)  => eval_expr(rhs, env),
                    _ => Err(YoloscriptError::panic("&&: expected Bool", span)),
                };
            }
            if matches!(op, BinOp::Or) {
                let l = eval_expr(lhs, env)?.into_value();
                return match l {
                    Value::Bool(true)  => Ok(Signal::Value(Value::Bool(true))),
                    Value::Bool(false) => eval_expr(rhs, env),
                    _ => Err(YoloscriptError::panic("||: expected Bool", span)),
                };
            }

            let lv = eval_expr(lhs, env)?.into_value();
            let rv = eval_expr(rhs, env)?.into_value();
            eval_binop(op, lv, rv, span)
        }

        TypedExpr::UnaryOp(op, operand, _, span) => {
            let v = eval_expr(operand, env)?.into_value();
            let result = match (op, v) {
                (UnaryOp::Neg, Value::Int(n))   => Value::Int(-n),
                (UnaryOp::Neg, Value::Float(f)) => Value::Float(-f),
                (UnaryOp::Not, Value::Bool(b))  => Value::Bool(!b),
                (UnaryOp::Neg, _) => return Err(YoloscriptError::panic("unary `-`: expected Int or Float", span)),
                (UnaryOp::Not, _) => return Err(YoloscriptError::panic("unary `!`: expected Bool", span)),
            };
            Ok(Signal::Value(result))
        }

        TypedExpr::Cast { expr: inner, target_type, span, .. } => {
            let v = eval_expr(inner, env)?.into_value();
            let result = match (&v, target_type) {
                (Value::Int(n), crate::ast::TypeExpr::Named(t, _)) if t == "Float" => {
                    Value::Float(*n as f64)
                }
                (Value::Float(f), crate::ast::TypeExpr::Named(t, _)) if t == "Int" => {
                    Value::Int(*f as i64)
                }
                _ => return Err(YoloscriptError::panic(
                    format!("cast: unsupported coercion"),
                    span,
                )),
            };
            Ok(Signal::Value(result))
        }

        TypedExpr::TupleAccess { object, index, span, .. } => {
            let v = eval_expr(object, env)?.into_value();
            match v {
                Value::Tuple(elems) => {
                    elems.into_iter().nth(*index).map(Signal::Value).ok_or_else(|| {
                        YoloscriptError::panic(
                            format!("tuple index {index} out of bounds"),
                            span,
                        )
                    })
                }
                _ => Err(YoloscriptError::panic("tuple access on non-tuple", span)),
            }
        }

        TypedExpr::Index { object, index, span, .. } => {
            let arr = eval_expr(object, env)?.into_value();
            let idx = eval_expr(index, env)?.into_value();
            match (arr, idx) {
                (Value::Array(rc), Value::Int(i)) => {
                    let borrowed = rc.borrow();
                    let len = borrowed.len() as i64;
                    if i < 0 || i >= len {
                        Err(YoloscriptError::panic(
                            format!("index {i} out of bounds (len {len})"),
                            span,
                        ))
                    } else {
                        Ok(Signal::Value(borrowed[i as usize].clone()))
                    }
                }
                _ => Err(YoloscriptError::panic("index: expected Array[Int]", span)),
            }
        }

        // All other variants are handled by later issues.
        other => Err(YoloscriptError::panic(
            format!("eval_expr: unimplemented variant {:?}", other.span()),
            other.span(),
        )),
    }
}

fn eval_binop(op: &BinOp, lv: Value, rv: Value, span: &Span) -> Result<Signal, YoloscriptError> {
    let result = match (op, lv, rv) {
        // Int arithmetic
        (BinOp::Add, Value::Int(a), Value::Int(b)) => Value::Int(a + b),
        (BinOp::Sub, Value::Int(a), Value::Int(b)) => Value::Int(a - b),
        (BinOp::Mul, Value::Int(a), Value::Int(b)) => Value::Int(a * b),
        (BinOp::Div, Value::Int(a), Value::Int(b)) => {
            if b == 0 { return Err(YoloscriptError::panic("division by zero", span)); }
            Value::Int(a / b)
        }
        (BinOp::Rem, Value::Int(a), Value::Int(b)) => {
            if b == 0 { return Err(YoloscriptError::panic("remainder by zero", span)); }
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

        (_, lv, rv) => return Err(YoloscriptError::panic(
            format!("binop: unsupported operand types ({lv:?}, {rv:?})"),
            span,
        )),
    };
    Ok(Signal::Value(result))
}

// ── Built-in functions ────────────────────────────────────────────────────────

fn register_builtins(env: &mut Environment) {
    // Each builtin is a named function value pre-loaded into the root environment.
    // Signatures match the spec's built-in function table.

    env.define("print", Value::Builtin("print".to_string(), |args, span| {
        if let Some(Value::Str(s)) = args.first() {
            print!("{}", s);
            Ok(Value::Unit)
        } else {
            Err(YoloscriptError::panic("print: expected String argument", span))
        }
    }));

    env.define("println", Value::Builtin("println".to_string(), |args, span| {
        if let Some(Value::Str(s)) = args.first() {
            println!("{}", s);
            Ok(Value::Unit)
        } else {
            Err(YoloscriptError::panic("println: expected String argument", span))
        }
    }));

    env.define("int_to_string", Value::Builtin("int_to_string".to_string(), |args, span| {
        if let Some(Value::Int(n)) = args.first() {
            Ok(Value::Str(n.to_string()))
        } else {
            Err(YoloscriptError::panic("int_to_string: expected Int argument", span))
        }
    }));

    env.define("float_to_string", Value::Builtin("float_to_string".to_string(), |args, span| {
        if let Some(Value::Float(f)) = args.first() {
            Ok(Value::Str(f.to_string()))
        } else {
            Err(YoloscriptError::panic("float_to_string: expected Float argument", span))
        }
    }));

    env.define("bool_to_string", Value::Builtin("bool_to_string".to_string(), |args, span| {
        if let Some(Value::Bool(b)) = args.first() {
            Ok(Value::Str(if *b { "true" } else { "false" }.to_string()))
        } else {
            Err(YoloscriptError::panic("bool_to_string: expected Bool argument", span))
        }
    }));

    env.define("string_len", Value::Builtin("string_len".to_string(), |args, span| {
        if let Some(Value::Str(s)) = args.first() {
            Ok(Value::Int(s.chars().count() as i64))
        } else {
            Err(YoloscriptError::panic("string_len: expected String argument", span))
        }
    }));

    env.define("string_concat", Value::Builtin("string_concat".to_string(), |args, span| {
        match (args.get(0), args.get(1)) {
            (Some(Value::Str(a)), Some(Value::Str(b))) => Ok(Value::Str(a.clone() + b)),
            _ => Err(YoloscriptError::panic("string_concat: expected two String arguments", span)),
        }
    }));

    env.define("array_push", Value::Builtin("array_push".to_string(), |args, span| {
        if let Some(Value::Array(arr)) = args.first() {
            if let Some(val) = args.get(1) {
                arr.borrow_mut().push(val.clone());
                Ok(Value::Unit)
            } else {
                Err(YoloscriptError::panic("array_push: missing value argument", span))
            }
        } else {
            Err(YoloscriptError::panic("array_push: expected Array as first argument", span))
        }
    }));

    env.define("array_len", Value::Builtin("array_len".to_string(), |args, span| {
        if let Some(Value::Array(arr)) = args.first() {
            Ok(Value::Int(arr.borrow().len() as i64))
        } else {
            Err(YoloscriptError::panic("array_len: expected Array argument", span))
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
}
