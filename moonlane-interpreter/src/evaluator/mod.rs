// PoC evaluator — this implementation will almost certainly be rewritten.
// Implement the simplest correct thing; do not over-engineer.

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::{BinOp, Literal, Param, Pattern, Span, UnaryOp};
use crate::error::{FrameInfo, RuntimeErrorCode, MoonlaneError};

thread_local! {
    static CALL_STACK: RefCell<Vec<FrameInfo>> = const { RefCell::new(Vec::new()) };
}

fn push_frame(fn_name: String, call_site: Span) {
    CALL_STACK.with(|s| s.borrow_mut().push(FrameInfo { fn_name, call_site }));
}

fn pop_frame() {
    CALL_STACK.with(|s| { s.borrow_mut().pop(); });
}

fn snapshot_stack() -> Vec<FrameInfo> {
    CALL_STACK.with(|s| s.borrow().clone())
}

fn attach_stack(err: MoonlaneError) -> MoonlaneError {
    err.with_stack(snapshot_stack())
}
use crate::ast::{Block, Decl, Expr, Stmt};
use crate::typed_ast::{FunBody, TypedBlock, TypedDecl, TypedExpr, TypedForInit, TypedProgram, TypedStmt};

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
    Builtin(String, fn(Vec<Value>, &Span) -> Result<Value, MoonlaneError>),
    Perhaps(Option<Box<Value>>),
    YoloResult(Result<Box<Value>, Box<Value>>),
    /// Read-only pointer placeholder — never constructed in v0.2; reserved for RFC-0001.
    Pointer(Rc<RefCell<Value>>),
    /// Writable pointer placeholder — never constructed in v0.2; reserved for RFC-0001.
    MutPointer(Rc<RefCell<Value>>),
}

/// The body of a closure — either a fully type-checked block (monomorphic) or the
/// original untyped block (generic / let-polymorphic). The evaluator dispatches on
/// this to choose between `eval_block` and `eval_untyped_block`.
#[derive(Debug, Clone)]
pub enum ClosureBody {
    Typed(TypedBlock),
    Untyped(Block),
}

#[derive(Debug, Clone)]
pub struct ClosureValue {
    pub name:     Option<String>,
    pub params:   Vec<Param>,
    pub body:     ClosureBody,
    pub captured: Environment,
}

