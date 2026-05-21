/// Root module for all integration tests.
/// This allows tests organized in subdirectories to be discovered by Cargo.

#[path = "typeinference/typeinference_tests.rs"]
mod typeinference_tests;

#[path = "typechecking/typechecking_tests.rs"]
mod typechecking_tests;

#[path = "parsing/parsing_tests.rs"]
mod parsing_tests;

#[path = "evaluator/evaluator_tests.rs"]
mod evaluator_tests;
