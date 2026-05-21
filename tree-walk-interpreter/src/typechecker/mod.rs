use std::collections::HashMap;
use std::collections::HashSet;

use crate::ast::*;
use crate::error::{ErrorCode, YoloscriptError};
use crate::typed_ast::*;
use crate::typeinference::*;
use crate::types::Type;

type SchemeEnv = HashMap<String, TypeScheme>;

fn array_push_scheme(t: TypeVar) -> TypeScheme {
    TypeScheme {
        quantified_vars: vec![t],
        ty: InferType::Fun(
            vec![InferType::Array(Box::new(InferType::Var(t))), InferType::Var(t)],
            Box::new(InferType::unit()),
        ),
    }
}

fn array_len_scheme(t: TypeVar) -> TypeScheme {
    TypeScheme {
        quantified_vars: vec![t],
        ty: InferType::Fun(
            vec![InferType::Array(Box::new(InferType::Var(t)))],
            Box::new(InferType::int()),
        ),
    }
}

/// Run the type checker over an untyped AST, producing a fully typed AST.
pub fn check(program: Program) -> Result<TypedProgram, YoloscriptError> {
    // Pre-pass: build the type registry, then create the inference context.
    let mut gen = TypeVarGenerator::new();
    let registry = build_registry(&program, &mut gen);
    let mut ctx = InferContext::new(registry, gen);

    // Pre-pass: register built-in value bindings and hoist function names.
    register_builtins(&mut ctx);
    hoist_fun_decls(&program.decls, &mut ctx);

    // Pass 1: walk AST, emit constraints, collect function generalizations.
    let mut fun_generalizations: Vec<FunGeneralization> = vec![];
    infer_program(&program, &mut ctx, &mut fun_generalizations)?;
    let subst = ctx.solve()?;

    // Build SchemeEnv from user functions, then add polymorphic built-in schemes.
    // Hand off the generator so all remaining TypeVar allocations are globally unique.
    let mut gen = ctx.split_gen();
    let mut scheme_env: SchemeEnv = HashMap::new();
    for fg in fun_generalizations {
        let resolved = subst.apply(&fg.fun_ty);
        let scheme = generalize(resolved, &fg.env_fvs);
        scheme_env.insert(fg.name, scheme);
    }
    register_builtin_poly_schemes(&mut scheme_env, &mut gen);

    // Build concrete environments for Pass 2.
    let concrete_struct_env = build_concrete_struct_env(ctx.registry().raw_struct_env(), &subst)?;
    let concrete_method_env = build_concrete_method_env(ctx.registry().raw_method_env(), &subst)?;
    let enum_env = ctx.registry().raw_enum_env();

    // Pass 2: re-derive concrete types and build TypedAST.
    construct_program(&program, &subst, &scheme_env, concrete_struct_env, concrete_method_env, enum_env, gen)
}

