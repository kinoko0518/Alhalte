//! Comprehensive tests for the dice AST, deterministic evaluator, primitives, and semantic operations.

use generast::*;

// Mock entropy for deterministic verification of primitive operations.
struct FixedEntropy {
    rolls: Vec<Integer>,
    index: usize,
}

impl FixedEntropy {
    fn new(rolls: Vec<Integer>) -> Self {
        Self { rolls, index: 0 }
    }
}

impl Entropy for FixedEntropy {
    fn take(&mut self, faces: Integer) -> Result<Integer, EntropyError> {
        if faces == 0 {
            return Err(EntropyError::ZeroFaces);
        }
        if self.index < self.rolls.len() {
            let r = self.rolls[self.index];
            self.index += 1;
            Ok(r)
        } else {
            Ok(1)
        }
    }
}

// spec: generast-rs/dice-types Rational arithmetic, normalization, and zero-division
#[test]
fn test_rational_basics() {
    let r1 = Rational::new(6, 8).unwrap();
    assert_eq!(r1.to_string(), "3/4");

    let r2 = Rational::new(1, 4).unwrap();
    let sum = &r1 + &r2;
    assert_eq!(sum.to_string(), "1");
    assert_eq!(sum.floor().unwrap(), 1);

    let sub = &r1 - &r2;
    assert_eq!(sub.to_string(), "1/2");

    let mul = &r1 * &r2;
    assert_eq!(mul.to_string(), "3/16");

    let div = r1.checked_div(&r2).unwrap();
    assert_eq!(div.to_string(), "3");

    // Division by zero
    let err = Rational::new(1, 0).unwrap_err();
    assert_eq!(err, EvalError::DivisionByZero);

    let err2 = r1.checked_div(&Rational::zero()).unwrap_err();
    assert_eq!(err2, EvalError::DivisionByZero);
}

// spec: generast-rs/dice-types Floor, Ceil, and Round (Floor(Add(x, 1/2)))
#[test]
fn test_rational_floor_ceil_round() {
    // 5/2 = 2.5
    let r = Rational::new(5, 2).unwrap();
    assert_eq!(r.floor().unwrap(), 2);
    assert_eq!(r.ceil().unwrap(), 3);
    assert_eq!(r.round().unwrap(), 3); // 2.5 + 0.5 = 3.0 -> floor = 3

    // 7/3 = 2.333...
    let r2 = Rational::new(7, 3).unwrap();
    assert_eq!(r2.floor().unwrap(), 2);
    assert_eq!(r2.ceil().unwrap(), 3);
    assert_eq!(r2.round().unwrap(), 2); // 2.333 + 0.5 = 2.833 -> floor = 2

    // 8/3 = 2.666...
    let r3 = Rational::new(8, 3).unwrap();
    assert_eq!(r3.round().unwrap(), 3); // 2.666 + 0.5 = 3.166 -> floor = 3
}

// spec: generast-rs/entropy SeededEntropy ChaCha20 determinism and zero faces check
#[test]
fn test_seeded_entropy_determinism() {
    let seed = [123u8; 32];
    let mut e1 = SeededEntropy::from_seed(seed);
    let mut e2 = SeededEntropy::from_seed(seed);

    let rolls1: Vec<_> = (0..50).map(|_| e1.take(100).unwrap()).collect();
    let rolls2: Vec<_> = (0..50).map(|_| e2.take(100).unwrap()).collect();

    assert_eq!(rolls1, rolls2);
    for r in &rolls1 {
        assert!(*r >= 1 && *r <= 100);
    }

    // Zero faces error
    let mut e = SeededEntropy::from_seed(seed);
    assert_eq!(e.take(0).unwrap_err(), EntropyError::ZeroFaces);
}

// spec: generast-rs/primitives MakeDice and Take with trace
#[test]
fn test_primitive_make_dice_and_take() {
    let expr = DiceExpr::Take(Box::new(DiceExpr::MakeDice(Box::new(DiceExpr::Integer(6)))));
    let mut entropy = FixedEntropy::new(vec![4]);
    let (val, trace) = expr.eval_with_trace(&mut entropy).unwrap();
    assert_eq!(val, DiceValue::Integer(4));
    assert_eq!(trace, vec![RollRecord { faces: 6, roll: 4 }]);
}

