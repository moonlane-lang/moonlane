/// Integration tests for the evaluator.
/// All Moonlane source files live in tests/evaluator/sources/.
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
    use moonlane::{evaluator, parser, typechecker};

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
            if let Some(pos) = line.find("// PARSE_ERROR[") {
                let rest = &line[pos + 15..];
                if let Some(end) = rest.find(']') {
                    return Some(("parse".into(), rest[..end].to_string()));
                }
            }
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
            Some((kind, expected)) if kind == "parse" => {
                let err = parser::parse(&source, filename)
                    .expect_err("expected parse error, but parsing succeeded")
                    .to_string();
                assert!(
                    err.contains(&expected),
                    "expected error containing '{expected}', got: {err}"
                );
            }
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
    fn literals() { check("01_literals.mln"); }

    #[test]
    fn arithmetic() { check("02_arithmetic.mln"); }

    #[test]
    fn float_arithmetic() { check("03_float_arithmetic.mln"); }

    #[test]
    fn comparison() { check("04_comparison.mln"); }

    #[test]
    fn logical() { check("05_logical.mln"); }

    #[test]
    fn unary() { check("06_unary.mln"); }

    #[test]
    fn range() { check("07_range.mln"); }

    #[test]
    fn cast() { check("08_cast.mln"); }

    #[test]
    fn tuple() { check("09_tuple.mln"); }

    #[test]
    fn array() { check("10_array.mln"); }

    #[test]
    fn enum_variant() { check("11_enum_variant.mln"); }

    #[test]
    fn if_expression() { check("12_if_expression.mln"); }

    #[test]
    fn loop_expr() { check("13_loop.mln"); }

    #[test]
    fn match_expr() { check("14_match.mln"); }

    #[test]
    fn while_loop() { check("15_while.mln"); }

    #[test]
    fn for_loop() { check("16_for_loop.mln"); }

    #[test]
    fn for_in() { check("17_for_in.mln"); }

    #[test]
    fn return_stmt() { check("18_return.mln"); }

    #[test]
    fn nested_signals() { check("19_nested_signals.mln"); }

    #[test]
    fn scoping() { check("20_scoping.mln"); }

    #[test]
    fn assign() { check("21_assign.mln"); }

    #[test]
    fn misc() { check("22_misc.mln"); }

    #[test]
    fn forward_reference() { check("23_forward_reference.mln"); }

    #[test]
    fn struct_literal() { check("24_struct_literal.mln"); }

    #[test]
    fn enum_with_fields() { check("25_enum_with_fields.mln"); }

    #[test]
    fn field_access() { check("26_field_access.mln"); }

    #[test]
    fn method_call_builtin() { check("27_method_call_builtin.mln"); }

    #[test]
    fn method_call_user() { check("28_method_call_user.mln"); }

    #[test]
    fn assign_index() { check("29_assign_index.mln"); }

    #[test]
    fn assign_field() { check("30_assign_field.mln"); }

    #[test]
    fn call() { check("31_call.mln"); }

    #[test]
    fn recursive() { check("32_recursive.mln"); }

    #[test]
    fn closure() { check("33_closure.mln"); }

    #[test]
    fn propagate_error() { check("34_propagate_error.mln"); }

    #[test]
    fn loop_if_break() { check("35_loop_if_break.mln"); }

    #[test]
    fn call_edge() { check("36_call_edge.mln"); }

    #[test]
    fn closure_edge() { check("37_closure_edge.mln"); }

    #[test]
    fn builtins() { check("38_builtins.mln"); }

    #[test]
    fn perhaps() { check("39_perhaps.mln"); }

    #[test]
    fn method_chain() { check("40_method_chain.mln"); }

    #[test]
    fn nested_struct() { check("41_nested_struct.mln"); }

    #[test]
    fn closures_advanced() { check("42_closures_advanced.mln"); }

    #[test]
    fn shorthand_field() { check("43_shorthand_field.mln"); }

    #[test]
    fn trailing_commas() { check("44_trailing_commas.mln"); }

    #[test]
    fn lvalue_paths() { check("45_lvalue_paths.mln"); }

    #[test]
    fn local_struct_scope() { check("46_local_struct_scope.mln"); }

    #[test]
    fn braceless_if() { check("47_braceless_if.mln"); }

    #[test]
    fn generics() { check("48_generics.mln"); }

    #[test]
    fn generic_higher_order() { check("49_generic_higher_order.mln"); }

    #[test]
    fn generic_consistency() { check("50_generic_consistency.mln"); }

    #[test]
    fn generic_nested_types() { check("51_generic_nested_types.mln"); }

    #[test]
    fn let_polymorphism() { check("52_let_polymorphism.mln"); }

    #[test]
    fn generic_struct() { check("53_generic_struct.mln"); }

    #[test]
    fn generic_enum_user() { check("54_generic_enum_user.mln"); }

    #[test]
    fn generic_nested() { check("55_generic_nested.mln"); }

    // ── Integration tests ─────────────────────────────────────────────────────

    #[test]
    fn int_statistics() { check("int_01_statistics.mln"); }

    #[test]
    fn int_battle() { check("int_02_battle.mln"); }

    // ── Negative tests ────────────────────────────────────────────────────────

    #[test]
    fn neg_div_by_zero() { check("neg_01_div_by_zero.mln"); }

    #[test]
    fn neg_rem_by_zero() { check("neg_02_rem_by_zero.mln"); }

    #[test]
    fn neg_array_oob() { check("neg_03_array_oob.mln"); }

    #[test]
    fn neg_array_negative_index() { check("neg_04_array_negative_index.mln"); }

    #[test]
    fn neg_array_index_at_len() { check("neg_05_array_index_at_len.mln"); }

    #[test]
    fn neg_no_arm() { check("neg_06_no_arm.mln"); }

    #[test]
    fn neg_no_main() { check("neg_07_no_main.mln"); }

    #[test]
    fn neg_cast_float_to_int() { check("neg_08_cast_float_to_int.mln"); }

    #[test]
    fn neg_tuple_oob() { check("neg_09_tuple_oob.mln"); }

    #[test]
    fn neg_and_rhs_evaluated() { check("neg_10_and_rhs_evaluated.mln"); }

    #[test]
    fn neg_or_rhs_evaluated() { check("neg_11_or_rhs_evaluated.mln"); }

    #[test]
    fn neg_missing_field() { check("neg_12_missing_field.mln"); }

    #[test]
    fn neg_nonexhaustive_match() { check("neg_13_nonexhaustive_match.mln"); }

    // ── Stack trace tests ─────────────────────────────────────────────────────

    #[test]
    fn neg_stack_single_frame() { check("neg_14_stack_single_frame.mln"); }

    #[test]
    fn neg_stack_outer_frame() { check("neg_15_stack_outer_frame.mln"); }

    #[test]
    fn neg_stack_deep_chain() { check("neg_16_stack_deep_chain.mln"); }

    #[test]
    fn neg_stack_recursive() { check("neg_17_stack_recursive.mln"); }

    #[test]
    fn neg_stack_closure_frame() { check("neg_18_stack_closure_frame.mln"); }

    #[test]
    fn neg_braceless_if_dangling_else() { check("neg_19_braceless_if_dangling_else.mln"); }

    #[test]
    fn neg_braceless_if_mixed_arms() { check("neg_20_braceless_if_mixed_arms.mln"); }

    #[test]
    fn neg_generic_type_conflict() { check("neg_21_generic_type_conflict.mln"); }
}
