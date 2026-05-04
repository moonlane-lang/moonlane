/// Test suite for the type inference system.
/// Tests are organized by phase/component matching the task breakdown.

#[cfg(test)]
mod phase_1_type_variables {
    use yolang::typeinference::{TypeVar, TypeVarGenerator};

    #[test]
    fn test_type_var_creation() {
        let var1 = TypeVar(0);
        let var2 = TypeVar(1);

        assert_eq!(var1.0, 0);
        assert_eq!(var2.0, 1);
        assert_ne!(var1, var2);
    }

    #[test]
    fn test_type_var_display() {
        let var = TypeVar(42);
        assert_eq!(format!("{}", var), "?t42");
    }

    #[test]
    fn test_type_var_generator_fresh() {
        let mut gen = TypeVarGenerator::new();

        let v1 = gen.fresh();
        let v2 = gen.fresh();
        let v3 = gen.fresh();

        assert_eq!(v1.0, 0);
        assert_eq!(v2.0, 1);
        assert_eq!(v3.0, 2);
        assert_ne!(v1, v2);
        assert_ne!(v2, v3);
    }

    #[test]
    fn test_type_var_generator_counter() {
        let mut gen = TypeVarGenerator::new();
        assert_eq!(gen.counter(), 0);

        gen.fresh();
        assert_eq!(gen.counter(), 1);

        gen.fresh();
        gen.fresh();
        assert_eq!(gen.counter(), 3);
    }

    #[test]
    fn test_type_var_ordering() {
        let v0 = TypeVar(0);
        let v1 = TypeVar(1);
        let v5 = TypeVar(5);

        assert!(v0 < v1);
        assert!(v1 < v5);
        assert!(v0 < v5);
    }

    #[test]
    fn test_type_var_hashable() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(TypeVar(0));
        set.insert(TypeVar(1));
        set.insert(TypeVar(0));  // Duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&TypeVar(0)));
        assert!(set.contains(&TypeVar(1)));
        assert!(!set.contains(&TypeVar(2)));
    }
}

#[cfg(test)]
mod phase_2_infer_types {
    use yolang::typeinference::{InferType, TypeVar};
    use yolang::types::Type;

    #[test]
    fn test_concrete_variants() {
        assert_eq!(InferType::int(), InferType::Concrete(Type::Int));
        assert_eq!(InferType::float(), InferType::Concrete(Type::Float));
        assert_eq!(InferType::bool(), InferType::Concrete(Type::Bool));
        assert_eq!(InferType::str(), InferType::Concrete(Type::Str));
        assert_eq!(InferType::unit(), InferType::Concrete(Type::Unit));
    }

    #[test]
    fn test_var_constructor() {
        let v = TypeVar(3);
        assert_eq!(InferType::var(v), InferType::Var(TypeVar(3)));
    }

    #[test]
    fn test_display_concrete() {
        assert_eq!(format!("{}", InferType::int()), "Int");
        assert_eq!(format!("{}", InferType::float()), "Float");
        assert_eq!(format!("{}", InferType::bool()), "Bool");
        assert_eq!(format!("{}", InferType::str()), "String");
        assert_eq!(format!("{}", InferType::unit()), "()");
    }

    #[test]
    fn test_display_var() {
        assert_eq!(format!("{}", InferType::var(TypeVar(0))), "?t0");
        assert_eq!(format!("{}", InferType::var(TypeVar(7))), "?t7");
    }

    #[test]
    fn test_display_fun() {
        let ty = InferType::Fun(
            vec![InferType::int(), InferType::bool()],
            Box::new(InferType::str()),
        );
        assert_eq!(format!("{}", ty), "fun(Int, Bool) -> String");
    }

    #[test]
    fn test_display_fun_no_params() {
        let ty = InferType::Fun(vec![], Box::new(InferType::unit()));
        assert_eq!(format!("{}", ty), "fun() -> ()");
    }

    #[test]
    fn test_display_tuple() {
        let ty = InferType::Tuple(vec![InferType::int(), InferType::bool()]);
        assert_eq!(format!("{}", ty), "(Int, Bool)");
    }

    #[test]
    fn test_display_array() {
        let ty = InferType::Array(Box::new(InferType::int()));
        assert_eq!(format!("{}", ty), "Int[]");
    }