// spec: generast-rs/primitives ItoR, Floor, Add, Sub, Mul, Div, Eq, Lt, Not, And, If
#[test]
fn test_primitive_arithmetic_and_logic() {
    let mut entropy = FixedEntropy::new(vec![]);

    // (5 + 3) * 2 / 4 = 4
    let expr = DiceExpr::Div(
        Box::new(DiceExpr::Mul(
            Box::new(DiceExpr::Add(
                Box::new(DiceExpr::ItoR(Box::new(DiceExpr::Integer(5)))),
                Box::new(DiceExpr::ItoR(Box::new(DiceExpr::Integer(3)))),
            )),
            Box::new(DiceExpr::ItoR(Box::new(DiceExpr::Integer(2)))),
        )),
        Box::new(DiceExpr::ItoR(Box::new(DiceExpr::Integer(4)))),
    );
    let val = expr.eval(&mut entropy).unwrap();
    assert_eq!(val, DiceValue::Rational(Rational::from_integer(4)));

    // Division by zero in Div node
    let div_zero = DiceExpr::Div(
        Box::new(DiceExpr::Rational(Rational::from_integer(10))),
        Box::new(DiceExpr::Rational(Rational::zero())),
    );
    assert_eq!(
        div_zero.eval(&mut entropy).unwrap_err(),
        EvalError::DivisionByZero
    );

    // Lt & Eq & Not & And & If
    // If(Lt(2, 5) && !Eq(2, 5), "yes", "no")
    let cond_expr = DiceExpr::If {
        cond: Box::new(DiceExpr::And(
            Box::new(DiceExpr::Lt(
                Box::new(DiceExpr::Rational(Rational::from_integer(2))),
                Box::new(DiceExpr::Rational(Rational::from_integer(5))),
            )),
            Box::new(DiceExpr::Not(Box::new(DiceExpr::Eq(
                Box::new(DiceExpr::Rational(Rational::from_integer(2))),
                Box::new(DiceExpr::Rational(Rational::from_integer(5))),
            )))),
        )),
        then_branch: Box::new(DiceExpr::String("yes".to_string())),
        else_branch: Box::new(DiceExpr::String("no".to_string())),
    };
    let cond_val = cond_expr.eval(&mut entropy).unwrap();
    assert_eq!(cond_val, DiceValue::String("yes".to_string()));
}

// spec: generast-rs/primitives DiceUntil infinite loop prevention
#[test]
fn test_dice_until_infinite_loop_rejection() {
    // A predicate where all faces of d6 satisfy the condition (e.g. roll >= 1)
    let bad_pred = Pred::last_roll(Pred::ge(1));
    let err = validate_dice_until(6, &bad_pred).unwrap_err();
    assert_eq!(err, EvalError::UnstoppableDiceUntil);

    // Construction through smart constructor
    let d6 = DiceExpr::Dice(Dice::new(6));
    let ctor_err = DiceExpr::dice_until(d6, bad_pred).unwrap_err();
    assert_eq!(ctor_err, EvalError::UnstoppableDiceUntil);

    // Predicate with terminating condition (e.g. reroll while roll == 6 on d6)
    let good_pred = Pred::last_roll(Pred::eq(6));
    assert!(validate_dice_until(6, &good_pred).is_ok());

    // Length-bounded predicate is always valid
    let len_pred = Pred::length_less_than(10);
    assert!(validate_dice_until(6, &len_pred).is_ok());
}

// spec: generast-rs/primitives DiceUntil evaluation and execution
#[test]
fn test_dice_until_execution() {
    // Rolls d6 while roll == 6. Sequence: 6, 6, 3 (terminates on 3)
    let mut entropy = FixedEntropy::new(vec![6, 6, 3]);
    let expr = DiceExpr::DiceUntil {
        dice: Box::new(DiceExpr::Dice(Dice::new(6))),
        pred: Pred::last_roll(Pred::eq(6)),
    };
    let (val, trace) = expr.eval_with_trace(&mut entropy).unwrap();
    assert_eq!(
        val,
        DiceValue::List(vec![
            DiceValue::Integer(6),
            DiceValue::Integer(6),
            DiceValue::Integer(3),
        ])
    );
    assert_eq!(trace.len(), 3);
}