/// Deep-clone a value so that arrays get independent copies.
/// Tuples, structs, and enums are recursed into so that nested arrays are also copied.
/// All other value kinds contain no shared mutable state and can be cloned shallowly.
fn deep_clone_value(v: Value) -> Value {
    match v {
        Value::Array(rc) => {
            let cloned: Vec<Value> = rc.borrow().iter().cloned().map(deep_clone_value).collect();
            Value::Array(Rc::new(RefCell::new(cloned)))
        }
        Value::Tuple(items) => Value::Tuple(items.into_iter().map(deep_clone_value).collect()),
        Value::Struct { name, fields } => Value::Struct {
            name,
            fields: fields.into_iter().map(|(k, v)| (k, deep_clone_value(v))).collect(),
        },
        Value::Enum { name, variant, fields } => Value::Enum {
            name,
            variant,
            fields: fields.into_iter().map(|(k, v)| (k, deep_clone_value(v))).collect(),
        },
        other => other,
    }
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
    /// Arrays are deep-cloned so each binding has an independent copy.
    pub fn define(&mut self, name: &str, value: Value) {
        let cell = Rc::new(RefCell::new(deep_clone_value(value)));
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
    /// Arrays are deep-cloned so each binding has an independent copy.
    pub fn set(&self, name: &str, value: Value) -> bool {
        for scope in self.scopes.iter().rev() {
            if let Some(cell) = scope.get(name) {
                *cell.borrow_mut() = deep_clone_value(value);
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

pub fn evaluate(program: TypedProgram) -> Result<(), MoonlaneError> {
    CALL_STACK.with(|s| s.borrow_mut().clear());
    let mut env = Environment::new();
    register_builtins(&mut env);

    // Pass 1a: define placeholder entries for all top-level functions and methods
    // so that closures created in 1b can capture references to them via shared Rcs.
    for decl in &program {
        match decl {
            TypedDecl::Fun(f) => { env.define(&f.name, Value::Unit); }
            TypedDecl::Impl(impl_block) => {
                if let crate::ast::TypeExpr::Named(type_name, _) = &impl_block.target_type {
                    for method in &impl_block.methods {
                        env.define(&format!("{}::{}", type_name, method.name), Value::Unit);
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 1b: create closures that capture the now-complete name set.
    // Using env.set() mutates existing Rc cells, so all already-captured envs
    // (from earlier iterations) see the updates — this "ties the knot" for
    // mutual recursion without a separate fixpoint pass.
    for decl in &program {
        match decl {
            TypedDecl::Fun(f) => {
                let body = match &f.body {
                    FunBody::Typed(b) => ClosureBody::Typed(b.clone()),
                    FunBody::Generic(b) => ClosureBody::Untyped(b.clone()),
                };
                let captured = env.clone();
                env.set(&f.name, Value::Closure(Rc::new(ClosureValue {
                    name:     Some(f.name.clone()),
                    params:   f.params.clone(),
                    body,
                    captured,
                })));
            }
            TypedDecl::Impl(impl_block) => {
                if let crate::ast::TypeExpr::Named(type_name, _) = &impl_block.target_type {
                    for method in &impl_block.methods {
                        let body = match &method.body {
                            FunBody::Typed(b) => ClosureBody::Typed(b.clone()),
                            FunBody::Generic(b) => ClosureBody::Untyped(b.clone()),
                        };
                        let key = format!("{}::{}", type_name, method.name);
                        let captured = env.clone();
                        env.set(&key, Value::Closure(Rc::new(ClosureValue {
                            name:     Some(method.name.clone()),
                            params:   method.params.clone(),
                            body,
                            captured,
                        })));
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 2: evaluate top-level let/mut bindings and statements in order.
    for decl in &program {
        if !matches!(decl, TypedDecl::Fun(_) | TypedDecl::Impl(_)) {
            eval_decl(decl, &mut env)?;
        }
    }

    // Call main() by executing its body directly in the full env so that any
    // top-level let/mut bindings from Pass 2 are visible.
    // Call main() by executing its body in the full top-level env so that any
    // top-level let/mut bindings from Pass 2 are visible.
    let dummy = Span { start: 0, end: 0, filename: "<program>".to_string(), line: 0, col: 0 };
    let main_body = match env.get("main") {
        Some(Value::Closure(rc)) => rc.body.clone(),
        Some(Value::Unit) =>
            return Err(MoonlaneError::panic(RuntimeErrorCode::R0002, "main() is generic — not supported", &dummy)),
        Some(_) =>
            return Err(MoonlaneError::panic(RuntimeErrorCode::R0002, "`main` is not a function", &dummy)),
        None =>
            return Err(MoonlaneError::panic(RuntimeErrorCode::R0001, "no main() function defined", &dummy)),
    };
    let main_sig = match &main_body {
        ClosureBody::Typed(b)   => eval_block(b, &mut env),
        ClosureBody::Untyped(b) => eval_untyped_block(b, &mut env),
    };
    match main_sig? {
        Signal::Value(_) | Signal::Return(_) => Ok(()),
        other => Err(MoonlaneError::internal(
            format!("unexpected signal from main(): {other:?}"),
        )),
    }
}

// ── Block and declaration evaluation ─────────────────────────────────────────

/// Evaluate a block: push scope, run stmts, return tail (or Unit).
/// Non-Value signals (Return, Break, Continue) short-circuit and propagate out.
pub fn eval_block(block: &TypedBlock, env: &mut Environment) -> Result<Signal, MoonlaneError> {
    env.push_scope();
    for decl in &block.stmts {
        let sig = eval_decl(decl, env)?;
        match sig {
            Signal::Value(_) => {}
            other => {
                env.pop_scope();
                return Ok(other);
            }
        }
    }
    let result = match &block.tail {
        Some(tail) => eval_expr(tail, env),
        None       => Ok(Signal::Value(Value::Unit)),
    };
    env.pop_scope();
    result
}

/// Evaluate a single declaration inside a block or at the top level.
fn eval_decl(decl: &TypedDecl, env: &mut Environment) -> Result<Signal, MoonlaneError> {
    match decl {
        TypedDecl::Let(d) => {
            match eval_expr(&d.value, env)? {
                Signal::Value(val) => { env.define(&d.name, val); Ok(Signal::Value(Value::Unit)) }
                other => Ok(other),
            }
        }
        TypedDecl::Mut(d) => {
            match eval_expr(&d.value, env)? {
                Signal::Value(val) => { env.define(&d.name, val); Ok(Signal::Value(Value::Unit)) }
                other => Ok(other),
            }
        }
        TypedDecl::Fun(f) => {
            let body = match &f.body {
                FunBody::Typed(b) => ClosureBody::Typed(b.clone()),
                FunBody::Generic(b) => ClosureBody::Untyped(b.clone()),
            };
            // Define a placeholder first so the closure can see itself via shared Rc
            // (enables self-recursion for functions defined inside other functions).
            env.define(&f.name, Value::Unit);
            let captured = env.clone();
            let closure = Value::Closure(Rc::new(ClosureValue {
                name:     Some(f.name.clone()),
                params:   f.params.clone(),
                body,
                captured,
            }));
            env.set(&f.name, closure);
            Ok(Signal::Value(Value::Unit))
        }
        TypedDecl::Stmt(s) => eval_stmt(s, env),
        // Type-level declarations have no runtime representation.
        TypedDecl::Struct(_) | TypedDecl::Enum(_) | TypedDecl::Impl(_) | TypedDecl::Aspect(_) => {
            Ok(Signal::Value(Value::Unit))
        }
    }
}

// ── Statement evaluation ──────────────────────────────────────────────────────

pub fn eval_stmt(stmt: &TypedStmt, env: &mut Environment) -> Result<Signal, MoonlaneError> {
    match stmt {
        TypedStmt::Expr(e) => {
            // Must propagate non-Value signals (Break/Continue/Return) that arise from
            // control-flow expressions used in statement position, e.g. `if (x) { break; }`.
            match eval_expr(e, env)? {
                Signal::Value(_) => Ok(Signal::Value(Value::Unit)),
                other            => Ok(other),
            }
        }
        TypedStmt::Return(r) => {
            let val = match &r.value {
                Some(e) => eval_expr(e, env)?.into_value(),
                None    => Value::Unit,
            };
            Ok(Signal::Return(val))
        }
        TypedStmt::Break(b) => {
            let val = match &b.value {
                Some(e) => eval_expr(e, env)?.into_value(),
                None    => Value::Unit,
            };
            Ok(Signal::Break(val))
        }
        TypedStmt::Continue(_) => Ok(Signal::Continue),

        TypedStmt::While(w) => {
            loop {
                match eval_expr(&w.condition, env)? {
                    Signal::Value(Value::Bool(false)) => break,
                    Signal::Value(Value::Bool(true))  => {}
                    Signal::Value(_) => return Err(MoonlaneError::internal(
                        "while: expected Bool condition (typechecker should have caught this)",
                    )),
                    other => return Ok(other), // Return / PropagateErr from condition
                }
                match eval_block(&w.body, env)? {
                    Signal::Value(_) | Signal::Continue => {}
                    Signal::Break(_)       => break,
                    Signal::Return(v)      => return Ok(Signal::Return(v)),
                    Signal::PropagateErr(v)=> return Ok(Signal::PropagateErr(v)),
                }
            }
            Ok(Signal::Value(Value::Unit))
        }

        TypedStmt::For(f) => {
            // The init binding lives in its own scope so it doesn't leak out.
            // PoC note: if eval_block errors inside the loop, this scope is not
            // popped (errors are fatal so it doesn't matter in practice).
            env.push_scope();
            if let Some(init) = &f.init {
                match init {
                    TypedForInit::Mut(d) => {
                        let val = eval_expr(&d.value, env)?.into_value();
                        env.define(&d.name, val);
                    }
                    TypedForInit::Expr(e) => { eval_expr(e, env)?; }
                }
            }
            let result = loop {
                if let Some(cond) = &f.condition {
                    match eval_expr(cond, env)? {
                        Signal::Value(Value::Bool(false)) => break Ok(Signal::Value(Value::Unit)),
                        Signal::Value(Value::Bool(true))  => {}
                        Signal::Value(_) => break Err(MoonlaneError::internal(
                            "for: expected Bool condition (typechecker should have caught this)",
                        )),
                        other => break Ok(other),
                    }
                }
                match eval_block(&f.body, env)? {
                    Signal::Value(_) | Signal::Continue => {}
                    Signal::Break(_)        => break Ok(Signal::Value(Value::Unit)),
                    Signal::Return(v)       => break Ok(Signal::Return(v)),
                    Signal::PropagateErr(v) => break Ok(Signal::PropagateErr(v)),
                }
                if let Some(step) = &f.step {
                    eval_expr(step, env)?;
                }
            };
            env.pop_scope();
            result
        }

        TypedStmt::ForIn(fi) => {
            let iterable = eval_expr(&fi.iterable, env)?.into_value();
            eval_for_in(&fi.binding, iterable, &fi.body, &fi.span, env)
        }
    }
}

fn eval_for_in(
    binding: &str,
    iterable: Value,
    body:     &TypedBlock,
    span:     &Span,
    env:      &mut Environment,
) -> Result<Signal, MoonlaneError> {
    // Fast path for built-in sequence types.
    let fast_items: Option<Vec<Value>> = match &iterable {
        Value::Array(rc) => Some(rc.borrow().clone()),
        Value::Struct { name, fields } if name == "Range" => {
            let s = range_field(fields, "start", span)?;
            let e = range_field(fields, "end",   span)?;
            Some((s..e).map(Value::Int).collect())
        }
        Value::Struct { name, fields } if name == "RangeInclusive" => {
            let s = range_field(fields, "start", span)?;
            let e = range_field(fields, "end",   span)?;
            Some((s..=e).map(Value::Int).collect())
        }
        _ => None,
    };

    if let Some(items) = fast_items {
        for item in items {
            env.push_scope();
            env.define(binding, item);
            let sig = eval_block(body, env)?;
            env.pop_scope();
            match sig {
                Signal::Value(_) | Signal::Continue => {}
                Signal::Break(_)        => break,
                Signal::Return(v)       => return Ok(Signal::Return(v)),
                Signal::PropagateErr(v) => return Ok(Signal::PropagateErr(v)),
            }
        }
        return Ok(Signal::Value(Value::Unit));
    }

    // User-defined Iterable: dispatch through TypeName::next.
    let type_name = match &iterable {
        Value::Struct { name, .. } => name.clone(),
        _ => return Err(MoonlaneError::panic(RuntimeErrorCode::R0011,
            "for-in: expected Array, Range, or Iterable value", span)),
    };
    let next_key = format!("{type_name}::next");
    let next_fn = env.get(&next_key).ok_or_else(|| {
        MoonlaneError::panic(RuntimeErrorCode::R0011,
            format!("for-in: `{type_name}` does not implement Iterable (no `next` method)"), span)
    })?;

    // Hold the iterator in an Rc so mutations inside next are visible across calls.
    let iter_cell = Rc::new(RefCell::new(iterable));
    loop {
        let iter_val = iter_cell.borrow().clone();
        let result = call_function(next_fn.clone(), vec![iter_val], span)?.into_value();
        match result {
            Value::Enum { ref name, ref variant, ref fields } if name == "Perhaps" => {
                match variant.as_str() {
                    "Nope" => break,
                    "Some" => {
                        let item = fields.get("value").cloned().ok_or_else(|| {
                            MoonlaneError::internal("Iterable::next: Perhaps::Some missing `value`")
                        })?;
                        env.push_scope();
                        env.define(binding, item);
                        let sig = eval_block(body, env)?;
                        env.pop_scope();
                        match sig {
                            Signal::Value(_) | Signal::Continue => {}
                            Signal::Break(_)        => break,
                            Signal::Return(v)       => return Ok(Signal::Return(v)),
                            Signal::PropagateErr(v) => return Ok(Signal::PropagateErr(v)),
                        }
                    }
                    _ => return Err(MoonlaneError::internal("Iterable::next: unexpected Perhaps variant")),
                }
            }
            _ => return Err(MoonlaneError::internal("Iterable::next: expected Perhaps value")),
        }
    }
    Ok(Signal::Value(Value::Unit))
}

fn range_field(fields: &HashMap<String, Value>, name: &str, _span: &Span) -> Result<i64, MoonlaneError> {
    match fields.get(name) {
        Some(Value::Int(n)) => Ok(*n),
        _ => Err(MoonlaneError::internal(format!("range: missing or non-Int field `{name}`"))),
    }
}

// ── Untyped evaluation (for generic / let-polymorphic closures) ───────────────
//
// These functions mirror eval_block / eval_decl / eval_stmt / eval_expr but operate
// on the untyped AST (`ast::Block`, `ast::Decl`, etc.).  Type annotations are absent
// or ignored; all dispatch is on the runtime `Value` kind.

fn eval_untyped_block(block: &Block, env: &mut Environment) -> Result<Signal, MoonlaneError> {
    env.push_scope();
    for decl in &block.stmts {
        let sig = eval_untyped_decl(decl, env)?;
        match sig {
            Signal::Value(_) => {}
            other => {
                env.pop_scope();
                return Ok(other);
            }
        }
    }
    let result = match &block.tail {
        Some(tail) => eval_untyped_expr(tail, env),
        None       => Ok(Signal::Value(Value::Unit)),
    };
    env.pop_scope();
    result
}

fn eval_untyped_decl(decl: &Decl, env: &mut Environment) -> Result<Signal, MoonlaneError> {
    match decl {
        Decl::Let(d) => {
            match eval_untyped_expr(&d.value, env)? {
                Signal::Value(val) => { env.define(&d.name, val); Ok(Signal::Value(Value::Unit)) }
                other => Ok(other),
            }
        }
        Decl::Mut(d) => {
            match eval_untyped_expr(&d.value, env)? {
                Signal::Value(val) => { env.define(&d.name, val); Ok(Signal::Value(Value::Unit)) }
                other => Ok(other),
            }
        }
        Decl::Fun(f) => {
            let body = ClosureBody::Untyped(f.body.clone());
            env.define(&f.name, Value::Unit);
            let captured = env.clone();
            let closure = Value::Closure(Rc::new(ClosureValue {
                name:     Some(f.name.clone()),
                params:   f.params.clone(),
                body,
                captured,
            }));
            env.set(&f.name, closure);
            Ok(Signal::Value(Value::Unit))
        }
        Decl::Struct(_) | Decl::Enum(_) | Decl::Impl(_) | Decl::Aspect(_) => {
            Ok(Signal::Value(Value::Unit))
        }
        Decl::Stmt(stmt) => eval_untyped_stmt(stmt, env),
    }
}

fn eval_untyped_stmt(stmt: &Stmt, env: &mut Environment) -> Result<Signal, MoonlaneError> {
    use crate::ast::{ForInit, Stmt};
    match stmt {
        Stmt::Expr(e) => {
            match eval_untyped_expr(e, env)? {
                Signal::Value(_) => Ok(Signal::Value(Value::Unit)),
                other            => Ok(other),
            }
        }
        Stmt::Return(r) => {
            let val = match &r.value {
                Some(e) => eval_untyped_expr(e, env)?.into_value(),
                None    => Value::Unit,
            };
            Ok(Signal::Return(val))
        }
        Stmt::Break(b) => {
            let val = match &b.value {
                Some(e) => eval_untyped_expr(e, env)?.into_value(),
                None    => Value::Unit,
            };
            Ok(Signal::Break(val))
        }
        Stmt::Continue(_) => Ok(Signal::Continue),

        Stmt::While(w) => {
            loop {
                match eval_untyped_expr(&w.condition, env)? {
                    Signal::Value(Value::Bool(false)) => break,
                    Signal::Value(Value::Bool(true))  => {}
                    Signal::Value(_) => return Err(MoonlaneError::internal("while: expected Bool condition")),
                    other => return Ok(other),
                }
                match eval_untyped_block(&w.body, env)? {
                    Signal::Value(_) | Signal::Continue => {}
                    Signal::Break(_)        => break,
                    Signal::Return(v)       => return Ok(Signal::Return(v)),
                    Signal::PropagateErr(v) => return Ok(Signal::PropagateErr(v)),
                }
            }
            Ok(Signal::Value(Value::Unit))
        }

        Stmt::For(f) => {
            env.push_scope();
            if let Some(init) = &f.init {
                match init {
                    ForInit::Mut(d) => {
                        let val = eval_untyped_expr(&d.value, env)?.into_value();
                        env.define(&d.name, val);
                    }
                    ForInit::Expr(e) => { eval_untyped_expr(e, env)?; }
                }
            }
            let result = loop {
                if let Some(cond) = &f.condition {
                    match eval_untyped_expr(cond, env)? {
                        Signal::Value(Value::Bool(false)) => break Ok(Signal::Value(Value::Unit)),
                        Signal::Value(Value::Bool(true))  => {}
                        Signal::Value(_) => break Err(MoonlaneError::internal("for: expected Bool condition")),
                        other => break Ok(other),
                    }
                }
                match eval_untyped_block(&f.body, env)? {
                    Signal::Value(_) | Signal::Continue => {}
                    Signal::Break(_)        => break Ok(Signal::Value(Value::Unit)),
                    Signal::Return(v)       => break Ok(Signal::Return(v)),
                    Signal::PropagateErr(v) => break Ok(Signal::PropagateErr(v)),
                }
                if let Some(step) = &f.step {
                    eval_untyped_expr(step, env)?;
                }
            };
            env.pop_scope();
            result
        }

        Stmt::ForIn(fi) => {
            let iterable = eval_untyped_expr(&fi.iterable, env)?.into_value();
            let items: Vec<Value> = match iterable {
                Value::Array(ref rc) => rc.borrow().clone(),
                Value::Struct { ref name, ref fields } if name == "Range" => {
                    let s = range_field(fields, "start", &fi.span)?;
                    let e = range_field(fields, "end",   &fi.span)?;
                    (s..e).map(Value::Int).collect()
                }
                Value::Struct { ref name, ref fields } if name == "RangeInclusive" => {
                    let s = range_field(fields, "start", &fi.span)?;
                    let e = range_field(fields, "end",   &fi.span)?;
                    (s..=e).map(Value::Int).collect()
                }
                _ => return Err(MoonlaneError::panic(RuntimeErrorCode::R0011, "for-in: expected Array or Range", &fi.span)),
            };
            for item in items {
                env.push_scope();
                env.define(&fi.binding, item);
                let sig = eval_untyped_block(&fi.body, env)?;
                env.pop_scope();
                match sig {
                    Signal::Value(_) | Signal::Continue => {}
                    Signal::Break(_)        => break,
                    Signal::Return(v)       => return Ok(Signal::Return(v)),
                    Signal::PropagateErr(v) => return Ok(Signal::PropagateErr(v)),
                }
            }
            Ok(Signal::Value(Value::Unit))
        }
    }
}

fn eval_untyped_expr(expr: &Expr, env: &mut Environment) -> Result<Signal, MoonlaneError> {
    use crate::ast::{AssignOp, AssignTarget, Expr};
    match expr {
        Expr::Literal(lit, _) => {
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

        Expr::Ident(name, span) => {
            match env.get(name) {
                Some(val) => Ok(Signal::Value(val)),
                None => Err(MoonlaneError::panic(RuntimeErrorCode::R0003, format!("undefined variable `{name}`"), span)),
            }
        }

        Expr::Path(segments, _) => {
            if segments.len() == 1 {
                let name = &segments[0];
                let span = expr.span();
                match env.get(name) {
                    Some(val) => Ok(Signal::Value(val)),
                    None => Err(MoonlaneError::panic(RuntimeErrorCode::R0003, format!("undefined variable `{name}`"), span)),
                }
            } else {
                // Check env first (e.g. "Circle::new" for static methods);
                // fall back to unit enum variant construction only if not found.
                let key = segments.join("::");
                if let Some(val) = env.get(&key) {
                    return Ok(Signal::Value(val));
                }
                let name    = segments[segments.len() - 2].clone();
                let variant = segments[segments.len() - 1].clone();
                Ok(Signal::Value(Value::Enum { name, variant, fields: HashMap::new() }))
            }
        }

        Expr::Tuple(elems, _) => {
            let vals = elems.iter()
                .map(|e| eval_untyped_expr(e, env).map(Signal::into_value))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Signal::Value(Value::Tuple(vals)))
        }

        Expr::Array(elems, _) => {
            let vals = elems.iter()
                .map(|e| eval_untyped_expr(e, env).map(Signal::into_value))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Signal::Value(Value::Array(Rc::new(RefCell::new(vals)))))
        }

        Expr::BinOp(lhs, op, rhs, span) => {
            if matches!(op, BinOp::And) {
                let l = eval_untyped_expr(lhs, env)?.into_value();
                return match l {
                    Value::Bool(false) => Ok(Signal::Value(Value::Bool(false))),
                    Value::Bool(true)  => eval_untyped_expr(rhs, env),
                    _ => Err(MoonlaneError::internal("&&: expected Bool")),
                };
            }
            if matches!(op, BinOp::Or) {
                let l = eval_untyped_expr(lhs, env)?.into_value();
                return match l {
                    Value::Bool(true)  => Ok(Signal::Value(Value::Bool(true))),
                    Value::Bool(false) => eval_untyped_expr(rhs, env),
                    _ => Err(MoonlaneError::internal("||: expected Bool")),
                };
            }
            let lv = eval_untyped_expr(lhs, env)?.into_value();
            let rv = eval_untyped_expr(rhs, env)?.into_value();
            eval_binop(op, lv, rv, span)
        }

        Expr::UnaryOp(op, operand, _) => {
            let v = eval_untyped_expr(operand, env)?.into_value();
            let result = match (op, v) {
                (UnaryOp::Neg, Value::Int(n))   => Value::Int(-n),
                (UnaryOp::Neg, Value::Float(f)) => Value::Float(-f),
                (UnaryOp::Not, Value::Bool(b))  => Value::Bool(!b),
                (UnaryOp::Neg, _) => return Err(MoonlaneError::internal("unary `-`: expected Int or Float")),
                (UnaryOp::Not, _) => return Err(MoonlaneError::internal("unary `!`: expected Bool")),
            };
            Ok(Signal::Value(result))
        }

        Expr::Cast { expr: inner, target_type, .. } => {
            let v = eval_untyped_expr(inner, env)?.into_value();
            let result = match (&v, target_type) {
                (Value::Int(n), crate::ast::TypeExpr::Named(t, _)) if t == "Float" => Value::Float(*n as f64),
                (Value::Int(_),   crate::ast::TypeExpr::Named(t, _)) if t == "Int"   => v,
                (Value::Float(_), crate::ast::TypeExpr::Named(t, _)) if t == "Float" => v,
                _ => return Err(MoonlaneError::internal("cast: unsupported coercion")),
            };
            Ok(Signal::Value(result))
        }

        Expr::Ascribe { expr: inner, .. } => eval_untyped_expr(inner, env),

        Expr::TupleAccess { object, index, span } => {
            let v = eval_untyped_expr(object, env)?.into_value();
            match v {
                Value::Tuple(elems) => elems.into_iter().nth(*index).map(Signal::Value).ok_or_else(|| {
                    MoonlaneError::panic(RuntimeErrorCode::R0005, format!("tuple index {index} out of bounds"), span)
                }),
                _ => Err(MoonlaneError::internal("tuple access on non-tuple")),
            }
        }

        Expr::Index { object, index, span } => {
            let arr = eval_untyped_expr(object, env)?.into_value();
            let idx = eval_untyped_expr(index, env)?.into_value();
            match (arr, idx) {
                (Value::Array(rc), Value::Int(i)) => {
                    let borrowed = rc.borrow();
                    let len = borrowed.len() as i64;
                    if i < 0 || i >= len {
                        Err(MoonlaneError::panic(RuntimeErrorCode::R0004, format!("index {i} out of bounds (len {len})"), span))
                    } else {
                        Ok(Signal::Value(borrowed[i as usize].clone()))
                    }
                }
                _ => Err(MoonlaneError::internal("index: expected Array[Int]")),
            }
        }

        Expr::If { condition, then_branch, else_branch, .. } => {
            match eval_untyped_expr(condition, env)? {
                Signal::Value(Value::Bool(true))  => eval_untyped_block(then_branch, env),
                Signal::Value(Value::Bool(false)) => match else_branch {
                    Some(branch) => eval_untyped_block(branch, env),
                    None         => Ok(Signal::Value(Value::Unit)),
                },
                Signal::Value(_) => Err(MoonlaneError::internal("if: expected Bool condition")),
                other => Ok(other),
            }
        }

        Expr::Loop { body, .. } => {
            loop {
                match eval_untyped_block(body, env)? {
                    Signal::Value(_) | Signal::Continue => {}
                    Signal::Break(val)      => return Ok(Signal::Value(val)),
                    Signal::Return(v)       => return Ok(Signal::Return(v)),
                    Signal::PropagateErr(v) => return Ok(Signal::PropagateErr(v)),
                }
            }
        }

        Expr::Match(m) => {
            let scrutinee = eval_untyped_expr(&m.scrutinee, env)?.into_value();
            for arm in &m.arms {
                let mut bindings = HashMap::new();
                if !match_pattern(&arm.pattern, &scrutinee, &mut bindings) { continue; }
                if let Some(guard) = &arm.guard {
                    env.push_scope();
                    for (k, v) in &bindings { env.define(k, v.clone()); }
                    let guard_val = eval_untyped_expr(guard, env)?.into_value();
                    env.pop_scope();
                    match guard_val {
                        Value::Bool(true)  => {}
                        Value::Bool(false) => continue,
                        _ => return Err(MoonlaneError::internal("match guard: expected Bool")),
                    }
                }
                env.push_scope();
                for (k, v) in bindings { env.define(&k, v); }
                let result = eval_untyped_block(&arm.body, env);
                env.pop_scope();
                return result;
            }
            Err(MoonlaneError::panic(RuntimeErrorCode::R0006, "match: no arm matched scrutinee", &m.span))
        }

        Expr::Assign { target, op, value, span } => {
            let rhs = eval_untyped_expr(value, env)?.into_value();
            match target {
                AssignTarget::Ident(name, _) => {
                    let new_val = if matches!(op, AssignOp::Assign) {
                        rhs
                    } else {
                        let cur = env.get(name).ok_or_else(|| {
                            MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: undefined `{name}`"), span)
                        })?;
                        apply_assign_op(op, cur, rhs, span)?
                    };
                    if !env.set(name, new_val) {
                        return Err(MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: undefined `{name}`"), span));
                    }
                    Ok(Signal::Value(Value::Unit))
                }
                AssignTarget::Index { object, index, span: tspan } => {
                    let i = eval_untyped_index(index, env, tspan)?;
                    let arr_val = eval_untyped_lvalue_value(object, env, tspan)?;
                    match arr_val {
                        Value::Array(rc) => {
                            let len = rc.borrow().len() as i64;
                            if i < 0 || i >= len {
                                return Err(MoonlaneError::panic(RuntimeErrorCode::R0004, format!("index {i} out of bounds (len {len})"), span));
                            }
                            let new_val = if matches!(op, AssignOp::Assign) {
                                rhs
                            } else {
                                let cur = rc.borrow()[i as usize].clone();
                                apply_assign_op(op, cur, rhs, span)?
                            };
                            rc.borrow_mut()[i as usize] = new_val;
                            Ok(Signal::Value(Value::Unit))
                        }
                        _ => Err(MoonlaneError::internal("index assign: receiver is not an Array")),
                    }
                }
                AssignTarget::FieldAccess { object, field, span: tspan } => {
                    let (root, path) = extract_lvalue_path(object, tspan)?;
                    let rc = env.get_rc(root).ok_or_else(|| {
                        MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: `{root}` not found"), tspan)
                    })?;
                    let mut borrowed = rc.borrow_mut();
                    let mut cur: &mut Value = &mut *borrowed;
                    for segment in &path {
                        cur = match cur {
                            Value::Struct { fields, .. } | Value::Enum { fields, .. } => {
                                fields.get_mut(*segment).ok_or_else(|| {
                                    MoonlaneError::panic(RuntimeErrorCode::R0008, format!("field assign: no field `{segment}`"), tspan)
                                })?
                            }
                            _ => return Err(MoonlaneError::internal(format!("field assign: `{segment}` is not a struct/enum"))),
                        };
                    }
                    let fields = match cur {
                        Value::Struct { fields, .. } | Value::Enum { fields, .. } => fields,
                        _ => return Err(MoonlaneError::internal("field assign: receiver is not a struct/enum")),
                    };
                    let new_val = if matches!(op, AssignOp::Assign) {
                        rhs
                    } else {
                        let cur = fields.get(field).cloned().ok_or_else(|| {
                            MoonlaneError::panic(RuntimeErrorCode::R0008, format!("field assign: no field `{field}`"), tspan)
                        })?;
                        apply_assign_op(op, cur, rhs, span)?
                    };
                    fields.insert(field.clone(), new_val);
                    Ok(Signal::Value(Value::Unit))
                }
            }
        }

        Expr::StructLiteral { path, fields, span: _ } => {
            let mut field_vals: HashMap<String, Value> = HashMap::new();
            for (name, expr) in fields {
                field_vals.insert(name.clone(), eval_untyped_expr(expr, env)?.into_value());
            }
            if path.len() == 2 {
                Ok(Signal::Value(Value::Enum { name: path[0].clone(), variant: path[1].clone(), fields: field_vals }))
            } else {
                let name = path.last().ok_or_else(|| MoonlaneError::internal("struct literal: empty path"))?.clone();
                Ok(Signal::Value(Value::Struct { name, fields: field_vals }))
            }
        }

        Expr::FieldAccess { object, field, span } => {
            let val = eval_untyped_expr(object, env)?.into_value();
            let fields = match &val {
                Value::Struct { fields, .. } | Value::Enum { fields, .. } => fields,
                _ => return Err(MoonlaneError::internal("field access on non-struct/enum")),
            };
            fields.get(field).cloned().map(Signal::Value).ok_or_else(|| {
                MoonlaneError::panic(RuntimeErrorCode::R0008, format!("no field `{field}` on value"), span)
            })
        }

        Expr::MethodCall { receiver, method, args, span } => {
            let recv_val = eval_untyped_expr(receiver, env)?.into_value();
            let arg_vals: Vec<Value> = args.iter()
                .map(|a| eval_untyped_expr(a, env).map(Signal::into_value))
                .collect::<Result<_, _>>()?;
            if let (Value::Str(s), "len") = (&recv_val, method.as_str()) {
                return Ok(Signal::Value(Value::Int(s.chars().count() as i64)));
            }
            let type_name = match &recv_val {
                Value::Struct { name, .. } | Value::Enum { name, .. } => name.clone(),
                _ => return Err(MoonlaneError::panic(RuntimeErrorCode::R0009, format!("method `{method}` not found on this value"), span)),
            };
            let key = format!("{type_name}::{method}");
            let func = env.get(&key).ok_or_else(|| {
                MoonlaneError::panic(RuntimeErrorCode::R0009, format!("no method `{method}` on `{type_name}`"), span)
            })?;
            let mut all_args = vec![recv_val];
            all_args.extend(arg_vals);
            call_function(func, all_args, span)
        }

        Expr::Call { callee, args, span } => {
            let func_val = eval_untyped_expr(callee, env)?.into_value();
            let arg_vals: Vec<Value> = args.iter()
                .map(|a| eval_untyped_expr(a, env).map(Signal::into_value))
                .collect::<Result<_, _>>()?;
            call_function(func_val, arg_vals, span)
        }

        Expr::Closure { params, body, .. } => {
            let captured = env.clone();
            Ok(Signal::Value(Value::Closure(Rc::new(ClosureValue {
                name:     None,
                params:   params.clone(),
                body:     ClosureBody::Untyped(body.clone()),
                captured,
            }))))
        }

        Expr::PropagateError { expr, span } => {
            let val = eval_untyped_expr(expr, env)?.into_value();
            match val {
                Value::YoloResult(Ok(v))  => Ok(Signal::Value(*v)),
                Value::YoloResult(Err(e)) => Ok(Signal::PropagateErr(*e)),
                Value::Enum { ref name, ref variant, ref fields } if name == "Result" => {
                    match variant.as_str() {
                        "Ok" => {
                            let v = fields.get("value").cloned().ok_or_else(|| MoonlaneError::internal("Result::Ok: missing `value` field"))?;
                            Ok(Signal::Value(v))
                        }
                        "Err" => {
                            let e = fields.get("error").cloned().ok_or_else(|| MoonlaneError::internal("Result::Err: missing `error` field"))?;
                            Ok(Signal::PropagateErr(e))
                        }
                        v => Err(MoonlaneError::internal(format!("?: unknown Result variant `{v}`"))),
                    }
                }
                _ => Err(MoonlaneError::panic(RuntimeErrorCode::R0012, "?: expected a Result value", span)),
            }
        }
    }
}

// ── Expression evaluation ─────────────────────────────────────────────────────

pub fn eval_expr(expr: &TypedExpr, env: &mut Environment) -> Result<Signal, MoonlaneError> {
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
                None => Err(MoonlaneError::panic(
                    RuntimeErrorCode::R0003,
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
                    None => Err(MoonlaneError::panic(
                        RuntimeErrorCode::R0003,
                        format!("undefined variable `{name}`"),
                        span,
                    )),
                }
            } else {
                // Check env first (e.g. "Circle::new" for static methods);
                // fall back to unit enum variant construction only if not found.
                let key = segments.join("::");
                if let Some(val) = env.get(&key) {
                    return Ok(Signal::Value(val));
                }
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
                    _ => Err(MoonlaneError::internal("&&: expected Bool (typechecker should have caught this)")),
                };
            }
            if matches!(op, BinOp::Or) {
                let l = eval_expr(lhs, env)?.into_value();
                return match l {
                    Value::Bool(true)  => Ok(Signal::Value(Value::Bool(true))),
                    Value::Bool(false) => eval_expr(rhs, env),
                    _ => Err(MoonlaneError::internal("||: expected Bool (typechecker should have caught this)")),
                };
            }

            let lv = eval_expr(lhs, env)?.into_value();
            let rv = eval_expr(rhs, env)?.into_value();
            eval_binop(op, lv, rv, span)
        }

        TypedExpr::UnaryOp(op, operand, _, _span) => {
            let v = eval_expr(operand, env)?.into_value();
            let result = match (op, v) {
                (UnaryOp::Neg, Value::Int(n))   => Value::Int(-n),
                (UnaryOp::Neg, Value::Float(f)) => Value::Float(-f),
                (UnaryOp::Not, Value::Bool(b))  => Value::Bool(!b),
                (UnaryOp::Neg, _) => return Err(MoonlaneError::internal("unary `-`: expected Int or Float (typechecker should have caught this)")),
                (UnaryOp::Not, _) => return Err(MoonlaneError::internal("unary `!`: expected Bool (typechecker should have caught this)")),
            };
            Ok(Signal::Value(result))
        }

        TypedExpr::Cast { expr: inner, target_type, span, .. } => {
            let v = eval_expr(inner, env)?.into_value();
            // Dispatch through From impl: look up "TypeName::from" in env.
            if let crate::ast::TypeExpr::Named(target_name, _) = target_type {
                let from_key = format!("{target_name}::from");
                if let Some(from_fn) = env.get(&from_key) {
                    return call_function(from_fn, vec![v], span);
                }
            }
            // Identity cast fallback (same type, no from registered).
            Ok(Signal::Value(v))
        }

        TypedExpr::TupleAccess { object, index, span, .. } => {
            let v = eval_expr(object, env)?.into_value();
            match v {
                Value::Tuple(elems) => {
                    elems.into_iter().nth(*index).map(Signal::Value).ok_or_else(|| {
                        MoonlaneError::panic(
                            RuntimeErrorCode::R0005,
                            format!("tuple index {index} out of bounds"),
                            span,
                        )
                    })
                }
                _ => Err(MoonlaneError::internal("tuple access on non-tuple (typechecker should have caught this)")),
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
                        Err(MoonlaneError::panic(
                            RuntimeErrorCode::R0004,
                            format!("index {i} out of bounds (len {len})"),
                            span,
                        ))
                    } else {
                        Ok(Signal::Value(borrowed[i as usize].clone()))
                    }
                }
                _ => Err(MoonlaneError::internal("index: expected Array[Int] (typechecker should have caught this)")),
            }
        }

        TypedExpr::If { condition, then_branch, else_branch, .. } => {
            match eval_expr(condition, env)? {
                Signal::Value(Value::Bool(true))  => eval_block(then_branch, env),
                Signal::Value(Value::Bool(false)) => match else_branch {
                    Some(branch) => eval_block(branch, env),
                    None         => Ok(Signal::Value(Value::Unit)),
                },
                Signal::Value(_) => Err(MoonlaneError::internal("if: expected Bool condition (typechecker should have caught this)")),
                other => Ok(other), // propagate Return / PropagateErr from condition
            }
        }

        TypedExpr::Loop { body, .. } => {
            loop {
                match eval_block(body, env)? {
                    Signal::Value(_) | Signal::Continue => {}
                    Signal::Break(val)      => return Ok(Signal::Value(val)),
                    Signal::Return(v)       => return Ok(Signal::Return(v)),
                    Signal::PropagateErr(v) => return Ok(Signal::PropagateErr(v)),
                }
            }
        }

        TypedExpr::Match(m) => {
            let scrutinee = eval_expr(&m.scrutinee, env)?.into_value();
            for arm in &m.arms {
                let mut bindings = HashMap::new();
                if !match_pattern(&arm.pattern, &scrutinee, &mut bindings) {
                    continue;
                }
                // Evaluate the guard (if any) in a scope that includes pattern bindings.
                if let Some(guard) = &arm.guard {
                    env.push_scope();
                    for (k, v) in &bindings { env.define(k, v.clone()); }
                    let guard_val = eval_expr(guard, env)?.into_value();
                    env.pop_scope();
                    match guard_val {
                        Value::Bool(true)  => {}
                        Value::Bool(false) => continue,
                        _ => return Err(MoonlaneError::internal("match guard: expected Bool (typechecker should have caught this)")),
                    }
                }
                // Execute the arm body in a scope with pattern bindings.
                env.push_scope();
                for (k, v) in bindings { env.define(&k, v); }
                let result = eval_block(&arm.body, env);
                env.pop_scope();
                return result;
            }
            Err(MoonlaneError::panic(RuntimeErrorCode::R0006, "match: no arm matched scrutinee", &m.span))
        }

        TypedExpr::Assign { target, op, value, span, .. } => {
            use crate::ast::{AssignOp, AssignTarget};
            let rhs = eval_expr(value, env)?.into_value();
            match target {
                AssignTarget::Ident(name, _) => {
                    let new_val = if matches!(op, AssignOp::Assign) {
                        rhs
                    } else {
                        let cur = env.get(name).ok_or_else(|| {
                            MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: undefined `{name}`"), span)
                        })?;
                        apply_assign_op(op, cur, rhs, span)?
                    };
                    if !env.set(name, new_val) {
                        return Err(MoonlaneError::panic(
                            RuntimeErrorCode::R0003, format!("assign: undefined `{name}`"), span,
                        ));
                    }
                    Ok(Signal::Value(Value::Unit))
                }

                AssignTarget::Index { object, index, span: tspan } => {
                    let i = eval_untyped_index(index, env, tspan)?;
                    let arr_val = eval_untyped_lvalue_value(object, env, tspan)?;
                    match arr_val {
                        Value::Array(rc) => {
                            let len = rc.borrow().len() as i64;
                            if i < 0 || i >= len {
                                return Err(MoonlaneError::panic(
                                    RuntimeErrorCode::R0004, format!("index {i} out of bounds (len {len})"), span,
                                ));
                            }
                            let new_val = if matches!(op, AssignOp::Assign) {
                                rhs
                            } else {
                                let cur = rc.borrow()[i as usize].clone();
                                apply_assign_op(op, cur, rhs, span)?
                            };
                            rc.borrow_mut()[i as usize] = new_val;
                            Ok(Signal::Value(Value::Unit))
                        }
                        _ => Err(MoonlaneError::internal(
                            "index assign: receiver is not an Array (typechecker should have caught this)",
                        )),
                    }
                }

                AssignTarget::FieldAccess { object, field, span: tspan } => {
                    let (root, path) = extract_lvalue_path(object, tspan)?;
                    let rc = env.get_rc(root).ok_or_else(|| {
                        MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: `{root}` not found"), tspan)
                    })?;
                    let mut borrowed = rc.borrow_mut();
                    // Navigate intermediate path segments to reach the parent struct.
                    let mut cur: &mut Value = &mut *borrowed;
                    for segment in &path {
                        cur = match cur {
                            Value::Struct { fields, .. } | Value::Enum { fields, .. } => {
                                fields.get_mut(*segment).ok_or_else(|| {
                                    MoonlaneError::panic(RuntimeErrorCode::R0008,
                                        format!("field assign: no field `{segment}`"), tspan)
                                })?
                            }
                            _ => return Err(MoonlaneError::internal(
                                format!("field assign: `{segment}` is not a struct/enum"),
                            )),
                        };
                    }
                    let fields = match cur {
                        Value::Struct { fields, .. } | Value::Enum { fields, .. } => fields,
                        _ => return Err(MoonlaneError::internal(
                            "field assign: receiver is not a struct/enum (typechecker should have caught this)",
                        )),
                    };
                    let new_val = if matches!(op, AssignOp::Assign) {
                        rhs
                    } else {
                        let cur = fields.get(field).cloned().ok_or_else(|| {
                            MoonlaneError::panic(
                                RuntimeErrorCode::R0008, format!("field assign: no field `{field}`"), tspan,
                            )
                        })?;
                        apply_assign_op(op, cur, rhs, span)?
                    };
                    fields.insert(field.clone(), new_val);
                    Ok(Signal::Value(Value::Unit))
                }
            }
        }

        TypedExpr::StructLiteral { path, fields, span: _, .. } => {
            let mut field_vals: HashMap<String, Value> = HashMap::new();
            for (name, expr) in fields {
                field_vals.insert(name.clone(), eval_expr(expr, env)?.into_value());
            }
            if path.len() == 2 {
                // Enum variant with named fields: `Enum::Variant { field: val, .. }`
                Ok(Signal::Value(Value::Enum {
                    name:    path[0].clone(),
                    variant: path[1].clone(),
                    fields:  field_vals,
                }))
            } else {
                let name = path.last().ok_or_else(|| {
                    MoonlaneError::internal("struct literal: empty path")
                })?.clone();
                Ok(Signal::Value(Value::Struct { name, fields: field_vals }))
            }
        }

        TypedExpr::FieldAccess { object, field, span, .. } => {
            let val = eval_expr(object, env)?.into_value();
            let fields = match &val {
                Value::Struct { fields, .. } | Value::Enum { fields, .. } => fields,
                _ => return Err(MoonlaneError::internal("field access on non-struct/enum (typechecker should have caught this)")),
            };
            fields.get(field).cloned().map(Signal::Value).ok_or_else(|| {
                MoonlaneError::panic(RuntimeErrorCode::R0008, format!("no field `{field}` on value"), span)
            })
        }

        TypedExpr::MethodCall { receiver, method, args, span, .. } => {
            let recv_val = eval_expr(receiver, env)?.into_value();
            let arg_vals: Vec<Value> = args.iter()
                .map(|a| eval_expr(a, env).map(Signal::into_value))
                .collect::<Result<_, _>>()?;

            // Built-in type methods.
            if let (Value::Str(s), "len") = (&recv_val, method.as_str()) {
                return Ok(Signal::Value(Value::Int(s.chars().count() as i64)));
            }

            // User-defined struct/enum methods — looked up by "TypeName::method".
            let type_name = match &recv_val {
                Value::Struct { name, .. } | Value::Enum { name, .. } => name.clone(),
                Value::Int(_)   => "Int".to_string(),
                Value::Float(_) => "Float".to_string(),
                Value::Bool(_)  => "Bool".to_string(),
                Value::Str(_)   => "String".to_string(),
                _ => return Err(MoonlaneError::panic(
                    RuntimeErrorCode::R0009,
                    format!("method `{method}` not found on this value"), span,
                )),
            };
            let key = format!("{type_name}::{method}");
            let func = env.get(&key).ok_or_else(|| {
                MoonlaneError::panic(RuntimeErrorCode::R0009, format!("no method `{method}` on `{type_name}`"), span)
            })?;
            // Prepend receiver as first argument (the `self` param).
            let mut all_args = vec![recv_val];
            all_args.extend(arg_vals);
            call_function(func, all_args, span)
        }

        TypedExpr::Call { callee, args, span, .. } => {
            let func_val = eval_expr(callee, env)?.into_value();
            let arg_vals: Vec<Value> = args.iter()
                .map(|a| eval_expr(a, env).map(Signal::into_value))
                .collect::<Result<_, _>>()?;
            call_function(func_val, arg_vals, span)
        }

        TypedExpr::Closure { params, body, .. } => {
            let captured = env.clone();
            Ok(Signal::Value(Value::Closure(Rc::new(ClosureValue {
                name:     None,
                params:   params.clone(),
                body:     ClosureBody::Typed(body.clone()),
                captured,
            }))))
        }

        TypedExpr::GenericClosure { params, body, .. } => {
            let captured = env.clone();
            Ok(Signal::Value(Value::Closure(Rc::new(ClosureValue {
                name:     None,
                params:   params.clone(),
                body:     ClosureBody::Untyped(body.clone()),
                captured,
            }))))
        }

        TypedExpr::PropagateError { expr, coercion, span, .. } => {
            let val = eval_expr(expr, env)?.into_value();
            match val {
                Value::YoloResult(Ok(v))  => Ok(Signal::Value(*v)),
                Value::YoloResult(Err(e)) => Ok(Signal::PropagateErr(*e)),
                Value::Enum { ref name, ref variant, ref fields } if name == "Result" => {
                    match variant.as_str() {
                        "Ok" => {
                            let v = fields.get("value").cloned().ok_or_else(|| {
                                MoonlaneError::internal("Result::Ok: missing `value` field")
                            })?;
                            Ok(Signal::Value(v))
                        }
                        "Err" => {
                            let e = fields.get("error").cloned().ok_or_else(|| {
                                MoonlaneError::internal("Result::Err: missing `error` field")
                            })?;
                            // Apply From coercion if needed.
                            let coerced = if let Some(key) = coercion {
                                if let Some(from_fn) = env.get(key) {
                                    call_function(from_fn, vec![e], span)?.into_value()
                                } else { e }
                            } else { e };
                            Ok(Signal::PropagateErr(coerced))
                        }
                        v => Err(MoonlaneError::internal(format!("?: unknown Result variant `{v}`"))),
                    }
                }
                _ => Err(MoonlaneError::panic(RuntimeErrorCode::R0012, "?: expected a Result value", span)),
            }
        }
    }
}

// ── Pattern matching ──────────────────────────────────────────────────────────

/// Try to match `value` against `pattern`.
/// On success, writes any new name→value bindings into `out` and returns `true`.
/// On failure, returns `false` (bindings already written are harmless — the caller
/// discards the map and tries the next arm).
fn match_pattern(pattern: &Pattern, value: &Value, out: &mut HashMap<String, Value>) -> bool {
    match pattern {
        Pattern::Wildcard(_) => true,

        Pattern::Nope(_) => matches!(value, Value::Perhaps(None)),

        Pattern::Literal(lit, _) => match (lit, value) {
            (Literal::Int(a),   Value::Int(b))          => a == b,
            (Literal::Float(a), Value::Float(b))        => a == b,
            (Literal::Bool(a),  Value::Bool(b))         => a == b,
            (Literal::Str(a),   Value::Str(b))          => a == b,
            (Literal::Unit,     Value::Unit)             => true,
            (Literal::Nope,     Value::Perhaps(None))   => true,
            _ => false,
        },

        Pattern::Binding(name, _) => {
            out.insert(name.clone(), value.clone());
            true
        }

        Pattern::Tuple(sub_patterns, _) => match value {
            Value::Tuple(elems) if elems.len() == sub_patterns.len() => {
                sub_patterns.iter().zip(elems.iter())
                    .all(|(p, v)| match_pattern(p, v, out))
            }
            _ => false,
        },

        Pattern::EnumVariant { path, fields, .. } => {
            let variant_name = path.last().map(String::as_str).unwrap_or("");
            match value {
                Value::Enum { variant, fields: enum_fields, .. } if variant == variant_name => {
                    for field_name in fields {
                        match enum_fields.get(field_name) {
                            Some(v) => { out.insert(field_name.clone(), v.clone()); }
                            None    => return false,
                        }
                    }
                    true
                }
                _ => false,
            }
        }
    }
}

// ── Function call dispatch ────────────────────────────────────────────────────

/// Dispatch a function call to a `Value::Builtin` or `Value::Closure`.
/// Converts `Signal::Return` and `Signal::PropagateErr` at the function boundary.
fn call_function(func: Value, args: Vec<Value>, span: &Span) -> Result<Signal, MoonlaneError> {
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
                Signal::PropagateErr(e) => Signal::Value(Value::Enum {
                    name:    "Result".to_string(),
                    variant: "Err".to_string(),
                    fields:  { let mut m = HashMap::new(); m.insert("error".to_string(), e); m },
                }),
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

// ── Assignment and binary operators ──────────────────────────────────────────

/// Evaluate a simple index expression (Ident or Int literal) from an untyped Expr.
/// The typechecker validates these, so only the most common forms appear in practice.
fn eval_untyped_index(
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

/// Evaluate an lvalue receiver expression to a Value.
/// Supports bare identifiers and field-access chains — sufficient for array index
/// assignment since Value::Array is Rc-backed (the returned Rc shares data with the env).
fn eval_untyped_lvalue_value(
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

/// Walk an untyped lvalue Expr chain (Ident or FieldAccess) and return
/// (root_name, [intermediate_field_path]) for use with env.get_rc + borrow_mut navigation.
fn extract_lvalue_path<'a>(
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

fn apply_assign_op(
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

fn eval_binop(op: &BinOp, lv: Value, rv: Value, span: &Span) -> Result<Signal, MoonlaneError> {
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

// ── Built-in functions ────────────────────────────────────────────────────────

fn value_to_display_string(v: &Value) -> Option<String> {
    match v {
        Value::Int(n)   => Some(n.to_string()),
        Value::Float(f) => Some(format_float(*f)),
        Value::Bool(b)  => Some(if *b { "true" } else { "false" }.to_string()),
        Value::Str(s)   => Some(s.clone()),
        _ => None,
    }
}

fn format_float(f: f64) -> String {
    if f.fract() == 0.0 && f.is_finite() {
        format!("{}", f as i64)
    } else {
        f.to_string()
    }
}

fn register_builtins(env: &mut Environment) {
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
        match (args.get(0), args.get(1)) {
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

fn format_value(val: &Value) -> String {
    match val {
        Value::Int(n)   => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b)  => b.to_string(),
        Value::Str(s)   => format!("{:?}", s),
        Value::Unit     => "()".to_string(),
        Value::Tuple(items) => {
            let inner = items.iter().map(format_value).collect::<Vec<_>>().join(", ");
            format!("({})", inner)
        }
        Value::Array(arr) => {
            let inner = arr.borrow().iter().map(format_value).collect::<Vec<_>>().join(", ");
            format!("[{}]", inner)
        }
        Value::Struct { name, fields } => {
            let mut pairs: Vec<_> = fields.iter().collect();
            pairs.sort_by_key(|(k, _)| k.as_str());
            let inner = pairs.iter().map(|(k, v)| format!("{}: {}", k, format_value(v))).collect::<Vec<_>>().join(", ");
            format!("{} {{ {} }}", name, inner)
        }
        Value::Enum { name, variant, fields } => {
            if fields.is_empty() {
                format!("{}::{}", name, variant)
            } else {
                let mut pairs: Vec<_> = fields.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                let inner = pairs.iter().map(|(k, v)| format!("{}: {}", k, format_value(v))).collect::<Vec<_>>().join(", ");
                format!("{}::{}{{ {} }}", name, variant, inner)
            }
        }
        Value::Closure(_) => "<closure>".to_string(),
        Value::Builtin(name, _) => format!("<builtin:{}>", name),
        Value::Perhaps(Some(v)) => format!("Some({})", format_value(v)),
        Value::Perhaps(None) => "None".to_string(),
        Value::YoloResult(Ok(v)) => format!("Ok({})", format_value(v)),
        Value::YoloResult(Err(e)) => format!("Err({})", format_value(e)),
        // RFC-0001 (pointer syntax) placeholder variants — not constructed until that RFC is implemented.
        Value::Pointer(_) | Value::MutPointer(_) => unreachable!("pointer values not constructed until RFC-0001 is implemented"),
    }
}
