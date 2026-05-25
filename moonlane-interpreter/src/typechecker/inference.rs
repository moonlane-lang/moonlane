use std::collections::HashMap;

use crate::ast::*;
use crate::error::{TypeErrorCode, MoonlaneError};
use crate::typeinference::*;
use crate::types::Type;

use super::FunGeneralization;
use super::conversions::{type_expr_to_infer, type_expr_to_infer_with_generics};

/// Register the names of all direct `FunDecl`s in `decls` with fresh type
/// variables so that forward references and mutual recursion work.
pub(super) fn hoist_fun_decls(decls: &[Decl], ctx: &mut InferContext) {
    for decl in decls {
        if let Decl::Fun(fun) = decl {
            if fun.generics.is_empty() {
                let fresh = ctx.fresh_var();
                ctx.bind_mono(&fun.name, fresh, false);
            }
        }
    }
}

pub(super) fn infer_program(
    program: &Program,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<(), MoonlaneError> {
    for decl in &program.decls {
        infer_decl(decl, ctx, fun_generalizations)?;
    }
    Ok(())
}

fn infer_decl(
    decl: &Decl,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    match decl {
        Decl::Let(ld) => {
            let env_fvs = ctx.env_free_vars();
            let val_ty = infer_expr(&ld.value, ctx, fun_generalizations)?;
            if let Some(ann) = &ld.type_ann {
                ctx.add_constraint(val_ty.clone(), type_expr_to_infer(ann), ld.span.clone());
            }
            // Let-polymorphism: generalize unannotated closure-valued let bindings.
            // If the resolved type still has free variables, they are quantified into a
            // polymorphic scheme so each call site gets a fresh instantiation.
            if matches!(&ld.value, Expr::Closure { .. }) && ld.type_ann.is_none() {
                let partial_subst = ctx.solve()?;
                let resolved_ty = partial_subst.apply(&val_ty);
                let scheme = generalize(resolved_ty.clone(), &env_fvs);
                if !scheme.quantified_vars.is_empty() {
                    ctx.bind_poly(&ld.name, scheme);
                    fun_generalizations.push(FunGeneralization {
                        name:    ld.name.clone(),
                        fun_ty:  resolved_ty,
                        env_fvs,
                    });
                    return Ok(InferType::unit());
                }
            }
            ctx.bind_mono(&ld.name, val_ty, false);
            Ok(InferType::unit())
        }
        Decl::Mut(md) => {
            let val_ty = infer_expr(&md.value, ctx, fun_generalizations)?;
            if let Some(ann) = &md.type_ann {
                ctx.add_constraint(val_ty.clone(), type_expr_to_infer(ann), md.span.clone());
            }
            ctx.bind_mono(&md.name, val_ty, true);
            Ok(InferType::unit())
        }
        Decl::Fun(fd) => { infer_fun_decl(fd, ctx, fun_generalizations)?; Ok(InferType::unit()) }
        Decl::Struct(_) | Decl::Enum(_) | Decl::Trait(_) => Ok(InferType::unit()),
        Decl::Impl(ib) => {
            if ib.trait_name.is_some() {
                return Err(MoonlaneError::internal("trait impl blocks not yet supported"));
            }
            let target_name = match &ib.target_type {
                TypeExpr::Named(name, args) if args.is_empty() => name.clone(),
                _ => return Err(MoonlaneError::internal("generic impl blocks not yet supported")),
            };
            for method in &ib.methods {
                infer_impl_method(method, &target_name, ctx, fun_generalizations)?;
            }
            Ok(InferType::unit())
        }
        Decl::Stmt(stmt) => infer_stmt(stmt, ctx, fun_generalizations),
    }
}

fn infer_fun_decl(
    fun: &FunDecl,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<(), MoonlaneError> {
    // For generic functions, create fresh type variables for each parameter name.
    let generic_map: HashMap<String, TypeVar> = fun.generics.iter()
        .map(|g| (g.name.clone(), ctx.fresh_type_var_raw()))
        .collect();

    let te_to_infer = |te: &TypeExpr| -> InferType {
        if generic_map.is_empty() {
            type_expr_to_infer(te)
        } else {
            type_expr_to_infer_with_generics(te, &generic_map)
        }
    };

    let param_types: Vec<InferType> = fun.params.iter().map(|p| {
        if let Some(ann) = &p.type_ann { te_to_infer(ann) } else { ctx.fresh_var() }
    }).collect();

    let ret_ty = if let Some(ann) = &fun.return_type {
        te_to_infer(ann)
    } else {
        ctx.fresh_var()
    };

    let env_fvs = ctx.env_free_vars();

    ctx.push_scope();
    for (param, pt) in fun.params.iter().zip(param_types.iter()) {
        ctx.bind_mono(&param.name, pt.clone(), false);
    }

    let saved_ret = ctx.push_return_type(ret_ty.clone());
    let body_ty = infer_block(&fun.body, ctx, fun_generalizations)?;

    ctx.add_constraint(body_ty, ret_ty.clone(), fun.body.span.clone());

    ctx.pop_return_type(saved_ret);
    ctx.pop_scope();

    let fun_ty = InferType::Fun(param_types, Box::new(ret_ty));

    if let Some(pre_reg) = ctx.lookup(&fun.name) {
        ctx.add_constraint(pre_reg, fun_ty.clone(), fun.span.clone());
    }

    // Inline solve-and-generalize: future call sites look up this function via the
    // poly_env and get a fresh instantiation per call, avoiding constraint conflicts
    // when the same polymorphic function is called at different types.
    let partial_subst = ctx.solve()?;
    let resolved_ty = partial_subst.apply(&fun_ty);
    let scheme = generalize(resolved_ty, &env_fvs);
    ctx.bind_poly(&fun.name, scheme);

    fun_generalizations.push(FunGeneralization { name: fun.name.clone(), fun_ty, env_fvs });
    Ok(())
}

fn infer_impl_method(
    method: &FunDecl,
    target_name: &str,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<(), MoonlaneError> {
    let generic_map: HashMap<String, TypeVar> = method.generics.iter()
        .map(|g| (g.name.clone(), ctx.fresh_type_var_raw()))
        .collect();

    let te_to_infer = |te: &TypeExpr| -> InferType {
        if generic_map.is_empty() {
            type_expr_to_infer(te)
        } else {
            type_expr_to_infer_with_generics(te, &generic_map)
        }
    };

    let self_ty = InferType::Named(target_name.to_string(), vec![]);
    let param_types: Vec<InferType> = method.params.iter().map(|p| {
        if p.name == "self" {
            self_ty.clone()
        } else if let Some(ann) = &p.type_ann {
            te_to_infer(ann)
        } else {
            ctx.fresh_var()
        }
    }).collect();
    let ret_ty = method.return_type.as_ref()
        .map(|ann| te_to_infer(ann))
        .unwrap_or_else(InferType::unit);

    ctx.push_scope();
    for (p, pt) in method.params.iter().zip(param_types.iter()) {
        ctx.bind_mono(&p.name, pt.clone(), p.mutable);
    }
    let saved_ret = ctx.push_return_type(ret_ty.clone());
    let body_ty = infer_block(&method.body, ctx, fun_generalizations)?;
    ctx.add_constraint(body_ty, ret_ty.clone(), method.body.span.clone());
    ctx.pop_return_type(saved_ret);
    ctx.pop_scope();

    let partial_subst = ctx.solve()?;
    let fun_ty = InferType::Fun(param_types, Box::new(ret_ty));
    let resolved_fun_ty = partial_subst.apply(&fun_ty);
    ctx.register_method(target_name.to_string(), method.name.clone(), resolved_fun_ty);
    Ok(())
}

fn infer_block(
    block: &Block,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    ctx.push_scope();
    ctx.push_struct_scope();
    // Hoist struct/enum declarations defined in this block before inferring any stmt,
    // so they can be referenced anywhere within the block regardless of order.
    for decl in &block.stmts {
        match decl {
            Decl::Struct(sd) => {
                let fields = sd.fields.iter()
                    .map(|f| (f.name.clone(), type_expr_to_infer(&f.type_ann)))
                    .collect();
                ctx.register_struct_fields(sd.name.clone(), fields);
            }
            Decl::Enum(ed) => {
                let variants = ed.variants.iter().map(|v| VariantInfo {
                    name: v.name.clone(),
                    fields: v.fields.iter()
                        .map(|f| (f.name.clone(), type_expr_to_infer(&f.type_ann)))
                        .collect(),
                }).collect();
                ctx.register_enum(ed.name.clone(), EnumInfo { type_params: vec![], variants });
            }
            _ => {}
        }
    }
    hoist_fun_decls(&block.stmts, ctx);
    let mut last_stmt_ty = InferType::unit();
    for stmt in &block.stmts {
        last_stmt_ty = infer_decl(stmt, ctx, fun_generalizations)?;
    }
    let ty = match &block.tail {
        Some(tail) => infer_expr(tail, ctx, fun_generalizations)?,
        None       => last_stmt_ty,
    };
    ctx.pop_struct_scope();
    ctx.pop_scope();
    Ok(ty)
}

fn infer_stmt(
    stmt: &Stmt,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    match stmt {
        Stmt::Expr(e) => { infer_expr(e, ctx, fun_generalizations)?; Ok(InferType::unit()) }
        Stmt::Return(r) => {
            let ret_ty = match &r.value {
                Some(e) => infer_expr(e, ctx, fun_generalizations)?,
                None    => InferType::unit(),
            };
            if let Some(expected) = ctx.current_return_type().cloned() {
                ctx.add_constraint(ret_ty, expected, r.span.clone());
            }
            Ok(InferType::never())
        }
        Stmt::Break(bs) => {
            let break_ty = match &bs.value {
                Some(e) => infer_expr(e, ctx, fun_generalizations)?,
                None    => InferType::unit(),
            };
            if let Some(expected) = ctx.current_break_type().cloned() {
                ctx.add_constraint(break_ty, expected, bs.span.clone());
            }
            Ok(InferType::never())
        }
        Stmt::Continue(_) => Ok(InferType::never()),
        Stmt::While(ws) => {
            let cond_ty = infer_expr(&ws.condition, ctx, fun_generalizations)?;
            ctx.add_constraint(cond_ty, InferType::bool(), ws.span.clone());
            infer_block(&ws.body, ctx, fun_generalizations)?;
            Ok(InferType::unit())
        }
        Stmt::For(fs) => {
            ctx.push_scope();
            if let Some(init) = &fs.init {
                match init {
                    ForInit::Mut(md) => {
                        let val_ty = infer_expr(&md.value, ctx, fun_generalizations)?;
                        if let Some(ann) = &md.type_ann {
                            ctx.add_constraint(val_ty.clone(), type_expr_to_infer(ann), md.span.clone());
                        }
                        ctx.bind_mono(&md.name, val_ty, true);
                    }
                    ForInit::Expr(e) => { infer_expr(e, ctx, fun_generalizations)?; }
                }
            }
            if let Some(cond) = &fs.condition {
                let cond_ty = infer_expr(cond, ctx, fun_generalizations)?;
                ctx.add_constraint(cond_ty, InferType::bool(), fs.span.clone());
            }
            if let Some(step) = &fs.step {
                infer_expr(step, ctx, fun_generalizations)?;
            }
            infer_block(&fs.body, ctx, fun_generalizations)?;
            ctx.pop_scope();
            Ok(InferType::unit())
        }
        Stmt::ForIn(fi) => {
            let iter_ty = infer_expr(&fi.iterable, ctx, fun_generalizations)?;
            let elem_ty = ctx.fresh_var();
            let iter_var = ctx.fresh_var();
            let partial = ctx.solve()?;
            let resolved_iter = partial.apply(&iter_ty);
            match &resolved_iter {
                InferType::Array(elem) => {
                    ctx.add_constraint(elem_ty.clone(), *elem.clone(), fi.span.clone());
                }
                InferType::Named(name, args) if name == "Range" && args.len() == 1 => {
                    ctx.add_constraint(elem_ty.clone(), InferType::int(), fi.span.clone());
                }
                InferType::Var(_) => {
                    ctx.add_constraint(iter_ty, InferType::Array(Box::new(elem_ty.clone())), fi.span.clone());
                }
                _ => {
                    return Err(MoonlaneError::type_error(
                        TypeErrorCode::T0001,
                        format!("expected array or range in for-in, got `{resolved_iter}`"),
                        &fi.span,
                    ));
                }
            }
            let iter_var_span = fi.span.clone();
            let _ = iter_var;
            ctx.push_scope();
            ctx.bind_mono(&fi.binding, elem_ty, false);
            infer_block(&fi.body, ctx, fun_generalizations)?;
            ctx.pop_scope();
            let _ = iter_var_span;
            Ok(InferType::unit())
        }
    }
}

fn infer_expr(
    expr: &Expr,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    match expr {
        Expr::Literal(lit, _)          => Ok(infer_literal(lit, ctx)),
        Expr::Ident(name, span)        => {
            ctx.lookup(name).ok_or_else(|| MoonlaneError::type_error(
                TypeErrorCode::T0003,
                format!("undefined name `{name}`"),
                span,
            ))
        }
        Expr::BinOp(lhs, op, rhs, span) => infer_binop(lhs, op, rhs, span, ctx, fun_generalizations),
        Expr::UnaryOp(op, operand, span) => infer_unaryop(op, operand, span, ctx, fun_generalizations),
        Expr::Tuple(elems, _) => {
            let elem_tys: Vec<InferType> = elems.iter()
                .map(|e| infer_expr(e, ctx, fun_generalizations))
                .collect::<Result<_, _>>()?;
            Ok(InferType::Tuple(elem_tys))
        }
        Expr::Array(elems, span) => {
            if elems.is_empty() {
                return Ok(InferType::Array(Box::new(ctx.fresh_var())));
            }
            let first_ty = infer_expr(&elems[0], ctx, fun_generalizations)?;
            for elem in &elems[1..] {
                let ty = infer_expr(elem, ctx, fun_generalizations)?;
                ctx.add_constraint(ty, first_ty.clone(), span.clone());
            }
            Ok(InferType::Array(Box::new(first_ty)))
        }
        Expr::Call { callee, args, span } => {
            let callee_ty = infer_expr(callee, ctx, fun_generalizations)?;
            let arg_tys: Vec<InferType> = args.iter()
                .map(|a| infer_expr(a, ctx, fun_generalizations))
                .collect::<Result<_, _>>()?;
            if let InferType::Fun(params, _) = &callee_ty {
                if params.len() != arg_tys.len() {
                    return Err(MoonlaneError::type_error(
                        TypeErrorCode::T0004,
                        format!("expected {} argument(s), got {}", params.len(), arg_tys.len()),
                        span,
                    ));
                }
            }
            let ret_var = ctx.fresh_var();
            ctx.add_constraint(callee_ty, InferType::Fun(arg_tys, Box::new(ret_var.clone())), span.clone());
            Ok(ret_var)
        }
        Expr::Index { object, index, span } => {
            let obj_ty   = infer_expr(object, ctx, fun_generalizations)?;
            let idx_ty   = infer_expr(index,  ctx, fun_generalizations)?;
            ctx.add_constraint(idx_ty, InferType::int(), span.clone());
            let elem_var = ctx.fresh_var();
            ctx.add_constraint(obj_ty, InferType::Array(Box::new(elem_var.clone())), span.clone());
            Ok(elem_var)
        }
        Expr::If { condition, then_branch, else_branch, span } => {
            let cond_ty = infer_expr(condition, ctx, fun_generalizations)?;
            ctx.add_constraint(cond_ty, InferType::bool(), span.clone());
            let then_ty = infer_block(then_branch, ctx, fun_generalizations)?;
            match else_branch {
                Some(else_block) => {
                    let else_ty = infer_block(else_block, ctx, fun_generalizations)?;
                    ctx.add_constraint(then_ty.clone(), else_ty, span.clone());
                    Ok(then_ty)
                }
                None => {
                    ctx.add_constraint(then_ty, InferType::unit(), span.clone());
                    Ok(InferType::unit())
                }
            }
        }
        Expr::Assign { target, op, value, span } => {
            let target_ty = match target {
                AssignTarget::Ident(name, target_span) => {
                    ctx.lookup_for_write(name, target_span)?
                }
                AssignTarget::Index { object, index, span: target_span } => {
                    let obj_ty   = infer_expr(object, ctx, fun_generalizations)?;
                    let idx_ty   = infer_expr(index,  ctx, fun_generalizations)?;
                    ctx.add_constraint(idx_ty, InferType::int(), target_span.clone());
                    let elem_var = ctx.fresh_var();
                    ctx.add_constraint(obj_ty, InferType::Array(Box::new(elem_var.clone())), target_span.clone());
                    elem_var
                }
                AssignTarget::FieldAccess { object, field, span: target_span } => {
                    infer_field_assign_type(object, field, target_span, ctx, fun_generalizations)?
                }
            };
            let value_ty = infer_expr(value, ctx, fun_generalizations)?;
            match op {
                AssignOp::Assign => {
                    ctx.add_constraint(target_ty, value_ty, span.clone());
                }
                AssignOp::AddAssign | AssignOp::SubAssign
                | AssignOp::MulAssign | AssignOp::DivAssign | AssignOp::RemAssign => {
                    let result = ctx.fresh_var();
                    ctx.add_constraint(target_ty, result.clone(), span.clone());
                    ctx.add_constraint(value_ty, result, span.clone());
                }
            }
            Ok(InferType::unit())
        }
        Expr::FieldAccess { object, field, span } => {
            let obj_ty = infer_expr(object, ctx, fun_generalizations)?;
            let obj_ty = ctx.solve()?.apply(&obj_ty);
            let struct_name = named_type_name(&obj_ty).ok_or_else(|| MoonlaneError::type_error(
                TypeErrorCode::T0002,
                "cannot infer struct type for field access; add a type annotation",
                span,
            ))?;
            let type_args = if let InferType::Named(_, args) = &obj_ty { args.clone() } else { vec![] };
            let fields = ctx.get_struct_fields(&struct_name)
                .ok_or_else(|| MoonlaneError::type_error(
                    TypeErrorCode::T0003,
                    format!("unknown type `{struct_name}`"),
                    span,
                ))?
                .clone();
            let raw_ty = fields.iter()
                .find(|(n, _)| n == field)
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| MoonlaneError::type_error(
                    TypeErrorCode::T0003,
                    format!("no field `{field}` on `{struct_name}`"),
                    span,
                ))?;
            // For generic structs, substitute declared type params with the resolved args.
            if let Some(type_params) = ctx.get_struct_type_params(&struct_name).cloned() {
                let mut remap = Substitution::new();
                for (&tp, arg) in type_params.iter().zip(type_args.iter()) {
                    remap.bind(tp, arg.clone());
                }
                Ok(remap.apply(&raw_ty))
            } else {
                Ok(raw_ty)
            }
        }
        Expr::MethodCall { receiver, method, args, span } => {
            let recv_ty = infer_expr(receiver, ctx, fun_generalizations)?;
            let recv_ty = ctx.solve()?.apply(&recv_ty);
            let struct_name = named_type_name(&recv_ty).ok_or_else(|| MoonlaneError::type_error(
                TypeErrorCode::T0002,
                "cannot infer receiver type for method call; add a type annotation",
                span,
            ))?;
            let method_ty = ctx.get_method_type(&struct_name, method)
                .cloned()
                .ok_or_else(|| MoonlaneError::type_error(
                    TypeErrorCode::T0003,
                    format!("no method `{method}` on `{struct_name}`"),
                    span,
                ))?;
            let arg_tys: Vec<InferType> = args.iter()
                .map(|a| infer_expr(a, ctx, fun_generalizations))
                .collect::<Result<_, _>>()?;
            let ret_var = ctx.fresh_var();
            let expected = InferType::Fun(
                std::iter::once(recv_ty).chain(arg_tys).collect(),
                Box::new(ret_var.clone()),
            );
            ctx.add_constraint(method_ty, expected, span.clone());
            Ok(ret_var)
        }
        Expr::StructLiteral { path, fields, span } => {
            if path.len() == 2 {
                infer_enum_variant_literal(&path[0], &path[1], fields, span, ctx, fun_generalizations)
            } else {
                let struct_name = path.last()
                    .ok_or_else(|| MoonlaneError::internal("empty path in struct literal"))?
                    .clone();
                infer_struct_literal(struct_name, fields, span, ctx, fun_generalizations)
            }
        }
        Expr::Ascribe { expr, ann, span } => {
            let inner_ty = infer_expr(expr, ctx, fun_generalizations)?;
            let ascribed_ty = type_expr_to_infer(ann);
            ctx.add_constraint(inner_ty.clone(), ascribed_ty, span.clone());
            Ok(inner_ty)
        }

        Expr::Cast { expr, target_type, span } => {
            let source_ty = infer_expr(expr, ctx, fun_generalizations)?;
            let target_ty = type_expr_to_infer(target_type);
            let source_resolved = ctx.solve()?.apply(&source_ty);
            let target_resolved = ctx.solve()?.apply(&target_ty);
            let valid = matches!(
                (&source_resolved, &target_resolved),
                (InferType::Concrete(Type::Int),   InferType::Concrete(Type::Float))
                | (InferType::Concrete(Type::Int),   InferType::Concrete(Type::Int))
                | (InferType::Concrete(Type::Float), InferType::Concrete(Type::Float))
            );
            if !valid {
                return Err(MoonlaneError::type_error(
                    TypeErrorCode::T0007,
                    format!("cannot cast `{source_resolved}` to `{target_resolved}` — only `Int as Float` and identity casts are supported"),
                    span,
                ));
            }
            Ok(target_ty)
        }
        Expr::TupleAccess { object, index, span } => {
            let obj_ty = infer_expr(object, ctx, fun_generalizations)?;
            let obj_ty = ctx.solve()?.apply(&obj_ty);
            match &obj_ty {
                InferType::Tuple(elems) => {
                    elems.get(*index).cloned().ok_or_else(|| MoonlaneError::type_error(
                        TypeErrorCode::T0003,
                        format!("tuple index {index} out of bounds (tuple has {} elements)", elems.len()),
                        span,
                    ))
                }
                _ => Err(MoonlaneError::type_error(
                    TypeErrorCode::T0002,
                    "cannot infer tuple type for index access; add a type annotation",
                    span,
                )),
            }
        }
        Expr::Loop { body, span } => {
            let break_var = ctx.fresh_var();
            let saved_break = ctx.push_break_type(break_var.clone());
            infer_block(body, ctx, fun_generalizations)?;
            ctx.pop_break_type(saved_break);
            let _ = span;
            Ok(break_var)
        }
        Expr::Path(segments, span) => {
            let [type_name, member_name] = segments.as_slice() else {
                return Err(MoonlaneError::type_error(
                    TypeErrorCode::T0003,
                    format!("unresolved path `{}`", segments.join("::")),
                    span,
                ));
            };
            if let Some(fun_ty) = ctx.get_method_type(type_name, member_name).cloned() {
                return Ok(fun_ty);
            }
            if let Some(info) = ctx.get_enum(type_name).cloned() {
                if let Some(variant) = info.variants.iter().find(|v| v.name == *member_name) {
                    if variant.fields.is_empty() {
                        let type_args: Vec<InferType> = info.type_params.iter()
                            .map(|_| ctx.fresh_var())
                            .collect();
                        return Ok(InferType::Named(type_name.clone(), type_args));
                    }
                }
            }
            Err(MoonlaneError::type_error(
                TypeErrorCode::T0003,
                format!("no member `{member_name}` on type `{type_name}`"),
                span,
            ))
        }
        Expr::Closure { params, return_type, body, .. } => {
            let param_types: Vec<InferType> = params.iter().map(|p| {
                if let Some(ann) = &p.type_ann { type_expr_to_infer(ann) } else { ctx.fresh_var() }
            }).collect();
            let ret_ty = return_type.as_ref()
                .map(type_expr_to_infer)
                .unwrap_or_else(|| ctx.fresh_var());
            ctx.push_scope();
            for (p, pt) in params.iter().zip(param_types.iter()) {
                ctx.bind_mono(&p.name, pt.clone(), p.mutable);
            }
            let saved_ret = ctx.push_return_type(ret_ty.clone());
            let body_ty = infer_block(body, ctx, fun_generalizations)?;
            ctx.add_constraint(body_ty, ret_ty.clone(), body.span.clone());
            ctx.pop_return_type(saved_ret);
            ctx.pop_scope();
            Ok(InferType::Fun(param_types, Box::new(ret_ty)))
        }
        Expr::PropagateError { expr, span } => {
            let inner_ty = infer_expr(expr, ctx, fun_generalizations)?;
            let ok_var  = ctx.fresh_var();
            let err_var = ctx.fresh_var();
            ctx.add_constraint(
                inner_ty,
                InferType::Named("Result".to_string(), vec![ok_var.clone(), err_var.clone()]),
                span.clone(),
            );
            if let Some(fn_ret) = ctx.current_return_type().cloned() {
                let fn_ok_var = ctx.fresh_var();
                ctx.add_constraint(
                    fn_ret,
                    InferType::Named("Result".to_string(), vec![fn_ok_var, err_var]),
                    span.clone(),
                );
            }
            Ok(ok_var)
        }
        Expr::Match(m) => infer_match(m, ctx, fun_generalizations),
    }
}

fn infer_match(
    m: &MatchExpr,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    let scrutinee_ty = infer_expr(&m.scrutinee, ctx, fun_generalizations)?;
    let result_var = ctx.fresh_var();
    for arm in &m.arms {
        ctx.push_scope();
        infer_pattern(&arm.pattern, &scrutinee_ty, ctx)?;
        if let Some(guard) = &arm.guard {
            let g = infer_expr(guard, ctx, fun_generalizations)?;
            ctx.add_constraint(g, InferType::bool(), arm.span.clone());
        }
        let arm_ty = infer_block(&arm.body, ctx, fun_generalizations)?;
        ctx.add_constraint(arm_ty, result_var.clone(), arm.span.clone());
        ctx.pop_scope();
    }
    Ok(result_var)
}

fn infer_pattern(
    pattern: &Pattern,
    scrutinee_ty: &InferType,
    ctx: &mut InferContext,
) -> Result<(), MoonlaneError> {
    let span = pattern_span(pattern);
    match pattern {
        Pattern::Wildcard(_) => {}
        Pattern::Literal(lit, _) => {
            let lit_ty = infer_literal(lit, ctx);
            ctx.add_constraint(scrutinee_ty.clone(), lit_ty, span.clone());
        }
        Pattern::Binding(name, _) => {
            ctx.bind_mono(name, scrutinee_ty.clone(), false);
        }
        Pattern::Nope(_) => {
            let fresh = ctx.fresh_var();
            ctx.add_constraint(
                scrutinee_ty.clone(),
                InferType::Named("Perhaps".to_string(), vec![fresh]),
                span.clone(),
            );
        }
        Pattern::Tuple(pats, _) => {
            let elem_vars: Vec<InferType> = pats.iter().map(|_| ctx.fresh_var()).collect();
            ctx.add_constraint(
                scrutinee_ty.clone(),
                InferType::Tuple(elem_vars.clone()),
                span.clone(),
            );
            for (pat, elem_ty) in pats.iter().zip(elem_vars.iter()) {
                infer_pattern(pat, elem_ty, ctx)?;
            }
        }
        Pattern::EnumVariant { path, fields, span: pat_span } => {
            let [enum_name, variant_name] = path.as_slice() else {
                return Err(MoonlaneError::type_error(
                    TypeErrorCode::T0003,
                    format!("unresolved pattern path `{}`", path.join("::")),
                    pat_span,
                ));
            };
            infer_enum_variant_pattern(enum_name, variant_name, fields, scrutinee_ty, pat_span, ctx)?;
        }
    }
    Ok(())
}

fn pattern_span(pattern: &Pattern) -> &Span {
    match pattern {
        Pattern::Wildcard(s) | Pattern::Nope(s) | Pattern::Binding(_, s)
        | Pattern::Literal(_, s) | Pattern::Tuple(_, s)
        | Pattern::EnumVariant { span: s, .. } => s,
    }
}

fn named_type_name(ty: &InferType) -> Option<String> {
    match ty {
        InferType::Named(name, _)         => Some(name.clone()),
        InferType::Concrete(Type::Str)    => Some("String".to_string()),
        InferType::Concrete(Type::Int)    => Some("Int".to_string()),
        InferType::Concrete(Type::Float)  => Some("Float".to_string()),
        InferType::Concrete(Type::Bool)   => Some("Bool".to_string()),
        _ => None,
    }
}

fn infer_literal(lit: &Literal, ctx: &mut InferContext) -> InferType {
    match lit {
        Literal::Int(_)   => InferType::int(),
        Literal::Float(_) => InferType::float(),
        Literal::Bool(_)  => InferType::bool(),
        Literal::Str(_)   => InferType::str(),
        Literal::Unit     => InferType::unit(),
        Literal::Nope     => InferType::Named("Perhaps".to_string(), vec![ctx.fresh_var()]),
    }
}

fn infer_binop(
    lhs: &Expr,
    op: &BinOp,
    rhs: &Expr,
    span: &Span,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    let lhs_ty = infer_expr(lhs, ctx, fun_generalizations)?;
    let rhs_ty = infer_expr(rhs, ctx, fun_generalizations)?;
    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
            let result = ctx.fresh_var();
            ctx.add_constraint(lhs_ty, result.clone(), span.clone());
            ctx.add_constraint(rhs_ty, result.clone(), span.clone());
            Ok(result)
        }
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            ctx.add_constraint(lhs_ty, rhs_ty, span.clone());
            Ok(InferType::bool())
        }
        BinOp::And | BinOp::Or => {
            ctx.add_constraint(lhs_ty, InferType::bool(), span.clone());
            ctx.add_constraint(rhs_ty, InferType::bool(), span.clone());
            Ok(InferType::bool())
        }
        BinOp::Range | BinOp::RangeInclusive => {
            ctx.add_constraint(lhs_ty, InferType::int(), span.clone());
            ctx.add_constraint(rhs_ty, InferType::int(), span.clone());
            Ok(InferType::Named("Range".to_string(), vec![InferType::int()]))
        }
    }
}

