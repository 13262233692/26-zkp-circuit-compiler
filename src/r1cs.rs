use num_bigint::BigUint;
use num_traits::{One, Zero};
use std::collections::BTreeMap;

pub const BN128_PRIME_HEX: &str =
    "21888242871839275222246405745257275088548364400416034343698204186575808495617";

pub fn bn128_prime() -> BigUint {
    BN128_PRIME_HEX.parse::<BigUint>().unwrap()
}

#[derive(Debug, Clone)]
pub struct LinearCombination {
    pub terms: BTreeMap<usize, BigUint>,
}

impl LinearCombination {
    pub fn new() -> Self {
        LinearCombination {
            terms: BTreeMap::new(),
        }
    }

    pub fn from_constant(val: BigUint) -> Self {
        let mut lc = LinearCombination::new();
        if !val.is_zero() {
            lc.terms.insert(0, val);
        }
        lc
    }

    pub fn from_var(idx: usize) -> Self {
        let mut lc = LinearCombination::new();
        lc.terms.insert(idx, BigUint::one());
        lc
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty() || self.terms.values().all(|v| v.is_zero())
    }

    pub fn add(&self, other: &LinearCombination, prime: &BigUint) -> LinearCombination {
        let mut result = self.terms.clone();
        for (idx, coeff) in &other.terms {
            let entry = result.entry(*idx).or_insert_with(BigUint::zero);
            *entry = (&*entry + coeff) % prime;
            if entry.is_zero() {
                result.remove(idx);
            }
        }
        LinearCombination { terms: result }
    }

    pub fn sub(&self, other: &LinearCombination, prime: &BigUint) -> LinearCombination {
        let mut neg_other = other.terms.clone();
        for (_, coeff) in neg_other.iter_mut() {
            if !coeff.is_zero() {
                *coeff = prime - &*coeff;
            }
        }
        self.add(&LinearCombination { terms: neg_other }, prime)
    }

    pub fn scale(&self, scalar: &BigUint, prime: &BigUint) -> LinearCombination {
        let mut result = BTreeMap::new();
        for (idx, coeff) in &self.terms {
            let new_coeff = (coeff * scalar) % prime;
            if !new_coeff.is_zero() {
                result.insert(*idx, new_coeff);
            }
        }
        LinearCombination { terms: result }
    }
}

impl std::fmt::Display for LinearCombination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.terms.is_empty() {
            return write!(f, "0");
        }
        let parts: Vec<String> = self
            .terms
            .iter()
            .map(|(idx, coeff)| {
                if *idx == 0 {
                    format!("{}", coeff)
                } else {
                    format!("{}*v{}", coeff, idx)
                }
            })
            .collect();
        write!(f, "{}", parts.join(" + "))
    }
}

#[derive(Debug, Clone)]
pub struct R1csConstraint {
    pub a: LinearCombination,
    pub b: LinearCombination,
    pub c: LinearCombination,
}

impl std::fmt::Display for R1csConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}) * ({}) = ({})", self.a, self.b, self.c)
    }
}

#[derive(Debug, Clone)]
pub struct R1csSystem {
    pub prime: BigUint,
    pub num_variables: usize,
    pub num_public_inputs: usize,
    pub num_public_outputs: usize,
    pub num_private_inputs: usize,
    pub constraints: Vec<R1csConstraint>,
    pub variable_names: Vec<String>,
}

impl R1csSystem {
    pub fn new(prime: BigUint) -> Self {
        R1csSystem {
            prime,
            num_variables: 1,
            num_public_inputs: 0,
            num_public_outputs: 0,
            num_private_inputs: 0,
            constraints: Vec::new(),
            variable_names: vec!["~one".to_string()],
        }
    }

    pub fn allocate_variable(&mut self, name: &str) -> usize {
        let idx = self.num_variables;
        self.num_variables += 1;
        self.variable_names.push(name.to_string());
        idx
    }

    pub fn add_constraint(&mut self, a: LinearCombination, b: LinearCombination, c: LinearCombination) {
        self.constraints.push(R1csConstraint { a, b, c });
    }

    pub fn display_summary(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("=== R1CS System Summary ===\n"));
        s.push_str(&format!("Field prime: {}\n", self.prime));
        s.push_str(&format!("Total variables: {} (including ~one)\n", self.num_variables));
        s.push_str(&format!("  Public inputs:  {}\n", self.num_public_inputs));
        s.push_str(&format!("  Public outputs: {}\n", self.num_public_outputs));
        s.push_str(&format!("  Private inputs: {}\n", self.num_private_inputs));
        s.push_str(&format!("  Intermediate:   {}\n",
            self.num_variables - 1 - self.num_public_inputs - self.num_public_outputs - self.num_private_inputs
        ));
        s.push_str(&format!("Total constraints: {}\n", self.constraints.len()));
        s.push_str(&format!("\n--- Variable Map ---\n"));
        for (i, name) in self.variable_names.iter().enumerate() {
            s.push_str(&format!("  v{} = {}\n", i, name));
        }
        s.push_str(&format!("\n--- Constraints ---\n"));
        for (i, c) in self.constraints.iter().enumerate() {
            s.push_str(&format!("  #{}: {}\n", i, c));
        }
        s
    }
}
