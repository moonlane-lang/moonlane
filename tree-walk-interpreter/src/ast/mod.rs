use from_pest::{ConversionError, FromPest, Void};
use pest::iterators::Pairs;

use crate::parser::Rule;

// ── Span ──────────────────────────────────────────────────────────────────────

/// Byte-offset source location, carried through the AST for error reporting.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end:   usize,
}

impl Span {
    fn of(pair: &pest::iterators::Pair<Rule>) -> Self {
        let s = pair.as_span();
        Span { start: s.start(), end: s.end() }
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

impl<'pest> FromPest<'pest> for Program {
    type Rule = Rule;
    type FatalError = Void;

    fn from_pest(pairs: &mut Pairs<'pest, Rule>) -> Result<Self, ConversionError<Void>> {
        let program_pair = pairs.next().ok_or(ConversionError::NoMatch)?;
        if program_pair.as_rule() != Rule::program {
            return Err(ConversionError::NoMatch);
        }
        let mut decls = Vec::new();
        for pair in program_pair.into_inner() {
            match pair.as_rule() {
                Rule::decl => {
                    decls.push(parse_decl(pair)?);
                }
                Rule::EOI => {}
                _ => {}
            }
        }
        Ok(Program { decls: decls })
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
    Trait(TraitDecl),
    Stmt(Stmt),
}

fn parse_decl(pair: pest::iterators::Pair<Rule>) -> Result<Decl, ConversionError<Void>> {
    // `decl` has exactly one child
    let inner = pair.into_inner().next().ok_or(ConversionError::NoMatch)?;
    match inner.as_rule() {
        Rule::let_decl    => Ok(Decl::Let(parse_let_decl(inner)?)),
        Rule::mut_decl    => Ok(Decl::Mut(parse_mut_decl(inner)?)),
        Rule::fun_decl    => Ok(Decl::Fun(parse_fun_decl(inner)?)),
        Rule::struct_decl => Ok(Decl::Struct(parse_struct_decl(inner)?)),
        Rule::enum_decl   => Ok(Decl::Enum(parse_enum_decl(inner)?)),
        Rule::impl_block  => Ok(Decl::Impl(parse_impl_block(inner)?)),
        Rule::trait_decl  => Ok(Decl::Trait(parse_trait_decl(inner)?)),
        Rule::stmt        => Ok(Decl::Stmt(parse_stmt(inner)?)),
        _ => Err(ConversionError::NoMatch),
    }
}

#[derive(Debug, Clone)]
pub struct LetDecl {
    pub name:     String,
    pub type_ann: Option<TypeExpr>,
    pub value:    Expr,
    pub span:     Span,
}

fn parse_let_decl(pair: pest::iterators::Pair<Rule>) -> Result<LetDecl, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let (type_ann, value) = parse_opt_type_then_expr(&mut inner)?;
    Ok(LetDecl { name, type_ann, value, span })
}

#[derive(Debug, Clone)]
pub struct MutDecl {
    pub name:     String,
    pub type_ann: Option<TypeExpr>,
    pub value:    Expr,
    pub span:     Span,
}

fn parse_mut_decl(pair: pest::iterators::Pair<Rule>) -> Result<MutDecl, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let (type_ann, value) = parse_opt_type_then_expr(&mut inner)?;
    Ok(MutDecl { name, type_ann, value, span })
}

/// Shared helper: parse `(":" type_expr)? expr` from a pair iterator.
fn parse_opt_type_then_expr(
    inner: &mut pest::iterators::Pairs<Rule>,
) -> Result<(Option<TypeExpr>, Expr), ConversionError<Void>> {
    let next = inner.next().ok_or(ConversionError::NoMatch)?;
    match next.as_rule() {
        Rule::type_expr => {
            let type_ann = Some(parse_type_expr(next)?);
            let expr_pair = inner.next().ok_or(ConversionError::NoMatch)?;
            let value = parse_expr(expr_pair)?;
            Ok((type_ann, value))
        }
        Rule::expr => Ok((None, parse_expr(next)?)),
        _ => Err(ConversionError::NoMatch),
    }
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

fn parse_fun_decl(pair: pest::iterators::Pair<Rule>) -> Result<FunDecl, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let mut generics    = vec![];
    let mut params      = vec![];
    let mut return_type = None;
    let mut body        = None;
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => generics = parse_generic_params(p)?,
            Rule::param_list     => params   = parse_param_list(p)?,
            Rule::type_expr      => return_type = Some(parse_type_expr(p)?),
            Rule::block          => body = Some(parse_block(p)?),
            _ => {}
        }
    }
    Ok(FunDecl { name, generics, params, return_type, body: body.ok_or(ConversionError::NoMatch)?, span })
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name:     String,
    pub generics: Vec<GenericParam>,
    pub fields:   Vec<FieldDef>,
    pub span:     Span,
}