fn infer_unaryop(
    op: &UnaryOp,
    operand: &Expr,
    span: &Span,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    let ty = infer_expr(operand, ctx, fun_generalizations)?;
    match op {
        UnaryOp::Neg => Ok(ty),
        UnaryOp::Not => {
            ctx.add_constraint(ty, InferType::bool(), span.clone());
            Ok(InferType::bool())
        }
    }
}

fn infer_enum_variant_literal(
    enum_name: &str,
    variant_name: &str,
    fields: &[(String, Expr)],
    span: &Span,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    let enum_info = ctx.get_enum(enum_name)
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
            format!("unknown enum `{enum_name}`"),
            span,
        ))?
        .clone();
    let variant = enum_info.variants.iter()
        .find(|v| v.name == variant_name)
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
            format!("no variant `{variant_name}` on enum `{enum_name}`"),
            span,
        ))?
        .clone();
    let mut remap: HashMap<TypeVar, InferType> = HashMap::new();
    for &tp in &enum_info.type_params {
        remap.insert(tp, ctx.fresh_var());
    }
    for (fname, expr) in fields {
        let raw_ty = variant.fields.iter()
            .find(|(n, _)| n == fname)
            .map(|(_, ty)| ty)
            .ok_or_else(|| MoonlaneError::type_error(
                TypeErrorCode::T0003,
                format!("no field `{fname}` on `{enum_name}::{variant_name}`"),
                span,
            ))?;
        let decl_ty = match raw_ty {
            InferType::Var(v) => remap.get(v).cloned().unwrap_or_else(|| raw_ty.clone()),
            other => other.clone(),
        };
        let expr_ty = infer_expr(expr, ctx, fun_generalizations)?;
        ctx.add_constraint(expr_ty, decl_ty, span.clone());
    }
    let type_args: Vec<InferType> = enum_info.type_params.iter()
        .map(|tp| remap[tp].clone())
        .collect();
    Ok(InferType::Named(enum_name.to_string(), type_args))
}

