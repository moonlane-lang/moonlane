use pest::iterators::Pairs;
use pest::Parser;
use pest_derive::Parser;

use crate::ast::*;
use crate::error::{ParseErrorCode, MoonlaneError};

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct MoonlaneParser;

/// Parse a Moonlane source string into an untyped AST.
pub fn parse(source: &str, filename: &str) -> Result<Program, MoonlaneError> {
    let mut pairs = MoonlaneParser::parse(Rule::program, source).map_err(|e| {
        let (start, end) = match e.location {
            pest::error::InputLocation::Pos(p) => (p, p),
            pest::error::InputLocation::Span((s, e)) => (s, e),
        };
        let (line, col) = match &e.line_col {
            pest::error::LineColLocation::Pos((l, c)) => (*l as u32, *c as u32),
            pest::error::LineColLocation::Span((l, c), _) => (*l as u32, *c as u32),
        };
        MoonlaneError::ParseError {
            code: ParseErrorCode::P0001,
            message: e.variant.to_string(),
            start,
            end,
            filename: filename.to_string(),
            line,
            col,
            source_line: Some(e.line().to_string()),
        }
    })?;

    parse_program(&mut pairs, filename)
}



fn parse_program(pairs: &mut Pairs<Rule>, filename: &str) -> Result<Program, MoonlaneError> {
    let program_pair = pairs.next().ok_or_else(|| MoonlaneError::internal("parse_program: no program rule from pest"))?;
    if program_pair.as_rule() != Rule::program {
        return Err(MoonlaneError::internal("parse_program: first rule is not program"));
    }
    let mut decls = Vec::new();
    for pair in program_pair.into_inner() {
        match pair.as_rule() {
            Rule::decl => {
                decls.push(parse_decl(pair, filename)?);
            }
            Rule::EOI => {}
            _ => {}
        }
    }
    Ok(Program { decls: decls })
}

fn parse_decl(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Decl, MoonlaneError> {
    // `decl` has exactly one child
    let inner = pair.into_inner().next()
        .ok_or_else(|| MoonlaneError::internal("decl: missing inner rule"))?;
    match inner.as_rule() {
        Rule::let_decl    => Ok(Decl::Let(parse_let_decl(inner, filename)?)),
        Rule::mut_decl    => Ok(Decl::Mut(parse_mut_decl(inner, filename)?)),
        Rule::fun_decl    => Ok(Decl::Fun(parse_fun_decl(inner, filename)?)),
        Rule::struct_decl => Ok(Decl::Struct(parse_struct_decl(inner, filename)?)),
        Rule::enum_decl   => Ok(Decl::Enum(parse_enum_decl(inner, filename)?)),
        Rule::impl_block  => Ok(Decl::Impl(parse_impl_block(inner, filename)?)),
        Rule::aspect_decl => Ok(Decl::Aspect(parse_aspect_decl(inner, filename)?)),
        Rule::stmt        => Ok(Decl::Stmt(parse_stmt(inner, filename)?)),
        r => Err(MoonlaneError::internal(format!("decl: unexpected rule {r:?}"))),
    }
}

fn parse_let_decl(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<LetDecl, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("let_decl: expected identifier"))?
        .as_str().to_string();
    let (type_ann, value) = parse_opt_type_then_expr(&mut inner, filename)?;
    Ok(LetDecl { name, type_ann, value, span })
}

fn parse_mut_decl(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<MutDecl, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("mut_decl: expected identifier"))?
        .as_str().to_string();
    let (type_ann, value) = parse_opt_type_then_expr(&mut inner, filename)?;
    Ok(MutDecl { name, type_ann, value, span })
}

/// Shared helper: parse `(":" type_expr)? expr` from a pair iterator.
fn parse_opt_type_then_expr(
    inner: &mut pest::iterators::Pairs<Rule>,
    filename: &str
) -> Result<(Option<TypeExpr>, Expr), MoonlaneError> {
    let next = inner.next()
        .ok_or_else(|| MoonlaneError::internal("expected type annotation or expression"))?;
    match next.as_rule() {
        Rule::type_expr => {
            let type_ann = Some(parse_type_expr(next, filename)?);
            let expr_pair = inner.next()
                .ok_or_else(|| MoonlaneError::internal("expected expression after type annotation"))?;
            let value = parse_expr(expr_pair, filename)?;
            Ok((type_ann, value))
        }
        Rule::expr => Ok((None, parse_expr(next, filename)?)),
        r => Err(MoonlaneError::internal(format!("expected type_expr or expr, got {r:?}"))),
    }
}

fn parse_fun_decl(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<FunDecl, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("fun_decl: expected function name"))?
        .as_str().to_string();
    let mut generics    = vec![];
    let mut params      = vec![];
    let mut return_type = None;
    let mut body        = None;
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => generics = parse_generic_params(p, filename)?,
            Rule::param_list     => params   = parse_param_list(p, filename)?,
            Rule::type_expr      => return_type = Some(parse_type_expr(p, filename)?),
            Rule::block          => body = Some(parse_block(p, filename)?),
            _ => {}
        }
    }
    Ok(FunDecl {
        name, generics, params, return_type,
        body: body.ok_or_else(|| MoonlaneError::internal("fun_decl: missing body block"))?,
        span,
    })
}

