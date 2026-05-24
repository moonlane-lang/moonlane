---
id: research
title: "Academic Research Angles"
type: guide
created_date: '2026-05-24'
---

# Academic Research Angles

This document maps the project's potential academic contributions honestly: what is genuinely novel, what is prior art, what needs to exist before each angle is publishable, and what venues are realistic. It is written for the author as a researcher, not as a language designer.

Read this alongside `docs/internal/vision.md`, which covers the product identity. This document is about intellectual contribution.

> **Note on verification:** the novelty claims in this document were checked against the academic literature using a systematic search. The prior work table and verdict sections reflect actual search results, not assumptions. Where a claim was found to be overstated, it is stated plainly. Where prior work is adjacent but not identical, the precise distinction is described.

---

## Prior Work and Honest Positioning

### Linear types — the full landscape

Linear types originate in Girard's linear logic (1987). The following table covers the most relevant work, including recent papers that are often omitted from language-design discussions.

| Work | What it does | What it does not do |
|---|---|---|
| Wadler (1990) — *Linear types can change the world* | Applies linear logic to functional programming | No opt-in; no RC; no dual-mode |
| Baker (1992) — *Lively Linear Lisp* | Linear types for GC-free Lisp | Dynamically typed |
| Adoption and Focus — Fähndrich & DeLine (PLDI 2002) | Scope-limited "focus" construct for temporary access to linear values without consuming them | Not expression-scoped; not formally proved |
| Cyclone — Grossman et al. (PLDI 2002) | Region types + linear types in a C-like language | Compiled only; mandatory; complex annotation burden |
| ATS — Xi (2012+) | Linear types for systems programming, formal soundness | Mandatory; complex annotation burden; no dual-mode |
| Rust (2010–) | Affine types via ownership + borrow checker | Mandatory; no interpreter; borrow checker required |
| Mezzo — Pottier & Protzenko (ACM TOPLAS 2016) | Permission-based type system, distinguishes duplicable/affine permissions, Coq soundness proof | References are storable and nameable inside scopes; not expression-scoped |
| PacLang / Relaxed Linear References (ECOOP 2017) | Restricted alias to a linear reference as function argument only; alias cannot co-exist with owner in same scope | Function-argument-scoped only; not an expression-position restriction |
| Linear Haskell — Bernardy et al. (POPL 2018) | Opt-in linear types in a lazy functional language via multiplicity annotations on function arrows | No `&T` mechanism; no RC; GHC-specific; GC runtime |
| RustBelt — Jung et al. (POPL 2018, Iris/Coq) | Formal soundness of Rust including `Rc<T>` and `Arc<T>` | Affine types (not linear); mandatory ownership; lifetimes required |
| QTT / Idris 2 — Atkey (LICS 2018); Brady (ECOOP 2021) | Quantitative Type Theory: multiplicities 0, 1, ω; linear variables erased or used once | No read-reference mechanism; inspecting a linear value by pattern-matching consumes it |
| COGENT — O'Connor, Rizkallah et al. (ICFP 2016; JFP 2021) | Uniqueness (linear) types + certified compilation: proves equivalence between functional value semantics and imperative update semantics in Isabelle/HOL | Uniqueness types only (mandatory); no RC; no interpreter-vs-compiler equivalence for concrete implementations |
| Granule — Orchard et al. (ICFP 2019) | Graded modal types generalizing linear types; coeffect annotations | Richer machinery than linear types alone; borrows are graded, not expression-scoped |
| Affe — Radanne, Saffrich, Thiemann (ICFP 2020) | ML + affine/linear types + Rust-style exclusive and shared borrows + complete type inference, formal soundness | Borrows can be named inside scope (not expression-position-only) |
| Perceus — Reinking, Xie, de Moura, Leijen (PLDI 2021) | Formal model of reference counting in a linear resource calculus (Koka); proves RC is correct implementation of linear value semantics | RC is implementation detail, not programmer-visible type distinction; no opt-in linearity |
| Soundly Handling Linearity — Tang et al. (POPL 2024) | Proves linear types + effect handlers are handled correctly | Not about read references or RC coexistence |
| Functional Ownership through Fractional Uniqueness — Marshall & Orchard (POPL 2024) | Unifies uniqueness, linearity, and Rust-style borrowing under graded types with formal model | Uses graded types (heavier machinery); borrows are not expression-position-only |
| Affect — van Rooij & Krebbers (POPL 2025) | Affine types + effect handlers, Iris/Coq soundness | Not about RC coexistence or read references |
| Austral — Borretti (2022+) | Mandatory linear types, region-scoped borrowing (`&[T, R]`), compiled to C | No formal soundness paper; borrows can be named within scope; no RC; compiled only |
| FabULous — Scherer et al. (2017) | Multi-language interoperability between ML (GC) and a linear language, full abstraction | GC, not RC; boundary is about multi-language interop, not opt-in linearity |
| Quill — Morris (2016) | Mixes linear and unrestricted types via qualified types | No memory management model |

