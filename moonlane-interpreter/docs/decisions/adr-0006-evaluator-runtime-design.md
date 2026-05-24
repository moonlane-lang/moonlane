---
id: decision-6
title: "Evaluator Runtime Design"
date: '2026-05-17'
status: proposed
---

## Context

The evaluator is a tree-walking interpreter that consumes a `typed_ast::TypedProgram` and produces program output. Before writing any evaluator code, four interconnected design decisions must be settled — they determine the signatures of nearly every function in the evaluator.

The questions are ordered by dependency: each answer constrains the next.

1. How are runtime values represented and shared?
2. How do non-local control flow signals (`break`, `continue`, `return`) propagate?
3. How is the variable environment structured?
4. How do closures capture their enclosing scope?

---

## Question 1 — Value representation: owned clone

### Options Considered

**Option A: Owned, cloned values** — the `Value` enum owns its data directly. `Array` holds `Vec<Value>`, `String` holds `String`. Assigning or passing a value to a function copies it.

```rust
enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array(Vec<Value>),
    Tuple(Vec<Value>),
    Struct { name: String, fields: HashMap<String, Value> },
    Enum { variant: String, fields: Vec<Value> },
    Function(Closure),
    Unit,
}
```

**Option B: Reference-counted shared values** — every `Value` is wrapped in `Rc<RefCell<Value>>`. Assignment and function-call argument passing share the same heap node. Mutations inside functions or closures propagate back to the caller.

### Recommendation

**Recommended: Option A — owned, cloned values.**

Moonlane is Rust-inspired and the spec defines no reference-type syntax, no pointer model, and no borrow semantics. The intended runtime model is value semantics throughout. Introducing `Rc<RefCell<>>` by default would mean the interpreter's runtime behaviour diverges from what the eventual compiler will implement, defeating the interpreter's purpose as a spec-validation tool.

Clone overhead is acceptable for v0.1. If a specific bottleneck appears (e.g. large string copies), `Rc<str>` can be adopted for strings in isolation without changing the overall model.

**Consequence for struct and enum representation:**
- Struct values: `HashMap<String, Value>` (field access by name, consistent with the evaluator not knowing field indices)
- Enum variants: `(String, Vec<Value>)` (variant tag + ordered fields)

---

## Question 2 — Non-local control flow: ControlFlow enum

### Options Considered

**Option A: `ControlFlow` return enum** — `eval_stmt` and `eval_expr` return a dedicated enum that carries the normal value, a `return` payload, a `break` payload, or a `continue` signal. Callers pattern-match and propagate upward; loop and function boundaries consume the variants they own.

```rust
enum ControlFlow {
    Value(Value),
    Return(Value),
    Break(Option<Value>),
    Continue,
}
// eval functions: Result<ControlFlow, RuntimeError>
```

**Option B: Rust panics** — implement `break` / `continue` / `return` by unwinding with `panic!` and catching with `std::panic::catch_unwind`.

**Option C: Mutable out-parameter** — thread a `&mut Option<ControlFlow>` through every evaluator call; check after each statement.

### Recommendation

**Recommended: Option A — `ControlFlow` return enum.**

Option B abuses Rust's panic mechanism for non-exceptional control flow; it is semantically incorrect and fragile in the face of other panics. Option C threads mutable state through every function signature and makes control flow implicit rather than typed.

A `ControlFlow` enum makes the signal explicit in the type system. The propagation protocol is simple and uniform: a caller that does not consume a variant (e.g. a block that receives `Return`) forwards it up; the boundary that owns the variant (the function evaluator for `Return`, the loop evaluator for `Break`/`Continue`) unwraps and handles it. Illegal signals (e.g. `Break` outside a loop) are already rejected by the type checker, so the evaluator can treat receiving them as an internal error.

---

## Question 3 — Environment structure: Vec-of-HashMaps scope stack

### Options Considered

**Option A: Linked frames via `Rc<RefCell<HashMap>>`** — each scope frame is a heap-allocated map with an `Rc` pointer to its parent. Closures capture the current frame pointer and share it with the enclosing scope.

**Option B: `Vec<HashMap<String, Value>>` scope stack** — a `Vec` of plain `HashMap`s. The last element is the innermost scope. Variable lookup walks backward through the vec. Closures take a snapshot of the vec at creation time.

