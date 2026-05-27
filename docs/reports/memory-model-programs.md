# Memory Model Programs — Design Exploration

**Date:** 2026-05-26  
**Status:** Exploratory — not normative  
**Related RFCs:** RFC-0028, RFC-0025, RFC-0003  

---

## Purpose

This report explores what Moonlane programs look like once RFC-0028 (linear types, `@T` read references, `*T`/`*mut T`/`unique *T` pointers), RFC-0025 (region allocation), and RFC-0003 (fibers and channels) are implemented. It is design material, not a spec change. The goal is to surface idioms, validate that the RFC decisions compose well, and identify gaps.

**Notation used throughout:**
- `!T` — use-site linearity sigil (OQ-1, syntax unresolved; candidates: `!T`, `|T`, `linear T`)
- `Box::alloc(v)` / `Box::take(p)` — unique pointer allocation (OQ-2, syntax unresolved)
- `@(*p)` — read reference through a unique pointer (OQ-3, syntax unresolved)

Where open questions affect an example, this is noted inline.

---

## Part 1 — The Three Core Features in Combination

### 1.1 Region allocation

The natural design for `Region::scope` is **implicit allocation**: all heap allocations inside the scope callback go to the region's bump allocator automatically, with no explicit `r.create()` call. The scope itself is unsized and grows as needed.

```moonlane
let ast: Ast = Region::scope(fun() {
    let tokens = tokenise(source);   // allocated in the region
    let ast    = parse(tokens);      // allocated in the region
    ast                              // Ast is a Send value type — copied out on return
});
let ir = lower(ast);   // semantic step on owned data, outside the region
```

This is cleaner than RFC-0025's current `r.create(value)` proposal. The `r` parameter becomes unnecessary. The `Send` return bound remains the safety mechanism: you can only return a type that contains no region-internal pointers, so nothing can dangle after the region is freed.

**`lower()` is only necessary when the return type uses raw pointers.** If `Ast` is a pure value type (nested structs and arrays, no `*T` fields), it is automatically `Send` and can be returned directly via a deep copy on scope exit. The `lower()` call exists for semantic transformation, not for escaping the region — and it belongs outside the scope when both concerns are present.

**Region implementation dependency:** RFC-0025 depends on RFC-0028 (linear types) only in the sense that `Region` is declared as a `linear struct`. Once the `linear` keyword and the LinearEnv pass exist in the language, region allocation can be implemented independently in the runtime/standard library. The internal implementation of the linearity checker does not need to know anything special about `Region`.

### 1.2 Linear types and channels

Channel send (`ch <- value`) is a natural consumption point for a linear value. After a send, the binding is dead — the linearity checker sees it as consumed. This means ownership transfer through channels falls out of two orthogonal features composing naturally, with no extra annotation.

```moonlane
let conn = Connection::new(fd);
ch <- conn;   // conn consumed — cannot be used again
```

For the loop constraint (a linear value created before a loop cannot be consumed inside it), **recursion is the idiomatic solution**. Each recursive call shadows the binding with the new handle returned by the consume-and-return method:

```moonlane
// Correct: each call consumes 'file' and shadows it with the returned handle
fun stream(file: !FileHandle, out: Chan<!Frame>) {
    let (data, file) = file.read_line();
    if (data == "") { file.close(); out.close(); return; }
    out <- Frame { data: data };
    stream(file, out);   // 'file' here is the new handle
}
```

### 1.3 `*T` and `*mut T` are not Send

The `Send` constraint on `*T` and `*mut T` means pointer-based data structures cannot cross fiber boundaries. Code that builds graphs, trees with parent pointers, or doubly-linked lists using RC pointers must stay on a single fiber. Results are communicated to other fibers via channels using `Send` types (plain values, not pointers).

```moonlane
// spawn { bfs(graph_ptr, out) }  ← TYPE ERROR: *GraphNode is not Send
bfs(graph_ptr, out);   // BFS must stay on this fiber; String results flow via channel
```

This is a hard constraint, not a limitation to work around — it is the mechanism that prevents data races on pointer-based structures.

---

## Part 2 — Program Examples

### 2.1 Linear byte-frame pipeline

Three fibers: reader produces linear `Frame` values from a connection, processor filters and counts them using `@T` read references, writer sinks them to another connection.

```moonlane
// !T = use-site linearity sigil (OQ-1, syntax TBD)

linear struct Frame { data: String, seq: Int }

fun is_keepalive(f: @Frame) -> Bool { f.data == "PING" }
fun byte_count(f: @Frame) -> Int    { string_len(f.data) }

// Recursion handles consume-and-return across "iterations".
// A loop cannot consume a linear value created before its body.
fun read_frames(conn: !Conn, out: Chan<!Frame>, seq: Int) {
    let (data, conn) = conn.recv();
    if (data == "") { conn.close(); out.close(); return; }
    out <- Frame { data: data, seq: seq };   // frame consumed by send
    read_frames(conn, out, seq + 1);
}

fun process(input: Chan<!Frame>, output: Chan<!Frame>, stats: Chan<Int>) {
    mut total = 0;
    while let Perhaps::Some { value: f } = <- input {
        if (is_keepalive(@f)) {
            drop(f);   // @f read without consuming; f must still be explicitly consumed
            continue;
        }
        total += byte_count(@f);
        output <- f;   // f consumed by send
    }
    stats <- total;
    output.close();
}

fun write_frames(input: Chan<!Frame>, dest: !Conn) {
    match <- input {
        nope => { dest.close(); }
        Perhaps::Some { value: f } => {
            let Frame { data, seq: _ } = f;    // f consumed by destructure
            let dest = dest.send(data);         // consume-and-return
            write_frames(input, dest);
        }
    }
}

fun main() {
    let frame_ch: Chan<!Frame> = Chan::buffered(32);
    let out_ch:   Chan<!Frame> = Chan::buffered(32);
    let stats_ch: Chan<Int>    = Chan::new();

    spawn { read_frames(Conn::dial("source:9000"), frame_ch, 0); };
    spawn { process(frame_ch, out_ch, stats_ch); };
    write_frames(out_ch, Conn::dial("sink:9001"));

    let total = (<- stats_ch).yolo();
    println("bytes: " + int_to_string(total));
}
```