### What is explicitly prior art for Moonlane

- Hindley-Milner type inference
- Reference counting as a memory management strategy
- Algebraic data types and exhaustive pattern matching
- Expression-oriented syntax
- Rust-like surface syntax
- Fibers and channel-based concurrency (Go model)
- The `Perhaps<T>` / `Result<T, E>` error handling pattern
- Linear types as a concept

Claiming novelty for any of the above is a rejection trigger at any serious venue.

---

## Novelty Claims

Three claims were assessed. They are presented with honest verdicts following a systematic literature search.

---

### Claim 1 — Formal soundness of expression-only `&T` in an opt-in linear type system

**Verdict: Defensible if narrowly scoped. The adjacent space is crowded — the claim requires precise framing and a careful related-work section.**

#### What the claim is

A `&T` restricted to expression position only — cannot be bound to a `let`, stored in a struct field, or appear in a function return type — allows inspection of a linear value without consuming it, without requiring any lifetime annotations. Formally characterizing and proving this sound is claimed to be a contribution.

#### What the search found

The *specific syntactic mechanism* — expression-position-only as the no-escape guarantee, with no lifetime annotation machinery — does not appear verbatim in the literature. However, the problem it solves (inspection without consumption) is actively studied:

- **Austral** has region-scoped borrows (`&[T, Region]`) that cannot escape the borrow block. Unlike Moonlane's `&T`, Austral's borrows can be bound to names and passed around inside the scope. The no-escape guarantee comes from a type-level region tag, not from a syntactic expression-position restriction. Austral has no formal soundness paper.
- **Affe** (ICFP 2020) has Rust-style shared borrows that can be named inside a scope. Full type inference and formal soundness. Not expression-position-only.
- **Mezzo** (TOPLAS 2016) has duplicable permissions that allow reading without consuming. References are storable within their permission scope. Full Coq proof. Not expression-position-only.
- **Marshall & Orchard** (POPL 2024) unifies borrowing under graded types. Borrows are graded "fractional" capabilities, not an expression-scoped syntactic restriction.
- **Adoption and Focus** (PLDI 2002) has a "focus" construct for temporary access without consumption. Not expression-scoped; no formal soundness proof.
- **PacLang / Relaxed Linear References** (ECOOP 2017) restricts aliasing to function-argument position specifically. The closest structural analogue, but different: argument-position-only is not the same as expression-position-only, and the formal treatment is different.

#### The precise novelty

The exact design point — *syntactic expression-position restriction as the complete mechanism for preventing escape, with no region tags, no graded annotations, no lifetime variables* — has not been formally characterized as a unified object of study. It is a specific point in the design space between "no inspection without consumption" and "full lifetime-tracked borrows." The contribution would be: characterizing this point precisely, proving it sound, and proving it incomplete (identifying the class of safe programs it cannot express, which require either lifetime annotations or consuming-and-returning style).

#### What needs to exist

- Formal inference rules for the linear type system including `&T`
- A soundness theorem: no use-after-free or double-free in well-typed programs
- A proof (mechanized in Lean 4 or Coq strengthens significantly)
- An incompleteness characterization: what safe programs does the expression-position restriction reject?
- A careful related-work section engaging with Austral, Affe, Mezzo, Marshall-Orchard, Adoption-and-Focus, and PacLang

#### Realistic venues

- **Onward!** or **ECOOP** — for the design analysis with an informal soundness argument
- **ICFP** or **POPL** — for a mechanized proof

---

### Claim 2 — Formal model of RC/linear coexistence

**Verdict: Broadly defensible. Important adjacent work exists (RustBelt, Perceus) but does not cover the exact combination. Framing must be precise.**

#### What the claim is

Mixing RC-managed values (the default) with opt-in linear types, with explicit boundary rules (no `Rc<LinearT>`, no `Arc<LinearT>`, linear values can move through channels as consumption events), has not been formally analyzed.

#### What the search found

