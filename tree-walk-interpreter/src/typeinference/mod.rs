/// Type inference module for Yolang.
///
/// This module is being built incrementally with comprehensive tests.
/// See tasks in docs/Yolang/tasks/epic-001-typechecker/ for the step-by-step breakdown.
///
/// Current status: Foundation phase (type variables)

use crate::ast::Span;
use crate::types::Type;
use crate::error::YolangError;
use std::collections::{HashMap, HashSet};

// ── Phase 1: Type Variables ───────────────────────────────────────────────────

/// A type variable representing an unknown type during inference.
/// Each type variable has a unique ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeVar(pub u32);

impl std::fmt::Display for TypeVar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "?t{}", self.0)
    }
}

/// Counter for generating fresh type variables.
pub struct TypeVarGenerator {
    counter: u32,
}

impl TypeVarGenerator {
    /// Create a new type variable generator.
    pub fn new() -> Self {
        TypeVarGenerator { counter: 0 }
    }

    /// Generate a fresh type variable.
    pub fn fresh(&mut self) -> TypeVar {
        let var = TypeVar(self.counter);
        self.counter += 1;
        var
    }

    /// Get the current counter state (for testing).
    pub fn counter(&self) -> u32 {
        self.counter
    }
}

impl Default for TypeVarGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ── Phase 2: Inference Types ──────────────────────────────────────────────────


/// A type that may contain unresolved type variables.
/// Used during inference before all types are known.
/// Distinct from `Type`, which is fully resolved and contains no variables.
#[derive(Debug, Clone, PartialEq)]
pub enum InferType {
    /// A fully resolved concrete type.
    Concrete(Type),
    /// An unknown type represented by a type variable.
    Var(TypeVar),
    /// A function type with parameter types and a return type.
    Fun(Vec<InferType>, Box<InferType>),
    /// A tuple type.
    Tuple(Vec<InferType>),
    /// A homogeneous array type.
    Array(Box<InferType>),
    /// A named type (struct, enum) with type arguments.
    Named(String, Vec<InferType>),
}

impl InferType {
    pub fn int() -> Self { InferType::Concrete(Type::Int) }
    pub fn float() -> Self { InferType::Concrete(Type::Float) }
    pub fn bool() -> Self { InferType::Concrete(Type::Bool) }
    pub fn str() -> Self { InferType::Concrete(Type::Str) }
    pub fn unit() -> Self { InferType::Concrete(Type::Unit) }
    pub fn var(v: TypeVar) -> Self { InferType::Var(v) }
}

impl std::fmt::Display for InferType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferType::Concrete(t) => write!(f, "{}", t),
            InferType::Var(v) => write!(f, "{}", v),
            InferType::Fun(params, ret) => {
                write!(f, "fun(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", ret)
            }
            InferType::Tuple(ts) => {
                write!(f, "(")?;
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            InferType::Array(t) => write!(f, "{}[]", t),
            InferType::Named(name, args) => {
                write!(f, "{}", name)?;
                if !args.is_empty() {
                    write!(f, "<")?;
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{}", a)?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
        }
    }
}

// ── Phase 3: Substitution ─────────────────────────────────────────────────────

/// A map from type variables to their resolved `InferType`s.
/// The right-hand side may still contain variables — `apply` chases them transitively.
#[derive(Debug, Clone, Default)]
pub struct Substitution {
    bindings: HashMap<TypeVar, InferType>,
}

impl Substitution {
    pub fn new() -> Self {
        Substitution { bindings: HashMap::new() }
    }

    /// Record that `var` maps to `ty`.
    pub fn bind(&mut self, var: TypeVar, ty: InferType) {
        self.bindings.insert(var, ty);
    }

    /// Look up the direct binding for `var`, if any.
    pub fn lookup(&self, var: TypeVar) -> Option<&InferType> {
        self.bindings.get(&var)
    }