**What this demonstrates:**
- `@T` for reading a field without consuming (validation, measurement)
- `drop(value)` to explicitly satisfy the linearity checker on an early-exit path
- Channel send as the linear value consumption point
- Destructuring as consumption (`let Frame { data, seq: _ } = f`)
- Consume-and-return method chaining (`conn.recv()` → `(data, conn)`)
- Recursion as the loop-with-linear-value pattern

### 2.2 Graph traversal with `*T` and `*mut T`

An adjacency-list graph built with mutable RC pointers. BFS on a single fiber; results collected via a channel.

```moonlane
// *T and *mut T are not Send — the graph lives on one fiber only.

struct Node { id: Int, label: String, edges: (*mut Node)[] }

fun link(a: *mut Node, b: *mut Node) {
    array_push((*a).edges, b);
    array_push((*b).edges, a);
}

fun bfs(start: *Node, n: Int, out: Chan<String>) {
    mut visited: Bool[] = [];
    for (mut i = 0; i < n; i += 1) { array_push(visited, false); }

    let q: Chan<*Node> = Chan::buffered(n);
    q <- start;

    while let Perhaps::Some { value: node } = <- q {
        let id = (*node).id;
        if (visited[id]) { continue; }
        visited[id] = true;
        out <- (*node).label;   // String is Send — safe to cross fiber boundaries

        for (let e in (*node).edges) {
            let r: *Node = e;   // *mut Node coerces to *Node (downgrade to read-only)
            q <- r;
        }
    }
    out.close();
}

fun main() {
    mut a = Node { id: 0, label: "alpha", edges: [] };
    mut b = Node { id: 1, label: "beta",  edges: [] };
    mut c = Node { id: 2, label: "gamma", edges: [] };

    let pa: *mut Node = &mut a;
    let pb: *mut Node = &mut b;
    let pc: *mut Node = &mut c;
    link(pa, pb);
    link(pb, pc);
    link(pa, pc);

    let out: Chan<String> = Chan::buffered(8);

    // spawn { bfs(pa, 3, out) }  ← TYPE ERROR: *Node is not Send
    bfs(pa, 3, out);

    while let Perhaps::Some { value: label } = <- out {
        println(label);
    }
}
```

**What this demonstrates:**
- `&mut x` to produce `*mut T`
- `(*p).field` — explicit dereference required for field access (no auto-deref, OQ-6)
- `*mut T` → `*T` implicit downgrade (safe; upgrade never allowed)
- `*T` is not `Send` — the type system prevents spawning over a pointer graph
- Channels used for result collection even on a single fiber

### 2.3 Priority dispatcher with `unique *T` and `select`

A pending-job stack as a linear linked list using `unique *T`. A dispatcher fiber uses `select` to prefer high-priority input.

```moonlane
// Box::alloc / Box::take = unique pointer allocation (OQ-2, syntax TBD)

linear struct Job  { id: Int, data: String }
linear struct Node { job: !Job, next: Perhaps<unique *Node> }

fun push(top: Perhaps<unique *Node>, job: !Job) -> unique *Node {
    Box::alloc(Node { job: job, next: top })
}

fun pop(ptr: unique *Node) -> (!Job, Perhaps<unique *Node>) {
    let Node { job, next } = Box::take(ptr);
    (job, next)
}

fun drain(top: Perhaps<unique *Node>) {
    match top {
        nope => {}
        Perhaps::Some { value: ptr } => {
            let (job, rest) = pop(ptr);
            drop(job);
            drain(rest);
        }
    }
}

fun dispatch(hi: Chan<!Job>, lo: Chan<!Job>, out: Chan<!Job>) {
    mut pending: Perhaps<unique *Node> = nope;
    mut running = true;

    while running {
        // select prefers hi-priority; falls back to lo; non-blocking poll
        let got = select {
            j <- hi => j,
            j <- lo => j,
            else    => nope,
        };
        match got {
            nope                                             => { os_yield(); }
            Perhaps::Some { value: nope }                   => { running = false; }
            Perhaps::Some { value: Perhaps::Some { value: job } } => {
                pending = Perhaps::Some { value: push(pending, job) };
            }
        }
        // Flush one job to workers if available
        match pending {
            nope => {}
            Perhaps::Some { value: ptr } => {
                let (job, rest) = pop(ptr);
                out <- job;
                pending = rest;
            }
        }
    }
    drain(pending);
    out.close();
}

fun worker(jobs: Chan<!Job>, results: Chan<String>) {
    while let Perhaps::Some { value: job } = <- jobs {
        let Job { id, data } = job;
        results <- "done:" + int_to_string(id) + " " + data;
    }
}

fun main() {
    let hi_ch:     Chan<!Job>   = Chan::buffered(8);
    let lo_ch:     Chan<!Job>   = Chan::buffered(32);
    let worker_ch: Chan<!Job>   = Chan::buffered(16);
    let result_ch: Chan<String> = Chan::buffered(64);

    spawn { dispatch(hi_ch, lo_ch, worker_ch); };
    for (mut i = 0; i < 4; i += 1) {
        spawn { worker(worker_ch, result_ch); };
    }

    hi_ch <- Job { id: 0, data: "urgent" };
    lo_ch <- Job { id: 1, data: "batch" };
    hi_ch <- Job { id: 2, data: "urgent2" };
    hi_ch.close();
    lo_ch.close();

    while let Perhaps::Some { value: r } = <- result_ch { println(r); }
}
```