    #[test]
    fn test_display_named_no_args() {
        let ty = InferType::Named("Foo".to_string(), vec![]);
        assert_eq!(format!("{}", ty), "Foo");
    }

    #[test]
    fn test_display_named_with_args() {
        let ty = InferType::Named("Map".to_string(), vec![InferType::str(), InferType::int()]);
        assert_eq!(format!("{}", ty), "Map<String, Int>");
    }

    #[test]
    fn test_display_with_type_vars() {
        let ty = InferType::Fun(
            vec![InferType::var(TypeVar(0))],
            Box::new(InferType::var(TypeVar(0))),
        );
        assert_eq!(format!("{}", ty), "fun(?t0) -> ?t0");
    }

    #[test]
    fn test_nested_types() {
        // Array of functions: fun(Int) -> Bool []
        let ty = InferType::Array(Box::new(InferType::Fun(
            vec![InferType::int()],
            Box::new(InferType::bool()),
        )));
        assert_eq!(format!("{}", ty), "fun(Int) -> Bool[]");
    }

    #[test]
    fn test_equality() {
        let a = InferType::Fun(
            vec![InferType::var(TypeVar(0))],
            Box::new(InferType::var(TypeVar(1))),
        );
        let b = InferType::Fun(
            vec![InferType::var(TypeVar(0))],
            Box::new(InferType::var(TypeVar(1))),
        );
        let c = InferType::Fun(
            vec![InferType::var(TypeVar(0))],
            Box::new(InferType::var(TypeVar(2))),
        );
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}

#[cfg(test)]
mod phase_3_substitution {
    use yolang::typeinference::{InferType, Substitution, TypeVar};

    #[test]
    fn test_bind_and_lookup() {
        let mut s = Substitution::new();
        s.bind(TypeVar(0), InferType::int());
        assert_eq!(s.lookup(TypeVar(0)), Some(&InferType::int()));
        assert_eq!(s.lookup(TypeVar(1)), None);
    }

    #[test]
    fn test_apply_concrete_unchanged() {
        let s = Substitution::new();
        assert_eq!(s.apply(&InferType::int()), InferType::int());
    }

