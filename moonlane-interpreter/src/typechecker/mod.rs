use std::collections::{HashMap, HashSet};

use crate::ast::{Decl, Program};
use crate::error::MoonlaneError;
use crate::module_loader::LoadedModule;
use crate::name_resolver::ResolvedNames;
use crate::path_normalizer::NormalizedModuleGraph;
use crate::typed_ast::{TypedDecl, TypedModule, TypedModuleGraph, TypedProgram};
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

// ── StdPrelude ────────────────────────────────────────────────────────────────

/// Pre-loaded standard library type schemes, seeded into GlobalExports before
/// the per-module typechecking loop begins.
pub struct StdPrelude {
    schemes: SchemeEnv,
}

impl StdPrelude {
    /// No standard library names pre-loaded. Use in tests that do not need std.
    pub fn empty() -> Self {
        Self { schemes: HashMap::new() }
    }

    /// Standard library types: Int, Float, Bool, String, Perhaps, Result.
    /// Builtins are already registered by `register_builtins`; this exists to
    /// make std:: imports resolve in the scope builder without a real std file.
    pub fn default() -> Self {
        Self { schemes: HashMap::new() }
    }
}

// ── GlobalExports ─────────────────────────────────────────────────────────────

type ModulePath = Vec<String>;

struct ModuleExports {
    pub_schemes: SchemeEnv,
}

struct GlobalExports {
    modules: HashMap<ModulePath, ModuleExports>,
}

impl GlobalExports {
    fn new() -> Self { Self { modules: HashMap::new() } }

    fn insert(&mut self, path: ModulePath, exports: ModuleExports) {
        self.modules.insert(path, exports);
    }

    fn get_scheme(&self, module_path: &[String], name: &str) -> Option<&TypeScheme> {
        self.modules.get(module_path)?.pub_schemes.get(name)
    }

    fn all_pub_schemes(&self, module_path: &[String]) -> Option<&SchemeEnv> {
        Some(&self.modules.get(module_path)?.pub_schemes)
    }
}

// ── check_graph ───────────────────────────────────────────────────────────────

/// Typecheck a normalized module graph. Processes modules in topological order
/// (dependencies before dependents); each module is typechecked against its
/// declared imports, with results accumulated into `GlobalExports`.
pub fn check_graph(
    graph: NormalizedModuleGraph,
    names: &ResolvedNames,
    std_prelude: StdPrelude,
) -> Result<TypedModuleGraph, MoonlaneError> {
    let mut global_exports = GlobalExports::new();

    // Seed std::core into GlobalExports so that std:: imports resolve.
    global_exports.insert(
        vec!["std".to_string(), "core".to_string()],
        ModuleExports { pub_schemes: std_prelude.schemes },
    );

    let mut typed_modules: Vec<TypedModule> = Vec::new();
    // Accumulated type-level decls (struct/enum/impl/aspect) from already-checked modules.
    // These are passed to build_registry so that imported types are known during typechecking.
    let mut type_context: Vec<Decl> = Vec::new();

    for loaded in graph.modules() {
        let imported_schemes = build_import_schemes(loaded, names, &global_exports);
        let (typed_decls, scheme_env) =
            check_impl(&loaded.program, &imported_schemes, &type_context)?;

        // Export pub names from this module's scheme_env.
        let pub_schemes = filter_pub_schemes(&scheme_env, loaded, names);
        global_exports.insert(loaded.module_path.clone(), ModuleExports { pub_schemes });

        // Accumulate this module's type decls for subsequent modules.
        for decl in &loaded.program.decls {
            if matches!(decl, Decl::Struct(_) | Decl::Enum(_) | Decl::Impl(_) | Decl::Aspect(_)) {
                type_context.push(decl.clone());
            }
        }

        typed_modules.push(TypedModule {
            module_path: loaded.module_path.clone(),
            decls: typed_decls,
        });
    }

    Ok(TypedModuleGraph { modules: typed_modules })
}

/// Build the set of imported name→scheme bindings for a module, drawn from
/// GlobalExports. Explicit imports take precedence over glob imports.
fn build_import_schemes(
    loaded: &LoadedModule,
    names: &ResolvedNames,
    global_exports: &GlobalExports,
) -> SchemeEnv {
    let mut env: SchemeEnv = HashMap::new();
    let Some(scope) = names.scopes.get(&loaded.module_path) else { return env };

    // Glob imports (lower priority — added first so explicit can override)
    for glob_module in &scope.globs {
        if let Some(all_schemes) = global_exports.all_pub_schemes(glob_module) {
            for (name, scheme) in all_schemes {
                env.entry(name.clone()).or_insert_with(|| scheme.clone());
            }
        }
    }

    // Explicit imports (higher priority — overwrite globs)
    for (local_name, binding) in &scope.explicit {
        if let Some(scheme) = global_exports.get_scheme(&binding.source_module, &binding.source_name) {
            env.insert(local_name.clone(), scheme.clone());
        }
    }

    env
}

