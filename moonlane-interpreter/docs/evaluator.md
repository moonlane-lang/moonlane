# Evaluator Implementation Notes

> Status: PoC complete (v0.1).  
> This evaluator is intentionally the simplest correct implementation. It will be rewritten before production use. Do not over-engineer it; open new issues for correctness gaps instead of adding complexity here.

---

## Pipeline Position

```
TypedProgram      ──►  evaluate()       ──►  side effects / RuntimePanic  (legacy)
TypedModuleGraph  ──►  evaluate_graph() ──►  side effects / RuntimePanic  (v0.6.0)
```

Entry points:
- `evaluator::evaluate(program: TypedProgram) -> Result<(), MoonlaneError>` — single-module legacy path
- `evaluator::evaluate_graph(graph: TypedModuleGraph) -> Result<(), MoonlaneError>` — multi-module path (v0.6.0): flattens the `TypedModuleGraph` into a single `TypedProgram` and delegates to `evaluate`

The evaluator operates on the typed AST produced by the typechecker. It does not re-check types — if the evaluator panics on a type mismatch, that is a typechecker bug, not an evaluator limitation.

Source: `src/evaluator/` — split into `mod.rs` (core), `builtins.rs`, `call.rs`, `display.rs`, `lvalue.rs`, `pattern.rs`

---

