use std::collections::{HashMap, HashSet};

use crate::ast::{Decl, Program, Visibility};
use crate::error::MoonlaneError;
use crate::module_loader::LoadedModule;
use crate::name_resolver::{GlobTier, ResolvedNames};
use crate::path_normalizer::NormalizedModuleGraph;
use crate::error::TypeErrorCode;
use crate::typed_ast::{TypedDecl, TypedModule, TypedModuleGraph, TypedProgram};
use crate::typeinference::*;

mod construction;
mod conversions;
mod inference;
mod registry;

type SchemeEnv = HashMap<String, TypeScheme>;

// ── ScopedEnv ─────────────────────────────────────────────────────────────────

/// A single resolved import binding, tracking the source module for conflict
/// reporting. Used by `ScopedEnv` and by #177 (T0011 conflict detection).
#[allow(dead_code)]
enum Binding {
    /// Unambiguous: one scheme from one source module.
    Single { scheme: TypeScheme, source: ModulePath },
    /// Conflicting glob imports both export the same name.
    /// Deferred error: T0011 fires when the name is looked up.
    Conflict { sources: Vec<ModulePath> },
}

/// Per-module import scope, seeded imports-first then local declarations.
/// Used to build the `SchemeEnv` passed to `check_impl`. (#177 will use this.)
#[allow(dead_code)]
type ScopedEnv = HashMap<String, Binding>;

struct FunGeneralization {
    name:    String,
    fun_ty:  InferType,
    env_fvs: HashSet<TypeVar>,
}

// ── StdPrelude ────────────────────────────────────────────────────────────────

/// Pre-loaded standard library type schemes, seeded into GlobalExports before
/// the per-module typechecking loop begins.  `StdPrelude` is the single source
/// of truth for all builtin function signatures — `register_builtins` and
/// the construction pass both derive from it.
pub struct StdPrelude {
    schemes: SchemeEnv,
}

impl StdPrelude {
    /// No standard library names pre-loaded. Use in tests that do not need std.
    pub fn empty() -> Self {
        Self { schemes: HashMap::new() }
    }

    /// All built-in function schemes (print, assert, array_push, …).
    /// This is the canonical list — nothing else should register builtins.
    pub fn default() -> Self {
        let mut schemes = HashMap::new();
        let mut gen = TypeVarGenerator::new();
        registry::populate_std_schemes(&mut schemes, &mut gen);
        Self { schemes }
    }

    pub(super) fn schemes(&self) -> &SchemeEnv {
        &self.schemes
    }

