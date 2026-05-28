use std::process;

use clap::Parser;

mod ast;
mod error;
mod evaluator;
mod module_loader;
mod name_resolver;
mod parser;
mod path_normalizer;
mod typed_ast;
mod typechecker;
mod typeinference;
mod types;

use error::MoonlaneError;
use typechecker::StdPrelude;

#[derive(Parser)]
#[command(name = "moonlane")]
#[command(version = "0.1.0")]
#[command(about = "Moonlane interpreter")]
#[command(long_about = "A tree-walk interpreter for the Moonlane programming language")]
struct Args {
    /// Path to the \.mln file to execute
    #[arg(value_name = "FILE")]
    file: String,

    /// Print the AST and exit without executing
    #[arg(long)]
    debug_ast: bool,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args.file, args.debug_ast) {
        eprintln!("{}", e);
        process::exit(1);
    }
}

fn run(filename: &str, debug_ast: bool) -> Result<(), MoonlaneError> {
    // 1. Load modules
    let graph = module_loader::load_root(filename)?;

    if debug_ast {
        for m in graph.modules.iter() {
            println!("=== {:?} ===\n{:#?}", m.module_path, m.program);
        }
        return Ok(());
    }

    // 2. Resolve names and normalize paths
    let names = name_resolver::resolve(&graph)?;
    let normalized = path_normalizer::normalize(graph, &names)?;

    // 3. Typecheck
    let typed_graph = typechecker::check_graph(normalized, &names, StdPrelude::default())?;

    // 4. Evaluate
    evaluator::evaluate_graph(typed_graph)?;

    Ok(())
}
