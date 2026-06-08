use num_bigint::BigUint;
use num_traits::One;
use zkp_circuit_compiler::ast::*;
use zkp_circuit_compiler::error::Result;
use zkp_circuit_compiler::flattener;
use zkp_circuit_compiler::lexer::Lexer;
use zkp_circuit_compiler::parser::Parser;
use zkp_circuit_compiler::r1cs::{bn128_prime, LinearCombination, R1csSystem};
use zkp_circuit_compiler::serializer;

fn lex_and_parse(source: &str) -> Result<Program> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    parser.parse()
}

fn compile_to_r1cs(source: &str) -> Result<R1csSystem> {
    let program = lex_and_parse(source)?;
    flattener::flatten(&program, bn128_prime())
}

#[test]
fn test_lexer_basic() {
    let source = "signal input a;";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(tokens.len(), 5);
}

#[test]
fn test_lexer_bigint() {
    let source = "123456789012345678901234567890;";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    assert_eq!(tokens.len(), 3);
    match &tokens[0].kind {
        zkp_circuit_compiler::lexer::TokenKind::BigInt(n) => {
            assert_eq!(
                *n,
                "123456789012345678901234567890"
                    .parse::<BigUint>()
                    .unwrap()
            );
        }
        _ => panic!("expected BigInt token"),
    }
}

#[test]
fn test_lexer_operators() {
    let source = "a <== b * c + d === e;";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    assert!(tokens.len() >= 8);
}

#[test]
fn test_parser_signal_decl() {
    let source = "signal input a; signal output b; signal c;";
    let program = lex_and_parse(source).unwrap();
    assert_eq!(program.statements.len(), 3);

    match &program.statements[0] {
        Statement::SignalDecl(s) => {
            assert!(matches!(s.kind, SignalKind::Input));
            assert_eq!(s.name, "a");
        }
        _ => panic!("expected signal decl"),
    }
}

#[test]
fn test_parser_assignment() {
    let source = "signal input a; signal output c; c <== a * a;";
    let program = lex_and_parse(source).unwrap();
    match &program.statements[2] {
        Statement::Assign { target, value } => {
            assert_eq!(target, "c");
            assert!(matches!(value, Expr::Mul(_, _)));
        }
        _ => panic!("expected assignment"),
    }
}

#[test]
fn test_parser_assert_bool() {
    let source = "signal flag; assert_bool(flag);";
    let program = lex_and_parse(source).unwrap();
    match &program.statements[1] {
        Statement::AssertBool(name) => assert_eq!(name, "flag"),
        _ => panic!("expected assert_bool"),
    }
}

#[test]
fn test_parser_conditional() {
    let source = "signal flag; signal input a; signal output c; if flag then c <== a;";
    let program = lex_and_parse(source).unwrap();
    match &program.statements[3] {
        Statement::Conditional {
            condition,
            then_stmt,
            else_stmt,
        } => {
            assert_eq!(condition, "flag");
            assert!(else_stmt.is_none());
            match then_stmt.as_ref() {
                Statement::Assign { target, .. } => assert_eq!(target, "c"),
                _ => panic!("expected assignment in then"),
            }
        }
        _ => panic!("expected conditional"),
    }
}

#[test]
fn test_linear_combination_add() {
    let prime = bn128_prime();
    let a = LinearCombination::from_var(1);
    let b = LinearCombination::from_var(2);
    let c = a.add(&b, &prime);
    assert_eq!(c.terms.get(&1), Some(&BigUint::one()));
    assert_eq!(c.terms.get(&2), Some(&BigUint::one()));
}

#[test]
fn test_linear_combination_sub() {
    let prime = bn128_prime();
    let a = LinearCombination::from_var(1);
    let one = LinearCombination::from_constant(BigUint::one());
    let result = one.sub(&a, &prime);
    assert_eq!(result.terms.get(&0), Some(&BigUint::one()));
    let neg_one = &prime - BigUint::one();
    assert_eq!(result.terms.get(&1), Some(&neg_one));
}

