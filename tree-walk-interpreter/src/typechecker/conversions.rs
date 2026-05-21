use crate::ast::{Span, TypeExpr};
use crate::error::{ErrorCode, YoloscriptError};
use crate::typeinference::{InferType, Substitution};
use crate::types::Type;

/// Convert a source-level `TypeExpr` to an `InferType` for use during inference.
pub(super) fn type_expr_to_infer(te: &TypeExpr) -> InferType {
    match te {
        TypeExpr::Named(name, args) => {
            let arg_tys: Vec<_> = args.iter().map(type_expr_to_infer).collect();
            match (name.as_str(), arg_tys.len()) {
                ("Int",    0) => InferType::int(),
                ("Float",  0) => InferType::float(),
                ("Bool",   0) => InferType::bool(),
                ("String", 0) => InferType::str(),
                ("Never",  0) => InferType::never(),
                _             => InferType::Named(name.clone(), arg_tys),
            }
        }
        TypeExpr::Unit         => InferType::unit(),
        TypeExpr::Tuple(ts)    => InferType::Tuple(ts.iter().map(type_expr_to_infer).collect()),
        TypeExpr::Array(t)     => InferType::Array(Box::new(type_expr_to_infer(t))),
        TypeExpr::Fun(ps, ret) => InferType::Fun(
            ps.iter().map(type_expr_to_infer).collect(),
            Box::new(ret.as_deref().map(type_expr_to_infer).unwrap_or(InferType::unit())),
        ),
    }
}

/// Convert a fully-solved `InferType` to a concrete `Type`.
/// Returns E0002 if any type variable is still unresolved.
pub(super) fn infer_type_to_type(ty: &InferType, span: &Span) -> Result<Type, YoloscriptError> {
    match ty {
        InferType::Concrete(t) => Ok(t.clone()),
        InferType::Never       => Ok(Type::Never),
        InferType::Var(_)      => Err(YoloscriptError::type_error(
            ErrorCode::E0002,
            "cannot infer type; add a type annotation",
            span,
        )),
        InferType::Fun(params, ret) => {
            let p: Result<Vec<_>, _> = params.iter().map(|p| infer_type_to_type(p, span)).collect();
            Ok(Type::Fun(p?, Box::new(infer_type_to_type(ret, span)?)))
        }
        InferType::Tuple(ts) => {
            let t: Result<Vec<_>, _> = ts.iter().map(|t| infer_type_to_type(t, span)).collect();
            Ok(Type::Tuple(t?))
        }
        InferType::Array(t) => Ok(Type::Array(Box::new(infer_type_to_type(t, span)?))),
        InferType::Named(name, args) => {
            let a: Result<Vec<_>, _> = args.iter().map(|a| infer_type_to_type(a, span)).collect();
            let args = a?;
            match (name.as_str(), args.len()) {
                ("Perhaps", 1) => Ok(Type::Perhaps(Box::new(args.into_iter().next().unwrap()))),
                ("Result",  2) => {
                    let mut it = args.into_iter();
                    Ok(Type::Result(Box::new(it.next().unwrap()), Box::new(it.next().unwrap())))
                }
                _ => Ok(Type::Named(name.clone(), args)),
            }
        }
    }
}

pub(super) fn resolved_to_type(
    ty: &InferType,
    subst: &Substitution,
    span: &Span,
) -> Result<Type, YoloscriptError> {
    infer_type_to_type(&subst.apply(ty), span)
}

pub(super) fn type_to_infer(ty: &Type) -> InferType {
    match ty {
        Type::Never          => InferType::Never,
        Type::Array(t)       => InferType::Array(Box::new(type_to_infer(t))),
        Type::Tuple(ts)      => InferType::Tuple(ts.iter().map(type_to_infer).collect()),
        Type::Fun(ps, ret)   => InferType::Fun(
            ps.iter().map(type_to_infer).collect(),
            Box::new(type_to_infer(ret)),
        ),
        Type::Named(n, args) => InferType::Named(n.clone(), args.iter().map(type_to_infer).collect()),
        Type::Perhaps(t)     => InferType::Named("Perhaps".into(), vec![type_to_infer(t)]),
        Type::Result(t, e)   => InferType::Named("Result".into(), vec![type_to_infer(t), type_to_infer(e)]),
        other                => InferType::Concrete(other.clone()),
    }
}