fn register_builtins(ctx: &mut InferContext) {
    let str_ty   = InferType::str();
    let int_ty   = InferType::int();
    let float_ty = InferType::float();
    let bool_ty  = InferType::bool();
    let unit_ty  = InferType::unit();

    let mono = |params, ret| TypeScheme::mono(InferType::Fun(params, Box::new(ret)));

    ctx.bind_poly("print",           mono(vec![str_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("println",         mono(vec![str_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("int_to_string",   mono(vec![int_ty.clone()], str_ty.clone()));
    ctx.bind_poly("float_to_string", mono(vec![float_ty],       str_ty.clone()));
    ctx.bind_poly("bool_to_string",  mono(vec![bool_ty],        str_ty.clone()));
    ctx.bind_poly("string_len",      mono(vec![str_ty.clone()], int_ty.clone()));
    ctx.bind_poly("string_concat",   mono(vec![str_ty.clone(), str_ty.clone()], str_ty.clone()));
    ctx.bind_poly("clock",           mono(vec![], int_ty.clone()));

    // Built-in type methods.
    ctx.register_method("String".to_string(), "len".to_string(),
        InferType::Fun(vec![str_ty.clone()], Box::new(int_ty.clone())));

    let t = ctx.fresh_type_var_raw();
    ctx.bind_poly("array_push", array_push_scheme(t));
    let t = ctx.fresh_type_var_raw();
    ctx.bind_poly("array_len", array_len_scheme(t));
}

fn register_builtin_poly_schemes(scheme_env: &mut SchemeEnv, gen: &mut TypeVarGenerator) {
    let t = gen.fresh();
    scheme_env.insert("array_push".into(), array_push_scheme(t));
    let t = gen.fresh();
    scheme_env.insert("array_len".into(), array_len_scheme(t));
}

/// Build the `TypeRegistry` from the program's declarations and built-in types.
/// Allocates TypeVars from `gen`; the caller must pass the same `gen` to
/// `InferContext::new` so that all TypeVar IDs are globally unique.
fn build_registry(program: &Program, gen: &mut TypeVarGenerator) -> TypeRegistry {
    let mut registry = TypeRegistry::new();

    // Register built-in generic enums.
    let t = gen.fresh();
    registry.register_enum("Perhaps".into(), EnumInfo {
        type_params: vec![t],
        variants: vec![
            VariantInfo { name: "Some".into(), fields: vec![("value".into(), InferType::Var(t))] },
            VariantInfo { name: "Nope".into(), fields: vec![] },
        ],
    });
    let t = gen.fresh();
    let e = gen.fresh();
    registry.register_enum("Result".into(), EnumInfo {
        type_params: vec![t, e],
        variants: vec![
            VariantInfo { name: "Ok".into(),  fields: vec![("value".into(), InferType::Var(t))] },
            VariantInfo { name: "Err".into(), fields: vec![("error".into(), InferType::Var(e))] },
        ],
    });

    // Hoist user-defined structs, enums, and impl method signatures.
    for decl in &program.decls {
        match decl {
            Decl::Struct(sd) => {
                let fields = sd.fields.iter()
                    .map(|f| (f.name.clone(), type_expr_to_infer(&f.type_ann)))
                    .collect();
                registry.register_struct_fields(sd.name.clone(), fields);
            }
            Decl::Enum(ed) => {
                // v0.1: user-defined enums have no generic type params.
                let variants = ed.variants.iter().map(|v| VariantInfo {
                    name: v.name.clone(),
                    fields: v.fields.iter()
                        .map(|f| (f.name.clone(), type_expr_to_infer(&f.type_ann)))
                        .collect(),
                }).collect();
                registry.register_enum(ed.name.clone(), EnumInfo {
                    type_params: vec![],
                    variants,
                });
            }
            Decl::Impl(ib) if ib.trait_name.is_none() => {
                let target_name = match &ib.target_type {
                    TypeExpr::Named(name, args) if args.is_empty() => name.clone(),
                    _ => continue,
                };
                for method in &ib.methods {
                    let mut param_types = vec![];
                    for p in &method.params {
                        let pt = if p.name == "self" {
                            InferType::Named(target_name.clone(), vec![])
                        } else if let Some(ann) = &p.type_ann {
                            type_expr_to_infer(ann)
                        } else {
                            InferType::Var(gen.fresh())
                        };
                        param_types.push(pt);
                    }
                    let ret_ty = method.return_type.as_ref()
                        .map(type_expr_to_infer)
                        .unwrap_or_else(InferType::unit);
                    registry.register_method(
                        target_name.clone(),
                        method.name.clone(),
                        InferType::Fun(param_types, Box::new(ret_ty)),
                    );
                }
            }
            _ => {}
        }
    }

    registry
}

// ── Function generalisation record ────────────────────────────────────────────

struct FunGeneralization {
    name:    String,
    fun_ty:  InferType,
    env_fvs: HashSet<TypeVar>,
}

// ── Pre-pass: function hoisting ───────────────────────────────────────────────

/// Register the names of all direct `FunDecl`s in `decls` with fresh type
/// variables so that forward references and mutual recursion work.
fn hoist_fun_decls(decls: &[Decl], ctx: &mut InferContext) {
    for decl in decls {
        if let Decl::Fun(fun) = decl {
            if fun.generics.is_empty() {
                let fresh = ctx.fresh_var();
                ctx.bind_mono(&fun.name, fresh, false);
            }
        }
    }
}


fn build_concrete_struct_env(
    struct_env: &HashMap<String, Vec<(String, InferType)>>,
    subst: &Substitution,
) -> Result<HashMap<String, Vec<(String, Type)>>, YoloscriptError> {
    let dummy = Span::new(0, 0, "");
    struct_env.iter()
        .map(|(name, fields)| {
            let concrete = fields.iter()
                .map(|(fname, fty)| Ok((fname.clone(), infer_type_to_type(&subst.apply(fty), &dummy)?)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((name.clone(), concrete))
        })
        .collect()
}

fn build_concrete_method_env(
    method_env: &HashMap<String, HashMap<String, InferType>>,
    subst: &Substitution,
) -> Result<HashMap<String, HashMap<String, Type>>, YoloscriptError> {
    let dummy = Span::new(0, 0, "");
    method_env.iter()
        .map(|(type_name, methods)| {
            let concrete = methods.iter()
                .map(|(mname, mty)| Ok((mname.clone(), infer_type_to_type(&subst.apply(mty), &dummy)?)))
                .collect::<Result<HashMap<_, _>, _>>()?;
            Ok((type_name.clone(), concrete))
        })
        .collect()
}

// ── Pass 1: type inference ────────────────────────────────────────────────────

fn infer_program(
    program: &Program,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<(), YoloscriptError> {
    for decl in &program.decls {
        infer_decl(decl, ctx, fun_generalizations)?;
    }
    Ok(())
}

fn infer_decl(
    decl: &Decl,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, YoloscriptError> {
    match decl {
        Decl::Let(ld) => {
            let val_ty = infer_expr(&ld.value, ctx, fun_generalizations)?;
            if let Some(ann) = &ld.type_ann {
                ctx.add_constraint(val_ty.clone(), type_expr_to_infer(ann), ld.span.clone());
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
                return Err(YoloscriptError::internal("trait impl blocks not yet supported"));
            }
            let target_name = match &ib.target_type {
                TypeExpr::Named(name, args) if args.is_empty() => name.clone(),
                _ => return Err(YoloscriptError::internal("generic impl blocks not yet supported")),
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
) -> Result<(), YoloscriptError> {
    if !fun.generics.is_empty() {
        return Err(YoloscriptError::internal(format!(
            "generic function `{}` not yet supported",
            fun.name
        )));
    }

    // Param types: use annotation if present, otherwise a fresh variable.
    let param_types: Vec<InferType> = fun.params.iter().map(|p| {
        if let Some(ann) = &p.type_ann { type_expr_to_infer(ann) } else { ctx.fresh_var() }
    }).collect();

    // Return type: use annotation if present, otherwise a fresh variable.
    let ret_ty = if let Some(ann) = &fun.return_type {
        type_expr_to_infer(ann)
    } else {
        ctx.fresh_var()
    };

    // Capture env free vars before entering the function scope (used for generalisation).
    let env_fvs = ctx.env_free_vars();

    ctx.push_scope();
    for (param, pt) in fun.params.iter().zip(param_types.iter()) {
        ctx.bind_mono(&param.name, pt.clone(), false);
    }

    let saved_ret = ctx.push_return_type(ret_ty.clone());
    let body_ty = infer_block(&fun.body, ctx, fun_generalizations)?;

    // The block's tail type must unify with the declared return type.
    ctx.add_constraint(body_ty, ret_ty.clone(), fun.body.span.clone());

    ctx.pop_return_type(saved_ret);
    ctx.pop_scope();

    let fun_ty = InferType::Fun(param_types, Box::new(ret_ty));

    // Unify with the pre-hoisted fresh variable registered during the pre-pass.
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
) -> Result<(), YoloscriptError> {
    if !method.generics.is_empty() {
        return Err(YoloscriptError::internal(format!(
            "generic method `{}` not yet supported", method.name
        )));
    }
    let self_ty = InferType::Named(target_name.to_string(), vec![]);
    let param_types: Vec<InferType> = method.params.iter().map(|p| {
        if p.name == "self" {
            self_ty.clone()
        } else if let Some(ann) = &p.type_ann {
            type_expr_to_infer(ann)
        } else {
            ctx.fresh_var()
        }
    }).collect();
    let ret_ty = method.return_type.as_ref()
        .map(type_expr_to_infer)
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
) -> Result<InferType, YoloscriptError> {
    ctx.push_scope();
    hoist_fun_decls(&block.stmts, ctx);
    let mut last_stmt_ty = InferType::unit();
    for stmt in &block.stmts {
        last_stmt_ty = infer_decl(stmt, ctx, fun_generalizations)?;
    }
    let ty = match &block.tail {
        Some(tail) => infer_expr(tail, ctx, fun_generalizations)?,
        None       => last_stmt_ty,
    };
    ctx.pop_scope();
    Ok(ty)
}

fn infer_stmt(
    stmt: &Stmt,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, YoloscriptError> {
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
            // Init scope wraps condition, step, and body.
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
                    return Err(YoloscriptError::type_error(
                        ErrorCode::E0001,
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
        _ => Err(YoloscriptError::internal("statement not yet supported")),
    }
}

fn infer_expr(
    expr: &Expr,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, YoloscriptError> {
    match expr {
        Expr::Literal(lit, _)          => Ok(infer_literal(lit, ctx)),
        Expr::Ident(name, span)        => {
            ctx.lookup(name).ok_or_else(|| YoloscriptError::type_error(
                ErrorCode::E0003,
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
                    return Err(YoloscriptError::type_error(
                        ErrorCode::E0004,
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
                    // No else: the then-branch must produce Unit (value is discarded).
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
                    let obj_ty = infer_expr(object, ctx, fun_generalizations)?;
                    let obj_ty = ctx.solve()?.apply(&obj_ty);
                    let struct_name = named_type_name(&obj_ty).ok_or_else(|| {
                        YoloscriptError::type_error(
                            ErrorCode::E0002,
                            "cannot infer struct type for field assignment; add a type annotation",
                            target_span,
                        )
                    })?;
                    let fields = ctx.get_struct_fields(&struct_name)
                        .ok_or_else(|| YoloscriptError::type_error(
                            ErrorCode::E0003,
                            format!("unknown type `{struct_name}`"),
                            target_span,
                        ))?
                        .clone();
                    fields.iter()
                        .find(|(n, _)| n == field)
                        .map(|(_, ty)| ty.clone())
                        .ok_or_else(|| YoloscriptError::type_error(
                            ErrorCode::E0003,
                            format!("no field `{field}` on `{struct_name}`"),
                            target_span,
                        ))?
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
            let struct_name = named_type_name(&obj_ty).ok_or_else(|| YoloscriptError::type_error(
                ErrorCode::E0002,
                "cannot infer struct type for field access; add a type annotation",
                span,
            ))?;
            let fields = ctx.get_struct_fields(&struct_name)
                .ok_or_else(|| YoloscriptError::type_error(
                    ErrorCode::E0003,
                    format!("unknown type `{struct_name}`"),
                    span,
                ))?;
            fields.iter()
                .find(|(n, _)| n == field)
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| YoloscriptError::type_error(
                    ErrorCode::E0003,
                    format!("no field `{field}` on `{struct_name}`"),
                    span,
                ))
        }
        Expr::MethodCall { receiver, method, args, span } => {
            let recv_ty = infer_expr(receiver, ctx, fun_generalizations)?;
            let recv_ty = ctx.solve()?.apply(&recv_ty);
            let struct_name = named_type_name(&recv_ty).ok_or_else(|| YoloscriptError::type_error(
                ErrorCode::E0002,
                "cannot infer receiver type for method call; add a type annotation",
                span,
            ))?;
            let method_ty = ctx.get_method_type(&struct_name, method)
                .cloned()
                .ok_or_else(|| YoloscriptError::type_error(
                    ErrorCode::E0003,
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
                // Two-segment path: enum variant construction (e.g. Shape::Circle { .. }).
                let enum_name    = &path[0];
                let variant_name = &path[1];
                let enum_info = ctx.get_enum(enum_name)
                    .ok_or_else(|| YoloscriptError::type_error(
                        ErrorCode::E0003,
                        format!("unknown enum `{enum_name}`"),
                        span,
                    ))?
                    .clone();
                let variant = enum_info.variants.iter()
                    .find(|v| v.name == *variant_name)
                    .ok_or_else(|| YoloscriptError::type_error(
                        ErrorCode::E0003,
                        format!("no variant `{variant_name}` on enum `{enum_name}`"),
                        span,
                    ))?
                    .clone();
                let mut remap: HashMap<TypeVar, InferType> = HashMap::new();
                for &tp in &enum_info.type_params {
                    remap.insert(tp, ctx.fresh_var());
                }
                let instantiate_ty = |ty: &InferType, remap: &HashMap<TypeVar, InferType>| -> InferType {
                    match ty {
                        InferType::Var(v) => remap.get(v).cloned().unwrap_or_else(|| ty.clone()),
                        other => other.clone(),
                    }
                };
                for (fname, expr) in fields {
                    let decl_ty = variant.fields.iter()
                        .find(|(n, _)| n == fname)
                        .map(|(_, ty)| instantiate_ty(ty, &remap))
                        .ok_or_else(|| YoloscriptError::type_error(
                            ErrorCode::E0003,
                            format!("no field `{fname}` on `{enum_name}::{variant_name}`"),
                            span,
                        ))?;
                    let expr_ty = infer_expr(expr, ctx, fun_generalizations)?;
                    ctx.add_constraint(expr_ty, decl_ty, span.clone());
                }
                let type_args: Vec<InferType> = enum_info.type_params.iter()
                    .map(|tp| remap[tp].clone())
                    .collect();
                Ok(InferType::Named(enum_name.clone(), type_args))
            } else {
                let struct_name = path.last()
                    .ok_or_else(|| YoloscriptError::internal("empty path in struct literal"))?
                    .clone();
                let expected_fields = ctx.get_struct_fields(&struct_name)
                    .ok_or_else(|| YoloscriptError::type_error(
                        ErrorCode::E0003,
                        format!("unknown struct `{struct_name}`"),
                        span,
                    ))?
                    .clone();
                for (name, expr) in fields {
                    let decl_ty = expected_fields.iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, ty)| ty.clone())
                        .ok_or_else(|| YoloscriptError::type_error(
                            ErrorCode::E0003,
                            format!("no field `{name}` on `{struct_name}`"),
                            span,
                        ))?;
                    let expr_ty = infer_expr(expr, ctx, fun_generalizations)?;
                    ctx.add_constraint(expr_ty, decl_ty, span.clone());
                }
                for (decl_name, _) in &expected_fields {
                    if !fields.iter().any(|(n, _)| n == decl_name) {
                        return Err(YoloscriptError::type_error(
                            ErrorCode::E0003,
                            format!("missing field `{decl_name}` in `{struct_name}`"),
                            span,
                        ));
                    }
                }
                Ok(InferType::Named(struct_name, vec![]))
            }
        }
        Expr::Cast { expr, target_type, span } => {
            // v0.1: only `Int as Float` (widening) and identity casts are permitted.
            // Narrowing (`Float as Int`) is intentionally rejected — use an explicit
            // conversion function instead.
            // TODO(Epic 004, task 0002): replace this check with a `From<S>` trait lookup
            // so that user-defined types can participate in `as` casts.
            let source_ty = infer_expr(expr, ctx, fun_generalizations)?;
            let target_ty = type_expr_to_infer(target_type);
            // Solve current constraints so we can inspect the concrete types.
            let source_resolved = ctx.solve()?.apply(&source_ty);
            let target_resolved = ctx.solve()?.apply(&target_ty);
            let valid = matches!(
                (&source_resolved, &target_resolved),
                (InferType::Concrete(Type::Int),   InferType::Concrete(Type::Float))
                | (InferType::Concrete(Type::Int),   InferType::Concrete(Type::Int))
                | (InferType::Concrete(Type::Float), InferType::Concrete(Type::Float))
            );
            if !valid {
                return Err(YoloscriptError::type_error(
                    ErrorCode::E0007,
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
                    elems.get(*index).cloned().ok_or_else(|| YoloscriptError::type_error(
                        ErrorCode::E0003,
                        format!("tuple index {index} out of bounds (tuple has {} elements)", elems.len()),
                        span,
                    ))
                }
                _ => Err(YoloscriptError::type_error(
                    ErrorCode::E0002,
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
                return Err(YoloscriptError::type_error(
                    ErrorCode::E0003,
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
            Err(YoloscriptError::type_error(
                ErrorCode::E0003,
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
            // v0.1 provisional: exact error type match. Epic 004 task 0003 upgrades to From<E>.
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
        _ => Err(YoloscriptError::internal("expression not yet supported")),
    }
}

fn infer_match(
    m: &MatchExpr,
    ctx: &mut InferContext,
    fun_generalizations: &mut Vec<FunGeneralization>,
) -> Result<InferType, YoloscriptError> {
    let scrutinee_ty = infer_expr(&m.scrutinee, ctx, fun_generalizations)?;
    let result_var = ctx.fresh_var();
    for arm in &m.arms {
        ctx.push_scope();
        infer_pattern(&arm.pattern, &scrutinee_ty, ctx)?;
        if let Some(guard) = &arm.guard {
            let g = infer_expr(guard, ctx, fun_generalizations)?;
            ctx.add_constraint(g, InferType::bool(), arm.span.clone());
        }
        let arm_ty = infer_expr(&arm.body, ctx, fun_generalizations)?;
        ctx.add_constraint(arm_ty, result_var.clone(), arm.span.clone());
        ctx.pop_scope();
    }
    Ok(result_var)
}

fn infer_pattern(
    pattern: &Pattern,
    scrutinee_ty: &InferType,
    ctx: &mut InferContext,
) -> Result<(), YoloscriptError> {
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
                return Err(YoloscriptError::type_error(
                    ErrorCode::E0003,
                    format!("unresolved pattern path `{}`", path.join("::")),
                    pat_span,
                ));
            };
            let enum_info = ctx.get_enum(enum_name)
                .ok_or_else(|| YoloscriptError::type_error(
                    ErrorCode::E0003,
                    format!("unknown enum `{enum_name}` in pattern"),
                    pat_span,
                ))?
                .clone();
            let variant = enum_info.variants.iter()
                .find(|v| v.name == *variant_name)
                .ok_or_else(|| YoloscriptError::type_error(
                    ErrorCode::E0003,
                    format!("no variant `{variant_name}` on `{enum_name}`"),
                    pat_span,
                ))?
                .clone();
            let mut remap: HashMap<TypeVar, InferType> = HashMap::new();
            for &tp in &enum_info.type_params {
                remap.insert(tp, ctx.fresh_var());
            }
            let instantiate_ty = |ty: &InferType| -> InferType {
                match ty {
                    InferType::Var(v) => remap.get(v).cloned().unwrap_or_else(|| ty.clone()),
                    other => other.clone(),
                }
            };
            let type_args: Vec<InferType> = enum_info.type_params.iter()
                .map(|tp| remap[tp].clone())
                .collect();
            ctx.add_constraint(
                scrutinee_ty.clone(),
                InferType::Named(enum_name.clone(), type_args),
                pat_span.clone(),
            );
            for field_name in fields {
                let field_ty = variant.fields.iter()
                    .find(|(n, _)| n == field_name)
                    .map(|(_, ty)| instantiate_ty(ty))
                    .ok_or_else(|| YoloscriptError::type_error(
                        ErrorCode::E0003,
                        format!("no field `{field_name}` on `{enum_name}::{variant_name}`"),
                        pat_span,
                    ))?;
                ctx.bind_mono(field_name, field_ty, false);
            }
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
) -> Result<InferType, YoloscriptError> {
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
) -> Result<InferType, YoloscriptError> {
    let ty = infer_expr(operand, ctx, fun_generalizations)?;
    match op {
        UnaryOp::Neg => Ok(ty),
        UnaryOp::Not => {
            ctx.add_constraint(ty, InferType::bool(), span.clone());
            Ok(InferType::bool())
        }
    }
}

// ── Type conversions ──────────────────────────────────────────────────────────

/// Convert a source-level `TypeExpr` to an `InferType` for use during inference.
fn type_expr_to_infer(te: &TypeExpr) -> InferType {
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
fn infer_type_to_type(ty: &InferType, span: &Span) -> Result<Type, YoloscriptError> {
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

fn resolved_to_type(ty: &InferType, subst: &Substitution, span: &Span) -> Result<Type, YoloscriptError> {
    infer_type_to_type(&subst.apply(ty), span)
}

// ── Pass 2: construction ──────────────────────────────────────────────────────

/// Scope-aware context for Pass 2. Mirrors InferContext's scope management but
/// holds concrete `Type` values; no constraint emission.
struct ConstructCtx<'a> {
    subst:      &'a Substitution,
    scheme_env: &'a SchemeEnv,
    env:        Vec<HashMap<String, Type>>,
    struct_env: HashMap<String, Vec<(String, Type)>>,
    method_env: HashMap<String, HashMap<String, Type>>,
    enum_env:   &'a HashMap<String, EnumInfo>,
    /// Shared generator continued from Pass 1; keeps TypeVar identities globally unique.
    gen:        TypeVarGenerator,
}

impl<'a> ConstructCtx<'a> {
    fn new(
        subst:      &'a Substitution,
        scheme_env: &'a SchemeEnv,
        struct_env: HashMap<String, Vec<(String, Type)>>,
        method_env: HashMap<String, HashMap<String, Type>>,
        enum_env:   &'a HashMap<String, EnumInfo>,
        gen:        TypeVarGenerator,
    ) -> Self {
        let mut ctx = Self {
            subst, scheme_env,
            env: vec![HashMap::new()],
            struct_env, method_env, enum_env, gen,
        };
        let str_ty   = Type::Str;
        let int_ty   = Type::Int;
        let float_ty = Type::Float;
        let bool_ty  = Type::Bool;
        let unit_ty  = Type::Unit;
        let mono = |params, ret| Type::Fun(params, Box::new(ret));
        ctx.bind("print",           mono(vec![str_ty.clone()], unit_ty.clone()));
        ctx.bind("println",         mono(vec![str_ty.clone()], unit_ty.clone()));
        ctx.bind("int_to_string",   mono(vec![int_ty.clone()], str_ty.clone()));
        ctx.bind("float_to_string", mono(vec![float_ty],       str_ty.clone()));
        ctx.bind("bool_to_string",  mono(vec![bool_ty],        str_ty.clone()));
        ctx.bind("string_len",      mono(vec![str_ty.clone()], int_ty.clone()));
        ctx.bind("string_concat",   mono(vec![str_ty.clone(), str_ty.clone()], str_ty.clone()));
        ctx.bind("clock",           mono(vec![], int_ty.clone()));
        ctx
    }

    fn push_scope(&mut self) { self.env.push(HashMap::new()); }
    fn pop_scope(&mut self)  { self.env.pop(); }

    fn bind(&mut self, name: impl Into<String>, ty: Type) {
        self.env.last_mut().unwrap().insert(name.into(), ty);
    }

    fn lookup(&self, name: &str) -> Option<&Type> {
        self.env.iter().rev().find_map(|s| s.get(name))
    }
}

fn construct_program(
    program:    &Program,
    subst:      &Substitution,
    scheme_env: &SchemeEnv,
    struct_env: HashMap<String, Vec<(String, Type)>>,
    method_env: HashMap<String, HashMap<String, Type>>,
    enum_env:   &HashMap<String, EnumInfo>,
    gen:        TypeVarGenerator,
) -> Result<TypedProgram, YoloscriptError> {
    let mut ctx = ConstructCtx::new(subst, scheme_env, struct_env, method_env, enum_env, gen);

    // Hoist resolved function types so forward references work in Pass 2.
    for decl in &program.decls {
        if let Decl::Fun(fd) = decl {
            if let Some(scheme) = scheme_env.get(&fd.name) {
                if let Ok(ty) = infer_type_to_type(&scheme.ty, &fd.span) {
                    ctx.bind(&fd.name, ty);
                }
            }
        }
    }

    let mut out = vec![];
    for decl in &program.decls {
        out.push(construct_decl(decl, &mut ctx)?);
    }
    Ok(out)
}

fn construct_decl(decl: &Decl, ctx: &mut ConstructCtx) -> Result<TypedDecl, YoloscriptError> {
    match decl {
        Decl::Let(ld) => {
            let expected_ty = ld.type_ann.as_ref()
                .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, &ld.span))
                .transpose()?;
            let value = construct_expr(&ld.value, expected_ty.as_ref(), ctx)?;
            let ty = expected_ty.unwrap_or_else(|| value.ty().clone());
            ctx.bind(&ld.name, ty);
            Ok(TypedDecl::Let(TypedLetDecl {
                name: ld.name.clone(), type_ann: ld.type_ann.clone(),
                value, span: ld.span.clone(),
            }))
        }
        Decl::Mut(md) => {
            let expected_ty = md.type_ann.as_ref()
                .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, &md.span))
                .transpose()?;
            let value = construct_expr(&md.value, expected_ty.as_ref(), ctx)?;
            let ty = expected_ty.unwrap_or_else(|| value.ty().clone());
            ctx.bind(&md.name, ty);
            Ok(TypedDecl::Mut(TypedMutDecl {
                name: md.name.clone(), type_ann: md.type_ann.clone(),
                value, span: md.span.clone(),
            }))
        }
        Decl::Fun(fd)    => construct_fun_decl(fd, ctx),
        Decl::Struct(sd) => Ok(TypedDecl::Struct(TypedStructDecl {
            name: sd.name.clone(), generics: sd.generics.clone(),
            fields: sd.fields.clone(), span: sd.span.clone(),
        })),
        Decl::Enum(ed)   => Ok(TypedDecl::Enum(TypedEnumDecl {
            name: ed.name.clone(), generics: ed.generics.clone(),
            variants: ed.variants.clone(), span: ed.span.clone(),
        })),
        Decl::Impl(ib)   => construct_impl_decl(ib, ctx),
        Decl::Trait(td)  => Ok(TypedDecl::Trait(TypedTraitDecl {
            name: td.name.clone(), methods: td.methods.clone(), span: td.span.clone(),
        })),
        Decl::Stmt(stmt) => Ok(TypedDecl::Stmt(construct_stmt(stmt, ctx)?)),
    }
}

fn construct_fun_decl(fun: &FunDecl, ctx: &mut ConstructCtx) -> Result<TypedDecl, YoloscriptError> {
    let scheme = ctx.scheme_env.get(&fun.name)
        .ok_or_else(|| YoloscriptError::internal(format!("missing type for fn `{}`", fun.name)))?
        .clone();

    let body = if scheme.quantified_vars.is_empty() {
        // Monomorphic: every expression has a concrete type — construct a typed body.
        let (param_types, ret_ty) = match ctx.subst.apply(&scheme.ty) {
            InferType::Fun(params, ret) => {
                let pts = params.iter()
                    .map(|p| infer_type_to_type(p, &fun.span))
                    .collect::<Result<Vec<_>, _>>()?;
                let rt = infer_type_to_type(&ret, &fun.span).ok();
                (pts, rt)
            }
            _ => return Err(YoloscriptError::internal(format!("expected Fun type for `{}`", fun.name))),
        };
        ctx.push_scope();
        for (param, ty) in fun.params.iter().zip(param_types.iter()) {
            ctx.bind(&param.name, ty.clone());
        }
        let typed_block = construct_block(&fun.body, ret_ty.as_ref(), ctx)?;
        ctx.pop_scope();
        FunBody::Typed(typed_block)
    } else {
        // Polymorphic: no single concrete instantiation — keep the original untyped body.
        FunBody::Generic(fun.body.clone())
    };

    Ok(TypedDecl::Fun(TypedFunDecl {
        name: fun.name.clone(), generics: fun.generics.clone(),
        params: fun.params.clone(), return_type: fun.return_type.clone(),
        body, span: fun.span.clone(),
    }))
}

fn construct_impl_decl(ib: &ImplBlock, ctx: &mut ConstructCtx) -> Result<TypedDecl, YoloscriptError> {
    if ib.trait_name.is_some() {
        return Err(YoloscriptError::internal("trait impl blocks not yet supported"));
    }
    let target_name = match &ib.target_type {
        TypeExpr::Named(name, _) => name.clone(),
        _ => return Err(YoloscriptError::internal("generic impl blocks not yet supported")),
    };
    let methods = ib.methods.iter()
        .map(|m| construct_impl_method(m, &target_name, ctx))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TypedDecl::Impl(TypedImplBlock {
        trait_name:  ib.trait_name.clone(),
        target_type: ib.target_type.clone(),
        methods,
        span: ib.span.clone(),
    }))
}

fn construct_impl_method(method: &FunDecl, target_name: &str, ctx: &mut ConstructCtx) -> Result<TypedFunDecl, YoloscriptError> {
    let self_ty = Type::Named(target_name.to_string(), vec![]);
    let param_types: Vec<Type> = method.params.iter()
        .map(|p| {
            if p.name == "self" {
                Ok(self_ty.clone())
            } else {
                p.type_ann.as_ref()
                    .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, &p.span))
                    .unwrap_or_else(|| Err(YoloscriptError::type_error(
                        ErrorCode::E0002,
                        format!("parameter `{}` needs a type annotation", p.name),
                        &p.span,
                    )))
            }
        })
        .collect::<Result<_, _>>()?;
    ctx.push_scope();
    for (p, ty) in method.params.iter().zip(param_types.iter()) {
        ctx.bind(&p.name, ty.clone());
    }
    let typed_block = construct_block(&method.body, None, ctx)?;
    ctx.pop_scope();
    Ok(TypedFunDecl {
        name:        method.name.clone(),
        generics:    method.generics.clone(),
        params:      method.params.clone(),
        return_type: method.return_type.clone(),
        body:        FunBody::Typed(typed_block),
        span:        method.span.clone(),
    })
}

fn construct_block(block: &Block, expected_tail_ty: Option<&Type>, ctx: &mut ConstructCtx) -> Result<TypedBlock, YoloscriptError> {
    ctx.push_scope();
    let mut stmts = vec![];
    for stmt in &block.stmts {
        stmts.push(construct_decl(stmt, ctx)?);
    }
    let tail = match &block.tail {
        Some(e) => Some(Box::new(construct_expr(e, expected_tail_ty, ctx)?)),
        None    => None,
    };
    ctx.pop_scope();
    Ok(TypedBlock { stmts, tail, span: block.span.clone() })
}

fn construct_stmt(stmt: &Stmt, ctx: &mut ConstructCtx) -> Result<TypedStmt, YoloscriptError> {
    match stmt {
        Stmt::Expr(e) => Ok(TypedStmt::Expr(construct_expr(e, None, ctx)?)),
        Stmt::Return(r) => {
            let value = match &r.value {
                Some(e) => Some(construct_expr(e, None, ctx)?),
                None    => None,
            };
            Ok(TypedStmt::Return(TypedReturnStmt { value, span: r.span.clone() }))
        }
        Stmt::Break(bs) => {
            let value = match &bs.value {
                Some(e) => Some(construct_expr(e, None, ctx)?),
                None    => None,
            };
            Ok(TypedStmt::Break(TypedBreakStmt { value, span: bs.span.clone() }))
        }
        Stmt::Continue(span) => Ok(TypedStmt::Continue(span.clone())),
        Stmt::While(ws) => {
            let condition = construct_expr(&ws.condition, None, ctx)?;
            let body = construct_block(&ws.body, None, ctx)?;
            Ok(TypedStmt::While(TypedWhileStmt { condition, body, span: ws.span.clone() }))
        }
        Stmt::For(fs) => {
            ctx.push_scope();
            let init = match &fs.init {
                Some(ForInit::Mut(md)) => {
                    let value = construct_expr(&md.value, None, ctx)?;
                    let ty = value.ty().clone();
                    ctx.bind(&md.name, ty);
                    let typed_md = TypedMutDecl {
                        name: md.name.clone(), type_ann: md.type_ann.clone(),
                        value, span: md.span.clone(),
                    };
                    Some(TypedForInit::Mut(typed_md))
                }
                Some(ForInit::Expr(e)) => {
                    Some(TypedForInit::Expr(construct_expr(e, None, ctx)?))
                }
                None => None,
            };
            let condition = match &fs.condition {
                Some(c) => Some(construct_expr(c, None, ctx)?),
                None    => None,
            };
            let step = match &fs.step {
                Some(s) => Some(construct_expr(s, None, ctx)?),
                None    => None,
            };
            let body = construct_block(&fs.body, None, ctx)?;
            ctx.pop_scope();
            Ok(TypedStmt::For(TypedForStmt { init, condition, step, body, span: fs.span.clone() }))
        }
        Stmt::ForIn(fi) => {
            let iterable = construct_expr(&fi.iterable, None, ctx)?;
            let elem_ty = match iterable.ty() {
                Type::Array(elem) => *elem.clone(),
                Type::Named(name, args) if name == "Range" && args.len() == 1 => Type::Int,
                _ => return Err(YoloscriptError::internal("for-in over non-iterable type")),
            };
            ctx.push_scope();
            ctx.bind(&fi.binding, elem_ty);
            let body = construct_block(&fi.body, None, ctx)?;
            ctx.pop_scope();
            Ok(TypedStmt::ForIn(TypedForInStmt {
                binding: fi.binding.clone(), iterable, body, span: fi.span.clone(),
            }))
        }
        _ => Err(YoloscriptError::internal("statement not yet supported in construct")),
    }
}

fn construct_expr(expr: &Expr, expected_ty: Option<&Type>, ctx: &mut ConstructCtx) -> Result<TypedExpr, YoloscriptError> {
    match expr {
        Expr::Literal(lit, span) => {
            let ty = construct_literal_type(lit, expected_ty, span)?;
            Ok(TypedExpr::Literal(lit.clone(), ty, span.clone()))
        }
        Expr::Ident(name, span) => {
            let ty = ctx.lookup(name).cloned().ok_or_else(|| YoloscriptError::type_error(
                ErrorCode::E0003,
                format!("undefined name `{name}`"),
                span,
            ))?;
            Ok(TypedExpr::Ident(name.clone(), ty, span.clone()))
        }
        Expr::BinOp(lhs, op, rhs, span) => construct_binop(lhs, op, rhs, span, ctx),
        Expr::UnaryOp(op, operand, span) => construct_unaryop(op, operand, span, ctx),
        Expr::Tuple(elems, span) => {
            let typed: Vec<TypedExpr> = elems.iter()
                .map(|e| construct_expr(e, None, ctx))
                .collect::<Result<_, _>>()?;
            let ty = Type::Tuple(typed.iter().map(|e| e.ty().clone()).collect());
            Ok(TypedExpr::Tuple(typed, ty, span.clone()))
        }
        Expr::Array(elems, span) => {
            if elems.is_empty() {
                let ty = expected_ty.cloned().ok_or_else(|| YoloscriptError::type_error(
                    ErrorCode::E0002,
                    "cannot infer element type of empty array; add a type annotation",
                    span,
                ))?;
                return Ok(TypedExpr::Array(vec![], ty, span.clone()));
            }
            let typed: Vec<TypedExpr> = elems.iter()
                .map(|e| construct_expr(e, None, ctx))
                .collect::<Result<_, _>>()?;
            let elem_ty = typed[0].ty().clone();
            let ty = Type::Array(Box::new(elem_ty));
            Ok(TypedExpr::Array(typed, ty, span.clone()))
        }
        Expr::Call { callee, args, span } => construct_call(callee, args, span, ctx),
        Expr::Index { object, index, span } => {
            let typed_obj = construct_expr(object, None, ctx)?;
            let typed_idx = construct_expr(index,  None, ctx)?;
            let elem_ty = match typed_obj.ty() {
                Type::Array(elem) => *elem.clone(),
                _ => return Err(YoloscriptError::type_error(
                    ErrorCode::E0001,
                    "indexed value is not an array",
                    span,
                )),
            };
            Ok(TypedExpr::Index {
                object: Box::new(typed_obj),
                index:  Box::new(typed_idx),
                ty: elem_ty,
                span: span.clone(),
            })
        }
        Expr::If { condition, then_branch, else_branch, span } => {
            let condition = construct_expr(condition, None, ctx)?;
            let then_branch = construct_block(then_branch, expected_ty, ctx)?;
            let (else_branch, ty) = match else_branch {
                Some(eb) => {
                    let typed_else = construct_block(eb, expected_ty, ctx)?;
                    let ty = then_branch.tail.as_ref()
                        .map(|e| e.ty().clone())
                        .unwrap_or(Type::Unit);
                    (Some(typed_else), ty)
                }
                None => (None, Type::Unit),
            };
            Ok(TypedExpr::If {
                condition: Box::new(condition),
                then_branch,
                else_branch,
                ty,
                span: span.clone(),
            })
        }
        Expr::Assign { target, op, value, span } => {
            let typed_value = construct_expr(value, None, ctx)?;
            Ok(TypedExpr::Assign {
                target: target.clone(),
                op: op.clone(),
                value: Box::new(typed_value),
                ty: Type::Unit,
                span: span.clone(),
            })
        }
        Expr::FieldAccess { object, field, span } => {
            let typed_obj = construct_expr(object, None, ctx)?;
            let struct_name = match typed_obj.ty() {
                Type::Named(name, _) => name.clone(),
                t => return Err(YoloscriptError::internal(
                    format!("field access on non-struct type {t}")
                )),
            };
            let field_ty = ctx.struct_env.get(&struct_name)
                .and_then(|fs| fs.iter().find(|(n, _)| n == field))
                .map(|(_, ty)| ty.clone())
                .ok_or_else(|| YoloscriptError::internal(
                    format!("no field `{field}` on `{struct_name}`")
                ))?;
            Ok(TypedExpr::FieldAccess {
                object: Box::new(typed_obj),
                field:  field.clone(),
                ty:     field_ty,
                span:   span.clone(),
            })
        }
        Expr::MethodCall { receiver, method, args, span } => {
            let typed_receiver = construct_expr(receiver, None, ctx)?;
            let struct_name = match typed_receiver.ty() {
                Type::Named(name, _) => name.clone(),
                Type::Str            => "String".to_string(),
                Type::Int            => "Int".to_string(),
                Type::Float          => "Float".to_string(),
                Type::Bool           => "Bool".to_string(),
                t => return Err(YoloscriptError::internal(
                    format!("method call on non-struct type {t}")
                )),
            };
            let method_fun_ty = ctx.method_env.get(&struct_name)
                .and_then(|m| m.get(method.as_str()))
                .cloned()
                .ok_or_else(|| YoloscriptError::internal(
                    format!("no method `{method}` on `{struct_name}`")
                ))?;
            let typed_args: Vec<TypedExpr> = args.iter()
                .map(|a| construct_expr(a, None, ctx))
                .collect::<Result<_, _>>()?;
            let ret_ty = match method_fun_ty {
                Type::Fun(_, ret) => *ret,
                _ => return Err(YoloscriptError::internal("method type is not a function")),
            };
            Ok(TypedExpr::MethodCall {
                receiver: Box::new(typed_receiver),
                method:   method.clone(),
                args:     typed_args,
                ty:       ret_ty,
                span:     span.clone(),
            })
        }
        Expr::StructLiteral { path, fields, span } => {
            let typed_fields: Vec<(String, TypedExpr)> = fields.iter()
                .map(|(name, expr)| Ok((name.clone(), construct_expr(expr, None, ctx)?)))
                .collect::<Result<_, _>>()?;

            let ty = if path.len() == 2 {
                // Enum variant: resolve concrete type arguments using the same
                // instantiate-then-unify pattern as instantiate_scheme_for_call.
                let enum_name    = &path[0];
                let variant_name = &path[1];
                let enum_info = ctx.enum_env.get(enum_name)
                    .ok_or_else(|| YoloscriptError::type_error(
                        ErrorCode::E0003,
                        format!("unknown enum `{enum_name}`"),
                        span,
                    ))?;
                let variant = enum_info.variants.iter()
                    .find(|v| v.name == *variant_name)
                    .ok_or_else(|| YoloscriptError::type_error(
                        ErrorCode::E0003,
                        format!("no variant `{variant_name}` on enum `{enum_name}`"),
                        span,
                    ))?;

                // Assign a fresh type variable to each formal type parameter and
                // build an instantiation substitution for this particular usage site.
                let mut init_subst = Substitution::new();
                let fresh_vars: Vec<InferType> = enum_info.type_params.iter()
                    .map(|&tp| {
                        let fresh = InferType::Var(ctx.gen.fresh());
                        init_subst.bind(tp, fresh.clone());
                        fresh
                    })
                    .collect();

                // Unify each instantiated field type against the actual expression type
                // to solve for the fresh variables.
                let mut local_subst = Substitution::new();
                for (field_name, typed_expr) in &typed_fields {
                    if let Some((_, field_decl_ty)) = variant.fields.iter()
                        .find(|(n, _)| n == field_name)
                    {
                        let instantiated = init_subst.apply(field_decl_ty);
                        let actual = type_to_infer(typed_expr.ty());
                        if let Ok(s) = unify(&local_subst.apply(&instantiated), &local_subst.apply(&actual)) {
                            local_subst = local_subst.compose(&s);
                        }
                    }
                }

                // Apply the local substitution to recover concrete type arguments.
                // If a type param remains unresolved (fieldless variants like
                // `Perhaps::Nope`), fall back to the annotation's args.
                // type_to_infer normalises Perhaps/Result into Named for uniform handling.
                let hint_args: Vec<Type> = expected_ty
                    .map(|ty| {
                        if let InferType::Named(n, args) = type_to_infer(ty) {
                            if n == *enum_name {
                                args.iter()
                                    .map(|a| infer_type_to_type(a, span))
                                    .collect::<Result<Vec<_>, _>>()
                                    .unwrap_or_default()
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    })
                    .unwrap_or_default();
                let concrete_args: Vec<Type> = fresh_vars.iter()
                    .enumerate()
                    .map(|(i, fv)| {
                        let resolved = local_subst.apply(fv);
                        if matches!(resolved, InferType::Var(_)) {
                            // Unresolved — use hint from annotation if available
                            hint_args.get(i).cloned()
                                .ok_or_else(|| YoloscriptError::type_error(
                                    ErrorCode::E0002,
                                    "cannot infer type; add a type annotation",
                                    span,
                                ))
                        } else {
                            infer_type_to_type(&resolved, span)
                        }
                    })
                    .collect::<Result<_, _>>()?;

                // Route through infer_type_to_type so "Perhaps"/"Result" become
                // Type::Perhaps/Type::Result rather than Type::Named — infer_type_to_type
                // is the single normalisation point for this conversion.
                let infer_args: Vec<InferType> = concrete_args.iter().map(type_to_infer).collect();
                infer_type_to_type(&InferType::Named(enum_name.clone(), infer_args), span)?
            } else {
                // Single-segment path: plain struct literal with no type parameters.
                let type_name = path.last().unwrap();
                Type::Named(type_name.clone(), vec![])
            };

            Ok(TypedExpr::StructLiteral {
                path:   path.clone(),
                fields: typed_fields,
                ty,
                span:   span.clone(),
            })
        }
        Expr::Path(segments, span) => {
            let [type_name, member_name] = segments.as_slice() else {
                return Err(YoloscriptError::internal("invalid path in construct"));
            };
            if let Some(ty) = ctx.method_env
                .get(type_name.as_str())
                .and_then(|m| m.get(member_name.as_str()))
                .cloned()
            {
                return Ok(TypedExpr::Path(segments.clone(), ty, span.clone()));
            }
            Ok(TypedExpr::Path(
                segments.clone(),
                Type::Named(type_name.clone(), vec![]),
                span.clone(),
            ))
        }
        Expr::Closure { params, return_type, body, span } => {
            let param_types: Vec<Type> = params.iter()
                .map(|p| p.type_ann.as_ref()
                    .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, &p.span))
                    .unwrap_or_else(|| Err(YoloscriptError::type_error(
                        ErrorCode::E0002,
                        format!("closure parameter `{}` needs a type annotation", p.name),
                        &p.span,
                    ))))
                .collect::<Result<_, _>>()?;
            let ret_ty = return_type.as_ref()
                .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, span))
                .transpose()?
                .unwrap_or(Type::Unit);
            ctx.push_scope();
            for (p, ty) in params.iter().zip(param_types.iter()) {
                ctx.bind(&p.name, ty.clone());
            }
            let typed_body = construct_block(body, None, ctx)?;
            ctx.pop_scope();
            let ty = Type::Fun(param_types, Box::new(ret_ty));
            Ok(TypedExpr::Closure { params: params.clone(), return_type: return_type.clone(), body: typed_body, ty, span: span.clone() })
        }
        Expr::PropagateError { expr, span } => {
            let typed_expr = construct_expr(expr, None, ctx)?;
            let ty = match typed_expr.ty() {
                Type::Result(ok, _) => *ok.clone(),
                Type::Named(name, args) if name == "Result" && args.len() == 2 => args[0].clone(),
                _ => return Err(YoloscriptError::internal("? on non-Result value")),
            };
            Ok(TypedExpr::PropagateError { expr: Box::new(typed_expr), ty, span: span.clone() })
        }
        Expr::Match(m) => construct_match(m, ctx),
        Expr::Cast { expr, target_type, span } => {
            let typed_expr = construct_expr(expr, None, ctx)?;
            let ty = resolved_to_type(&type_expr_to_infer(target_type), ctx.subst, span)?;
            Ok(TypedExpr::Cast { expr: Box::new(typed_expr), target_type: target_type.clone(), ty, span: span.clone() })
        }
        Expr::TupleAccess { object, index, span } => {
            let typed_obj = construct_expr(object, None, ctx)?;
            let ty = match typed_obj.ty() {
                Type::Tuple(elems) => elems.get(*index).cloned()
                    .ok_or_else(|| YoloscriptError::internal(
                        format!("tuple index {index} out of bounds")
                    ))?,
                _ => return Err(YoloscriptError::internal("tuple access on non-tuple")),
            };
            Ok(TypedExpr::TupleAccess { object: Box::new(typed_obj), index: *index, ty, span: span.clone() })
        }
        Expr::Loop { body, span } => {
            let typed_body = construct_block(body, None, ctx)?;
            let ty = find_loop_break_type(&typed_body).unwrap_or(Type::Never);
            Ok(TypedExpr::Loop { body: typed_body, ty, span: span.clone() })
        }
        _ => Err(YoloscriptError::internal("expression not yet supported in construct")),
    }
}

fn find_loop_break_type(block: &TypedBlock) -> Option<Type> {
    block.stmts.iter().find_map(find_break_in_decl)
}

fn find_break_in_decl(decl: &TypedDecl) -> Option<Type> {
    match decl {
        TypedDecl::Stmt(stmt) => find_break_in_stmt(stmt),
        _ => None,
    }
}

fn find_break_in_stmt(stmt: &TypedStmt) -> Option<Type> {
    match stmt {
        TypedStmt::Break(bs) => bs.value.as_ref().map(|v| v.ty().clone()),
        TypedStmt::Expr(expr) => find_break_in_expr(expr),
        // break inside a nested while/for/for-in exits that loop, not the outer loop
        TypedStmt::While(_) | TypedStmt::For(_) | TypedStmt::ForIn(_) => None,
        TypedStmt::Return(_) | TypedStmt::Continue(_) => None,
    }
}

fn find_break_in_expr(expr: &TypedExpr) -> Option<Type> {
    match expr {
        TypedExpr::If { then_branch, else_branch, .. } => {
            find_loop_break_type(then_branch)
                .or_else(|| else_branch.as_ref().and_then(|b| find_loop_break_type(b)))
        }
        // break inside a nested loop exits the inner loop, not the outer
        TypedExpr::Loop { .. } => None,
        // break inside a closure doesn't escape to the enclosing loop
        TypedExpr::Closure { .. } => None,
        _ => None,
    }
}

fn construct_match(m: &MatchExpr, ctx: &mut ConstructCtx) -> Result<TypedExpr, YoloscriptError> {
    let scrutinee = construct_expr(&m.scrutinee, None, ctx)?;
    let scrutinee_ty = scrutinee.ty().clone();
    let mut typed_arms = vec![];
    for arm in &m.arms {
        ctx.push_scope();
        construct_pattern_bindings(&arm.pattern, &scrutinee_ty, ctx)?;
        let guard = match &arm.guard {
            Some(g) => Some(construct_expr(g, None, ctx)?),
            None    => None,
        };
        let body = construct_expr(&arm.body, None, ctx)?;
        let arm_ty = body.ty().clone();
        typed_arms.push(TypedMatchArm {
            pattern: arm.pattern.clone(),
            guard,
            body,
            span: arm.span.clone(),
        });
        ctx.pop_scope();
        let _ = arm_ty;
    }
    check_match_exhaustiveness(&typed_arms, &scrutinee_ty, ctx.enum_env, &m.span)?;
    let expr_type = typed_arms.first()
        .map(|a| a.body.ty().clone())
        .unwrap_or(Type::Unit);
    Ok(TypedExpr::Match(TypedMatchExpr {
        scrutinee: Box::new(scrutinee),
        arms: typed_arms,
        expr_type,
        span: m.span.clone(),
    }))
}

fn check_match_exhaustiveness(
    arms: &[TypedMatchArm],
    scrutinee_ty: &Type,
    enum_env: &HashMap<String, EnumInfo>,
    span: &Span,
) -> Result<(), YoloscriptError> {
    // An unguarded wildcard or bare binding catches everything.
    if arms.iter().any(|a| a.guard.is_none() && is_catch_all_pattern(&a.pattern)) {
        return Ok(());
    }
    let exhaustive = match scrutinee_ty {
        Type::Bool => {
            let has_true  = arms.iter().any(|a| a.guard.is_none() && is_bool_literal_pattern(&a.pattern, true));
            let has_false = arms.iter().any(|a| a.guard.is_none() && is_bool_literal_pattern(&a.pattern, false));
            has_true && has_false
        }
        Type::Perhaps(_) => {
            let has_some = arms.iter().any(|a| a.guard.is_none() && pattern_covers_variant(&a.pattern, "Perhaps", "Some"));
            let has_nope = arms.iter().any(|a| a.guard.is_none() && pattern_covers_variant(&a.pattern, "Perhaps", "Nope"));
            has_some && has_nope
        }
        Type::Result(_, _) => {
            let has_ok  = arms.iter().any(|a| a.guard.is_none() && pattern_covers_variant(&a.pattern, "Result", "Ok"));
            let has_err = arms.iter().any(|a| a.guard.is_none() && pattern_covers_variant(&a.pattern, "Result", "Err"));
            has_ok && has_err
        }
        Type::Named(name, _) => {
            if let Some(enum_info) = enum_env.get(name.as_str()) {
                enum_info.variants.iter().all(|v| {
                    arms.iter().any(|a| a.guard.is_none() && pattern_covers_variant(&a.pattern, name, &v.name))
                })
            } else {
                false
            }
        }
        // Int, Float, Str, Tuple, Array, Fun — value-infinite; only a catch-all suffices.
        _ => false,
    };
    if !exhaustive {
        return Err(YoloscriptError::type_error(
            ErrorCode::E0003,
            "non-exhaustive match: not all cases are covered".to_string(),
            span,
        ));
    }
    Ok(())
}

fn is_catch_all_pattern(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Wildcard(_) | Pattern::Binding(_, _) => true,
        // A tuple pattern is irrefutable when every element is also irrefutable.
        Pattern::Tuple(pats, _) => pats.iter().all(is_catch_all_pattern),
        _ => false,
    }
}

fn is_bool_literal_pattern(pattern: &Pattern, expected: bool) -> bool {
    matches!(pattern, Pattern::Literal(Literal::Bool(b), _) if *b == expected)
}

/// Returns true if `pattern` (unguarded) covers variant `variant_name` of enum `enum_name`.
fn pattern_covers_variant(pattern: &Pattern, enum_name: &str, variant_name: &str) -> bool {
    match pattern {
        // `nope` covers the "Nope" variant of "Perhaps".
        Pattern::Nope(_) => enum_name == "Perhaps" && variant_name == "Nope",
        Pattern::EnumVariant { path, .. } => {
            path.first().map(String::as_str) == Some(enum_name)
                && path.get(1).map(String::as_str) == Some(variant_name)
        }
        _ => false,
    }
}

fn construct_pattern_bindings(
    pattern: &Pattern,
    scrutinee_ty: &Type,
    ctx: &mut ConstructCtx,
) -> Result<(), YoloscriptError> {
    match pattern {
        Pattern::Wildcard(_) | Pattern::Literal(_, _) | Pattern::Nope(_) => {}
        Pattern::Binding(name, _) => {
            ctx.bind(name, scrutinee_ty.clone());
        }
        Pattern::Tuple(pats, _) => {
            let elems = match scrutinee_ty {
                Type::Tuple(ts) => ts.clone(),
                _ => return Err(YoloscriptError::internal("tuple pattern on non-tuple")),
            };
            for (pat, elem_ty) in pats.iter().zip(elems.iter()) {
                construct_pattern_bindings(pat, elem_ty, ctx)?;
            }
        }
        Pattern::EnumVariant { path, fields, span } => {
            let [enum_name, variant_name] = path.as_slice() else {
                return Err(YoloscriptError::internal("invalid pattern path"));
            };
            let enum_info = ctx.enum_env.get(enum_name.as_str())
                .ok_or_else(|| YoloscriptError::internal(format!("unknown enum `{enum_name}`")))?
                .clone();
            let variant = enum_info.variants.iter()
                .find(|v| v.name == *variant_name)
                .ok_or_else(|| YoloscriptError::internal(format!("unknown variant `{variant_name}`")))?
                .clone();
            let type_args = extract_type_args_from_type(scrutinee_ty);
            let mut remap = Substitution::new();
            for (&tp, arg_ty) in enum_info.type_params.iter().zip(type_args.iter()) {
                remap.bind(tp, InferType::Concrete(arg_ty.clone()));
            }
            let dummy = Span::new(0, 0, "");
            for field_name in fields {
                let template_ty = variant.fields.iter()
                    .find(|(n, _)| n == field_name)
                    .map(|(_, ty)| ty.clone())
                    .ok_or_else(|| YoloscriptError::internal(
                        format!("no field `{field_name}` on variant `{variant_name}`")
                    ))?;
                let concrete = infer_type_to_type(&remap.apply(&template_ty), &dummy)?;
                ctx.bind(field_name, concrete);
            }
            let _ = span;
        }
    }
    Ok(())
}

fn extract_type_args_from_type(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Perhaps(t)     => vec![*t.clone()],
        Type::Result(t, e)   => vec![*t.clone(), *e.clone()],
        Type::Named(_, args) => args.clone(),
        _ => vec![],
    }
}

/// Build a typed Call expression.
///
/// For polymorphic callees (Idents in scheme_env whose type still contains free
/// vars), re-instantiate the scheme against the concrete argument types using
/// local unification. This is the Pass 2 counterpart of the inline
/// solve-and-generalize done in `infer_fun_decl`.
fn construct_call(
    callee: &Expr,
    args:   &[Expr],
    span:   &Span,
    ctx:    &mut ConstructCtx,
) -> Result<TypedExpr, YoloscriptError> {
    // Construct arguments first — needed to drive polymorphic instantiation.
    let typed_args: Vec<TypedExpr> = args.iter()
        .map(|a| construct_expr(a, None, ctx))
        .collect::<Result<_, _>>()?;
    let arg_types: Vec<&Type> = typed_args.iter().map(|a| a.ty()).collect();

    // Resolve the callee's concrete function type.
    let (typed_callee, fun_ty) = match callee {
        Expr::Ident(name, ident_span) if ctx.lookup(name).is_none() => {
            // Not in ConstructCtx — must be a polymorphic function in scheme_env.
            let scheme = ctx.scheme_env.get(name.as_str()).ok_or_else(|| {
                YoloscriptError::type_error(ErrorCode::E0003, format!("undefined name `{name}`"), ident_span)
            })?;
            let concrete = instantiate_scheme_for_call(scheme, &arg_types, span, &mut ctx.gen)?;
            let typed = TypedExpr::Ident(name.clone(), concrete.clone(), ident_span.clone());
            (typed, concrete)
        }
        _ => {
            let typed = construct_expr(callee, None, ctx)?;
            let ty = typed.ty().clone();
            (typed, ty)
        }
    };

    match &fun_ty {
        Type::Fun(params, ret) => {
            if params.len() != typed_args.len() {
                return Err(YoloscriptError::type_error(
                    ErrorCode::E0004,
                    format!("expected {} argument(s), got {}", params.len(), typed_args.len()),
                    span,
                ));
            }
            Ok(TypedExpr::Call {
                callee: Box::new(typed_callee),
                args:   typed_args,
                ty:     *ret.clone(),
                span:   span.clone(),
            })
        }
        _ => Err(YoloscriptError::type_error(
            ErrorCode::E0001,
            "called a non-function value",
            span,
        )),
    }
}

/// Instantiate a polymorphic scheme against concrete argument types.
fn type_to_infer(ty: &Type) -> InferType {
    match ty {
        Type::Never            => InferType::Never,
        Type::Array(t)         => InferType::Array(Box::new(type_to_infer(t))),
        Type::Tuple(ts)        => InferType::Tuple(ts.iter().map(type_to_infer).collect()),
        Type::Fun(ps, ret)     => InferType::Fun(
            ps.iter().map(type_to_infer).collect(),
            Box::new(type_to_infer(ret)),
        ),
        Type::Named(n, args)   => InferType::Named(n.clone(), args.iter().map(type_to_infer).collect()),
        Type::Perhaps(t)       => InferType::Named("Perhaps".into(), vec![type_to_infer(t)]),
        Type::Result(t, e)     => InferType::Named("Result".into(), vec![type_to_infer(t), type_to_infer(e)]),
        other                  => InferType::Concrete(other.clone()),
    }
}

fn instantiate_scheme_for_call(
    scheme:    &TypeScheme,
    arg_types: &[&Type],
    span:      &Span,
    gen:       &mut TypeVarGenerator,
) -> Result<Type, YoloscriptError> {
    let instance = instantiate(scheme, gen);

    let (params, ret) = match instance {
        InferType::Fun(p, r) => (p, r),
        _ => return Err(YoloscriptError::internal("scheme type is not a function")),
    };

    let mut subst = Substitution::new();
    for (param, arg_ty) in params.iter().zip(arg_types.iter()) {
        let arg_infer = type_to_infer(*arg_ty);
        let s = unify(&subst.apply(param), &arg_infer).map_err(|_| {
            YoloscriptError::type_error(ErrorCode::E0001, "argument type mismatch", span)
        })?;
        subst = subst.compose(&s);
    }

    let concrete_params: Vec<Type> = params.iter()
        .map(|p| infer_type_to_type(&subst.apply(p), span))
        .collect::<Result<_, _>>()?;
    let concrete_ret = infer_type_to_type(&subst.apply(&ret), span)?;
    Ok(Type::Fun(concrete_params, Box::new(concrete_ret)))
}

fn construct_literal_type(lit: &Literal, expected_ty: Option<&Type>, span: &Span) -> Result<Type, YoloscriptError> {
    match lit {
        Literal::Int(_)   => Ok(Type::Int),
        Literal::Float(_) => Ok(Type::Float),
        Literal::Bool(_)  => Ok(Type::Bool),
        Literal::Str(_)   => Ok(Type::Str),
        Literal::Unit     => Ok(Type::Unit),
        // nope's type cannot be re-derived from the literal alone. Pass 2 must receive
        // the expected type from the enclosing binding's annotation (propagated via
        // construct_expr's expected_ty parameter). If no annotation, E0002 — but Pass 1
        // should have already caught the unannotated case via an unresolved type var.
        Literal::Nope     => expected_ty.cloned().ok_or_else(|| YoloscriptError::type_error(
            ErrorCode::E0002,
            "cannot infer type of `nope`; add a type annotation",
            span,
        )),
    }
}

fn construct_binop(
    lhs: &Expr,
    op:  &BinOp,
    rhs: &Expr,
    span: &Span,
    ctx: &mut ConstructCtx,
) -> Result<TypedExpr, YoloscriptError> {
    let lhs = construct_expr(lhs, None, ctx)?;
    let rhs = construct_expr(rhs, None, ctx)?;
    let ty = match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => lhs.ty().clone(),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Type::Bool,
        BinOp::And | BinOp::Or => Type::Bool,
        BinOp::Range | BinOp::RangeInclusive => Type::Named("Range".to_string(), vec![Type::Int]),
    };
    Ok(TypedExpr::BinOp(Box::new(lhs), op.clone(), Box::new(rhs), ty, span.clone()))
}

fn construct_unaryop(
    op:      &UnaryOp,
    operand: &Expr,
    span:    &Span,
    ctx:     &mut ConstructCtx,
) -> Result<TypedExpr, YoloscriptError> {
    let operand = construct_expr(operand, None, ctx)?;
    let ty = match op {
        UnaryOp::Neg => operand.ty().clone(),
        UnaryOp::Not => Type::Bool,
    };
    Ok(TypedExpr::UnaryOp(op.clone(), Box::new(operand), ty, span.clone()))
}
