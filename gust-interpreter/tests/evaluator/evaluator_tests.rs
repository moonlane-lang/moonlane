/// Integration tests for the evaluator.
/// All Gust source files live in tests/evaluator/sources/.
///
/// Positive files are self-asserting:
///   `let _ok = match (actual == expected) { true => 0, };`
///   If the condition is false no arm matches → runtime panic → test fails.
///
/// Negative files carry one annotation on any line:
///   `// RUNTIME_ERROR[substring]`   — program typechecks but fails at runtime
///   `// TYPECHECK_ERROR[substring]` — program is rejected at typecheck time

#[cfg(test)]
mod tests {
    use std::path::Path;
    use gust::{evaluator, parser, typechecker};

    // ── Harness ───────────────────────────────────────────────────────────────

    fn test_dir() -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/evaluator/sources").to_string()
    }

    fn load_source(path: &str) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("could not read {path}: {e}"))
    }

    fn parse_annotation(source: &str) -> Option<(String, String)> {
        for line in source.lines() {
            if let Some(pos) = line.find("// RUNTIME_ERROR[") {
                let rest = &line[pos + 17..];
                if let Some(end) = rest.find(']') {
                    return Some(("runtime".into(), rest[..end].to_string()));
                }
            }
            if let Some(pos) = line.find("// TYPECHECK_ERROR[") {
                let rest = &line[pos + 19..];
                if let Some(end) = rest.find(']') {
                    return Some(("typecheck".into(), rest[..end].to_string()));
                }
            }
        }
        None
    }

    fn check_file(path: &str) {
        let source = load_source(path);
        let filename = Path::new(path).file_name().unwrap().to_str().unwrap();
        match parse_annotation(&source) {
            Some((kind, expected)) if kind == "runtime" => {
                let ast = parser::parse(&source, filename).expect("parse error");
                let prog = typechecker::check(ast).expect("typecheck error");
                let err = evaluator::evaluate(prog)
                    .expect_err("expected runtime error, but program succeeded")
                    .to_string();
                assert!(
                    err.contains(&expected),
                    "expected error containing '{expected}', got: {err}"
                );
            }
            Some((_, expected)) => {
                let ast = parser::parse(&source, filename).expect("parse error");
                let err = typechecker::check(ast)
                    .expect_err("expected typecheck error, but check() returned Ok")
                    .to_string();
                assert!(
                    err.contains(&expected),
                    "expected error containing '{expected}', got: {err}"
                );
            }
            None => {
                let ast = parser::parse(&source, filename).expect("parse error");
                let prog = typechecker::check(ast).expect("typecheck error");
                evaluator::evaluate(prog).expect("runtime error");
            }
        }
    }

    fn check(filename: &str) {
        check_file(&format!("{}/{filename}", test_dir()));
    }

    // ── Positive tests ────────────────────────────────────────────────────────

    #[test]
    fn literals() { check("01_literals.gust"); }

    #[test]
    fn arithmetic() { check("02_arithmetic.gust"); }

    #[test]
    fn float_arithmetic() { check("03_float_arithmetic.gust"); }

    #[test]
    fn comparison() { check("04_comparison.gust"); }

    #[test]
    fn logical() { check("05_logical.gust"); }

    #[test]
    fn unary() { check("06_unary.gust"); }

    #[test]
    fn range() { check("07_range.gust"); }

    #[test]
    fn cast() { check("08_cast.gust"); }

    #[test]
    fn tuple() { check("09_tuple.gust"); }

    #[test]
    fn array() { check("10_array.gust"); }

    #[test]
    fn enum_variant() { check("11_enum_variant.gust"); }

    #[test]
    fn if_expression() { check("12_if_expression.gust"); }

    #[test]
    fn loop_expr() { check("13_loop.gust"); }

    #[test]
    fn match_expr() { check("14_match.gust"); }

    #[test]
    fn while_loop() { check("15_while.gust"); }

    #[test]
    fn for_loop() { check("16_for_loop.gust"); }

    #[test]
    fn for_in() { check("17_for_in.gust"); }

    #[test]
    fn return_stmt() { check("18_return.gust"); }

    #[test]
    fn nested_signals() { check("19_nested_signals.gust"); }

    #[test]
    fn scoping() { check("20_scoping.gust"); }

    #[test]
    fn assign() { check("21_assign.gust"); }

    #[test]
    fn misc() { check("22_misc.gust"); }

    #[test]
    fn forward_reference() { check("23_forward_reference.gust"); }

    #[test]
    fn struct_literal() { check("24_struct_literal.gust"); }

    #[test]
    fn enum_with_fields() { check("25_enum_with_fields.gust"); }

    #[test]
    fn field_access() { check("26_field_access.gust"); }

    #[test]
    fn method_call_builtin() { check("27_method_call_builtin.gust"); }

    #[test]
    fn method_call_user() { check("28_method_call_user.gust"); }

    #[test]
    fn assign_index() { check("29_assign_index.gust"); }

    #[test]
    fn assign_field() { check("30_assign_field.gust"); }

    #[test]
    fn call() { check("31_call.gust"); }

    #[test]
    fn recursive() { check("32_recursive.gust"); }

    #[test]
    fn closure() { check("33_closure.gust"); }

    #[test]
    fn propagate_error() { check("34_propagate_error.gust"); }

    #[test]
    fn loop_if_break() { check("35_loop_if_break.gust"); }

    #[test]
    fn call_edge() { check("36_call_edge.gust"); }

    #[test]
    fn closure_edge() { check("37_closure_edge.gust"); }

    #[test]
    fn builtins() { check("38_builtins.gust"); }

    #[test]
    fn perhaps() { check("39_perhaps.gust"); }

    #[test]
    fn method_chain() { check("40_method_chain.gust"); }

    #[test]
    fn nested_struct() { check("41_nested_struct.gust"); }

    #[test]
    fn closures_advanced() { check("42_closures_advanced.gust"); }

    // ── Integration tests ─────────────────────────────────────────────────────

    #[test]
    fn int_statistics() { check("int_01_statistics.gust"); }

    #[test]
    fn int_battle() { check("int_02_battle.gust"); }

    // ── Negative tests ────────────────────────────────────────────────────────

    #[test]
    fn neg_div_by_zero() { check("neg_01_div_by_zero.gust"); }

    #[test]
    fn neg_rem_by_zero() { check("neg_02_rem_by_zero.gust"); }

    #[test]
    fn neg_array_oob() { check("neg_03_array_oob.gust"); }

    #[test]
    fn neg_array_negative_index() { check("neg_04_array_negative_index.gust"); }

    #[test]
    fn neg_array_index_at_len() { check("neg_05_array_index_at_len.gust"); }

    #[test]
    fn neg_no_arm() { check("neg_06_no_arm.gust"); }

    #[test]
    fn neg_no_main() { check("neg_07_no_main.gust"); }

    #[test]
    fn neg_cast_float_to_int() { check("neg_08_cast_float_to_int.gust"); }

    #[test]
    fn neg_tuple_oob() { check("neg_09_tuple_oob.gust"); }

    #[test]
    fn neg_and_rhs_evaluated() { check("neg_10_and_rhs_evaluated.gust"); }

    #[test]
    fn neg_or_rhs_evaluated() { check("neg_11_or_rhs_evaluated.gust"); }

    #[test]
    fn neg_missing_field() { check("neg_12_missing_field.gust"); }

    #[test]
    fn neg_nonexhaustive_match() { check("neg_13_nonexhaustive_match.gust"); }

    // ── Stack trace tests ─────────────────────────────────────────────────────

    #[test]
    fn neg_stack_single_frame() { check("neg_14_stack_single_frame.gust"); }

    #[test]
    fn neg_stack_outer_frame() { check("neg_15_stack_outer_frame.gust"); }

    #[test]
    fn neg_stack_deep_chain() { check("neg_16_stack_deep_chain.gust"); }

    #[test]
    fn neg_stack_recursive() { check("neg_17_stack_recursive.gust"); }

    #[test]
    fn neg_stack_closure_frame() { check("neg_18_stack_closure_frame.gust"); }
}
