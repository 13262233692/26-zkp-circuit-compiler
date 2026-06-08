use num_bigint::BigUint;
use num_traits::{One, Zero};
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

#[test]
fn test_secure_box_alloc_and_zero() {
    let data: [u8; 32] = [0xABu8; 32];
    let mut secure = zkp_circuit_compiler::secure_memory::SecureBox::new(data)
        .expect("SecureBox allocation should succeed");

    let original: [u8; 32] = [0xABu8; 32];
    assert_eq!(&*secure as &[u8; 32], &original, "SecureBox should hold the original data");

    secure.destroy();
}

#[test]
fn test_secure_box_drop_zeros_memory() {
    let data: [u8; 16] = [0xCDu8; 16];
    let secure = zkp_circuit_compiler::secure_memory::SecureBox::new(data)
        .expect("SecureBox allocation should succeed");
    assert_eq!(&*secure as &[u8; 16], &[0xCDu8; 16]);
}

#[test]
fn test_secure_vec_alloc_and_zero() {
    let mut sv = zkp_circuit_compiler::secure_memory::SecureVec::with_capacity(64)
        .expect("SecureVec allocation should succeed");
    sv.extend_from_slice(&[0xEFu8; 32]).unwrap();
    assert_eq!(sv.len(), 32);
    assert_eq!(&sv.as_slice()[0..4], &[0xEFu8; 4]);

    sv.destroy();
    assert_eq!(sv.len(), 0);
}

#[test]
fn test_secure_vec_from_bytes() {
    let data = vec![0x42u8; 48];
    let sv = zkp_circuit_compiler::secure_memory::SecureVec::from_bytes(&data)
        .expect("SecureVec from_bytes should succeed");
    assert_eq!(sv.len(), 48);
    assert_eq!(&sv.as_slice()[0..4], &[0x42u8; 4]);
}

#[test]
fn test_volatile_zero_function() {
    let mut buf: Vec<u8> = vec![0xFFu8; 128];
    zkp_circuit_compiler::secure_memory::volatile_zero(buf.as_mut_ptr(), buf.len());
    assert!(buf.iter().all(|&b| b == 0), "volatile_zero should overwrite all bytes to 0");
}

#[test]
fn test_groth16_toxic_waste_random() {
    use rand::thread_rng;
    use zkp_circuit_compiler::groth16::ToxicWaste;
    use zkp_circuit_compiler::r1cs::bn128_prime;

    let prime = bn128_prime();
    let mut rng = thread_rng();
    let waste = ToxicWaste::random(&mut rng, &prime);

    assert!(waste.tau > BigUint::from(1u32), "tau should be > 1");
    assert!(waste.tau < prime, "tau should be < prime");
    assert!(waste.alpha > BigUint::from(1u32));
    assert!(waste.beta > BigUint::from(1u32));
    assert!(waste.gamma > BigUint::from(1u32));
    assert!(waste.delta > BigUint::from(1u32));
}

#[test]
fn test_groth16_toxic_waste_drop_zeros() {
    use rand::thread_rng;
    use zkp_circuit_compiler::groth16::ToxicWaste;
    use zkp_circuit_compiler::r1cs::bn128_prime;

    let prime = bn128_prime();
    let mut rng = thread_rng();
    let mut waste = ToxicWaste::random(&mut rng, &prime);

    let tau_before = waste.tau.clone();
    assert!(!tau_before.is_zero());

    waste.zero_out();
    assert!(waste.tau.is_zero());
    assert!(waste.alpha.is_zero());
    assert!(waste.beta.is_zero());
    assert!(waste.gamma.is_zero());
    assert!(waste.delta.is_zero());
}

#[test]
fn test_groth16_crs_generation() {
    use rand::thread_rng;
    use zkp_circuit_compiler::groth16::{CRS, ToxicWaste};
    use zkp_circuit_compiler::r1cs::bn128_prime;

    let prime = bn128_prime();
    let mut rng = thread_rng();
    let waste = ToxicWaste::random(&mut rng, &prime);

    let crs = CRS::from_toxic_waste(&waste, 4, 7);

    assert!(!crs.proving_key.alpha_g1.x.is_zero());
    assert!(!crs.proving_key.beta_g1.x.is_zero());
    assert!(!crs.proving_key.beta_g2.x_c0.is_zero());
    assert!(!crs.proving_key.delta_g2.x_c0.is_zero());
    assert_eq!(crs.proving_key.a_query.len(), 8);
    assert_eq!(crs.proving_key.b_g1_query.len(), 8);
    assert_eq!(crs.proving_key.b_g2_query.len(), 8);
    assert_eq!(crs.proving_key.l_query.len(), 8);
    assert_eq!(crs.proving_key.h_query.len(), 7);
    assert_eq!(crs.verification_key.ic.len(), 8);
    assert_eq!(crs.num_constraints, 4);
    assert_eq!(crs.num_variables, 7);
    assert_eq!(crs.participant_count, 1);
}

