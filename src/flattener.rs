use std::collections::{HashMap, HashSet};

use num_bigint::BigUint;
use num_traits::One;

use crate::ast::{Expr, Program, SignalKind, Statement};
use crate::error::{CompileError, Result};
use crate::r1cs::{LinearCombination, R1csSystem};

struct Flattener {
    system: R1csSystem,
    signal_map: HashMap<String, usize>,
    bool_constrained: HashSet<String>,
    temp_counter: usize,
}

impl Flattener {
    fn new(prime: BigUint) -> Self {
        Flattener {
            system: R1csSystem::new(prime),
            signal_map: HashMap::new(),
            bool_constrained: HashSet::new(),
            temp_counter: 0,
        }
    }

    fn fresh_temp(&mut self) -> usize {
        self.temp_counter += 1;
        let name = format!("$tmp{}", self.temp_counter);
        self.system.allocate_variable(&name)
    }

    fn flatten_expr(&mut self, expr: &Expr) -> Result<LinearCombination> {
        match expr {
            Expr::Const(n) => {
                let reduced = n % &self.system.prime;
                Ok(LinearCombination::from_constant(reduced))
            }
            Expr::Var(name) => {
                let idx = self.signal_map.get(name).ok_or_else(|| {
                    CompileError::R1csError {
                        message: format!("undefined signal: {}", name),
                    }
                })?;
                Ok(LinearCombination::from_var(*idx))
            }
            Expr::Add(a, b) => {
                let lc_a = self.flatten_expr(a)?;
                let lc_b = self.flatten_expr(b)?;
                Ok(lc_a.add(&lc_b, &self.system.prime))
            }
            Expr::Sub(a, b) => {
                let lc_a = self.flatten_expr(a)?;
                let lc_b = self.flatten_expr(b)?;
                Ok(lc_a.sub(&lc_b, &self.system.prime))
            }
            Expr::Mul(a, b) => {
                let lc_a = self.flatten_expr(a)?;
                let lc_b = self.flatten_expr(b)?;
                let tmp_idx = self.fresh_temp();
                let lc_result = LinearCombination::from_var(tmp_idx);
                self.system.add_constraint(lc_a, lc_b, lc_result.clone());
                Ok(lc_result)
            }
            Expr::Neg(a) => {
                let lc_a = self.flatten_expr(a)?;
                let zero = LinearCombination::new();
                let neg_lc = zero.sub(&lc_a, &self.system.prime);
                let tmp_idx = self.fresh_temp();
                let lc_result = LinearCombination::from_var(tmp_idx);
                let one_lc = LinearCombination::from_constant(BigUint::one());
                self.system.add_constraint(neg_lc, one_lc, lc_result.clone());
                Ok(lc_result)
            }
        }
    }

    fn enforce_boolean(&mut self, name: &str) -> Result<()> {
        if self.bool_constrained.contains(name) {
            return Ok(());
        }
        let idx = *self.signal_map.get(name).ok_or_else(|| {
            CompileError::R1csError {
                message: format!("undefined signal: {}", name),
            }
        })?;
        let lc = LinearCombination::from_var(idx);
        let one_lc = LinearCombination::from_constant(BigUint::one());
        let one_minus_lc = one_lc.sub(&lc, &self.system.prime);
        let zero_lc = LinearCombination::new();
        self.system.add_constraint(lc, one_minus_lc, zero_lc);
        self.bool_constrained.insert(name.to_string());
        Ok(())
    }

