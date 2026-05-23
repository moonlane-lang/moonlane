//! Integration tests: each .gust test program must run through the parser without errors. These tests are meant to cover the full range of language features, and are not expected to be minimal. They are primarily intended to catch regressions in the parser as new features are added.

use gust::parser;

fn run(filename: &str) {
    let path = format!("tests/parsing/sources/{}", filename);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {}: {}", path, e));
    parser::parse(&source, &path)
        .unwrap_or_else(|e| panic!("{}", e));
}

#[test]
fn test_01_literals_and_variables() { run("01_literals_and_variables.gust"); }

#[test]
fn test_02_control_flow() { run("02_control_flow.gust"); }

#[test]
fn test_03_functions_and_closures() { run("03_functions_and_closures.gust"); }

#[test]
fn test_04_structs_and_impl() { run("04_structs_and_impl.gust"); }

#[test]
fn test_05_enums_and_match() { run("05_enums_and_match.gust"); }

#[test]
fn test_06_traits() { run("06_traits.gust"); }

#[test]
fn test_07_arrays_and_tuples() { run("07_arrays_and_tuples.gust"); }

#[test]
fn test_08_error_handling() { run("08_error_handling.gust"); }

#[test]
fn test_09_casting_and_generics() { run("09_casting_and_generics.gust"); }

#[test]
fn test_10_comprehensive() { run("10_comprehensive.gust"); }

#[test]
fn test_11_block_expr_stmts() { run("11_block_expr_stmts.gust"); }

// ── Error format tests ────────────────────────────────────────────────────────

/// Parse a source with a known error and return the error Display string.
fn parse_error_message(filename: &str) -> String {
    let path = format!("tests/parsing/sources/{}", filename);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {}: {}", path, e));
    match gust::parser::parse(&source, filename) {
        Err(e) => format!("{}", e),
        Ok(_) => panic!("expected a parse error from {filename} but parsing succeeded"),
    }
}

// neg_01_syntax_error.gust: `@@@` on line 3, col 1 → P0001

#[test]
fn error_format_p0001_contains_filename() {
    let msg = parse_error_message("neg_01_syntax_error.gust");
    assert!(msg.contains("neg_01_syntax_error.gust"), "message was: {msg}");
}

#[test]
fn error_format_p0001_contains_line_col() {
    let msg = parse_error_message("neg_01_syntax_error.gust");
    assert!(
        msg.contains("neg_01_syntax_error.gust:3:1"),
        "expected 'file:3:1' in message, got: {msg}"
    );
}

#[test]
fn error_format_p0001_contains_error_code() {
    let msg = parse_error_message("neg_01_syntax_error.gust");
    assert!(msg.contains("P0001"), "expected '[P0001]' in message, got: {msg}");
}

#[test]
fn error_format_p0001_does_not_contain_raw_byte_offset() {
    let msg = parse_error_message("neg_01_syntax_error.gust");
    // Old format was "at <start>..<end>"; verify raw byte range is gone.
    assert!(!msg.contains(".."), "message should not contain '..' (raw byte range), got: {msg}");
}

// neg_02_int_overflow.gust: oversized integer at line 1, col 14 → P0002

#[test]
fn error_format_p0002_file_line_col() {
    let msg = parse_error_message("neg_02_int_overflow.gust");
    assert!(msg.contains("P0002"), "expected '[P0002]' in message, got: {msg}");
    assert!(
        msg.contains("neg_02_int_overflow.gust:1:14"),
        "expected 'file:1:14' in message, got: {msg}"
    );
}

// neg_04_error_at_line_10.gust: `@@@` on line 10 — verifies line counting past line 9

#[test]
fn error_format_line_counting_past_nine() {
    let msg = parse_error_message("neg_04_error_at_line_10.gust");
    assert!(
        msg.contains("neg_04_error_at_line_10.gust:10:1"),
        "expected 'file:10:1' in message, got: {msg}"
    );
}
