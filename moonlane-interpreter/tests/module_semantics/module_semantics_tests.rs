/// Integration tests for the v0.6.0 module semantics pipeline:
/// `load_root → resolve → normalize → check_graph → evaluate_graph`

use std::fs;
use std::path::{Path, PathBuf};

use moonlane::{evaluator, module_loader, name_resolver, path_normalizer, typechecker};

fn fixture_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "moonlane_module_semantics_{}_{}_{}",
        std::process::id(),
        nonce,
        name,
    ));
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("failed to create {}: {e}", dir.display()));
    dir
}

fn write(path: &Path, source: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
    }
    fs::write(path, source).unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
}

fn run_graph(main: &Path) -> Result<(), moonlane::error::MoonlaneError> {
    let graph = module_loader::load_root(main)?;
    let names = name_resolver::resolve(&graph)?;
    let normalized = path_normalizer::normalize(graph, &names)?;
    let typed = typechecker::check_graph(normalized, &names, typechecker::StdPrelude::empty())?;
    evaluator::evaluate_graph(typed)
}

fn run_graph_std(main: &Path) -> Result<(), moonlane::error::MoonlaneError> {
    let graph = module_loader::load_root(main)?;
    let names = name_resolver::resolve(&graph)?;
    let normalized = path_normalizer::normalize(graph, &names)?;
    let typed = typechecker::check_graph(normalized, &names, typechecker::StdPrelude::default())?;
    evaluator::evaluate_graph(typed)
}

// ── Basic single-module graph ─────────────────────────────────────────────────