    fn flatten_statement(&mut self, stmt: &Statement) -> Result<()> {
        match stmt {
            Statement::SignalDecl(decl) => {
                let idx = self.system.allocate_variable(&decl.name);
                self.signal_map.insert(decl.name.clone(), idx);
                match decl.kind {
                    SignalKind::Input => self.system.num_public_inputs += 1,
                    SignalKind::Output => self.system.num_public_outputs += 1,
                    SignalKind::Intermediate => self.system.num_private_inputs += 1,
                }
                Ok(())
            }
            Statement::Assign { target, value } => {
                let target_idx = *self.signal_map.get(target).ok_or_else(|| {
                    CompileError::R1csError {
                        message: format!("undefined signal: {}", target),
                    }
                })?;
                let lc_target = LinearCombination::from_var(target_idx);
                match value {
                    Expr::Mul(a, b) => {
                        let lc_a = self.flatten_expr(a)?;
                        let lc_b = self.flatten_expr(b)?;
                        self.system.add_constraint(lc_a, lc_b, lc_target);
                    }
                    _ => {
                        let lc_value = self.flatten_expr(value)?;
                        let one_lc = LinearCombination::from_constant(BigUint::one());
                        self.system.add_constraint(lc_value, one_lc, lc_target);
                    }
                }
                Ok(())
            }
            Statement::Constraint { lhs, rhs } => {
                let lc_lhs = self.flatten_expr(lhs)?;
                let lc_rhs = self.flatten_expr(rhs)?;
                let diff = lc_lhs.sub(&lc_rhs, &self.system.prime);
                let one_lc = LinearCombination::from_constant(BigUint::one());
                let zero_lc = LinearCombination::new();
                self.system.add_constraint(diff, one_lc, zero_lc);
                Ok(())
            }
            Statement::AssertBool(name) => {
                self.enforce_boolean(name)?;
                Ok(())
            }
            Statement::Conditional {
                condition,
                then_stmt,
                else_stmt,
            } => {
                self.enforce_boolean(condition)?;

                let cond_idx = *self.signal_map.get(condition).ok_or_else(|| {
                    CompileError::R1csError {
                        message: format!("undefined signal: {}", condition),
                    }
                })?;
                let cond_lc = LinearCombination::from_var(cond_idx);

                match &**then_stmt {
                    Statement::Assign { target, value } => {
                        let target_idx = *self.signal_map.get(target).ok_or_else(|| {
                            CompileError::R1csError {
                                message: format!("undefined signal: {}", target),
                            }
                        })?;
                        let lc_value = self.flatten_expr(value)?;
                        let lc_target = LinearCombination::from_var(target_idx);
                        let diff = lc_value.sub(&lc_target, &self.system.prime);
                        let zero_lc = LinearCombination::new();
                        self.system.add_constraint(diff, cond_lc.clone(), zero_lc);
                    }
                    Statement::Constraint { lhs, rhs } => {
                        let lc_lhs = self.flatten_expr(lhs)?;
                        let lc_rhs = self.flatten_expr(rhs)?;
                        let diff = lc_lhs.sub(&lc_rhs, &self.system.prime);
                        let zero_lc = LinearCombination::new();
                        self.system.add_constraint(diff, cond_lc.clone(), zero_lc);
                    }
                    _ => {
                        return Err(CompileError::R1csError {
                            message: "conditional body must be an assignment or constraint"
                                .to_string(),
                        });
                    }
                }

                if let Some(else_s) = else_stmt {
                    let one_lc = LinearCombination::from_constant(BigUint::one());
                    let neg_cond = one_lc.sub(&cond_lc, &self.system.prime);

                    match &**else_s {
                        Statement::Assign { target, value } => {
                            let target_idx = *self.signal_map.get(target).ok_or_else(|| {
                                CompileError::R1csError {
                                    message: format!("undefined signal: {}", target),
                                }
                            })?;
                            let lc_value = self.flatten_expr(value)?;
                            let lc_target = LinearCombination::from_var(target_idx);
                            let diff = lc_value.sub(&lc_target, &self.system.prime);
                            let zero_lc = LinearCombination::new();
                            self.system.add_constraint(diff, neg_cond, zero_lc);
                        }
                        Statement::Constraint { lhs, rhs } => {
                            let lc_lhs = self.flatten_expr(lhs)?;
                            let lc_rhs = self.flatten_expr(rhs)?;
                            let diff = lc_lhs.sub(&lc_rhs, &self.system.prime);
                            let zero_lc = LinearCombination::new();
                            self.system.add_constraint(diff, neg_cond, zero_lc);
                        }
                        _ => {
                            return Err(CompileError::R1csError {
                                message: "conditional else body must be an assignment or constraint"
                                    .to_string(),
                            });
                        }
                    }
                }

                Ok(())
            }
        }
    }

    fn flatten_program(&mut self, program: &Program) -> Result<()> {
        for stmt in &program.statements {
            self.flatten_statement(stmt)?;
        }
        Ok(())
    }
}

pub fn flatten(program: &Program, prime: BigUint) -> Result<R1csSystem> {
    let mut flattener = Flattener::new(prime);
    flattener.flatten_program(program)?;
    Ok(flattener.system)
}
