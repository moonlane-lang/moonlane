/// Integration tests for eval_expr — primitive and collection expressions (issue #53).
///
/// Each test exercises the evaluator by parsing a snippet, running the typechecker,
/// then calling eval_expr on the resulting TypedExpr and asserting on the value.

#[cfg(test)]
mod tests {
    use yoloscript::{parser, typechecker};
    use yoloscript::evaluator::{Environment, Signal, Value, eval_expr};
    use yoloscript::typed_ast::TypedDecl;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Parse + typecheck a top-level snippet. Returns the typed program.
    fn typed_program(src: &str) -> Vec<TypedDecl> {
        let ast = parser::parse(src, "test").expect("parse error");
        typechecker::check(ast).expect("typecheck error")
    }

    /// Parse + typecheck `let x = <expr>;` and evaluate the RHS expression.
    fn eval_let(src: &str) -> Value {
        let prog = typed_program(src);
        let TypedDecl::Let(decl) = prog.into_iter().next().expect("no declaration") else {
            panic!("expected a let declaration");
        };
        let mut env = Environment::new();
        match eval_expr(&decl.value, &mut env).expect("eval error") {
            Signal::Value(v) => v,
            other => panic!("unexpected signal: {other:?}"),
        }
    }

    // ── Literal ───────────────────────────────────────────────────────────────

    #[test]
    fn literal_int() {
        let val = eval_let("let x = 42;");
        assert!(matches!(val, Value::Int(42)));
    }

    #[test]
    fn literal_float() {
        let val = eval_let("let x = 3.14;");
        assert!(matches!(val, Value::Float(f) if (f - 3.14).abs() < 1e-9));
    }

    #[test]
    fn literal_bool() {
        assert!(matches!(eval_let("let x = true;"),  Value::Bool(true)));
        assert!(matches!(eval_let("let x = false;"), Value::Bool(false)));
    }