fn infer_struct_literal(
    struct_name: String,
    fields: &[(String, Expr)],
    span: &Span,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    let expected_fields = ctx.get_struct_fields(&struct_name)
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
            format!("unknown struct `{struct_name}`"),
            span,
        ))?
        .clone();
    // For generic structs, create fresh type vars and remap declared TypeVars.
    let type_params = ctx.get_struct_type_params(&struct_name).cloned();
    let mut remap: HashMap<TypeVar, InferType> = HashMap::new();
    if let Some(ref params) = type_params {
        for &tp in params {
            remap.insert(tp, ctx.fresh_var());
        }
    }
    let apply_remap = |ty: &InferType| -> InferType {
        if remap.is_empty() { return ty.clone(); }
        match ty {
            InferType::Var(v) => remap.get(v).cloned().unwrap_or_else(|| ty.clone()),
            other => other.clone(),
        }
    };
    for (name, expr) in fields {
        let raw_ty = expected_fields.iter()
            .find(|(n, _)| n == name)
            .map(|(_, ty)| ty)
            .ok_or_else(|| MoonlaneError::type_error(
                TypeErrorCode::T0003,
                format!("no field `{name}` on `{struct_name}`"),
                span,
            ))?;
        let decl_ty = apply_remap(raw_ty);
        let expr_ty = infer_expr(expr, ctx, fun_generalizations)?;
        ctx.add_constraint(expr_ty, decl_ty, span.clone());
    }
    for (decl_name, _) in &expected_fields {
        if !fields.iter().any(|(n, _)| n == decl_name) {
            return Err(MoonlaneError::type_error(
                TypeErrorCode::T0003,
                format!("missing field `{decl_name}` in `{struct_name}`"),
                span,
            ));
        }
    }
    let type_args: Vec<InferType> = type_params.as_deref().unwrap_or(&[])
        .iter().map(|tp| remap[tp].clone()).collect();
    Ok(InferType::Named(struct_name, type_args))
}