fn parse_struct_decl(pair: pest::iterators::Pair<Rule>) -> Result<StructDecl, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let mut generics = vec![];
    let mut fields   = vec![];
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => generics = parse_generic_params(p)?,
            Rule::struct_fields  => fields   = parse_struct_fields(p)?,
            _ => {}
        }
    }
    Ok(StructDecl { name, generics, fields, span })
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name:     String,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<VariantDef>,
    pub span:     Span,
}

fn parse_enum_decl(pair: pest::iterators::Pair<Rule>) -> Result<EnumDecl, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let mut generics = vec![];
    let mut variants = vec![];
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => generics = parse_generic_params(p)?,
            Rule::enum_variants  => {
                for v in p.into_inner() {
                    if v.as_rule() == Rule::enum_variant {
                        variants.push(parse_enum_variant(v)?);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(EnumDecl { name, generics, variants, span })
}

#[derive(Debug, Clone)]
pub struct ImplBlock {
    pub trait_name:  Option<String>,
    pub target_type: TypeExpr,
    pub methods:     Vec<FunDecl>,
    pub span:        Span,
}

fn parse_impl_block(pair: pest::iterators::Pair<Rule>) -> Result<ImplBlock, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner       = pair.into_inner();
    let mut trait_name  = None;
    let mut target_type = None;
    let mut methods     = vec![];

    // Grammar: "impl" ~ (type_path ~ "for")? ~ type_expr ~ "{" ~ fun_decl* ~ "}"
    // Children: optionally [type_path, type_expr], or just [type_expr], then fun_decls.
    // type_path and type_expr both start with ident, so we peek at the sequence.
    let mut collected: Vec<pest::iterators::Pair<Rule>> = inner.collect();

    // Separate fun_decls from the front (type_path / type_expr) pairs
    let fun_start = collected.iter().position(|p| p.as_rule() == Rule::fun_decl)
        .unwrap_or(collected.len());
    let type_pairs: Vec<_> = collected.drain(..fun_start).collect();
    let fun_pairs = collected;

    match type_pairs.len() {
        0 => return Err(ConversionError::NoMatch),
        1 => {
            // `impl Type { ... }`
            target_type = Some(parse_type_expr(type_pairs.into_iter().next().unwrap())?);
        }
        2 => {
            // `impl Trait for Type { ... }`
            let mut it = type_pairs.into_iter();
            let trait_pair = it.next().unwrap();
            // type_path is `ident ~ ("::" ~ ident)*`
            let path: Vec<String> = trait_pair.into_inner()
                .filter(|p| p.as_rule() == Rule::ident)
                .map(|p| p.as_str().to_string())
                .collect();
            trait_name = Some(path.join("::"));
            target_type = Some(parse_type_expr(it.next().unwrap())?);
        }
        _ => return Err(ConversionError::NoMatch),
    }

    for p in fun_pairs {
        if p.as_rule() == Rule::fun_decl {
            methods.push(parse_fun_decl(p)?);
        }
    }

    Ok(ImplBlock { trait_name, target_type: target_type.unwrap(), methods, span })
}

#[derive(Debug, Clone)]
pub struct TraitDecl {
    pub name:    String,
    pub methods: Vec<TraitMethod>,
    pub span:    Span,
}

fn parse_trait_decl(pair: pest::iterators::Pair<Rule>) -> Result<TraitDecl, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let mut methods = vec![];
    for p in inner {
        if p.as_rule() == Rule::trait_method {
            methods.push(parse_trait_method(p)?);
        }
    }
    Ok(TraitDecl { name, methods, span })
}

// ── Supporting types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name:  String,
    pub bound: Option<TypeExpr>,
}

fn parse_generic_params(pair: pest::iterators::Pair<Rule>) -> Result<Vec<GenericParam>, ConversionError<Void>> {
    let mut params = vec![];
    for p in pair.into_inner() {
        if p.as_rule() == Rule::generic_param {
            let mut it = p.into_inner();
            let name = it.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
            let bound = it.next().map(parse_type_expr).transpose()?;
            params.push(GenericParam { name, bound });
        }
    }
    Ok(params)
}

#[derive(Debug, Clone)]
pub struct Param {
    pub mutable:  bool,
    pub name:     String,
    pub type_ann: Option<TypeExpr>,
    pub span:     Span,
}

fn parse_param_list(pair: pest::iterators::Pair<Rule>) -> Result<Vec<Param>, ConversionError<Void>> {
    let mut params = vec![];
    for p in pair.into_inner() {
        if p.as_rule() == Rule::param {
            params.push(parse_param(p)?);
        }
    }
    Ok(params)
}

