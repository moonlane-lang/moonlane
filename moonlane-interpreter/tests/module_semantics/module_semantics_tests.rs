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
