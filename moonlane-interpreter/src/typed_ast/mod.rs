// ── Typed AST ─────────────────────────────────────────────────────────────────
// Mirrors the untyped AST but every expression node carries resolved type information.
// Generic declarations do not appear here — they are monomorphised by the type checker.

use crate::ast::{
    Literal, BinOp, UnaryOp, AssignTarget, AssignOp, Pattern, Span,
    Param, TypeExpr, FieldDef, GenericParam, AspectMethod, VariantDef, Block,
};
use crate::types::Type;

// ── Program ───────────────────────────────────────────────────────────────────

/// A fully typed program — list of typed declarations.
/// Used by the old single-module pipeline; kept for compatibility.
pub type TypedProgram = Vec<TypedDecl>;

/// A single typed module, produced by `check_graph`.
#[derive(Debug, Clone)]
pub struct TypedModule {
    pub module_path: Vec<String>,
    pub decls: Vec<TypedDecl>,
    /// Alias → canonical name for imports declared `import mod::name as alias`.
    /// The evaluator registers these so that `alias` resolves to the same value as `name`.
    pub import_aliases: std::collections::HashMap<String, String>,
}

/// The output of `check_graph` — one typed module per loaded module, in
/// topological order (dependencies before dependents).
#[derive(Debug, Clone)]
pub struct TypedModuleGraph {
    pub modules: Vec<TypedModule>,
}

// ── Typed Declarations ────────────────────────────────────────────────────────

/// Mirrors `ast::Decl` but with typed expressions.
/// Struct/Enum/Aspect variants carry no runtime data — the evaluator ignores them.
/// They exist for structural completeness and future tooling (reflection, docs, LSP).
#[derive(Debug, Clone)]
pub enum TypedDecl {
    Let(TypedLetDecl),
    Mut(TypedMutDecl),
    Fun(TypedFunDecl),
    Struct(#[allow(dead_code)] TypedStructDecl),
    Enum(#[allow(dead_code)] TypedEnumDecl),
    Impl(TypedImplBlock),
    Aspect(#[allow(dead_code)] TypedAspectDecl),
    Stmt(Box<TypedStmt>),
}

#[derive(Debug, Clone)]
pub struct TypedLetDecl {
    pub name:     String,
    #[allow(dead_code)] // kept for future tooling (hover types, LSP)
    pub type_ann: Option<TypeExpr>,
    pub value:    TypedExpr,
    #[allow(dead_code)] // kept for future error messages
    pub span:     Span,
}

#[derive(Debug, Clone)]
pub struct TypedMutDecl {
    pub name:     String,
    #[allow(dead_code)] // kept for future tooling (hover types, LSP)
    pub type_ann: Option<TypeExpr>,
    pub value:    TypedExpr,
    #[allow(dead_code)] // kept for future error messages
    pub span:     Span,
}

/// The body of a typed function declaration.
///
/// Monomorphic functions have a fully typed body; polymorphic functions
/// (those with quantified type variables) keep the original untyped AST body
/// because there is no single concrete instantiation to type-check against.
/// The evaluator uses runtime values, not type annotations, so this is safe.
#[derive(Debug, Clone)]
pub enum FunBody {
    Typed(TypedBlock),
    Generic(Block),
}

#[derive(Debug, Clone)]
pub struct TypedFunDecl {
    pub name:        String,
    #[allow(dead_code)] // kept for future reflection / documentation generation
    pub generics:    Vec<GenericParam>,
    pub params:      Vec<Param>,
    #[allow(dead_code)] // kept for future reflection / documentation generation
    pub return_type: Option<TypeExpr>,
    pub body:        FunBody,
    #[allow(dead_code)] // kept for future error messages
    pub span:        Span,
}

/// Carried in TypedDecl for structural completeness; the evaluator produces no
/// runtime representation for struct/enum declarations.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TypedStructDecl {
    pub name:     String,
    pub generics: Vec<GenericParam>,
    pub fields:   Vec<FieldDef>,
    pub span:     Span,
}

/// Carried in TypedDecl for structural completeness; the evaluator produces no
/// runtime representation for struct/enum declarations.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TypedEnumDecl {
    pub name:     String,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<VariantDef>,
    pub span:     Span,
}

#[derive(Debug, Clone)]
pub struct TypedImplBlock {
    pub aspect_name:      Option<String>,
    pub aspect_type_args: Vec<TypeExpr>,
    pub target_type:      TypeExpr,
    pub methods:          Vec<TypedFunDecl>,
    #[allow(dead_code)] // kept for future error messages
    pub span:             Span,
}

/// Carried in TypedDecl for structural completeness; the evaluator produces no
/// runtime representation for aspect declarations.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TypedAspectDecl {
    pub name:     String,
    pub generics: Vec<String>,
    pub methods:  Vec<AspectMethod>,
    pub span:     Span,
}

