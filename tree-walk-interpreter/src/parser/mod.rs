use from_pest::FromPest;
use pest::iterators::{Pair, Pairs};
use pest::Parser;
use pest_derive::Parser;

use crate::ast::*;
use crate::error::{Span, YolangError};

#[derive(Parser)]
#[grammar = "grammar.pest"]
struct YolangParser;

/// Parse a Yolang source string into an untyped AST.
pub fn parse(source: &str, filename: &str) -> Result<Program, YolangError> {
    let mut pairs = YolangParser::parse(Rule::program, source).map_err(|e| {
        let (start, end) = match e.location {
            pest::error::InputLocation::Pos(p) => (p, p),
            pest::error::InputLocation::Span((s, e)) => (s, e),
        };
        YolangError::ParseErrorWithLine {
            message: e.variant.to_string(),
            start,
            end,
            line : e.line().to_string(),
            filename: filename.to_string(),
        }
    })?;

    Program::from_pest(&mut pairs).or_else(|op| {
        Result::Err(YolangError::ParseError { message: op.to_string(), start: 0, end: source.len(), filename: filename.to_string() })
    })
}
