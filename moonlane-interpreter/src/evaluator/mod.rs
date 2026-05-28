// PoC evaluator — this implementation will almost certainly be rewritten.
// Implement the simplest correct thing; do not over-engineer.

mod builtins;
mod call;
mod display;
mod lvalue;
mod pattern;

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::{BinOp, Literal, Param, Span, UnaryOp};
use crate::error::{FrameInfo, RuntimeErrorCode, MoonlaneError};

thread_local! {
    static CALL_STACK: RefCell<Vec<FrameInfo>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn push_frame(fn_name: String, call_site: Span) {
    CALL_STACK.with(|s| s.borrow_mut().push(FrameInfo { fn_name, call_site }));
}

pub(super) fn pop_frame() {
    CALL_STACK.with(|s| { s.borrow_mut().pop(); });
}

fn snapshot_stack() -> Vec<FrameInfo> {
    CALL_STACK.with(|s| s.borrow().clone())
}

pub(super) fn attach_stack(err: MoonlaneError) -> MoonlaneError {
    err.with_stack(snapshot_stack())
}
use crate::ast::{Block, Decl, Expr, Stmt};
use crate::typed_ast::{FunBody, TypedBlock, TypedDecl, TypedExpr, TypedForInit, TypedModuleGraph, TypedProgram, TypedStmt};

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
    Result(Result<Box<Value>, Box<Value>>),
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

impl Default for Environment {
    fn default() -> Self { Self::new() }
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

// Compute the environment key for an impl method.
//
// `impl From<T> for U` is special: multiple From impls for the same target type
// U can coexist (one per source type T). The key encodes the full impl identity
// as "U::From<T>::from" so each coexists without collision.
//
// All other aspects use the simple "TypeName::method_name" form because at most
// one impl of a given aspect per type is allowed (no disambiguation needed).
fn impl_method_key(type_name: &str, method_name: &str, impl_block: &crate::typed_ast::TypedImplBlock) -> String {
    if impl_block.aspect_name.as_deref() == Some("From")
        && method_name == "from"
        && !impl_block.aspect_type_args.is_empty()
    {
        if let crate::ast::TypeExpr::Named(src, _) = &impl_block.aspect_type_args[0] {
            return format!("{type_name}::From<{src}>::from");
        }
    }
    format!("{type_name}::{method_name}")
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Evaluate a typed module graph produced by `check_graph`.
///
/// Concatenates all module declarations in topological order into a flat program
/// and evaluates them. Per-module runtime environments are deferred to v0.7.0 (#189).
pub fn evaluate_graph(graph: TypedModuleGraph) -> Result<(), MoonlaneError> {
    // Best-effort detection of duplicate top-level names across modules (#192).
    let mut seen_names: HashMap<String, Vec<String>> = HashMap::new();
    for module in &graph.modules {
        for decl in &module.decls {
            let name = match decl {
                TypedDecl::Fun(f) => Some(f.name.clone()),
                TypedDecl::Let(l) => Some(l.name.clone()),
                TypedDecl::Mut(m) => Some(m.name.clone()),
                _ => None,
            };
            if let Some(n) = name {
                seen_names.entry(n).or_default().push(module.module_path.join("::"));
            }
        }
    }
    for (name, modules) in &seen_names {
        if modules.len() > 1 {
            eprintln!(
                "warning: top-level name `{name}` declared in multiple modules ({}); \
                 behaviour is undefined (#189)",
                modules.join(", ")
            );
        }
    }

    // Collect import aliases before consuming graph: alias → canonical_name.
    let all_aliases: Vec<(String, String)> = graph.modules.iter()
        .flat_map(|m| m.import_aliases.iter().map(|(a, c)| (a.clone(), c.clone())))
        .collect();

    // Flatten all module decls into a single program in topological order.
    let flat: TypedProgram = graph.modules.into_iter()
        .flat_map(|m| m.decls)
        .collect();
    evaluate_with_aliases(flat, &all_aliases)
}

/// Like `evaluate`, but after Pass 1b also registers import aliases so that
/// `alias` resolves to the same value as `canonical_name` in the flat env.
fn evaluate_with_aliases(program: TypedProgram, aliases: &[(String, String)]) -> Result<(), MoonlaneError> {
    evaluate_inner(program, aliases)
}

pub fn evaluate(program: TypedProgram) -> Result<(), MoonlaneError> {
    evaluate_inner(program, &[])
}

fn evaluate_inner(program: TypedProgram, aliases: &[(String, String)]) -> Result<(), MoonlaneError> {
    CALL_STACK.with(|s| s.borrow_mut().clear());
    let mut env = Environment::new();
    builtins::register_builtins(&mut env);

    // Pass 1a: define placeholder entries for all top-level functions and methods
    // so that closures created in 1b can capture references to them via shared Rcs.
    for decl in &program {
        match decl {
            TypedDecl::Fun(f) => { env.define(&f.name, Value::Unit); }
            TypedDecl::Impl(impl_block) => {
                if let crate::ast::TypeExpr::Named(type_name, _) = &impl_block.target_type {
                    for method in &impl_block.methods {
                        let key = impl_method_key(type_name, &method.name, impl_block);
                        env.define(&key, Value::Unit);
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
                        let key = impl_method_key(type_name, &method.name, impl_block);
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

    // Register import aliases after all closures are created (Pass 1b complete).
    // Each alias points to the same value as its canonical name so calls like
    // `compute()` (aliased from `answer`) resolve correctly at runtime.
    for (alias, canonical) in aliases {
        if let Some(val) = env.get(canonical) {
            if env.get(alias).is_none() {
                env.define(alias, val);
            }
        }
    }

    // Pass 2: evaluate top-level let/mut bindings and statements in order.
    for decl in &program {
        if !matches!(decl, TypedDecl::Fun(_) | TypedDecl::Impl(_)) {
            eval_decl(decl, &mut env)?;
        }
    }

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

    let mut iter_val = iterable;
    loop {
        let (result_sig, updated_self) = call::call_function_mut_self(next_fn.clone(), vec![iter_val.clone()], span)?;
        iter_val = updated_self;
        let result = result_sig.into_value();
        let maybe_item: Option<Value> = match result {
            Value::Perhaps(None)    => None,
            Value::Perhaps(Some(v)) => Some(*v),
            _ => return Err(MoonlaneError::internal("Iterable::next: expected Perhaps value")),
        };
        match maybe_item {
            None => break,
            Some(item) => {
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

pub(super) fn eval_untyped_block(block: &Block, env: &mut Environment) -> Result<Signal, MoonlaneError> {
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
                Literal::None     => Value::Perhaps(None),
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

        Expr::ResolvedPath { resolved, span, .. } => {
            match env.get(resolved) {
                Some(val) => Ok(Signal::Value(val)),
                None => Err(MoonlaneError::panic(RuntimeErrorCode::R0003, format!("undefined variable `{resolved}`"), span)),
            }
        }

        Expr::Path(segments, _) => { // last-segment fallback below: ADR-0019, ADR-0020
            if segments.len() == 1 {
                let name = &segments[0];
                let span = expr.span();
                match env.get(name) {
                    Some(val) => Ok(Signal::Value(val)),
                    None => Err(MoonlaneError::panic(RuntimeErrorCode::R0003, format!("undefined variable `{name}`"), span)),
                }
            } else {
                // Check full qualified name (e.g. "Circle::new" for static methods).
                let key = segments.join("::");
                if let Some(val) = env.get(&key) {
                    return Ok(Signal::Value(val));
                }
                // Last-segment fallback: `mod::name` evaluates as bare `name` because the
                // flat merge (ADR-0019) binds all declarations under their bare names.
                // Remove when per-module scope is introduced (ADR-0020).
                let last = segments.last().unwrap();
                if let Some(val) = env.get(last) {
                    return Ok(Signal::Value(val));
                }
                let name    = segments[segments.len() - 2].clone();
                let variant = segments[segments.len() - 1].clone();
                if name == "Perhaps" && variant == "None" {
                    return Ok(Signal::Value(Value::Perhaps(None)));
                }
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
            lvalue::eval_binop(op, lv, rv, span)
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
                if !pattern::match_pattern(&arm.pattern, &scrutinee, &mut bindings) { continue; }
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
                        lvalue::apply_assign_op(op, cur, rhs, span)?
                    };
                    if !env.set(name, new_val) {
                        return Err(MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: undefined `{name}`"), span));
                    }
                    Ok(Signal::Value(Value::Unit))
                }
                AssignTarget::Index { object, index, span: tspan } => {
                    let i = lvalue::eval_untyped_index(index, env, tspan)?;
                    let arr_val = lvalue::eval_untyped_lvalue_value(object, env, tspan)?;
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
                                lvalue::apply_assign_op(op, cur, rhs, span)?
                            };
                            rc.borrow_mut()[i as usize] = new_val;
                            Ok(Signal::Value(Value::Unit))
                        }
                        _ => Err(MoonlaneError::internal("index assign: receiver is not an Array")),
                    }
                }
                AssignTarget::FieldAccess { object, field, span: tspan } => {
                    let (root, path) = lvalue::extract_lvalue_path(object, tspan)?;
                    let rc = env.get_rc(root).ok_or_else(|| {
                        MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: `{root}` not found"), tspan)
                    })?;
                    let mut borrowed = rc.borrow_mut();
                    let mut cur: &mut Value = &mut borrowed;
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
                        lvalue::apply_assign_op(op, cur, rhs, span)?
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
                match (path[0].as_str(), path[1].as_str()) {
                    ("Perhaps", "Some") => {
                        let v = field_vals.remove("value").ok_or_else(|| MoonlaneError::internal("Perhaps::Some: missing `value` field"))?;
                        Ok(Signal::Value(Value::Perhaps(Some(Box::new(v)))))
                    }
                    ("Perhaps", "None") => Ok(Signal::Value(Value::Perhaps(None))),
                    ("Result", "Ok") => {
                        let v = field_vals.remove("value").ok_or_else(|| MoonlaneError::internal("Result::Ok: missing `value` field"))?;
                        Ok(Signal::Value(Value::Result(Ok(Box::new(v)))))
                    }
                    ("Result", "Err") => {
                        let e = field_vals.remove("error").ok_or_else(|| MoonlaneError::internal("Result::Err: missing `error` field"))?;
                        Ok(Signal::Value(Value::Result(Err(Box::new(e)))))
                    }
                    _ => Ok(Signal::Value(Value::Enum { name: path[0].clone(), variant: path[1].clone(), fields: field_vals })),
                }
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
            call::call_function(func, all_args, span)
        }

        Expr::Call { callee, args, span } => {
            let func_val = eval_untyped_expr(callee, env)?.into_value();
            let arg_vals: Vec<Value> = args.iter()
                .map(|a| eval_untyped_expr(a, env).map(Signal::into_value))
                .collect::<Result<_, _>>()?;
            call::call_function(func_val, arg_vals, span)
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
                Value::Result(Ok(v))  => Ok(Signal::Value(*v)),
                Value::Result(Err(e)) => Ok(Signal::PropagateErr(*e)),
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
                Literal::None     => Value::Perhaps(None),
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

        TypedExpr::Path(segments, _, _) => { // last-segment fallback below: ADR-0019, ADR-0020
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
                // Check full qualified name (e.g. "Circle::new" for static methods).
                let key = segments.join("::");
                if let Some(val) = env.get(&key) {
                    return Ok(Signal::Value(val));
                }
                // Last-segment fallback: `mod::name` evaluates as bare `name` because the
                // flat merge (ADR-0019) binds all declarations under their bare names.
                // Remove when per-module scope is introduced (ADR-0020).
                let last = segments.last().unwrap();
                if let Some(val) = env.get(last) {
                    return Ok(Signal::Value(val));
                }
                let name    = segments[segments.len() - 2].clone();
                let variant = segments[segments.len() - 1].clone();
                if name == "Perhaps" && variant == "None" {
                    return Ok(Signal::Value(Value::Perhaps(None)));
                }
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
            lvalue::eval_binop(op, lv, rv, span)
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
            // Dispatch through From impl using the full aspect-signature key
            // "Target::From<Source>::from", then fall back to "Target::from"
            // (used by built-in Int::from / Float::from which have no type arg).
            if let crate::ast::TypeExpr::Named(target_name, _) = target_type {
                let src_name = match &v {
                    Value::Struct { name, .. } => Some(name.as_str()),
                    Value::Int(_) => Some("Int"),
                    Value::Float(_) => Some("Float"),
                    Value::Bool(_) => Some("Bool"),
                    Value::Str(_) => Some("String"),
                    _ => None,
                };
                let from_fn = src_name
                    .and_then(|s| env.get(&format!("{target_name}::From<{s}>::from")))
                    .or_else(|| env.get(&format!("{target_name}::from")));
                if let Some(f) = from_fn {
                    return call::call_function(f, vec![v], span);
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
                if !pattern::match_pattern(&arm.pattern, &scrutinee, &mut bindings) {
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
                        lvalue::apply_assign_op(op, cur, rhs, span)?
                    };
                    if !env.set(name, new_val) {
                        return Err(MoonlaneError::panic(
                            RuntimeErrorCode::R0003, format!("assign: undefined `{name}`"), span,
                        ));
                    }
                    Ok(Signal::Value(Value::Unit))
                }

                AssignTarget::Index { object, index, span: tspan } => {
                    let i = lvalue::eval_untyped_index(index, env, tspan)?;
                    let arr_val = lvalue::eval_untyped_lvalue_value(object, env, tspan)?;
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
                                lvalue::apply_assign_op(op, cur, rhs, span)?
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
                    let (root, path) = lvalue::extract_lvalue_path(object, tspan)?;
                    let rc = env.get_rc(root).ok_or_else(|| {
                        MoonlaneError::panic(RuntimeErrorCode::R0003, format!("assign: `{root}` not found"), tspan)
                    })?;
                    let mut borrowed = rc.borrow_mut();
                    // Navigate intermediate path segments to reach the parent struct.
                    let mut cur: &mut Value = &mut borrowed;
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
                        lvalue::apply_assign_op(op, cur, rhs, span)?
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
                match (path[0].as_str(), path[1].as_str()) {
                    ("Perhaps", "Some") => {
                        let v = field_vals.remove("value").ok_or_else(|| MoonlaneError::internal("Perhaps::Some: missing `value` field"))?;
                        Ok(Signal::Value(Value::Perhaps(Some(Box::new(v)))))
                    }
                    ("Perhaps", "None") => Ok(Signal::Value(Value::Perhaps(None))),
                    ("Result", "Ok") => {
                        let v = field_vals.remove("value").ok_or_else(|| MoonlaneError::internal("Result::Ok: missing `value` field"))?;
                        Ok(Signal::Value(Value::Result(Ok(Box::new(v)))))
                    }
                    ("Result", "Err") => {
                        let e = field_vals.remove("error").ok_or_else(|| MoonlaneError::internal("Result::Err: missing `error` field"))?;
                        Ok(Signal::Value(Value::Result(Err(Box::new(e)))))
                    }
                    _ => Ok(Signal::Value(Value::Enum {
                        name:    path[0].clone(),
                        variant: path[1].clone(),
                        fields:  field_vals,
                    })),
                }
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
            call::call_function(func, all_args, span)
        }

        TypedExpr::Call { callee, args, span, .. } => {
            let func_val = eval_expr(callee, env)?.into_value();
            let arg_vals: Vec<Value> = args.iter()
                .map(|a| eval_expr(a, env).map(Signal::into_value))
                .collect::<Result<_, _>>()?;
            call::call_function(func_val, arg_vals, span)
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
                Value::Result(Ok(v))  => Ok(Signal::Value(*v)),
                Value::Result(Err(e)) => {
                    // Apply From coercion if needed.
                    let coerced = if let Some(key) = coercion {
                        if let Some(from_fn) = env.get(key) {
                            call::call_function(from_fn, vec![*e], span)?.into_value()
                        } else { *e }
                    } else { *e };
                    Ok(Signal::PropagateErr(coerced))
                }
                _ => Err(MoonlaneError::panic(RuntimeErrorCode::R0012, "?: expected a Result value", span)),
            }
        }
    }
}

