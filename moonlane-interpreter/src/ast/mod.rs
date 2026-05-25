use crate::parser::Rule;

// ── Span ──────────────────────────────────────────────────────────────────────

/// Source location (byte offsets + resolved line/col into the original source string).
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub filename: String,
    pub line: u32,
    pub col: u32,
}

impl Span {
    pub fn new(start: usize, end: usize, filename: impl Into<String>) -> Self {
        Self { start, end, filename: filename.into(), line: 0, col: 0 }
    }
}

impl Span {
    pub fn of(pair: &pest::iterators::Pair<Rule>, filename: impl Into<String>) -> Self {
        let s = pair.as_span();
        let (line, col) = s.start_pos().line_col();
        Span { start: s.start(), end: s.end(), filename: filename.into(), line: line as u32, col: col as u32 }
    }
}

// ── Top-level ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Program {
    pub decls: Vec<Decl>,
}

impl Program {
    pub fn new(decls: Vec<Decl>) -> Self {
        Program { decls }
    }
}

// ── Declarations ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Decl {
    Let(LetDecl),
    Mut(MutDecl),
    Fun(FunDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Impl(ImplBlock),
    Aspect(AspectDecl),
    Stmt(Stmt),
}

#[derive(Debug, Clone)]
pub struct LetDecl {
    pub name:     String,
    pub type_ann: Option<TypeExpr>,
    pub value:    Expr,
    pub span:     Span,
}



#[derive(Debug, Clone)]
pub struct MutDecl {
    pub name:     String,
    pub type_ann: Option<TypeExpr>,
    pub value:    Expr,
    pub span:     Span,
}




#[derive(Debug, Clone)]
pub struct FunDecl {
    pub name:        String,
    pub generics:    Vec<GenericParam>,
    pub params:      Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body:        Block,
    pub span:        Span,
}


#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name:     String,
    pub generics: Vec<GenericParam>,
    pub fields:   Vec<FieldDef>,
    pub span:     Span,
}


#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name:     String,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<VariantDef>,
    pub span:     Span,
}


#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub aspect_name:      Option<String>,
    pub aspect_type_args: Vec<TypeExpr>,
    pub target_type:      TypeExpr,
    pub methods:          Vec<FunDecl>,
    pub span:             Span,
}

#[derive(Debug, Clone)]
pub struct AspectDecl {
    pub name:     String,
    pub generics: Vec<String>,
    pub methods:  Vec<AspectMethod>,
    pub span:     Span,
}


// ── Supporting types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name:  String,
    pub bound: Option<TypeExpr>,
}



#[derive(Debug, Clone)]
pub struct Param {
    pub mutable:  bool,
    pub name:     String,
    pub type_ann: Option<TypeExpr>,
    pub span:     Span,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name:     String,
    pub type_ann: TypeExpr,
    pub span:     Span,
}


#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name:   String,
    pub fields: Vec<FieldDef>,
    pub span:   Span,
}

#[derive(Debug, Clone)]
pub struct AspectMethod {
    pub name:         String,
    pub generics:     Vec<GenericParam>,
    pub params:       Vec<Param>,
    pub return_type:  Option<TypeExpr>,
    pub default_body: Option<Block>,
    pub span:         Span,
}

// ── Block ─────────────────────────────────────────────────────────────────────

/// `{ decl* expr? }` — the `tail` expression is the block's value when used in
/// expression position (if-expr, loop-expr, closure body, etc.).
#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Decl>,
    pub tail:  Option<Box<Expr>>,
    pub span:  Span,
}



// ── Statements ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    While(WhileStmt),
    For(ForStmt),
    ForIn(ForInStmt),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(Span),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body:      Block,
    pub span:      Span,
}


#[derive(Debug, Clone)]
pub struct ForStmt {
    pub init:      Option<ForInit>,
    pub condition: Option<Expr>,
    pub step:      Option<Expr>,
    pub body:      Block,
    pub span:      Span,
}

#[derive(Debug, Clone)]
pub enum ForInit {
    Mut(MutDecl),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct ForInStmt {
    pub binding:  String,
    pub iterable: Expr,
    pub body:     Block,
    pub span:     Span,
}





#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span:  Span,
}