#[test]
fn test_r1cs_simple_mul() {
    let source = "signal input a;\nsignal input b;\nsignal output c;\nc <== a * b;";
    let system = compile_to_r1cs(source).unwrap();

    assert_eq!(system.num_variables, 4);
    assert_eq!(system.constraints.len(), 1);
    assert_eq!(system.num_public_inputs, 2);
    assert_eq!(system.num_public_outputs, 1);

    let constraint = &system.constraints[0];
    assert_eq!(constraint.a.terms.get(&1), Some(&BigUint::one()));
    assert_eq!(constraint.b.terms.get(&2), Some(&BigUint::one()));
    assert_eq!(constraint.c.terms.get(&3), Some(&BigUint::one()));
}

#[test]
fn test_r1cs_addition_constraint() {
    let source = "signal input a;\nsignal input b;\nsignal output c;\nc <== a + b;";
    let system = compile_to_r1cs(source).unwrap();

    assert_eq!(system.constraints.len(), 1);
    let constraint = &system.constraints[0];
    assert_eq!(constraint.a.terms.get(&1), Some(&BigUint::one()));
    assert_eq!(constraint.a.terms.get(&2), Some(&BigUint::one()));
    assert_eq!(constraint.b.terms.get(&0), Some(&BigUint::one()));
    assert_eq!(constraint.c.terms.get(&3), Some(&BigUint::one()));
}

#[test]
fn test_r1cs_boolean_constraint() {
    let source = "signal flag;\nassert_bool(flag);";
    let system = compile_to_r1cs(source).unwrap();

    assert_eq!(system.constraints.len(), 1);
    let constraint = &system.constraints[0];
    let flag_idx = 1;
    let prime = bn128_prime();
    let neg_one = &prime - BigUint::one();

    assert_eq!(constraint.a.terms.get(&flag_idx), Some(&BigUint::one()));
    assert_eq!(constraint.b.terms.get(&0), Some(&BigUint::one()));
    assert_eq!(constraint.b.terms.get(&flag_idx), Some(&neg_one));
    assert!(constraint.c.terms.is_empty());
}

#[test]
fn test_r1cs_conditional() {
    let source = "signal input a;\nsignal output c;\nsignal flag;\nassert_bool(flag);\nif flag then c <== a;";
    let system = compile_to_r1cs(source).unwrap();

    assert!(system.constraints.len() >= 2);

    let bool_constraint = &system.constraints[0];
    let flag_idx = 3;
    let prime = bn128_prime();
    let neg_one = &prime - BigUint::one();

    assert_eq!(bool_constraint.a.terms.get(&flag_idx), Some(&BigUint::one()));
    assert_eq!(bool_constraint.b.terms.get(&0), Some(&BigUint::one()));
    assert_eq!(bool_constraint.b.terms.get(&flag_idx), Some(&neg_one));
    assert!(bool_constraint.c.terms.is_empty());
}

#[test]
fn test_r1cs_complex_expression() {
    let source = r#"
        signal input a;
        signal input b;
        signal output c;
        signal t1;
        t1 <== a * b;
        c <== t1 + a;
    "#;
    let system = compile_to_r1cs(source).unwrap();
    assert_eq!(system.constraints.len(), 2);
    assert_eq!(system.num_variables, 5);
}

#[test]
fn test_serializer_roundtrip() {
    let source = r#"
        signal input a;
        signal input b;
        signal output c;
        c <== a * b;
    "#;
    let system = compile_to_r1cs(source).unwrap();

    let mut buffer = Vec::new();
    serializer::serialize(&system, &mut buffer).unwrap();

    assert!(buffer.len() > 12);
    assert_eq!(&buffer[0..4], b"r1cs");

    let report = serializer::deserialize_and_inspect(&buffer).unwrap();
    assert!(report.contains("BN128") || report.contains("21888242871839275222246405745257275088548364400416034343698204186575808495617"));
}

#[test]
fn test_full_pipeline() {
    let source = r#"
        signal input x;
        signal input y;
        signal output z;
        signal t1;
        signal t2;
        signal t3;
        t1 <== x * x;
        t2 <== y * y;
        t3 <== t1 + t2;
        z <== t3 * t3;
    "#;
    let system = compile_to_r1cs(source).unwrap();

    assert_eq!(system.num_public_inputs, 2);
    assert_eq!(system.num_public_outputs, 1);
    assert_eq!(system.constraints.len(), 4);

    let mut buffer = Vec::new();
    serializer::serialize(&system, &mut buffer).unwrap();

    let report = serializer::deserialize_and_inspect(&buffer).unwrap();
    assert!(report.contains("Constraints: 4"));
}

