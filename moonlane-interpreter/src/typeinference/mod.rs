/// Type inference module for Moonlane.
///
/// This module is being built incrementally with comprehensive tests.
/// See tasks in docs/Moonlane/tasks/epic-001-typechecker/ for the step-by-step breakdown.
///
/// Current status: Foundation phase (type variables)

use crate::ast::Span;
use crate::types::Type;
use crate::error::MoonlaneError;
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
///
/// # Invariant: TypeVar identity is global
///
/// `TypeVar` equality means identity — two vars with the same `u32` are the *same* variable.
/// All `TypeVarGenerator` instances within a single type-check run must therefore be
/// coordinated: each new generator must start past the highest counter value produced by
/// any earlier generator.  Creating an independent `TypeVarGenerator::new()` in a call site
/// that produces vars intended to be globally unique will cause collisions — the "fresh"
/// var may be identical to an already-used one, producing self-referential substitutions
/// and infinite recursion in `Substitution::apply`.
///
/// The correct pattern: `InferContext` owns the generator for Pass 1.  After Pass 1,
/// call `ctx.split_gen()` to obtain a new generator that starts past all Pass 1 vars,
/// then thread that single instance through Pass 2 (and any intermediate steps like
/// `register_builtin_poly_schemes`).
pub struct TypeVarGenerator {
    counter: u32,
}

impl TypeVarGenerator {
    /// Create a new type variable generator.
    pub fn new() -> Self {
        TypeVarGenerator { counter: 0 }
    }