    /// Recursively replace all type variables in `ty` using this substitution.
    pub fn apply(&self, ty: &InferType) -> InferType {
        match ty {
            InferType::Concrete(_) => ty.clone(),
            InferType::Var(v) => match self.bindings.get(v) {
                Some(resolved) => self.apply(resolved),
                None => ty.clone(),
            },
            InferType::Fun(params, ret) => InferType::Fun(
                params.iter().map(|p| self.apply(p)).collect(),
                Box::new(self.apply(ret)),
            ),
            InferType::Tuple(ts) => InferType::Tuple(ts.iter().map(|t| self.apply(t)).collect()),
            InferType::Array(t) => InferType::Array(Box::new(self.apply(t))),
            InferType::Named(name, args) => {
                InferType::Named(name.clone(), args.iter().map(|a| self.apply(a)).collect())
            }
        }
    }

    /// Produce a substitution equivalent to applying `self` first, then `other`
    /// (i.e. `other ∘ self` in mathematical notation).
    ///
    /// `self` wins on overlap: if both substitutions bind `?t0`, `other` is applied
    /// to `self`'s value — not to the variable itself — so a concrete value from
    /// `self` passes through `other` unchanged. This matches Algorithm W, where a
    /// variable is unified at most once and later substitutions refine free variables
    /// in the *values*, not the *keys*.
    pub fn compose(&self, other: &Substitution) -> Substitution {
        let mut result = Substitution::new();
        for (var, ty) in &self.bindings {
            result.bind(*var, other.apply(ty));
        }
        for (var, ty) in &other.bindings {
            result.bindings.entry(*var).or_insert_with(|| ty.clone());
        }
        result
    }
}

// ── Phase 4: Unification ──────────────────────────────────────────────────────

/// Returns true if `var` appears anywhere inside `ty`.
/// Used by the occurs check to prevent infinite types like `?t0 = Array<?t0>`.
fn occurs_in(var: TypeVar, ty: &InferType) -> bool {
    match ty {
        InferType::Concrete(_) => false,
        InferType::Var(v) => *v == var,
        InferType::Fun(params, ret) => {
            params.iter().any(|p| occurs_in(var, p)) || occurs_in(var, ret)
        }
        InferType::Tuple(ts) => ts.iter().any(|t| occurs_in(var, t)),
        InferType::Array(t) => occurs_in(var, t),
        InferType::Named(_, args) => args.iter().any(|a| occurs_in(var, a)),
    }
}

/// Bind `var` to `ty`, failing if the occurs check would create an infinite type.
fn bind_var(var: TypeVar, ty: &InferType) -> Result<Substitution, YolangError> {
    if let InferType::Var(v) = ty {
        if *v == var {
            return Ok(Substitution::new());
        }
    }
    if occurs_in(var, ty) {
        return Err(YolangError::internal(format!(
            "occurs check failed: {} occurs in {}",
            var, ty
        )));
    }
    let mut s = Substitution::new();
    s.bind(var, ty.clone());
    Ok(s)
}

/// Unify two inference types, returning a substitution that makes them equal.
///
/// Returns an error if the types are structurally incompatible or if the occurs
/// check detects an infinite type.
pub fn unify(a: &InferType, b: &InferType) -> Result<Substitution, YolangError> {
    match (a, b) {
        (InferType::Concrete(t1), InferType::Concrete(t2)) => {
            if t1 == t2 {
                Ok(Substitution::new())
            } else {
                Err(YolangError::internal(format!("cannot unify {} with {}", a, b)))
            }
        }
        (InferType::Var(v), _) => bind_var(*v, b),
        (_, InferType::Var(v)) => bind_var(*v, a),
        (InferType::Fun(params1, ret1), InferType::Fun(params2, ret2)) => {
            if params1.len() != params2.len() {
                return Err(YolangError::internal(format!("cannot unify {} with {}", a, b)));
            }
            let mut subst = Substitution::new();
            for (p1, p2) in params1.iter().zip(params2.iter()) {
                let s = unify(&subst.apply(p1), &subst.apply(p2))?;
                subst = subst.compose(&s);
            }
            let s = unify(&subst.apply(ret1), &subst.apply(ret2))?;
            Ok(subst.compose(&s))
        }
        (InferType::Tuple(ts1), InferType::Tuple(ts2)) => {
            if ts1.len() != ts2.len() {
                return Err(YolangError::internal(format!("cannot unify {} with {}", a, b)));
            }
            let mut subst = Substitution::new();
            for (t1, t2) in ts1.iter().zip(ts2.iter()) {
                let s = unify(&subst.apply(t1), &subst.apply(t2))?;
                subst = subst.compose(&s);
            }
            Ok(subst)
        }
        (InferType::Array(t1), InferType::Array(t2)) => unify(t1, t2),
        (InferType::Named(n1, args1), InferType::Named(n2, args2)) => {
            if n1 != n2 || args1.len() != args2.len() {
                return Err(YolangError::internal(format!("cannot unify {} with {}", a, b)));
            }
            let mut subst = Substitution::new();
            for (a1, a2) in args1.iter().zip(args2.iter()) {
                let s = unify(&subst.apply(a1), &subst.apply(a2))?;
                subst = subst.compose(&s);
            }
            Ok(subst)
        }
        _ => Err(YolangError::internal(format!("cannot unify {} with {}", a, b))),
    }
}