    #[test]
    fn test_apply_resolves_var() {
        let mut s = Substitution::new();
        s.bind(TypeVar(0), InferType::int());
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_apply_unbound_var_unchanged() {
        let s = Substitution::new();
        assert_eq!(s.apply(&InferType::var(TypeVar(5))), InferType::var(TypeVar(5)));
    }

    #[test]
    fn test_apply_chains_transitively() {
        // ?t0 → ?t1, ?t1 → Int  ⟹  apply(?t0) = Int
        let mut s = Substitution::new();
        s.bind(TypeVar(0), InferType::var(TypeVar(1)));
        s.bind(TypeVar(1), InferType::int());
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_apply_nested_fun() {
        // fun(?t0) -> ?t1  with { ?t0→Bool, ?t1→Int }  ⟹  fun(Bool) -> Int
        let mut s = Substitution::new();
        s.bind(TypeVar(0), InferType::bool());
        s.bind(TypeVar(1), InferType::int());
        let ty = InferType::Fun(
            vec![InferType::var(TypeVar(0))],
            Box::new(InferType::var(TypeVar(1))),
        );
        let expected = InferType::Fun(vec![InferType::bool()], Box::new(InferType::int()));
        assert_eq!(s.apply(&ty), expected);
    }

    #[test]
    fn test_apply_array() {
        let mut s = Substitution::new();
        s.bind(TypeVar(0), InferType::str());
        let ty = InferType::Array(Box::new(InferType::var(TypeVar(0))));
        assert_eq!(s.apply(&ty), InferType::Array(Box::new(InferType::str())));
    }

    #[test]
    fn test_apply_tuple() {
        let mut s = Substitution::new();
        s.bind(TypeVar(0), InferType::int());
        s.bind(TypeVar(1), InferType::bool());
        let ty = InferType::Tuple(vec![InferType::var(TypeVar(0)), InferType::var(TypeVar(1))]);
        assert_eq!(s.apply(&ty), InferType::Tuple(vec![InferType::int(), InferType::bool()]));
    }

    #[test]
    fn test_apply_named() {
        let mut s = Substitution::new();
        s.bind(TypeVar(0), InferType::int());
        let ty = InferType::Named("List".to_string(), vec![InferType::var(TypeVar(0))]);
        assert_eq!(s.apply(&ty), InferType::Named("List".to_string(), vec![InferType::int()]));
    }

    #[test]
    fn test_compose_applies_other_to_self_values() {
        // s1: { ?t0 → ?t1 },  s2: { ?t1 → Int }
        // compose(s1, s2) should resolve ?t0 all the way to Int
        let mut s1 = Substitution::new();
        s1.bind(TypeVar(0), InferType::var(TypeVar(1)));
        let mut s2 = Substitution::new();
        s2.bind(TypeVar(1), InferType::int());

        let composed = s1.compose(&s2);
        assert_eq!(composed.apply(&InferType::var(TypeVar(0))), InferType::int());
        assert_eq!(composed.apply(&InferType::var(TypeVar(1))), InferType::int());
    }

    #[test]
    fn test_compose_self_wins_on_overlap() {
        // compose(s1, s2) means "apply s1 first, then s2".
        // s1: { ?t0 → Int },  s2: { ?t0 → Bool }
        // ?t0 → s2(s1(?t0)) = s2(Int) = Int  (s2 only binds vars, not concrete types)
        let mut s1 = Substitution::new();
        s1.bind(TypeVar(0), InferType::int());
        let mut s2 = Substitution::new();
        s2.bind(TypeVar(0), InferType::bool());

        let composed = s1.compose(&s2);
        assert_eq!(composed.apply(&InferType::var(TypeVar(0))), InferType::int());
    }
}

#[cfg(test)]
mod phase_4_unification {
    use yolang::typeinference::{unify, InferType, TypeVar};

    #[test]
    fn test_unify_identical_concrete() {
        let s = unify(&InferType::int(), &InferType::int()).unwrap();
        assert_eq!(s.apply(&InferType::int()), InferType::int());
    }

    #[test]
    fn test_unify_incompatible_concrete() {
        assert!(unify(&InferType::int(), &InferType::bool()).is_err());
        assert!(unify(&InferType::str(), &InferType::float()).is_err());
    }

    #[test]
    fn test_unify_var_with_concrete() {
        let s = unify(&InferType::var(TypeVar(0)), &InferType::int()).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_unify_concrete_with_var() {
        let s = unify(&InferType::bool(), &InferType::var(TypeVar(1))).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(1))), InferType::bool());
    }

    #[test]
    fn test_unify_var_with_var() {
        let s = unify(&InferType::var(TypeVar(0)), &InferType::var(TypeVar(1))).unwrap();
        // One of them should resolve to the other
        let result = s.apply(&InferType::var(TypeVar(0)));
        let other = s.apply(&InferType::var(TypeVar(1)));
        assert_eq!(result, other);
    }

    #[test]
    fn test_unify_var_with_itself() {
        let s = unify(&InferType::var(TypeVar(0)), &InferType::var(TypeVar(0))).unwrap();
        // Empty substitution — no binding needed
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::var(TypeVar(0)));
    }

    #[test]
    fn test_unify_function_types() {
        // fun(?t0) -> ?t0  with  fun(Int) -> Int  => ?t0 = Int
        let a = InferType::Fun(
            vec![InferType::var(TypeVar(0))],
            Box::new(InferType::var(TypeVar(0))),
        );
        let b = InferType::Fun(vec![InferType::int()], Box::new(InferType::int()));
        let s = unify(&a, &b).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_unify_function_arity_mismatch() {
        let a = InferType::Fun(vec![InferType::int()], Box::new(InferType::bool()));
        let b = InferType::Fun(
            vec![InferType::int(), InferType::int()],
            Box::new(InferType::bool()),
        );
        assert!(unify(&a, &b).is_err());
    }

    #[test]
    fn test_unify_function_return_type() {
        // fun(Int) -> ?t0  with  fun(Int) -> Bool  => ?t0 = Bool
        let a = InferType::Fun(vec![InferType::int()], Box::new(InferType::var(TypeVar(0))));
        let b = InferType::Fun(vec![InferType::int()], Box::new(InferType::bool()));
        let s = unify(&a, &b).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::bool());
    }