// ── Typed Statements ──────────────────────────────────────────────────────────

/// Mirrors `ast::Stmt` but with typed expressions.
#[derive(Debug, Clone)]
pub enum TypedStmt {
    While(TypedWhileStmt),
    For(Box<TypedForStmt>),
    ForIn(Box<TypedForInStmt>),
    Return(TypedReturnStmt),
    Break(TypedBreakStmt),
    Continue(#[allow(dead_code)] Span),
    Expr(TypedExpr),
}

#[derive(Debug, Clone)]
pub struct TypedWhileStmt {
    pub condition: TypedExpr,
    pub body:      TypedBlock,
    #[allow(dead_code)] // kept for future error messages
    pub span:      Span,
}

#[derive(Debug, Clone)]
pub struct TypedForStmt {
    pub init:      Option<TypedForInit>,
    pub condition: Option<TypedExpr>,
    pub step:      Option<TypedExpr>,
    pub body:      TypedBlock,
    #[allow(dead_code)] // kept for future error messages
    pub span:      Span,
}

#[derive(Debug, Clone)]
pub enum TypedForInit {
    Mut(TypedMutDecl),
    Expr(TypedExpr),
}

#[derive(Debug, Clone)]
pub struct TypedForInStmt {
    pub binding:  String,
    pub iterable: TypedExpr,
    pub body:     TypedBlock,
    pub span:     Span,
}

#[derive(Debug, Clone)]
pub struct TypedReturnStmt {
    pub value: Option<TypedExpr>,
    #[allow(dead_code)] // kept for future error messages
    pub span:  Span,
}

#[derive(Debug, Clone)]
pub struct TypedBreakStmt {
    pub value: Option<TypedExpr>,
    #[allow(dead_code)] // kept for future error messages
    pub span:  Span,
}

// ── Typed Block ───────────────────────────────────────────────────────────────

/// `{ decl* expr? }` — mirrors `ast::Block` with typed contents.
#[derive(Debug, Clone)]
pub struct TypedBlock {
    pub stmts: Vec<TypedDecl>,
    pub tail:  Option<Box<TypedExpr>>,
    #[allow(dead_code)] // kept for future error messages
    pub span:  Span,
}

// ── Typed Expressions ─────────────────────────────────────────────────────────

/// Mirrors `ast::Expr` but every variant includes a `Type` field.
/// This is the central type: after type inference, every expression is annotated with its type.
#[derive(Debug, Clone)]
pub enum TypedExpr {
    Literal(Literal, Type, Span),
    Ident(String, Type, Span),
    Path(Vec<String>, Type, Span),
    Tuple(Vec<TypedExpr>, Type, Span),
    Array(Vec<TypedExpr>, Type, Span),
    BinOp(Box<TypedExpr>, BinOp, Box<TypedExpr>, Type, Span),
    UnaryOp(UnaryOp, Box<TypedExpr>, Type, Span),
    Assign {
        target: AssignTarget,
        op: AssignOp,
        value: Box<TypedExpr>,
        ty: Type,
        span: Span,
    },
    Call {
        callee: Box<TypedExpr>,
        args: Vec<TypedExpr>,
        ty: Type,
        span: Span,
    },
    MethodCall {
        receiver: Box<TypedExpr>,
        method: String,
        args: Vec<TypedExpr>,
        ty: Type,
        span: Span,
    },
    FieldAccess {
        object: Box<TypedExpr>,
        field: String,
        ty: Type,
        span: Span,
    },
    TupleAccess {
        object: Box<TypedExpr>,
        index: usize,
        ty: Type,
        span: Span,
    },
    Index {
        object: Box<TypedExpr>,
        index: Box<TypedExpr>,
        ty: Type,
        span: Span,
    },
    Cast {
        expr: Box<TypedExpr>,
        target_type: TypeExpr,
        ty: Type,
        span: Span,
    },
    Match(TypedMatchExpr),
    If {
        condition: Box<TypedExpr>,
        then_branch: TypedBlock,
        else_branch: Option<TypedBlock>,
        ty: Type,
        span: Span,
    },
    Loop {
        body: TypedBlock,
        ty: Type,
        span: Span,
    },
    Closure {
        params: Vec<Param>,
        #[allow(dead_code)] // kept for future type annotation checking
        return_type: Option<TypeExpr>,
        body: TypedBlock,
        ty: Type,
        span: Span,
    },
    /// A let-bound polymorphic closure (let-polymorphism).
    /// The body is kept untyped for runtime re-evaluation at each call site's concrete type.
    GenericClosure {
        params:      Vec<Param>,
        #[allow(dead_code)] // kept for future type annotation checking
        return_type: Option<TypeExpr>,
        body:        Block,
        ty:          Type,
        span:        Span,
    },
    StructLiteral {
        path: Vec<String>,
        fields: Vec<(String, TypedExpr)>,
        ty: Type,
        span: Span,
    },
    PropagateError {
        expr:      Box<TypedExpr>,
        /// When E1 != E2, holds the env key for the coercion function (e.g. "AppError::from").
        coercion:  Option<String>,
        ty:        Type,
        span:      Span,
    },
}

impl TypedExpr {
    /// Convenience method to get the type of this expression.
    pub fn ty(&self) -> &Type {
        match self {
            TypedExpr::Literal(_, ty, _)
            | TypedExpr::Ident(_, ty, _)
            | TypedExpr::Path(_, ty, _)
            | TypedExpr::Tuple(_, ty, _)
            | TypedExpr::Array(_, ty, _)
            | TypedExpr::BinOp(_, _, _, ty, _)
            | TypedExpr::UnaryOp(_, _, ty, _) => ty,
            TypedExpr::Assign { ty, .. }
            | TypedExpr::Call { ty, .. }
            | TypedExpr::MethodCall { ty, .. }
            | TypedExpr::FieldAccess { ty, .. }
            | TypedExpr::TupleAccess { ty, .. }
            | TypedExpr::Index { ty, .. }
            | TypedExpr::Cast { ty, .. }
            | TypedExpr::If { ty, .. }
            | TypedExpr::Loop { ty, .. }
            | TypedExpr::Closure { ty, .. }
            | TypedExpr::GenericClosure { ty, .. }
            | TypedExpr::StructLiteral { ty, .. }
            | TypedExpr::PropagateError { ty, .. } => ty,
            TypedExpr::Match(m) => &m.expr_type,
        }
    }