fn parse_struct_decl(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<StructDecl, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("struct_decl: expected name"))?
        .as_str().to_string();
    let mut generics = vec![];
    let mut fields   = vec![];
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => generics = parse_generic_params(p, filename)?,
            Rule::struct_fields  => fields   = parse_struct_fields(p, filename)?,
            _ => {}
        }
    }
    Ok(StructDecl { name, generics, fields, span })
}

fn parse_enum_decl(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<EnumDecl, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("enum_decl: expected name"))?
        .as_str().to_string();
    let mut generics = vec![];
    let mut variants = vec![];
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => generics = parse_generic_params(p, filename)?,
            Rule::enum_variants  => {
                for v in p.into_inner() {
                    if v.as_rule() == Rule::enum_variant {
                        variants.push(parse_enum_variant(v, filename)?);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(EnumDecl { name, generics, variants, span })
}

fn parse_impl_block(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<ImplBlock, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let inner = pair.into_inner();
    let mut aspect_name = None;
    let mut aspect_type_args = vec![];
    let target_type;
    let mut methods = vec![];

    // Grammar: "impl" ~ (named_type ~ "for")? ~ type_expr ~ "{" ~ fun_decl* ~ "}"
    // Children: optionally [named_type, type_expr], or just [type_expr], then fun_decls.
    let mut collected: Vec<pest::iterators::Pair<Rule>> = inner.collect();

    let fun_start = collected.iter().position(|p| p.as_rule() == Rule::fun_decl)
        .unwrap_or(collected.len());
    let type_pairs: Vec<_> = collected.drain(..fun_start).collect();
    let fun_pairs = collected;

    match type_pairs.len() {
        0 => return Err(MoonlaneError::internal("impl_block: no target type found")),
        1 => {
            // `impl Type { ... }`
            target_type = Some(parse_type_expr(type_pairs.into_iter().next().unwrap(), filename)?);
        }
        2 => {
            // `impl Aspect<T> for Type { ... }`
            let mut it = type_pairs.into_iter();
            let aspect_pair = it.next().unwrap(); // named_type rule
            // named_type = { ident ~ ("<" ~ type_args ~ ">")? }
            let mut inner_pairs = aspect_pair.into_inner();
            let name_ident = inner_pairs.next().unwrap();
            aspect_name = Some(name_ident.as_str().to_string());
            // Collect generic type args if present
            for p in inner_pairs {
                if p.as_rule() == Rule::type_args {
                    for arg in p.into_inner() {
                        if arg.as_rule() == Rule::type_expr {
                            aspect_type_args.push(parse_type_expr(arg, filename)?);
                        }
                    }
                }
            }
            target_type = Some(parse_type_expr(it.next().unwrap(), filename)?);
        }
        n => return Err(MoonlaneError::internal(format!("impl_block: unexpected {n} type pairs"))),
    }

    for p in fun_pairs {
        if p.as_rule() == Rule::fun_decl {
            methods.push(parse_fun_decl(p, filename)?);
        }
    }

    Ok(ImplBlock { aspect_name, aspect_type_args, target_type: target_type.unwrap(), methods, span })
}


fn parse_param_list(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Vec<Param>, MoonlaneError> {
    let mut params = vec![];
    for p in pair.into_inner() {
        if p.as_rule() == Rule::param {
            params.push(parse_param(p, filename)?);
        }
    }
    Ok(params)
}

fn parse_param(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Param, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let text = pair.as_str().trim();
    if text == "self" {
        return Ok(Param { mutable: false, name: "self".into(), type_ann: None, span });
    }
    if text == "mut self" {
        return Ok(Param { mutable: true, name: "self".into(), type_ann: None, span });
    }
    // ident (":" type_expr)?
    let mut inner = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("param: expected name"))?
        .as_str().to_string();
    let type_ann = inner.next().map(|p| parse_type_expr(p, filename)).transpose()?;
    Ok(Param { mutable: false, name, type_ann, span })
}

fn parse_struct_fields(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Vec<FieldDef>, MoonlaneError> {
    let mut fields = vec![];
    for p in pair.into_inner() {
        if p.as_rule() == Rule::struct_field {
            let span = Span::of(&p, filename);
            let mut it = p.into_inner();
            let name = it.next()
                .ok_or_else(|| MoonlaneError::internal("struct_field: expected name"))?
                .as_str().to_string();
            let type_ann = parse_type_expr(
                it.next().ok_or_else(|| MoonlaneError::internal("struct_field: expected type"))?,
                filename,
            )?;
            fields.push(FieldDef { name, type_ann, span });
        }
    }
    Ok(fields)
}

fn parse_enum_variant(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<VariantDef, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("enum_variant: expected name"))?
        .as_str().to_string();
    let mut fields = vec![];
    for p in inner {
        if p.as_rule() == Rule::struct_fields {
            fields = parse_struct_fields(p, filename)?;
        }
    }
    Ok(VariantDef { name, fields, span })
}

fn parse_aspect_method(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<AspectMethod, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner       = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("aspect_method: expected name"))?
        .as_str().to_string();
    let mut generics    = vec![];
    let mut params      = vec![];
    let mut return_type = None;
    let mut default_body = None;
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => generics     = parse_generic_params(p, filename)?,
            Rule::param_list     => params       = parse_param_list(p, filename)?,
            Rule::type_expr      => return_type  = Some(parse_type_expr(p, filename)?),
            Rule::block          => default_body = Some(parse_block(p, filename)?),
            _ => {}
        }
    }
    Ok(AspectMethod { name, generics, params, return_type, default_body, span })
}