**What this demonstrates:**
- `unique *T` for heap-allocated linear nodes in a recursive structure
- `Box::alloc` / `Box::take` for creating and consuming unique pointer handles
- `drain` using recursion to free a linear list (loop constraint applies here too)
- `select` with `else` for non-blocking priority poll
- Linear jobs flowing from dispatcher to workers via channels

---

## Part 3 — The Two Memory Systems

### 3.1 Overview

Moonlane has two memory management systems that operate in parallel:

| | RC heap | Region scope |
|---|---|---|
| Allocation | `*T`, `*mut T` via `&` / `&mut` | Bump allocator, no RC overhead |
| Cycles | Leak — manual breaking required | Free — entire block freed atomically |
| Lifetime tracking | Reference count | Scope boundary |
| Cross-fiber safety | `Send` constraint on pointer types | `Send` return bound on scope result |
| Pointer rules | Conservative — `*T` not `Send`, no cycles | Relaxed inside, `Send` enforced on exit |
| Intended use | Long-lived shared state | Scratch work, temporary complex structures |

The RC heap is the default. It is ergonomic for most code: values live as long as someone holds a reference, and memory is reclaimed when the last reference drops.

The region is an opt-in scratch arena. Its defining property is that the **scope boundary is the lifetime guarantee** — not reference counts. When `Region::scope` returns, the entire backing block is freed in one operation regardless of what was built inside.

### 3.2 Regions as a pointer playground

The most consequential consequence of the region's lifetime model is that **pointer cycles are safe inside a region scope**. RC cycles outside the region cause leaks. Cycles inside the region are free: the scope exits, the backing block is freed, every pointer into it becomes invalid simultaneously. There is nothing to leak.

This means that code that is painful or impossible on the RC heap — graphs with bidirectional edges, trees with parent pointers, doubly-linked lists — is straightforward inside a region scope. You write the same `*T`/`*mut T` code, but without any need for weak pointers, manual cycle-breaking, or a separate GC.

The `Send` return bound enforces the only rule that matters: nothing pointing into the region can escape the scope. Outside the scope, the region is gone.

### 3.3 Example — graph analysis with the two systems working in parallel

The RC heap holds the input data and the final result. The region handles the scratch graph structure, including bidirectional edges that would cause RC cycles outside.

```moonlane
// ── RC heap: holds input and output ──────────────────────────────────────────

struct Edge { from: Int, to: Int }

struct Summary { components: Int, largest: Int }

// ── Region: scratch graph with free pointer cycles ────────────────────────────

struct GraphNode { id: Int, visited: Bool, edges: (*mut GraphNode)[] }

fun build_graph(n: Int, edges: Edge[]) -> (*mut GraphNode)[] {
    mut nodes: (*mut GraphNode)[] = [];
    for (mut i = 0; i < n; i += 1) {
        mut node = GraphNode { id: i, visited: false, edges: [] };
        array_push(nodes, &mut node);
    }
    for (let e in edges) {
        let a: *mut GraphNode = nodes[e.from];
        let b: *mut GraphNode = nodes[e.to];
        array_push((*a).edges, b);
        array_push((*b).edges, a);   // back-edge: RC cycle outside, free inside region
    }
    nodes
}

fun dfs(node: *mut GraphNode) -> Int {
    if ((*node).visited) { return 0; }
    (*node).visited = true;
    mut count = 1;
    for (let e in (*node).edges) { count += dfs(e); }
    count
}

// ── Entry point: two systems working together ─────────────────────────────────

fun analyze(n: Int, edges: Edge[]) -> Summary {
    Region::scope(fun() {
        // Inside: pointer-rich, cycle-safe, no RC overhead
        let nodes = build_graph(n, edges);

        mut components = 0;
        mut largest    = 0;
        for (let node in nodes) {
            if (!(*node).visited) {
                let size = dfs(node);
                components += 1;
                if (size > largest) { largest = size; }
            }
        }

        Summary { components: components, largest: largest }
        // Summary contains only Int — it is Send, allowed to escape.
        // All GraphNode allocations and edge pointers freed with the region.
    })
}

fun main() {
    // Long-lived data on the RC heap
    let edges: Edge[] = [
        Edge { from: 0, to: 1 },
        Edge { from: 1, to: 2 },
        Edge { from: 2, to: 0 },   // cycle: component {0,1,2}
        Edge { from: 3, to: 4 },   // separate component {3,4}
    ];

    let s = analyze(5, edges);
    println("components: " + int_to_string(s.components));   // 2
    println("largest:    " + int_to_string(s.largest));      // 3
}
```