fn parse_param(pair: pest::iterators::Pair<Rule>) -> Result<Param, ConversionError<Void>> {
    let span = Span::of(&pair);
    let text = pair.as_str().trim();
    if text == "self" {
        return Ok(Param { mutable: false, name: "self".into(), type_ann: None, span });
    }
    if text == "mut self" {
        return Ok(Param { mutable: true, name: "self".into(), type_ann: None, span });
    }
    // ident ":" type_expr
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let type_ann = inner.next().map(parse_type_expr).transpose()?;
    Ok(Param { mutable: false, name, type_ann, span })
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name:     String,
    pub type_ann: TypeExpr,
    pub span:     Span,
}

fn parse_struct_fields(pair: pest::iterators::Pair<Rule>) -> Result<Vec<FieldDef>, ConversionError<Void>> {
    let mut fields = vec![];
    for p in pair.into_inner() {
        if p.as_rule() == Rule::struct_field {
            let span = Span::of(&p);
            let mut it = p.into_inner();
            let name = it.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
            let type_ann = parse_type_expr(it.next().ok_or(ConversionError::NoMatch)?)?;
            fields.push(FieldDef { name, type_ann, span });
        }
    }
    Ok(fields)
}

#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name:   String,
    pub fields: Vec<FieldDef>,
    pub span:   Span,
}

fn parse_enum_variant(pair: pest::iterators::Pair<Rule>) -> Result<VariantDef, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let mut fields = vec![];
    for p in inner {
        if p.as_rule() == Rule::struct_fields {
            fields = parse_struct_fields(p)?;
        }
    }
    Ok(VariantDef { name, fields, span })
}

#[derive(Debug, Clone)]
pub struct TraitMethod {
    pub name:         String,
    pub generics:     Vec<GenericParam>,
    pub params:       Vec<Param>,
    pub return_type:  Option<TypeExpr>,
    pub default_body: Option<Block>,
    pub span:         Span,
}

fn parse_trait_method(pair: pest::iterators::Pair<Rule>) -> Result<TraitMethod, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner       = pair.into_inner();
    let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let mut generics    = vec![];
    let mut params      = vec![];
    let mut return_type = None;
    let mut default_body = None;
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => generics     = parse_generic_params(p)?,
            Rule::param_list     => params       = parse_param_list(p)?,
            Rule::type_expr      => return_type  = Some(parse_type_expr(p)?),
            Rule::block          => default_body = Some(parse_block(p)?),
            _ => {}
        }
    }
    Ok(TraitMethod { name, generics, params, return_type, default_body, span })
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

fn parse_block(pair: pest::iterators::Pair<Rule>) -> Result<Block, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut stmts = vec![];
    let mut tail  = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::decl => stmts.push(parse_decl(p)?),
            Rule::expr => tail = Some(Box::new(parse_expr(p)?)),
            _ => {}
        }
    }
    Ok(Block { stmts, tail, span })
}

// ── Statements ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Stmt {
    If(IfStmt),
    While(WhileStmt),
    For(ForStmt),
    ForIn(ForInStmt),
    Loop(LoopStmt),
    Match(MatchExpr),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(Span),
    Expr(Expr),
}

fn parse_stmt(pair: pest::iterators::Pair<Rule>) -> Result<Stmt, ConversionError<Void>> {
    let inner = pair.into_inner().next().ok_or(ConversionError::NoMatch)?;
    match inner.as_rule() {
        Rule::if_stmt      => Ok(Stmt::If(parse_if_stmt(inner)?)),
        Rule::while_stmt   => Ok(Stmt::While(parse_while_stmt(inner)?)),
        Rule::for_stmt     => Ok(Stmt::For(parse_for_stmt(inner)?)),
        Rule::for_in_stmt  => Ok(Stmt::ForIn(parse_for_in_stmt(inner)?)),
        Rule::loop_stmt    => Ok(Stmt::Loop(parse_loop_stmt(inner)?)),
        Rule::match_stmt   => Ok(Stmt::Match(parse_match_expr(inner)?)),
        Rule::return_stmt  => Ok(Stmt::Return(parse_return_stmt(inner)?)),
        Rule::break_stmt   => Ok(Stmt::Break(parse_break_stmt(inner)?)),
        Rule::continue_stmt => Ok(Stmt::Continue(Span::of(&inner))),
        Rule::expr_stmt    => {
            let expr_pair = inner.into_inner().next().ok_or(ConversionError::NoMatch)?;
            Ok(Stmt::Expr(parse_expr(expr_pair)?))
        }
        _ => Err(ConversionError::NoMatch),
    }
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition:   Expr,
    pub then_branch: Block,
    pub else_branch: Option<ElseBranch>,
    pub span:        Span,
}

#[derive(Debug, Clone)]
pub enum ElseBranch {
    Block(Block),
    If(Box<IfStmt>),
}