fn parse_stmt(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Stmt, MoonlaneError> {
    let inner = pair.into_inner().next()
        .ok_or_else(|| MoonlaneError::internal("stmt: missing inner rule"))?;
    match inner.as_rule() {
        Rule::while_stmt   => Ok(Stmt::While(parse_while_stmt(inner, filename)?)),
        Rule::for_stmt     => Ok(Stmt::For(parse_for_stmt(inner, filename)?)),
        Rule::for_in_stmt  => Ok(Stmt::ForIn(parse_for_in_stmt(inner, filename)?)),
        Rule::return_stmt  => Ok(Stmt::Return(parse_return_stmt(inner, filename)?)),
        Rule::break_stmt   => Ok(Stmt::Break(parse_break_stmt(inner, filename)?)),
        Rule::continue_stmt => Ok(Stmt::Continue(Span::of(&inner, filename))),
        Rule::expr_stmt    => {
            let expr_pair = inner.into_inner().next()
                .ok_or_else(|| MoonlaneError::internal("expr_stmt: missing expression"))?;
            Ok(Stmt::Expr(parse_expr(expr_pair, filename)?))
        }
        r => Err(MoonlaneError::internal(format!("stmt: unexpected rule {r:?}"))),
    }
}


fn parse_while_stmt(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<WhileStmt, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let condition = parse_expr(
        inner.next().ok_or_else(|| MoonlaneError::internal("while_stmt: expected condition"))?,
        filename,
    )?;
    let body = parse_block(
        inner.next().ok_or_else(|| MoonlaneError::internal("while_stmt: expected body"))?,
        filename,
    )?;
    Ok(WhileStmt { condition, body, span })
}


fn parse_for_stmt(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<ForStmt, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();

    // for_init
    let init_pair = inner.next()
        .ok_or_else(|| MoonlaneError::internal("for_stmt: expected init"))?;
    let init = if init_pair.as_rule() == Rule::for_init {
        match init_pair.into_inner().next() {
            Some(p) => match p.as_rule() {
                Rule::mut_decl  => Some(ForInit::Mut(parse_mut_decl(p, filename)?)),
                Rule::expr_stmt => {
                    let ep = p.into_inner().next()
                        .ok_or_else(|| MoonlaneError::internal("for_stmt: expected expr in expr_stmt"))?;
                    Some(ForInit::Expr(parse_expr(ep, filename)?))
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
            Rule::expr  => if condition.is_none() { condition = Some(parse_expr(p, filename)?); }
                           else                   { step      = Some(parse_expr(p, filename)?); }
            Rule::block => body = Some(parse_block(p, filename)?),
            _ => {}
        }
    }
    Ok(ForStmt {
        init, condition, step,
        body: body.ok_or_else(|| MoonlaneError::internal("for_stmt: missing body"))?,
        span,
    })
}


fn parse_return_stmt(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<ReturnStmt, MoonlaneError> {
    let span  = Span::of(&pair, filename);
    let value = pair.into_inner().next().map(|p| parse_expr(p, filename)).transpose()?;
    Ok(ReturnStmt { value, span })
}


fn parse_break_stmt(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<BreakStmt, MoonlaneError> {
    let span  = Span::of(&pair, filename);
    let value = pair.into_inner().next().map(|p| parse_expr(p, filename)).transpose()?;
    Ok(BreakStmt { value, span })
}


/// Entry point: consumes one `expr` pair.
fn parse_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    match pair.as_rule() {
        Rule::expr => {
            let inner = pair.into_inner().next()
                .ok_or_else(|| MoonlaneError::internal("expr: missing inner rule"))?;
            parse_expr(inner, filename)
        }
        Rule::assign_expr => parse_assign_expr(pair, filename),
        Rule::or_expr     => parse_lr_binary(pair, filename),
        Rule::and_expr    => parse_lr_binary(pair, filename),
        Rule::cmp_expr    => parse_lr_binary(pair, filename),
        Rule::range_expr  => parse_lr_binary(pair, filename),
        Rule::add_expr    => parse_lr_binary(pair, filename),
        Rule::mul_expr    => parse_lr_binary(pair, filename),
        Rule::cast_expr   => parse_cast_expr(pair, filename),
        Rule::asc_expr    => parse_asc_expr(pair, filename),
        Rule::unary_expr  => parse_unary_expr(pair, filename),
        Rule::postfix_expr => parse_postfix_expr(pair, filename),
        Rule::primary_expr => {
            let inner = pair.into_inner().next()
                .ok_or_else(|| MoonlaneError::internal("primary_expr: missing inner rule"))?;
            parse_expr(inner, filename)
        }
        // Terminals and composites reachable from primary_expr
        Rule::int_lit | Rule::float_lit | Rule::string_lit
        | Rule::bool_lit | Rule::nope_lit | Rule::unit_lit => parse_literal_expr(pair, filename),
        Rule::path_expr     => parse_path_expr(pair, filename),
        Rule::tuple_or_paren => parse_tuple_or_paren(pair, filename),
        Rule::array_lit     => parse_array_lit(pair, filename),
        Rule::match_expr    => Ok(Expr::Match(parse_match_expr(pair, filename)?)),
        Rule::if_expr       => parse_if_expr(pair, filename),
        Rule::loop_expr     => parse_loop_expr(pair, filename),
        Rule::closure_expr  => parse_closure_expr(pair, filename),
        Rule::struct_literal => parse_struct_literal(pair, filename),
        r => Err(MoonlaneError::internal(format!("parse_expr: unexpected rule {r:?}"))),
    }
}

fn parse_literal_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let text = pair.as_str();
    let lit = match pair.as_rule() {
        Rule::int_lit => Literal::Int(
            text.replace('_', "").parse().map_err(|_| MoonlaneError::ParseError {
                code: ParseErrorCode::P0002,
                message: format!("integer literal '{text}' is out of range for i64"),
                start: span.start, end: span.end, filename: filename.to_string(),
                line: span.line, col: span.col, source_line: None,
            })?
        ),
        Rule::float_lit => Literal::Float(
            text.parse().map_err(|_| MoonlaneError::ParseError {
                code: ParseErrorCode::P0003,
                message: format!("invalid float literal '{text}'"),
                start: span.start, end: span.end, filename: filename.to_string(),
                line: span.line, col: span.col, source_line: None,
            })?
        ),
        Rule::string_lit => Literal::Str(unescape(&text[1..text.len()-1])),
        Rule::bool_lit   => Literal::Bool(text == "true"),
        Rule::nope_lit   => Literal::Nope,
        Rule::unit_lit   => Literal::Unit,
        r => return Err(MoonlaneError::internal(format!("parse_literal_expr: unexpected rule {r:?}"))),
    };
    Ok(Expr::Literal(lit, span))
}

fn parse_path_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span  = Span::of(&pair, filename);
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

fn parse_tuple_or_paren(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let elems: Vec<Expr> = pair.into_inner()
        .filter(|p| p.as_rule() == Rule::expr)
        .map(|p| parse_expr(p, filename))
        .collect::<Result<_, _>>()?;
    if elems.len() == 1 {
        Ok(elems.into_iter().next().unwrap())
    } else {
        Ok(Expr::Tuple(elems, span))
    }
}

fn parse_array_lit(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let elems = pair.into_inner()
        .filter(|p| p.as_rule() == Rule::expr)
        .map(|p| parse_expr(p, filename))
        .collect::<Result<_, _>>()?;
    Ok(Expr::Array(elems, span))
}

fn wrap_expr_as_block(expr: Expr) -> Block {
    let s = expr.span().clone();
    Block { stmts: vec![], tail: Some(Box::new(expr)), span: s }
}

fn parse_if_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();

    let condition = parse_expr(
        inner.next().ok_or_else(|| MoonlaneError::internal("if_expr: expected condition"))?,
        filename,
    )?;

    let then_pair = inner.next().ok_or_else(|| MoonlaneError::internal("if_expr: expected then body"))?;
    let then_is_block = then_pair.as_rule() == Rule::block;
    let then_branch = if then_is_block {
        parse_block(then_pair, filename)?
    } else {
        let expr = parse_expr(then_pair, filename)?;
        // Braceless body that is itself an if–else creates dangling-else ambiguity.
        if let Expr::If { else_branch: Some(_), .. } = &expr {
            return Err(MoonlaneError::parse(
                ParseErrorCode::P0001,
                "braceless if body may not contain an if–else expression; wrap the outer body in braces",
                &span,
            ));
        }
        wrap_expr_as_block(expr)
    };

    let else_branch = match inner.next() {
        None => None,
        Some(p) => {
            let else_is_block = p.as_rule() == Rule::block;
            let else_is_if    = p.as_rule() == Rule::if_expr;
            // Mixed arm styles are not allowed.
            if then_is_block && !else_is_block && !else_is_if {
                return Err(MoonlaneError::parse(
                    ParseErrorCode::P0001,
                    "mismatched if arm styles: then branch uses braces but else branch does not",
                    &span,
                ));
            }
            if !then_is_block && else_is_block {
                return Err(MoonlaneError::parse(
                    ParseErrorCode::P0001,
                    "mismatched if arm styles: then branch is braceless but else branch uses braces",
                    &span,
                ));
            }
            Some(match p.as_rule() {
                Rule::block => parse_block(p, filename)?,
                // `else if` — wrap the nested if_expr in a synthetic block so that
                // Expr::If.else_branch is always Option<Block>.
                Rule::if_expr => {
                    let nested = parse_if_expr(p, filename)?;
                    wrap_expr_as_block(nested)
                }
                _ => wrap_expr_as_block(parse_expr(p, filename)?),
            })
        }
    };

    Ok(Expr::If { condition: Box::new(condition), then_branch, else_branch, span })
}

fn parse_loop_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let body = parse_block(
        pair.into_inner().next().ok_or_else(|| MoonlaneError::internal("loop_expr: expected body"))?,
        filename,
    )?;
    Ok(Expr::Loop { body, span })
}

fn parse_closure_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut params      = vec![];
    let mut return_type = None;
    let mut body        = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::param_list => params      = parse_param_list(p, filename)?,
            Rule::type_expr  => return_type = Some(parse_type_expr(p, filename)?),
            Rule::block      => body        = Some(parse_block(p, filename)?),
            _ => {}
        }
    }
    Ok(Expr::Closure {
        params, return_type,
        body: body.ok_or_else(|| MoonlaneError::internal("closure: missing body block"))?,
        span,
    })
}

