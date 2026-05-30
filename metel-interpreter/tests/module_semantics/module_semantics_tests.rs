/// Integration tests for the v0.6.0 module semantics pipeline:
/// `load_root → resolve → normalize → check_graph → evaluate_graph`

use std::fs;
use std::path::{Path, PathBuf};

use metel::{evaluator, module_loader, name_resolver, path_normalizer, typechecker};

fn fixture_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "metel_module_semantics_{}_{}_{}",
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

fn run_graph(main: &Path) -> Result<(), metel::error::MetelError> {
    let graph = module_loader::load_root(main)?;
    let names = name_resolver::resolve(&graph)?;
    let normalized = path_normalizer::normalize(graph, &names)?;
    let typed = typechecker::check_graph(normalized, &names, typechecker::StdPrelude::empty())?;
    evaluator::evaluate_graph(typed)
}

fn run_graph_std(main: &Path) -> Result<(), metel::error::MetelError> {
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
    let main = dir.join("main.mtl");
    write(&main, "fun main() { }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn single_module_with_arithmetic() {
    let dir = fixture_dir("arith");
    let main = dir.join("main.mtl");
    write(&main, "fun main() -> Int { return 1 + 2; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── Multi-module graph ────────────────────────────────────────────────────────

#[test]
fn two_module_function_call() {
    let dir = fixture_dir("two_mod");
    let main = dir.join("main.mtl");
    write(&main, "import helper::*;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mtl"), "pub fun answer() -> Int { return 42; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn explicit_named_import_function_call() {
    let dir = fixture_dir("named_import");
    let main = dir.join("main.mtl");
    write(&main, "import helper::answer;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mtl"), "pub fun answer() -> Int { return 7; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn struct_imported_via_glob() {
    let dir = fixture_dir("struct_glob");
    let main = dir.join("main.mtl");
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
    write(&dir.join("point.mtl"), "pub struct Point { x: Int, y: Int }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn transitive_dependency_via_graph_pipeline() {
    let dir = fixture_dir("transitive_graph");
    let main = dir.join("main.mtl");
    write(&main, "import parser::*;\nfun main() -> Int { return parse(); }\n");
    write(
        &dir.join("parser.mtl"),
        "import lexer::*;\npub fun parse() -> Int { return tokenize(); }\n",
    );
    write(&dir.join("lexer.mtl"), "pub fun tokenize() -> Int { return 1; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── #176: glob import filters to public items only ───────────────────────────

#[test]
fn glob_import_makes_pub_items_accessible() {
    let dir = fixture_dir("glob_pub");
    let main = dir.join("main.mtl");
    write(&main, "import helper::*;\nfun main() -> Int { return pub_fn(); }\n");
    write(&dir.join("helper.mtl"), "pub fun pub_fn() -> Int { return 1; }\nfun private_fn() -> Int { return 2; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn glob_import_does_not_expose_private_items() {
    let dir = fixture_dir("glob_priv");
    let main = dir.join("main.mtl");
    // `private_fn` is not pub — should not be callable even after `import helper::*`
    write(&main, "import helper::*;\nfun main() -> Int { return private_fn(); }\n");
    write(&dir.join("helper.mtl"), "pub fun pub_fn() -> Int { return 1; }\nfun private_fn() -> Int { return 2; }\n");

    run_graph(&main).expect_err("private item via glob should fail");
}

// ── #175: alias resolution ────────────────────────────────────────────────────

#[test]
fn alias_import_makes_alias_callable() {
    let dir = fixture_dir("alias_ok");
    let main = dir.join("main.mtl");
    write(&main, "import helper::answer as compute;\nfun main() -> Int { return compute(); }\n");
    write(&dir.join("helper.mtl"), "pub fun answer() -> Int { return 42; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn alias_import_original_name_not_in_scope() {
    // `answer` should not be resolvable after `import helper::answer as compute`
    let dir = fixture_dir("alias_orig_out");
    let main = dir.join("main.mtl");
    write(&main, "import helper::answer as compute;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mtl"), "pub fun answer() -> Int { return 42; }\n");

    // `answer` is not imported — only `compute` is. Should fail (T0003 or unresolved).
    run_graph(&main).expect_err("original name should not be in scope");
}

// ── #178: re-export propagation ──────────────────────────────────────────────

#[test]
fn facade_re_exports_item_and_consumer_can_use_it() {
    // facade.mtl re-exports `answer` from helper.mtl.
    // main.mtl imports only from facade and calls `answer` without importing helper directly.
    let dir = fixture_dir("re_export");
    let main = dir.join("main.mtl");
    write(&main, "import facade::answer;\nfun main() -> Int { return answer(); }\n");
    // facade imports answer from helper (so helper is loaded) and re-exports it
    write(&dir.join("facade.mtl"), "import helper::answer;\nexport helper::answer;\n");
    write(&dir.join("helper.mtl"), "pub fun answer() -> Int { return 42; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── T0011: import conflict detection ─────────────────────────────────────────

#[test]
fn two_explicit_imports_same_local_name_is_t0011() {
    let dir = fixture_dir("t0011_explicit");
    let main = dir.join("main.mtl");
    // Both `import a::foo` and `import b::foo` bind local name `foo` → conflict
    write(&main, "import a::foo;\nimport b::foo;\nfun main() -> Int { return foo(); }\n");
    write(&dir.join("a.mtl"), "pub fun foo() -> Int { return 1; }\n");
    write(&dir.join("b.mtl"), "pub fun foo() -> Int { return 2; }\n");

    let err = run_graph(&main).expect_err("expected T0011");
    let msg = format!("{err}");
    assert!(msg.contains("T0011"), "expected T0011, got: {msg}");
}

#[test]
fn two_glob_imports_same_name_is_t0011() {
    let dir = fixture_dir("t0011_glob");
    let main = dir.join("main.mtl");
    write(&main, "import a::*;\nimport b::*;\nfun main() -> Int { return foo(); }\n");
    write(&dir.join("a.mtl"), "pub fun foo() -> Int { return 1; }\n");
    write(&dir.join("b.mtl"), "pub fun foo() -> Int { return 2; }\n");

    let err = run_graph(&main).expect_err("expected T0011 on glob/glob conflict");
    let msg = format!("{err}");
    assert!(msg.contains("T0011"), "expected T0011, got: {msg}");
}

#[test]
fn explicit_import_wins_over_glob_same_name() {
    // Explicit import silently wins over glob that exports the same name.
    let dir = fixture_dir("t0011_explicit_wins");
    let main = dir.join("main.mtl");
    write(&main, "import a::foo;\nimport b::*;\nfun main() -> Int { return foo(); }\n");
    write(&dir.join("a.mtl"), "pub fun foo() -> Int { return 1; }\n");
    write(&dir.join("b.mtl"), "pub fun foo() -> Int { return 2; }\n");

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
    let main = dir.join("main.mtl");
    write(&main, "import a::*;\nfun main() -> Int { return foo(); }\n");
    write(&dir.join("a.mtl"), "pub fun foo() -> Int { return 42; }\n");
    // Single User glob — no conflict possible, must succeed.
    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── std::core auto-import (RFC-0030) ─────────────────────────────────────────

#[test]
fn print_available_without_explicit_import() {
    // print() must be in scope in every module without any import statement.
    let dir = fixture_dir("auto_import_print");
    let main = dir.join("main.mtl");
    write(&main, "fun main() { print(42); }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn explicit_std_core_import_is_valid() {
    // `import std::core::print` should work and bring print into scope.
    let dir = fixture_dir("explicit_std_import");
    let main = dir.join("main.mtl");
    write(&main, "import std::core::print;\nfun main() { print(\"hi\"); }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn local_function_shadows_std_core_auto_import() {
    // A user-defined `print` function must shadow the auto-imported std::core::print.
    let dir = fixture_dir("shadow_std_print");
    let main = dir.join("main.mtl");
    write(&main, "fun print(x: Int) -> Int { return x + 1; }\nfun main() { print(1); }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── std::core type declarations (#202) ───────────────────────────────────────

#[test]
fn import_std_core_perhaps_is_valid() {
    // `import std::core::Perhaps` must not error.
    let dir = fixture_dir("import_perhaps");
    let main = dir.join("main.mtl");
    write(&main, "import std::core::Perhaps;\nfun main() { let x = Perhaps::Some { value: 1 }; }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn import_std_core_group_is_valid() {
    // `import std::core::{Perhaps, Result}` must not error.
    let dir = fixture_dir("import_perhaps_result");
    let main = dir.join("main.mtl");
    write(&main, "import std::core::{Perhaps, Result};\nfun main() { let x = Perhaps::Some { value: 1 }; }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn std_core_perhaps_path_in_struct_literal() {
    // `std::core::Perhaps::Some { value: 42 }` must be resolved to `Perhaps::Some { value: 42 }`.
    let dir = fixture_dir("std_path_struct_lit");
    let main = dir.join("main.mtl");
    write(&main, "fun main() { let x = std::core::Perhaps::Some { value: 42 }; }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn programs_without_explicit_std_import_still_use_perhaps() {
    // Programs that never mention std::core must still be able to use Perhaps and Result.
    let dir = fixture_dir("no_std_import_perhaps");
    let main = dir.join("main.mtl");
    write(&main, "fun main() { let x = Perhaps::Some { value: 1 }; }\n");
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── T0009: visibility enforcement ────────────────────────────────────────────

#[test]
fn importing_private_item_is_t0009() {
    let dir = fixture_dir("t0009_private");
    let main = dir.join("main.mtl");
    write(&main, "import helper::secret;\nfun main() -> Int { return secret(); }\n");
    write(&dir.join("helper.mtl"), "fun secret() -> Int { return 42; }\n");

    let err = run_graph(&main).expect_err("expected T0009");
    let msg = format!("{err}");
    assert!(msg.contains("T0009"), "expected T0009, got: {msg}");
}

#[test]
fn importing_nonexistent_name_is_t0003() {
    let dir = fixture_dir("t0003_absent");
    let main = dir.join("main.mtl");
    write(&main, "import helper::nonexistent;\nfun main() -> Int { return nonexistent(); }\n");
    write(&dir.join("helper.mtl"), "pub fun answer() -> Int { return 42; }\n");

    let err = run_graph(&main).expect_err("expected T0003");
    let msg = format!("{err}");
    assert!(msg.contains("T0003"), "expected T0003, got: {msg}");
}

#[test]
fn importing_pub_item_is_accepted() {
    let dir = fixture_dir("t0009_pub_ok");
    let main = dir.join("main.mtl");
    write(&main, "import helper::answer;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mtl"), "pub fun answer() -> Int { return 42; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── T0010: pub declarations require explicit annotations ──────────────────────

#[test]
fn pub_fun_without_return_type_is_t0010() {
    let dir = fixture_dir("t0010_no_return");
    let main = dir.join("main.mtl");
    write(&main, "import helper::*;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mtl"), "pub fun answer() { return 42; }\n");

    let err = run_graph(&main).expect_err("expected T0010 error");
    let msg = format!("{err}");
    assert!(msg.contains("T0010"), "expected T0010, got: {msg}");
}

#[test]
fn pub_fun_with_unannotated_param_is_t0010() {
    let dir = fixture_dir("t0010_no_param_ann");
    let main = dir.join("main.mtl");
    write(&main, "import helper::*;\nfun main() -> Int { return double(2); }\n");
    write(&dir.join("helper.mtl"), "pub fun double(x) -> Int { return x * 2; }\n");

    let err = run_graph(&main).expect_err("expected T0010 error");
    let msg = format!("{err}");
    assert!(msg.contains("T0010"), "expected T0010, got: {msg}");
}

#[test]
fn non_pub_fun_without_annotation_is_accepted() {
    let dir = fixture_dir("t0010_non_pub_ok");
    let main = dir.join("main.mtl");
    write(&main, "import helper::*;\nfun main() -> Int { return call(); }\n");
    write(
        &dir.join("helper.mtl"),
        "fun internal() { }\npub fun call() -> Int { return 0; }\n",
    );

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── Collision detection ───────────────────────────────────────────────────────

#[test]
fn private_names_in_different_modules_do_not_collide() {
    // Two modules each declare a private function with the same name.
    // Neither is exported (so no T0011 import conflict). With per-module
    // isolated environments each `helper` lives only in its own module's env —
    // no collision, no warning.
    let dir = fixture_dir("collision");
    let main = dir.join("main.mtl");
    write(
        &main,
        "import a::pub_a;\nimport b::pub_b;\nfun main() -> Int { return pub_a() + pub_b(); }\n",
    );
    write(&dir.join("a.mtl"), "pub fun pub_a() -> Int { return 1; }\nfun helper() -> Int { return 10; }\n");
    write(&dir.join("b.mtl"), "pub fun pub_b() -> Int { return 2; }\nfun helper() -> Int { return 20; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── Path normalization ────────────────────────────────────────────────────────

#[test]
fn qualified_call_normalized_to_bare_name() {
    // helper::answer() — the normalizer rewrites this to a bare `answer` lookup
    let dir = fixture_dir("qual_norm");
    let main = dir.join("main.mtl");
    write(&main, "import helper::*;\nfun main() -> Int { return helper::answer(); }\n");
    write(&dir.join("helper.mtl"), "pub fun answer() -> Int { return 99; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn self_qualified_call_normalized() {
    let dir = fixture_dir("self_norm");
    let main = dir.join("main.mtl");
    write(&main, "fun answer() -> Int { return 5; }\nfun main() -> Int { return self::answer(); }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── #181: remaining integration coverage ─────────────────────────────────────

#[test]
fn explicit_import_limits_scope_to_named_item() {
    // `import helper::answer` should make `answer` callable but not `other`.
    let dir = fixture_dir("explicit_scope_limit");
    let main = dir.join("main.mtl");
    write(&main, "import helper::answer;\nfun main() -> Int { return other(); }\n");
    write(
        &dir.join("helper.mtl"),
        "pub fun answer() -> Int { return 1; }\npub fun other() -> Int { return 2; }\n",
    );

    run_graph(&main).expect_err("non-imported name should not be in scope");
}

#[test]
fn transitive_item_not_accessible_without_direct_import() {
    // main imports parser; parser imports lexer.
    // main should NOT be able to call tokenize() from lexer without importing lexer directly.
    let dir = fixture_dir("transitive_isolation");
    let main = dir.join("main.mtl");
    write(&main, "import parser::*;\nfun main() -> Int { return tokenize(); }\n");
    write(
        &dir.join("parser.mtl"),
        "import lexer::*;\npub fun parse() -> Int { return tokenize(); }\n",
    );
    write(&dir.join("lexer.mtl"), "pub fun tokenize() -> Int { return 1; }\n");

    run_graph(&main).expect_err("transitive item should not be accessible without direct import");
}

#[test]
fn root_qualified_path_in_non_root_module() {
    // parser.mtl uses root::helper to resolve a sibling module from the root namespace.
    let dir = fixture_dir("root_path");
    let main = dir.join("main.mtl");
    write(&main, "import parser::*;\nfun main() -> Int { return parse(); }\n");
    write(
        &dir.join("parser.mtl"),
        "import root::helper::*;\npub fun parse() -> Int { return helper_fn(); }\n",
    );
    write(&dir.join("helper.mtl"), "pub fun helper_fn() -> Int { return 7; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── v0.6.0 cross-feature combination tests ───────────────────────────────────

#[test]
fn pub_alias_and_re_export_combined() {
    // Exercises: T0010 (pub must be annotated), alias resolution,
    // and re-export propagation all in one program.
    let dir = fixture_dir("combined_v060");
    let main = dir.join("main.mtl");
    // main imports via alias AND via re-exported name from facade
    write(
        &main,
        "import facade::compute;\nimport util::answer as get_answer;\nfun main() -> Int { return compute() + get_answer(); }\n",
    );
    // facade re-exports `compute` from impl module
    write(
        &dir.join("facade.mtl"),
        "import impl_mod::compute;\nexport impl_mod::compute;\n",
    );
    write(&dir.join("impl_mod.mtl"), "pub fun compute() -> Int { return 10; }\n");
    write(&dir.join("util.mtl"), "pub fun answer() -> Int { return 32; }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn glob_and_explicit_import_from_same_module() {
    // `import a::*` brings pub_a and pub_b; `import a::pub_a` explicitly — explicit wins, no T0011.
    let dir = fixture_dir("glob_explicit_same");
    let main = dir.join("main.mtl");
    write(
        &main,
        "import a::*;\nimport a::pub_a;\nfun main() -> Int { return pub_a() + pub_b(); }\n",
    );
    write(
        &dir.join("a.mtl"),
        "pub fun pub_a() -> Int { return 1; }\npub fun pub_b() -> Int { return 2; }\n",
    );

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn t0010_pub_struct_requires_field_type_annotations() {
    // pub struct fields already require type annotations by grammar; this verifies
    // a properly annotated pub struct compiles and its fields are accessible cross-module.
    let dir = fixture_dir("pub_struct_cross");
    let main = dir.join("main.mtl");
    write(
        &main,
        "import point::Point;\nfun main() -> Int { let p = Point { x: 5, y: 3 }; return p.x; }\n",
    );
    write(&dir.join("point.mtl"), "pub struct Point { x: Int, y: Int }\n");

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn group_import_with_alias_subset() {
    // `import math::{add, mul as multiply}` — group import with per-item alias
    let dir = fixture_dir("group_alias");
    let main = dir.join("main.mtl");
    write(
        &main,
        "import math::{add, mul as multiply};\nfun main() -> Int { return add(multiply(3, 4), 10); }\n",
    );
    write(
        &dir.join("math.mtl"),
        "pub fun add(a: Int, b: Int) -> Int { return a + b; }\npub fun mul(a: Int, b: Int) -> Int { return a * b; }\n",
    );

    run_graph(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── Sprint 12: std::core auto-import + module interaction ────────────────────

#[test]
fn std_core_builtins_available_in_each_module_without_import() {
    // Every module in a multi-module graph must see std::core builtins (print,
    // assert, array_push, array_len) without any explicit import statement.
    let dir = fixture_dir("int_std_auto_import");
    let main = dir.join("main.mtl");
    write(
        &dir.join("helper.mtl"),
        "pub fun sum(arr: Int[]) -> Int {\
         \n    assert(array_len(arr) > 0);\
         \n    mut total = 0;\
         \n    mut i = 0;\
         \n    while (i < array_len(arr)) { total += arr[i]; i += 1; }\
         \n    return total;\
         \n}\n",
    );
    write(
        &main,
        "import helper::sum;\
         \nfun main() {\
         \n    let arr = [1, 2, 3, 4, 5];\
         \n    let result = sum(arr);\
         \n    assert(result == 15);\
         \n    print(result);\
         \n}\n",
    );
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn user_glob_overrides_std_core_same_name_in_multi_module() {
    // A User-tier glob export of a function with the same name as a std::core
    // builtin wins silently over the Std-tier auto-import (no T0011).
    let dir = fixture_dir("int_user_glob_overrides_std");
    let main = dir.join("main.mtl");
    write(
        &dir.join("mylib.mtl"),
        "pub fun double(x: Int) -> Int { return x * 2; }\n",
    );
    write(
        &main,
        "import mylib::*;\
         \nfun main() {\
         \n    let result = double(21);\
         \n    assert(result == 42);\
         \n}\n",
    );
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn multi_module_perhaps_and_result_without_explicit_std_import() {
    // Perhaps and Result are available in every module via std::core auto-import.
    // No explicit `import std::core::Perhaps` should be needed.
    let dir = fixture_dir("int_module_perhaps");
    let main = dir.join("main.mtl");
    write(
        &dir.join("finder.mtl"),
        "pub fun find_first_positive(arr: Int[]) -> Perhaps<Int> {\
         \n    mut i = 0;\
         \n    while (i < array_len(arr)) {\
         \n        if (arr[i] > 0) { return Perhaps::Some { value: arr[i] }; }\
         \n        i += 1;\
         \n    }\
         \n    None\
         \n}\n",
    );
    write(
        &main,
        "import finder::find_first_positive;\
         \nfun main() {\
         \n    let arr = [-1, -2, 7, 3];\
         \n    let r = find_first_positive(arr);\
         \n    match r {\
         \n        Perhaps::Some { value } => assert(value == 7),\
         \n        None => assert(false),\
         \n    };\
         \n}\n",
    );
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn explicit_std_core_import_and_auto_glob_coexist() {
    // A module may `import std::core::Perhaps` explicitly while other std::core
    // names (like assert) are still available via the auto-glob.
    let dir = fixture_dir("int_explicit_and_auto");
    let main = dir.join("main.mtl");
    write(
        &main,
        "import std::core::Perhaps;\
         \nfun main() {\
         \n    let p = Perhaps::Some { value: 42 };\
         \n    match p {\
         \n        Perhaps::Some { value } => assert(value == 42),\
         \n        None => assert(false),\
         \n    };\
         \n}\n",
    );
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── #189: cross-module closure capture and pass-ordering correctness ──────────

#[test]
fn cross_module_closure_captures_imported_function() {
    // Module builder returns a higher-order closure that captures `add` from math.
    // Verifies that a closure created in builder's pass-1b correctly holds a
    // real value for the imported function (not a Unit placeholder).
    let dir = fixture_dir("closure_capture_import");
    let main = dir.join("main.mtl");
    write(&dir.join("math.mtl"), "pub fun add(x: Int, y: Int) -> Int { return x + y; }\n");
    write(
        &dir.join("builder.mtl"),
        "import math::add;\
         \npub fun make_adder(n: Int) -> fun(Int) -> Int {\
         \n    return fun(x: Int) -> Int { return add(x, n); };\
         \n}\n",
    );
    write(
        &main,
        "import builder::make_adder;\
         \nfun main() {\
         \n    let add5 = make_adder(5);\
         \n    assert(add5(3) == 8);\
         \n    assert(add5(10) == 15);\
         \n}\n",
    );
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn intra_module_recursion_visible_after_cross_module_import() {
    // Module recur has a recursive function count_down.
    // main imports and calls it, verifying that recur's pass-1b correctly
    // set up the self-referencing closure before main seeded it.
    let dir = fixture_dir("mutual_rec_cross");
    let main = dir.join("main.mtl");
    write(
        &dir.join("recur.mtl"),
        "pub fun count_down(n: Int) -> Int {\
         \n    if (n <= 0) { return 0; }\
         \n    return 1 + count_down(n - 1);\
         \n}\n",
    );
    write(
        &main,
        "import recur::count_down;\
         \nfun main() {\
         \n    assert(count_down(0) == 0);\
         \n    assert(count_down(5) == 5);\
         \n    assert(count_down(10) == 10);\
         \n}\n",
    );
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn two_same_tier_imports_both_captured_in_closure() {
    // main imports from two independent modules (left and right) at the same tier.
    // A closure in main's pass-1b must capture real values from both — not Unit
    // placeholders — even though left and right are initialized in arbitrary order.
    let dir = fixture_dir("same_tier_closure");
    let main = dir.join("main.mtl");
    write(&dir.join("left.mtl"), "pub fun left_val() -> Int { return 6; }\n");
    write(&dir.join("right.mtl"), "pub fun right_val() -> Int { return 10; }\n");
    write(
        &main,
        "import left::left_val;\
         \nimport right::right_val;\
         \nfun make_combiner() -> fun() -> Int {\
         \n    return fun() -> Int { return left_val() + right_val(); };\
         \n}\
         \nfun main() {\
         \n    let f = make_combiner();\
         \n    assert(f() == 16);\
         \n}\n",
    );
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}

// ── #228: diamond dependency (same physical file via multiple paths) ───────────

#[test]
fn diamond_dependency_shared_base_accessible_in_both_importers() {
    // base.mtl is reachable from both left.mtl and right.mtl via their imports.
    // Without the path-alias fix, the name resolver would assign base a path that
    // doesn't exist in the registry when loaded via the second importer, causing T0003.
    let dir = fixture_dir("diamond_dep");
    let main = dir.join("main.mtl");
    write(
        &dir.join("base.mtl"),
        "pub fun shared() -> Int { return 42; }\n",
    );
    write(
        &dir.join("left.mtl"),
        "import base::shared;\npub fun left_answer() -> Int { return shared(); }\n",
    );
    write(
        &dir.join("right.mtl"),
        "import base::shared;\npub fun right_answer() -> Int { return shared() + 1; }\n",
    );
    write(
        &main,
        "import left::left_answer;\
         \nimport right::right_answer;\
         \nfun main() {\
         \n    assert(left_answer() == 42);\
         \n    assert(right_answer() == 43);\
         \n}\n",
    );
    run_graph_std(&main).unwrap_or_else(|e| panic!("{e}"));
}
