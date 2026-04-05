use std::{env, fs, process};

mod ast;
mod error;
mod evaluator;
mod parser;
mod typed_ast;
mod typechecker;
mod types;

use error::YolangError;

fn main() {
    let args: Vec<String> = env::args().collect();

    let (debug_ast, path) = match args.as_slice() {
        [_, flag, path] if flag == "--debug-ast" => (true, path.clone()),
        [_, path] => (false, path.clone()),
        _ => {
            eprintln!("Usage: yolang [--debug-ast] <file.yolo>");
            process::exit(1);
        }
    };

    let source = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading '{}': {}", path, e);
            process::exit(1);
        }
    };

    if let Err(e) = run(&source, &path, debug_ast) {
        eprintln!("{}", e);
        process::exit(1);
    }
}

fn run(source: &str, filename: &str, debug_ast: bool) -> Result<(), YolangError> {
    // 1. Parse source → untyped AST
    let ast = parser::parse(source, filename)?;

    if debug_ast {
        println!("{:#?}", ast);
        return Ok(());
    }

    // 2. Type check → typed AST
    let typed_ast = typechecker::check(ast)?;

    // 3. Evaluate
    evaluator::evaluate(typed_ast)?;

    Ok(())
}