fn parse_struct_literal(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let path_pair = inner.next()
        .ok_or_else(|| MoonlaneError::internal("struct_literal: expected path"))?;
    let path: Vec<String> = path_pair.into_inner()
        .filter(|p| p.as_rule() == Rule::ident)
        .map(|p| p.as_str().to_string())
        .collect();
    let mut fields = vec![];
    for p in inner {
        if p.as_rule() == Rule::field_init {
            let field_span = Span::of(&p, filename);
            let mut it = p.into_inner();
            let name_pair = it.next()
                .ok_or_else(|| MoonlaneError::internal("struct_literal: expected field name"))?;
            let name = name_pair.as_str().to_string();
            let value = match it.next() {
                Some(expr_pair) => parse_expr(expr_pair, filename)?,
                None => Expr::Ident(name.clone(), field_span),
            };
            fields.push((name, value));
        }
    }
    Ok(Expr::StructLiteral { path, fields, span })
}

// ── Assignment ────────────────────────────────────────────────────────────────

fn parse_assign_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span  = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let first = inner.next()
        .ok_or_else(|| MoonlaneError::internal("assign_expr: expected first child"))?;

    // assign_expr = { postfix_expr ~ assign_op ~ assign_expr | or_expr }
    // If first child is postfix_expr and next is assign_op, it's an assignment.
    // Otherwise it's an or_expr chain.
    match first.as_rule() {
        Rule::postfix_expr => {
            let lhs = parse_postfix_expr(first, filename)?;
            match inner.next() {
                Some(op_pair) if op_pair.as_rule() == Rule::assign_op => {
                    let op     = parse_assign_op(op_pair.as_str());
                    let rhs    = parse_expr(
                        inner.next().ok_or_else(|| MoonlaneError::internal("assign_expr: expected rhs"))?,
                        filename,
                    )?;
                    let target = expr_to_assign_target(lhs)?;
                    Ok(Expr::Assign { target, op, value: Box::new(rhs), span })
                }
                _ => Ok(lhs), // shouldn't happen with valid grammar
            }
        }
        Rule::or_expr => parse_lr_binary(first, filename),
        _             => parse_expr(first, filename),
    }
}