fn parse_if_stmt(pair: pest::iterators::Pair<Rule>) -> Result<IfStmt, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let condition   = parse_expr(inner.next().ok_or(ConversionError::NoMatch)?)?;
    let then_branch = parse_block(inner.next().ok_or(ConversionError::NoMatch)?)?;
    let else_branch = match inner.next() {
        Some(p) => Some(match p.as_rule() {
            Rule::if_stmt => ElseBranch::If(Box::new(parse_if_stmt(p)?)),
            Rule::block   => ElseBranch::Block(parse_block(p)?),
            _ => return Err(ConversionError::NoMatch),
        }),
        None => None,
    };
    Ok(IfStmt { condition, then_branch, else_branch, span })
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body:      Block,
    pub span:      Span,
}

fn parse_while_stmt(pair: pest::iterators::Pair<Rule>) -> Result<WhileStmt, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let condition = parse_expr(inner.next().ok_or(ConversionError::NoMatch)?)?;
    let body      = parse_block(inner.next().ok_or(ConversionError::NoMatch)?)?;
    Ok(WhileStmt { condition, body, span })
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

fn parse_for_stmt(pair: pest::iterators::Pair<Rule>) -> Result<ForStmt, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();

    // for_init
    let init_pair = inner.next().ok_or(ConversionError::NoMatch)?;
    let init = if init_pair.as_rule() == Rule::for_init {
        match init_pair.into_inner().next() {
            Some(p) => match p.as_rule() {
                Rule::mut_decl  => Some(ForInit::Mut(parse_mut_decl(p)?)),
                Rule::expr_stmt => {
                    let ep = p.into_inner().next().ok_or(ConversionError::NoMatch)?;
                    Some(ForInit::Expr(parse_expr(ep)?))
                }
                _ => None, // bare ";"
            },
            None => None,
        }
    } else {
        None
    };

    // condition and step are optional `expr` pairs; body is a `block`
    let mut condition = None;
    let mut step      = None;
    let mut body      = None;
    for p in inner {
        match p.as_rule() {
            Rule::expr  => if condition.is_none() { condition = Some(parse_expr(p)?); }
                           else                   { step      = Some(parse_expr(p)?); }
            Rule::block => body = Some(parse_block(p)?),
            _ => {}
        }
    }
    Ok(ForStmt { init, condition, step, body: body.ok_or(ConversionError::NoMatch)?, span })
}

#[derive(Debug, Clone)]
pub struct ForInStmt {
    pub binding:  String,
    pub iterable: Expr,
    pub body:     Block,
    pub span:     Span,
}

fn parse_for_in_stmt(pair: pest::iterators::Pair<Rule>) -> Result<ForInStmt, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let binding  = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
    let iterable = parse_expr(inner.next().ok_or(ConversionError::NoMatch)?)?;
    let body     = parse_block(inner.next().ok_or(ConversionError::NoMatch)?)?;
    Ok(ForInStmt { binding, iterable, body, span })
}

#[derive(Debug, Clone)]
pub struct LoopStmt {
    pub body: Block,
    pub span: Span,
}

fn parse_loop_stmt(pair: pest::iterators::Pair<Rule>) -> Result<LoopStmt, ConversionError<Void>> {
    let span = Span::of(&pair);
    let body = parse_block(pair.into_inner().next().ok_or(ConversionError::NoMatch)?)?;
    Ok(LoopStmt { body, span })
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span:  Span,
}

fn parse_return_stmt(pair: pest::iterators::Pair<Rule>) -> Result<ReturnStmt, ConversionError<Void>> {
    let span  = Span::of(&pair);
    let value = pair.into_inner().next().map(parse_expr).transpose()?;
    Ok(ReturnStmt { value, span })
}

#[derive(Debug, Clone)]
pub struct BreakStmt {
    pub value: Option<Expr>,
    pub span:  Span,
}

fn parse_break_stmt(pair: pest::iterators::Pair<Rule>) -> Result<BreakStmt, ConversionError<Void>> {
    let span  = Span::of(&pair);
    let value = pair.into_inner().next().map(parse_expr).transpose()?;
    Ok(BreakStmt { value, span })
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
    Match(MatchExpr),
    If { condition: Box<Expr>, then_branch: Block, else_branch: Block, span: Span },
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
            | Expr::Cast      { span: s, .. } | Expr::If            { span: s, .. }
            | Expr::Loop      { span: s, .. } | Expr::Closure       { span: s, .. }
            | Expr::StructLiteral { span: s, .. } | Expr::PropagateError { span: s, .. } => s,
            Expr::Match(m) => &m.span,
        }
    }
}