// spec: generast-rs/primitives Take limit (4096) exceeded check
#[test]
fn test_take_limit_exceeded() {
    let mut entropy = FixedEntropy::new(vec![1; 10000]);
    let expr = DiceExpr::RollN {
        dice: Box::new(DiceExpr::Dice(Dice::new(6))),
        count: Box::new(DiceExpr::Integer(5000)),
    };
    let err = expr.eval(&mut entropy).unwrap_err();
    assert_eq!(err, EvalError::TakeLimitExceeded { limit: 4096 });
}

// spec: generast-rs/primitives Filter, Fold, Flatten, SortAsc, Slice, Length
#[test]
fn test_primitive_list_operations() {
    let mut entropy = FixedEntropy::new(vec![]);

    // List: [5, 2, 8, 1, 9]
    let list_expr = DiceExpr::List(vec![
        DiceExpr::Rational(Rational::from_integer(5)),
        DiceExpr::Rational(Rational::from_integer(2)),
        DiceExpr::Rational(Rational::from_integer(8)),
        DiceExpr::Rational(Rational::from_integer(1)),
        DiceExpr::Rational(Rational::from_integer(9)),
    ]);

    // Length
    let len_val = DiceExpr::Length(Box::new(list_expr.clone()))
        .eval(&mut entropy)
        .unwrap();
    assert_eq!(len_val, DiceValue::Integer(5));

    // SortAsc -> [1, 2, 5, 8, 9]
    let sorted = DiceExpr::SortAsc(Box::new(list_expr.clone()))
        .eval(&mut entropy)
        .unwrap();
    assert_eq!(
        sorted,
        DiceValue::List(vec![
            DiceValue::Rational(Rational::from_integer(1)),
            DiceValue::Rational(Rational::from_integer(2)),
            DiceValue::Rational(Rational::from_integer(5)),
            DiceValue::Rational(Rational::from_integer(8)),
            DiceValue::Rational(Rational::from_integer(9)),
        ])
    );

    // Slice: start 1, len 3 from sorted -> [2, 5, 8]
    let sliced = DiceExpr::Slice {
        list: Box::new(DiceExpr::SortAsc(Box::new(list_expr.clone()))),
        start: Box::new(DiceExpr::Integer(1)),
        length: Box::new(DiceExpr::Integer(3)),
    }
    .eval(&mut entropy)
    .unwrap();
    assert_eq!(
        sliced,
        DiceValue::List(vec![
            DiceValue::Rational(Rational::from_integer(2)),
            DiceValue::Rational(Rational::from_integer(5)),
            DiceValue::Rational(Rational::from_integer(8)),
        ])
    );

    // Filter: elements >= 5 -> [5, 8, 9]
    let filtered = DiceExpr::Filter {
        list: Box::new(list_expr.clone()),
        pred: Pred::ge(5),
    }
    .eval(&mut entropy)
    .unwrap();
    assert_eq!(
        filtered,
        DiceValue::List(vec![
            DiceValue::Rational(Rational::from_integer(5)),
            DiceValue::Rational(Rational::from_integer(8)),
            DiceValue::Rational(Rational::from_integer(9)),
        ])
    );

    // Fold: sum with Add -> 25
    let folded = DiceExpr::Fold {
        list: Box::new(list_expr),
        init: Box::new(DiceExpr::Rational(Rational::zero())),
        op: FoldOp::Add,
    }
    .eval(&mut entropy)
    .unwrap();
    assert_eq!(folded, DiceValue::Rational(Rational::from_integer(25)));

    // Flatten: [[1, 2], [3], 4] -> [1, 2, 3, 4]
    let nested_list = DiceExpr::List(vec![
        DiceExpr::List(vec![DiceExpr::Integer(1), DiceExpr::Integer(2)]),
        DiceExpr::List(vec![DiceExpr::Integer(3)]),
        DiceExpr::Integer(4),
    ]);
    let flat = DiceExpr::Flatten(Box::new(nested_list))
        .eval(&mut entropy)
        .unwrap();
    assert_eq!(
        flat,
        DiceValue::List(vec![
            DiceValue::Integer(1),
            DiceValue::Integer(2),
            DiceValue::Integer(3),
            DiceValue::Integer(4),
        ])
    );
}

