/// Integration tests for the full typechecker pipeline.
/// Tests the complete flow from parsing through type checking.

#[cfg(test)]
mod tests {
    use std::path::Path;
    use yoloscript::error::YoloscriptError;
    use yoloscript::parser;
    use yoloscript::typechecker;

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

    fn byte_offset_to_line(source: &str, offset: usize) -> usize {
        let safe = offset.min(source.len());
        source[..safe].chars().filter(|&c| c == '\n').count() + 1
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
                YoloscriptError::TypeError { code, start, .. } => {
                    let (expected_line, expected_code) = &annotations[0];
                    let actual_line = byte_offset_to_line(&source, *start);
                    assert_eq!(
                        format!("{code}"), *expected_code,
                        "wrong error code in {filename}"
                    );
                    assert_eq!(
                        actual_line, *expected_line,
                        "wrong error line in {filename}: expected {expected_line}, got {actual_line}"
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
        check_file(&format!("{}/01_literals.yolo", test_dir()));
    }

    #[test]
    fn stage1_annotations() {
        check_file(&format!("{}/02_annotations.yolo", test_dir()));
    }

    #[test]
    fn stage1_arithmetic() {
        check_file(&format!("{}/03_arithmetic.yolo", test_dir()));
    }

    #[test]
    fn stage1_mut_bindings() {
        check_file(&format!("{}/08_mut_bindings.yolo", test_dir()));
    }

    // ── Stage 1 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage1_neg_type_mismatch() {
        check_file(&format!("{}/neg_01_type_mismatch.yolo", test_dir()));
    }

    #[test]
    fn stage1_neg_annotation_required() {
        check_file(&format!("{}/neg_02_annotation_required.yolo", test_dir()));
    }

    // ── Stage 2 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage2_if_stmt() {
        check_file(&format!("{}/stage2_01_if_stmt.yolo", test_dir()));
    }

    #[test]
    fn stage2_while_stmt() {
        check_file(&format!("{}/stage2_02_while_stmt.yolo", test_dir()));
    }

    #[test]
    fn stage2_if_expr() {
        check_file(&format!("{}/stage2_03_if_expr.yolo", test_dir()));
    }

    #[test]
    fn stage2_else_if() {
        check_file(&format!("{}/stage2_04_else_if.yolo", test_dir()));
    }

    // ── Stage 2 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage2_neg_non_bool_condition() {
        check_file(&format!("{}/stage2_neg_01_non_bool_condition.yolo", test_dir()));
    }

    // ── Stage 3 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage3_function_calls() {
        check_file(&format!("{}/04_functions.yolo", test_dir()));
    }

    #[test]
    fn stage3_nested_calls() {
        check_file(&format!("{}/05_nested_calls.yolo", test_dir()));
    }

    #[test]
    fn stage3_let_polymorphism() {
        check_file(&format!("{}/06_let_polymorphism.yolo", test_dir()));
    }

    #[test]
    fn stage3_forward_reference() {
        check_file(&format!("{}/07_forward_reference.yolo", test_dir()));
    }

    #[test]
    fn stage3_tuples() {
        check_file(&format!("{}/stage3_01_tuples.yolo", test_dir()));
    }

    #[test]
    fn stage3_arrays() {
        check_file(&format!("{}/stage3_02_arrays.yolo", test_dir()));
    }

    // ── Stage 3 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage3_neg_arity_mismatch() {
        check_file(&format!("{}/stage3_neg_01_arity_mismatch.yolo", test_dir()));
    }

    #[test]
    fn stage3_neg_index_non_array() {
        check_file(&format!("{}/stage3_neg_02_index_non_array.yolo", test_dir()));
    }

    #[test]
    fn stage3_neg_non_function_callee() {
        check_file(&format!("{}/stage3_neg_03_non_function_callee.yolo", test_dir()));
    }

    #[test]
    fn stage3_neg_empty_array_no_annotation() {
        check_file(&format!("{}/stage3_neg_04_empty_array_no_annotation.yolo", test_dir()));
    }

    #[test]
    fn stage3_neg_array_element_mismatch() {
        check_file(&format!("{}/stage3_neg_05_array_element_mismatch.yolo", test_dir()));
    }

    #[test]
    fn stage3_neg_non_int_index() {
        check_file(&format!("{}/stage3_neg_06_non_int_index.yolo", test_dir()));
    }

    // ── Stage 4 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage4_if_as_block_tail() {
        check_file(&format!("{}/stage3_03_if_as_block_tail.yolo", test_dir()));
    }

    #[test]
    fn stage4_assign() {
        check_file(&format!("{}/stage4_01_assign.yolo", test_dir()));
    }

    #[test]
    fn stage4_return_diverges() {
        check_file(&format!("{}/stage4_02_return_diverges.yolo", test_dir()));
    }

    #[test]
    fn stage4_index_assign() {
        check_file(&format!("{}/stage4_03_index_assign.yolo", test_dir()));
    }

    // ── Stage 4 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage4_neg_if_no_else_non_unit() {
        check_file(&format!("{}/stage3_neg_07_if_no_else_non_unit.yolo", test_dir()));
    }

    #[test]
    fn stage4_neg_assign_to_let() {
        check_file(&format!("{}/stage4_neg_01_assign_to_let.yolo", test_dir()));
    }

    #[test]
    fn stage4_neg_assign_undeclared() {
        check_file(&format!("{}/stage4_neg_02_assign_undeclared.yolo", test_dir()));
    }

    #[test]
    fn stage4_neg_assign_type_mismatch() {
        check_file(&format!("{}/stage4_neg_03_assign_type_mismatch.yolo", test_dir()));
    }

    #[test]
    fn stage4_neg_index_assign_type_mismatch() {
        check_file(&format!("{}/stage4_neg_04_index_assign_type_mismatch.yolo", test_dir()));
    }

    // ── Stage 5 positive tests ────────────────────────────────────────────────

    #[test]
    fn stage5_structs_and_methods() {
        check_file(&format!("{}/stage5_01_structs_and_methods.yolo", test_dir()));
    }

    // ── Stage 5 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage5_neg_struct_field_type_mismatch() {
        check_file(&format!("{}/stage5_neg_01_struct_field_type_mismatch.yolo", test_dir()));
    }

    #[test]
    fn stage5_neg_unknown_field() {
        check_file(&format!("{}/stage5_neg_02_unknown_field.yolo", test_dir()));
    }

    #[test]
    fn stage5_neg_method_arg_type_mismatch() {
        check_file(&format!("{}/stage5_neg_03_method_arg_type_mismatch.yolo", test_dir()));
    }

    // ── Stage 6 tests ─────────────────────────────────────────────────────────

    #[test]
    fn stage6_for_loops() {
        check_file(&format!("{}/stage6_02_for_loops.yolo", test_dir()));
    }

    // ── Stage 6 negative tests ────────────────────────────────────────────────

    #[test]
    fn stage6_neg_for_in_non_iterable() {
        check_file(&format!("{}/stage6_neg_01_for_in_non_iterable.yolo", test_dir()));
    }
}
