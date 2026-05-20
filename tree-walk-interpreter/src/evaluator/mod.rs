// PoC evaluator — this implementation will almost certainly be rewritten.
// Implement the simplest correct thing; do not over-engineer.

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::Span;
use crate::error::YoloscriptError;
use crate::typed_ast::TypedProgram;

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