#[derive(Debug, Clone)]
pub struct BreakStmt {
    pub value: Option<Expr>,
    pub span:  Span,
}

// ── Expressions ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal, Span),
    Ident(String, Span),
    Path(Vec<String>, Span),
    Tuple(Vec<Expr>, Span),
    Array(Vec<Expr>, Span),
    BinOp(Box<Expr>, BinOp, Box<Expr>, Span),
    UnaryOp(UnaryOp, Box<Expr>, Span),
    Assign { target: AssignTarget, op: AssignOp, value: Box<Expr>, span: Span },
    Call { callee: Box<Expr>, args: Vec<Expr>, span: Span },
    MethodCall { receiver: Box<Expr>, method: String, args: Vec<Expr>, span: Span },
    FieldAccess { object: Box<Expr>, field: String, span: Span },
    TupleAccess { object: Box<Expr>, index: usize, span: Span },
    Index { object: Box<Expr>, index: Box<Expr>, span: Span },
    Cast { expr: Box<Expr>, target_type: TypeExpr, span: Span },
    Ascribe { expr: Box<Expr>, ann: TypeExpr, span: Span },
    Match(MatchExpr),
    If { condition: Box<Expr>, then_branch: Block, else_branch: Option<Block>, span: Span },
    Loop { body: Block, span: Span },
    Closure { params: Vec<Param>, return_type: Option<TypeExpr>, body: Block, span: Span },
    StructLiteral { path: Vec<String>, fields: Vec<(String, Expr)>, span: Span },
    PropagateError { expr: Box<Expr>, span: Span },
}

impl Expr {
    pub fn span(&self) -> &Span {
        match self {
            Expr::Literal(_, s) | Expr::Ident(_, s) | Expr::Path(_, s)
            | Expr::Tuple(_, s) | Expr::Array(_, s) | Expr::BinOp(_, _, _, s)
            | Expr::UnaryOp(_, _, s)
            | Expr::Assign    { span: s, .. } | Expr::Call          { span: s, .. }
            | Expr::MethodCall { span: s, .. } | Expr::FieldAccess  { span: s, .. }
            | Expr::TupleAccess { span: s, .. } | Expr::Index       { span: s, .. }
            | Expr::Cast      { span: s, .. } | Expr::Ascribe       { span: s, .. }
            | Expr::If            { span: s, .. }
            | Expr::Loop      { span: s, .. } | Expr::Closure       { span: s, .. }
            | Expr::StructLiteral { span: s, .. } | Expr::PropagateError { span: s, .. } => s,
            Expr::Match(m) => &m.span,
        }
    }
}



// ── Match ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub scrutinee: Box<Expr>,
    pub arms:      Vec<MatchArm>,
    pub span:      Span,
}


#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard:   Option<Expr>,
    pub body:    Block,
    pub span:    Span,
}

// ── Patterns ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard(Span),
    Nope(Span),
    Literal(Literal, Span),
    Binding(String, Span),
    EnumVariant { path: Vec<String>, fields: Vec<String>, span: Span },
    Tuple(Vec<Pattern>, Span),
}


// ── Operators ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Rem,
    Eq, Ne, Lt, Le, Gt, Ge,
    And, Or,
    Range, RangeInclusive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp { Neg, Not }

#[derive(Debug, Clone, PartialEq)]
pub enum AssignOp {
    Assign,
    AddAssign, SubAssign, MulAssign, DivAssign, RemAssign,
}

// ── Assignment targets ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AssignTarget {
    Ident(String, Span),
    FieldAccess { object: Box<Expr>, field: String, span: Span },
    Index { object: Box<Expr>, index: Box<Expr>, span: Span },
}


// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named(String, Vec<TypeExpr>),
    Unit,
    Tuple(Vec<TypeExpr>),
    Array(Box<TypeExpr>),
    Fun(Vec<TypeExpr>, Option<Box<TypeExpr>>),
}

// ── Literals ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nope,
    Unit,
}

// ── String unescaping ─────────────────────────────────────────────────────────