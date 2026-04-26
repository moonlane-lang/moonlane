# Task 0004: Implement Basic Type System (Int, Float, Bool, String, Array, Unit, Tuple)

**Status:** open  
**Epic:** epic-001-typechecker  
**Component:** typechecker, interpreter  
**Spec Link:** spec/Language Spec.md#Basic-Types  
**Blocked By:** 0001

## What

Define and implement the basic type system (first iteration, no generics):

- **Int** — signed or unsigned integers (64-bit)
- **Float** — floating-point numbers (64-bit)
- **Bool** — true/false values
- **String** — UTF-8 text
- **Array** — homogeneous collections (single element type, not generic)
- **Unit** — the `()` type
- **Tuple** — fixed-size heterogeneous tuples `(Int, String, Bool)`

Include type coercion rules and operation compatibility (e.g., can you add an int to a float? to a bool?).

## Design Decisions

1. **Coercion rules:** String concatenation coerces numbers to strings, arithmetic on mixed numeric types (int/float) promotes to float
2. **Array:** Non-parametric in this iteration; infer element type from array literal `[1, 2, 3]` → `Array<Int>`
3. **Empty arrays:** Type must be annotated `x: Array<Int> = []` or inferred from context
4. **Tuple:** Fixed-size, heterogeneous; `(42, "hi", true)` has type `(Int, String, Bool)`

## Acceptance Criteria

- [ ] Type system defines Int, Float, Bool, String, Array, Unit, Tuple types
- [ ] Coercion rules documented in spec
- [ ] Binary operations validate operand types
- [ ] Array operations type-check (indexing, length, etc.)
- [ ] Tuple operations type-check (field access by index)
- [ ] Type inference handles annotations like `x: int = 42`
- [ ] Evaluator enforces type constraints
- [ ] Tests cover basic operations for each type
- [ ] No regressions

## Notes

- Array is non-parametric for now; generics come in Epic 002
- Infer array element type from literals: `[1, 2, 3]` → `Array(Int)`
- Tuples are fixed-size and can mix types
- Unit type `()` is returned by statements with no value
- This task can work in parallel with 0001-0003
