use std::collections::HashMap;

use crate::ast::{Decl, Program, Span, TypeExpr, AspectDecl};
use crate::error::MoonlaneError;
use crate::typeinference::{
    EnumInfo, InferContext, InferType, TypeRegistry, TypeScheme, TypeVar, TypeVarGenerator,
    VariantInfo,
};
use crate::types::Type;

use super::conversions::{infer_type_to_type, type_expr_to_infer, type_expr_to_infer_with_generics};

fn dbg_scheme(t: TypeVar) -> TypeScheme {
    TypeScheme {
        quantified_vars: vec![t],
        ty: InferType::Fun(
            vec![InferType::Var(t)],
            Box::new(InferType::Var(t)),
        ),
    }
}

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

/// Build the `TypeRegistry` from the program's declarations and built-in types.
/// Allocates TypeVars from `gen`; the caller must pass the same `gen` to
/// `InferContext::new` so that all TypeVar IDs are globally unique.
pub(super) fn build_registry(program: &Program, gen: &mut TypeVarGenerator) -> TypeRegistry {
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
            Decl::Struct(sd) if sd.generics.is_empty() => {
                let fields = sd.fields.iter()
                    .map(|f| (f.name.clone(), type_expr_to_infer(&f.type_ann)))
                    .collect();
                registry.register_struct_fields(sd.name.clone(), fields);
            }
            Decl::Struct(sd) => {
                let mut gen_map: HashMap<String, TypeVar> = HashMap::new();
                let mut type_params = vec![];
                for gp in &sd.generics {
                    let tv = gen.fresh();
                    gen_map.insert(gp.name.clone(), tv);
                    type_params.push(tv);
                }
                let fields = sd.fields.iter()
                    .map(|f| (f.name.clone(), type_expr_to_infer_with_generics(&f.type_ann, &gen_map)))
                    .collect();
                registry.register_struct_fields(sd.name.clone(), fields);
                registry.register_struct_type_params(sd.name.clone(), type_params);
            }
            Decl::Enum(ed) => {
                let mut gen_map: HashMap<String, TypeVar> = HashMap::new();
                let mut type_params = vec![];
                for gp in &ed.generics {
                    let tv = gen.fresh();
                    gen_map.insert(gp.name.clone(), tv);
                    type_params.push(tv);
                }
                let variants = ed.variants.iter().map(|v| VariantInfo {
                    name: v.name.clone(),
                    fields: v.fields.iter()
                        .map(|f| (f.name.clone(), type_expr_to_infer_with_generics(&f.type_ann, &gen_map)))
                        .collect(),
                }).collect();
                registry.register_enum(ed.name.clone(), EnumInfo {
                    type_params,
                    variants,
                });
            }
            Decl::Aspect(ad) => {
                register_aspect_decl(ad, &mut registry);
            }
            Decl::Impl(ib) => {
                let target_name = match &ib.target_type {
                    TypeExpr::Named(name, _) => name.clone(),
                    _ => continue,
                };
                register_impl_methods(ib.methods.iter(), &target_name, gen, &mut registry);
            }
            _ => {}
        }
    }

    registry
}

fn register_aspect_decl(ad: &AspectDecl, registry: &mut TypeRegistry) {
    let method_names = ad.methods.iter().map(|m| m.name.clone()).collect();
    registry.register_aspect(ad.name.clone(), method_names);
}

fn register_impl_methods<'a>(
    methods: impl Iterator<Item = &'a crate::ast::FunDecl>,
    target_name: &str,
    gen: &mut TypeVarGenerator,
    registry: &mut TypeRegistry,
) {
    for method in methods {
        let mut param_types = vec![];
        for p in &method.params {
            let pt = if p.name == "self" {
                InferType::Named(target_name.to_string(), vec![])
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
            target_name.to_string(),
            method.name.clone(),
            InferType::Fun(param_types, Box::new(ret_ty)),
        );
    }
}

pub(super) fn register_builtins(ctx: &mut InferContext) {
    let str_ty   = InferType::str();
    let int_ty   = InferType::int();
    let float_ty = InferType::float();
    let bool_ty  = InferType::bool();
    let unit_ty  = InferType::unit();

    let mono = |params, ret| TypeScheme::mono(InferType::Fun(params, Box::new(ret)));

    ctx.bind_poly("print",           mono(vec![str_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("println",         mono(vec![str_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("print_int",       mono(vec![int_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("println_int",     mono(vec![int_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("print_float",     mono(vec![float_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("println_float",   mono(vec![float_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("int_to_string",   mono(vec![int_ty.clone()], str_ty.clone()));
    ctx.bind_poly("float_to_string", mono(vec![float_ty],       str_ty.clone()));
    ctx.bind_poly("bool_to_string",  mono(vec![bool_ty.clone()], str_ty.clone()));
    ctx.bind_poly("string_len",      mono(vec![str_ty.clone()], int_ty.clone()));
    ctx.bind_poly("string_concat",   mono(vec![str_ty.clone(), str_ty.clone()], str_ty.clone()));
    ctx.bind_poly("clock",           mono(vec![], int_ty.clone()));
    ctx.bind_poly("assert",          mono(vec![bool_ty.clone()], unit_ty.clone()));
    ctx.bind_poly("assert_msg",      mono(vec![bool_ty, str_ty.clone()], unit_ty.clone()));

    ctx.register_method("String".to_string(), "len".to_string(),
        InferType::Fun(vec![str_ty.clone()], Box::new(int_ty.clone())));

    let t = ctx.fresh_type_var_raw();
    ctx.bind_poly("array_push", array_push_scheme(t));
    let t = ctx.fresh_type_var_raw();
    ctx.bind_poly("array_len", array_len_scheme(t));
    let t = ctx.fresh_type_var_raw();
    ctx.bind_poly("dbg", dbg_scheme(t));
}

pub(super) fn register_builtin_poly_schemes(
    scheme_env: &mut HashMap<String, TypeScheme>,
    gen: &mut TypeVarGenerator,
) {
    let t = gen.fresh();
    scheme_env.insert("array_push".into(), array_push_scheme(t));
    let t = gen.fresh();
    scheme_env.insert("array_len".into(), array_len_scheme(t));
    let t = gen.fresh();
    scheme_env.insert("dbg".into(), dbg_scheme(t));
}

pub(super) fn build_concrete_struct_env(
    struct_env: &HashMap<String, Vec<(String, InferType)>>,
    struct_type_params: &HashMap<String, Vec<TypeVar>>,
    subst: &crate::typeinference::Substitution,
) -> Result<HashMap<String, Vec<(String, Type)>>, MoonlaneError> {
    let dummy = Span::new(0, 0, "");
    struct_env.iter()
        .filter(|(name, _)| !struct_type_params.contains_key(name.as_str()))
        .map(|(name, fields)| {
            let concrete = fields.iter()
                .map(|(fname, fty)| Ok((fname.clone(), infer_type_to_type(&subst.apply(fty), &dummy)?)))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((name.clone(), concrete))
        })
        .collect()
}

pub(super) fn build_concrete_method_env(
    method_env: &HashMap<String, HashMap<String, InferType>>,
    subst: &crate::typeinference::Substitution,
) -> Result<HashMap<String, HashMap<String, Type>>, MoonlaneError> {
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
