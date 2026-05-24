---
id: rfc-0001
title: "Pointer Syntax and Semantics"
date: '2026-05-19'
status: deferred
---

## Summary

Introduce an optional pointer type `*T` to Gust, inspired by Go's pointer model: typed, non-owning, runtime-managed references with explicit address-of (`&x`) and dereference (`*p`) syntax and no pointer arithmetic.

---

## Motivation

The current language has no way to share a single value across multiple bindings or to mutate a caller's data from within a called function without returning it. Every struct value is copied at assignment. This makes certain idioms verbose or impossible:

- Sharing mutable state between two data structures (e.g. a node owned by both a list and a map)
- Building self-referential types (linked lists, trees)
- Functions that mutate their caller's variable without returning a new value

Go's pointer model addresses these use cases without ownership semantics, which aligns with Gust's design principle: *memory is managed by the runtime — no ownership semantics in the language*.

---

## Proposed Design

### Pointer type syntax

A new `TypeExpr` variant: `*T` — pointer to a value of type `T`.

```gust
let p: *Int = &x;
```

`*T` is distinct from `T`. No implicit coercion between the two.

### Address-of operator: `&x`

Produces a `*T` from an expression of type `T`. The pointed-to value is heap-allocated (or promoted from the stack — transparent to the programmer, managed by the RC runtime).

```gust
mut x: Int = 42;
let p: *mut Int = &x;
```

Only a `mut` binding may be the target of a `*mut T` pointer (see Mutability below). Taking the address of a `let` binding produces a read-only `*T`.

### Dereference operator: `*p`

Reads the value behind a pointer.

```gust
let y: Int = *p;
```

`*p` in an lvalue position (left of `=`) writes through the pointer (only valid for `*mut T`).

```gust
*p = 100;
```

### Mutability

Two address-of operators, both explicit at the call site:

| Expression | Result type | Valid when |
|---|---|---|
| `&x` | `*T` | always — `x` may be `let` or `mut` |
| `&mut x` | `*mut T` | type error if `x` is a `let` binding |

Two pointer types:

| Pointer type | Can read | Can write through |
|---|---|---|
| `*T` | yes | no |
| `*mut T` | yes | yes |

Making the mutability intent explicit at the `&` site rather than deriving it from the binding keeps the syntax unambiguous — `&x` always produces a read-only pointer regardless of how `x` was declared. A programmer who wants to share mutable access must write `&mut` and will get a type error immediately if the binding doesn't allow it:

```gust
let x = 4;
let p = &x;       // *Int  — ok
let q = &mut x;   // type error: cannot take mutable reference to immutable binding `x`

mut y = 4;
let r = &y;       // *Int  — ok, read-only view of a mutable binding
let s = &mut y;   // *mut Int — ok
```

**Downgrade (`*mut T` → `*T`):** allowed implicitly. `*mut T` is strictly more capable; discarding write access is safe. `*mut T` is a subtype of `*T` and coerces wherever `*T` is expected.

**Upgrade (`*T` → `*mut T`):** never allowed. There is no syntax that produces a `*mut T` from a `let` binding.

### No auto-deref

Go silently dereferences struct pointers for field access (`p.field` where `p: *Struct`). Gust's design principle of *no implicit conversions* rules this out. Field access through a pointer requires explicit deref:

```gust
let s = Point { x: 1, y: 2 };
let p: *Point = &s;
let x = (*p).x;    // explicit deref required
```

### No pointer arithmetic

Consistent with Go and the runtime-managed memory model. `*Int + 1` is a type error.

### Null safety

Null pointers are not a primitive concept. If a pointer may be absent, wrap it in `Perhaps<*T>`. There is no implicit null; the type system enforces explicit handling via `match`.

```gust
let maybe_p: Perhaps<*Int> = Perhaps::Some { value: &x };
```

### Self-referential structs

Pointers enable recursive struct types:

```gust
struct Node {
    value: Int,
    next: Perhaps<*Node>,
}
```

Without pointers, a field of type `Node` would require infinite size. `*Node` breaks the recursion.

---

## Implications for Existing Syntax

### Grammar (`grammar.pest`)

| Change | Notes |
|---|---|
| New type production: `"*" type_expr` | Unambiguous — `*` does not appear in type position today |
| New type production: `"*" "mut" type_expr` | Only in type annotations, not in expressions |
| New unary operator: `"&" expr` | `&` is unused in expression position (`&&` remains logical AND) |
| New unary operator: `"&" "mut" expr` | `&mut` as a two-token prefix; `mut` is already a keyword so no new keyword needed |
| Prefix `"*" expr` for deref | Ambiguous with infix `*` (multiply); resolved by parse position (prefix vs infix). C, Go, and Rust all solve this the same way |

