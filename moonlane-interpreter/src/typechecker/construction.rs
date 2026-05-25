use std::collections::HashMap;

use crate::ast::*;
use crate::error::{TypeErrorCode, MoonlaneError};
use crate::typed_ast::*;
use crate::typeinference::*;
use crate::types::Type;

use super::SchemeEnv;
use super::conversions::{infer_type_to_type, resolved_to_type, type_expr_to_infer, type_to_infer};

/// Scope-aware context for Pass 2. Mirrors InferContext's scope management but
/// holds concrete `Type` values; no constraint emission.
struct ConstructCtx<'a> {
    subst:        &'a Substitution,
    scheme_env:   &'a SchemeEnv,
    env:          Vec<HashMap<String, Type>>,
    struct_scopes: Vec<HashMap<String, Vec<(String, Type)>>>,
    /// Raw (InferType) fields and type params for generic structs, for field-type resolution.
    generic_struct_raw: &'a HashMap<String, Vec<(String, InferType)>>,
    generic_struct_type_params: &'a HashMap<String, Vec<TypeVar>>,
    method_env:   HashMap<String, HashMap<String, Type>>,
    enum_env:     &'a HashMap<String, EnumInfo>,
    /// Shared generator continued from Pass 1; keeps TypeVar identities globally unique.
    gen:          TypeVarGenerator,
    /// Return type of the innermost enclosing function (None = unit / unknown).
    current_return_ty: Option<Type>,
    /// Break value type of the innermost enclosing `loop` (None = no loop or bare break).
    current_break_ty:  Option<Type>,
}

impl<'a> ConstructCtx<'a> {
    fn new(
        subst:      &'a Substitution,
        scheme_env: &'a SchemeEnv,
        struct_env: HashMap<String, Vec<(String, Type)>>,
        generic_struct_raw: &'a HashMap<String, Vec<(String, InferType)>>,
        generic_struct_type_params: &'a HashMap<String, Vec<TypeVar>>,
        method_env: HashMap<String, HashMap<String, Type>>,
        enum_env:   &'a HashMap<String, EnumInfo>,
        gen:        TypeVarGenerator,
    ) -> Self {
        let mut ctx = Self {
            subst, scheme_env,
            env: vec![HashMap::new()],
            struct_scopes: vec![struct_env],  // global scope pre-pushed
            generic_struct_raw,
            generic_struct_type_params,
            method_env, enum_env, gen,
            current_return_ty: None,
            current_break_ty:  None,
        };
        let str_ty   = Type::Str;
        let int_ty   = Type::Int;
        let float_ty = Type::Float;
        let bool_ty  = Type::Bool;
        let unit_ty  = Type::Unit;
        let mono = |params, ret| Type::Fun(params, Box::new(ret));
        ctx.bind("print",           mono(vec![str_ty.clone()], unit_ty.clone()));
        ctx.bind("println",         mono(vec![str_ty.clone()], unit_ty.clone()));
        ctx.bind("print_int",       mono(vec![int_ty.clone()], unit_ty.clone()));
        ctx.bind("println_int",     mono(vec![int_ty.clone()], unit_ty.clone()));
        ctx.bind("print_float",     mono(vec![float_ty.clone()], unit_ty.clone()));
        ctx.bind("println_float",   mono(vec![float_ty.clone()], unit_ty.clone()));
        ctx.bind("int_to_string",   mono(vec![int_ty.clone()], str_ty.clone()));
        ctx.bind("float_to_string", mono(vec![float_ty],       str_ty.clone()));
        ctx.bind("bool_to_string",  mono(vec![bool_ty.clone()], str_ty.clone()));
        ctx.bind("string_len",      mono(vec![str_ty.clone()], int_ty.clone()));
        ctx.bind("string_concat",   mono(vec![str_ty.clone(), str_ty.clone()], str_ty.clone()));
        ctx.bind("clock",           mono(vec![], int_ty.clone()));
        ctx.bind("assert",          mono(vec![bool_ty.clone()], unit_ty.clone()));
        ctx.bind("assert_msg",      mono(vec![bool_ty, str_ty.clone()], unit_ty.clone()));
        ctx
    }