    pub(super) fn contains(&self, name: &str) -> bool {
        self.schemes.contains_key(name)
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

// ── check_pub_annotations ─────────────────────────────────────────────────────

/// Enforce T0010: every `pub` function must have explicit return type and
/// explicit parameter type annotations. Runs before inference so errors are
/// surfaced early with clear messages rather than cryptic inference failures
/// when downstream modules attempt to import the function.
fn check_pub_annotations(
    loaded: &LoadedModule,
    names: &ResolvedNames,
) -> Result<(), MoonlaneError> {
    let pub_surface = match names.pub_surface.get(&loaded.module_path) {
        Some(s) => s,
        None => return Ok(()),
    };

    for decl in &loaded.program.decls {
        match decl {
            Decl::Fun(fd) if fd.visibility == Visibility::Public => {
                if !pub_surface.contains(fd.name.as_str()) {
                    continue;
                }
                if fd.return_type.is_none() {
                    return Err(MoonlaneError::type_error(
                        TypeErrorCode::T0010,
                        format!(
                            "public declaration `{}` requires an explicit return type annotation; \
                             add `-> <Type>` after the parameter list",
                            fd.name
                        ),
                        &fd.span,
                    ));
                }
                for param in &fd.params {
                    if param.type_ann.is_none() {
                        return Err(MoonlaneError::type_error(
                            TypeErrorCode::T0010,
                            format!(
                                "public declaration `{}` requires explicit type annotations on \
                                 all parameters; add `: <Type>` to parameter `{}`",
                                fd.name, param.name
                            ),
                            &param.span,
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// ── check_graph ───────────────────────────────────────────────────────────────

/// Typecheck a normalized module graph. Processes modules in topological order
/// (dependencies before dependents); each module is typechecked against its
/// declared imports, with results accumulated into `GlobalExports`.
/// Typecheck a normalized module graph. See ADR-0022 for the GlobalExports accumulator
/// pattern and the invariant that imported_schemes must reach both inference and construction.
pub fn check_graph(
    graph: NormalizedModuleGraph,
    names: &ResolvedNames,
    std_prelude: StdPrelude,
) -> Result<TypedModuleGraph, MoonlaneError> {
    let mut global_exports = GlobalExports::new();

    // Seed std::core into GlobalExports so that std:: imports and the auto-glob resolve.
    // StdPrelude is the single source of truth for all built-in schemes. See ADR-0027.
    global_exports.insert(
        vec!["std".to_string(), "core".to_string()],
        ModuleExports { pub_schemes: std_prelude.schemes().clone() },
    );

    let mut typed_modules: Vec<TypedModule> = Vec::new();
    // Accumulated type-level decls (struct/enum/impl/aspect) from already-checked modules.
    // These are passed to build_registry so that imported types are known during typechecking.
    let mut type_context: Vec<Decl> = Vec::new();

    for loaded in graph.modules() {
        check_pub_annotations(loaded, names)?;
        let imported_schemes = build_import_schemes(loaded, names, &global_exports, &graph)?;
        let (typed_decls, scheme_env) =
            check_impl(&loaded.program, &imported_schemes, &type_context, &std_prelude)?;

        // Export pub names from this module's scheme_env, plus re-exported names
        // pulled from their source modules in GlobalExports (#178).
        let pub_schemes = filter_pub_schemes(&scheme_env, loaded, names, &global_exports);
        global_exports.insert(loaded.module_path.clone(), ModuleExports { pub_schemes });

        // Accumulate this module's type decls for subsequent modules.
        for decl in &loaded.program.decls {
            if matches!(decl, Decl::Struct(_) | Decl::Enum(_) | Decl::Impl(_) | Decl::Aspect(_)) {
                type_context.push(decl.clone());
            }
        }

        // Collect import aliases (local_name → source_name where they differ).
        // Used by evaluate_graph to register aliased names in the flat environment.
        let import_aliases = names.scopes.get(&loaded.module_path)
            .map(|scope| {
                scope.explicit.iter()
                    .filter(|(local, binding)| *local != &binding.source_name)
                    .map(|(local, binding)| (local.clone(), binding.source_name.clone()))
                    .collect()
            })
            .unwrap_or_default();

        typed_modules.push(TypedModule {
            module_path: loaded.module_path.clone(),
            decls: typed_decls,
            import_aliases,
        });
    }

    Ok(TypedModuleGraph { modules: typed_modules })
}

/// Build the set of imported name→scheme bindings for a module, drawn from
/// GlobalExports. Explicit imports take precedence over glob imports.
///
/// For explicit imports: if the name is absent from GlobalExports, scans the
/// source module's `program.decls` in the graph to distinguish T0009 (private
/// item — declared but not pub) from T0003 (name does not exist). See #191.
fn build_import_schemes(
    loaded: &LoadedModule,
    names: &ResolvedNames,
    global_exports: &GlobalExports,
    graph: &NormalizedModuleGraph,
) -> Result<SchemeEnv, MoonlaneError> {
    let mut env: SchemeEnv = HashMap::new();
    let Some(scope) = names.scopes.get(&loaded.module_path) else { return Ok(env) };

    // Glob imports (lower priority — added first so explicit can override).
    // Process Std globs before User globs so User silently wins cross-tier conflicts.
    // T0011 fires only when two globs of the **same** tier export the same name. See ADR-0026.
    let mut glob_source: HashMap<String, (Vec<String>, GlobTier)> = HashMap::new();
    let ordered_globs = scope.globs.iter()
        .filter(|(t, _)| *t == GlobTier::Std)
        .chain(scope.globs.iter().filter(|(t, _)| *t == GlobTier::User));
    for (tier, glob_module) in ordered_globs {
        let Some(all_schemes) = global_exports.all_pub_schemes(glob_module) else { continue };
        for (name, scheme) in all_schemes {
            let conflict = glob_source.get(name.as_str()).map(|(s, t)| (s.clone(), t.clone()));
            match conflict {
                Some((prior_source, ref prior_tier)) if prior_tier == tier => {
                    // Same-tier conflict → T0011.
                    return Err(MoonlaneError::type_error(
                        TypeErrorCode::T0011,
                        format!(
                            "import conflict: `{name}` is exported by both `{}` and `{}`; \
                             use an explicit import to disambiguate: `import {}::{}` or `import {}::{}`",
                            prior_source.join("::"),
                            glob_module.join("::"),
                            prior_source.join("::"), name,
                            glob_module.join("::"), name,
                        ),
                        &crate::ast::Span::new(0, 0, loaded.file_path.display().to_string()),
                    ));
                }
                Some((_, GlobTier::User)) => {
                    // Prior User glob claimed this name; current Std glob cannot override.
                }
                _ => {
                    // Either no prior claim, or current User tier overrides a prior Std.
                    glob_source.insert(name.clone(), (glob_module.clone(), tier.clone()));
                    env.insert(name.clone(), scheme.clone());
                }
            }
        }
    }

    // Explicit imports (higher priority — overwrite globs).
    for (local_name, binding) in &scope.explicit {
        if let Some(scheme) = global_exports.get_scheme(&binding.source_module, &binding.source_name) {
            env.insert(local_name.clone(), scheme.clone());
        } else {
            // No function scheme — check if it is a public struct/enum/aspect (type-only import).
            let is_pub_type = names.pub_surface
                .get(&binding.source_module)
                .map_or(false, |surface| surface.contains(binding.source_name.as_str()));
            if is_pub_type {
                // Valid public struct/enum/aspect import — no scheme needed; type registry
                // handles these via type_context. Skip silently.
                continue;
            }
            // Check if the source module is in the graph (not std, which is not file-loaded).
            let src = graph.modules().iter()
                .find(|m| m.module_path == binding.source_module);
            if let Some(src_module) = src {
                let span = find_import_span(loaded, &binding.source_module, &binding.source_name);
                if decl_exists(&src_module.program, &binding.source_name) {
                    return Err(MoonlaneError::type_error(
                        TypeErrorCode::T0009,
                        format!(
                            "visibility error: `{}` is not public in module `{}`",
                            binding.source_name,
                            binding.source_module.join("::")
                        ),
                        &span,
                    ));
                } else {
                    return Err(MoonlaneError::type_error(
                        TypeErrorCode::T0003,
                        format!(
                            "cannot import `{}` from module `{}`: name does not exist",
                            binding.source_name,
                            binding.source_module.join("::")
                        ),
                        &span,
                    ));
                }
            }
            // Source not in graph (std or future external crate) — skip silently.
        }
    }

    Ok(env)
}

/// Find the span of the import declaration in `loaded` that references `source_name`
/// from `source_module`. Falls back to a file-level span if no match is found.
fn find_import_span(
    loaded: &LoadedModule,
    source_module: &[String],
    source_name: &str,
) -> crate::ast::Span {
    use crate::ast::{ImportTree, PathRoot};

    fn tree_contains(tree: &ImportTree, name: &str) -> bool {
        match tree {
            ImportTree::Name { name: n, alias } => n == name || alias.as_deref() == Some(name),
            ImportTree::Path { tree, .. } => tree_contains(tree, name),
            ImportTree::Group(items) => items.iter().any(|t| tree_contains(t, name)),
            ImportTree::Glob => false,
        }
    }

    for import in &loaded.program.imports {
        let root_matches = match &import.path.root {
            PathRoot::Name(n) => source_module.first().map(|s| s == n).unwrap_or(false),
            PathRoot::Self_ => source_module == &loaded.module_path,
            PathRoot::Root | PathRoot::Super => true,
            PathRoot::Std => false,
        };
        if root_matches && tree_contains(&import.path.tree, source_name) {
            return import.span.clone();
        }
    }
    crate::ast::Span::new(0, 0, loaded.file_path.display().to_string())
}

/// Returns true if `name` is declared as a top-level item in `program`,
/// regardless of visibility. Used to distinguish T0009 (private) from T0003 (absent).
fn decl_exists(program: &Program, name: &str) -> bool {
    program.decls.iter().any(|d| match d {
        Decl::Fun(f) => f.name == name,
        Decl::Struct(s) => s.name == name,
        Decl::Enum(e) => e.name == name,
        Decl::Aspect(a) => a.name == name,
        Decl::Let(l) => l.name == name,
        Decl::Mut(m) => m.name == name,
        Decl::Impl(_) | Decl::Stmt(_) => false,
    })
}

/// Build the public scheme export for a module: pub-declared names from its
/// own scheme_env, plus any re-exported names pulled from `global_exports`.
fn filter_pub_schemes(
    scheme_env: &SchemeEnv,
    loaded: &LoadedModule,
    names: &ResolvedNames,
    global_exports: &GlobalExports,
) -> SchemeEnv {
    let Some(pub_names) = names.pub_surface.get(&loaded.module_path) else {
        return HashMap::new();
    };

    // Locally-declared pub names from this module's inference output.
    let mut result: SchemeEnv = scheme_env.iter()
        .filter(|(name, _)| pub_names.contains(name.as_str()))
        .map(|(name, scheme)| (name.clone(), scheme.clone()))
        .collect();

    // Re-exported names: present in pub_surface but not in scheme_env.
    // Pull their schemes from the source module's GlobalExports entry.
    if let Some(scope) = names.scopes.get(&loaded.module_path) {
        for (local_name, binding) in &scope.re_exports {
            if pub_names.contains(local_name.as_str()) && !result.contains_key(local_name) {
                if let Some(scheme) = global_exports.get_scheme(&binding.source_module, &binding.source_name) {
                    result.insert(local_name.clone(), scheme.clone());
                }
            }
        }
    }

    result
}

/// Run the type checker over an untyped AST, producing a fully typed AST.
pub fn check(program: Program) -> Result<TypedProgram, MoonlaneError> {
    let (decls, _) = check_impl(&program, &HashMap::new(), &[], &StdPrelude::default())?;
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
    std_prelude: &StdPrelude,
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
    registry::register_builtins(&mut ctx, std_prelude);
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
    registry::register_builtin_schemes(&mut scheme_env, std_prelude);

    // Imported schemes must be visible in the construction pass so calls to imported
    // functions can be constructed. Use or_insert so locally-defined names shadow imports.
    // INVARIANT: imported_schemes must be seeded into BOTH InferContext (above, via
    // bind_poly) AND scheme_env (here). Missing either breaks one of the two passes.
    // See ADR-0022.
    for (name, scheme) in imported_schemes {
        scheme_env.entry(name.clone()).or_insert_with(|| scheme.clone());
    }

    // Pass 2: construct typed AST for the current module only.
    // The registry owns all type definitions; ConstructCtx derives concrete envs from it.
    let typed_decls = construction::construct_program(
        program, &subst, &scheme_env, ctx.registry(), gen,
    )?;

    // Return only user-defined names. Builtins (from StdPrelude) are available to
    // every module via the auto-glob and don't need to be in GlobalExports.
    let user_scheme_env: SchemeEnv = scheme_env.into_iter()
        .filter(|(name, _)| !std_prelude.contains(name))
        .collect();

    Ok((typed_decls, user_scheme_env))
}