/// Entry point: consumes one `expr` pair.
fn parse_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    match pair.as_rule() {
        Rule::expr => {
            let inner = pair.into_inner().next().ok_or(ConversionError::NoMatch)?;
            parse_expr(inner)
        }
        Rule::assign_expr => parse_assign_expr(pair),
        Rule::or_expr     => parse_lr_binary(pair),
        Rule::and_expr    => parse_lr_binary(pair),
        Rule::cmp_expr    => parse_lr_binary(pair),
        Rule::range_expr  => parse_lr_binary(pair),
        Rule::add_expr    => parse_lr_binary(pair),
        Rule::mul_expr    => parse_lr_binary(pair),
        Rule::cast_expr   => parse_cast_expr(pair),
        Rule::unary_expr  => parse_unary_expr(pair),
        Rule::postfix_expr => parse_postfix_expr(pair),
        Rule::primary_expr => {
            let inner = pair.into_inner().next().ok_or(ConversionError::NoMatch)?;
            parse_expr(inner)
        }
        // Terminals and composites reachable from primary_expr
        Rule::int_lit | Rule::float_lit | Rule::string_lit
        | Rule::bool_lit | Rule::nope_lit | Rule::unit_lit => parse_literal_expr(pair),
        Rule::path_expr     => parse_path_expr(pair),
        Rule::tuple_or_paren => parse_tuple_or_paren(pair),
        Rule::array_lit     => parse_array_lit(pair),
        Rule::match_expr    => Ok(Expr::Match(parse_match_expr(pair)?)),
        Rule::match_stmt    => Ok(Expr::Match(parse_match_expr(pair)?)),
        Rule::if_expr       => parse_if_expr(pair),
        Rule::loop_expr     => parse_loop_expr(pair),
        Rule::closure_expr  => parse_closure_expr(pair),
        Rule::struct_literal => parse_struct_literal(pair),
        _ => Err(ConversionError::NoMatch),
    }
}

fn parse_literal_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let text = pair.as_str();
    let lit = match pair.as_rule() {
        Rule::int_lit    => Literal::Int(text.replace('_', "").parse().map_err(|_| ConversionError::NoMatch)?),
        Rule::float_lit  => Literal::Float(text.parse().map_err(|_| ConversionError::NoMatch)?),
        Rule::string_lit => Literal::Str(unescape(&text[1..text.len()-1])),
        Rule::bool_lit   => Literal::Bool(text == "true"),
        Rule::nope_lit   => Literal::Nope,
        Rule::unit_lit   => Literal::Unit,
        _ => return Err(ConversionError::NoMatch),
    };
    Ok(Expr::Literal(lit, span))
}

fn parse_path_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span  = Span::of(&pair);
    let parts: Vec<String> = pair.into_inner()
        .filter(|p| p.as_rule() == Rule::ident)
        .map(|p| p.as_str().to_string())
        .collect();
    if parts.len() == 1 {
        Ok(Expr::Ident(parts.into_iter().next().unwrap(), span))
    } else {
        Ok(Expr::Path(parts, span))
    }
}

fn parse_tuple_or_paren(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let elems: Vec<Expr> = pair.into_inner()
        .filter(|p| p.as_rule() == Rule::expr)
        .map(parse_expr)
        .collect::<Result<_, _>>()?;
    if elems.len() == 1 {
        Ok(elems.into_iter().next().unwrap())
    } else {
        Ok(Expr::Tuple(elems, span))
    }
}

fn parse_array_lit(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let elems = pair.into_inner()
        .filter(|p| p.as_rule() == Rule::expr)
        .map(parse_expr)
        .collect::<Result<_, _>>()?;
    Ok(Expr::Array(elems, span))
}

fn parse_if_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let condition   = parse_expr(inner.next().ok_or(ConversionError::NoMatch)?)?;
    let then_branch = parse_block(inner.next().ok_or(ConversionError::NoMatch)?)?;
    let else_branch = parse_block(inner.next().ok_or(ConversionError::NoMatch)?)?;
    Ok(Expr::If { condition: Box::new(condition), then_branch, else_branch, span })
}

fn parse_loop_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let body = parse_block(pair.into_inner().next().ok_or(ConversionError::NoMatch)?)?;
    Ok(Expr::Loop { body, span })
}

fn parse_closure_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut params      = vec![];
    let mut return_type = None;
    let mut body        = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::param_list => params      = parse_param_list(p)?,
            Rule::type_expr  => return_type = Some(parse_type_expr(p)?),
            Rule::block      => body        = Some(parse_block(p)?),
            _ => {}
        }
    }
    Ok(Expr::Closure { params, return_type, body: body.ok_or(ConversionError::NoMatch)?, span })
}