The grammar change is **additive** — no existing syntax is modified or removed.

### AST (`src/ast/mod.rs`)

| Change | Impact |
|---|---|
| `TypeExpr::Pointer(Box<TypeExpr>)` | New variant; all `match` on `TypeExpr` need a new arm |
| `TypeExpr::MutPointer(Box<TypeExpr>)` | Or fold mutability into `Pointer` with a bool flag |
| `UnaryOp::Deref` | New variant |
| `UnaryOp::AddressOf` | New variant — `&x`, always produces read-only pointer |
| `UnaryOp::AddressOfMut` | New variant — `&mut x`, type error if operand is a `let` binding |

All existing `match` statements on `TypeExpr` and `UnaryOp` will produce exhaustiveness warnings at compile time, making the blast radius of the change explicit and compiler-guided.

### Type inference (`src/typeinference/mod.rs`)

| Change | Notes |
|---|---|
| `InferType::Pointer(Box<InferType>, /*mutable*/ bool)` | New variant; `unify` needs pointer cases |
| `type_expr_to_infer` | Handle `TypeExpr::Pointer` → `InferType::Pointer` |
| `infer_expr` for `UnaryOp::AddressOf` | `&x`: return `Pointer(T, false)` — always read-only regardless of binding |
| `infer_expr` for `UnaryOp::AddressOfMut` | `&mut x`: check binding is `mut` (E0006 if not), return `Pointer(T, true)` |
| `infer_expr` for `UnaryOp::Deref` | `*p`: if `p: Pointer(T, _)`, return `T` |
| `infer_expr` for deref-assign | `*p = v`: require `p: Pointer(T, true)`, constrain `v: T` |
| `InferContext.lookup_for_write` | `*p` as an assign target needs a new path |

### Resolved type system (`src/types/mod.rs`)

`Type::Pointer(Box<Type>, bool)` — new variant. All consumers of `Type` (evaluator, typed AST construction) need handling.

### Typechecker (`src/typechecker/mod.rs`)

`type_expr_to_infer`, `infer_type_to_type`, and the Pass 2 construction (`construct_expr`) each need new arms for pointers. The `Assign` handling in Pass 1 needs to recognise deref-assign.

### Typed AST (`src/typed_ast/mod.rs`)

`TypedExpr` already stores `UnaryOp` + operand + type. No new variant needed — `TypedExpr::UnaryOp(UnaryOp::Deref, ...)` and `TypedExpr::UnaryOp(UnaryOp::AddressOf, ...)` work as-is, as long as `UnaryOp` gets the new variants.

---

## Implications for the Evaluator

### Value representation

A new variant is needed:

```rust
pub enum Value {
    // ... existing variants ...
    Pointer(Rc<RefCell<Value>>),   // *T — immutable through pointer
    MutPointer(Rc<RefCell<Value>>), // *mut T — mutable through pointer
}
```

The evaluator already uses `Rc<RefCell<Value>>` internally for all mutable bindings (`Environment::get_rc` returns one). Implementing both operators:

```rust
UnaryOp::AddressOf => {
    let name = /* extract ident from operand */;
    let rc = env.get_rc(name).ok_or(...)?;
    Ok(Signal::Value(Value::Pointer(rc)))          // read-only
}
UnaryOp::AddressOfMut => {
    // Typechecker has already verified the binding is `mut`; no runtime check needed.
    let name = /* extract ident from operand */;
    let rc = env.get_rc(name).ok_or(...)?;
    Ok(Signal::Value(Value::MutPointer(rc)))        // writable
}
```

And `*p` (read):

```rust
UnaryOp::Deref => {
    let ptr = eval_expr(operand, env)?;
    match ptr {
        Value::Pointer(rc) | Value::MutPointer(rc) => Ok(Signal::Value(rc.borrow().clone())),
        _ => Err(runtime_error("deref of non-pointer")),
    }
}
```

The internal mechanics are already present; this is mostly surfacing them as explicit language features.

### Self-referential struct evaluation

`Value::Struct { fields: HashMap<String, Value> }` can already hold a `Value::Pointer` in a field. No structural change needed.

### Impact on PoC evaluator

Because `Rc<RefCell<Value>>` is already the evaluator's internal representation of shared mutable state, adding `Value::Pointer` is low-risk. The PoC does not need to implement it — it can treat pointer-typed AST nodes as unimplemented and panic — but the `Value` enum should include the variant from the start to avoid a `Value` rewrite later.

---

## Open Questions

1. ~~**`*mut T` syntax or inferred mutability?**~~ **Resolved.**  
   Adopted: `&x` and `&mut x` as distinct operators, explicit at the call site. `&x` always produces `*T`; `&mut x` always produces `*mut T` and is a type error on a `let` binding. Mutability is not inferred from the binding — it is declared by the programmer at the point of reference creation.  
   `*mut T` is a subtype of `*T` (downgrade implicit; upgrade never allowed).

