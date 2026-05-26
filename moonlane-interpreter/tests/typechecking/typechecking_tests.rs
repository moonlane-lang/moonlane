/// Integration tests for the full typechecker pipeline.
/// Tests the complete flow from parsing through type checking.
/// Source files are organized by language feature under tests/typechecking/sources/<feature>/.

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

    fn check(path: &str) {
        check_file(&format!("{}/{path}", test_dir()));
    }

    // ── Literals ──────────────────────────────────────────────────────────────

    #[test]
    fn stage1_literals() { check("literals/01_literals.mln"); }

    // ── Arithmetic ────────────────────────────────────────────────────────────

    #[test]
    fn stage1_arithmetic() { check("arithmetic/03_arithmetic.mln"); }

    #[test]
    fn stage1_chained_arithmetic() { check("arithmetic/09_chained_arithmetic.mln"); }

    #[test]
    fn stage1_neg_arithmetic_on_bool() { check("arithmetic/neg_03_arithmetic_on_bool.mln"); }

    #[test]
    fn stage1_neg_neg_on_bool() { check("arithmetic/neg_04_neg_on_bool.mln"); }

    #[test]
    fn stage1_neg_ordering_on_bool() { check("arithmetic/neg_05_ordering_on_bool.mln"); }

    // ── Functions ─────────────────────────────────────────────────────────────

    #[test]
    fn stage1_annotations() { check("functions/02_annotations.mln"); }

    #[test]
    fn stage3_function_calls() { check("functions/04_functions.mln"); }

    #[test]
    fn stage3_nested_calls() { check("functions/05_nested_calls.mln"); }

    #[test]
    fn stage3_let_polymorphism() { check("functions/06_let_polymorphism.mln"); }

    #[test]
    fn stage3_forward_reference() { check("functions/07_forward_reference.mln"); }

    #[test]
    fn stage1_mut_bindings() { check("functions/08_mut_bindings.mln"); }

    #[test]
    fn stage1_scoping() { check("functions/10_scoping.mln"); }

    #[test]
    fn stage1_neg_type_mismatch() { check("functions/neg_01_type_mismatch.mln"); }

    #[test]
    fn stage1_neg_annotation_required() { check("functions/neg_02_annotation_required.mln"); }

    #[test]
    fn stage4_assign() { check("functions/stage4_01_assign.mln"); }

    #[test]
    fn stage4_return_diverges() { check("functions/stage4_02_return_diverges.mln"); }

    #[test]
    fn stage4_index_assign() { check("functions/stage4_03_index_assign.mln"); }

    #[test]
    fn stage4_neg_assign_to_let() { check("functions/stage4_neg_01_assign_to_let.mln"); }

    #[test]
    fn stage4_neg_assign_undeclared() { check("functions/stage4_neg_02_assign_undeclared.mln"); }

    #[test]
    fn stage4_neg_assign_type_mismatch() { check("functions/stage4_neg_03_assign_type_mismatch.mln"); }

    #[test]
    fn stage4_neg_index_assign_type_mismatch() { check("functions/stage4_neg_04_index_assign_type_mismatch.mln"); }

    #[test]
    fn stage7_return_type_propagation() { check("functions/stage7_01_return_type_propagation.mln"); }

    #[test]
    fn stage7_match_arm_blocks() { check("functions/stage7_02_match_arm_blocks.mln"); }

    // ── Control flow ──────────────────────────────────────────────────────────

    #[test]
    fn stage2_if_stmt() { check("control_flow/stage2_01_if_stmt.mln"); }

    #[test]
    fn stage2_while_stmt() { check("control_flow/stage2_02_while_stmt.mln"); }

    #[test]
    fn stage2_if_expr() { check("control_flow/stage2_03_if_expr.mln"); }

    #[test]
    fn stage2_else_if() { check("control_flow/stage2_04_else_if.mln"); }

    #[test]
    fn stage2_neg_non_bool_condition() { check("control_flow/stage2_neg_01_non_bool_condition.mln"); }

    #[test]
    fn stage6_for_loops() { check("control_flow/stage6_02_for_loops.mln"); }

    #[test]
    fn stage6_loop_expr() { check("control_flow/stage6_03_loop_expr.mln"); }

    #[test]
    fn stage6_nested_loop_break() { check("control_flow/stage6_09_nested_loop_break.mln"); }

    #[test]
    fn stage6_neg_for_in_non_iterable() { check("control_flow/stage6_neg_01_for_in_non_iterable.mln"); }

    #[test]
    fn stage6_neg_loop_break_mismatch() { check("control_flow/stage6_neg_02_loop_break_mismatch.mln"); }

    // ── Types (arrays, tuples, casts) ─────────────────────────────────────────

    #[test]
    fn stage3_tuples() { check("types/stage3_01_tuples.mln"); }

    #[test]
    fn stage3_arrays() { check("types/stage3_02_arrays.mln"); }

    #[test]
    fn stage4_if_as_block_tail() { check("types/stage3_03_if_as_block_tail.mln"); }

    #[test]
    fn stage3_neg_arity_mismatch() { check("types/stage3_neg_01_arity_mismatch.mln"); }

    #[test]
    fn stage3_neg_index_non_array() { check("types/stage3_neg_02_index_non_array.mln"); }

    #[test]
    fn stage3_neg_non_function_callee() { check("types/stage3_neg_03_non_function_callee.mln"); }

    #[test]
    fn stage3_neg_empty_array_no_annotation() { check("types/stage3_neg_04_empty_array_no_annotation.mln"); }

    #[test]
    fn stage3_neg_array_element_mismatch() { check("types/stage3_neg_05_array_element_mismatch.mln"); }

    #[test]
    fn stage3_neg_non_int_index() { check("types/stage3_neg_06_non_int_index.mln"); }

    #[test]
    fn stage4_neg_if_no_else_non_unit() { check("types/stage3_neg_07_if_no_else_non_unit.mln"); }

    #[test]
    fn stage6_tuple_access() { check("types/stage6_04_tuple_access.mln"); }

    #[test]
    fn stage6_neg_tuple_access_oob() { check("types/stage6_neg_03_tuple_access_oob.mln"); }

    #[test]
    fn stage6_cast() { check("types/stage6_06_cast.mln"); }

    #[test]
    fn stage6_neg_cast_string() { check("types/stage6_neg_04_cast_string.mln"); }

    #[test]
    fn stage6_neg_cast_bool() { check("types/stage6_neg_10_cast_bool.mln"); }

    #[test]
    fn stage6_neg_cast_float_to_int() { check("types/stage6_neg_11_cast_float_to_int.mln"); }

    // ── Structs ───────────────────────────────────────────────────────────────

    #[test]
    fn stage5_structs_and_methods() { check("structs/stage5_01_structs_and_methods.mln"); }

    #[test]
    fn stage5_builtin_type_methods() { check("structs/stage5_02_builtin_type_methods.mln"); }

    #[test]
    fn stage5_neg_struct_field_type_mismatch() { check("structs/stage5_neg_01_struct_field_type_mismatch.mln"); }

    #[test]
    fn stage5_neg_unknown_field() { check("structs/stage5_neg_02_unknown_field.mln"); }

    #[test]
    fn stage5_neg_method_arg_type_mismatch() { check("structs/stage5_neg_03_method_arg_type_mismatch.mln"); }

    #[test]
    fn stage5_neg_unknown_method() { check("structs/stage5_neg_04_unknown_method.mln"); }

    #[test]
    fn stage5_neg_field_access_non_struct() { check("structs/stage5_neg_05_field_access_non_struct.mln"); }

    #[test]
    fn stage5_neg_field_access_unknown_field() { check("structs/stage5_neg_06_field_access_unknown_field.mln"); }

    #[test]
    fn stage5_neg_struct_literal_missing_field() { check("structs/stage5_neg_07_struct_literal_missing_field.mln"); }

    #[test]
    fn stage9_local_struct_scope() { check("structs/stage9_01_local_struct_scope.mln"); }

    #[test]
    fn stage9_neg_local_struct_not_exported() { check("structs/stage9_neg_01_local_struct_not_exported.mln"); }

    // ── Enums ─────────────────────────────────────────────────────────────────

    #[test]
    fn stage6_enums() { check("enums/stage6_08_enums.mln"); }

    #[test]
    fn stage6_enum_literal_types() { check("enums/stage6_10_enum_literal_types.mln"); }

    #[test]
    fn stage6_neg_match_arm_mismatch() { check("enums/stage6_neg_06_match_arm_mismatch.mln"); }

    #[test]
    fn stage6_neg_enum_unknown_variant() { check("enums/stage6_neg_08_enum_unknown_variant.mln"); }

    #[test]
    fn stage6_neg_enum_field_type_mismatch() { check("enums/stage6_neg_09_enum_field_type_mismatch.mln"); }

    // ── Closures ──────────────────────────────────────────────────────────────

    #[test]
    fn stage6_closures() { check("closures/stage6_05_closures.mln"); }

    // ── Error handling ────────────────────────────────────────────────────────

    #[test]
    fn stage6_error_propagation() { check("error_handling/stage6_07_error_propagation.mln"); }

    #[test]
    fn stage6_neg_error_propagation_non_result() { check("error_handling/stage6_neg_05_error_propagation_non_result.mln"); }

    // ── Builtins and type ascription ──────────────────────────────────────────

    #[test]
    fn stage6_builtins() { check("builtins/stage6_01_builtins.mln"); }

    #[test]
    fn stage6_neg_builtin_wrong_arg_type() { check("builtins/stage6_neg_07_builtin_wrong_arg_type.mln"); }

    #[test]
    fn stage8_assert() { check("builtins/stage8_01_assert.mln"); }

    #[test]
    fn stage8_dbg() { check("builtins/stage8_02_dbg.mln"); }

    #[test]
    fn stage8_print_numeric() { check("builtins/stage8_03_print_numeric.mln"); }

    #[test]
    fn stage8_type_ascription() { check("builtins/stage8_04_type_ascription.mln"); }

    #[test]
    fn stage8_ascription_match_arm() { check("builtins/stage8_05_ascription_match_arm.mln"); }

    #[test]
    fn stage8_ascription_match_arm_bare() { check("builtins/stage8_05_ascription_match_arm_bare.mln"); }

    #[test]
    fn stage8_ascription_two_args() { check("builtins/stage8_06_ascription_two_args.mln"); }

    #[test]
    fn stage8_ascription_two_args_bare() { check("builtins/stage8_06_ascription_two_args_bare.mln"); }

    #[test]
    fn stage8_ascription_nope_arg() { check("builtins/stage8_07_ascription_nope_arg.mln"); }

    #[test]
    fn stage8_ascription_nope_arg_bare() { check("builtins/stage8_07_ascription_nope_arg_bare.mln"); }

    #[test]
    fn stage8_neg_assert_non_bool() { check("builtins/stage8_neg_01_assert_non_bool.mln"); }

    #[test]
    fn stage8_neg_ascribe_type_mismatch() { check("builtins/stage8_neg_02_ascribe_type_mismatch.mln"); }

    #[test]
    fn stage8_neg_ascribe_bool_as_int() { check("builtins/stage8_neg_03_ascribe_bool_as_int.mln"); }

    #[test]
    fn stage8_neg_ascribe_wrong_struct() { check("builtins/stage8_neg_04_ascribe_wrong_struct.mln"); }

    // ── Generics ──────────────────────────────────────────────────────────────

    #[test]
    fn stage10_generic_function() { check("generics/stage10_01_generic_function.mln"); }

    #[test]
    fn stage10_type_param_multiple_uses() { check("generics/stage10_02_type_param_multiple_uses.mln"); }

    #[test]
    fn stage10_generic_return_tuple() { check("generics/stage10_03_generic_return_tuple.mln"); }

    #[test]
    fn stage10_generic_higher_order() { check("generics/stage10_04_generic_higher_order.mln"); }

    #[test]
    fn stage10_generic_nested_types() { check("generics/stage10_05_generic_nested_types.mln"); }

    #[test]
    fn stage10_neg_type_param_conflict() { check("generics/stage10_neg_01_type_param_conflict.mln"); }

    #[test]
    fn stage10_neg_return_type_conflict() { check("generics/stage10_neg_02_return_type_conflict.mln"); }

    #[test]
    fn stage11_generic_struct_basic() { check("generics/stage11_01_generic_struct_basic.mln"); }

    #[test]
    fn stage11_generic_struct_two_params() { check("generics/stage11_02_generic_struct_two_params.mln"); }

    #[test]
    fn stage11_generic_enum_user() { check("generics/stage11_03_generic_enum_user.mln"); }

    #[test]
    fn stage11_generic_nested() { check("generics/stage11_04_generic_nested.mln"); }

    #[test]
    fn stage11_neg_generic_struct_field_conflict() { check("generics/stage11_neg_01_generic_struct_field_conflict.mln"); }

    // ── Known limitations ─────────────────────────────────────────────────────

    #[test]
    fn limit_rank1_fn_arg() { check("generics/limit_01_rank1_fn_arg.mln"); }

    #[test]
    fn stage10_let_polymorphism() { check("generics/limit_02_let_closure_mono.mln"); }

    #[test]
    fn limit_field_access_needs_annotation() { check("generics/limit_03_field_access_needs_annotation.mln"); }
}
