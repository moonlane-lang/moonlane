/// Resolved types — produced by the type checker, consumed by the evaluator.
/// No type variables exist here; generics have been monomorphised.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Unit,
    /// The bottom type `!`. Produced by expressions that never return (infinite
    /// loops with no reachable `break`, `return`, `panic!`). Coerces to any type.
    Never,
    Tuple(Vec<Type>),
    Array(Box<Type>),
    Fun(Vec<Type>, Box<Type>),
    /// A named type (struct, enum) with concrete type arguments after monomorphisation.
    Named(String, Vec<Type>),
    /// Convenience aliases — resolve to Named("Perhaps", ...) and Named("Result", ...)
    Perhaps(Box<Type>),
    Result(Box<Type>, Box<Type>),
}


impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Float => write!(f, "Float"),
            Type::Bool => write!(f, "Bool"),
            Type::Str => write!(f, "String"),
            Type::Unit => write!(f, "()"),
            Type::Never => write!(f, "!"),
            Type::Tuple(ts) => {
                write!(f, "(")?;
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            Type::Array(t) => write!(f, "{}[]", t),
            Type::Fun(params, ret) => {
                write!(f, "fun(")?;
                for (i, t) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", t)?;
                }
                write!(f, ") -> {}", ret)
            }
            Type::Named(name, args) => {
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
            Type::Perhaps(t) => write!(f, "Perhaps<{}>", t),
            Type::Result(t, e) => write!(f, "Result<{}, {}>", t, e),
        }
    }
}