    #[test]
    fn test_unify_array_types() {
        // ?t0[]  with  Int[]  => ?t0 = Int
        let a = InferType::Array(Box::new(InferType::var(TypeVar(0))));
        let b = InferType::Array(Box::new(InferType::int()));
        let s = unify(&a, &b).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_unify_array_element_mismatch() {
        let a = InferType::Array(Box::new(InferType::int()));
        let b = InferType::Array(Box::new(InferType::bool()));
        assert!(unify(&a, &b).is_err());
    }

    #[test]
    fn test_unify_tuple_types() {
        // (?t0, Bool)  with  (Int, Bool)  => ?t0 = Int
        let a = InferType::Tuple(vec![InferType::var(TypeVar(0)), InferType::bool()]);
        let b = InferType::Tuple(vec![InferType::int(), InferType::bool()]);
        let s = unify(&a, &b).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_unify_tuple_length_mismatch() {
        let a = InferType::Tuple(vec![InferType::int()]);
        let b = InferType::Tuple(vec![InferType::int(), InferType::bool()]);
        assert!(unify(&a, &b).is_err());
    }

    #[test]
    fn test_unify_named_types() {
        // List<?t0>  with  List<Int>  => ?t0 = Int
        let a = InferType::Named("List".to_string(), vec![InferType::var(TypeVar(0))]);
        let b = InferType::Named("List".to_string(), vec![InferType::int()]);
        let s = unify(&a, &b).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_unify_named_type_name_mismatch() {
        let a = InferType::Named("List".to_string(), vec![InferType::int()]);
        let b = InferType::Named("Set".to_string(), vec![InferType::int()]);
        assert!(unify(&a, &b).is_err());
    }

    #[test]
    fn test_unify_incompatible_shapes() {
        // Fun vs Array
        let a = InferType::Fun(vec![InferType::int()], Box::new(InferType::bool()));
        let b = InferType::Array(Box::new(InferType::int()));
        assert!(unify(&a, &b).is_err());

        // Tuple vs Concrete
        assert!(unify(
            &InferType::Tuple(vec![InferType::int()]),
            &InferType::int()
        )
        .is_err());
    }

    #[test]
    fn test_occurs_check_array() {
        // ?t0 = ?t0[]  — should fail
        let a = InferType::var(TypeVar(0));
        let b = InferType::Array(Box::new(InferType::var(TypeVar(0))));
        assert!(unify(&a, &b).is_err());
    }

    #[test]
    fn test_occurs_check_function() {
        // ?t0 = fun(?t0) -> Int  — should fail
        let a = InferType::var(TypeVar(0));
        let b = InferType::Fun(vec![InferType::var(TypeVar(0))], Box::new(InferType::int()));
        assert!(unify(&a, &b).is_err());
    }

    #[test]
    fn test_unify_multi_var_function() {
        // fun(?t0, ?t1) -> ?t0  with  fun(Int, Bool) -> Int  => ?t0=Int, ?t1=Bool
        let a = InferType::Fun(
            vec![InferType::var(TypeVar(0)), InferType::var(TypeVar(1))],
            Box::new(InferType::var(TypeVar(0))),
        );
        let b = InferType::Fun(
            vec![InferType::int(), InferType::bool()],
            Box::new(InferType::int()),
        );
        let s = unify(&a, &b).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
        assert_eq!(s.apply(&InferType::var(TypeVar(1))), InferType::bool());
    }
}

#[cfg(test)]
mod phase_5_constraints {
    use yolang::typeinference::{solve_constraints, Constraint, InferType, TypeVar};
    use yolang::ast::Span;

    fn span() -> Span {
        Span::new(0, 1, "test")
    }

    #[test]
    fn test_single_constraint_var_concrete() {
        // ?t0 = Int  =>  { ?t0 → Int }
        let cs = vec![Constraint::new(InferType::var(TypeVar(0)), InferType::int(), span())];
        let s = solve_constraints(cs).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_multiple_independent_constraints() {
        // ?t0 = Int, ?t1 = Bool  =>  { ?t0 → Int, ?t1 → Bool }
        let cs = vec![
            Constraint::new(InferType::var(TypeVar(0)), InferType::int(), span()),
            Constraint::new(InferType::var(TypeVar(1)), InferType::bool(), span()),
        ];
        let s = solve_constraints(cs).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
        assert_eq!(s.apply(&InferType::var(TypeVar(1))), InferType::bool());
    }

    #[test]
    fn test_transitive_constraints() {
        // ?t0 = ?t1, ?t1 = Int  =>  ?t0 resolves to Int
        let cs = vec![
            Constraint::new(InferType::var(TypeVar(0)), InferType::var(TypeVar(1)), span()),
            Constraint::new(InferType::var(TypeVar(1)), InferType::int(), span()),
        ];
        let s = solve_constraints(cs).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::int());
        assert_eq!(s.apply(&InferType::var(TypeVar(1))), InferType::int());
    }

    #[test]
    fn test_conflicting_constraints_error() {
        // ?t0 = Int, ?t0 = Bool  =>  error
        let cs = vec![
            Constraint::new(InferType::var(TypeVar(0)), InferType::int(), span()),
            Constraint::new(InferType::var(TypeVar(0)), InferType::bool(), span()),
        ];
        assert!(solve_constraints(cs).is_err());
    }

    #[test]
    fn test_error_carries_span() {
        let bad_span = Span::new(10, 20, "myfile.yolo");
        let cs = vec![Constraint::new(InferType::int(), InferType::bool(), bad_span)];
        let err = solve_constraints(cs).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("myfile.yolo"));
        assert!(msg.contains("10"));
        assert!(msg.contains("20"));
    }

    #[test]
    fn test_empty_constraints() {
        let s = solve_constraints(vec![]).unwrap();
        // Empty substitution — variables unchanged
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), InferType::var(TypeVar(0)));
    }