    fn push_scope(&mut self) { self.env.push(HashMap::new()); }
    fn pop_scope(&mut self)  { self.env.pop(); }

    fn push_struct_scope(&mut self) { self.struct_scopes.push(HashMap::new()); }
    fn pop_struct_scope(&mut self)  { self.struct_scopes.pop(); }

    fn register_local_struct(&mut self, name: String, fields: Vec<(String, Type)>) {
        self.struct_scopes.last_mut().unwrap().insert(name, fields);
    }

    fn get_struct_fields(&self, name: &str) -> Option<&Vec<(String, Type)>> {
        self.struct_scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn bind(&mut self, name: impl Into<String>, ty: Type) {
        self.env.last_mut().unwrap().insert(name.into(), ty);
    }

    fn lookup(&self, name: &str) -> Option<&Type> {
        self.env.iter().rev().find_map(|s| s.get(name))
    }

    fn push_return_type(&mut self, ty: Option<Type>) -> Option<Type> {
        std::mem::replace(&mut self.current_return_ty, ty)
    }
    fn pop_return_type(&mut self, prev: Option<Type>) {
        self.current_return_ty = prev;
    }
    fn push_break_type(&mut self, ty: Option<Type>) -> Option<Type> {
        std::mem::replace(&mut self.current_break_ty, ty)
    }
    fn pop_break_type(&mut self, prev: Option<Type>) {
        self.current_break_ty = prev;
    }
}

pub(super) fn construct_program(
    program:    &Program,
    subst:      &Substitution,
    scheme_env: &SchemeEnv,
    struct_env: HashMap<String, Vec<(String, Type)>>,
    generic_struct_raw: &HashMap<String, Vec<(String, InferType)>>,
    generic_struct_type_params: &HashMap<String, Vec<TypeVar>>,
    method_env: HashMap<String, HashMap<String, Type>>,
    enum_env:   &HashMap<String, EnumInfo>,
    gen:        TypeVarGenerator,
) -> Result<TypedProgram, MoonlaneError> {
    let mut ctx = ConstructCtx::new(subst, scheme_env, struct_env, generic_struct_raw, generic_struct_type_params, method_env, enum_env, gen);

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

fn construct_decl(decl: &Decl, ctx: &mut ConstructCtx) -> Result<TypedDecl, MoonlaneError> {
    match decl {
        Decl::Let(ld) => {
            // Let-polymorphism: if a closure is in scheme_env with quantified vars,
            // store it as GenericClosure. The name stays absent from ctx.env so call
            // sites use scheme_env instantiation in construct_call.
            if let Expr::Closure { params, return_type, body, span: cls_span } = &ld.value {
                if let Some(scheme) = ctx.scheme_env.get(ld.name.as_str()) {
                    if !scheme.quantified_vars.is_empty() {
                        return Ok(TypedDecl::Let(TypedLetDecl {
                            name:     ld.name.clone(),
                            type_ann: ld.type_ann.clone(),
                            value: TypedExpr::GenericClosure {
                                params:      params.clone(),
                                return_type: return_type.clone(),
                                body:        body.clone(),
                                ty:          Type::Unit,
                                span:        cls_span.clone(),
                            },
                            span: ld.span.clone(),
                        }));
                    }
                }
            }
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

fn construct_fun_decl(fun: &FunDecl, ctx: &mut ConstructCtx) -> Result<TypedDecl, MoonlaneError> {
    let scheme = ctx.scheme_env.get(&fun.name)
        .ok_or_else(|| MoonlaneError::internal(format!("missing type for fn `{}`", fun.name)))?
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
            _ => return Err(MoonlaneError::internal(format!("expected Fun type for `{}`", fun.name))),
        };
        ctx.push_scope();
        for (param, ty) in fun.params.iter().zip(param_types.iter()) {
            ctx.bind(&param.name, ty.clone());
        }
        let saved_return = ctx.push_return_type(ret_ty.clone());
        let typed_block = construct_block(&fun.body, ret_ty.as_ref(), ctx)?;
        ctx.pop_return_type(saved_return);
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

fn construct_impl_decl(ib: &ImplBlock, ctx: &mut ConstructCtx) -> Result<TypedDecl, MoonlaneError> {
    if ib.trait_name.is_some() {
        return Err(MoonlaneError::not_implemented("trait impl blocks not yet supported"));
    }
    let target_name = match &ib.target_type {
        TypeExpr::Named(name, _) => name.clone(),
        _ => return Err(MoonlaneError::not_implemented("generic impl blocks not yet supported")),
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
) -> Result<TypedFunDecl, MoonlaneError> {
    let self_ty = Type::Named(target_name.to_string(), vec![]);
    let param_types: Vec<Type> = method.params.iter()
        .map(|p| {
            if p.name == "self" {
                Ok(self_ty.clone())
            } else {
                p.type_ann.as_ref()
                    .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, &p.span))
                    .unwrap_or_else(|| Err(MoonlaneError::type_error(
                        TypeErrorCode::T0002,
                        format!("parameter `{}` needs a type annotation", p.name),
                        &p.span,
                    )))
            }
        })
        .collect::<Result<_, _>>()?;
    let ret_ty = method.return_type.as_ref()
        .map(|ann| resolved_to_type(&type_expr_to_infer(ann), ctx.subst, &method.span))
        .transpose()?;
    ctx.push_scope();
    for (p, ty) in method.params.iter().zip(param_types.iter()) {
        ctx.bind(&p.name, ty.clone());
    }
    let saved_return = ctx.push_return_type(ret_ty.clone());
    let typed_block = construct_block(&method.body, ret_ty.as_ref(), ctx)?;
    ctx.pop_return_type(saved_return);
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
) -> Result<TypedBlock, MoonlaneError> {
    ctx.push_scope();
    ctx.push_struct_scope();
    // Hoist struct/enum declarations defined in this block so they are available
    // for any expression in the block regardless of declaration order.
    for decl in &block.stmts {
        if let Decl::Struct(sd) = decl {
            let dummy = &sd.span;
            let fields = sd.fields.iter()
                .map(|f| {
                    let ty = resolved_to_type(&type_expr_to_infer(&f.type_ann), ctx.subst, dummy)?;
                    Ok((f.name.clone(), ty))
                })
                .collect::<Result<_, MoonlaneError>>()?;
            ctx.register_local_struct(sd.name.clone(), fields);
        }
    }
    let mut stmts = vec![];
    for stmt in &block.stmts {
        stmts.push(construct_decl(stmt, ctx)?);
    }
    let tail = match &block.tail {
        Some(e) => Some(Box::new(construct_expr(e, expected_tail_ty, ctx)?)),
        None    => None,
    };
    ctx.pop_struct_scope();
    ctx.pop_scope();
    Ok(TypedBlock { stmts, tail, span: block.span.clone() })
}

fn construct_stmt(stmt: &Stmt, ctx: &mut ConstructCtx) -> Result<TypedStmt, MoonlaneError> {
    match stmt {
        Stmt::Expr(e) => Ok(TypedStmt::Expr(construct_expr(e, None, ctx)?)),
        Stmt::Return(r) => {
            let return_ty = ctx.current_return_ty.clone();
            let value = match &r.value {
                Some(e) => Some(construct_expr(e, return_ty.as_ref(), ctx)?),
                None    => None,
            };
            Ok(TypedStmt::Return(TypedReturnStmt { value, span: r.span.clone() }))
        }
        Stmt::Break(bs) => {
            let break_ty = ctx.current_break_ty.clone();
            let value = match &bs.value {
                Some(e) => Some(construct_expr(e, break_ty.as_ref(), ctx)?),
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
                _ => return Err(MoonlaneError::internal("for-in over non-iterable type")),
            };
            ctx.push_scope();
            ctx.bind(&fi.binding, elem_ty);
            let body = construct_block(&fi.body, None, ctx)?;
            ctx.pop_scope();
            Ok(TypedStmt::ForIn(TypedForInStmt {
                binding: fi.binding.clone(), iterable, body, span: fi.span.clone(),
            }))
        }
    }
}

fn construct_expr(
    expr: &Expr,
    expected_ty: Option<&Type>,
    ctx: &mut ConstructCtx,
) -> Result<TypedExpr, MoonlaneError> {
    match expr {
        Expr::Literal(lit, span) => {
            let ty = construct_literal_type(lit, expected_ty, span)?;
            Ok(TypedExpr::Literal(lit.clone(), ty, span.clone()))
        }
        Expr::Ident(name, span) => {
            let ty = ctx.lookup(name).cloned().ok_or_else(|| MoonlaneError::type_error(
                TypeErrorCode::T0003,
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
                let ty = expected_ty.cloned().ok_or_else(|| MoonlaneError::type_error(
                    TypeErrorCode::T0002,
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
                _ => return Err(MoonlaneError::type_error(
                    TypeErrorCode::T0001,
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
            let (struct_name, type_args) = match typed_obj.ty() {
                Type::Named(name, args) => (name.clone(), args.clone()),
                t => return Err(MoonlaneError::internal(
                    format!("field access on non-struct type {t}")
                )),
            };
            let field_ty = if let Some(type_params) = ctx.generic_struct_type_params.get(&struct_name) {
                // Generic struct: look up raw InferType field, build remap, apply, convert.
                let raw_fields = ctx.generic_struct_raw.get(&struct_name)
                    .ok_or_else(|| MoonlaneError::internal(format!("missing raw fields for `{struct_name}`")))?;
                let raw_ty = raw_fields.iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, ty)| ty.clone())
                    .ok_or_else(|| MoonlaneError::internal(format!("no field `{field}` on `{struct_name}`")))?;
                let mut remap = Substitution::new();
                for (&tp, arg) in type_params.iter().zip(type_args.iter()) {
                    remap.bind(tp, type_to_infer(arg));
                }
                infer_type_to_type(&remap.apply(&raw_ty), span)?
            } else {
                ctx.get_struct_fields(&struct_name)
                    .and_then(|fs| fs.iter().find(|(n, _)| n == field))
                    .map(|(_, ty)| ty.clone())
                    .ok_or_else(|| MoonlaneError::internal(
                        format!("no field `{field}` on `{struct_name}`")
                    ))?
            };
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
                t => return Err(MoonlaneError::internal(
                    format!("method call on non-struct type {t}")
                )),
            };
            let method_fun_ty = ctx.method_env.get(&struct_name)
                .and_then(|m| m.get(method.as_str()))
                .cloned()
                .ok_or_else(|| MoonlaneError::internal(
                    format!("no method `{method}` on `{struct_name}`")
                ))?;
            let typed_args: Vec<TypedExpr> = args.iter()
                .map(|a| construct_expr(a, None, ctx))
                .collect::<Result<_, _>>()?;
            let ret_ty = match method_fun_ty {
                Type::Fun(_, ret) => *ret,
                _ => return Err(MoonlaneError::internal("method type is not a function")),
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
                if let Some(type_params) = ctx.generic_struct_type_params.get(type_name) {
                    // Generic struct: infer type args from the typed field values.
                    let raw_fields = ctx.generic_struct_raw.get(type_name.as_str())
                        .ok_or_else(|| MoonlaneError::internal(format!("missing raw fields for `{type_name}`")))?;
                    let mut remap: HashMap<TypeVar, InferType> = HashMap::new();
                    for &tp in type_params {
                        remap.entry(tp).or_insert_with(|| InferType::Var(tp));
                    }
                    // Match each field value type to its raw InferType param; resolve via subst.
                    for (fname, fexpr) in &typed_fields {
                        if let Some((_, raw_ty)) = raw_fields.iter().find(|(n, _)| n == fname) {
                            if let InferType::Var(v) = raw_ty {
                                if type_params.contains(v) {
                                    remap.insert(*v, type_to_infer(&fexpr.ty()));
                                }
                            }
                        }
                    }
                    let type_args: Vec<Type> = type_params.iter()
                        .map(|tp| {
                            let it = remap.get(tp).cloned().unwrap_or(InferType::Var(*tp));
                            infer_type_to_type(&ctx.subst.apply(&it), span)
                        })
                        .collect::<Result<_, _>>()?;
                    Type::Named(type_name.clone(), type_args)
                } else {
                    Type::Named(type_name.clone(), vec![])
                }
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
                return Err(MoonlaneError::internal("invalid path in construct"));
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
                    .unwrap_or_else(|| Err(MoonlaneError::type_error(
                        TypeErrorCode::T0002,
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
                _ => return Err(MoonlaneError::internal("? on non-Result value")),
            };
            Ok(TypedExpr::PropagateError { expr: Box::new(typed_expr), ty, span: span.clone() })
        }
        Expr::Match(m) => construct_match(m, expected_ty, ctx),
        Expr::Ascribe { expr, ann, span } => {
            let ty = resolved_to_type(&type_expr_to_infer(ann), ctx.subst, span)?;
            construct_expr(expr, Some(&ty), ctx)
        }

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
                    .ok_or_else(|| MoonlaneError::internal(
                        format!("tuple index {index} out of bounds")
                    ))?,
                _ => return Err(MoonlaneError::internal("tuple access on non-tuple")),
            };
            Ok(TypedExpr::TupleAccess {
                object: Box::new(typed_obj),
                index: *index,
                ty,
                span: span.clone(),
            })
        }
        Expr::Loop { body, span } => {
            let saved_break = ctx.push_break_type(expected_ty.cloned());
            let typed_body = construct_block(body, None, ctx)?;
            ctx.pop_break_type(saved_break);
            let ty = find_loop_break_type(&typed_body).unwrap_or(Type::Never);
            Ok(TypedExpr::Loop { body: typed_body, ty, span: span.clone() })
        }
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
        TypedExpr::Closure { .. } | TypedExpr::GenericClosure { .. } => None,
        _ => None,
    }
}

fn construct_match(m: &MatchExpr, expected_ty: Option<&Type>, ctx: &mut ConstructCtx) -> Result<TypedExpr, MoonlaneError> {
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
        let body = construct_block(&arm.body, expected_ty, ctx)?;
        typed_arms.push(TypedMatchArm {
            pattern: arm.pattern.clone(),
            guard,
            body,
            span: arm.span.clone(),
        });
        ctx.pop_scope();
    }
    check_match_exhaustiveness(&typed_arms, &scrutinee_ty, ctx.enum_env, &m.span)?;
    let expr_type = typed_arms.first()
        .map(|a| a.body.tail.as_ref().map(|e| e.ty().clone()).unwrap_or(Type::Unit))
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
) -> Result<(), MoonlaneError> {
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
        return Err(MoonlaneError::type_error(
            TypeErrorCode::T0008,
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
) -> Result<(), MoonlaneError> {
    match pattern {
        Pattern::Wildcard(_) | Pattern::Literal(_, _) | Pattern::Nope(_) => {}
        Pattern::Binding(name, _) => {
            ctx.bind(name, scrutinee_ty.clone());
        }
        Pattern::Tuple(pats, _) => {
            let elems = match scrutinee_ty {
                Type::Tuple(ts) => ts.clone(),
                _ => return Err(MoonlaneError::internal("tuple pattern on non-tuple")),
            };
            for (pat, elem_ty) in pats.iter().zip(elems.iter()) {
                construct_pattern_bindings(pat, elem_ty, ctx)?;
            }
        }
        Pattern::EnumVariant { path, fields, span } => {
            let [enum_name, variant_name] = path.as_slice() else {
                return Err(MoonlaneError::internal("invalid pattern path"));
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
) -> Result<Type, MoonlaneError> {
    // Resolve concrete type arguments using the same instantiate-then-unify
    // pattern as instantiate_scheme_for_call.
    let enum_info = ctx.enum_env.get(enum_name)
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
            format!("unknown enum `{enum_name}`"),
            span,
        ))?;
    let variant = enum_info.variants.iter()
        .find(|v| v.name == variant_name)
        .ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0003,
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
                    .ok_or_else(|| MoonlaneError::type_error(
                        TypeErrorCode::T0002,
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
) -> Result<(), MoonlaneError> {
    let enum_info = ctx.enum_env.get(enum_name)
        .ok_or_else(|| MoonlaneError::internal(format!("unknown enum `{enum_name}`")))?
        .clone();
    let variant = enum_info.variants.iter()
        .find(|v| v.name == variant_name)
        .ok_or_else(|| MoonlaneError::internal(format!("unknown variant `{variant_name}`")))?
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
            .ok_or_else(|| MoonlaneError::internal(
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
) -> Result<TypedExpr, MoonlaneError> {
    // For monomorphic callee identifiers already in scope, extract param types as hints so
    // inherently ambiguous args (bare `[]`, `nope`) can resolve without requiring ascription.
    // Generic (scheme-based) callees need arg types first for instantiation — no hints there.
    let param_hints: Vec<Option<Type>> = match callee {
        Expr::Ident(name, _) => {
            match ctx.lookup(name) {
                Some(Type::Fun(params, _)) if params.len() == args.len() =>
                    params.iter().map(|p| Some(p.clone())).collect(),
                _ => vec![None; args.len()],
            }
        }
        _ => vec![None; args.len()],
    };

    let typed_args: Vec<TypedExpr> = args.iter()
        .zip(param_hints.iter())
        .map(|(a, hint)| construct_expr(a, hint.as_ref(), ctx))
        .collect::<Result<_, _>>()?;
    let arg_types: Vec<&Type> = typed_args.iter().map(|a| a.ty()).collect();

    let (typed_callee, fun_ty) = match callee {
        Expr::Ident(name, ident_span) if ctx.lookup(name).is_none() => {
            let scheme = ctx.scheme_env.get(name.as_str()).ok_or_else(|| {
                MoonlaneError::type_error(TypeErrorCode::T0003, format!("undefined name `{name}`"), ident_span)
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
                return Err(MoonlaneError::type_error(
                    TypeErrorCode::T0004,
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
        _ => Err(MoonlaneError::type_error(
            TypeErrorCode::T0001,
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
) -> Result<Type, MoonlaneError> {
    let instance = instantiate(scheme, gen);

    let (params, ret) = match instance {
        InferType::Fun(p, r) => (p, r),
        _ => return Err(MoonlaneError::internal("scheme type is not a function")),
    };

    let mut subst = Substitution::new();
    for (param, arg_ty) in params.iter().zip(arg_types.iter()) {
        let arg_infer = type_to_infer(*arg_ty);
        let s = unify(&subst.apply(param), &arg_infer).map_err(|_| {
            MoonlaneError::type_error(TypeErrorCode::T0001, "argument type mismatch", span)
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
) -> Result<Type, MoonlaneError> {
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
        Literal::Nope     => expected_ty.cloned().ok_or_else(|| MoonlaneError::type_error(
            TypeErrorCode::T0002,
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
) -> Result<TypedExpr, MoonlaneError> {
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
) -> Result<TypedExpr, MoonlaneError> {
    let operand = construct_expr(operand, None, ctx)?;
    let ty = match op {
        UnaryOp::Neg => operand.ty().clone(),
        UnaryOp::Not => Type::Bool,
    };
    Ok(TypedExpr::UnaryOp(op.clone(), Box::new(operand), ty, span.clone()))
}