// ── Binary expressions (left-recursive) ──────────────────────────────────────

/// Handles or_expr, and_expr, cmp_expr, range_expr, add_expr, mul_expr.
/// All follow the pattern: operand (op operand)* where op is a named rule.
fn parse_lr_binary(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span  = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let first = inner.next()
        .ok_or_else(|| MoonlaneError::internal("binary_expr: expected first operand"))?;
    let mut expr = parse_expr(first, filename)?;

    // Consume op/operand pairs
    while let Some(op_pair) = inner.next() {
        let op      = parse_bin_op(&op_pair);
        let rhs_pair = inner.next()
            .ok_or_else(|| MoonlaneError::internal("binary_expr: expected rhs operand"))?;
        let rhs     = parse_expr(rhs_pair, filename)?;
        let op_span = Span::of(&op_pair, filename);
        expr = Expr::BinOp(Box::new(expr), op, Box::new(rhs), op_span);
    }
    let _ = span; // span used in outer call if needed
    Ok(expr)
}

// ── Ascription and Cast ───────────────────────────────────────────────────────

fn parse_asc_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let first = inner.next()
        .ok_or_else(|| MoonlaneError::internal("asc_expr: expected operand"))?;
    let expr = parse_expr(first, filename)?;
    match inner.next() {
        Some(ty_pair) => {
            let ann = parse_type_expr(ty_pair, filename)?;
            Ok(Expr::Ascribe { expr: Box::new(expr), ann, span })
        }
        None => Ok(expr),
    }
}