// spec: generast-rs/semantics Sum, KeepHighest, KeepLowest, DropHighest, DropLowest
#[test]
fn test_semantic_keep_drop_and_sum() {
    let mut entropy = FixedEntropy::new(vec![]);
    // List: [10, 30, 20, 50, 40]
    let list_expr = DiceExpr::List(vec![
        DiceExpr::Rational(Rational::from_integer(10)),
        DiceExpr::Rational(Rational::from_integer(30)),
        DiceExpr::Rational(Rational::from_integer(20)),
        DiceExpr::Rational(Rational::from_integer(50)),
        DiceExpr::Rational(Rational::from_integer(40)),
    ]);

    // Sum -> 150
    let sum_val = DiceExpr::Sum(Box::new(list_expr.clone()))
        .eval(&mut entropy)
        .unwrap();
    assert_eq!(sum_val, DiceValue::Rational(Rational::from_integer(150)));

    // KeepHighest 2 -> [40, 50]
    let keep_h = DiceExpr::KeepHighest {
        list: Box::new(list_expr.clone()),
        k: Box::new(DiceExpr::Integer(2)),
    }
    .eval(&mut entropy)
    .unwrap();
    assert_eq!(
        keep_h,
        DiceValue::List(vec![
            DiceValue::Rational(Rational::from_integer(40)),
            DiceValue::Rational(Rational::from_integer(50)),
        ])
    );

    // KeepLowest 2 -> [10, 20]
    let keep_l = DiceExpr::KeepLowest {
        list: Box::new(list_expr.clone()),
        k: Box::new(DiceExpr::Integer(2)),
    }
    .eval(&mut entropy)
    .unwrap();
    assert_eq!(
        keep_l,
        DiceValue::List(vec![
            DiceValue::Rational(Rational::from_integer(10)),
            DiceValue::Rational(Rational::from_integer(20)),
        ])
    );

    // DropHighest 2 -> [10, 20, 30]
    let drop_h = DiceExpr::DropHighest {
        list: Box::new(list_expr.clone()),
        k: Box::new(DiceExpr::Integer(2)),
    }
    .eval(&mut entropy)
    .unwrap();
    assert_eq!(
        drop_h,
        DiceValue::List(vec![
            DiceValue::Rational(Rational::from_integer(10)),
            DiceValue::Rational(Rational::from_integer(20)),
            DiceValue::Rational(Rational::from_integer(30)),
        ])
    );

    // DropLowest 2 -> [30, 40, 50]
    let drop_l = DiceExpr::DropLowest {
        list: Box::new(list_expr),
        k: Box::new(DiceExpr::Integer(2)),
    }
    .eval(&mut entropy)
    .unwrap();
    assert_eq!(
        drop_l,
        DiceValue::List(vec![
            DiceValue::Rational(Rational::from_integer(30)),
            DiceValue::Rational(Rational::from_integer(40)),
            DiceValue::Rational(Rational::from_integer(50)),
        ])
    );
}

// spec: generast-rs/semantics RollN and CountSuccess
#[test]
fn test_semantic_roll_n_and_count_success() {
    let mut entropy = FixedEntropy::new(vec![3, 6, 2, 6, 5]);
    // 5d6
    let roll_expr = DiceExpr::RollN {
        dice: Box::new(DiceExpr::Dice(Dice::new(6))),
        count: Box::new(DiceExpr::Integer(5)),
    };
    // Count rolls >= 5
    let count_expr = DiceExpr::CountSuccess {
        list: Box::new(roll_expr),
        pred: Pred::ge(5),
    };
    let (val, trace) = count_expr.eval_with_trace(&mut entropy).unwrap();
    assert_eq!(val, DiceValue::Integer(3)); // 6, 6, 5 >= 5
    assert_eq!(trace.len(), 5);
}