## Runtime Values

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Unit,
    Tuple(Vec<Value>),
    Array(Rc<RefCell<Vec<Value>>>),
    Struct { name: String, fields: HashMap<String, Value> },
    Enum   { name: String, variant: String, fields: HashMap<String, Value> },
    Closure(Rc<ClosureValue>),
    Builtin(String, fn(Vec<Value>, &Span) -> Result<Value, MoonlaneError>),
    Perhaps(Option<Box<Value>>),
    Result(Result<Box<Value>, Box<Value>>),
    Pointer(Rc<RefCell<Value>>),        // RFC-0001 placeholder — never constructed
    MutPointer(Rc<RefCell<Value>>),     // RFC-0001 placeholder — never constructed
}
```

### Array representation

`Value::Array` uses `Rc<RefCell<Vec<Value>>>` internally, but the evaluator enforces **value semantics** at every binding site. When `env.define()` or `env.set()` stores an array, it calls `deep_clone_value()` to produce a fully independent copy. This means:

- Assigning an array variable to another name gives an independent copy — mutations to one do not affect the other.
- Passing an array to a function gives the function its own copy; `array_push` inside the function does not mutate the caller's array.
- `array_push` applied to the binding itself mutates through the `Rc<RefCell>` as expected.

**`Value::Perhaps` and `Value::Result`** are the canonical runtime representations for the built-in `Perhaps<T>` and `Result<T,E>` types. All construction paths route through these variants — `Perhaps::Some { value: v }` struct literals produce `Value::Perhaps(Some(Box::new(v)))`, `None` produces `Value::Perhaps(None)`, `Result::Ok { value: v }` produces `Value::Result(Ok(Box::new(v)))`, and `Result::Err { error: e }` produces `Value::Result(Err(Box::new(e)))`. Pattern matching and the `?` operator match against these dedicated variants, not `Value::Enum`.

### Range representation

`a..b` evaluates to `Value::Struct { name: "Range", fields: { start: Int, end: Int } }`. This is an ad-hoc struct, not a typed `Range` struct — it exists so `for-in` can inspect the fields without a dedicated type. Same pattern for `a..=b` → `"RangeInclusive"`.

---

## Signal-Based Control Flow

All evaluation functions return `Result<Signal, MoonlaneError>`:

```rust
pub enum Signal {
    Value(Value),
    Return(Value),
    Break(Value),        // carries the break-expression value
    Continue,
    PropagateErr(Value), // the ? operator
}
```

`Signal::Value` is the normal case. The others implement non-local control flow by propagating up the call stack until handled:

| Signal | Consumed by |
|---|---|
| `Return(v)` | `call_function` — converts to `Signal::Value(v)` at the function boundary |
| `Break(v)` | `Expr::Loop` handler — exits the loop, returns `Signal::Value(v)` |
| `Continue` | `While`, `For`, `ForIn` loop bodies — skips to next iteration |
| `PropagateErr(e)` | `Expr::PropagateError` handler — or `call_function`, which wraps `e` in `Result::Err { error: e }` |

`Signal::into_value()` is a convenience that panics on non-Value signals. It is used at call sites where the typechecker guarantees the expression cannot diverge (e.g., function arguments, struct field expressions). If it panics, that indicates a typechecker bug.

---

## Environment

```rust
pub struct Environment {
    scopes: Vec<HashMap<String, Rc<RefCell<Value>>>>,
}
```

Each binding is stored as an `Rc<RefCell<Value>>`. This has two consequences:

1. **Mutation is visible through the scope chain.** `env.set(name, val)` finds the binding's `Rc` in any enclosing scope and mutates through it. This correctly implements `mut` re-assignment without requiring the caller to traverse scopes differently for reads vs writes.

2. **Closures share mutable state with their definition scope.** `env.clone()` clones the `HashMap`s, but each `Rc<RefCell<Value>>` clone is a shared pointer — not a deep copy. A closure that captures a binding and the enclosing scope that owns that binding share the same `RefCell`. This gives reference semantics for captured mutable variables.

   This is an unintentional consequence of the PoC design. RFC-0006 (closure capture semantics) will establish the intended semantics. For now, any program that relies on closures sharing mutable state with their enclosing scope may produce surprising results, and any program that expects clone-at-definition isolation may also be surprised. The test suite avoids this ambiguity.

---

## Evaluation Entry Point

`evaluate()` runs three passes over the top-level declarations:

**Pass 1a — Define placeholders:**
Every top-level `Fun` and `Impl` method is bound to `Value::Unit` in the root environment. This ensures the names exist before any closure is created, so closures formed in Pass 1b can capture them via shared `Rc`s.

Impl methods are registered under a structured key produced by `ImplMethodKey::from_block(...).to_env_key()`:
- Ordinary impl methods: `"TypeName::method_name"`
- `impl From<S> for T` methods: `"T::From<S>::from"` (disambiguates multiple `From` impls on the same target)

**Pass 1b — Create closures:**
Every top-level `Fun` and `Impl` method clones the full current environment and creates a `Value::Closure`. The clone captures the `Rc`s from Pass 1a, not copies of `Value::Unit`. `env.set()` then mutates those `Rc`s in place.

Because all closures from Pass 1b share the same set of `Rc`s, after Pass 1b completes every closure's captured environment already contains references to every other closure — including those defined after it. This "ties the knot" for mutual recursion without a fixpoint pass or separate reference-resolution step.

**Pass 2 — Evaluate bindings:**
Top-level `let`/`mut` bindings and statements are evaluated in order. `Fun` and `Impl` declarations are skipped (already handled in 1a/1b).

**Call `main()`:**
`main`'s body is executed directly in the root environment so that top-level `let`/`mut` bindings from Pass 2 are visible. `Signal::Return` from `main` is treated as a normal exit.

### Self-recursion inside blocks

`eval_decl` for `Fun` uses the same define-placeholder / clone / set pattern as the top-level pass so that functions defined inside a block can call themselves recursively.

---

## Closure Capture

At closure definition (`TypedExpr::Closure`), the evaluator clones the entire current environment:

```rust
let captured = env.clone();
```

As noted in the Environment section, this clone shares `Rc`s rather than deep-copying values. The captured environment is stored in `ClosureValue.captured`.

At call time (`call_function`), `captured` is cloned again and a new scope is pushed for the parameters:

```rust
let mut call_env = closure.captured.clone();
call_env.push_scope();
```

This means:
- Each call to the same closure gets a fresh parameter scope.
- The captured variable `Rc`s are shared across all calls — mutations to captured variables persist between calls to the same closure.

**This is not the intended permanent semantics.** See RFC-0006.

---

## Pattern Matching

`match_pattern(pattern, value, out)` returns `bool` and writes bindings into `out: &mut HashMap<String, Value>`. It does not mutate the environment directly — the caller pushes a scope and inserts the bindings after a successful match.

Guarded arms: the guard is evaluated in a temporary scope containing the pattern bindings. If the guard returns `false`, the scope is popped and the next arm is tried. Pattern bindings accumulated so far are discarded (the `out` map is not reused between arms).

The evaluator will panic with `"match: no arm matched scrutinee"` if no arm matches at runtime. The typechecker's exhaustiveness check (E0008) is the static guarantee that this panic is unreachable for well-typed programs.

---

## Call Stack Trace

Every user-defined function call pushes a `FrameInfo { fn_name, call_site }` onto a thread-local `CALL_STACK` before evaluating the body. On any runtime error, `attach_stack()` captures a snapshot of the stack and attaches it to the `MoonlaneError`. The stack is displayed innermost-first in the error message:

```
[R0001] runtime error: division by zero
  at file.mln:10:5
  in bar at file.mln:7:9    ← innermost (called from line 7)
  in foo at file.mln:4:5    ← outermost