### 3.4 Why `Region` is not `Send`

The relaxed pointer rules inside a region scope are only sound because the scope is single-fiber. `Region` itself is linear and not `Send` — it cannot be passed across a fiber boundary.

If two fibers could allocate into the same region simultaneously, they could build cycles and pointer structures concurrently without any synchronisation — a data race at the allocator level. Restricting the region to one fiber eliminates this class of problem entirely.

Per-request arenas, per-frame scratch allocators, and parser scratch spaces all fit the single-fiber model naturally. For multi-fiber scratch work, each fiber creates its own region.

### 3.5 The `Send` return bound as the only safety rule

Because the scope is single-fiber and its lifetime is bounded by the callback, the single constraint needed for safety is: **the return type of the scope callback must be `Send`**.

Since `*T` and `*mut T` are not `Send`, any type that directly or transitively contains a pointer to region memory cannot be returned. The compiler rejects the scope if the return type would allow a dangling pointer to escape.

Pure value types — structs with only `Int`, `Float`, `Bool`, `String`, and array fields — are automatically `Send` and can be returned freely. The region deep-copies them to the RC heap on scope exit.

This is the only rule the programmer needs to reason about. There are no lifetime annotations, no borrow checker, no explicit `unsafe`. The region scope is the boundary; `Send` is the exit condition.

---

## Part 4 — Design Questions Surfaced

These questions were raised while sketching the programs above. They are not resolved here; they are recorded as input for RFC updates.

### Q1 — Linear values and mutable rebinding in loops

The RFC prohibits consuming a linear value created before a loop body. This forces recursion for any "carry a linear handle through iterations" pattern (file reading, socket streaming). Whether `mut` bindings with explicit rebinding (`file = new_file`) should be treated specially — since the linearity invariant is maintained at each iteration boundary — is unresolved.

**Impact:** all streaming/iteration patterns with linear handles require tail recursion today.

### Q2 — `@T` and `spawn { }` capture

`@T` cannot be stored, so a `spawn { }` block cannot capture a read reference. To pass a linear value to a spawned fiber you must move it through a channel. Whether `spawn { }` should support short-lived `@T` capture with a scoped lifetime (bringing the scope lifetime guarantee into the fiber model) is an open design question.

**Impact:** linear values cannot be "inspected" by a spawned fiber without first sending them through a channel.

### Q3 — Error propagation through linear values

When `?` short-circuits out of a region scope (or any block containing live linear values), all in-scope linear bindings must be consumed before the early exit. Whether `?` should trigger automatic `drop` calls for linear values, or require the programmer to restructure error paths, is the "destructor protocol" open question (RFC-0028 OQ-5).

**Impact:** error-handling code paths with linear values are verbose today unless a `Drop` aspect or `#[auto_drop]` mechanism is introduced.

### Q4 — Region-internal `*T`/`*mut T` vs. RC-heap `*T`/`*mut T`

The two-memory-systems model implies that `*T`/`*mut T` inside a region scope are semantically different from `*T`/`*mut T` on the RC heap: they are bump-allocated, carry no refcount, and may form cycles safely. Whether this difference should be visible in the type system (e.g. a separate `~T` region-pointer type) or invisible to the programmer (same syntax, different runtime behavior depending on allocation context) is an open question.

**Impact:** if they are the same type, the programmer cannot tell from a signature whether a pointer is RC-backed or region-backed. If they are different types, the two systems compose less transparently but are more explicit.

### Q5 — `Region::scope` return value and `Send` bound

RFC-0025 Option A uses `Send` as the return constraint, relying on `*T` being non-`Send` to prevent pointer escape. This works, but is conservative: some types that are safe to return from a region scope (e.g. a struct containing a `unique *T` to heap memory that was not region-allocated) would be incorrectly rejected.

**Impact:** the `Send` constraint may need to be refined as `RegionFree` or similar — "contains no pointers into the current region" — rather than the broader "contains no pointers at all."

---

## Part 5 — Limitations Relative to Lifetime-Annotated Systems

Moonlane's memory model trades lifetime annotations for a simpler contract: use `@T` for short-lived reads, use regions for scratch work, use `Send` as the safety boundary. This works well for most programs, but there are patterns where Rust-style lifetimes express something precisely that Moonlane either cannot express at all or can only approximate with a copy or a restructure.

Each subsection below shows the desired pattern, why it fails (or degrades) in Moonlane, and what the workaround costs. §5.1–5.5 cover cases where the model cannot express something lifetimes can; §5.6–5.7 cover cases where the model is overly conservative — it rejects programs that are actually safe.

### 5.1 Returning a borrowed view into input

**What lifetimes give you.** A function can return a reference that borrows from one of its arguments. The return value is valid exactly as long as the argument is live — enforced statically by the lifetime parameter.

```rust
// Rust: zero-copy; return value borrows from `input`
fn first_word(input: &str) -> &str {
    match input.find(' ') {
        None    => input,
        Some(i) => &input[..i],
    }
}
```

**Why it fails in Moonlane.** `@T` cannot be returned from a function. A `@String` parameter is a local read alias; the callee cannot hand it back to the caller. There is no syntax for expressing "this return value borrows from this argument."

```moonlane
// TYPE ERROR: @String cannot be returned — read reference cannot escape the call
fun first_word(input: @String) -> @String { ... }
```