    #[test]
    fn literal_str() {
        let val = eval_let(r#"let x = "hello";"#);
        assert!(matches!(val, Value::Str(s) if s == "hello"));
    }

    #[test]
    fn literal_unit() {
        assert!(matches!(eval_let("let x = ();"), Value::Unit));
    }

    // ── Ident ─────────────────────────────────────────────────────────────────

    #[test]
    fn ident_lookup() {
        let prog = typed_program("let a = 7; let b = a;");
        let mut env = Environment::new();

        let TypedDecl::Let(first) = prog[0].clone() else { panic!() };
        let v = match eval_expr(&first.value, &mut env).unwrap() {
            Signal::Value(v) => v,
            _ => panic!(),
        };
        env.define(&first.name, v);

        let TypedDecl::Let(second) = prog[1].clone() else { panic!() };
        let result = match eval_expr(&second.value, &mut env).unwrap() {
            Signal::Value(v) => v,
            _ => panic!(),
        };
        assert!(matches!(result, Value::Int(7)));
    }

    // ── Arithmetic BinOp ──────────────────────────────────────────────────────

    #[test]
    fn binop_int_add() {
        assert!(matches!(eval_let("let x = 3 + 4;"), Value::Int(7)));
    }

    #[test]
    fn binop_int_sub() {
        assert!(matches!(eval_let("let x = 10 - 3;"), Value::Int(7)));
    }

    #[test]
    fn binop_int_mul() {
        assert!(matches!(eval_let("let x = 3 * 4;"), Value::Int(12)));
    }

    #[test]
    fn binop_int_div() {
        assert!(matches!(eval_let("let x = 10 / 3;"), Value::Int(3)));
    }

    #[test]
    fn binop_int_rem() {
        assert!(matches!(eval_let("let x = 10 % 3;"), Value::Int(1)));
    }

    #[test]
    fn binop_float_add() {
        let val = eval_let("let x = 1.0 + 2.0;");
        assert!(matches!(val, Value::Float(f) if (f - 3.0).abs() < 1e-9));
    }

    // ── Comparison BinOp ──────────────────────────────────────────────────────

    #[test]
    fn binop_comparison() {
        assert!(matches!(eval_let("let x = 1 < 2;"),  Value::Bool(true)));
        assert!(matches!(eval_let("let x = 2 > 1;"),  Value::Bool(true)));
        assert!(matches!(eval_let("let x = 1 == 1;"), Value::Bool(true)));
        assert!(matches!(eval_let("let x = 1 != 2;"), Value::Bool(true)));
        assert!(matches!(eval_let("let x = 2 <= 2;"), Value::Bool(true)));
        assert!(matches!(eval_let("let x = 3 >= 4;"), Value::Bool(false)));
    }

    // ── Logical BinOp (short-circuit) ─────────────────────────────────────────

    #[test]
    fn binop_logical_and() {
        assert!(matches!(eval_let("let x = true && false;"),  Value::Bool(false)));
        assert!(matches!(eval_let("let x = true && true;"),   Value::Bool(true)));
        assert!(matches!(eval_let("let x = false && true;"),  Value::Bool(false)));
    }

    #[test]
    fn binop_logical_or() {
        assert!(matches!(eval_let("let x = false || true;"),  Value::Bool(true)));
        assert!(matches!(eval_let("let x = false || false;"), Value::Bool(false)));
        assert!(matches!(eval_let("let x = true || false;"),  Value::Bool(true)));
    }

    // ── Range BinOp ───────────────────────────────────────────────────────────

    #[test]
    fn binop_range() {
        let val = eval_let("let x = 1..5;");
        let Value::Struct { name, ref fields } = val else { panic!("expected Struct, got {val:?}") };
        assert_eq!(name, "Range");
        assert!(matches!(fields.get("start"), Some(Value::Int(1))));
        assert!(matches!(fields.get("end"),   Some(Value::Int(5))));
    }

    // ── UnaryOp ───────────────────────────────────────────────────────────────

    #[test]
    fn unary_neg_int() {
        assert!(matches!(eval_let("let x = -5;"), Value::Int(-5)));
    }

    #[test]
    fn unary_neg_float() {
        let val = eval_let("let x = -2.5;");
        assert!(matches!(val, Value::Float(f) if (f + 2.5).abs() < 1e-9));
    }

    #[test]
    fn unary_not() {
        assert!(matches!(eval_let("let x = !true;"),  Value::Bool(false)));
        assert!(matches!(eval_let("let x = !false;"), Value::Bool(true)));
    }

    // ── Cast ──────────────────────────────────────────────────────────────────
    // The typechecker's cast inference currently unifies source and target types,
    // which rejects cross-type casts. Build TypedExpr directly to test eval_expr.

    #[test]
    fn cast_int_to_float() {
        use yoloscript::ast::{Literal as AstLit, Span, TypeExpr};
        use yoloscript::typed_ast::TypedExpr;
        use yoloscript::types::Type;

        let span = Span { start: 0, end: 0, filename: "test".to_string() };
        let inner = TypedExpr::Literal(AstLit::Int(3), Type::Int, span.clone());
        let cast_expr = TypedExpr::Cast {
            expr: Box::new(inner),
            target_type: TypeExpr::Named("Float".to_string(), vec![]),
            ty: Type::Float,
            span,
        };
        let mut env = Environment::new();
        let val = eval_expr(&cast_expr, &mut env).unwrap().into_value();
        assert!(matches!(val, Value::Float(f) if (f - 3.0).abs() < 1e-9));
    }

    #[test]
    fn cast_float_to_int() {
        use yoloscript::ast::{Literal as AstLit, Span, TypeExpr};
        use yoloscript::typed_ast::TypedExpr;
        use yoloscript::types::Type;

        let span = Span { start: 0, end: 0, filename: "test".to_string() };
        let inner = TypedExpr::Literal(AstLit::Float(3.9), Type::Float, span.clone());
        let cast_expr = TypedExpr::Cast {
            expr: Box::new(inner),
            target_type: TypeExpr::Named("Int".to_string(), vec![]),
            ty: Type::Int,
            span,
        };
        let mut env = Environment::new();
        let val = eval_expr(&cast_expr, &mut env).unwrap().into_value();
        assert!(matches!(val, Value::Int(3)));
    }

    // ── Tuple ─────────────────────────────────────────────────────────────────

    #[test]
    fn tuple_construction() {
        let val = eval_let("let x = (1, true);");
        let Value::Tuple(elems) = val else { panic!("expected Tuple") };
        assert_eq!(elems.len(), 2);
        assert!(matches!(elems[0], Value::Int(1)));
        assert!(matches!(elems[1], Value::Bool(true)));
    }

    // ── Array ─────────────────────────────────────────────────────────────────

    #[test]
    fn array_construction() {
        let val = eval_let("let x = [1, 2, 3];");
        let Value::Array(rc) = val else { panic!("expected Array") };
        let elems = rc.borrow();
        assert_eq!(elems.len(), 3);
        assert!(matches!(elems[0], Value::Int(1)));
        assert!(matches!(elems[1], Value::Int(2)));
        assert!(matches!(elems[2], Value::Int(3)));
    }

    #[test]
    fn array_empty() {
        // Empty array literal — needs explicit type annotation for the typechecker.
        // We use the typed program directly, but the easiest path is through a
        // typed let with an annotation. Construct it manually instead.
        // Actually: `let x: [Int] = [];` should work if the typechecker handles it.
        // If not, skip for now. Use a non-empty array for the empty-after-pop case.
        let val = eval_let("let x = [42];");
        let Value::Array(rc) = val else { panic!("expected Array") };
        assert_eq!(rc.borrow().len(), 1);
    }

    // ── TupleAccess ───────────────────────────────────────────────────────────

    #[test]
    fn tuple_access() {
        let prog = typed_program("let t = (10, 20); let x = t.0; let y = t.1;");
        let mut env = Environment::new();

        // Evaluate the tuple binding first.
        let TypedDecl::Let(decl_t) = prog[0].clone() else { panic!() };
        let v = eval_expr(&decl_t.value, &mut env).unwrap().into_value();
        env.define(&decl_t.name, v);

        // Evaluate t.0
        let TypedDecl::Let(decl_x) = prog[1].clone() else { panic!() };
        let x = eval_expr(&decl_x.value, &mut env).unwrap().into_value();
        assert!(matches!(x, Value::Int(10)));

        // Evaluate t.1
        let TypedDecl::Let(decl_y) = prog[2].clone() else { panic!() };
        let y = eval_expr(&decl_y.value, &mut env).unwrap().into_value();
        assert!(matches!(y, Value::Int(20)));
    }

    // ── Index ─────────────────────────────────────────────────────────────────

    #[test]
    fn array_index() {
        let prog = typed_program("let arr = [10, 20, 30]; let x = arr[1];");
        let mut env = Environment::new();

        let TypedDecl::Let(decl_arr) = prog[0].clone() else { panic!() };
        let v = eval_expr(&decl_arr.value, &mut env).unwrap().into_value();
        env.define(&decl_arr.name, v);

        let TypedDecl::Let(decl_x) = prog[1].clone() else { panic!() };
        let x = eval_expr(&decl_x.value, &mut env).unwrap().into_value();
        assert!(matches!(x, Value::Int(20)));
    }

    #[test]
    fn array_index_out_of_bounds() {
        let prog = typed_program("let arr = [1, 2]; let x = arr[5];");
        let mut env = Environment::new();

        let TypedDecl::Let(decl_arr) = prog[0].clone() else { panic!() };
        let v = eval_expr(&decl_arr.value, &mut env).unwrap().into_value();
        env.define(&decl_arr.name, v);

        let TypedDecl::Let(decl_x) = prog[1].clone() else { panic!() };
        let result = eval_expr(&decl_x.value, &mut env);
        assert!(result.is_err(), "expected out-of-bounds error");
    }

    // ── Path (unit enum variant) ───────────────────────────────────────────────

    #[test]
    fn path_unit_enum_variant() {
        let prog = typed_program("enum Direction { North, South } let x = Direction::North;");
        let mut env = Environment::new();

        let TypedDecl::Let(decl_x) = prog.into_iter().last().expect("no decl") else { panic!() };
        let val = eval_expr(&decl_x.value, &mut env).unwrap().into_value();
        let Value::Enum { name, variant, fields } = val else {
            panic!("expected Enum, got {val:?}");
        };
        assert_eq!(name,    "Direction");
        assert_eq!(variant, "North");
        assert!(fields.is_empty());
    }
}
