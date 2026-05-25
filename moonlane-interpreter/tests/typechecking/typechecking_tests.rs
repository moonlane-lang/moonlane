/// Integration tests for the full typechecker pipeline.
/// Tests the complete flow from parsing through type checking.

#[cfg(test)]
mod tests {
    use std::path::Path;
    use moonlane::error::MoonlaneError;
    use moonlane::parser;
    use moonlane::typechecker;

    // ── Harness helpers ───────────────────────────────────────────────────────

    fn load_source(path: &str) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("could not read {path}: {e}"))
    }

    /// Parse `// ERROR[EXXXX]` annotations: returns (1-based line, code string) pairs.
    fn parse_error_annotations(source: &str) -> Vec<(usize, String)> {
        let mut out = vec![];
        for (idx, line) in source.lines().enumerate() {
            if let Some(pos) = line.find("// ERROR[") {
                let rest = &line[pos + 9..];
                if let Some(end) = rest.find(']') {
                    out.push((idx + 1, rest[..end].to_string()));
                }
            }
        }
        out
    }

    fn check_file(path: &str) {
        let source = load_source(path);
        let annotations = parse_error_annotations(&source);
        let filename = Path::new(path).file_name().unwrap().to_str().unwrap();

        let program = parser::parse(&source, filename)
            .unwrap_or_else(|e| panic!("parse error in {filename}: {e}"));
        let result = typechecker::check(program);

        if annotations.is_empty() {
            // Positive test: expect success.
            assert!(
                result.is_ok(),
                "expected Ok for {filename}, got error: {}",
                result.unwrap_err()
            );
        } else {
            // Negative test: expect a TypeError on the annotated line with the annotated code.
            let err = match result {
                Err(e) => e,
                Ok(_) => panic!("expected type error in {filename} but check() returned Ok"),
            };
            match &err {
                MoonlaneError::TypeError { code, line, .. } => {
                    let (expected_line, expected_code) = &annotations[0];
                    assert_eq!(
                        format!("{code}"), *expected_code,
                        "wrong error code in {filename}"
                    );
                    assert_eq!(
                        *line as usize, *expected_line,
                        "wrong error line in {filename}: expected {expected_line}, got {line}"
                    );
                }
                other => panic!("expected TypeError in {filename}, got: {other}"),
            }
        }
    }

    fn test_dir() -> String {
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/typechecking/sources").to_string()
    }

    // ── Stage 1 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage1_literals() {
        check_file(&format!("{}/01_literals.mln", test_dir()));
    }

    #[test]
    fn stage1_annotations() {
        check_file(&format!("{}/02_annotations.mln", test_dir()));
    }

    #[test]
    fn stage1_arithmetic() {
        check_file(&format!("{}/03_arithmetic.mln", test_dir()));
    }

    #[test]
    fn stage1_mut_bindings() {
        check_file(&format!("{}/08_mut_bindings.mln", test_dir()));
    }

    // ── Stage 1 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage1_neg_type_mismatch() {
        check_file(&format!("{}/neg_01_type_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage1_neg_annotation_required() {
        check_file(&format!("{}/neg_02_annotation_required.mln", test_dir()));
    }

    // ── Stage 2 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage2_if_stmt() {
        check_file(&format!("{}/stage2_01_if_stmt.mln", test_dir()));
    }

    #[test]
    fn stage2_while_stmt() {
        check_file(&format!("{}/stage2_02_while_stmt.mln", test_dir()));
    }

    #[test]
    fn stage2_if_expr() {
        check_file(&format!("{}/stage2_03_if_expr.mln", test_dir()));
    }

    #[test]
    fn stage2_else_if() {
        check_file(&format!("{}/stage2_04_else_if.mln", test_dir()));
    }

    // ── Stage 2 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage2_neg_non_bool_condition() {
        check_file(&format!("{}/stage2_neg_01_non_bool_condition.mln", test_dir()));
    }

    // ── Stage 3 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage3_function_calls() {
        check_file(&format!("{}/04_functions.mln", test_dir()));
    }

    #[test]
    fn stage3_nested_calls() {
        check_file(&format!("{}/05_nested_calls.mln", test_dir()));
    }

    #[test]
    fn stage3_let_polymorphism() {
        check_file(&format!("{}/06_let_polymorphism.mln", test_dir()));
    }

    #[test]
    fn stage3_forward_reference() {
        check_file(&format!("{}/07_forward_reference.mln", test_dir()));
    }

    #[test]
    fn stage3_tuples() {
        check_file(&format!("{}/stage3_01_tuples.mln", test_dir()));
    }

    #[test]
    fn stage3_arrays() {
        check_file(&format!("{}/stage3_02_arrays.mln", test_dir()));
    }

    // ── Stage 3 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage3_neg_arity_mismatch() {
        check_file(&format!("{}/stage3_neg_01_arity_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage3_neg_index_non_array() {
        check_file(&format!("{}/stage3_neg_02_index_non_array.mln", test_dir()));
    }

    #[test]
    fn stage3_neg_non_function_callee() {
        check_file(&format!("{}/stage3_neg_03_non_function_callee.mln", test_dir()));
    }

    #[test]
    fn stage3_neg_empty_array_no_annotation() {
        check_file(&format!("{}/stage3_neg_04_empty_array_no_annotation.mln", test_dir()));
    }

    #[test]
    fn stage3_neg_array_element_mismatch() {
        check_file(&format!("{}/stage3_neg_05_array_element_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage3_neg_non_int_index() {
        check_file(&format!("{}/stage3_neg_06_non_int_index.mln", test_dir()));
    }

    // ── Stage 4 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage4_if_as_block_tail() {
        check_file(&format!("{}/stage3_03_if_as_block_tail.mln", test_dir()));
    }

    #[test]
    fn stage4_assign() {
        check_file(&format!("{}/stage4_01_assign.mln", test_dir()));
    }

    #[test]
    fn stage4_return_diverges() {
        check_file(&format!("{}/stage4_02_return_diverges.mln", test_dir()));
    }

    #[test]
    fn stage4_index_assign() {
        check_file(&format!("{}/stage4_03_index_assign.mln", test_dir()));
    }

    // ── Stage 4 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage4_neg_if_no_else_non_unit() {
        check_file(&format!("{}/stage3_neg_07_if_no_else_non_unit.mln", test_dir()));
    }

    #[test]
    fn stage4_neg_assign_to_let() {
        check_file(&format!("{}/stage4_neg_01_assign_to_let.mln", test_dir()));
    }

    #[test]
    fn stage4_neg_assign_undeclared() {
        check_file(&format!("{}/stage4_neg_02_assign_undeclared.mln", test_dir()));
    }

    #[test]
    fn stage4_neg_assign_type_mismatch() {
        check_file(&format!("{}/stage4_neg_03_assign_type_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage4_neg_index_assign_type_mismatch() {
        check_file(&format!("{}/stage4_neg_04_index_assign_type_mismatch.mln", test_dir()));
    }

    // ── Stage 5 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage5_structs_and_methods() {
        check_file(&format!("{}/stage5_01_structs_and_methods.mln", test_dir()));
    }

    #[test]
    fn stage5_builtin_type_methods() {
        check_file(&format!("{}/stage5_02_builtin_type_methods.mln", test_dir()));
    }

    // ── Stage 5 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage5_neg_struct_field_type_mismatch() {
        check_file(&format!("{}/stage5_neg_01_struct_field_type_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage5_neg_unknown_field() {
        check_file(&format!("{}/stage5_neg_02_unknown_field.mln", test_dir()));
    }

    #[test]
    fn stage5_neg_method_arg_type_mismatch() {
        check_file(&format!("{}/stage5_neg_03_method_arg_type_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage5_neg_unknown_method() {
        check_file(&format!("{}/stage5_neg_04_unknown_method.mln", test_dir()));
    }

    #[test]
    fn stage5_neg_field_access_non_struct() {
        check_file(&format!("{}/stage5_neg_05_field_access_non_struct.mln", test_dir()));
    }

    #[test]
    fn stage5_neg_field_access_unknown_field() {
        check_file(&format!("{}/stage5_neg_06_field_access_unknown_field.mln", test_dir()));
    }

    #[test]
    fn stage5_neg_struct_literal_missing_field() {
        check_file(&format!("{}/stage5_neg_07_struct_literal_missing_field.mln", test_dir()));
    }

    // ── Stage 6 tests ─────────────────────────────────────────────────────────

    #[test]
    fn stage6_builtins() {
        check_file(&format!("{}/stage6_01_builtins.mln", test_dir()));
    }

    #[test]
    fn stage6_for_loops() {
        check_file(&format!("{}/stage6_02_for_loops.mln", test_dir()));
    }

    #[test]
    fn stage6_loop_expr() {
        check_file(&format!("{}/stage6_03_loop_expr.mln", test_dir()));
    }

    #[test]
    fn stage6_tuple_access() {
        check_file(&format!("{}/stage6_04_tuple_access.mln", test_dir()));
    }

    #[test]
    fn stage6_cast() {
        check_file(&format!("{}/stage6_06_cast.mln", test_dir()));
    }

    #[test]
    fn stage6_enums() {
        check_file(&format!("{}/stage6_08_enums.mln", test_dir()));
    }

    #[test]
    fn stage6_error_propagation() {
        check_file(&format!("{}/stage6_07_error_propagation.mln", test_dir()));
    }

    #[test]
    fn stage6_closures() {
        check_file(&format!("{}/stage6_05_closures.mln", test_dir()));
    }

    // ── Stage 6 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage6_nested_loop_break() {
        check_file(&format!("{}/stage6_09_nested_loop_break.mln", test_dir()));
    }

    #[test]
    fn stage6_enum_literal_types() {
        check_file(&format!("{}/stage6_10_enum_literal_types.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_for_in_non_iterable() {
        check_file(&format!("{}/stage6_neg_01_for_in_non_iterable.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_loop_break_mismatch() {
        check_file(&format!("{}/stage6_neg_02_loop_break_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_tuple_access_oob() {
        check_file(&format!("{}/stage6_neg_03_tuple_access_oob.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_cast_string() {
        check_file(&format!("{}/stage6_neg_04_cast_string.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_cast_bool() {
        check_file(&format!("{}/stage6_neg_10_cast_bool.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_cast_float_to_int() {
        check_file(&format!("{}/stage6_neg_11_cast_float_to_int.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_match_arm_mismatch() {
        check_file(&format!("{}/stage6_neg_06_match_arm_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_error_propagation_non_result() {
        check_file(&format!("{}/stage6_neg_05_error_propagation_non_result.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_builtin_wrong_arg_type() {
        check_file(&format!("{}/stage6_neg_07_builtin_wrong_arg_type.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_enum_unknown_variant() {
        check_file(&format!("{}/stage6_neg_08_enum_unknown_variant.mln", test_dir()));
    }

    #[test]
    fn stage6_neg_enum_field_type_mismatch() {
        check_file(&format!("{}/stage6_neg_09_enum_field_type_mismatch.mln", test_dir()));
    }

    // ── Stage 7 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage7_return_type_propagation() {
        check_file(&format!("{}/stage7_01_return_type_propagation.mln", test_dir()));
    }

    #[test]
    fn stage7_match_arm_blocks() {
        check_file(&format!("{}/stage7_02_match_arm_blocks.mln", test_dir()));
    }

    // ── Stage 8: assert / dbg / numeric print builtins ────────────────────────

    #[test]
    fn stage8_assert() {
        check_file(&format!("{}/stage8_01_assert.mln", test_dir()));
    }

    #[test]
    fn stage8_dbg() {
        check_file(&format!("{}/stage8_02_dbg.mln", test_dir()));
    }

    #[test]
    fn stage8_print_numeric() {
        check_file(&format!("{}/stage8_03_print_numeric.mln", test_dir()));
    }

    #[test]
    fn stage8_neg_assert_non_bool() {
        check_file(&format!("{}/stage8_neg_01_assert_non_bool.mln", test_dir()));
    }

    #[test]
    fn stage8_type_ascription() {
        check_file(&format!("{}/stage8_04_type_ascription.mln", test_dir()));
    }

    #[test]
    fn stage8_neg_ascribe_type_mismatch() {
        check_file(&format!("{}/stage8_neg_02_ascribe_type_mismatch.mln", test_dir()));
    }

    #[test]
    fn stage8_neg_ascribe_bool_as_int() {
        check_file(&format!("{}/stage8_neg_03_ascribe_bool_as_int.mln", test_dir()));
    }

    #[test]
    fn stage8_neg_ascribe_wrong_struct() {
        check_file(&format!("{}/stage8_neg_04_ascribe_wrong_struct.mln", test_dir()));
    }

    // ── Stage 8 showcase: ascription where let binding is the inferior alternative ──

    #[test]
    fn stage8_ascription_match_arm() {
        check_file(&format!("{}/stage8_05_ascription_match_arm.mln", test_dir()));
    }

    #[test]
    fn stage8_ascription_match_arm_bare() {
        check_file(&format!("{}/stage8_05_ascription_match_arm_bare.mln", test_dir()));
    }

    #[test]
    fn stage8_ascription_two_args() {
        check_file(&format!("{}/stage8_06_ascription_two_args.mln", test_dir()));
    }

    #[test]
    fn stage8_ascription_two_args_bare() {
        check_file(&format!("{}/stage8_06_ascription_two_args_bare.mln", test_dir()));
    }

    #[test]
    fn stage8_ascription_nope_arg() {
        check_file(&format!("{}/stage8_07_ascription_nope_arg.mln", test_dir()));
    }

    #[test]
    fn stage8_ascription_nope_arg_bare() {
        check_file(&format!("{}/stage8_07_ascription_nope_arg_bare.mln", test_dir()));
    }

    // ── Stage 9: local struct scope ───────────────────────────────────────────

    #[test]
    fn stage9_local_struct_scope() {
        check_file(&format!("{}/stage9_01_local_struct_scope.mln", test_dir()));
    }

    #[test]
    fn stage9_neg_local_struct_not_exported() {
        check_file(&format!("{}/stage9_neg_01_local_struct_not_exported.mln", test_dir()));
    }

    // ── Known-limitation tests ─────────────────────────────────────────────────

    #[test]
    fn limit_rank1_fn_arg() {
        check_file(&format!("{}/limit_01_rank1_fn_arg.mln", test_dir()));
    }

    #[test]
    fn limit_let_closure_mono() {
        check_file(&format!("{}/limit_02_let_closure_mono.mln", test_dir()));
    }

    #[test]
    fn limit_field_access_needs_annotation() {
        check_file(&format!("{}/limit_03_field_access_needs_annotation.mln", test_dir()));
    }
}