**Workaround.** Return an owned copy.

```moonlane
fun first_word(input: @String) -> String {
    match string_find(input, " ") {
        nope                        => string_copy(input),
        Perhaps::Some { value: i }  => string_slice(input, 0, i),
    }
}
```

**Cost.** Every call allocates a new `String`. A hot path that slices strings repeatedly (lexer, parser, CSV reader) pays allocation overhead that a lifetime-aware system avoids entirely.

---

### 5.2 Structs that hold a reference

**What lifetimes give you.** A type can carry a borrow of an external value, parameterised by a lifetime that prevents the struct from outliving the referent.

```rust
// Rust: Parser holds a reference into the source buffer — zero copy, lifetime-safe
struct Parser<'a> { input: &'a str, pos: usize }

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> { self.input[self.pos..].chars().next() }
}
```

**Why it fails in Moonlane.** `@T` cannot be stored in a struct field. There is no lifetime parameter mechanism.

```moonlane
// NOT VALID: @String cannot appear as a struct field
struct Parser { input: @String, pos: Int }
```

**Workaround A — Clone into the struct.** Give `Parser` an owned `String`. Safe and simple, but copies the entire source on construction.

```moonlane
struct Parser { input: String, pos: Int }
```

**Workaround B — Raw pointer inside a region.** Use `*String` as the field inside a `Region::scope`. Zero-copy but the struct cannot escape the scope (it is not `Send`).

```moonlane
// Valid only inside Region::scope; Parser is not Send
struct Parser { input: *String, pos: Int }
```

**Cost.** Workaround A pays an allocation per parse. Workaround B is zero-copy but the parser cannot be suspended and resumed across scope boundaries — it is tied to one region's lifetime.

---

### 5.3 Iterators that yield references

**What lifetimes give you.** An iterator over a collection can yield `&T` — a reference into the collection. No allocation; the borrow checker ensures the collection is not modified while the iterator is live.

```rust
// Rust: no allocation; yields references into the existing Vec
fn print_long(v: &Vec<String>, min_len: usize) {
    for s in v.iter().filter(|s| s.len() >= min_len) {
        println!("{}", s);
    }
}
```

**Why it fails in Moonlane.** The array iteration construct (`for let x in arr`) yields owned values. There is no `@T`-yielding iterator type because `@T` cannot be stored in an iterator struct (see §5.2). Every element visited is either moved or copied.

**Workaround.** For read-only iteration, copy cost is often negligible for small `Send` values. For large values, pass the array and an index to avoid copying the whole element.

```moonlane
fun process_by_ref(items: String[], i: Int) { ... }

for (mut i = 0; i < array_len(items); i += 1) {
    process_by_ref(items, i);
}
```

**Cost.** Functional pipelines (map, filter, fold over a borrowed collection) must operate on copies or be rewritten as index loops. General-purpose zero-copy adaptor combinators (`zip`, `windows`, `chunks`) cannot be built.

---

### 5.4 Splitting a structure into concurrent borrows

**What lifetimes give you.** Rust's borrow checker can prove that two mutable references cover disjoint memory, allowing safe concurrent mutation of different parts of a struct or slice.

```rust
// Rust: split_at_mut proves non-overlap — both halves mutable, safe
let (left, right) = data.split_at_mut(mid);
left[0]  = 1;   // mutates data[0]
right[0] = 2;   // mutates data[mid]
```

**Why it fails in Moonlane.** There is no type-level proof of disjointness. Two `*mut T` pointers into different parts of the same allocation are indistinguishable by type; the system cannot certify they are non-overlapping.

```moonlane
let pa: *mut Node = nodes[0];
let pb: *mut Node = nodes[1];
// Nothing in the type system records that pa ≠ pb — aliasing is possible
```

**Workaround.** Enforce disjointness structurally: use separate allocations, or keep all mutation on a single fiber and use channel-based ownership transfer (`!T`) for cross-fiber work.

**Cost.** Algorithms that exploit in-place disjoint partitioning (parallel mergesort, parallel prefix sum, matrix tiling) must either be serialised or restructured as pipeline stages connected by channels.

---

### 5.5 Lifetime-parameterised closure captures

**What lifetimes give you.** A closure can borrow from its environment with a tracked lifetime; the return value can also borrow from the captured binding.

```rust
// Rust: closure borrows haystack; return value borrows from it too
let haystack = vec!["alpha", "beta", "gamma"];
let find = |needle: &str| haystack.iter().find(|&&s| s == needle);
// haystack is borrowed, not moved — still usable after 'find' is dropped
```

**Why it fails in Moonlane.** Closures cannot capture by borrow — `@T` cannot be stored, and there is no lifetime parameter to tie the closure's return to a captured binding. Capture is either by copy (for `Send` types) or by linear move (for `!T`).

```moonlane
// NOT VALID: cannot capture @String[] in a closure
let find = fun(needle: String) { array_find(@haystack, needle) };
```

**Workaround.** Move the array into the closure (exclusive ownership) or pass it as an explicit argument on each call.

```moonlane
// Owned capture: haystack moved into 'find'
let find = fun(needle: String) -> Perhaps<String> {
    array_find(haystack, needle)   // haystack is no longer accessible outside
};
```

**Cost.** The closure gains exclusive ownership of the array; callers outside it lose access. Shared read-only caches or lookup tables must be passed as explicit arguments rather than captured once.