fn parse_struct_literal(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let path_pair = inner.next().ok_or(ConversionError::NoMatch)?;
    let path: Vec<String> = path_pair.into_inner()
        .filter(|p| p.as_rule() == Rule::ident)
        .map(|p| p.as_str().to_string())
        .collect();
    let mut fields = vec![];
    for p in inner {
        if p.as_rule() == Rule::field_init {
            let mut it = p.into_inner();
            let name  = it.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
            let value = parse_expr(it.next().ok_or(ConversionError::NoMatch)?)?;
            fields.push((name, value));
        }
    }
    Ok(Expr::StructLiteral { path, fields, span })
}

// ── Assignment ────────────────────────────────────────────────────────────────

fn parse_assign_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span  = Span::of(&pair);
    let mut inner = pair.into_inner();
    let first = inner.next().ok_or(ConversionError::NoMatch)?;

    // assign_expr = { postfix_expr ~ assign_op ~ assign_expr | or_expr }
    // If first child is postfix_expr and next is assign_op, it's an assignment.
    // Otherwise it's an or_expr chain.
    match first.as_rule() {
        Rule::postfix_expr => {
            let lhs = parse_postfix_expr(first)?;
            match inner.next() {
                Some(op_pair) if op_pair.as_rule() == Rule::assign_op => {
                    let op     = parse_assign_op(op_pair.as_str());
                    let rhs    = parse_expr(inner.next().ok_or(ConversionError::NoMatch)?)?;
                    let target = expr_to_assign_target(lhs)?;
                    Ok(Expr::Assign { target, op, value: Box::new(rhs), span })
                }
                _ => Ok(lhs), // shouldn't happen with valid grammar
            }
        }
        Rule::or_expr => parse_lr_binary(first),
        _             => parse_expr(first),
    }
}

// ── Binary expressions (left-recursive) ──────────────────────────────────────

/// Handles or_expr, and_expr, cmp_expr, range_expr, add_expr, mul_expr.
/// All follow the pattern: operand (op operand)* where op is a named rule.
fn parse_lr_binary(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span  = Span::of(&pair);
    let mut inner = pair.into_inner();
    let first = inner.next().ok_or(ConversionError::NoMatch)?;
    let mut expr = parse_expr(first)?;

    // Consume op/operand pairs
    while let Some(op_pair) = inner.next() {
        let op      = parse_bin_op(&op_pair);
        let rhs_pair = inner.next().ok_or(ConversionError::NoMatch)?;
        let rhs     = parse_expr(rhs_pair)?;
        let op_span = Span::of(&op_pair);
        expr = Expr::BinOp(Box::new(expr), op, Box::new(rhs), op_span);
    }
    let _ = span; // span used in outer call if needed
    Ok(expr)
}

// ── Cast ──────────────────────────────────────────────────────────────────────

fn parse_cast_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span  = Span::of(&pair);
    let mut inner = pair.into_inner();
    let first = inner.next().ok_or(ConversionError::NoMatch)?;
    let mut expr = parse_expr(first)?;
    for p in inner {
        if p.as_rule() == Rule::type_expr {
            let target_type = parse_type_expr(p)?;
            expr = Expr::Cast { expr: Box::new(expr), target_type, span: span.clone() };
        }
    }
    Ok(expr)
}

// ── Unary ─────────────────────────────────────────────────────────────────────

fn parse_unary_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let text = pair.as_str();
    let child = pair.into_inner().next().ok_or(ConversionError::NoMatch)?;
    if text.starts_with('!') {
        Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(parse_expr(child)?), span))
    } else if text.starts_with('-') {
        Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(parse_expr(child)?), span))
    } else {
        parse_expr(child)
    }
}

// ── Postfix ───────────────────────────────────────────────────────────────────

fn parse_postfix_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let mut inner = pair.into_inner();
    let primary = inner.next().ok_or(ConversionError::NoMatch)?;
    let mut expr = parse_expr(primary)?;
    for postfix in inner {
        if postfix.as_rule() == Rule::postfix {
            expr = apply_postfix(expr, postfix)?;
        }
    }
    Ok(expr)
}

fn apply_postfix(base: Expr, pair: pest::iterators::Pair<Rule>) -> Result<Expr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let text = pair.as_str();
    let mut inner = pair.into_inner();

    if text.starts_with('(') {
        // Function call: postfix children are (arg_list?), so unwrap one level
        let args = match inner.next() {
            Some(a) if a.as_rule() == Rule::arg_list => collect_args(a.into_inner())?,
            _ => vec![],
        };
        Ok(Expr::Call { callee: Box::new(base), args, span })
    } else if text.starts_with('[') {
        // Index
        let idx = parse_expr(inner.next().ok_or(ConversionError::NoMatch)?)?;
        Ok(Expr::Index { object: Box::new(base), index: Box::new(idx), span })
    } else if text == "?" {
        Ok(Expr::PropagateError { expr: Box::new(base), span })
    } else {
        // Dot postfix — first named child is decimal_int or ident
        let first = inner.next().ok_or(ConversionError::NoMatch)?;
        match first.as_rule() {
            Rule::decimal_int => {
                let idx = first.as_str().parse::<usize>().map_err(|_| ConversionError::NoMatch)?;
                Ok(Expr::TupleAccess { object: Box::new(base), index: idx, span })
            }
            Rule::ident => {
                let name = first.as_str().to_string();
                // If a `(` follows in the text, it's a method call
                if text.contains('(') {
                    let args = match inner.next() {
                        Some(a) if a.as_rule() == Rule::arg_list => collect_args(a.into_inner())?,
                        _ => vec![],
                    };
                    Ok(Expr::MethodCall { receiver: Box::new(base), method: name, args, span })
                } else {
                    Ok(Expr::FieldAccess { object: Box::new(base), field: name, span })
                }
            }
            _ => Err(ConversionError::NoMatch),
        }
    }
}

