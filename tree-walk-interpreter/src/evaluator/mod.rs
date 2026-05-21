// PoC evaluator — this implementation will almost certainly be rewritten.
// Implement the simplest correct thing; do not over-engineer.

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::{BinOp, Literal, Param, Pattern, Span, UnaryOp};
use crate::error::YoloscriptError;
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
    /// A named function definition hoisted into the environment.
    /// Actual call dispatch is implemented in #4; the entry point uses this directly for main().
    Function { name: String, params: Vec<Param>, body: FunBody },
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

    // Pass 1: hoist function declarations and impl block methods so forward
    // references work regardless of declaration order.
    for decl in &program {
        match decl {
            TypedDecl::Fun(f) => {
                env.define(&f.name, Value::Function {
                    name:   f.name.clone(),
                    params: f.params.clone(),
                    body:   f.body.clone(),
                });
            }
            TypedDecl::Impl(impl_block) => {
                // Register each method under "TypeName::method_name" so method
                // dispatch in eval_expr can look them up without a separate table.
                if let crate::ast::TypeExpr::Named(type_name, _) = &impl_block.target_type {
                    for method in &impl_block.methods {
                        let key = format!("{}::{}", type_name, method.name);
                        env.define(&key, Value::Function {
                            name:   method.name.clone(),
                            params: method.params.clone(),
                            body:   method.body.clone(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 2: evaluate top-level let/mut bindings and statements in order.
    // Fun and Impl are already handled in Pass 1.
    for decl in &program {
        if !matches!(decl, TypedDecl::Fun(_) | TypedDecl::Impl(_)) {
            eval_decl(decl, &mut env)?;
        }
    }

    // Call main() directly — it takes no arguments, so we execute its body without
    // the full call-dispatch machinery that will come in #4.
    let dummy = Span { start: 0, end: 0, filename: "<program>".to_string() };
    let main_body = match env.get("main") {
        Some(Value::Function { body: FunBody::Typed(b), .. }) => b,
        Some(Value::Function { body: FunBody::Generic(_), .. }) =>
            return Err(YoloscriptError::panic("main() must not be generic", &dummy)),
        Some(_) =>
            return Err(YoloscriptError::panic("`main` is not a function", &dummy)),
        None =>
            return Err(YoloscriptError::panic("no main() function defined", &dummy)),
    };
    match eval_block(&main_body, &mut env)? {
        Signal::Value(_) | Signal::Return(_) => Ok(()),
        other => Err(YoloscriptError::panic(
            format!("unexpected signal from main(): {other:?}"),
            &dummy,
        )),
    }
}

// ── Block and declaration evaluation ─────────────────────────────────────────

/// Evaluate a block: push scope, run stmts, return tail (or Unit).
/// Non-Value signals (Return, Break, Continue) short-circuit and propagate out.
pub fn eval_block(block: &TypedBlock, env: &mut Environment) -> Result<Signal, YoloscriptError> {
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
fn eval_decl(decl: &TypedDecl, env: &mut Environment) -> Result<Signal, YoloscriptError> {
    match decl {
        TypedDecl::Let(d) => {
            let val = eval_expr(&d.value, env)?.into_value();
            env.define(&d.name, val);
            Ok(Signal::Value(Value::Unit))
        }
        TypedDecl::Mut(d) => {
            let val = eval_expr(&d.value, env)?.into_value();
            env.define(&d.name, val);
            Ok(Signal::Value(Value::Unit))
        }
        TypedDecl::Fun(f) => {
            env.define(&f.name, Value::Function {
                name:   f.name.clone(),
                params: f.params.clone(),
                body:   f.body.clone(),
            });
            Ok(Signal::Value(Value::Unit))
        }
        TypedDecl::Stmt(s) => eval_stmt(s, env),
        // Type-level declarations have no runtime representation.
        TypedDecl::Struct(_) | TypedDecl::Enum(_) | TypedDecl::Impl(_) | TypedDecl::Trait(_) => {
            Ok(Signal::Value(Value::Unit))
        }
    }
}

// ── Statement evaluation ──────────────────────────────────────────────────────

pub fn eval_stmt(stmt: &TypedStmt, env: &mut Environment) -> Result<Signal, YoloscriptError> {
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
                    Signal::Value(_) => return Err(YoloscriptError::panic(
                        "while: expected Bool condition", &w.span,
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
                        Signal::Value(_) => break Err(YoloscriptError::panic(
                            "for: expected Bool condition", &f.span,
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
) -> Result<Signal, YoloscriptError> {
    let items: Vec<Value> = match iterable {
        Value::Array(rc) => rc.borrow().clone(),
        Value::Struct { ref name, ref fields } if name == "Range" => {
            let s = range_field(fields, "start", span)?;
            let e = range_field(fields, "end",   span)?;
            (s..e).map(Value::Int).collect()
        }
        Value::Struct { ref name, ref fields } if name == "RangeInclusive" => {
            let s = range_field(fields, "start", span)?;
            let e = range_field(fields, "end",   span)?;
            (s..=e).map(Value::Int).collect()
        }
        _ => return Err(YoloscriptError::panic("for-in: expected Array or Range", span)),
    };

    for item in items {
        // Push a scope for the loop variable, then eval_block pushes its own inner scope.
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
    Ok(Signal::Value(Value::Unit))
}

fn range_field(fields: &HashMap<String, Value>, name: &str, span: &Span) -> Result<i64, YoloscriptError> {
    match fields.get(name) {
        Some(Value::Int(n)) => Ok(*n),
        _ => Err(YoloscriptError::panic(format!("range: missing or non-Int field `{name}`"), span)),
    }
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
            // v0.1: only `Int as Float` (widening) and identity casts reach here —
            // the typechecker rejects all other forms before evaluation.
            // TODO(Epic 004, task 0002): replace with From<S> trait dispatch.
            let v = eval_expr(inner, env)?.into_value();
            let result = match (&v, target_type) {
                (Value::Int(n), crate::ast::TypeExpr::Named(t, _)) if t == "Float" => {
                    Value::Float(*n as f64)
                }
                // Identity casts
                (Value::Int(_),   crate::ast::TypeExpr::Named(t, _)) if t == "Int"   => v,
                (Value::Float(_), crate::ast::TypeExpr::Named(t, _)) if t == "Float" => v,
                _ => return Err(YoloscriptError::panic(
                    "cast: unsupported coercion (should have been caught by typechecker)",
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

        TypedExpr::If { condition, then_branch, else_branch, span, .. } => {
            match eval_expr(condition, env)? {
                Signal::Value(Value::Bool(true))  => eval_block(then_branch, env),
                Signal::Value(Value::Bool(false)) => match else_branch {
                    Some(branch) => eval_block(branch, env),
                    None         => Ok(Signal::Value(Value::Unit)),
                },
                Signal::Value(_) => Err(YoloscriptError::panic("if: expected Bool condition", span)),
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
                        _ => return Err(YoloscriptError::panic("match guard: expected Bool", &arm.span)),
                    }
                }
                // Execute the arm body in a scope with pattern bindings.
                env.push_scope();
                for (k, v) in bindings { env.define(&k, v); }
                let result = eval_expr(&arm.body, env);
                env.pop_scope();
                return result;
            }
            Err(YoloscriptError::panic("match: no arm matched scrutinee", &m.span))
        }

        TypedExpr::Assign { target, op, value, span, .. } => {
            use crate::ast::{AssignOp, AssignTarget, Expr};
            let rhs = eval_expr(value, env)?.into_value();
            match target {
                AssignTarget::Ident(name, _) => {
                    let new_val = if matches!(op, AssignOp::Assign) {
                        rhs
                    } else {
                        let cur = env.get(name).ok_or_else(|| {
                            YoloscriptError::panic(format!("assign: undefined `{name}`"), span)
                        })?;
                        apply_assign_op(op, cur, rhs, span)?
                    };
                    if !env.set(name, new_val) {
                        return Err(YoloscriptError::panic(
                            format!("assign: undefined `{name}`"), span,
                        ));
                    }
                    Ok(Signal::Value(Value::Unit))
                }

                AssignTarget::Index { object, index, span: tspan } => {
                    let arr_name = match object.as_ref() {
                        Expr::Ident(n, _) => n,
                        _ => return Err(YoloscriptError::panic(
                            "index assign: only `ident[...]` supported in PoC", tspan,
                        )),
                    };
                    let i = eval_untyped_index(index, env, tspan)?;
                    let arr_val = env.get(arr_name).ok_or_else(|| {
                        YoloscriptError::panic(format!("assign: `{arr_name}` not found"), tspan)
                    })?;
                    match arr_val {
                        Value::Array(rc) => {
                            let len = rc.borrow().len() as i64;
                            if i < 0 || i >= len {
                                return Err(YoloscriptError::panic(
                                    format!("index {i} out of bounds (len {len})"), span,
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
                        _ => Err(YoloscriptError::panic(
                            format!("index assign: `{arr_name}` is not an Array"), tspan,
                        )),
                    }
                }

                AssignTarget::FieldAccess { object, field, span: tspan } => {
                    let obj_name = match object.as_ref() {
                        Expr::Ident(n, _) => n,
                        _ => return Err(YoloscriptError::panic(
                            "field assign: only `ident.field` supported in PoC", tspan,
                        )),
                    };
                    let rc = env.get_rc(obj_name).ok_or_else(|| {
                        YoloscriptError::panic(format!("assign: `{obj_name}` not found"), tspan)
                    })?;
                    let mut borrowed = rc.borrow_mut();
                    let fields = match &mut *borrowed {
                        Value::Struct { fields, .. } | Value::Enum { fields, .. } => fields,
                        _ => return Err(YoloscriptError::panic(
                            format!("field assign: `{obj_name}` is not a struct/enum"), tspan,
                        )),
                    };
                    let new_val = if matches!(op, AssignOp::Assign) {
                        rhs
                    } else {
                        let cur = fields.get(field).cloned().ok_or_else(|| {
                            YoloscriptError::panic(
                                format!("field assign: no field `{field}`"), tspan,
                            )
                        })?;
                        apply_assign_op(op, cur, rhs, span)?
                    };
                    fields.insert(field.clone(), new_val);
                    Ok(Signal::Value(Value::Unit))
                }
            }
        }

        TypedExpr::StructLiteral { path, fields, span, .. } => {
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
                    YoloscriptError::panic("struct literal: empty path", span)
                })?.clone();
                Ok(Signal::Value(Value::Struct { name, fields: field_vals }))
            }
        }

        TypedExpr::FieldAccess { object, field, span, .. } => {
            let val = eval_expr(object, env)?.into_value();
            let fields = match &val {
                Value::Struct { fields, .. } | Value::Enum { fields, .. } => fields,
                _ => return Err(YoloscriptError::panic("field access on non-struct/enum", span)),
            };
            fields.get(field).cloned().map(Signal::Value).ok_or_else(|| {
                YoloscriptError::panic(format!("no field `{field}` on value"), span)
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
                _ => return Err(YoloscriptError::panic(
                    format!("method `{method}` not found on this value"), span,
                )),
            };
            let key = format!("{type_name}::{method}");
            let func = env.get(&key).ok_or_else(|| {
                YoloscriptError::panic(format!("no method `{method}` on `{type_name}`"), span)
            })?;
            let (params, body) = match func {
                Value::Function { params, body: FunBody::Typed(b), .. } => (params, b),
                _ => return Err(YoloscriptError::panic(
                    format!("method `{method}` is not a typed function"), span,
                )),
            };
            // PoC inline dispatch: bind self + args, eval body, pop scope.
            // Full call machinery (stack frames, recursion, closures) comes in #4.
            env.push_scope();
            if let Some(self_param) = params.first() {
                env.define(&self_param.name, recv_val);
            }
            for (param, val) in params.iter().skip(1).zip(arg_vals.iter()) {
                env.define(&param.name, val.clone());
            }
            let result = eval_block(&body, env);
            env.pop_scope();
            result
        }

        // Variants for #4 (Call/Closure) and error propagation.
        other => Err(YoloscriptError::panic(
            format!("eval_expr: unimplemented variant"),
            other.span(),
        )),
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

// ── Assignment and binary operators ──────────────────────────────────────────

/// Evaluate a simple index expression (Ident or Int literal) from an untyped Expr.
/// The typechecker validates these, so only the most common forms appear in practice.
fn eval_untyped_index(
    expr: &crate::ast::Expr,
    env: &Environment,
    span: &Span,
) -> Result<i64, YoloscriptError> {
    use crate::ast::Expr;
    match expr {
        Expr::Literal(Literal::Int(n), _) => Ok(*n),
        Expr::Ident(name, _) => match env.get(name) {
            Some(Value::Int(n)) => Ok(n),
            Some(_) => Err(YoloscriptError::panic(format!("`{name}` is not an Int"), span)),
            None    => Err(YoloscriptError::panic(format!("undefined `{name}`"), span)),
        },
        _ => Err(YoloscriptError::panic(
            "index expression too complex for PoC; assign the index to a variable first", span,
        )),
    }
}

fn apply_assign_op(
    op: &crate::ast::AssignOp,
    cur: Value,
    rhs: Value,
    span: &Span,
) -> Result<Value, YoloscriptError> {
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
