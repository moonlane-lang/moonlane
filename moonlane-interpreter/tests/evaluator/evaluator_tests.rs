/// Integration tests for the evaluator.
/// All Moonlane source files live in tests/evaluator/sources/<feature>/.
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

    fn check(path: &str) {
        check_file(&format!("{}/{path}", test_dir()));
    }

    // ── Literals ──────────────────────────────────────────────────────────────

    #[test]
    fn literals() { check("literals/01_literals.mln"); }

    // ── Arithmetic ────────────────────────────────────────────────────────────

    #[test]
    fn arithmetic() { check("arithmetic/02_arithmetic.mln"); }

    #[test]
    fn float_arithmetic() { check("arithmetic/03_float_arithmetic.mln"); }

    #[test]
    fn comparison() { check("arithmetic/04_comparison.mln"); }

    #[test]
    fn logical() { check("arithmetic/05_logical.mln"); }

    #[test]
    fn unary() { check("arithmetic/06_unary.mln"); }

    #[test]
    fn range() { check("arithmetic/07_range.mln"); }

    #[test]
    fn neg_div_by_zero() { check("arithmetic/neg_01_div_by_zero.mln"); }

    #[test]
    fn neg_rem_by_zero() { check("arithmetic/neg_02_rem_by_zero.mln"); }

    // ── Types (arrays, tuples, casts) ─────────────────────────────────────────

    #[test]
    fn cast() { check("types/08_cast.mln"); }

    #[test]
    fn tuple() { check("types/09_tuple.mln"); }

    #[test]
    fn array() { check("types/10_array.mln"); }

    #[test]
    fn from_cast() { check("types/60_from_cast.mln"); }

    #[test]
    fn from_edge_cases() { check("types/63_from_edge_cases.mln"); }

    #[test]
    fn neg_array_oob() { check("types/neg_03_array_oob.mln"); }

    #[test]
    fn neg_array_negative_index() { check("types/neg_04_array_negative_index.mln"); }

    #[test]
    fn neg_array_index_at_len() { check("types/neg_05_array_index_at_len.mln"); }

    #[test]
    fn neg_cast_float_to_int() { check("types/neg_08_cast_float_to_int.mln"); }

    #[test]
    fn neg_tuple_oob() { check("types/neg_09_tuple_oob.mln"); }

    #[test]
    fn neg_cast_no_from() { check("types/neg_23_cast_no_from.mln"); }

    // ── Control flow ──────────────────────────────────────────────────────────

    #[test]
    fn if_expression() { check("control_flow/12_if_expression.mln"); }

    #[test]
    fn loop_expr() { check("control_flow/13_loop.mln"); }

    #[test]
    fn match_expr() { check("control_flow/14_match.mln"); }

    #[test]
    fn while_loop() { check("control_flow/15_while.mln"); }

    #[test]
    fn for_loop() { check("control_flow/16_for_loop.mln"); }

    #[test]
    fn for_in() { check("control_flow/17_for_in.mln"); }

    #[test]
    fn loop_if_break() { check("control_flow/35_loop_if_break.mln"); }

    #[test]
    fn braceless_if() { check("control_flow/47_braceless_if.mln"); }

    #[test]
    fn neg_no_arm() { check("control_flow/neg_06_no_arm.mln"); }

    #[test]
    fn neg_and_rhs_evaluated() { check("control_flow/neg_10_and_rhs_evaluated.mln"); }

    #[test]
    fn neg_or_rhs_evaluated() { check("control_flow/neg_11_or_rhs_evaluated.mln"); }

    #[test]
    fn neg_nonexhaustive_match() { check("control_flow/neg_13_nonexhaustive_match.mln"); }

    #[test]
    fn neg_braceless_if_dangling_else() { check("control_flow/neg_19_braceless_if_dangling_else.mln"); }

    #[test]
    fn neg_braceless_if_mixed_arms() { check("control_flow/neg_20_braceless_if_mixed_arms.mln"); }

    // ── Functions ─────────────────────────────────────────────────────────────

    #[test]
    fn return_stmt() { check("functions/18_return.mln"); }

    #[test]
    fn nested_signals() { check("functions/19_nested_signals.mln"); }

    #[test]
    fn scoping() { check("functions/20_scoping.mln"); }

    #[test]
    fn assign() { check("functions/21_assign.mln"); }

    #[test]
    fn misc() { check("functions/22_misc.mln"); }

    #[test]
    fn forward_reference() { check("functions/23_forward_reference.mln"); }

    #[test]
    fn call() { check("functions/31_call.mln"); }

    #[test]
    fn recursive() { check("functions/32_recursive.mln"); }

    #[test]
    fn call_edge() { check("functions/36_call_edge.mln"); }

    #[test]
    fn neg_no_main() { check("functions/neg_07_no_main.mln"); }

    #[test]
    fn neg_stack_single_frame() { check("functions/neg_14_stack_single_frame.mln"); }

    #[test]
    fn neg_stack_outer_frame() { check("functions/neg_15_stack_outer_frame.mln"); }

    #[test]
    fn neg_stack_deep_chain() { check("functions/neg_16_stack_deep_chain.mln"); }

    #[test]
    fn neg_stack_recursive() { check("functions/neg_17_stack_recursive.mln"); }

    #[test]
    fn neg_stack_closure_frame() { check("functions/neg_18_stack_closure_frame.mln"); }

    // ── Closures ──────────────────────────────────────────────────────────────

    #[test]
    fn closure() { check("closures/33_closure.mln"); }

    #[test]
    fn closure_edge() { check("closures/37_closure_edge.mln"); }

    #[test]
    fn closures_advanced() { check("closures/42_closures_advanced.mln"); }

    // ── Structs ───────────────────────────────────────────────────────────────

    #[test]
    fn struct_literal() { check("structs/24_struct_literal.mln"); }

    #[test]
    fn field_access() { check("structs/26_field_access.mln"); }

    #[test]
    fn method_call_builtin() { check("structs/27_method_call_builtin.mln"); }

    #[test]
    fn method_call_user() { check("structs/28_method_call_user.mln"); }

    #[test]
    fn assign_index() { check("structs/29_assign_index.mln"); }

    #[test]
    fn assign_field() { check("structs/30_assign_field.mln"); }

    #[test]
    fn method_chain() { check("structs/40_method_chain.mln"); }

    #[test]
    fn nested_struct() { check("structs/41_nested_struct.mln"); }

    #[test]
    fn shorthand_field() { check("structs/43_shorthand_field.mln"); }

    #[test]
    fn trailing_commas() { check("structs/44_trailing_commas.mln"); }

    #[test]
    fn lvalue_paths() { check("structs/45_lvalue_paths.mln"); }

    #[test]
    fn local_struct_scope() { check("structs/46_local_struct_scope.mln"); }

    #[test]
    fn neg_missing_field() { check("structs/neg_12_missing_field.mln"); }

    // ── Enums ─────────────────────────────────────────────────────────────────

    #[test]
    fn enum_variant() { check("enums/11_enum_variant.mln"); }

    #[test]
    fn enum_with_fields() { check("enums/25_enum_with_fields.mln"); }

    #[test]
    fn perhaps() { check("enums/39_perhaps.mln"); }

    // ── Generics ──────────────────────────────────────────────────────────────

    #[test]
    fn generics() { check("generics/48_generics.mln"); }

    #[test]
    fn generic_consistency() { check("generics/50_generic_consistency.mln"); }

    #[test]
    fn generic_nested_types() { check("generics/51_generic_nested_types.mln"); }

    #[test]
    fn let_polymorphism() { check("generics/52_let_polymorphism.mln"); }

    #[test]
    fn generic_struct() { check("generics/53_generic_struct.mln"); }

    #[test]
    fn generic_enum_user() { check("generics/54_generic_enum_user.mln"); }

    #[test]
    fn generic_nested() { check("generics/55_generic_nested.mln"); }

    #[test]
    fn generic_body_annotation() { check("generics/56_generic_body_annotation.mln"); }

    #[test]
    fn generic_enum_infer_context() { check("generics/57_generic_enum_infer_context.mln"); }

    #[test]
    fn neg_generic_type_conflict() { check("generics/neg_21_generic_type_conflict.mln"); }

    // ── Aspects ───────────────────────────────────────────────────────────────

    #[test]
    fn aspect_dispatch() { check("aspects/58_aspect_dispatch.mln"); }

    #[test]
    fn iterable_aspect() { check("aspects/59_iterable_aspect.mln"); }

    #[test]
    fn iterable_edge_cases() { check("aspects/62_iterable_edge_cases.mln"); }

    #[test]
    fn neg_missing_aspect_method() { check("aspects/neg_22_missing_aspect_method.mln"); }

    // ── Error handling ────────────────────────────────────────────────────────

    #[test]
    fn propagate_error() { check("error_handling/34_propagate_error.mln"); }

    #[test]
    fn propagate_error_coercion() { check("error_handling/61_propagate_error_coercion.mln"); }

    #[test]
    fn propagate_error_edge_cases() { check("error_handling/64_propagate_error_edge_cases.mln"); }

    // ── Builtins ──────────────────────────────────────────────────────────────

    #[test]
    fn builtins() { check("builtins/38_builtins.mln"); }

    // ── Integration ───────────────────────────────────────────────────────────

    #[test]
    fn int_statistics() { check("integration/int_01_statistics.mln"); }

    #[test]
    fn int_battle() { check("integration/int_02_battle.mln"); }

    #[test]
    fn int_aspects() { check("integration/int_03_aspects.mln"); }

    #[test]
    fn int_generic_option_chain() { check("integration/int_03_generic_option_chain.mln"); }

    #[test]
    fn int_pipeline() { check("integration/int_04_pipeline.mln"); }

    #[test]
    fn int_generic_algorithms() { check("integration/int_04_generic_algorithms.mln"); }

    #[test]
    fn int_aspects_combined() { check("integration/int_05_aspects_combined.mln"); }

    #[test]
    fn int_generic_data_pipeline() { check("integration/int_05_generic_data_pipeline.mln"); }

    #[test]
    fn int_display() { check("integration/int_06_display.mln"); }
}
