use std::fs;
use std::path::{Path, PathBuf};

use moonlane::evaluator;
use moonlane::module_loader;
use moonlane::name_resolver;
use moonlane::path_normalizer;
use moonlane::typechecker;

fn fixture_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "moonlane_module_loading_{}_{}_{}",
        std::process::id(),
        nonce,
        name,
    ));
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("failed to create {}: {e}", dir.display()));
    dir
}

fn write(path: &Path, source: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
    }
    fs::write(path, source).unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
}

#[test]
fn single_file_program_loads_without_modules() {
    let dir = fixture_dir("single");
    let main = dir.join("main.mln");
    write(&main, "fun main() { }\n");

    let program = module_loader::load_program(&main).unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(program.imports.len(), 0);
    assert_eq!(program.decls.len(), 1);
}

#[test]
fn multi_file_program_loads_declared_modules() {
    let dir = fixture_dir("multi");
    let main = dir.join("main.mln");
    write(&main, "import parser::Token;\nfun main() { }\n");
    write(&dir.join("parser.mln"), "pub struct Token { value: Int }\n");

    let graph = module_loader::load_root(&main).unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(graph.modules.len(), 2);
    assert!(graph.modules.iter().any(|m| m.module_path == vec!["parser".to_string()]));
}

#[test]
fn multi_file_program_runs_after_module_loading() {
    let dir = fixture_dir("run_multi");
    let main = dir.join("main.mln");
    write(&main, "import helper::answer;\nfun main() -> Int { return answer(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    let program = module_loader::load_program(&main).unwrap_or_else(|e| panic!("{e}"));
    let typed = typechecker::check(program).unwrap_or_else(|e| panic!("{e}"));

    evaluator::evaluate(typed).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn facade_module_alongside_directory() {
    let dir = fixture_dir("facade");
    let main = dir.join("main.mln");
    write(&main, "import parser::Token;\nfun main() { }\n");
    // parser.mln is the facade; parser/ is the namespace — both can coexist
    write(&dir.join("parser.mln"), "struct Token { value: Int }\n");
    fs::create_dir_all(dir.join("parser")).unwrap();
    write(&dir.join("parser").join("ast.mln"), "pub struct Ast { value: Int }\n");

    let graph = module_loader::load_root(&main).unwrap_or_else(|e| panic!("{e}"));

    // main + parser.mln loaded; parser/ast.mln not imported so not loaded
    assert_eq!(graph.modules.len(), 2);
    assert!(graph.modules.iter().any(|m| m.module_path == vec!["parser".to_string()]));
}

#[test]
fn rejects_super_from_root_import() {
    let dir = fixture_dir("root_super");
    let main = dir.join("main.mln");
    write(&main, "import super::parser;\nfun main() { }\n");

    let err = module_loader::load_root(&main).expect_err("super from root should fail");
    let msg = err.to_string();

    assert!(msg.contains("super::"), "message was: {msg}");
    assert!(msg.contains("root module"), "message was: {msg}");
}

#[test]
fn accepts_root_self_super_std_and_child_roots_in_non_root_modules() {
    let dir = fixture_dir("roots");
    let main = dir.join("main.mln");
    write(&main, "import parser::Token;\nfun main() { }\n");
    write(
        &dir.join("parser.mln"),
        r#"
import self::child::Thing;
import root::child::Thing;
import super::child::Thing;
import std::core::Int;
import child::Thing;

struct Token { value: Int }
"#,
    );
    write(&dir.join("child.mln"), "struct Thing { value: Int }\n");

    let graph = module_loader::load_root(&main).unwrap_or_else(|e| panic!("{e}"));

    assert_eq!(graph.modules.len(), 3);
}

#[cfg(unix)]
#[test]
fn rejects_circular_module_graph() {
    use std::os::unix::fs::symlink;

    let dir = fixture_dir("cycle");
    let main = dir.join("main.mln");
    write(&main, "import a::Thing;\nfun main() { }\n");
    write(&dir.join("a.mln"), "import b::Other;\n");
    // create b/ as a symlink back to a/ to simulate a cycle
    symlink(dir.join("a.mln"), dir.join("b.mln"))
        .unwrap_or_else(|e| panic!("failed to create symlink cycle: {e}"));

    let err = module_loader::load_root(&main).expect_err("cycle should fail");
    let msg = err.to_string();

    assert!(msg.contains("circular module dependency"), "message was: {msg}");
}

#[test]
fn qualified_function_call_via_module_handle() {
    let dir = fixture_dir("qual_fn");
    let main = dir.join("main.mln");
    // import helper::* loads helper.mln into the graph.
    // helper::answer() uses a qualified path; the path normalizer rewrites it to "answer".
    write(&main, "import helper::*;\nfun main() -> Int { return helper::answer(); }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    let graph = module_loader::load_root(&main).unwrap_or_else(|e| panic!("{e}"));
    let names = name_resolver::resolve(&graph).unwrap_or_else(|e| panic!("{e}"));
    let normalized = path_normalizer::normalize(graph, &names).unwrap_or_else(|e| panic!("{e}"));
    let typed = typechecker::check_graph(normalized, &names, typechecker::StdPrelude::empty())
        .unwrap_or_else(|e| panic!("{e}"));
    evaluator::evaluate_graph(typed).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn qualified_type_in_return_signature_typechecks() {
    let dir = fixture_dir("qual_type");
    let main = dir.join("main.mln");
    // helper::Token as return type — TypeExpr::Named("helper::Token") strips to "Token".
    // The struct literal uses the bare name Token (already visible in merged namespace).
    write(&main, "import helper::*;\nfun wrap(v: Int) -> helper::Token { return Token { value: v }; }\nfun main() -> Int { let t = wrap(7); return t.value; }\n");
    write(&dir.join("helper.mln"), "pub struct Token { value: Int }\n");

    let program = module_loader::load_program(&main).unwrap_or_else(|e| panic!("{e}"));
    let typed = typechecker::check(program).unwrap_or_else(|e| panic!("{e}"));
    evaluator::evaluate(typed).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn self_qualified_path_in_expression_resolves() {
    let dir = fixture_dir("self_path");
    let main = dir.join("main.mln");
    // self::answer() — Path(["self","answer"]); the path normalizer rewrites it to "answer".
    write(&main, "fun answer() -> Int { return 99; }\nfun main() -> Int { return self::answer(); }\n");

    let graph = module_loader::load_root(&main).unwrap_or_else(|e| panic!("{e}"));
    let names = name_resolver::resolve(&graph).unwrap_or_else(|e| panic!("{e}"));
    let normalized = path_normalizer::normalize(graph, &names).unwrap_or_else(|e| panic!("{e}"));
    let typed = typechecker::check_graph(normalized, &names, typechecker::StdPrelude::empty())
        .unwrap_or_else(|e| panic!("{e}"));
    evaluator::evaluate_graph(typed).unwrap_or_else(|e| panic!("{e}"));
}

// ── #169: module system integration tests ────────────────────────────────────

#[test]
fn pub_enum_imported_and_matched() {
    let dir = fixture_dir("enum_match");
    let main = dir.join("main.mln");
    write(
        &main,
        r#"
import color::*;
fun main() -> Int {
    let c = Color::Red;
    match c {
        Color::Red   => { return 1; },
        Color::Green => { return 2; },
        Color::Blue  => { return 3; }
    }
}
"#,
    );
    write(&dir.join("color.mln"), "pub enum Color { Red, Green, Blue }\n");

    let program = module_loader::load_program(&main).unwrap_or_else(|e| panic!("{e}"));
    let typed = typechecker::check(program).unwrap_or_else(|e| panic!("{e}"));
    evaluator::evaluate(typed).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn group_import_makes_both_names_accessible() {
    let dir = fixture_dir("group_import");
    let main = dir.join("main.mln");
    write(
        &main,
        r#"
import math::{add, mul};
fun main() -> Int { return add(mul(2, 3), 1); }
"#,
    );
    write(
        &dir.join("math.mln"),
        "pub fun add(a: Int, b: Int) -> Int { return a + b; }\npub fun mul(a: Int, b: Int) -> Int { return a * b; }\n",
    );

    let program = module_loader::load_program(&main).unwrap_or_else(|e| panic!("{e}"));
    let typed = typechecker::check(program).unwrap_or_else(|e| panic!("{e}"));
    evaluator::evaluate(typed).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn import_with_alias_loads_module_into_graph() {
    let dir = fixture_dir("alias_load");
    let main = dir.join("main.mln");
    write(&main, "import helper::answer as compute;\nfun main() { }\n");
    write(&dir.join("helper.mln"), "pub fun answer() -> Int { return 42; }\n");

    let graph = module_loader::load_root(&main).unwrap_or_else(|e| panic!("{e}"));

    // The file is loaded even though the local binding uses an alias.
    assert_eq!(graph.modules.len(), 2);
    assert!(graph.modules.iter().any(|m| m.module_path == vec!["helper".to_string()]));
}

#[test]
fn transitive_dependency_loaded_via_facade() {
    let dir = fixture_dir("transitive");
    let main = dir.join("main.mln");
    write(&main, "import parser::*;\nfun main() -> Int { return parse(); }\n");
    // parser imports (and thereby loads) lexer; exposes parse() which delegates to tokenize()
    write(
        &dir.join("parser.mln"),
        "import lexer::*;\npub fun parse() -> Int { return tokenize(); }\n",
    );
    write(&dir.join("lexer.mln"), "pub fun tokenize() -> Int { return 1; }\n");

    let graph = module_loader::load_root(&main).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(graph.modules.len(), 3);

    let program = module_loader::load_program(&main).unwrap_or_else(|e| panic!("{e}"));
    let typed = typechecker::check(program).unwrap_or_else(|e| panic!("{e}"));
    evaluator::evaluate(typed).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn import_nonexistent_module_is_a_load_error() {
    // After #186: a missing .mln file is a hard load error, not a silent skip.
    let dir = fixture_dir("missing_mod");
    let main = dir.join("main.mln");
    write(
        &main,
        "import nonexistent::Thing;\nfun main() -> Int { return Thing(); }\n",
    );

    let err = module_loader::load_root(&main).expect_err("missing module should fail at load time");
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent") || msg.contains("cannot find module"),
        "message was: {msg}",
    );
}

#[test]
fn struct_field_access_across_modules() {
    let dir = fixture_dir("struct_field");
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

    let program = module_loader::load_program(&main).unwrap_or_else(|e| panic!("{e}"));
    let typed = typechecker::check(program).unwrap_or_else(|e| panic!("{e}"));
    evaluator::evaluate(typed).unwrap_or_else(|e| panic!("{e}"));
}