    pub fn with_counter(start: u32) -> Self {
        TypeVarGenerator { counter: start }
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
    /// The bottom type `!` — produced by diverging expressions (infinite loops with
    /// no reachable `break`, `return`, `panic!`). Unifies with any type.
    Never,
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
    pub fn never() -> Self { InferType::Never }
    pub fn var(v: TypeVar) -> Self { InferType::Var(v) }
}

impl std::fmt::Display for InferType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferType::Concrete(t) => write!(f, "{}", t),
            InferType::Var(v) => write!(f, "{}", v),
            InferType::Never => write!(f, "!"),
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
            InferType::Concrete(_) | InferType::Never => ty.clone(),
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
        InferType::Concrete(_) | InferType::Never => false,
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
fn bind_var(var: TypeVar, ty: &InferType) -> Result<Substitution, MoonlaneError> {
    if let InferType::Var(v) = ty {
        if *v == var {
            return Ok(Substitution::new());
        }
    }
    if occurs_in(var, ty) {
        return Err(MoonlaneError::internal(format!(
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
pub fn unify(a: &InferType, b: &InferType) -> Result<Substitution, MoonlaneError> {
    match (a, b) {
        // Never is the bottom type — it coerces to any type.
        (InferType::Never, _) | (_, InferType::Never) => Ok(Substitution::new()),
        (InferType::Concrete(t1), InferType::Concrete(t2)) => {
            if t1 == t2 {
                Ok(Substitution::new())
            } else {
                Err(MoonlaneError::internal(format!("cannot unify {} with {}", a, b)))
            }
        }
        (InferType::Var(v), _) => bind_var(*v, b),
        (_, InferType::Var(v)) => bind_var(*v, a),
        (InferType::Fun(params1, ret1), InferType::Fun(params2, ret2)) => {
            if params1.len() != params2.len() {
                return Err(MoonlaneError::internal(format!("cannot unify {} with {}", a, b)));
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
                return Err(MoonlaneError::internal(format!("cannot unify {} with {}", a, b)));
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
                return Err(MoonlaneError::internal(format!("cannot unify {} with {}", a, b)));
            }
            let mut subst = Substitution::new();
            for (a1, a2) in args1.iter().zip(args2.iter()) {
                let s = unify(&subst.apply(a1), &subst.apply(a2))?;
                subst = subst.compose(&s);
            }
            Ok(subst)
        }
        _ => Err(MoonlaneError::internal(format!("cannot unify {} with {}", a, b))),
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
pub fn solve_constraints(constraints: Vec<Constraint>) -> Result<Substitution, MoonlaneError> {
    let mut subst = Substitution::new();
    for c in constraints {
        let lhs = subst.apply(&c.lhs);
        let rhs = subst.apply(&c.rhs);
        let s = unify(&lhs, &rhs).map_err(|_| {
            MoonlaneError::type_error(crate::error::TypeErrorCode::T0001, format!("cannot unify {} with {}", lhs, rhs), &c.span)
        })?;
        subst = subst.compose(&s);
    }
    Ok(subst)
}

// ── Phase 6: Type Schemes ─────────────────────────────────────────────────────

/// Collect all type variables that appear free in `ty`.
pub fn free_vars(ty: &InferType) -> HashSet<TypeVar> {
    match ty {
        InferType::Concrete(_) | InferType::Never => HashSet::new(),
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

// ── Enum environment ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VariantInfo {
    pub name:   String,
    pub fields: Vec<(String, InferType)>,
}

#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub type_params: Vec<TypeVar>,
    pub variants:    Vec<VariantInfo>,
}

// ── Type Registry ─────────────────────────────────────────────────────────────

/// Immutable store of type definitions built during the pre-pass.
/// Created by `build_registry` and injected into `InferContext` before inference begins.
pub struct TypeRegistry {
    struct_env:         HashMap<String, Vec<(String, InferType)>>,
    /// Ordered type-parameter TypeVars per generic struct (absent for non-generic structs).
    struct_type_params: HashMap<String, Vec<TypeVar>>,
    /// Tracks which struct names were registered in each lexical scope so they
    /// can be removed on scope exit. Empty when outside any scoped block.
    struct_scope_stack: Vec<Vec<String>>,
    method_env: HashMap<String, HashMap<String, InferType>>,
    enum_env:   HashMap<String, EnumInfo>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            struct_env:         HashMap::new(),
            struct_type_params: HashMap::new(),
            struct_scope_stack: Vec::new(),
            method_env:         HashMap::new(),
            enum_env:           HashMap::new(),
        }
    }

    pub fn register_struct_fields(&mut self, name: String, fields: Vec<(String, InferType)>) {
        self.struct_env.insert(name.clone(), fields);
        if let Some(scope) = self.struct_scope_stack.last_mut() {
            scope.push(name);
        }
    }

    pub fn push_struct_scope(&mut self) {
        self.struct_scope_stack.push(Vec::new());
    }

    pub fn pop_struct_scope(&mut self) {
        if let Some(names) = self.struct_scope_stack.pop() {
            for name in names {
                self.struct_env.remove(&name);
            }
        }
    }

    pub fn register_method(&mut self, type_name: String, method_name: String, fun_ty: InferType) {
        self.method_env.entry(type_name).or_default().insert(method_name, fun_ty);
    }

    pub fn register_struct_type_params(&mut self, name: String, type_params: Vec<TypeVar>) {
        self.struct_type_params.insert(name, type_params);
    }

    pub fn register_enum(&mut self, name: String, info: EnumInfo) {
        self.enum_env.insert(name, info);
    }

    pub fn struct_fields(&self, name: &str) -> Option<&Vec<(String, InferType)>> {
        self.struct_env.get(name)
    }

    pub fn struct_type_params_for(&self, name: &str) -> Option<&Vec<TypeVar>> {
        self.struct_type_params.get(name)
    }

    pub fn method_type(&self, type_name: &str, method_name: &str) -> Option<&InferType> {
        self.method_env.get(type_name)?.get(method_name)
    }

    pub fn enum_info(&self, name: &str) -> Option<&EnumInfo> {
        self.enum_env.get(name)
    }

    pub(crate) fn raw_struct_env(&self) -> &HashMap<String, Vec<(String, InferType)>> {
        &self.struct_env
    }

    pub(crate) fn raw_struct_type_params(&self) -> &HashMap<String, Vec<TypeVar>> {
        &self.struct_type_params
    }

    pub(crate) fn raw_method_env(&self) -> &HashMap<String, HashMap<String, InferType>> {
        &self.method_env
    }

    pub(crate) fn raw_enum_env(&self) -> &HashMap<String, EnumInfo> {
        &self.enum_env
    }
}

impl Default for TypeRegistry {
    fn default() -> Self { Self::new() }
}

// ── Phase 7: Inference Context ────────────────────────────────────────────────

/// State threaded through the entire AST walk during type inference.
///
/// Owns the variable generator, both environments, and the accumulated
/// constraint list. Call `solve()` after the walk to get the final substitution.
///
/// `mono_env` is a scope stack: call `push_scope`/`pop_scope` in matched pairs
/// when entering and leaving lexical scopes (function bodies, blocks).
/// `poly_env` is scoped like `mono_env`; each `push_scope`/`pop_scope` adds/removes a layer.
pub struct InferContext {
    var_gen: TypeVarGenerator,
    mono_env: Vec<HashMap<String, (InferType, bool)>>,
    poly_env: Vec<HashMap<String, TypeScheme>>,
    constraints: Vec<Constraint>,
    current_return_type: Option<InferType>,
    current_break_type:  Option<InferType>,
    registry: TypeRegistry,
}

impl InferContext {
    /// Create a new inference context with a pre-built registry and a generator
    /// that has already been advanced past all TypeVars allocated during registry
    /// construction, ensuring global TypeVar uniqueness.
    pub fn new(registry: TypeRegistry, gen: TypeVarGenerator) -> Self {
        Self {
            var_gen: gen,
            mono_env: vec![HashMap::new()],  // root scope pre-pushed
            poly_env: vec![HashMap::new()],  // root scope pre-pushed
            constraints: Vec::new(),
            current_return_type: None,
            current_break_type:  None,
            registry,
        }
    }

    pub fn register_struct_fields(&mut self, name: String, fields: Vec<(String, InferType)>) {
        self.registry.register_struct_fields(name, fields);
    }

    pub fn get_struct_type_params(&self, name: &str) -> Option<&Vec<TypeVar>> {
        self.registry.struct_type_params_for(name)
    }

    pub fn push_struct_scope(&mut self) { self.registry.push_struct_scope(); }
    pub fn pop_struct_scope(&mut self)  { self.registry.pop_struct_scope(); }

    pub fn register_method(&mut self, type_name: String, method_name: String, fun_ty: InferType) {
        self.registry.register_method(type_name, method_name, fun_ty);
    }

    pub fn get_struct_fields(&self, name: &str) -> Option<&Vec<(String, InferType)>> {
        self.registry.struct_fields(name)
    }

    pub fn get_method_type(&self, type_name: &str, method_name: &str) -> Option<&InferType> {
        self.registry.method_type(type_name, method_name)
    }

    pub fn register_enum(&mut self, name: String, info: EnumInfo) {
        self.registry.register_enum(name, info);
    }

    pub fn get_enum(&self, name: &str) -> Option<&EnumInfo> {
        self.registry.enum_info(name)
    }

    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    pub fn fresh_type_var_raw(&mut self) -> TypeVar {
        self.var_gen.fresh()
    }

    /// Return a new generator whose counter starts immediately past all vars
    /// allocated by this context.  Use this to hand off to a subsequent phase
    /// (Pass 2, `register_builtin_poly_schemes`) so that every `TypeVar` ever
    /// produced during a type-check run is globally unique.
    pub fn split_gen(&self) -> TypeVarGenerator {
        TypeVarGenerator::with_counter(self.var_gen.counter())
    }

    /// Enter a new lexical scope (e.g. a function body or block).
    /// Must be matched with a call to `pop_scope`.
    pub fn push_scope(&mut self) {
        self.mono_env.push(HashMap::new());
        self.poly_env.push(HashMap::new());
    }

    /// Exit the current lexical scope, discarding all bindings introduced in it.
    /// Panics if called with no inner scope (i.e. at the root).
    pub fn pop_scope(&mut self) {
        assert!(self.mono_env.len() > 1, "pop_scope called at root scope");
        self.mono_env.pop();
        assert!(self.poly_env.len() > 1, "pop_scope called at root scope");
        self.poly_env.pop();
    }

    /// Generate a fresh type variable.
    pub fn fresh_var(&mut self) -> InferType {
        InferType::Var(self.var_gen.fresh())
    }

    /// Bind a name to a monomorphic type in the current scope.
    /// `is_mutable` is `true` for `mut` bindings, `false` for `let` bindings and parameters.
    pub fn bind_mono(&mut self, name: impl Into<String>, ty: InferType, is_mutable: bool) {
        self.mono_env.last_mut().unwrap().insert(name.into(), (ty, is_mutable));
    }

    /// Bind a name to a polymorphic type scheme in the current scope.
    pub fn bind_poly(&mut self, name: impl Into<String>, scheme: TypeScheme) {
        self.poly_env.last_mut().unwrap().insert(name.into(), scheme);
    }

    /// Look up a name. Polymorphic bindings are automatically instantiated with
    /// fresh variables; monomorphic bindings are searched innermost-scope-first.
    /// Poly env takes precedence over mono env within each scope level.
    pub fn lookup(&mut self, name: &str) -> Option<InferType> {
        if let Some(scheme) = self.poly_env.iter().rev().find_map(|s| s.get(name)).cloned() {
            Some(instantiate(&scheme, &mut self.var_gen))
        } else {
            self.mono_env.iter().rev()
                .find_map(|scope| scope.get(name))
                .map(|(ty, _)| ty.clone())
        }
    }

    /// Look up a name for writing (assignment). Returns the binding's type on success.
    /// Errors:
    ///   - E0003 if the name is not in scope
    ///   - E0006 if the binding is immutable (`let` or parameter)
    pub fn lookup_for_write(&self, name: &str, span: &Span) -> Result<InferType, MoonlaneError> {
        match self.mono_env.iter().rev().find_map(|scope| scope.get(name)) {
            None => Err(MoonlaneError::type_error(
                crate::error::TypeErrorCode::T0003,
                format!("use of undeclared variable `{name}`"),
                span,
            )),
            Some((_, false)) => Err(MoonlaneError::type_error(
                crate::error::TypeErrorCode::T0006,
                format!("cannot assign to immutable binding `{name}`"),
                span,
            )),
            Some((ty, true)) => Ok(ty.clone()),
        }
    }

    /// Collect all type variables that appear free across all current mono scopes.
    /// Pass this to `generalize()` to avoid capturing variables still being solved.
    pub fn env_free_vars(&self) -> HashSet<TypeVar> {
        self.mono_env.iter()
            .flat_map(|scope| scope.values())
            .flat_map(|(ty, _)| free_vars(ty))
            .collect()
    }

    /// Record that `lhs` and `rhs` must unify, tagged with its source location.
    pub fn add_constraint(&mut self, lhs: InferType, rhs: InferType, span: Span) {
        self.constraints.push(Constraint::new(lhs, rhs, span));
    }

    /// Solve all accumulated constraints and return the resulting substitution.
    pub fn solve(&self) -> Result<Substitution, MoonlaneError> {
        solve_constraints(self.constraints.clone())
    }

    /// Set the expected return type for the current function, returning the previous value.
    /// Call `pop_return_type` with the returned value to restore on function exit.
    pub fn push_return_type(&mut self, ty: InferType) -> Option<InferType> {
        std::mem::replace(&mut self.current_return_type, Some(ty))
    }

    /// Restore the return type context after leaving a function body.
    pub fn pop_return_type(&mut self, prev: Option<InferType>) {
        self.current_return_type = prev;
    }

    /// The expected return type of the innermost enclosing function, if any.
    pub fn current_return_type(&self) -> Option<&InferType> {
        self.current_return_type.as_ref()
    }

    pub fn push_break_type(&mut self, ty: InferType) -> Option<InferType> {
        std::mem::replace(&mut self.current_break_type, Some(ty))
    }

    pub fn pop_break_type(&mut self, prev: Option<InferType>) {
        self.current_break_type = prev;
    }

    pub fn current_break_type(&self) -> Option<&InferType> {
        self.current_break_type.as_ref()
    }
}

impl Default for InferContext {
    fn default() -> Self {
        Self::new(TypeRegistry::new(), TypeVarGenerator::new())
    }
}