// spec: generast-rs/semantics RerollDice and UpperDice
#[test]
fn test_semantic_reroll_and_upper_dice() {
    // 2 dice: die 1 rolls 6, then 2. die 2 rolls 6, then 6, then 1.
    let mut entropy = FixedEntropy::new(vec![6, 2, 6, 6, 1]);
    let reroll_expr = DiceExpr::RerollDice {
        dice_list: Box::new(DiceExpr::List(vec![
            DiceExpr::Dice(Dice::new(6)),
            DiceExpr::Dice(Dice::new(6)),
        ])),
        pred: Pred::eq(6),
    };
    let val = reroll_expr.eval(&mut entropy).unwrap();
    assert_eq!(
        val,
        DiceValue::List(vec![
            DiceValue::Integer(6),
            DiceValue::Integer(2),
            DiceValue::Integer(6),
            DiceValue::Integer(6),
            DiceValue::Integer(1),
        ])
    );

    // UpperDice: 2 groups, rolls on d6 while roll >= 5
    // group 1: 5, 3. group 2: 6, 5, 2.
    let mut entropy2 = FixedEntropy::new(vec![5, 3, 6, 5, 2]);
    let upper_expr = DiceExpr::UpperDice {
        dice: Box::new(DiceExpr::Dice(Dice::new(6))),
        groups: Box::new(DiceExpr::Integer(2)),
        pred: Pred::ge(5),
    };
    let val2 = upper_expr.eval(&mut entropy2).unwrap();
    assert_eq!(
        val2,
        DiceValue::List(vec![
            DiceValue::List(vec![DiceValue::Integer(5), DiceValue::Integer(3)]),
            DiceValue::List(vec![
                DiceValue::Integer(6),
                DiceValue::Integer(5),
                DiceValue::Integer(2),
            ]),
        ])
    );
}

// spec: generast-rs/semantics D66 rolls with SortMode
#[test]
fn test_semantic_d66() {
    // Rolls 5 and 2
    let mut e1 = FixedEntropy::new(vec![5, 2]);
    assert_eq!(
        DiceExpr::D66(SortMode::None).eval(&mut e1).unwrap(),
        DiceValue::Integer(52)
    );

    let mut e2 = FixedEntropy::new(vec![5, 2]);
    assert_eq!(
        DiceExpr::D66(SortMode::Asc).eval(&mut e2).unwrap(),
        DiceValue::Integer(25)
    );

    let mut e3 = FixedEntropy::new(vec![2, 5]);
    assert_eq!(
        DiceExpr::D66(SortMode::Desc).eval(&mut e3).unwrap(),
        DiceValue::Integer(52)
    );
}

// spec: generast-rs/semantics Tally normal and exclude_zero (TY)
#[test]
fn test_semantic_tally() {
    // 5d4 rolls: 1, 2, 2, 4, 4 (no 3s)
    let mut e1 = FixedEntropy::new(vec![1, 2, 2, 4, 4]);
    let tally_normal = DiceExpr::Tally {
        dice: Box::new(DiceExpr::Dice(Dice::new(4))),
        count: Box::new(DiceExpr::Integer(5)),
        exclude_zero: false,
    };
    let res1 = tally_normal.eval(&mut e1).unwrap();
    assert_eq!(
        res1,
        DiceValue::List(vec![
            DiceValue::List(vec![DiceValue::Integer(1), DiceValue::Integer(1)]),
            DiceValue::List(vec![DiceValue::Integer(2), DiceValue::Integer(2)]),
            DiceValue::List(vec![DiceValue::Integer(3), DiceValue::Integer(0)]),
            DiceValue::List(vec![DiceValue::Integer(4), DiceValue::Integer(2)]),
        ])
    );

    let mut e2 = FixedEntropy::new(vec![1, 2, 2, 4, 4]);
    let tally_ty = DiceExpr::Tally {
        dice: Box::new(DiceExpr::Dice(Dice::new(4))),
        count: Box::new(DiceExpr::Integer(5)),
        exclude_zero: true,
    };
    let res2 = tally_ty.eval(&mut e2).unwrap();
    assert_eq!(
        res2,
        DiceValue::List(vec![
            DiceValue::List(vec![DiceValue::Integer(1), DiceValue::Integer(1)]),
            DiceValue::List(vec![DiceValue::Integer(2), DiceValue::Integer(2)]),
            DiceValue::List(vec![DiceValue::Integer(4), DiceValue::Integer(2)]),
        ])
    );
}

