use std::collections::{HashMap, HashSet};

use crate::ast::{Decl, ImportTree, PathRoot, Span, Visibility};
use crate::error::{MoonlaneError, TypeErrorCode};
use crate::module_loader::{LoadedModule, ModuleGraph};

// ── Public types ──────────────────────────────────────────────────────────────

/// A single resolved import binding within a module's scope.
#[derive(Debug, Clone)]
pub struct ImportBinding {
    /// Canonical module path of the module that provides this name.
    pub source_module: Vec<String>,
    /// The name as declared in the source module.
    pub source_name: String,
    pub kind: BindingKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingKind {
    /// A specific item (type, function, constant, …).
    Item,
    /// A whole module imported as a handle (`import std::math;` → `math::sin()`).
    Module,
}

/// The resolved import scope for a single module.
#[derive(Debug, Clone)]
pub struct ModuleScope {
    pub module_path: Vec<String>,
    /// Explicit bindings: local_name → ImportBinding.
    /// Two explicit bindings with the same local_name are a compile error.
    pub explicit: HashMap<String, ImportBinding>,
    /// Glob-imported module paths (`import path::*`).
    /// Names from these modules are in scope at lower priority than explicit imports.
    /// Ambiguity between two glob-sourced names is deferred to use-site.
    pub globs: Vec<Vec<String>>,
    /// Re-exported names: local_name → source binding.
    /// These names are part of this module's public API surface for callers.
    pub re_exports: HashMap<String, ImportBinding>,
}

/// The output of the name resolution pass: one scope per loaded module.
#[derive(Debug, Clone)]
pub struct ResolvedNames {
    pub scopes: HashMap<Vec<String>, ModuleScope>,
    /// Combined public surface per module: local declarations + re-exports.
    /// Used by callers to check import visibility.
    pub pub_surface: HashMap<Vec<String>, HashSet<String>>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn resolve(graph: &ModuleGraph) -> Result<ResolvedNames, MoonlaneError> {
    let known_modules: HashSet<Vec<String>> = graph.modules.iter()
        .map(|m| m.module_path.clone())
        .collect();

    // First pass: collect locally-declared public names per module.
    let mut pub_surface: HashMap<Vec<String>, HashSet<String>> = graph.modules.iter()
        .map(|m| {
            let names = m.program.decls.iter()
                .filter_map(|d| decl_pub_name(d))
                .collect();
            (m.module_path.clone(), names)
        })
        .collect();

    // Second pass: process re-exports and extend pub_surface.
    // Simple single-pass (no support for transitive re-export chains; that's future work).
    for loaded in &graph.modules {
        let re_exported = collect_re_exports(loaded, &known_modules, &pub_surface)?;
        pub_surface.entry(loaded.module_path.clone())
            .or_default()
            .extend(re_exported.keys().cloned());
    }

    // Third pass: resolve imports using the final pub_surface.
    let mut scopes = HashMap::new();
    for loaded in &graph.modules {
        let scope = resolve_module(loaded, &known_modules, &pub_surface)?;
        scopes.insert(loaded.module_path.clone(), scope);
    }

    Ok(ResolvedNames { scopes, pub_surface })
}

/// Returns the name of a declaration if it is public.
fn decl_pub_name(decl: &Decl) -> Option<String> {
    match decl {
        Decl::Fun(d)    if d.visibility == Visibility::Public => Some(d.name.clone()),
        Decl::Struct(d) if d.visibility == Visibility::Public => Some(d.name.clone()),
        Decl::Enum(d)   if d.visibility == Visibility::Public => Some(d.name.clone()),
        Decl::Aspect(d) if d.visibility == Visibility::Public => Some(d.name.clone()),
        _ => None,
    }
}

// ── Per-module resolution ─────────────────────────────────────────────────────

fn resolve_module(
    loaded: &LoadedModule,
    known_modules: &HashSet<Vec<String>>,
    pub_surface: &HashMap<Vec<String>, HashSet<String>>,
) -> Result<ModuleScope, MoonlaneError> {
    let re_exports = collect_re_exports(loaded, known_modules, pub_surface)?;
    let mut scope = ModuleScope {
        module_path: loaded.module_path.clone(),
        explicit: HashMap::new(),
        globs: Vec::new(),
        re_exports,
    };

    for import in &loaded.program.imports {
        let base = absolute_base(&import.path.root, &loaded.module_path);
        process_tree(&base, &import.path.tree, known_modules, pub_surface, &mut scope, &import.span)?;
    }

    Ok(scope)
}

/// Collect re-exported names from a module's `export` declarations.
/// Returns a map of local_name → binding for each successfully resolved export.
fn collect_re_exports(
    loaded: &LoadedModule,
    known_modules: &HashSet<Vec<String>>,
    pub_surface: &HashMap<Vec<String>, HashSet<String>>,
) -> Result<HashMap<String, ImportBinding>, MoonlaneError> {
    let mut re_exports: HashMap<String, ImportBinding> = HashMap::new();

    for export in &loaded.program.exports {
        let base = absolute_base(&export.path.root, &loaded.module_path);
        process_export_tree(&base, &export.path.tree, known_modules, pub_surface, &mut re_exports, &export.span)?;
    }

    Ok(re_exports)
}

/// Walk an export path tree and populate the re_exports map.
fn process_export_tree(
    base: &[String],
    tree: &ImportTree,
    known_modules: &HashSet<Vec<String>>,
    pub_surface: &HashMap<Vec<String>, HashSet<String>>,
    re_exports: &mut HashMap<String, ImportBinding>,
    export_span: &Span,
) -> Result<(), MoonlaneError> {
    match tree {
        ImportTree::Glob => {
            // Re-export all public names from the base module.
            if let Some(names) = pub_surface.get(base) {
                for name in names {
                    re_exports.insert(name.clone(), ImportBinding {
                        source_module: base.to_vec(),
                        source_name: name.clone(),
                        kind: BindingKind::Item,
                    });
                }
            }
        }

        ImportTree::Name { name, alias } => {
            let local = alias.as_deref().unwrap_or(name.as_str()).to_string();
            let mut module_candidate = base.to_vec();
            module_candidate.push(name.clone());

            if known_modules.contains(&module_candidate) {
                // Re-exporting a module handle (unusual but allowed).
                re_exports.insert(local, ImportBinding {
                    source_module: module_candidate,
                    source_name: name.clone(),
                    kind: BindingKind::Module,
                });
            } else {
                // Item re-export: verify it's public in the source module.
                if let Some(surface) = pub_surface.get(base) {
                    if !surface.contains(name.as_str()) {
                        return Err(MoonlaneError::type_error(
                            TypeErrorCode::T0009,
                            format!(
                                "visibility error: cannot re-export `{name}` — it is not public in module `{}`",
                                base.join("::")
                            ),
                            export_span,
                        ));
                    }
                }
                re_exports.insert(local, ImportBinding {
                    source_module: base.to_vec(),
                    source_name: name.clone(),
                    kind: BindingKind::Item,
                });
            }
        }

        ImportTree::Path { name, tree } => {
            let mut new_base = base.to_vec();
            new_base.push(name.clone());
            process_export_tree(&new_base, tree, known_modules, pub_surface, re_exports, export_span)?;
        }

        ImportTree::Group(items) => {
            for item in items {
                process_export_tree(base, item, known_modules, pub_surface, re_exports, export_span)?;
            }
        }
    }
    Ok(())
}

/// Compute the absolute path prefix corresponding to a path root,
/// given the importing module's own path.
fn absolute_base(root: &PathRoot, current: &[String]) -> Vec<String> {
    match root {
        PathRoot::Root  => vec![],
        PathRoot::Std   => vec!["std".to_string()],
        PathRoot::Self_ => current.to_vec(),
        PathRoot::Super => {
            if current.is_empty() {
                vec![] // validated as error elsewhere; tolerate gracefully
            } else {
                current[..current.len() - 1].to_vec()
            }
        }
        // Hierarchical: parent_path ++ [n] — must match module_loader's child_path
        // construction. See ADR-0023.
        PathRoot::Name(n) => {
            let mut path = current.to_vec();
            path.push(n.clone());
            path
        }
    }
}

fn process_tree(
    base: &[String],
    tree: &ImportTree,
    known_modules: &HashSet<Vec<String>>,
    pub_items: &HashMap<Vec<String>, HashSet<String>>,
    scope: &mut ModuleScope,
    import_span: &Span,
) -> Result<(), MoonlaneError> {
    match tree {
        ImportTree::Glob => {
            scope.globs.push(base.to_vec());
        }

        ImportTree::Name { name, alias } => {
            let local = alias.as_deref().unwrap_or(name.as_str()).to_string();

            // Determine whether `base + name` is a known module path —
            // if so this is a module-handle import, not an item import.
            let mut module_candidate = base.to_vec();
            module_candidate.push(name.clone());

            let (source_module, kind) = if known_modules.contains(&module_candidate) {
                (module_candidate, BindingKind::Module)
            } else {
                // Record the binding regardless of visibility. See ADR-0024.
                // Visibility (T0009) and existence (T0003) are checked by the typechecker
                // in build_import_schemes, which has access to the full graph and GlobalExports.
                (base.to_vec(), BindingKind::Item)
            };

            add_explicit(scope, local, ImportBinding {
                source_module,
                source_name: name.clone(),
                kind,
            }, import_span)?;
        }

        ImportTree::Path { name, tree } => {
            let mut new_base = base.to_vec();
            new_base.push(name.clone());
            process_tree(&new_base, tree, known_modules, pub_items, scope, import_span)?;
        }

        ImportTree::Group(items) => {
            for item in items {
                process_tree(base, item, known_modules, pub_items, scope, import_span)?;
            }
        }
    }

    Ok(())
}

fn add_explicit(
    scope: &mut ModuleScope,
    local_name: String,
    binding: ImportBinding,
    import_span: &Span,
) -> Result<(), MoonlaneError> {
    if let Some(existing) = scope.explicit.get(&local_name) {
        return Err(MoonlaneError::type_error(
            TypeErrorCode::T0011,
            format!(
                "import conflict: `{local_name}` is imported from both `{}` and `{}`; \
                 use an explicit import to disambiguate: `import {}::{}` or `import {}::{}`",
                existing.source_module.join("::"),
                binding.source_module.join("::"),
                existing.source_module.join("::"), existing.source_name,
                binding.source_module.join("::"), binding.source_name,
            ),
            import_span,
        ));
    }
    scope.explicit.insert(local_name, binding);
    Ok(())
}

// ── Query helpers ─────────────────────────────────────────────────────────────

impl ModuleScope {
    /// Look up a local name in this scope.
    /// Returns the explicit binding if one exists, or the glob source modules
    /// that may provide the name (for deferred ambiguity checking).
    pub fn lookup(&self, name: &str) -> ScopeLookup<'_> {
        if let Some(binding) = self.explicit.get(name) {
            return ScopeLookup::Explicit(binding);
        }
        ScopeLookup::MaybeGlob(&self.globs)
    }
}

pub enum ScopeLookup<'a> {
    Explicit(&'a ImportBinding),
    /// The name was not explicitly imported; these glob sources may provide it.
    MaybeGlob(&'a Vec<Vec<String>>),
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Block, Decl, FunDecl, ImportDecl, ImportPath, ImportTree, PathRoot, Program, Span, Visibility};
    use crate::module_loader::{LoadedModule, ModuleGraph};
    use std::path::PathBuf;

    fn span() -> Span {
        Span::new(0, 0, "test")
    }

    fn make_import(root: PathRoot, tree: ImportTree) -> ImportDecl {
        ImportDecl { path: ImportPath { root, tree }, span: span() }
    }

    fn make_program(imports: Vec<ImportDecl>) -> Program {
        Program { imports, exports: vec![], decls: vec![] }
    }

    fn make_program_with_pubs(imports: Vec<ImportDecl>, pub_names: &[&str]) -> Program {
        let decls = pub_names.iter().map(|n| Decl::Fun(FunDecl {
            visibility: Visibility::Public,
            name: (*n).into(),
            generics: vec![],
            params: vec![],
            return_type: None,
            body: Block { stmts: vec![], tail: None, span: span() },
            span: span(),
        })).collect();
        Program { imports, exports: vec![], decls }
    }

    fn make_graph(modules: Vec<(Vec<String>, Program)>) -> ModuleGraph {
        let root = if modules.is_empty() { PathBuf::new() } else { PathBuf::from("root.mln") };
        let modules = modules.into_iter().map(|(path, program)| LoadedModule {
            module_path: path,
            file_path: PathBuf::from("test.mln"),
            program,
        }).collect();
        ModuleGraph { root, modules }
    }

    #[test]
    fn resolves_explicit_item_import() {
        // import parser::Token;
        let graph = make_graph(vec![
            (vec![], make_program(vec![
                make_import(PathRoot::Name("parser".into()), ImportTree::Name {
                    name: "Token".into(), alias: None,
                }),
            ])),
            (vec!["parser".into()], make_program_with_pubs(vec![], &["Token"])),
        ]);

        let names = resolve(&graph).unwrap();
        let root_scope = &names.scopes[&vec![]];
        let binding = root_scope.explicit.get("Token").expect("Token should be bound");
        assert_eq!(binding.source_module, vec!["parser"]);
        assert_eq!(binding.source_name, "Token");
        assert_eq!(binding.kind, BindingKind::Item);
    }

    #[test]
    fn resolves_alias_import() {
        // import parser::Token as Tok;
        let graph = make_graph(vec![
            (vec![], make_program(vec![
                make_import(PathRoot::Name("parser".into()), ImportTree::Name {
                    name: "Token".into(), alias: Some("Tok".into()),
                }),
            ])),
            (vec!["parser".into()], make_program_with_pubs(vec![], &["Token"])),
        ]);

        let names = resolve(&graph).unwrap();
        let root_scope = &names.scopes[&vec![]];
        assert!(root_scope.explicit.contains_key("Tok"), "alias Tok should be bound");
        assert!(!root_scope.explicit.contains_key("Token"), "original name Token should not be bound");
        let binding = &root_scope.explicit["Tok"];
        assert_eq!(binding.source_name, "Token");
    }

    #[test]
    fn resolves_group_import() {
        // import parser::{Ast, Token};
        let graph = make_graph(vec![
            (vec![], make_program(vec![
                make_import(PathRoot::Name("parser".into()), ImportTree::Group(vec![
                    ImportTree::Name { name: "Ast".into(), alias: None },
                    ImportTree::Name { name: "Token".into(), alias: None },
                ])),
            ])),
            (vec!["parser".into()], make_program_with_pubs(vec![], &["Ast", "Token"])),
        ]);

        let names = resolve(&graph).unwrap();
        let root_scope = &names.scopes[&vec![]];
        assert!(root_scope.explicit.contains_key("Ast"));
        assert!(root_scope.explicit.contains_key("Token"));
    }

    #[test]
    fn resolves_glob_import() {
        // import parser::*;
        let graph = make_graph(vec![
            (vec![], make_program(vec![
                make_import(PathRoot::Name("parser".into()), ImportTree::Glob),
            ])),
            (vec!["parser".into()], make_program(vec![])),
        ]);

        let names = resolve(&graph).unwrap();
        let root_scope = &names.scopes[&vec![]];
        assert!(root_scope.explicit.is_empty(), "glob should not add explicit bindings");
        assert_eq!(root_scope.globs, vec![vec!["parser".to_string()]]);
    }

    #[test]
    fn resolves_module_handle_import() {
        // import parser; — parser is a known module, so this is a handle import
        let graph = make_graph(vec![
            (vec![], make_program(vec![
                make_import(PathRoot::Root, ImportTree::Name {
                    name: "parser".into(), alias: None,
                }),
            ])),
            (vec!["parser".into()], make_program(vec![])),
        ]);

        let names = resolve(&graph).unwrap();
        let root_scope = &names.scopes[&vec![]];
        let binding = root_scope.explicit.get("parser").expect("parser handle should be bound");
        assert_eq!(binding.kind, BindingKind::Module);
        assert_eq!(binding.source_module, vec!["parser"]);
    }

    #[test]
    fn rejects_duplicate_explicit_import() {
        // import parser::Token;
        // import lexer::Token;  ← conflict
        let graph = make_graph(vec![
            (vec![], make_program(vec![
                make_import(PathRoot::Name("parser".into()), ImportTree::Name {
                    name: "Token".into(), alias: None,
                }),
                make_import(PathRoot::Name("lexer".into()), ImportTree::Name {
                    name: "Token".into(), alias: None,
                }),
            ])),
            (vec!["parser".into()], make_program_with_pubs(vec![], &["Token"])),
            (vec!["lexer".into()],  make_program_with_pubs(vec![], &["Token"])),
        ]);

        let err = resolve(&graph).expect_err("duplicate import should fail");
        assert!(err.to_string().contains("Token"), "error should mention Token");
    }

    #[test]
    fn private_item_import_is_recorded_for_typechecker() {
        // import parser::Token; where Token is private in parser.
        // The name_resolver records the binding; visibility enforcement (T0009)
        // happens in the typechecker's build_import_schemes which has access to
        // the full NormalizedModuleGraph to distinguish private from absent.
        let graph = make_graph(vec![
            (vec![], make_program(vec![
                make_import(PathRoot::Name("parser".into()), ImportTree::Name {
                    name: "Token".into(), alias: None,
                }),
            ])),
            (vec!["parser".into()], make_program(vec![])), // no pub declarations
        ]);

        let names = resolve(&graph).expect("name_resolver should not reject private imports");
        let root_scope = names.scopes.get(&vec![]).expect("root scope should exist");
        assert!(
            root_scope.explicit.contains_key("Token"),
            "Token binding should be recorded so the typechecker can produce T0009"
        );
    }

    #[test]
    fn resolves_root_absolute_path() {
        // import root::parser::Ast;
        let graph = make_graph(vec![
            (vec![], make_program(vec![
                make_import(PathRoot::Root, ImportTree::Path {
                    name: "parser".into(),
                    tree: Box::new(ImportTree::Name { name: "Ast".into(), alias: None }),
                }),
            ])),
            (vec!["parser".into()], make_program_with_pubs(vec![], &["Ast"])),
        ]);

        let names = resolve(&graph).unwrap();
        let root_scope = &names.scopes[&vec![]];
        let binding = root_scope.explicit.get("Ast").expect("Ast should be bound");
        assert_eq!(binding.source_module, vec!["parser"]);
    }

    #[test]
    fn resolves_self_relative_path() {
        // In module ["parser"], import self::child::Thing;
        let graph = make_graph(vec![
            (vec!["parser".into()], make_program(vec![
                make_import(PathRoot::Self_, ImportTree::Path {
                    name: "child".into(),
                    tree: Box::new(ImportTree::Name { name: "Thing".into(), alias: None }),
                }),
            ])),
            (vec!["parser".into(), "child".into()], make_program_with_pubs(vec![], &["Thing"])),
        ]);

        let names = resolve(&graph).unwrap();
        let parser_scope = &names.scopes[&vec!["parser".to_string()]];
        let binding = parser_scope.explicit.get("Thing").expect("Thing should be bound");
        assert_eq!(binding.source_module, vec!["parser", "child"]);
    }

    #[test]
    fn resolves_super_relative_path() {
        // In module ["parser", "child"], import super::Token;
        let graph = make_graph(vec![
            (vec!["parser".into(), "child".into()], make_program(vec![
                make_import(PathRoot::Super, ImportTree::Name {
                    name: "Token".into(), alias: None,
                }),
            ])),
            (vec!["parser".into()], make_program_with_pubs(vec![], &["Token"])),
        ]);

        let names = resolve(&graph).unwrap();
        let child_scope = &names.scopes[&vec!["parser".to_string(), "child".to_string()]];
        let binding = child_scope.explicit.get("Token").expect("Token should be bound");
        assert_eq!(binding.source_module, vec!["parser"]);
    }

    fn make_export(root: PathRoot, tree: ImportTree) -> crate::ast::ExportDecl {
        use crate::ast::ExportDecl;
        ExportDecl { path: ImportPath { root, tree }, span: span() }
    }

    #[test]
    fn facade_re_exports_item_for_callers() {
        // parser.mln: export ast::Ast;
        // Caller can import parser::Ast even though Ast is defined in ast.
        use crate::ast::ExportDecl;
        let ast_module_prog = make_program_with_pubs(vec![], &["Ast"]);
        let parser_prog = Program {
            imports: vec![],
            exports: vec![make_export(
                PathRoot::Name("ast".into()),
                ImportTree::Name { name: "Ast".into(), alias: None },
            )],
            decls: vec![],
        };
        let root_prog = make_program(vec![
            make_import(PathRoot::Name("parser".into()), ImportTree::Name {
                name: "Ast".into(), alias: None,
            }),
        ]);
        let graph = make_graph(vec![
            (vec![], root_prog),
            (vec!["parser".into()], parser_prog),
            // ast is imported by parser, so its path is ["parser", "ast"]
            (vec!["parser".into(), "ast".into()], ast_module_prog),
        ]);

        let names = resolve(&graph).unwrap();
        // parser's re_exports should include Ast
        let parser_scope = &names.scopes[&vec!["parser".to_string()]];
        assert!(parser_scope.re_exports.contains_key("Ast"), "parser should re-export Ast");
        // root should have imported Ast from parser
        let root_scope = &names.scopes[&vec![]];
        let binding = root_scope.explicit.get("Ast").expect("Ast should be importable from facade");
        assert_eq!(binding.source_module, vec!["parser"]);
    }

    #[test]
    fn re_export_alias_is_visible_not_original() {
        // parser.mln: export ast::Ast as Tree;
        use crate::ast::ExportDecl;
        let ast_module_prog = make_program_with_pubs(vec![], &["Ast"]);
        let parser_prog = Program {
            imports: vec![],
            exports: vec![make_export(
                PathRoot::Name("ast".into()),
                ImportTree::Name { name: "Ast".into(), alias: Some("Tree".into()) },
            )],
            decls: vec![],
        };
        let graph = make_graph(vec![
            (vec!["parser".into()], parser_prog),
            // ast is imported by parser, so its path is ["parser", "ast"]
            (vec!["parser".into(), "ast".into()], ast_module_prog),
        ]);

        let names = resolve(&graph).unwrap();
        let parser_scope = &names.scopes[&vec!["parser".to_string()]];
        assert!(parser_scope.re_exports.contains_key("Tree"), "aliased re-export Tree should appear");
        assert!(!parser_scope.re_exports.contains_key("Ast"), "original name Ast should not appear");
    }

    #[test]
    fn rejects_re_export_of_private_item() {
        // parser.mln: export ast::Hidden; where Hidden is private in ast
        use crate::ast::ExportDecl;
        let ast_module_prog = make_program(vec![]); // no pub declarations
        let parser_prog = Program {
            imports: vec![],
            exports: vec![make_export(
                PathRoot::Name("ast".into()),
                ImportTree::Name { name: "Hidden".into(), alias: None },
            )],
            decls: vec![],
        };
        let graph = make_graph(vec![
            (vec!["parser".into()], parser_prog),
            // ast is imported by parser, so its path is ["parser", "ast"]
            (vec!["parser".into(), "ast".into()], ast_module_prog),
        ]);

        let err = resolve(&graph).expect_err("re-exporting private item should fail");
        let msg = err.to_string();
        assert!(msg.contains("Hidden"), "error should mention Hidden");
        assert!(msg.contains("visibility"), "error should mention visibility");
    }

    #[test]
    fn glob_re_export_includes_all_public_names() {
        // parser.mln: export ast::*;
        use crate::ast::ExportDecl;
        let ast_module_prog = make_program_with_pubs(vec![], &["Ast", "Token"]);
        let parser_prog = Program {
            imports: vec![],
            exports: vec![make_export(
                PathRoot::Name("ast".into()),
                ImportTree::Glob,
            )],
            decls: vec![],
        };
        let root_prog = make_program(vec![
            make_import(PathRoot::Name("parser".into()), ImportTree::Name {
                name: "Ast".into(), alias: None,
            }),
        ]);
        let graph = make_graph(vec![
            (vec![], root_prog),
            (vec!["parser".into()], parser_prog),
            // ast is imported by parser, so its path is ["parser", "ast"]
            (vec!["parser".into(), "ast".into()], ast_module_prog),
        ]);

        let names = resolve(&graph).unwrap();
        let parser_scope = &names.scopes[&vec!["parser".to_string()]];
        assert!(parser_scope.re_exports.contains_key("Ast"));
        assert!(parser_scope.re_exports.contains_key("Token"));
        // root can import Ast from parser
        let root_scope = &names.scopes[&vec![]];
        assert!(root_scope.explicit.contains_key("Ast"));
    }
}
