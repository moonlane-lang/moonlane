use std::collections::HashMap;

use crate::ast::{Decl, Program, Span, TypeExpr, AspectDecl};
use crate::typeinference::{
    EnumInfo, FieldEntry, InferContext, InferType, TypeDefinitionRegistry, TypeScheme, TypeVar,
    TypeVarGenerator, VariantInfo,
};

use super::conversions::{type_expr_to_infer, type_expr_to_infer_with_generics};

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

fn print_scheme(t: TypeVar) -> TypeScheme {
    TypeScheme {
        quantified_vars: vec![t],
        ty: InferType::Fun(
            vec![InferType::Var(t)],
            Box::new(InferType::unit()),
        ),
    }
}

fn register_builtin_aspect_impls(registry: &mut TypeDefinitionRegistry) {
    use crate::types::Type;
    // Iterable impls for built-in sequence types
    registry.register_aspect_impl("Range".into(),          "Iterable".into(), vec![Type::Int]);
    registry.register_aspect_impl("RangeInclusive".into(), "Iterable".into(), vec![Type::Int]);
    // From impls for numeric conversions
    registry.register_aspect_impl("Int".into(),   "From".into(), vec![Type::Float]);
    registry.register_aspect_impl("Float".into(), "From".into(), vec![Type::Int]);
    // Display impls for built-in types (used by to_string method dispatch)
    registry.register_aspect_impl("Int".into(),    "Display".into(), vec![]);
    registry.register_aspect_impl("Float".into(),  "Display".into(), vec![]);
    registry.register_aspect_impl("Bool".into(),   "Display".into(), vec![]);
    registry.register_aspect_impl("String".into(), "Display".into(), vec![]);
}

/// Build the `TypeDefinitionRegistry` from the program's declarations and built-in types.
/// Allocates TypeVars from `gen`; the caller must pass the same `gen` to
/// `InferContext::new` so that all TypeVar IDs are globally unique.
pub(super) fn build_registry(program: &Program, gen: &mut TypeVarGenerator) -> TypeDefinitionRegistry {
    let mut registry = TypeDefinitionRegistry::new();
    register_builtin_aspect_impls(&mut registry);

    // Built-in generic enums use a synthetic span (no source file).
    let builtin_span = Span::new(0, 0, "<builtin>");

    // Register built-in generic enums.
    let t = gen.fresh();
    registry.register_enum("Perhaps".into(), EnumInfo {
        type_params: vec![t],
        variants: vec![
            VariantInfo { name: "Some".into(), fields: vec![("value".into(), InferType::Var(t), builtin_span.clone())] },
            VariantInfo { name: "None".into(), fields: vec![] },
        ],
    });
    let t = gen.fresh();
    let e = gen.fresh();
    registry.register_enum("Result".into(), EnumInfo {
        type_params: vec![t, e],
        variants: vec![
            VariantInfo { name: "Ok".into(),  fields: vec![("value".into(), InferType::Var(t), builtin_span.clone())] },
            VariantInfo { name: "Err".into(), fields: vec![("error".into(), InferType::Var(e), builtin_span.clone())] },
        ],
    });

    // Hoist user-defined structs, enums, and impl method signatures.
    for decl in &program.decls {
        match decl {
            Decl::Struct(sd) if sd.generics.is_empty() => {
                let fields: Vec<FieldEntry> = sd.fields.iter()
                    .map(|f| (f.name.clone(), type_expr_to_infer(&f.type_ann), f.span.clone()))
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
                let fields: Vec<FieldEntry> = sd.fields.iter()
                    .map(|f| (f.name.clone(), type_expr_to_infer_with_generics(&f.type_ann, &gen_map), f.span.clone()))
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
                        .map(|f| (f.name.clone(), type_expr_to_infer_with_generics(&f.type_ann, &gen_map), f.span.clone()))
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
                // Track which aspects this type implements (with concrete type args).
                if let Some(aspect_name) = &ib.aspect_name {
                    let type_args: Vec<crate::types::Type> = ib.aspect_type_args.iter()
                        .filter_map(|te| {
                            use super::conversions::type_expr_to_infer;
                            match type_expr_to_infer(te) {
                                InferType::Concrete(t) => Some(t),
                                InferType::Named(n, _) => Some(crate::types::Type::Named(n, vec![])),
                                _ => None,
                            }
                        })
                        .collect();
                    registry.register_aspect_impl(target_name.clone(), aspect_name.clone(), type_args);
                }
            }
            _ => {}
        }
    }

    registry
}