- **RustBelt** (POPL 2018) formally verifies Rust's safety including `Rc<T>` and `Arc<T>` in Iris/Coq. However, Rust is *pervasively affine* (not RC-default + linear-opt-in), uses lifetimes, and has no "no Rc<linear_value>" rule (because Rust has no linear types in the classical sense — its `Drop` types are affine, not linear). RustBelt models affine+Rc, not linear-opt-in+RC-default.
- **Perceus** (PLDI 2021) formally proves RC is a correct implementation of a linear resource calculus. The RC is an *implementation detail* of the Koka runtime, not a programmer-visible type-level distinction. The programmer does not write RC types; the compiler inserts them. This is not the same as "RC-managed values as a programmer-visible default."
- **Mackie (1995)** establishes a theoretical relationship between RC and linear logic as an implementation strategy. Not about programmer-visible mixing.
- **Reference Counting with Linear Types** (alt-romes, 2024 Haskell) layers RC on top of GHC's GC using Linear Haskell's linear arrows. A library technique, not a formal model.
- **FabULous** (2017) models a multi-language system combining GC-ML and a linear language. GC, not RC; the boundary rules are about multi-language type-directed translation, not about RC wrapping linear values.

#### The precise novelty

The specific combination — programmer-visible RC-managed values as default, linear types as an opt-in annotation, an explicit typing rule forbidding `Rc<T>` and `Arc<T>` when `T` is linear, and a formal proof that no program can have simultaneous RC and linear access to the same value — appears uncharted. RustBelt is the closest formal work but models a different design. Perceus models RC formally but from an implementation-semantics angle, not a programmer-visible type distinction angle.

#### What needs to exist

- Resolution of the memory model RFC cluster (boundary rules must be decided before they can be formalized)
- Formal type system including both RC-managed and linear-managed values with explicit boundary rules
- A soundness theorem: no aliasing violation (no simultaneous RC and linear access to the same value)
- Engagement with RustBelt and Perceus in the related-work section

#### Realistic venues

- **PLDI** — if implementation evaluation accompanies the formal model
- **ICFP** — if the type-theoretic contribution is the focus
- **OOPSLA** — if the practical programming model is the focus

---

### Claim 3 — Dual-mode semantic equivalence

**Verdict: Significantly overstated as written. COGENT (JFP 2021) directly addresses the core problem. The claim must be substantially reframed or dropped.**

#### What the claim is

Proving that an interpreter and a native compiler implement identical semantics for a linear type system is a novel formal verification problem.

#### What the search found — COGENT