```

Anonymous closures appear as `<closure>`. The call stack is cleared at the start of each `evaluate()` call. `main()` itself is not pushed (it is executed directly, not via `call_function`).

---

## Function Call Dispatch

`call_function(func, args, span)` handles three cases:

- `Value::Builtin(_, f)` — calls the function pointer directly.
- `Value::Closure(rc)` — clones the captured environment, pushes a parameter scope, evaluates the body, and converts `Signal::Return` to `Signal::Value` at the boundary. `Signal::PropagateErr` is also converted: it wraps the error value in `Value::Result(Err(Box::new(e)))` and returns `Signal::Value` — so the `?` error appears as a `Result::Err` value to the caller.
- `Value::Closure(rc)` where `rc.body` is `ClosureBody::Untyped(block)` — a polymorphic generic function or let-bound closure. The evaluator re-runs the construction pass on the untyped block at the concrete argument types, producing a `TypedBlock` that is evaluated immediately. This is the monomorphization path.

---

## Known Limitations

### Generic function dispatch — re-constructs on each call

Generic functions and let-polymorphic closures re-run the construction pass at every call site. This is correct but not optimal: for hot generic functions, monomorphization at a higher level (pre-compiling all instantiation sites) would be faster. Acceptable for the tree-walk interpreter.

### `call_function_mut_self` — non-standard calling convention for iterators

`call_function_mut_self` returns `(Signal, updated_self)` instead of just `Signal`. This exists because the language currently has no mutable references — when `next(&mut self)` is called on a user-defined iterator, mutations to `self` are local to the call frame and invisible to the caller. Returning `updated_self` threads the mutated iterator forward to the next loop iteration.

This function goes away when RFC-0001 (memory model with mutable references) is implemented. At that point, `next` can take `&mut self` and mutate in place, and `eval_for_in` can call `call_function` directly.

### Flat module environment — std::core builtins can be shadowed by user names (#189)

`evaluate_graph` merges all module environments into one. `std::core` builtin names (e.g. `print`, `assert`) are seeded into this flat environment at lowest priority. A user-defined function with the same name in any module will shadow the builtin silently at runtime, even if the typechecker resolved them to different scopes.

This will be resolved when per-module runtime environments are introduced (tracked in #189).

### Declaration name collisions across modules produce undefined runtime behaviour

In v0.6.0, two modules may each declare a top-level name (e.g. `fun tokenize()`) without importing each other. The typechecker approves both in isolation, but `evaluate_graph` flattens all modules into a single environment: the second declaration silently overwrites the first.

`evaluate_graph` emits a best-effort warning when this happens, but does not hard-error — the typechecker approved the program. The correct fix is per-module runtime environments, tracked as a future issue.

### Closure/scope mutation semantics unspecified

The PoC's `Rc<RefCell<Value>>` environment gives closures reference semantics for captured variables, which is not the intended permanent behaviour (see RFC-0006). Do not write tests that rely on cross-closure mutation sharing unless they explicitly document the dependency.

---

## Extension Points

### v0.4 — Aspects / `?` coercion (shipped)

`PropagateError` now supports `From<E>` coercion: after matching `Result::Err`, the evaluator looks up the `"Target::From<Source>::from"` key in the impl environment and calls it if the error types differ. Identity coercion (same types) skips the lookup.

### Rewrite

The evaluator is designed to be thrown away. The correct rewrite path is:
1. Decide the permanent value representation (likely a tagged pointer or NaN-boxing scheme).
2. Implement RFC-0006 capture semantics (explicit pointer types for aliasing).
3. Wire the module system name resolver (RFC-0030, implemented in v0.5.0) into the evaluator for per-module scope isolation before the evaluator is shared as a library.