2. **Auto-deref for field access?**  
   Proposed: no — explicit `(*p).field` required.  
   Counter-argument: ergonomics. If structs are commonly passed by pointer, requiring `(*p).field` everywhere is verbose.  
   Middle ground: a deref-and-access operator `p->field` (C-style) or implicit deref only at field/method boundaries (Go-style).

3. **Address-of non-variable expressions?**  
   Can you write `&(x + 1)`? In Go, no — only addressable values (variables, struct fields, array elements). Gust should likely adopt the same rule: only named bindings and indexed locations are addressable.

4. **Pointer equality**  
   Should `p == q` compare addresses (identity) or values (`*p == *q`)? Go compares addresses. A separate `ptr_eq` function vs operator overload vs no equality at all are the options.

5. **Interaction with `Perhaps<T>`**  
   `Perhaps<*T>` vs a first-class nullable pointer type. The proposed design uses `Perhaps<*T>`, which is consistent but verbose for common patterns like optional node links.

---

## Timing Recommendation

### Option A — Resolve before evaluator implementation

Implement grammar + AST + typechecker changes for pointers before Sprint 3 begins. The PoC evaluator then includes `Value::Pointer` from the start.

**Pros:**
- Grammar and AST — the most stable parts of the pipeline — get the right shape from the outset
- The production evaluator (post-PoC rewrite) is designed around the full feature set
- No grammar/AST retrofit needed later

**Cons:**
- Pushes PoC completion back significantly (grammar + AST + typechecker changes are not trivial — see blast radius above)
- The open questions (especially auto-deref and `*mut` vs inferred mutability) require design resolution before any code is written

### Option B — Defer to after PoC evaluator

Complete the PoC evaluator. Resolve the RFC after the PoC ships. Grammar/AST/typechecker changes happen in a subsequent sprint.

**Pros:**
- PoC ships on the current timeline
- Evaluator implementation experience informs pointer design (e.g. whether auto-deref is actually needed in practice)
- The PoC evaluator rewrite is already planned — pointer support can be a first-class goal of the rewrite

**Cons:**
- Grammar and AST will need a retrofit, touching many match statements
- If the RFC is accepted with a design that contradicts a previous implementation choice, some typechecker work may need revisiting

### Recommendation

**Option B** — defer to after PoC. The primary reason: the `Value`/`Environment` design in the evaluator is already pointer-compatible (it uses `Rc<RefCell<Value>>` internally), so there is no meaningful evaluator design debt to incur. The PoC will be rewritten regardless. The grammar/AST retrofit risk is real but bounded — the compiler will surface all exhaustiveness gaps. Resolving the open questions (especially auto-deref and mutability syntax) with real implementation experience is worth the deferred cost.

**Minimum action before closing the RFC:** add `Value::Pointer(Rc<RefCell<Value>>)` to the PoC evaluator's `Value` enum as a placeholder, so the enum shape is correct from the start even if the variant is never constructed.

---

## References

- Language Spec: [`spec/types.md`](../../public/spec/types.md) (type system), [`spec/declarations.md`](../../public/spec/declarations.md) (variables, structs, enums)
- ADR-0001: `gust-interpreter/docs/decisions/adr-0001-typeregistry-structure-and-location.md` (TypeRegistry — will need Pointer handling for v0.3)
- Typechecker impl-notes: `gust-interpreter/docs/typechecker.md`
- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md` — `&x` on linear values must be restricted; `&T` read reference conflicts with address-of syntax
- RFC-0025: `docs/internal/rfcs/rfc-0025-region-allocation.md` — pointer-into-region lifetime problem; scope/callback solution avoids need for lifetime annotations
- RFC-0026: `docs/internal/rfcs/rfc-0026-unsafe-blocks.md` — pointer arithmetic and `*T` to linear values unlocked inside `unsafe`; `unsafe fun` form needed for FFI pointer signatures
- Cluster report: `docs/internal/rfc-cluster-memory-model.md`
- Related: #5 (Type Variables and Constraint System — generics RFC; pointer type params interact)

---

## Decision

**Outcome:** Deferred  
**Target:** v0.3

Pointer syntax is out of scope for v0.2 (generics + traits). The evaluator already uses `Rc<RefCell<Value>>` internally, so there is no structural debt to incur by deferring. Before this RFC is closed, a placeholder `Value::Pointer(Rc<RefCell<Value>>)` variant will be added to the evaluator's `Value` enum so the shape is correct from the start — tracked in a separate issue. The open questions (auto-deref, mutability syntax, pointer equality) will be resolved with concrete implementation experience from the v0.2 evaluator before this RFC is re-evaluated for v0.3.