**COGENT** (O'Connor, Rizkallah et al., ICFP 2016; JFP 2021) is the most directly relevant and damaging prior art. COGENT has a uniqueness (linear) type system and proves — with a machine-checked Isabelle/HOL proof — the equivalence between two operational semantics: a purely functional *value semantics* (suitable for high-level reasoning) and an imperative *update semantics* (the basis for the generated C output). The paper explicitly calls this the first machine-checked proof of such a dual-semantics property for a full-scale language with uniqueness types.

This is dual-mode semantic equivalence for a linear/uniqueness-typed language, formally proved. The "tree-walk interpreter vs. native compiler" framing is different from COGENT's "value semantics vs. update semantics" — COGENT's two semantics are both theoretical, not two separately implemented executable engines — but the core verification problem (proving two execution models are observationally equivalent for a linear-typed language) is the same.

Any paper making Claim 3 must begin by acknowledging COGENT and then argue precisely why the concrete-implementations framing adds something over COGENT's abstract-semantics framing.

#### What can be salvaged

A narrower, more honest version of the claim: "proving equivalence between a *concrete tree-walk interpreter* and a *concrete optimizing native compiler*, both separately implemented, for an opt-in linear type system" — where the novelty is in the *methodology* (translation validation or bisimulation between two executable artifacts rather than two mathematical semantics) rather than the problem itself. This is a weaker but still valid contribution, closer to CompCert's style of work than COGENT's.

This narrower version is a long-term goal — it requires both backends to be production-quality — and is a stronger engineering contribution than a theoretical one.

#### Revised status

Downgraded from "lower confidence" to **very long-term / speculative**. The theoretical core has been done by COGENT. The engineering contribution (concrete implementations) is real but years away and less academically distinctive.

---

## Revised Confidence Rankings

| Claim | Original confidence | Revised confidence | Key blocker |
|---|---|---|---|
| `&T` soundness (Claim 1) | High | Medium-high | Adjacent space crowded; related-work section is critical; mechanized proof needed for top venues |
| RC/linear coexistence (Claim 2) | Medium | Medium | RFC cluster must be resolved first; RustBelt/Perceus must be carefully distinguished |
| Dual-mode equivalence (Claim 3) | Lower | Very low / speculative | COGENT covers the theoretical core; requires both backends (years away) |

---

## What Would Make the Project More Academically Compelling

**1. Formalize the linear type system now (high impact, moderate effort)**

Write down the inference rules formally — judgment forms, typing rules for `&T`, branch merge rules, loop restriction. This is the foundation for Claim 1. The rules are already described in prose in RFC-0024; translating them to formal notation is the work. Lean 4 is recommended over Coq for syntax reasons.

**2. Write the incompleteness analysis for `&T` (high impact, low effort)**

Identify and document programs that are safe but that `&T` cannot express without a consuming-and-returning style. No implementation required — this is a design analysis. It gives the paper a precise negative result alongside the soundness claim, and directly distinguishes the approach from Affe, Mezzo, and Marshall-Orchard.

**3. Engage with the Marshall-Orchard (POPL 2024) paper carefully**

This is the closest recent work to Claim 1. The paper's graded "fractional uniqueness" approach is strictly more expressive than `&T` — it can accept programs that `&T` rejects. A paper on `&T` should frame it as a *simpler, inference-friendly* point in the same design space, trading expressiveness for implementation simplicity and no annotation burden.

**4. Resolve the memory model RFC cluster (enables Claim 2)**

The boundary rules between RC and linear values must be decided before they can be formalized. This is an implementation prerequisite regardless.

**5. Mechanize the proof (dramatically increases publishability of Claim 1)**

A mechanized proof in Lean 4 or Coq is much harder to reject than a paper proof. For top venues (POPL, ICFP), it is increasingly expected.

---

## Publication Strategy (Revised)

**Short term — Claim 1 with incompleteness analysis:**

Write a paper scoped to the linear fragment: `linear struct`, `&T`, `drop`, branching, and loops. Formal system + soundness proof + incompleteness characterization + comparison with Affe, Mezzo, and Marshall-Orchard. Target Onward! or ECOOP for a first submission without mechanized proof; target ICFP or POPL with mechanized proof.

The related-work section must engage with at minimum: Linear Haskell, Austral, Affe, Mezzo, Marshall-Orchard (POPL 2024), Adoption-and-Focus, and PacLang. The precise distinction from each must be stated.

**Medium term — Claim 2 after RFC cluster resolution:**

Extend the formal model to include RC/linear coexistence boundary rules. A paper covering both Claim 1 and Claim 2 is likely stronger than two separate papers. Target PLDI or OOPSLA.

**Long term — revised Claim 3 (concrete implementation equivalence):**

Once both backends exist, the engineering contribution of verifying two concrete implementations against a shared spec is real. Frame it as a methodology paper (how to specify a linear-typed language precisely enough to be implemented twice independently) rather than a theoretical novelty. Target a systems/engineering venue or PLDI.

---

## References

- Project vision: `docs/internal/vision.md`
- RFC-0024: `docs/internal/rfcs/rfc-0024-linear-types.md` — linear type system design
- RFC cluster report: `docs/internal/rfc-cluster-memory-model.md` — RC/linear coexistence rules

### Papers cited

- Girard (1987) — *Linear logic*. Theoretical Computer Science 50(1)
- Wadler (1990) — *Linear types can change the world*. IFIP TC 2 Working Conference
- Fähndrich & DeLine (PLDI 2002) — *Adoption and focus: practical linear types for imperative programming*
- Grossman et al. (PLDI 2002) — *Region-based memory management in Cyclone*
- Pottier & Protzenko (ACM TOPLAS 2016) — *The design and formalization of Mezzo, a permission-based programming language*
- Relaxed Linear References (ECOOP 2017) — *Relaxed linear references for lock-free data structures*
- Bernardy et al. (POPL 2018) — *Linear Haskell: practical linearity in a higher-order polymorphic language*
- Jung et al. (POPL 2018) — *RustBelt: securing the foundations of the Rust programming language*
- Atkey (LICS 2018) — *Syntax and semantics of quantitative type theory*
- O'Connor, Rizkallah et al. (ICFP 2016; JFP 2021) — *Cogent: uniqueness types and certifying compilation* (**directly relevant to Claim 3**)
- Orchard et al. (ICFP 2019) — *Quantitative program reasoning with graded modal types* (Granule)
- Radanne, Saffrich, Thiemann (ICFP 2020) — *Kindly bent to free us* (Affe)
- Reinking, Xie, de Moura, Leijen (PLDI 2021) — *Perceus: garbage-free reference counting with reuse*
- Brady (ECOOP 2021) — *Idris 2: quantitative type theory in practice*
- Marshall & Orchard (POPL 2024) — *Functional ownership through fractional uniqueness* (**directly relevant to Claim 1**)
- Tang et al. (POPL 2024) — *Soundly handling linearity*
- van Rooij & Krebbers (POPL 2025) — *Affect: an affine type and effect system*
- Walker (2005) — *Substructural type systems*. In Pierce (ed.), *Advanced Topics in Types and Programming Languages*. MIT Press
- Borretti (2022+) — *Austral language specification*. austral-lang.org