// ── Phase 5: Constraints ──────────────────────────────────────────────────────

/// A deferred type equation: `lhs` and `rhs` must unify, recorded with the
/// source `span` so that failures produce actionable error messages.
#[derive(Debug, Clone)]
pub struct Constraint {
    pub lhs: InferType,
    pub rhs: InferType,
    pub span: Span,
}

impl Constraint {
    pub fn new(lhs: InferType, rhs: InferType, span: Span) -> Self {
        Self { lhs, rhs, span }
    }
}

/// Solve a list of constraints by unifying each `lhs`/`rhs` pair in order.
///
/// The running substitution is applied to both sides before each unification
/// so that earlier bindings propagate into later constraints. Errors are
/// reported with the source span of the offending constraint.
pub fn solve_constraints(constraints: Vec<Constraint>) -> Result<Substitution, YolangError> {
    let mut subst = Substitution::new();
    for c in constraints {
        let lhs = subst.apply(&c.lhs);
        let rhs = subst.apply(&c.rhs);
        let s = unify(&lhs, &rhs).map_err(|_| {
            YolangError::type_error(format!("cannot unify {} with {}", lhs, rhs), &c.span)
        })?;
        subst = subst.compose(&s);
    }
    Ok(subst)
}

// ── Phase 6: Type Schemes ─────────────────────────────────────────────────────

/// Collect all type variables that appear free in `ty`.
pub fn free_vars(ty: &InferType) -> HashSet<TypeVar> {
    match ty {
        InferType::Concrete(_) => HashSet::new(),
        InferType::Var(v) => [*v].into(),
        InferType::Fun(params, ret) => {
            let mut vars = free_vars(ret);
            for p in params { vars.extend(free_vars(p)); }
            vars
        }
        InferType::Tuple(ts) => ts.iter().flat_map(free_vars).collect(),
        InferType::Array(t) => free_vars(t),
        InferType::Named(_, args) => args.iter().flat_map(free_vars).collect(),
    }
}

/// A universally quantified type: `∀ quantified_vars. ty`.
///
/// Variables in `quantified_vars` are locally owned — each use site gets
/// fresh copies via `instantiate`, enabling let-polymorphism.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeScheme {
    pub quantified_vars: Vec<TypeVar>,
    pub ty: InferType,
}

impl TypeScheme {
    /// A monomorphic scheme — no quantified variables.
    pub fn mono(ty: InferType) -> Self {
        Self { quantified_vars: vec![], ty }
    }
}

impl std::fmt::Display for TypeScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.quantified_vars.is_empty() {
            write!(f, "{}", self.ty)
        } else {
            write!(f, "∀")?;
            for (i, v) in self.quantified_vars.iter().enumerate() {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{}", v)?;
            }
            write!(f, ". {}", self.ty)
        }
    }
}

/// Generalize `ty` into a type scheme by quantifying over all type variables
/// that appear free in `ty` but not in `env_free_vars`.
///
/// `env_free_vars` is the set of variables that are still being solved in the
/// surrounding environment — those must not be captured.
pub fn generalize(ty: InferType, env_free_vars: &HashSet<TypeVar>) -> TypeScheme {
    let mut quantified: Vec<TypeVar> = free_vars(&ty)
        .difference(env_free_vars)
        .copied()
        .collect();
    quantified.sort();
    TypeScheme { quantified_vars: quantified, ty }
}