/// Filter a module's scheme_env to only the names declared `pub` in that module.
fn filter_pub_schemes(
    scheme_env: &SchemeEnv,
    loaded: &LoadedModule,
    names: &ResolvedNames,
) -> SchemeEnv {
    let Some(pub_names) = names.pub_surface.get(&loaded.module_path) else {
        return HashMap::new();
    };
    scheme_env.iter()
        .filter(|(name, _)| pub_names.contains(name.as_str()))
        .map(|(name, scheme)| (name.clone(), scheme.clone()))
        .collect()
}

/// Run the type checker over an untyped AST, producing a fully typed AST.
pub fn check(program: Program) -> Result<TypedProgram, MoonlaneError> {
    let (decls, _) = check_impl(&program, &HashMap::new(), &[])?;
    Ok(decls)
}

/// Core typechecking pipeline.
///
/// - `imported_schemes`: type schemes from imported modules, seeded into the
///   inference context so imported names are visible.
/// - `type_context`: struct/enum/impl/aspect declarations from already-checked
///   modules, included in the type registry so imported types are known.
///
/// Returns `(typed_decls, scheme_env)` where `scheme_env` maps user-defined
/// function names to their inferred schemes (used by `filter_pub_schemes`).
fn check_impl(
    program: &Program,
    imported_schemes: &SchemeEnv,
    type_context: &[Decl],
) -> Result<(Vec<TypedDecl>, SchemeEnv), MoonlaneError> {
    // Build a registry program that includes type decls from imported modules
    // so the registry knows about all available struct/enum types.
    let registry_program = if type_context.is_empty() {
        program.clone()
    } else {
        let mut combined = Program {
            imports: Vec::new(),
            exports: Vec::new(),
            decls: type_context.to_vec(),
        };
        combined.decls.extend_from_slice(&program.decls);
        combined
    };

    let mut gen = TypeVarGenerator::new();
    let reg = registry::build_registry(&registry_program, &mut gen);
    let mut ctx = InferContext::new(reg, gen);

    // Seed imported name bindings before registering builtins.
    for (name, scheme) in imported_schemes {
        ctx.bind_poly(name, scheme.clone());
    }

    // Pre-pass: register built-in value bindings and hoist function names.
    registry::register_builtins(&mut ctx);
    inference::hoist_fun_decls(&program.decls, &mut ctx);

    // Pass 1: walk AST, emit constraints, collect function generalizations.
    let mut fun_generalizations: Vec<FunGeneralization> = vec![];
    inference::infer_program(program, &mut ctx, &mut fun_generalizations)?;
    let subst = ctx.solve()?;

    // Build SchemeEnv from user functions, then add all built-in schemes.
    let mut gen = ctx.split_gen();
    let mut scheme_env: SchemeEnv = HashMap::new();
    for fg in fun_generalizations {
        let resolved = subst.apply(&fg.fun_ty);
        let scheme = generalize(resolved, &fg.env_fvs);
        scheme_env.insert(fg.name, scheme);
    }
    registry::register_builtin_schemes(&mut scheme_env, &mut gen);

    // Imported schemes must be visible in the construction pass so calls to imported
    // functions can be constructed. Use or_insert so locally-defined names shadow imports.
    for (name, scheme) in imported_schemes {
        scheme_env.entry(name.clone()).or_insert_with(|| scheme.clone());
    }

    // Build concrete environments for Pass 2 using the full registry (includes imported types).
    let concrete_struct_env = registry::build_concrete_struct_env(ctx.registry().raw_struct_env(), ctx.registry().raw_struct_type_params(), &subst)?;
    let concrete_method_env = registry::build_concrete_method_env(ctx.registry().raw_method_env(), &subst)?;
    let enum_env = ctx.registry().raw_enum_env();
    let raw_struct_env = ctx.registry().raw_struct_env();
    let raw_struct_type_params = ctx.registry().raw_struct_type_params();

    // Pass 2: construct typed AST for the current module only.
    let typed_decls = construction::construct_program(
        program, &subst, &scheme_env,
        concrete_struct_env, raw_struct_env, raw_struct_type_params,
        concrete_method_env, enum_env, gen,
    )?;

    // Return the user-defined scheme_env (before builtin_schemes were added to it,
    // but builtins are already in register_builtin_schemes output so we recompute here).
    let user_scheme_env: SchemeEnv = scheme_env.into_iter()
        .filter(|(name, _)| {
            // Keep only user-defined names (not builtins registered by register_builtin_schemes).
            // Builtins are always available and don't need to be in GlobalExports.
            !matches!(name.as_str(),
                "print" | "println" | "string_len" | "parse_int" | "parse_float"
                | "int_to_string" | "float_to_string" | "bool_to_string" | "assert"
                | "string_to_chars" | "char_to_string"
            )
        })
        .collect();

    Ok((typed_decls, user_scheme_env))
}