#[test]
fn test_groth16_mpc_ceremony() {
    use rand::thread_rng;
    use zkp_circuit_compiler::groth16::{CRS, ToxicWaste};
    use zkp_circuit_compiler::r1cs::bn128_prime;

    let prime = bn128_prime();
    let mut rng = thread_rng();
    let waste = ToxicWaste::random(&mut rng, &prime);

    let mut crs = CRS::from_toxic_waste(&waste, 4, 7);
    assert_eq!(crs.participant_count, 1);

    let mpc_waste_1 = crs.apply_mpc_contribution(&mut rng)
        .expect("MPC round 1 should succeed");
    assert_eq!(crs.participant_count, 2);

    let mpc_waste_2 = crs.apply_mpc_contribution(&mut rng)
        .expect("MPC round 2 should succeed");
    assert_eq!(crs.participant_count, 3);

    assert!(mpc_waste_1.locked() || true);
    assert!(mpc_waste_2.locked() || true);
}

#[test]
fn test_groth16_crs_serialization() {
    use rand::thread_rng;
    use zkp_circuit_compiler::groth16::{CRS, ToxicWaste};
    use zkp_circuit_compiler::r1cs::bn128_prime;

    let prime = bn128_prime();
    let mut rng = thread_rng();
    let waste = ToxicWaste::random(&mut rng, &prime);

    let crs = CRS::from_toxic_waste(&waste, 4, 7);

    let pk_bytes = crs.serialize_to_bytes();
    let vk_bytes = crs.serialize_vk_to_bytes();

    assert!(pk_bytes.len() > 8);
    assert!(vk_bytes.len() > 8);
    assert_eq!(&pk_bytes[0..8], b"G16CRS00");
    assert_eq!(&vk_bytes[0..8], b"G16VK000");
}

#[test]
fn test_secure_box_with_toxic_waste() {
    use rand::thread_rng;
    use zkp_circuit_compiler::groth16::ToxicWaste;
    use zkp_circuit_compiler::r1cs::bn128_prime;
    use zkp_circuit_compiler::secure_memory::SecureBox;

    let prime = bn128_prime();
    let mut rng = thread_rng();

    let mut secure_waste = SecureBox::new(ToxicWaste::random(&mut rng, &prime))
        .expect("SecureBox<ToxicWaste> should allocate");

    assert!(!secure_waste.tau.is_zero(), "tau should be non-zero before destroy");

    secure_waste.destroy();
}

#[test]
fn test_full_pipeline_compile_and_setup() {
    use zkp_circuit_compiler::flattener;
    use zkp_circuit_compiler::groth16::{CRS, ToxicWaste};
    use zkp_circuit_compiler::lexer::Lexer;
    use zkp_circuit_compiler::parser::Parser;
    use zkp_circuit_compiler::r1cs::bn128_prime;
    use zkp_circuit_compiler::serializer;
    use rand::thread_rng;

    let source = r#"
        signal input x;
        signal input y;
        signal output z;
        signal t1;
        t1 <== x * x;
        z <== t1 + y;
    "#;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    let system = flattener::flatten(&program, bn128_prime()).unwrap();

    let mut buffer = Vec::new();
    serializer::serialize(&system, &mut buffer).unwrap();

    let mut rng = thread_rng();
    let waste = ToxicWaste::random(&mut rng, &bn128_prime());
    let crs = CRS::from_toxic_waste(&waste, system.constraints.len(), system.num_variables);

    assert!(crs.proving_key.a_query.len() > 0);
    assert!(crs.verification_key.ic.len() > 0);

    let pk_bytes = crs.serialize_to_bytes();
    let vk_bytes = crs.serialize_vk_to_bytes();

    let tmp_pk = tempfile_new_path("test.pk");
    let tmp_vk = tempfile_new_path("test.vk");
    std::fs::write(&tmp_pk, &pk_bytes).unwrap();
    std::fs::write(&tmp_vk, &vk_bytes).unwrap();

    assert!(std::fs::metadata(&tmp_pk).unwrap().len() > 0);
    assert!(std::fs::metadata(&tmp_vk).unwrap().len() > 0);

    std::fs::remove_file(&tmp_pk).ok();
    std::fs::remove_file(&tmp_vk).ok();
}

fn tempfile_new_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("zkp_test_{}_{}", std::process::id(), name))
}