    /// Convenience method to get the span of this expression.
    pub fn span(&self) -> &Span {
        match self {
            TypedExpr::Literal(_, _, s)
            | TypedExpr::Ident(_, _, s)
            | TypedExpr::Path(_, _, s)
            | TypedExpr::Tuple(_, _, s)
            | TypedExpr::Array(_, _, s)
            | TypedExpr::BinOp(_, _, _, _, s)
            | TypedExpr::UnaryOp(_, _, _, s) => s,
            TypedExpr::Assign { span, .. }
            | TypedExpr::Call { span, .. }
            | TypedExpr::MethodCall { span, .. }
            | TypedExpr::FieldAccess { span, .. }
            | TypedExpr::TupleAccess { span, .. }
            | TypedExpr::Index { span, .. }
            | TypedExpr::Cast { span, .. }
            | TypedExpr::If { span, .. }
            | TypedExpr::Loop { span, .. }
            | TypedExpr::Closure { span, .. }
            | TypedExpr::GenericClosure { span, .. }
            | TypedExpr::StructLiteral { span, .. }
            | TypedExpr::PropagateError { span, .. } => span,
            TypedExpr::Match(m) => &m.span,
        }
    }
}

// ── Typed Match ───────────────────────────────────────────────────────────────

/// Mirrors `ast::MatchExpr` with typed expressions.
#[derive(Debug, Clone)]
pub struct TypedMatchExpr {
    pub scrutinee: Box<TypedExpr>,
    pub arms: Vec<TypedMatchArm>,
    pub expr_type: Type,  // The type of the entire match expression
    pub span: Span,
}

/// Mirrors `ast::MatchArm` with typed expressions.
#[derive(Debug, Clone)]
pub struct TypedMatchArm {
    pub pattern: Pattern,  // Patterns don't contain expressions, reuse as-is
    pub guard: Option<TypedExpr>,
    pub body: TypedBlock,
    #[allow(dead_code)] // kept for future error messages
    pub span: Span,
}