// spec: generast-rs/semantics Choose sampling without replacement
#[test]
fn test_semantic_choose() {
    // Options: ["A", "B", "C", "D"]
    // Choose 2.
    // 1st take from 4: roll 2 -> "B" chosen, remaining ["A", "C", "D"]
    // 2nd take from 3: roll 1 -> "A" chosen, remaining ["C", "D"]
    let mut entropy = FixedEntropy::new(vec![2, 1]);
    let choose_expr = DiceExpr::Choose {
        list: Box::new(DiceExpr::List(vec![
            DiceExpr::String("A".to_string()),
            DiceExpr::String("B".to_string()),
            DiceExpr::String("C".to_string()),
            DiceExpr::String("D".to_string()),
        ])),
        count: Box::new(DiceExpr::Integer(2)),
    };
    let val = choose_expr.eval(&mut entropy).unwrap();
    assert_eq!(
        val,
        DiceValue::List(vec![
            DiceValue::String("B".to_string()),
            DiceValue::String("A".to_string()),
        ])
    );
}

// spec: generast-rs/semantics Lookup table reference
#[test]
fn test_semantic_lookup() {
    let mut entropy = FixedEntropy::new(vec![]);
    let table = DiceExpr::List(vec![
        DiceExpr::List(vec![
            DiceExpr::Integer(1),
            DiceExpr::String("Critical Fumble".to_string()),
        ]),
        DiceExpr::List(vec![
            DiceExpr::Integer(2),
            DiceExpr::String("Normal Result".to_string()),
        ]),
        DiceExpr::List(vec![
            DiceExpr::Integer(3),
            DiceExpr::String("Special Critical".to_string()),
        ]),
    ]);

    let lookup_hit = DiceExpr::Lookup {
        table: Box::new(table.clone()),
        key: Box::new(DiceExpr::Integer(3)),
    };
    assert_eq!(
        lookup_hit.eval(&mut entropy).unwrap(),
        DiceValue::String("Special Critical".to_string())
    );

    let lookup_miss = DiceExpr::Lookup {
        table: Box::new(table),
        key: Box::new(DiceExpr::Integer(99)),
    };
    assert_eq!(
        lookup_miss.eval(&mut entropy).unwrap_err(),
        EvalError::LookupNotFound(99)
    );
}

// spec: generast-rs/semantics Repeat command
#[test]
fn test_semantic_repeat() {
    // Repeat 3 times: 1d6 roll. Rolls: 3, 5, 1
    let mut entropy = FixedEntropy::new(vec![3, 5, 1]);
    let repeat_expr = DiceExpr::Repeat {
        count: Box::new(DiceExpr::Integer(3)),
        expr: Box::new(DiceExpr::Take(Box::new(DiceExpr::Dice(Dice::new(6))))),
    };
    let (val, trace) = repeat_expr.eval_with_trace(&mut entropy).unwrap();
    assert_eq!(
        val,
        DiceValue::List(vec![
            DiceValue::Integer(3),
            DiceValue::Integer(5),
            DiceValue::Integer(1),
        ])
    );
    assert_eq!(trace.len(), 3);
}

// spec: generast-rs/eval-trace Depth-first left-to-right evaluation trace order
#[test]
fn test_eval_trace_dfs_left_to_right() {
    // (1d6 + 1d8) * 1d10
    // Rolls: d6 -> 4, d8 -> 7, d10 -> 9
    let mut entropy = FixedEntropy::new(vec![4, 7, 9]);
    let expr = DiceExpr::Mul(
        Box::new(DiceExpr::Add(
            Box::new(DiceExpr::ItoR(Box::new(DiceExpr::Take(Box::new(
                DiceExpr::Dice(Dice::new(6)),
            ))))),
            Box::new(DiceExpr::ItoR(Box::new(DiceExpr::Take(Box::new(
                DiceExpr::Dice(Dice::new(8)),
            ))))),
        )),
        Box::new(DiceExpr::ItoR(Box::new(DiceExpr::Take(Box::new(
            DiceExpr::Dice(Dice::new(10)),
        ))))),
    );

    let (val, trace) = expr.eval_with_trace(&mut entropy).unwrap();
    // (4 + 7) * 9 = 99
    assert_eq!(val, DiceValue::Rational(Rational::from_integer(99)));
    assert_eq!(
        trace,
        vec![
            RollRecord { faces: 6, roll: 4 },
            RollRecord { faces: 8, roll: 7 },
            RollRecord { faces: 10, roll: 9 },
        ]
    );
}