    #[test]
    fn test_constraint_with_function_type() {
        // ?t0 = fun(Int) -> Bool
        let fun_ty = InferType::Fun(vec![InferType::int()], Box::new(InferType::bool()));
        let cs = vec![Constraint::new(InferType::var(TypeVar(0)), fun_ty.clone(), span())];
        let s = solve_constraints(cs).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(0))), fun_ty);
    }

    #[test]
    fn test_earlier_bindings_propagate() {
        // ?t0 = Int, fun(?t0) -> Bool = fun(?t1) -> Bool  =>  ?t1 = Int
        let cs = vec![
            Constraint::new(InferType::var(TypeVar(0)), InferType::int(), span()),
            Constraint::new(
                InferType::Fun(vec![InferType::var(TypeVar(0))], Box::new(InferType::bool())),
                InferType::Fun(vec![InferType::var(TypeVar(1))], Box::new(InferType::bool())),
                span(),
            ),
        ];
        let s = solve_constraints(cs).unwrap();
        assert_eq!(s.apply(&InferType::var(TypeVar(1))), InferType::int());
    }
}

#[cfg(test)]
mod phase_6_type_schemes {
    use std::collections::HashSet;
    use yolang::typeinference::{
        free_vars, generalize, instantiate, InferType, TypeScheme, TypeVar, TypeVarGenerator,
    };

    #[test]
    fn test_free_vars_concrete() {
        assert!(free_vars(&InferType::int()).is_empty());
        assert!(free_vars(&InferType::bool()).is_empty());
    }

    #[test]
    fn test_free_vars_var() {
        let vars = free_vars(&InferType::var(TypeVar(0)));
        assert_eq!(vars, [TypeVar(0)].into());
    }

    #[test]
    fn test_free_vars_fun() {
        // fun(?t0, Int) -> ?t1  =>  { ?t0, ?t1 }
        let ty = InferType::Fun(
            vec![InferType::var(TypeVar(0)), InferType::int()],
            Box::new(InferType::var(TypeVar(1))),
        );
        assert_eq!(free_vars(&ty), [TypeVar(0), TypeVar(1)].into());
    }

    #[test]
    fn test_free_vars_nested() {
        // ?t0[]  =>  { ?t0 }
        let ty = InferType::Array(Box::new(InferType::var(TypeVar(0))));
        assert_eq!(free_vars(&ty), [TypeVar(0)].into());
    }

    #[test]
    fn test_generalize_no_env() {
        // generalize(fun(?t0) -> ?t0, {})  =>  ∀?t0. fun(?t0) -> ?t0
        let ty = InferType::Fun(
            vec![InferType::var(TypeVar(0))],
            Box::new(InferType::var(TypeVar(0))),
        );
        let scheme = generalize(ty, &HashSet::new());
        assert_eq!(scheme.quantified_vars, vec![TypeVar(0)]);
    }

