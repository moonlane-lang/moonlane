use std::collections::{HashMap, HashSet};

use crate::ast::Program;
use crate::error::MoonlaneError;
use crate::typed_ast::TypedProgram;
use crate::typeinference::*;

mod construction;
mod conversions;
mod inference;
mod registry;

type SchemeEnv = HashMap<String, TypeScheme>;

struct FunGeneralization {
    name:    String,
    fun_ty:  InferType,
    env_fvs: HashSet<TypeVar>,
}

/// Run the type checker over an untyped AST, producing a fully typed AST.
pub fn check(program: Program) -> Result<TypedProgram, MoonlaneError> {
    // Pre-pass: build the type registry, then create the inference context.
    let mut gen = TypeVarGenerator::new();
    let reg = registry::build_registry(&program, &mut gen);
    let mut ctx = InferContext::new(reg, gen);

    // Pre-pass: register built-in value bindings and hoist function names.
    registry::register_builtins(&mut ctx);
    inference::hoist_fun_decls(&program.decls, &mut ctx);

    // Pass 1: walk AST, emit constraints, collect function generalizations.
    let mut fun_generalizations: Vec<FunGeneralization> = vec![];
    inference::infer_program(&program, &mut ctx, &mut fun_generalizations)?;
    let subst = ctx.solve()?;

    // Build SchemeEnv from user functions, then add all built-in schemes.
    // Hand off the generator so all remaining TypeVar allocations are globally unique.
    let mut gen = ctx.split_gen();
    let mut scheme_env: SchemeEnv = HashMap::new();
    for fg in fun_generalizations {
        let resolved = subst.apply(&fg.fun_ty);
        let scheme = generalize(resolved, &fg.env_fvs);
        scheme_env.insert(fg.name, scheme);
    }
    registry::register_builtin_schemes(&mut scheme_env, &mut gen);

    // Build concrete environments for Pass 2.
    let concrete_struct_env = registry::build_concrete_struct_env(ctx.registry().raw_struct_env(), ctx.registry().raw_struct_type_params(), &subst)?;
    let concrete_method_env = registry::build_concrete_method_env(ctx.registry().raw_method_env(), &subst)?;
    let enum_env = ctx.registry().raw_enum_env();

    let raw_struct_env = ctx.registry().raw_struct_env();
    let raw_struct_type_params = ctx.registry().raw_struct_type_params();

    // Pass 2: re-derive concrete types and build TypedAST.
    construction::construct_program(&program, &subst, &scheme_env, concrete_struct_env, raw_struct_env, raw_struct_type_params, concrete_method_env, enum_env, gen)
}
