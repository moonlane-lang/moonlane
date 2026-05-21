use std::collections::HashMap;

use crate::ast::*;
use crate::error::{ErrorCode, YoloscriptError};
use crate::typed_ast::*;
use crate::typeinference::*;
use crate::types::Type;

use super::SchemeEnv;
use super::conversions::{infer_type_to_type, resolved_to_type, type_expr_to_infer, type_to_infer};

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

pub(super) fn construct_program(
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

fn construct_impl_method(
    method: &FunDecl,
    target_name: &str,
    ctx: &mut ConstructCtx,
) -> Result<TypedFunDecl, YoloscriptError> {
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

fn construct_block(
    block: &Block,
    expected_tail_ty: Option<&Type>,
    ctx: &mut ConstructCtx,
) -> Result<TypedBlock, YoloscriptError> {
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

fn construct_expr(
    expr: &Expr,
    expected_ty: Option<&Type>,
    ctx: &mut ConstructCtx,
) -> Result<TypedExpr, YoloscriptError> {
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
                construct_enum_literal_ty(&path[0], &path[1], &typed_fields, expected_ty, span, ctx)?
            } else {
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
            Ok(TypedExpr::Closure {
                params: params.clone(),
                return_type: return_type.clone(),
                body: typed_body,
                ty,
                span: span.clone(),
            })
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
            Ok(TypedExpr::Cast {
                expr: Box::new(typed_expr),
                target_type: target_type.clone(),
                ty,
                span: span.clone(),
            })
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
            Ok(TypedExpr::TupleAccess {
                object: Box::new(typed_obj),
                index: *index,
                ty,
                span: span.clone(),
            })
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
        // Never is uninhabited — a match on it is vacuously exhaustive.
        Type::Never => true,
        // Int, Float, Str, Tuple, Array, Fun — value-infinite; only a catch-all suffices.
        _ => false,
    };
    if !exhaustive {
        return Err(YoloscriptError::type_error(
            ErrorCode::E0008,
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
            let _ = span;
            bind_enum_variant_fields(enum_name, variant_name, fields, scrutinee_ty, ctx)?;
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

fn construct_enum_literal_ty(
    enum_name: &str,
    variant_name: &str,
    typed_fields: &[(String, TypedExpr)],
    expected_ty: Option<&Type>,
    span: &Span,
    ctx: &mut ConstructCtx,
) -> Result<Type, YoloscriptError> {
    // Resolve concrete type arguments using the same instantiate-then-unify
    // pattern as instantiate_scheme_for_call.
    let enum_info = ctx.enum_env.get(enum_name)
        .ok_or_else(|| YoloscriptError::type_error(
            ErrorCode::E0003,
            format!("unknown enum `{enum_name}`"),
            span,
        ))?;
    let variant = enum_info.variants.iter()
        .find(|v| v.name == variant_name)
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
    for (field_name, typed_expr) in typed_fields {
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
    // If a type param remains unresolved (fieldless variants like `Perhaps::Nope`),
    // fall back to the annotation's args.
    // type_to_infer normalises Perhaps/Result into Named for uniform handling.
    let hint_args: Vec<Type> = expected_ty
        .map(|ty| {
            if let InferType::Named(n, args) = type_to_infer(ty) {
                if n == enum_name {
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
    infer_type_to_type(&InferType::Named(enum_name.to_string(), infer_args), span)
}

fn bind_enum_variant_fields(
    enum_name: &str,
    variant_name: &str,
    fields: &[String],
    scrutinee_ty: &Type,
    ctx: &mut ConstructCtx,
) -> Result<(), YoloscriptError> {
    let enum_info = ctx.enum_env.get(enum_name)
        .ok_or_else(|| YoloscriptError::internal(format!("unknown enum `{enum_name}`")))?
        .clone();
    let variant = enum_info.variants.iter()
        .find(|v| v.name == variant_name)
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
    Ok(())
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
    let typed_args: Vec<TypedExpr> = args.iter()
        .map(|a| construct_expr(a, None, ctx))
        .collect::<Result<_, _>>()?;
    let arg_types: Vec<&Type> = typed_args.iter().map(|a| a.ty()).collect();

    let (typed_callee, fun_ty) = match callee {
        Expr::Ident(name, ident_span) if ctx.lookup(name).is_none() => {
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

fn construct_literal_type(
    lit: &Literal,
    expected_ty: Option<&Type>,
    span: &Span,
) -> Result<Type, YoloscriptError> {
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