#[test]
fn single_module_check_graph_runs() {
    let dir = fixture_dir("single");
    let main = dir.join("main.mln");
    write(&main, "fun main() { }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn single_module_with_arithmetic() {
    let dir = fixture_dir("arith");
    let main = dir.join("main.mln");
    write(&main, "fun main() -> Int { return 1 + 2; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── Multi-module graph ────────────────────────────────────────────────────────

#[test]
fn two_module_function_call() {
    let dir = fixture_dir("two_mod");
    let main = dir.join("main.mln");
    write(&main, "import helper::*;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn explicit_named_import_function_call() {
    let dir = fixture_dir("named_import");
    let main = dir.join("main.mln");
    write(&main, "import helper::answer;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 7; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn struct_imported_via_glob() {
    let dir = fixture_dir("struct_glob");
    let main = dir.join("main.mln");
    write(
        &main,
        r#"
import point::*;
fun main() -> Int {
    let p = Point { x: 3, y: 4 };
    return p.x;
}
"#,
    );
    write(&dir.join("point.mln"), "pub struct Point { x: Int, y: Int }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn transitive_dependency_via_graph_pipeline() {
    let dir = fixture_dir("transitive_graph");
    let main = dir.join("main.mln");
    write(&main, "import parser::*;\nfun main() -> Int { return parse(); }\n");
    write(
        &dir.join("parser.mln"),
        "import lexer::*;\npub fun parse() -> Int { return tokenize(); }\n",
    );
    write(&dir.join("lexer.mln"), "pub fun tokenize() -> Int { return 1; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── #176: glob import filters to public items only ───────────────────────────

#[test]
fn glob_import_makes_pub_items_accessible() {
    let dir = fixture_dir("glob_pub");
    let main = dir.join("main.mln");
    write(&main, "import helper::*;\nfun main() -> Int { return pub_fn(); }\n");
    write(&dir.join("helper.mln"), "pub fun pub_fn() -> Int { return 1; }\nfun private_fn() -> Int { return 2; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn glob_import_does_not_expose_private_items() {
    let dir = fixture_dir("glob_priv");
    let main = dir.join("main.mln");
    // `private_fn` is not pub — should not be callable even after `import helper::*`
    write(&main, "import helper::*;\nfun main() -> Int { return private_fn(); }\n");
    write(&dir.join("helper.mln"), "pub fun pub_fn() -> Int { return 1; }\nfun private_fn() -> Int { return 2; }\n");

    run_graph(&main).expect_err("private item via glob should fail");
}

// ── #175: alias resolution ────────────────────────────────────────────────────

#[test]
fn alias_import_makes_alias_callable() {
    let dir = fixture_dir("alias_ok");
    let main = dir.join("main.mln");
    write(&main, "import helper::answer as compute;\nfun main() -> Int { return compute(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn alias_import_original_name_not_in_scope() {
    // `answer` should not be resolvable after `import helper::answer as compute`
    let dir = fixture_dir("alias_orig_out");
    let main = dir.join("main.mln");
    write(&main, "import helper::answer as compute;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    // `answer` is not imported — only `compute` is. Should fail (T0003 or unresolved).
    run_graph(&main).expect_err("original name should not be in scope");
}

// ── #178: re-export propagation ──────────────────────────────────────────────

#[test]
fn facade_re_exports_item_and_consumer_can_use_it() {
    // facade.mln re-exports `answer` from helper.mln.
    // main.mln imports only from facade and calls `answer` without importing helper directly.
    let dir = fixture_dir("re_export");
    let main = dir.join("main.mln");
    write(&main, "import facade::answer;\nfun main() -> Int { return answer(); }\n");
    // facade imports answer from helper (so helper is loaded) and re-exports it
    write(&dir.join("facade.mln"), "import helper::answer;\nexport helper::answer;\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── T0011: import conflict detection ─────────────────────────────────────────

#[test]
fn two_explicit_imports_same_local_name_is_t0011() {
    let dir = fixture_dir("t0011_explicit");
    let main = dir.join("main.mln");
    // Both `import a::foo` and `import b::foo` bind local name `foo` → conflict
    write(&main, "import a::foo;\nimport b::foo;\nfun main() -> Int { return foo(); }\n");
    write(&dir.join("a.mln"), "pub fun foo() -> Int { return 1; }\n");
    write(&dir.join("b.mln"), "pub fun foo() -> Int { return 2; }\n");

    let err = run_graph(&main).expect_err("expected T0011");
    let msg = format!("{err}");
    assert!(msg.contains("T0011"), "expected T0011, got: {msg}");
}

#[test]
fn two_glob_imports_same_name_is_t0011() {
    let dir = fixture_dir("t0011_glob");
    let main = dir.join("main.mln");
    write(&main, "import a::*;\nimport b::*;\nfun main() -> Int { return foo(); }\n");
    write(&dir.join("a.mln"), "pub fun foo() -> Int { return 1; }\n");
    write(&dir.join("b.mln"), "pub fun foo() -> Int { return 2; }\n");

    let err = run_graph(&main).expect_err("expected T0011 on glob/glob conflict");
    let msg = format!("{err}");
    assert!(msg.contains("T0011"), "expected T0011, got: {msg}");
}

#[test]
fn explicit_import_wins_over_glob_same_name() {
    // Explicit import silently wins over glob that exports the same name.
    let dir = fixture_dir("t0011_explicit_wins");
    let main = dir.join("main.mln");
    write(&main, "import a::foo;\nimport b::*;\nfun main() -> Int { return foo(); }\n");
    write(&dir.join("a.mln"), "pub fun foo() -> Int { return 1; }\n");
    write(&dir.join("b.mln"), "pub fun foo() -> Int { return 2; }\n");

    // Should succeed — explicit import from `a` wins
    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn user_glob_wins_over_std_glob_same_name_no_t0011() {
    // Simulates the std::core auto-import scenario: a Std-tier glob and a User-tier
    // glob both export the same name. The User glob must win silently — no T0011.
    // (In production this will be triggered by std::core exporting `print`, `println`,
    //  etc. while user modules may also export or re-export those names.)
    //
    // We test the tier model indirectly by injecting a Std-tier glob directly into
    // the scope before name resolution runs, since there is no user syntax for Std globs.
    // The integration test is in evaluator tests once #201 lands.
    //
    // For now: two User globs with the same name *do* produce T0011 (unchanged),
    // and the test above covers that. This test documents the intended behaviour
    // for cross-tier resolution which will be exercised end-to-end by #201.
    //
    // Placeholder: this test will be expanded to a real end-to-end case in #201.
    let dir = fixture_dir("tier_model_placeholder");
    let main = dir.join("main.mln");
    write(&main, "import a::*;\nfun main() -> Int { return foo(); }\n");
    write(&dir.join("a.mln"), "pub fun foo() -> Int { return 42; }\n");
    // Single User glob — no conflict possible, must succeed.
    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── std::core auto-import (RFC-0030) ─────────────────────────────────────────

#[test]
fn print_available_without_explicit_import() {
    // print() must be in scope in every module without any import statement.
    let dir = fixture_dir("auto_import_print");
    let main = dir.join("main.mln");
    write(&main, "fun main() { print(42); }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn explicit_std_core_import_is_valid() {
    // `import std::core::print` should work and bring print into scope.
    let dir = fixture_dir("explicit_std_import");
    let main = dir.join("main.mln");
    write(&main, "import std::core::print;\nfun main() { print(\"hi\"); }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn local_function_shadows_std_core_auto_import() {
    // A user-defined `print` function must shadow the auto-imported std::core::print.
    let dir = fixture_dir("shadow_std_print");
    let main = dir.join("main.mln");
    write(&main, "fun print(x: Int) -> Int { return x + 1; }\nfun main() { print(1); }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── T0009: visibility enforcement ────────────────────────────────────────────

#[test]
fn importing_private_item_is_t0009() {
    let dir = fixture_dir("t0009_private");
    let main = dir.join("main.mln");
    write(&main, "import helper::secret;\nfun main() -> Int { return secret(); }\n");
    write(&dir.join("helper.mln"), "fun secret() -> Int { return 42; }\n");

    let err = run_graph(&main).expect_err("expected T0009");
    let msg = format!("{err}");
    assert!(msg.contains("T0009"), "expected T0009, got: {msg}");
}

#[test]
fn importing_nonexistent_name_is_t0003() {
    let dir = fixture_dir("t0003_absent");
    let main = dir.join("main.mln");
    write(&main, "import helper::nonexistent;\nfun main() -> Int { return nonexistent(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    let err = run_graph(&main).expect_err("expected T0003");
    let msg = format!("{err}");
    assert!(msg.contains("T0003"), "expected T0003, got: {msg}");
}

#[test]
fn importing_pub_item_is_accepted() {
    let dir = fixture_dir("t0009_pub_ok");
    let main = dir.join("main.mln");
    write(&main, "import helper::answer;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── T0010: pub declarations require explicit annotations ──────────────────────

#[test]
fn pub_fun_without_return_type_is_t0010() {
    let dir = fixture_dir("t0010_no_return");
    let main = dir.join("main.mln");
    write(&main, "import helper::*;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() { return 42; }\n");

    let err = run_graph(&main).expect_err("expected T0010 error");
    let msg = format!("{err}");
    assert!(msg.contains("T0010"), "expected T0010, got: {msg}");
}

#[test]
fn pub_fun_with_unannotated_param_is_t0010() {
    let dir = fixture_dir("t0010_no_param_ann");
    let main = dir.join("main.mln");
    write(&main, "import helper::*;\nfun main() -> Int { return double(2); }\n");
    write(&dir.join("helper.mln"), "pub fun double(x) -> Int { return x * 2; }\n");

    let err = run_graph(&main).expect_err("expected T0010 error");
    let msg = format!("{err}");
    assert!(msg.contains("T0010"), "expected T0010, got: {msg}");
}

#[test]
fn non_pub_fun_without_annotation_is_accepted() {
    let dir = fixture_dir("t0010_non_pub_ok");
    let main = dir.join("main.mln");
    write(&main, "import helper::*;\nfun main() -> Int { return call(); }\n");
    write(
        &dir.join("helper.mln"),
        "fun internal() { }\npub fun call() -> Int { return 0; }\n",
    );

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── Collision detection ───────────────────────────────────────────────────────

#[test]
fn duplicate_top_level_name_across_modules_runs_with_warning() {
    // Two modules each declare a private function with the same name.
    // Neither is exported (so no T0011 import conflict); the collision appears
    // only in the flat runtime environment and is detected as a warning.
    let dir = fixture_dir("collision");
    let main = dir.join("main.mln");
    write(
        &main,
        "import a::pub_a;\nimport b::pub_b;\nfun main() -> Int { return pub_a() + pub_b(); }\n",
    );
    write(&dir.join("a.mln"), "pub fun pub_a() -> Int { return 1; }\nfun helper() -> Int { return 10; }\n");
    write(&dir.join("b.mln"), "pub fun pub_b() -> Int { return 2; }\nfun helper() -> Int { return 20; }\n");

    // The program typechecks and evaluates — runtime collision of `helper` is a warning.
    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── Path normalization ────────────────────────────────────────────────────────

#[test]
fn qualified_call_normalized_to_bare_name() {
    // helper::answer() — the normalizer rewrites this to a bare `answer` lookup
    let dir = fixture_dir("qual_norm");
    let main = dir.join("main.mln");
    write(&main, "import helper::*;\nfun main() -> Int { return helper::answer(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 99; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn self_qualified_call_normalized() {
    let dir = fixture_dir("self_norm");
    let main = dir.join("main.mln");
    write(&main, "fun answer() -> Int { return 5; }\nfun main() -> Int { return self::answer(); }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── #181: remaining integration coverage ─────────────────────────────────────

#[test]
fn explicit_import_limits_scope_to_named_item() {
    // `import helper::answer` should make `answer` callable but not `other`.
    let dir = fixture_dir("explicit_scope_limit");
    let main = dir.join("main.mln");
    write(&main, "import helper::answer;\nfun main() -> Int { return other(); }\n");
    write(
        &dir.join("helper.mln"),
        "pub fun answer() -> Int { return 1; }\npub fun other() -> Int { return 2; }\n",
    );

    run_graph(&main).expect_err("non-imported name should not be in scope");
}

#[test]
fn transitive_item_not_accessible_without_direct_import() {
    // main imports parser; parser imports lexer.
    // main should NOT be able to call tokenize() from lexer without importing lexer directly.
    let dir = fixture_dir("transitive_isolation");
    let main = dir.join("main.mln");
    write(&main, "import parser::*;\nfun main() -> Int { return tokenize(); }\n");
    write(
        &dir.join("parser.mln"),
        "import lexer::*;\npub fun parse() -> Int { return tokenize(); }\n",
    );
    write(&dir.join("lexer.mln"), "pub fun tokenize() -> Int { return 1; }\n");

    run_graph(&main).expect_err("transitive item should not be accessible without direct import");
}

#[test]
fn root_qualified_path_in_non_root_module() {
    // parser.mln uses root::helper to resolve a sibling module from the root namespace.
    let dir = fixture_dir("root_path");
    let main = dir.join("main.mln");
    write(&main, "import parser::*;\nfun main() -> Int { return parse(); }\n");
    write(
        &dir.join("parser.mln"),
        "import root::helper::*;\npub fun parse() -> Int { return helper_fn(); }\n",
    );
    write(&dir.join("helper.mln"), "pub fun helper_fn() -> Int { return 7; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── v0.6.0 cross-feature combination tests ───────────────────────────────────

#[test]
fn pub_alias_and_re_export_combined() {
    // Exercises: T0010 (pub must be annotated), alias resolution,
    // and re-export propagation all in one program.
    let dir = fixture_dir("combined_v060");
    let main = dir.join("main.mln");
    // main imports via alias AND via re-exported name from facade
    write(
        &main,
        "import facade::compute;\nimport util::answer as get_answer;\nfun main() -> Int { return compute() + get_answer(); }\n",
    );
    // facade re-exports `compute` from impl module
    write(
        &dir.join("facade.mln"),
        "import impl_mod::compute;\nexport impl_mod::compute;\n",
    );
    write(&dir.join("impl_mod.mln"), "pub fun compute() -> Int { return 10; }\n");
    write(&dir.join("util.mln"), "pub fun answer() -> Int { return 32; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn glob_and_explicit_import_from_same_module() {
    // `import a::*` brings pub_a and pub_b; `import a::pub_a` explicitly — explicit wins, no T0011.
    let dir = fixture_dir("glob_explicit_same");
    let main = dir.join("main.mln");
    write(
        &main,
        "import a::*;\nimport a::pub_a;\nfun main() -> Int { return pub_a() + pub_b(); }\n",
    );
    write(
        &dir.join("a.mln"),
        "pub fun pub_a() -> Int { return 1; }\npub fun pub_b() -> Int { return 2; }\n",
    );

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn t0010_pub_struct_requires_field_type_annotations() {
    // pub struct fields already require type annotations by grammar; this verifies
    // a properly annotated pub struct compiles and its fields are accessible cross-module.
    let dir = fixture_dir("pub_struct_cross");
    let main = dir.join("main.mln");
    write(
        &main,
        "import point::Point;\nfun main() -> Int { let p = Point { x: 5, y: 3 }; return p.x; }\n",
    );
    write(&dir.join("point.mln"), "pub struct Point { x: Int, y: Int }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn group_import_with_alias_subset() {
    // `import math::{add, mul as multiply}` — group import with per-item alias
    let dir = fixture_dir("group_alias");
    let main = dir.join("main.mln");
    write(
        &main,
        "import math::{add, mul as multiply};\nfun main() -> Int { return add(multiply(3, 4), 10); }\n",
    );
    write(
        &dir.join("math.mln"),
        "pub fun add(a: Int, b: Int) -> Int { return a + b; }\npub fun mul(a: Int, b: Int) -> Int { return a * b; }\n",
    );

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}