fn parse_cast_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span  = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let first = inner.next()
        .ok_or_else(|| MoonlaneError::internal("cast_expr: expected operand"))?;
    let mut expr = parse_expr(first, filename)?;
    for p in inner {
        if p.as_rule() == Rule::type_expr {
            let target_type = parse_type_expr(p, filename)?;
            expr = Expr::Cast { expr: Box::new(expr), target_type, span: span.clone() };
        }
    }
    Ok(expr)
}

// ── Unary ─────────────────────────────────────────────────────────────────────

fn parse_unary_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let text = pair.as_str();
    let child = pair.into_inner().next()
        .ok_or_else(|| MoonlaneError::internal("unary_expr: expected operand"))?;
    if text.starts_with('!') {
        Ok(Expr::UnaryOp(UnaryOp::Not, Box::new(parse_expr(child, filename)?), span))
    } else if text.starts_with('-') {
        Ok(Expr::UnaryOp(UnaryOp::Neg, Box::new(parse_expr(child, filename)?), span))
    } else {
        parse_expr(child, filename)
    }
}

// ── Postfix ───────────────────────────────────────────────────────────────────

fn parse_postfix_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let mut inner = pair.into_inner();
    let primary = inner.next()
        .ok_or_else(|| MoonlaneError::internal("postfix_expr: expected primary"))?;
    let mut expr = parse_expr(primary, filename)?;
    for postfix in inner {
        if postfix.as_rule() == Rule::postfix {
            expr = apply_postfix(expr, postfix, filename)?;
        }
    }
    Ok(expr)
}

fn apply_postfix(base: Expr, pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Expr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let text = pair.as_str();
    let mut inner = pair.into_inner();

    if text.starts_with('(') {
        // Function call: postfix children are (arg_list?), so unwrap one level
        let args = match inner.next() {
            Some(a) if a.as_rule() == Rule::arg_list => collect_args(a.into_inner(), filename)?,
            _ => vec![],
        };
        Ok(Expr::Call { callee: Box::new(base), args, span })
    } else if text.starts_with('[') {
        // Index
        let idx = parse_expr(
            inner.next().ok_or_else(|| MoonlaneError::internal("postfix index: expected index expr"))?,
            filename,
        )?;
        Ok(Expr::Index { object: Box::new(base), index: Box::new(idx), span })
    } else if text == "?" {
        Ok(Expr::PropagateError { expr: Box::new(base), span })
    } else {
        // Dot postfix — first named child is decimal_int or ident
        let first = inner.next()
            .ok_or_else(|| MoonlaneError::internal("postfix dot: expected field name or index"))?;
        match first.as_rule() {
            Rule::decimal_int => {
                let idx = first.as_str().parse::<usize>()
                    .map_err(|_| MoonlaneError::internal(
                        format!("postfix dot: '{}' is not a valid tuple index", first.as_str())
                    ))?;
                Ok(Expr::TupleAccess { object: Box::new(base), index: idx, span })
            }
            Rule::ident => {
                let name = first.as_str().to_string();
                // If a `(` follows in the text, it's a method call
                if text.contains('(') {
                    let args = match inner.next() {
                        Some(a) if a.as_rule() == Rule::arg_list => collect_args(a.into_inner(), filename)?,
                        _ => vec![],
                    };
                    Ok(Expr::MethodCall { receiver: Box::new(base), method: name, args, span })
                } else {
                    Ok(Expr::FieldAccess { object: Box::new(base), field: name, span })
                }
            }
            r => Err(MoonlaneError::internal(format!("postfix dot: unexpected child rule {r:?}"))),
        }
    }
}

fn collect_args(
    pairs: pest::iterators::Pairs<Rule>,
    filename: &str
) -> Result<Vec<Expr>, MoonlaneError> {
    pairs.filter(|p| p.as_rule() == Rule::expr)
         .map(|p| parse_expr(p, filename))
         .collect()
}


fn parse_match_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<MatchExpr, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let scrutinee = parse_expr(
        inner.next().ok_or_else(|| MoonlaneError::internal("match_expr: expected scrutinee"))?,
        filename,
    )?;
    let arms: Vec<MatchArm> = inner
        .filter(|p| p.as_rule() == Rule::match_arm)
        .map(|p| parse_match_arm(p, filename))
        .collect::<Result<_, _>>()?;
    Ok(MatchExpr { scrutinee: Box::new(scrutinee), arms, span })
}