**Option C: Single flat `HashMap` with a key-stack for cleanup** — one map; a side stack tracks which keys were introduced per scope and removes them on scope exit.

### Recommendation

**Recommended: Option B — `Vec<HashMap<String, Value>>` scope stack.**

Option A (`Rc<RefCell<>>` frames) is the right choice only if closures must share mutable state with their enclosing scope — which Question 1 rules out under the value-semantics recommendation. There is no reason to pay the aliasing complexity without the aliasing benefit. Option C is error-prone when the same name is shadowed across multiple scopes.

A `Vec` of `HashMap`s is structurally simple: `push` an empty map on scope entry, `pop` on exit, walk backward for lookup (innermost-first). Snapshots for closure capture are a `clone()` of the `Vec`. No smart pointers needed.

---

## Question 4 — Closure capture: snapshot at creation time

### Options Considered

**Option A: Copy captured values at closure creation** — when a `Closure` value is constructed, the relevant portion of the environment is cloned into the closure struct. Mutations to those variables in the enclosing scope after closure creation have no effect on the closure, and vice versa.

**Option B: Capture by shared reference** — the closure holds `Rc<RefCell<Value>>` handles to specific bindings; mutations propagate bidirectionally.

### Recommendation

**Recommended: Option A — snapshot at closure creation.**

Directly follows from Questions 1 and 3. The value-semantics recommendation (Q1) rules out shared mutable state; the `Vec` environment recommendation (Q3) has no identity to share. A closure would therefore store a `Vec<HashMap<String, Value>>` snapshot taken at the moment of its creation. This matches the semantics a compiler would generate (Rust closures capture by move by default; Moonlane's value model is analogous).

If the spec ever introduces a `move` vs capture-by-reference distinction, that is a future spec change — not a v0.1 concern.

---

## Expected Consequences (if all recommendations are accepted)

**Value type:**
- `Value` derives `Clone`; no `Rc` in the hot path
- `Struct` and `Enum` variants use `HashMap<String, Value>` and `(String, Vec<Value>)` respectively

**Eval function signatures:**
- `eval_expr(expr: &TypedExpr, env: &mut Env) -> Result<ControlFlow, RuntimeError>`
- `eval_stmt(stmt: &TypedStmt, env: &mut Env) -> Result<ControlFlow, RuntimeError>`
- Callers forward unknown `ControlFlow` variants; boundary evaluators (function, loop) consume their owned variants

**Environment type (`Env`):**
- `type Env = Vec<HashMap<String, Value>>`
- Scope entry: `env.push(HashMap::new())`
- Scope exit: `env.pop()`
- Lookup: iterate from end, return first match
- Closure capture: `env.clone()` at closure construction

**Closure value:**
```rust
struct Closure {
    params: Vec<(String, Type)>,
    body: TypedBlock,
    captured_env: Env,
}
```
On call: prepend `captured_env`, push a new frame for the call's arguments, evaluate body, then restore.

**Integer overflow:** wrapping arithmetic for v0.1. Revisit when a debug/release mode distinction is added (doc-3 open item).

**Built-in functions:** matched by name in `eval_call` before user-defined function lookup. No registry for v0.1 — the set is small and fixed. Add a registry if the built-in count grows.

---

## Open prerequisite (not part of this ADR)

**`.yolo()` vs `yolo` keyword** — tracked as an open item in doc-3. Must be resolved in the spec before implementing `PropagateError` evaluation (one of the 20 `TypedExpr` variants in TASK-22). If kept as a method call, `eval_call` must special-case it before generic method dispatch. If promoted to a keyword, it is a dedicated `TypedExpr` variant with no dispatch.

---

## References

- ADR: [ADR-0004 — Interpreter Architecture](adr-0004-interpreter-architecture.md) (tree-walking, monomorphisation)
- Spec: [docs/public/spec.md](../../../docs/public/spec.md) (value semantics, closure behaviour)
- RFCs: [RFC-0015](../../../docs/internal/rfcs/rfc-0015-unwrap-syntax.md) (`.yolo()` open question), [RFC-0013](../../../docs/internal/rfcs/rfc-0013-integer-overflow.md) (integer overflow)
- v0.1 — Evaluator: Tasks 21–24 (Value Representation, Expression Evaluation, Control Flow, Function Calls; complete)