#[test]
fn test_mux_circuit() {
    let source = r#"
        signal input a;
        signal input b;
        signal output c;
        signal flag;
        assert_bool(flag);
        c <== flag * a + (1 - flag) * b;
    "#;
    let system = compile_to_r1cs(source).unwrap();

    assert!(system.constraints.len() >= 2);
    assert!(system.num_variables >= 5);
}

#[test]
fn test_parser_error_undefined() {
    let source = "x <== a * b;";
    let result = compile_to_r1cs(source);
    assert!(result.is_err());
}

#[test]
fn test_parser_equality_constraint() {
    let source = r#"
        signal input a;
        signal input b;
        a === b;
    "#;
    let system = compile_to_r1cs(source).unwrap();
    assert_eq!(system.constraints.len(), 1);
}

#[test]
fn test_conditional_auto_injects_bool_constraint() {
    let source = r#"
        signal input a;
        signal output c;
        signal flag;
        if flag then c <== a;
    "#;
    let system = compile_to_r1cs(source).unwrap();

    assert!(system.constraints.len() >= 2,
        "conditional MUST auto-inject boolean constraint on flag, got {} constraints",
        system.constraints.len());

    let bool_constraint = &system.constraints[0];
    let flag_idx = 3;
    let prime = bn128_prime();
    let neg_one = &prime - BigUint::one();

    assert_eq!(bool_constraint.a.terms.get(&flag_idx), Some(&BigUint::one()),
        "A side of bool constraint must be flag");
    assert_eq!(bool_constraint.b.terms.get(&0), Some(&BigUint::one()),
        "B side of bool constraint must contain constant 1");
    assert_eq!(bool_constraint.b.terms.get(&flag_idx), Some(&neg_one),
        "B side of bool constraint must contain -flag (i.e. 1-flag)");
    assert!(bool_constraint.c.terms.is_empty(),
        "C side of bool constraint must be 0");
}

#[test]
fn test_no_duplicate_bool_constraint() {
    let source = r#"
        signal input a;
        signal output c;
        signal flag;
        assert_bool(flag);
        if flag then c <== a;
    "#;
    let system_with_assert = compile_to_r1cs(source).unwrap();

    let source_no_assert = r#"
        signal input a;
        signal output c;
        signal flag;
        if flag then c <== a;
    "#;
    let system_no_assert = compile_to_r1cs(source_no_assert).unwrap();

    assert_eq!(system_with_assert.constraints.len(), system_no_assert.constraints.len(),
        "assert_bool + conditional must NOT produce duplicate boolean constraints");
}

#[test]
fn test_conditional_else_auto_injects_bool() {
    let source = r#"
        signal input a;
        signal input b;
        signal output c;
        signal flag;
        if flag then c <== a;
        else c <== b;
    "#;
    let system = compile_to_r1cs(source).unwrap();

    assert!(system.constraints.len() >= 3,
        "if-else must auto-inject 1 bool + 1 then + 1 else = 3 constraints, got {}",
        system.constraints.len());

    let bool_constraint = &system.constraints[0];
    let flag_idx = 4;
    let prime = bn128_prime();
    let neg_one = &prime - BigUint::one();

    assert_eq!(bool_constraint.a.terms.get(&flag_idx), Some(&BigUint::one()));
    assert_eq!(bool_constraint.b.terms.get(&0), Some(&BigUint::one()));
    assert_eq!(bool_constraint.b.terms.get(&flag_idx), Some(&neg_one));
    assert!(bool_constraint.c.terms.is_empty());
}

#[test]
fn test_bool_constraint_blocks_non_binary_flag() {
    let source = r#"
        signal input a;
        signal output c;
        signal flag;
        if flag then c <== a;
    "#;
    let system = compile_to_r1cs(source).unwrap();

    let bool_constraint = &system.constraints[0];
    let flag_idx = 3;

    let has_bool_constraint = bool_constraint.a.terms.get(&flag_idx).is_some()
        && bool_constraint.b.terms.contains_key(&0)
        && bool_constraint.b.terms.contains_key(&flag_idx)
        && bool_constraint.c.terms.is_empty();

    assert!(has_bool_constraint,
        "flag * (1 - flag) = 0 constraint must be present to block non-binary values like 0.5");
}