fn parse_match_arm(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<MatchArm, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let pattern = parse_pattern(
        inner.next().ok_or_else(|| MoonlaneError::internal("match_arm: expected pattern"))?,
        filename,
    )?;

    // Remaining children: optionally a guard `expr`, then body `block | expr`.
    let remaining: Vec<_> = inner.collect();
    let (body_pair, guard_pairs) = remaining.split_last()
        .ok_or_else(|| MoonlaneError::internal("match_arm: expected body"))?;

    let guard = guard_pairs.iter()
        .find(|p| p.as_rule() == Rule::expr)
        .map(|p| parse_expr(p.clone(), filename))
        .transpose()?;

    let body = match body_pair.as_rule() {
        Rule::block => parse_block(body_pair.clone(), filename)?,
        Rule::expr  => {
            let body_span = Span::of(body_pair, filename);
            let expr = parse_expr(body_pair.clone(), filename)?;
            Block { stmts: vec![], tail: Some(Box::new(expr)), span: body_span }
        }
        _ => return Err(MoonlaneError::internal("match_arm: unexpected body rule")),
    };

    Ok(MatchArm { pattern, guard, body, span })
}

fn parse_pattern(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Pattern, MoonlaneError> {
    match pair.as_rule() {
        Rule::pattern => {
            // The anonymous wildcard alternative (`"_" ~ !(...))`) produces a
            // Rule::pattern pair with no children, so check for it first.
            if pair.as_str().trim() == "_" {
                return Ok(Pattern::Wildcard(Span::of(&pair, filename)));
            }
            let inner = pair.into_inner().next()
                .ok_or_else(|| MoonlaneError::internal("pattern: missing inner rule"))?;
            parse_pattern(inner, filename)
        }
        Rule::nope_lit => Ok(Pattern::Nope(Span::of(&pair, filename))),
        Rule::tuple_pattern => {
            let span = Span::of(&pair,filename);
            let pats = pair.into_inner()
                .filter(|p| p.as_rule() == Rule::pattern)
                .map(|p| parse_pattern(p, filename))
                .collect::<Result<_, _>>()?;
            Ok(Pattern::Tuple(pats, span))
        }
        Rule::enum_pattern => {
            let span = Span::of(&pair, filename);
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
            let span = Span::of(&pair, filename);
            let lit_pair = pair.into_inner().next()
                .ok_or_else(|| MoonlaneError::internal("literal_pattern: expected literal"))?;
            let text = lit_pair.as_str();
            let lit = match lit_pair.as_rule() {
                Rule::float_lit => Literal::Float(
                    text.parse().map_err(|_| MoonlaneError::ParseError {
                        code: ParseErrorCode::P0003,
                        message: format!("float literal '{text}' is out of range"),
                        start: span.start, end: span.end, filename: filename.to_string(),
                        line: span.line, col: span.col, source_line: None,
                    })?
                ),
                Rule::int_lit => Literal::Int(
                    text.replace('_', "").parse().map_err(|_| MoonlaneError::ParseError {
                        code: ParseErrorCode::P0002,
                        message: format!("integer literal '{text}' is out of range for i64"),
                        start: span.start, end: span.end, filename: filename.to_string(),
                        line: span.line, col: span.col, source_line: None,
                    })?
                ),
                Rule::string_lit => Literal::Str(unescape(&text[1..text.len()-1])),
                Rule::bool_lit   => Literal::Bool(text == "true"),
                r => return Err(MoonlaneError::internal(format!("literal_pattern: unexpected rule {r:?}"))),
            };
            Ok(Pattern::Literal(lit, span))
        }
        Rule::bind_pattern => {
            let span = Span::of(&pair, filename);
            let name = pair.into_inner().next()
                .ok_or_else(|| MoonlaneError::internal("bind_pattern: expected name"))?
                .as_str().to_string();
            Ok(Pattern::Binding(name, span))
        }
        // Wildcard: the `"_" ~ !(...)` alternative in `pattern` is anonymous;
        // pest emits no sub-rule, so `pair.as_rule() == Rule::pattern` and
        // `pair.as_str() == "_"` — handled by the outer `pattern` arm above
        // which recurses into the single child. If there is no child and the
        // text is "_", we match here.
        _ if pair.as_str().trim() == "_" => Ok(Pattern::Wildcard(Span::of(&pair, filename))),
        r => Err(MoonlaneError::internal(format!("pattern: unexpected rule {r:?}"))),
    }
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

fn expr_to_assign_target(expr: Expr) -> Result<AssignTarget, MoonlaneError> {
    match expr {
        Expr::Ident(name, span) =>
            Ok(AssignTarget::Ident(name, span)),
        Expr::FieldAccess { object, field, span } =>
            Ok(AssignTarget::FieldAccess { object, field, span }),
        Expr::Index { object, index, span } =>
            Ok(AssignTarget::Index { object, index, span }),
        _ => Err(MoonlaneError::internal("assign target must be an identifier, field access, or index expression")),
    }
}


fn parse_type_expr(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<TypeExpr, MoonlaneError> {
    match pair.as_rule() {
        Rule::type_expr => {
            let inner = pair.into_inner().next()
                .ok_or_else(|| MoonlaneError::internal("type_expr: missing inner rule"))?;
            parse_type_expr(inner, filename)
        }
        Rule::unit_type  => Ok(TypeExpr::Unit),
        Rule::tuple_type => {
            let elems = pair.into_inner()
                .filter(|p| p.as_rule() == Rule::type_expr)
                .map(|p| parse_type_expr(p, filename))
                .collect::<Result<_, _>>()?;
            Ok(TypeExpr::Tuple(elems))
        }
        Rule::array_type => {
            let elem = parse_type_expr(
                pair.into_inner().next()
                    .ok_or_else(|| MoonlaneError::internal("array_type: expected element type"))?,
                filename,
            )?;
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
                            .map(|p| parse_type_expr(p, filename))
                            .collect::<Result<_, _>>()?;
                    }
                    Rule::type_expr => return_type = Some(Box::new(parse_type_expr(p, filename)?)),
                    _ => {}
                }
            }
            Ok(TypeExpr::Fun(params, return_type))
        }
        Rule::named_type => {
            let mut inner = pair.into_inner();
            let name = inner.next()
                .ok_or_else(|| MoonlaneError::internal("named_type: expected name"))?
                .as_str().to_string();
            let mut args = vec![];
            for p in inner {
                if p.as_rule() == Rule::type_args {
                    args = p.into_inner()
                        .filter(|q| q.as_rule() == Rule::type_expr)
                        .map(|p| parse_type_expr(p, filename))
                        .collect::<Result<_, _>>()?;
                }
            }
            Ok(TypeExpr::Named(name, args))
        }
        r => Err(MoonlaneError::internal(format!("type_expr: unexpected rule {r:?}"))),
    }
}