fn collect_args(
    pairs: pest::iterators::Pairs<Rule>,
) -> Result<Vec<Expr>, ConversionError<Void>> {
    pairs.filter(|p| p.as_rule() == Rule::expr)
         .map(parse_expr)
         .collect()
}

// ── Match ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub scrutinee: Box<Expr>,
    pub arms:      Vec<MatchArm>,
    pub span:      Span,
}

fn parse_match_expr(pair: pest::iterators::Pair<Rule>) -> Result<MatchExpr, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let scrutinee = parse_expr(inner.next().ok_or(ConversionError::NoMatch)?)?;
    let arms: Vec<MatchArm> = inner
        .filter(|p| p.as_rule() == Rule::match_arm)
        .map(parse_match_arm)
        .collect::<Result<_, _>>()?;
    Ok(MatchExpr { scrutinee: Box::new(scrutinee), arms, span })
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard:   Option<Expr>,
    pub body:    Expr,
    pub span:    Span,
}

fn parse_match_arm(pair: pest::iterators::Pair<Rule>) -> Result<MatchArm, ConversionError<Void>> {
    let span = Span::of(&pair);
    let mut inner = pair.into_inner();
    let pattern = parse_pattern(inner.next().ok_or(ConversionError::NoMatch)?)?;

    // Remaining children: optionally a guard `expr`, then the body `expr`.
    let mut exprs: Vec<pest::iterators::Pair<Rule>> = inner
        .filter(|p| p.as_rule() == Rule::expr)
        .collect();

    let body  = parse_expr(exprs.pop().ok_or(ConversionError::NoMatch)?)?;
    let guard = exprs.into_iter().next().map(parse_expr).transpose()?;

    Ok(MatchArm { pattern, guard, body, span })
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

fn parse_pattern(pair: pest::iterators::Pair<Rule>) -> Result<Pattern, ConversionError<Void>> {
    match pair.as_rule() {
        Rule::pattern => {
            let inner = pair.into_inner().next().ok_or(ConversionError::NoMatch)?;
            parse_pattern(inner)
        }
        Rule::nope_lit => Ok(Pattern::Nope(Span::of(&pair))),
        Rule::tuple_pattern => {
            let span = Span::of(&pair);
            let pats = pair.into_inner()
                .filter(|p| p.as_rule() == Rule::pattern)
                .map(parse_pattern)
                .collect::<Result<_, _>>()?;
            Ok(Pattern::Tuple(pats, span))
        }
        Rule::enum_pattern => {
            let span = Span::of(&pair);
            let idents: Vec<String> = pair.into_inner()
                .filter(|p| p.as_rule() == Rule::ident)
                .map(|p| p.as_str().to_string())
                .collect();
            // First two idents are Type::Variant; rest are field bindings
            let (path, fields) = if idents.len() > 2 {
                let (p, f) = idents.split_at(2);
                (p.to_vec(), f.to_vec())
            } else {
                (idents, vec![])
            };
            Ok(Pattern::EnumVariant { path, fields, span })
        }
        Rule::literal_pattern => {
            let span = Span::of(&pair);
            let lit_pair = pair.into_inner().next().ok_or(ConversionError::NoMatch)?;
            let text = lit_pair.as_str();
            let lit = match lit_pair.as_rule() {
                Rule::float_lit  => Literal::Float(text.parse().map_err(|_| ConversionError::NoMatch)?),
                Rule::int_lit    => Literal::Int(text.replace('_', "").parse().map_err(|_| ConversionError::NoMatch)?),
                Rule::string_lit => Literal::Str(unescape(&text[1..text.len()-1])),
                Rule::bool_lit   => Literal::Bool(text == "true"),
                _ => return Err(ConversionError::NoMatch),
            };
            Ok(Pattern::Literal(lit, span))
        }
        Rule::bind_pattern => {
            let span = Span::of(&pair);
            let name = pair.into_inner().next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
            Ok(Pattern::Binding(name, span))
        }
        // Wildcard: the `"_" ~ !(...)` alternative in `pattern` is anonymous;
        // pest emits no sub-rule, so `pair.as_rule() == Rule::pattern` and
        // `pair.as_str() == "_"` — handled by the outer `pattern` arm above
        // which recurses into the single child. If there is no child and the
        // text is "_", we match here.
        _ if pair.as_str().trim() == "_" => Ok(Pattern::Wildcard(Span::of(&pair))),
        _ => Err(ConversionError::NoMatch),
    }
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

fn parse_bin_op(pair: &pest::iterators::Pair<Rule>) -> BinOp {
    match pair.as_rule() {
        Rule::add_op   => if pair.as_str() == "-" { BinOp::Sub } else { BinOp::Add },
        Rule::mul_op   => match pair.as_str() { "/" => BinOp::Div, "%" => BinOp::Rem, _ => BinOp::Mul },
        Rule::or_op    => BinOp::Or,
        Rule::and_op   => BinOp::And,
        Rule::range_op => if pair.as_str() == "..=" { BinOp::RangeInclusive } else { BinOp::Range },
        Rule::cmp_op   => match pair.as_str() {
            "==" => BinOp::Eq, "!=" => BinOp::Ne,
            "<=" => BinOp::Le, ">=" => BinOp::Ge,
            "<"  => BinOp::Lt, _    => BinOp::Gt,
        },
        _ => BinOp::Add, // fallback
    }
}

fn parse_assign_op(s: &str) -> AssignOp {
    match s {
        "+=" => AssignOp::AddAssign, "-=" => AssignOp::SubAssign,
        "*=" => AssignOp::MulAssign, "/=" => AssignOp::DivAssign,
        "%=" => AssignOp::RemAssign, _    => AssignOp::Assign,
    }
}

// ── Assignment targets ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AssignTarget {
    Ident(String, Span),
    FieldAccess { object: Box<Expr>, field: String, span: Span },
    Index { object: Box<Expr>, index: Box<Expr>, span: Span },
}

fn expr_to_assign_target(expr: Expr) -> Result<AssignTarget, ConversionError<Void>> {
    match expr {
        Expr::Ident(name, span) =>
            Ok(AssignTarget::Ident(name, span)),
        Expr::FieldAccess { object, field, span } =>
            Ok(AssignTarget::FieldAccess { object, field, span }),
        Expr::Index { object, index, span } =>
            Ok(AssignTarget::Index { object, index, span }),
        _ => Err(ConversionError::NoMatch),
    }
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

fn parse_type_expr(pair: pest::iterators::Pair<Rule>) -> Result<TypeExpr, ConversionError<Void>> {
    match pair.as_rule() {
        Rule::type_expr => {
            let inner = pair.into_inner().next().ok_or(ConversionError::NoMatch)?;
            parse_type_expr(inner)
        }
        Rule::unit_type  => Ok(TypeExpr::Unit),
        Rule::tuple_type => {
            let elems = pair.into_inner()
                .filter(|p| p.as_rule() == Rule::type_expr)
                .map(parse_type_expr)
                .collect::<Result<_, _>>()?;
            Ok(TypeExpr::Tuple(elems))
        }
        Rule::array_type => {
            let elem = parse_type_expr(pair.into_inner().next().ok_or(ConversionError::NoMatch)?)?;
            Ok(TypeExpr::Array(Box::new(elem)))
        }
        Rule::fun_type => {
            let mut params      = vec![];
            let mut return_type = None;
            for p in pair.into_inner() {
                match p.as_rule() {
                    Rule::type_list => {
                        params = p.into_inner()
                            .filter(|q| q.as_rule() == Rule::type_expr)
                            .map(parse_type_expr)
                            .collect::<Result<_, _>>()?;
                    }
                    Rule::type_expr => return_type = Some(Box::new(parse_type_expr(p)?)),
                    _ => {}
                }
            }
            Ok(TypeExpr::Fun(params, return_type))
        }
        Rule::named_type => {
            let mut inner = pair.into_inner();
            let name = inner.next().ok_or(ConversionError::NoMatch)?.as_str().to_string();
            let mut args = vec![];
            for p in inner {
                if p.as_rule() == Rule::type_args {
                    args = p.into_inner()
                        .filter(|q| q.as_rule() == Rule::type_expr)
                        .map(parse_type_expr)
                        .collect::<Result<_, _>>()?;
                }
            }
            Ok(TypeExpr::Named(name, args))
        }
        _ => Err(ConversionError::NoMatch),
    }
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

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n')  => out.push('\n'),
                Some('t')  => out.push('\t'),
                Some('r')  => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"')  => out.push('"'),
                Some(c)    => { out.push('\\'); out.push(c); }
                None       => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}