    #[test]
    fn test_generalize_env_blocks_capture() {
        // If ?t0 is free in the env, it must not be quantified
        let ty = InferType::Fun(
            vec![InferType::var(TypeVar(0))],
            Box::new(InferType::var(TypeVar(0))),
        );
        let env: HashSet<TypeVar> = [TypeVar(0)].into();
        let scheme = generalize(ty.clone(), &env);
        assert!(scheme.quantified_vars.is_empty());
        assert_eq!(scheme.ty, ty);
    }

    #[test]
    fn test_generalize_partial_capture() {
        // fun(?t0, ?t1) -> ?t0, env has ?t1  =>  only ?t0 is quantified
        let ty = InferType::Fun(
            vec![InferType::var(TypeVar(0)), InferType::var(TypeVar(1))],
            Box::new(InferType::var(TypeVar(0))),
        );
        let env: HashSet<TypeVar> = [TypeVar(1)].into();
        let scheme = generalize(ty, &env);
        assert_eq!(scheme.quantified_vars, vec![TypeVar(0)]);
    }

    #[test]
    fn test_generalize_monomorphic_type() {
        // Concrete type has no free vars — scheme is mono
        let scheme = generalize(InferType::int(), &HashSet::new());
        assert!(scheme.quantified_vars.is_empty());
    }