fn parse_for_in_stmt(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<ForInStmt, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let binding  = inner.next()
        .ok_or_else(|| MoonlaneError::internal("for_in: expected binding name"))?
        .as_str().to_string();
    let iterable = parse_expr(
        inner.next().ok_or_else(|| MoonlaneError::internal("for_in: expected iterable expression"))?,
        filename,
    )?;
    let body = parse_block(
        inner.next().ok_or_else(|| MoonlaneError::internal("for_in: expected body block"))?,
        filename,
    )?;
    Ok(ForInStmt { binding, iterable, body, span })
}

fn parse_block(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Block, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut stmts = vec![];
    let mut tail  = None;
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::block_item => {
                let inner = p.into_inner().next()
                    .ok_or_else(|| MoonlaneError::internal("block_item: missing inner rule"))?;
                match inner.as_rule() {
                    Rule::block_expr_stmt => {
                        let expr_pair = inner.into_inner().next()
                            .ok_or_else(|| MoonlaneError::internal("block_expr_stmt: missing expr"))?;
                        let expr = match expr_pair.as_rule() {
                            Rule::if_expr    => parse_if_expr(expr_pair, filename)?,
                            Rule::match_expr => Expr::Match(parse_match_expr(expr_pair, filename)?),
                            Rule::loop_expr  => parse_loop_expr(expr_pair, filename)?,
                            r => return Err(MoonlaneError::internal(format!("block_expr_stmt: unexpected rule {r:?}"))),
                        };
                        stmts.push(Decl::Stmt(Stmt::Expr(expr)));
                    }
                    Rule::decl => stmts.push(parse_decl(inner, filename)?),
                    r => return Err(MoonlaneError::internal(format!("block_item: unexpected rule {r:?}"))),
                }
            }
            Rule::decl => stmts.push(parse_decl(p, filename)?),
            Rule::expr => tail = Some(Box::new(parse_expr(p, filename)?)),
            _ => {}
        }
    }
    Ok(Block { stmts, tail, span })
}

fn parse_generic_params(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<Vec<GenericParam>, MoonlaneError> {
    let mut params = vec![];
    for p in pair.into_inner() {
        if p.as_rule() == Rule::generic_param {
            let mut it = p.into_inner();
            let name = it.next()
                .ok_or_else(|| MoonlaneError::internal("generic_param: expected name"))?
                .as_str().to_string();
            let bound = it.next().map(|p| parse_type_expr(p, filename)).transpose()?;
            params.push(GenericParam { name, bound });
        }
    }
    Ok(params)
}

fn parse_aspect_decl(pair: pest::iterators::Pair<Rule>, filename: &str) -> Result<AspectDecl, MoonlaneError> {
    let span = Span::of(&pair, filename);
    let mut inner = pair.into_inner();
    let name = inner.next()
        .ok_or_else(|| MoonlaneError::internal("aspect_decl: expected name"))?
        .as_str().to_string();
    let mut generics = vec![];
    let mut methods = vec![];
    for p in inner {
        match p.as_rule() {
            Rule::generic_params => {
                for gp in p.into_inner() {
                    if gp.as_rule() == Rule::generic_param {
                        let pname = gp.into_inner().next().map(|i| i.as_str().to_string()).unwrap_or_default();
                        generics.push(pname);
                    }
                }
            }
            Rule::aspect_method => { methods.push(parse_aspect_method(p, filename)?); }
            _ => {}
        }
    }
    Ok(AspectDecl { name, generics, methods, span })
}


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