/// Instantiate a type scheme by replacing each quantified variable with a
/// fresh type variable from `gen`. Called once per use site.
pub fn instantiate(scheme: &TypeScheme, gen: &mut TypeVarGenerator) -> InferType {
    let mut subst = Substitution::new();
    for &var in &scheme.quantified_vars {
        subst.bind(var, InferType::Var(gen.fresh()));
    }
    subst.apply(&scheme.ty)
}

// ── Phase 7: Inference Context ────────────────────────────────────────────────

/// State threaded through the entire AST walk during type inference.
///
/// Owns the variable generator, both environments, and the accumulated
/// constraint list. Call `solve()` after the walk to get the final substitution.
///
/// `mono_env` is a scope stack: call `push_scope`/`pop_scope` in matched pairs
/// when entering and leaving lexical scopes (function bodies, blocks).
/// `poly_env` is flat — polymorphic bindings are top-level only.
pub struct InferContext {
    var_gen: TypeVarGenerator,
    mono_env: Vec<HashMap<String, InferType>>,
    poly_env: HashMap<String, TypeScheme>,
    constraints: Vec<Constraint>,
}

impl InferContext {
    pub fn new() -> Self {
        Self {
            var_gen: TypeVarGenerator::new(),
            mono_env: vec![HashMap::new()],  // root scope pre-pushed
            poly_env: HashMap::new(),
            constraints: Vec::new(),
        }
    }

    /// Enter a new lexical scope (e.g. a function body or block).
    /// Must be matched with a call to `pop_scope`.
    pub fn push_scope(&mut self) {
        self.mono_env.push(HashMap::new());
    }

    /// Exit the current lexical scope, discarding all bindings introduced in it.
    /// Panics if called with no inner scope (i.e. at the root).
    pub fn pop_scope(&mut self) {
        assert!(self.mono_env.len() > 1, "pop_scope called at root scope");
        self.mono_env.pop();
    }

    /// Generate a fresh type variable.
    pub fn fresh_var(&mut self) -> InferType {
        InferType::Var(self.var_gen.fresh())
    }

    /// Bind a name to a monomorphic type in the current scope (e.g. a function parameter).
    pub fn bind_mono(&mut self, name: impl Into<String>, ty: InferType) {
        self.mono_env.last_mut().unwrap().insert(name.into(), ty);
    }

    /// Bind a name to a polymorphic type scheme (e.g. a let-binding).
    pub fn bind_poly(&mut self, name: impl Into<String>, scheme: TypeScheme) {
        self.poly_env.insert(name.into(), scheme);
    }

    /// Look up a name. Polymorphic bindings are automatically instantiated with
    /// fresh variables; monomorphic bindings are searched innermost-scope-first.
    /// Poly env takes precedence over mono env.
    pub fn lookup(&mut self, name: &str) -> Option<InferType> {
        if let Some(scheme) = self.poly_env.get(name).cloned() {
            Some(instantiate(&scheme, &mut self.var_gen))
        } else {
            self.mono_env.iter().rev()
                .find_map(|scope| scope.get(name))
                .cloned()
        }
    }

    /// Collect all type variables that appear free across all current mono scopes.
    /// Pass this to `generalize()` to avoid capturing variables still being solved.
    pub fn env_free_vars(&self) -> HashSet<TypeVar> {
        self.mono_env.iter()
            .flat_map(|scope| scope.values())
            .flat_map(free_vars)
            .collect()
    }

    /// Record that `lhs` and `rhs` must unify, tagged with its source location.
    pub fn add_constraint(&mut self, lhs: InferType, rhs: InferType, span: Span) {
        self.constraints.push(Constraint::new(lhs, rhs, span));
    }

    /// Solve all accumulated constraints and return the resulting substitution.
    pub fn solve(self) -> Result<Substitution, YolangError> {
        solve_constraints(self.constraints)
    }
}

impl Default for InferContext {
    fn default() -> Self {
        Self::new()
    }
}