---

### 5.6 Overly conservative: `unique *T` and region-internal pointers are the same type

**What the model does.** `*T` and `*mut T` are the same syntactic type whether the pointer came from a region bump-allocator or from the RC heap. The `Send` constraint treats them identically: neither can cross a fiber boundary or escape a region scope.

**Where this is too conservative.** A `unique *T` whose pointee was allocated on the RC heap (not in a region) is safe to return from a `Region::scope` — the memory it points to will not be freed when the region exits. But the `Send` constraint on `*T` rejects it anyway because the type system has no way to distinguish "region-internal pointer" from "heap pointer".

```moonlane
fun make_node() -> unique *Node {
    Box::alloc(Node { id: 0, label: "root", edges: [] })
    // This pointer is RC-heap backed — not region-internal
}

let result: unique *Node = Region::scope(fun() {
    let scratch = build_graph(...);   // region-internal
    make_node()                       // heap-backed — safe to return, but...
    // TYPE ERROR: unique *Node is not Send — rejected even though it won't dangle
});
```

**What lifetimes would give you.** A lifetime parameter on the pointer type — `*'heap T` vs `*'region T` — would let the compiler distinguish the two cases and allow the heap-backed pointer to escape the scope freely. See also Q4 (§4).

---

### 5.7 Overly conservative: `@T` and `spawn { }` capture

**What the model does.** `spawn { }` cannot capture `@T`. To pass a linear value to a spawned fiber you must move it through a channel.

**Where this is too conservative.** A scoped spawn — one where the spawning fiber provably waits for the child to finish before the referenced value is freed — is safe to give a `@T` reference. The child fiber cannot outlive the referent because the parent blocks until join. Rust's `std::thread::scope` exploits exactly this pattern.

```moonlane
// Desired: parent blocks at join; child read is safe for the duration
let summary = spawn_scoped(fun() { summarise(@large_value) });
let result = join(summary);
// @large_value is still live here — no dangling

// Current reality: must send large_value through a channel (copy or move)
let ch: Chan<Summary> = Chan::new();
spawn { ch <- summarise(large_value); };   // large_value moved — no longer accessible
let result = (<- ch).yolo();
```