    #[test]
    fn test_instantiate_produces_fresh_vars() {
        // ∀?t0. fun(?t0) -> ?t0  instantiated with generator at 5
        // => fun(?t5) -> ?t5
        let scheme = TypeScheme {
            quantified_vars: vec![TypeVar(0)],
            ty: InferType::Fun(
                vec![InferType::var(TypeVar(0))],
                Box::new(InferType::var(TypeVar(0))),
            ),
        };
        let mut gen = TypeVarGenerator::new();
        // burn 0-4 so we can assert the exact fresh var
        for _ in 0..5 { gen.fresh(); }

        let result = instantiate(&scheme, &mut gen);
        let expected = InferType::Fun(
            vec![InferType::var(TypeVar(5))],
            Box::new(InferType::var(TypeVar(5))),
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_instantiate_twice_gives_different_vars() {
        let mut gen = TypeVarGenerator::new();
        // Use the generator to produce the quantified var, matching real usage
        // where schemes are built from vars that were already generated.
        let quantified = gen.fresh(); // TypeVar(0)
        let scheme = TypeScheme {
            quantified_vars: vec![quantified],
            ty: InferType::var(quantified),
        };
        let first = instantiate(&scheme, &mut gen);
        let second = instantiate(&scheme, &mut gen);
        assert_ne!(first, second);
    }

    #[test]
    fn test_instantiate_mono_unchanged() {
        let scheme = TypeScheme::mono(InferType::int());
        let mut gen = TypeVarGenerator::new();
        assert_eq!(instantiate(&scheme, &mut gen), InferType::int());
    }

    #[test]
    fn test_display_mono() {
        let scheme = TypeScheme::mono(InferType::int());
        assert_eq!(format!("{}", scheme), "Int");
    }

    #[test]
    fn test_display_poly() {
        let scheme = TypeScheme {
            quantified_vars: vec![TypeVar(0)],
            ty: InferType::Fun(
                vec![InferType::var(TypeVar(0))],
                Box::new(InferType::var(TypeVar(0))),
            ),
        };
        assert_eq!(format!("{}", scheme), "∀?t0. fun(?t0) -> ?t0");
    }

    #[test]
    fn test_display_multi_var() {
        let scheme = TypeScheme {
            quantified_vars: vec![TypeVar(0), TypeVar(1)],
            ty: InferType::Fun(
                vec![InferType::var(TypeVar(0))],
                Box::new(InferType::var(TypeVar(1))),
            ),
        };
        assert_eq!(format!("{}", scheme), "∀?t0, ?t1. fun(?t0) -> ?t1");
    }
}

#[cfg(test)]
mod phase_7_infer_context {
    use yolang::ast::Span;
    use yolang::typeinference::{generalize, InferContext, InferType, TypeScheme, TypeVar};
    use std::collections::HashSet;

    fn span() -> Span {
        Span::new(0, 1, "test")
    }

    #[test]
    fn test_fresh_var_sequential() {
        let mut ctx = InferContext::new();
        let a = ctx.fresh_var();
        let b = ctx.fresh_var();
        let c = ctx.fresh_var();
        assert_eq!(a, InferType::Var(TypeVar(0)));
        assert_eq!(b, InferType::Var(TypeVar(1)));
        assert_eq!(c, InferType::Var(TypeVar(2)));
    }

    #[test]
    fn test_bind_mono_and_lookup() {
        let mut ctx = InferContext::new();
        ctx.bind_mono("x", InferType::int());
        assert_eq!(ctx.lookup("x"), Some(InferType::int()));
    }

    #[test]
    fn test_lookup_unknown_is_none() {
        let mut ctx = InferContext::new();
        assert_eq!(ctx.lookup("unknown"), None);
    }

    #[test]
    fn test_bind_poly_auto_instantiates() {
        let mut ctx = InferContext::new();
        // Manually build ∀?t0. fun(?t0) -> ?t0
        // Use fresh_var so the generator counter is ahead of the quantified var
        let v = ctx.fresh_var(); // TypeVar(0)
        let scheme = TypeScheme {
            quantified_vars: vec![TypeVar(0)],
            ty: InferType::Fun(vec![v.clone()], Box::new(v)),
        };
        ctx.bind_poly("id", scheme);

        // Each lookup returns a fresh instantiation
        let t1 = ctx.lookup("id").unwrap();
        let t2 = ctx.lookup("id").unwrap();
        assert_ne!(t1, t2);
    }

    #[test]
    fn test_poly_lookup_produces_infer_type() {
        let mut ctx = InferContext::new();
        let v = ctx.fresh_var(); // TypeVar(0)
        let scheme = TypeScheme {
            quantified_vars: vec![TypeVar(0)],
            ty: v,
        };
        ctx.bind_poly("id", scheme);

        // Should be a Var, not the original ?t0
        let result = ctx.lookup("id").unwrap();
        assert!(matches!(result, InferType::Var(v) if v != TypeVar(0)));
    }

    #[test]
    fn test_poly_takes_precedence_over_mono() {
        let mut ctx = InferContext::new();
        let v = ctx.fresh_var(); // TypeVar(0)
        ctx.bind_mono("f", InferType::int());
        ctx.bind_poly("f", TypeScheme {
            quantified_vars: vec![TypeVar(0)],
            ty: v,
        });
        // Poly env wins — result is a fresh Var, not Int
        let result = ctx.lookup("f").unwrap();
        assert!(matches!(result, InferType::Var(_)));
    }

    #[test]
    fn test_add_constraint_and_solve() {
        let mut ctx = InferContext::new();
        let v = ctx.fresh_var(); // ?t0
        ctx.add_constraint(v, InferType::int(), span());

        let subst = ctx.solve().unwrap();
        assert_eq!(subst.apply(&InferType::Var(TypeVar(0))), InferType::int());
    }

    #[test]
    fn test_solve_conflicting_constraints_errors() {
        let mut ctx = InferContext::new();
        let v = ctx.fresh_var(); // ?t0
        ctx.add_constraint(v.clone(), InferType::int(), span());
        ctx.add_constraint(v, InferType::bool(), span());
        assert!(ctx.solve().is_err());
    }

    #[test]
    fn test_full_inference_scenario() {
        // Simulates: let id = fun(x) { x }; id(42)
        // Step 1: infer fun(x) { x } — give x a fresh var ?t0, body is also ?t0
        let mut ctx = InferContext::new();
        let x_ty = ctx.fresh_var();           // ?t0
        ctx.bind_mono("x", x_ty.clone());
        let body_ty = ctx.lookup("x").unwrap(); // ?t0

        // fun type is fun(?t0) -> ?t0
        let fun_ty = InferType::Fun(vec![x_ty.clone()], Box::new(body_ty));

        // Step 2: generalize and store as poly
        let scheme = generalize(fun_ty, &HashSet::new());
        ctx.bind_poly("id", scheme);

        // Step 3: call id(42) — instantiate id, unify with fun(Int) -> ?ret
        let id_ty = ctx.lookup("id").unwrap();  // fun(?t1) -> ?t1
        let ret_ty = ctx.fresh_var();           // ?t2
        let call_ty = InferType::Fun(vec![InferType::int()], Box::new(ret_ty.clone()));
        ctx.add_constraint(id_ty, call_ty, span());

        let subst = ctx.solve().unwrap();
        // ret_ty should resolve to Int
        assert_eq!(subst.apply(&ret_ty), InferType::int());
    }
}