fn infer_field_assign_type(
    object: &Expr,
    field: &str,
    target_span: &Span,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, MoonlaneError> {
    let obj_ty = infer_expr(object, ctx, fun_generalizations)?;
    let obj_ty = ctx.solve()?.apply(&obj_ty);
    let struct_name = named_type_name(&obj_ty).ok_or_else(|| {
        MoonlaneError::type_error(
            TypeErrorCode::T0002,
            "cannot infer struct type for field assignment; add a type annotation",
            target_span,
        )
    })?;
    let type_args = if let InferType::Named(_, args) = &obj_ty { args.clone() } else { vec![] };
    let fields = ctx.get_struct_fields(&struct_name)
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
            format!("unknown type `{struct_name}`"),
            target_span,
        ))?
        .clone();
    let raw_ty = fields.iter()
        .find(|(n, _)| n == field)
        .map(|(_, ty)| ty.clone())
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
            format!("no field `{field}` on `{struct_name}`"),
            target_span,
        ))?;
    if let Some(type_params) = ctx.get_struct_type_params(&struct_name).cloned() {
        let mut remap = Substitution::new();
        for (&tp, arg) in type_params.iter().zip(type_args.iter()) {
            remap.bind(tp, arg.clone());
        }
        Ok(remap.apply(&raw_ty))
    } else {
        Ok(raw_ty)
    }
}

