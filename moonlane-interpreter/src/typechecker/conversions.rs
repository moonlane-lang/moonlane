use std::collections::HashMap;
use crate::ast::{Span, TypeExpr};
use crate::error::{TypeErrorCode, MoonlaneError};
use crate::typeinference::{InferType, Substitution, TypeVar};
use crate::types::Type;

/// Strip a module-path prefix from a type name written as `mod::path::Type`,
/// returning just the final type name segment. Single-segment names pass through.
fn bare_type_name(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
}

/// Like `type_expr_to_infer` but substitutes known generic parameter names with their
/// corresponding `InferType::Var`s.  Call this when inferring a generic function body
/// where `generics` maps each parameter name (e.g. `"T"`) to its fresh `TypeVar`.
pub(super) fn type_expr_to_infer_with_generics(
    te: &TypeExpr,
    generics: &HashMap<String, TypeVar>,
) -> InferType {
    match te {
        TypeExpr::Named(name, args) => {
            let bare = bare_type_name(name);
            // Zero-arg named type that matches a generic param → type variable.
            if args.is_empty() {
                if let Some(&tv) = generics.get(bare) {
                    return InferType::Var(tv);
                }
            }
            let arg_tys: Vec<_> = args.iter()
                .map(|a| type_expr_to_infer_with_generics(a, generics))
                .collect();
            match (bare, arg_tys.len()) {
                ("Int",    0) => InferType::int(),
                ("Float",  0) => InferType::float(),
                ("Bool",   0) => InferType::bool(),
                ("String", 0) => InferType::str(),
                ("Never",  0) => InferType::never(),
                _             => InferType::Named(bare.to_string(), arg_tys),
            }
        }
        TypeExpr::Unit => InferType::unit(),
        TypeExpr::Tuple(ts) => InferType::Tuple(
            ts.iter().map(|t| type_expr_to_infer_with_generics(t, generics)).collect(),
        ),
        TypeExpr::Array(t) => InferType::Array(
            Box::new(type_expr_to_infer_with_generics(t, generics)),
        ),
        TypeExpr::Fun(ps, ret) => InferType::Fun(
            ps.iter().map(|p| type_expr_to_infer_with_generics(p, generics)).collect(),
            Box::new(
                ret.as_deref()
                    .map(|r| type_expr_to_infer_with_generics(r, generics))
                    .unwrap_or(InferType::unit()),
            ),
        ),
    }
}

/// Convert a source-level `TypeExpr` to an `InferType` for use during inference.
pub(super) fn type_expr_to_infer(te: &TypeExpr) -> InferType {
    match te {
        TypeExpr::Named(name, args) => {
            let bare = bare_type_name(name);
            let arg_tys: Vec<_> = args.iter().map(type_expr_to_infer).collect();
            match (bare, arg_tys.len()) {
                ("Int",    0) => InferType::int(),
                ("Float",  0) => InferType::float(),
                ("Bool",   0) => InferType::bool(),
                ("String", 0) => InferType::str(),
                ("Never",  0) => InferType::never(),
                _             => InferType::Named(bare.to_string(), arg_tys),
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
pub(super) fn infer_type_to_type(ty: &InferType, span: &Span) -> Result<Type, MoonlaneError> {
    match ty {
        InferType::Concrete(t) => Ok(t.clone()),
        InferType::Never       => Ok(Type::Never),
        InferType::Var(_)      => Err(MoonlaneError::type_error(
            TypeErrorCode::T0002,
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
) -> Result<Type, MoonlaneError> {
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