fn register_aspect_decl(ad: &AspectDecl, registry: &mut TypeDefinitionRegistry) {
    let method_names = ad.methods.iter().map(|m| m.name.clone()).collect();
    registry.register_aspect(ad.name.clone(), method_names);
}

fn register_impl_methods<'a>(
    methods: impl Iterator<Item = &'a crate::ast::FunDecl>,
    target_name: &str,
    gen: &mut TypeVarGenerator,
    registry: &mut TypeDefinitionRegistry,
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

/// Seed `ctx` with all built-in free-function bindings from `StdPrelude`,
/// plus built-in method registrations and aspect declarations.
pub(super) fn register_builtins(ctx: &mut InferContext, prelude: &super::StdPrelude) {
    let str_ty   = InferType::str();
    let int_ty   = InferType::int();
    let float_ty = InferType::float();
    let bool_ty  = InferType::bool();

    // Free-function builtins all come from StdPrelude — no separate list needed.
    for (name, scheme) in prelude.schemes() {
        ctx.bind_poly(name, scheme.clone());
    }

    // Methods are not free functions; they're not in StdPrelude::schemes.
    for type_name in &["Int", "Float", "Bool", "String"] {
        let self_ty = match *type_name {
            "Int"    => int_ty.clone(),
            "Float"  => float_ty.clone(),
            "Bool"   => bool_ty.clone(),
            "String" => str_ty.clone(),
            _ => unreachable!(),
        };
        ctx.register_method(type_name.to_string(), "to_string".to_string(),
            InferType::Fun(vec![self_ty], Box::new(str_ty.clone())));
    }
    ctx.register_method("String".to_string(), "len".to_string(),
        InferType::Fun(vec![str_ty.clone()], Box::new(int_ty.clone())));

    ctx.registry_mut().register_aspect("Display".into(),  vec!["to_string".into()]);
    ctx.registry_mut().register_aspect("Iterable".into(), vec!["next".into()]);
    ctx.registry_mut().register_aspect("From".into(),     vec!["from".into()]);
}

/// Add all built-in function schemes from `StdPrelude` to `scheme_env`.
/// Used by the construction pass so builtin names are known during typed-AST building.
pub(super) fn register_builtin_schemes(
    scheme_env: &mut HashMap<String, TypeScheme>,
    prelude: &super::StdPrelude,
) {
    for (name, scheme) in prelude.schemes() {
        scheme_env.insert(name.clone(), scheme.clone());
    }
}

/// Populate `map` with all built-in function schemes.
/// Called by `StdPrelude::default()` — this is the single canonical list.
pub(super) fn populate_std_schemes(map: &mut HashMap<String, TypeScheme>, gen: &mut TypeVarGenerator) {
    let mono = |params: Vec<InferType>, ret: InferType| {
        TypeScheme::mono(InferType::Fun(params, Box::new(ret)))
    };
    let str_ty   = InferType::str();
    let int_ty   = InferType::int();
    let bool_ty  = InferType::bool();
    let unit_ty  = InferType::unit();

    // Polymorphic builtins.
    let t = gen.fresh(); map.insert("print".into(),      print_scheme(t));
    let t = gen.fresh(); map.insert("println".into(),    print_scheme(t));
    let t = gen.fresh(); map.insert("array_push".into(), array_push_scheme(t));
    let t = gen.fresh(); map.insert("array_len".into(),  array_len_scheme(t));
    let t = gen.fresh(); map.insert("dbg".into(),        dbg_scheme(t));

    // Monomorphic builtins.
    map.insert("string_len".into(),    mono(vec![str_ty.clone()], int_ty.clone()));
    map.insert("string_concat".into(), mono(vec![str_ty.clone(), str_ty.clone()], str_ty.clone()));
    map.insert("clock".into(),         mono(vec![], int_ty.clone()));
    map.insert("assert".into(),        mono(vec![bool_ty.clone()], unit_ty.clone()));
    map.insert("assert_msg".into(),    mono(vec![bool_ty, str_ty], unit_ty));
}