fn infer_enum_variant_pattern(
    enum_name: &str,
    variant_name: &str,
    fields: &[String],
    scrutinee_ty: &InferType,
    pat_span: &Span,
    ctx: &mut InferContext,
) -> Result<(), MoonlaneError> {
    let enum_info = ctx.get_enum(enum_name)
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
            format!("unknown enum `{enum_name}` in pattern"),
            pat_span,
        ))?
        .clone();
    let variant = enum_info.variants.iter()
        .find(|v| v.name == variant_name)
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
            format!("no variant `{variant_name}` on `{enum_name}`"),
            pat_span,
        ))?
        .clone();
    let mut remap: HashMap<TypeVar, InferType> = HashMap::new();
    for &tp in &enum_info.type_params {
        remap.insert(tp, ctx.fresh_var());
    }
    let type_args: Vec<InferType> = enum_info.type_params.iter()
        .map(|tp| remap[tp].clone())
        .collect();
    ctx.add_constraint(
        scrutinee_ty.clone(),
        InferType::Named(enum_name.to_string(), type_args),
        pat_span.clone(),
    );
    for field_name in fields {
        let raw_ty = variant.fields.iter()
            .find(|(n, _)| n == field_name)
            .map(|(_, ty)| ty)
            .ok_or_else(|| MoonlaneError::type_error(
                TypeErrorCode::T0003,
                format!("no field `{field_name}` on `{enum_name}::{variant_name}`"),
                pat_span,
            ))?;
        let field_ty = match raw_ty {
            InferType::Var(v) => remap.get(v).cloned().unwrap_or_else(|| raw_ty.clone()),
            other => other.clone(),
        };
        ctx.bind_mono(field_name, field_ty, false);
    }
    Ok(())
}