**What lifetimes would give you.** A scoped-thread lifetime (like `'scope` in Rust's `thread::scope`) would bound the spawned closure's borrows to the scope lifetime, making `@T` capture safe without moving the value.

**Cost.** Every read-only inspection of a large value by a child fiber today requires either a copy into the channel or a restructure to move ownership and return it via the result channel.

---

### 5.8 Summary

| Pattern | Lifetime-annotated system | Moonlane today | Kind of limitation |
|---|---|---|---|
| Return borrow from input | Zero-copy `&'a T` return | Must return owned copy | Expressiveness gap |
| Struct holding a borrow | `struct Foo<'a> { r: &'a T }` | Own data, or `*T` inside region | Expressiveness gap |
| Reference-yielding iterator | `Iterator<Item = &'a T>` | Iterator yields owned copies | Expressiveness gap |
| Disjoint mutable borrows | `split_at_mut` / field projections | Separate allocations or channel handoff | Expressiveness gap |
| Closure capturing by borrow | Lifetime-tied closure capture | Move into closure or pass each call | Expressiveness gap |
| Heap-backed `unique *T` escaping region scope | `*'heap T` vs `*'region T` distinct types | Rejected by `Send` constraint | Overly conservative |
| `@T` capture in scoped spawn | Scoped-thread lifetime | Must send through channel | Overly conservative |

The expressiveness gaps share a common root: `@T` is an ephemeral local alias, not a tracked borrow. The over-conservatism cases share a different root: the type system has no vocabulary for "pointer whose referent outlives this scope" — it can only ask "is this type `Send`?" which conflates lifetime-safe pointers with lifetime-unsafe ones.

---

## Part 6 — Possible Extensions to Address the Limitations

This part proposes concrete changes or additions to the model, one per limitation cluster. They are ordered from smallest blast radius (targeted changes to existing rules) to largest (systemic extensions to the type system). None of these are spec decisions; they are design options for future RFCs.

---

### 6.1 Replace `Send` with `RegionFree` as the region scope exit bound

**Addresses:** §5.6 (heap-backed `unique *T` rejected at scope exit)

**The change.** Introduce a `RegionFree` marker aspect meaning "contains no pointers into the current region." Use it as the return constraint on `Region::scope` instead of `Send`.

`Send` is "contains no raw pointers at all." `RegionFree` is strictly weaker: a heap-backed `*T` or `unique *T` satisfies it; a region-internal `*T` does not. The distinction requires the type system to track pointer provenance — specifically, whether a pointer was bump-allocated by the current region or allocated on the RC heap.

**How it would work.** The simplest implementation is to introduce a distinct region pointer type — call it `~T` — for pointers produced by a region's allocator. The `RegionFree` bound is then "contains no `~T`." The programmer-visible `*T` syntax continues to mean RC-heap pointer and is always `RegionFree`. Inside a `Region::scope`, the compiler implicitly produces `~T` instead of `*T` for allocations; outside it, `~T` is not constructible.

```moonlane
// After this change: heap-backed unique *T can escape the scope

fun make_node() -> unique *Node {
    Box::alloc(Node { id: 0, label: "root", edges: [] })   // RC-heap — RegionFree
}

let result: unique *Node = Region::scope(fun() {
    let scratch: ~GraphNode[] = build_graph(...);   // ~T — region-internal, not RegionFree
    make_node()                                     // *Node — RegionFree, allowed to escape
});
// scratch is freed with the region; result is not
```

**Trade-off.** This adds one concept (`~T` or an implicit provenance tag) and changes one rule. It does not touch function signatures, struct fields, or the `@T` semantics. It is the most self-contained extension in this list.

The main cost: if `~T` is user-visible syntax, the programmer must understand two pointer types for region-internal code. If it is always implicit (the compiler rewrites allocations inside a scope silently), the programmer never writes `~T` — but the type error messages when a `~T` escapes must be readable.

---

### 6.2 Scoped fiber spawn

**Addresses:** §5.7 (`@T` blocked from spawn captures)

**The change.** Add `fiber::scope` (or `spawn_scoped`) as a standard-library primitive. A scoped spawn blocks the spawning fiber until all child fibers in the scope have finished. Because the children cannot outlive the scope, they are allowed to capture `@T` read references from the enclosing binding — the referent is provably live for the entire child lifetime.

```moonlane
// fiber::scope blocks here until both children exit
let summary = fiber::scope(fun(s: FiberScope) {
    // @large_value is safe to capture: children cannot outlive the scope
    let a = s.spawn(fun() { count_words(@large_value) });
    let b = s.spawn(fun() { count_lines(@large_value) });
    Summary { words: join(a), lines: join(b) }
});
```

`FiberScope` is a linear value; `s.spawn` is a consume-and-return method that returns a `JoinHandle` per child. The scope itself is the lifetime boundary: `large_value` is guaranteed live until `fiber::scope` returns because the spawning fiber is blocked inside it.

**What the type rule is.** A closure passed to `s.spawn` may capture `@T` if the `@T` refers to a binding that is live in the enclosing scope of `fiber::scope`. The compiler checks this at the `s.spawn` call site, not at the function boundary — the same pattern RFC-0003 already uses for `Send` capture checking at `spawn { }` sites.

**Trade-off.** This does not require lifetime parameters or changes to `@T` semantics. It is a library API backed by a single new compiler rule: scoped-spawn closures are exempt from the "no `@T` capture" restriction. The cost is structured-vs-unstructured: `fiber::scope` cannot return until all children finish, which rules out fire-and-forget patterns. That is the whole point — the structure is what makes `@T` capture sound.

---

### 6.3 Output lifetime elision for `@T`

**Addresses:** §5.1 (returning a borrowed view from a function) — partial

**The change.** Allow a function to return `@T` when the return lifetime is unambiguously derivable from the inputs. Specifically: if a function takes exactly one `@T` parameter and returns `@T`, the return reference is inferred to borrow from that parameter. No annotation is needed in the common case; an explicit annotation resolves ambiguity.

```moonlane
// Elided: one @T input, one @T output — unambiguously borrowing from 'input'
fun first_word(input: @String) -> @String {
    match string_find(input, " ") {
        nope                       => input,
        Perhaps::Some { value: i } => string_view(input, 0, i),
    }
}

// Explicit annotation needed: two @T inputs, compiler cannot infer which one is returned
fun longest<'a>(x: @'a String, y: @'a String) -> @'a String {
    if string_len(x) >= string_len(y) { x } else { y }
}
```

**What `@'a T` means.** A lifetime-labelled read reference. The label `'a` is a scope identifier: the return value is valid as long as the argument labelled `'a` is live. The elision rule means `'a` is almost never written — only when there are two `@T` inputs and the return borrows from one of them.

**Trade-off.** This is a minimal annotation surface: no struct parameters, no lifetime bounds on types, just a label on `@T` in function signatures when disambiguation is needed. It addresses the zero-copy return pattern (§5.1) without requiring region-parameterized types (§6.4).

It does not address §5.2 (structs holding borrows) or §5.3 (reference-yielding iterators) — those require storable `@T`, which is the subject of §6.4. This extension and §6.4 are composable: §6.3 can be adopted first as a low-cost partial fix.

---

### 6.4 Region-parameterized types: storable `@T`

**Addresses:** §5.1, §5.2, §5.3, §5.5 (all expressiveness gaps stemming from `@T` being non-storable)

**The change.** Allow `@T` to be stored in structs and returned from functions when it is annotated with a *region lifetime* — a label that names the region (or scope) the referenced value belongs to. A type parameterized by a region lifetime cannot outlive that region. The compiler enforces this by checking that any value carrying a `@'r T` field is dropped before region `r` exits.

Region lifetimes are a restricted form of lifetime parameters: they correspond to concrete `Region::scope` boundaries, not abstract lifetime variables. This avoids the full generality — and complexity — of Rust's lifetime system while covering the principal use cases.

```moonlane
// 'r is a region lifetime — bound to the enclosing Region::scope
struct Parser<'r> {
    input: @'r String,
    pos:   Int,
}

fun make_parser<'r>(src: @'r String) -> Parser<'r> {
    Parser { input: src, pos: 0 }
}

fun peek<'r>(p: @Parser<'r>) -> Perhaps<Char> {
    string_char_at(p.input, p.pos)
}

let result: Token[] = Region::scope(fun() {
    let src: String = load_source();
    let p = make_parser(@src);   // Parser<'scope> — borrows from 'scope
    let tokens = tokenise(p);    // Token[] is Send — can escape
    tokens
    // p and src are freed with the scope; tokens is a copy-out value
});
```

**What the type rules are.**
- `@'r T` is a read reference tied to region lifetime `'r`. It can be stored in a struct field if the struct is also parameterized by `'r`.
- A struct that carries a `@'r T` field is not `Send` (and not `RegionFree` in the sense of §6.1): it cannot escape the scope of `'r`.
- The elision rule from §6.3 applies: `'r` can be omitted in function signatures when it is unambiguous.
- Outside a `Region::scope`, `'r` is the implicit "program lifetime" — equivalent to `'static` in Rust. A `@'static T` is a reference to a value that is never freed; only global or leaked values qualify.

**Reference-yielding iterators.** Once `@'r T` can be stored, an iterator that yields read references is expressible:

```moonlane
struct ArrayIter<'r, T> { arr: @'r T[], pos: Int }

fun next<'r, T>(iter: @mut ArrayIter<'r, T>) -> Perhaps<@'r T> {
    if iter.pos >= array_len(iter.arr) { return nope; }
    let elem = @(iter.arr[iter.pos]);
    iter.pos += 1;
    Perhaps::Some { value: elem }
}
```

**Trade-off.** This is the highest-impact but also highest-complexity change in this list. It introduces a new concept (region lifetime parameter) with its own syntax, its own variance rules, and its own error messages. The restriction to region lifetimes (rather than arbitrary lifetime variables) limits the annotation surface: you only write `'r` when a struct or function crosses a `Region::scope` boundary. Programs that never use `Region` are unaffected.

The principal risk is that region lifetimes and the existing `@T` (which has no lifetime) are now two related but distinct things, and the compiler must reason about both. A gradual adoption path: ship §6.3 first (output lifetime elision, no storable `@T`), then extend to §6.4 (storable `@'r T`) once the elision rules are proven out.

---

### 6.5 Disjoint pointer split

**Addresses:** §5.4 (disjoint mutable borrows)

**The change.** Add a `split` primitive that takes a `*mut T[]` and a midpoint and returns two handles with a type-level guarantee that they are non-overlapping. The guarantee is represented by a linear pair: consuming both handles is the only way to get back the original pointer, and neither can be cloned.

```moonlane
// split returns a linear pair — neither half is independently droppable
fun array_split_at_mut<T>(arr: unique *mut T[], mid: Int)
    -> (unique *mut T[], unique *mut T[])

// Usage: process both halves in parallel, then rejoin
let (left, right) = array_split_at_mut(data, mid);
spawn { left_result_ch <- process(left); };
let right_result = process(right);
let left_result  = (<- left_result_ch).yolo();
```

The linearity of each half means: the borrow checker (linear type checker) ensures both are consumed exactly once. The type does not encode which elements each half covers — that is a runtime property — but the uniqueness guarantees that no two live handles can alias the same storage.

**Field projection.** For structs, a similar mechanism allows projecting a `*mut Foo` into a `*mut Int` for a specific field:

```moonlane
// Compiler-generated for each field; consumes the struct pointer, returns field pointers
fun split_foo(p: unique *mut Foo) -> (unique *mut Int, unique *mut String)

// Rejoin: consumes both field pointers, returns the struct pointer
fun join_foo(a: unique *mut Int, b: unique *mut String) -> unique *mut Foo
```

**Trade-off.** This is the most targeted of the proposals — it adds two primitives (`split`, `join`) and a compiler rule for uniqueness tracking on split handles. It does not require lifetime parameters. The cost is that "split and rejoin" is a pattern the programmer must write explicitly; there is no automatic inference of disjointness the way Rust's borrow checker handles field borrows. For the common case (parallel processing of two array halves), the explicit split is not onerous; for fine-grained field borrowing it could become verbose.

---

### 6.6 Interaction between proposals

The proposals are largely independent but compose in a specific order of dependency:

| Proposal | Depends on | Prerequisite for |
|---|---|---|
| §6.1 `RegionFree` bound | Region provenance tracking (`~T`) | §6.4 (region lifetimes build on region identity) |
| §6.2 Scoped spawn | Nothing (library + one compiler rule) | — |
| §6.3 Output lifetime elision | Nothing | §6.4 (elision rules are a subset) |
| §6.4 Region-parameterized types | §6.3 (elision syntax) | — |
| §6.5 Disjoint split | `unique *T` (RFC-0028 OQ-2) | — |

A minimal first step that addresses both over-conservatism cases without touching expressiveness: ship §6.1 and §6.2. These change two rules and add one API; they do not add annotation syntax.

A second step that closes most of the expressiveness gaps: ship §6.3 (output lifetime elision, low cost) and then §6.4 (storable `@'r T`, higher cost). These together make zero-copy view patterns, borrowed-reference structs, and reference-yielding iterators possible.

§6.5 (disjoint split) is independent and can be deferred until a concrete use case (parallel array processing, parallel struct mutation) is blocking.

---

## References

- RFC-0028: Memory and Reference Model — `docs/internal/rfcs/rfc-0028-memory-and-reference-model.md`
- RFC-0025: Region Allocation — `docs/internal/rfcs/rfc-0025-region-allocation.md`
- RFC-0003: Concurrency Model — `docs/internal/rfcs/rfc-0003-concurrency-model.md`
- RFC cluster: Memory Model — `docs/internal/rfc-cluster-memory-model.md`
