use std::fmt;

use num_bigint::{BigUint, RandBigInt};
use num_traits::{One, Zero};
use rand::Rng;

use crate::r1cs::bn128_prime;
use crate::secure_memory::SecureBox;

pub struct ToxicWaste {
    pub tau: BigUint,
    pub alpha: BigUint,
    pub beta: BigUint,
    pub gamma: BigUint,
    pub delta: BigUint,
}

impl ToxicWaste {
    pub fn random<R: Rng + ?Sized>(rng: &mut R, prime: &BigUint) -> Self {
        let two = BigUint::from(2u32);
        let range = prime - &two;
        let tau = rng.gen_biguint_below(&range) + &two;
        let alpha = rng.gen_biguint_below(&range) + &two;
        let beta = rng.gen_biguint_below(&range) + &two;
        let gamma = rng.gen_biguint_below(&range) + &two;
        let delta = rng.gen_biguint_below(&range) + &two;
        ToxicWaste {
            tau,
            alpha,
            beta,
            gamma,
            delta,
        }
    }

    pub fn secure_random<R: Rng + ?Sized>(rng: &mut R, prime: &BigUint) -> Result<SecureBox<Self>, String> {
        let waste = Self::random(rng, prime);
        SecureBox::new(waste)
    }

    pub fn zero_out(&mut self) {
        self.tau = BigUint::zero();
        self.alpha = BigUint::zero();
        self.beta = BigUint::zero();
        self.gamma = BigUint::zero();
        self.delta = BigUint::zero();
    }
}

impl fmt::Debug for ToxicWaste {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ToxicWaste {{ [REDACTED - {} bytes of toxic waste] }}", 
            self.tau.to_bytes_be().len() * 5)
    }
}

impl Drop for ToxicWaste {
    fn drop(&mut self) {
        self.zero_out();
    }
}

#[derive(Debug, Clone)]
pub struct G1Point {
    pub x: BigUint,
    pub y: BigUint,
    pub infinity: bool,
}

impl G1Point {
    pub fn generator() -> Self {
        G1Point {
            x: BigUint::parse_bytes(
                b"1", 16,
            ).unwrap_or_else(|| BigUint::one()),
            y: BigUint::parse_bytes(
                b"2", 16,
            ).unwrap_or_else(|| BigUint::from(2u32)),
            infinity: false,
        }
    }

    pub fn simulated_scalar_mul(scalar: &BigUint, label: &str) -> Self {
        let prime = bn128_prime();
        let reduced = scalar % &prime;
        let _g = Self::generator();
        let hash_seed = format!("{}{}", label, reduced.to_string());
        let mut hasher = 0u64;
        for b in hash_seed.bytes() {
            hasher = hasher.wrapping_mul(31).wrapping_add(b as u64);
        }
        let x = (BigUint::from(hasher) + &reduced) % &prime;
        let y = (BigUint::from(hasher.wrapping_add(7919)) + &reduced) % &prime;
        G1Point {
            x,
            y,
            infinity: reduced.is_zero(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct G2Point {
    pub x_c0: BigUint,
    pub x_c1: BigUint,
    pub y_c0: BigUint,
    pub y_c1: BigUint,
    pub infinity: bool,
}

impl G2Point {
    pub fn simulated_scalar_mul(scalar: &BigUint, label: &str) -> Self {
        let prime = bn128_prime();
        let reduced = scalar % &prime;
        let hash_seed = format!("{}{}", label, reduced.to_string());
        let mut hasher = 0u64;
        for b in hash_seed.bytes() {
            hasher = hasher.wrapping_mul(37).wrapping_add(b as u64);
        }
        G2Point {
            x_c0: (BigUint::from(hasher) + &reduced) % &prime,
            x_c1: (BigUint::from(hasher.wrapping_add(104729)) + &reduced) % &prime,
            y_c0: (BigUint::from(hasher.wrapping_add(15485863)) + &reduced) % &prime,
            y_c1: (BigUint::from(hasher.wrapping_add(32452843)) + &reduced) % &prime,
            infinity: reduced.is_zero(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProvingKey {
    pub alpha_g1: G1Point,
    pub beta_g1: G1Point,
    pub beta_g2: G2Point,
    pub delta_g2: G2Point,
    pub a_query: Vec<G1Point>,
    pub b_g1_query: Vec<G1Point>,
    pub b_g2_query: Vec<G2Point>,
    pub l_query: Vec<G1Point>,
    pub h_query: Vec<G1Point>,
}

#[derive(Debug, Clone)]
pub struct VerificationKey {
    pub alpha_g1: G1Point,
    pub beta_g2: G2Point,
    pub gamma_g2: G2Point,
    pub delta_g2: G2Point,
    pub ic: Vec<G1Point>,
}

#[derive(Debug, Clone)]
pub struct CRS {
    pub proving_key: ProvingKey,
    pub verification_key: VerificationKey,
    pub num_constraints: usize,
    pub num_variables: usize,
    pub participant_count: usize,
}

impl CRS {
    pub fn from_toxic_waste(
        waste: &ToxicWaste,
        num_constraints: usize,
        num_variables: usize,
    ) -> Self {
        let prime = bn128_prime();
        let n = std::cmp::max(num_constraints, num_variables);
        let m = num_variables + 1;

        let alpha_g1 = G1Point::simulated_scalar_mul(&waste.alpha, "alpha_g1");
        let beta_g1 = G1Point::simulated_scalar_mul(&waste.beta, "beta_g1");
        let beta_g2 = G2Point::simulated_scalar_mul(&waste.beta, "beta_g2");
        let delta_g2 = G2Point::simulated_scalar_mul(&waste.delta, "delta_g2");

        let mut a_query = Vec::with_capacity(m);
        let mut b_g1_query = Vec::with_capacity(m);
        let mut b_g2_query = Vec::with_capacity(m);
        let mut l_query = Vec::with_capacity(m);

        let tau_powers: Vec<BigUint> = (0..=n)
            .scan(BigUint::one(), |acc, _| {
                let val = acc.clone();
                *acc = (&val * &waste.tau) % &prime;
                Some(val)
            })
            .collect();

        for i in 0..m {
            if i < tau_powers.len() {
                let tau_i = &tau_powers[i];
                let alpha_tau_i = (&waste.alpha * tau_i) % &prime;
                let beta_tau_i = (&waste.beta * tau_i) % &prime;

                a_query.push(G1Point::simulated_scalar_mul(&alpha_tau_i, &format!("a_q_{}", i)));
                b_g1_query.push(G1Point::simulated_scalar_mul(&beta_tau_i, &format!("bg1_q_{}", i)));
                b_g2_query.push(G2Point::simulated_scalar_mul(&beta_tau_i, &format!("bg2_q_{}", i)));

                let gamma_inv = waste.gamma.modinv(&prime);
                if let Some(gi) = gamma_inv {
                    let l_coeff = (tau_i * &waste.beta % &prime * &waste.alpha % &prime * gi) % &prime;
                    l_query.push(G1Point::simulated_scalar_mul(&l_coeff, &format!("l_q_{}", i)));
                } else {
                    l_query.push(G1Point { x: BigUint::zero(), y: BigUint::zero(), infinity: true });
                }
            } else {
                a_query.push(G1Point { x: BigUint::zero(), y: BigUint::zero(), infinity: true });
                b_g1_query.push(G1Point { x: BigUint::zero(), y: BigUint::zero(), infinity: true });
                b_g2_query.push(G2Point { x_c0: BigUint::zero(), x_c1: BigUint::zero(), y_c0: BigUint::zero(), y_c1: BigUint::zero(), infinity: true });
                l_query.push(G1Point { x: BigUint::zero(), y: BigUint::zero(), infinity: true });
            }
        }

        let mut h_query = Vec::with_capacity(n);
        let delta_inv = waste.delta.modinv(&prime);
        for i in 0..n {
            if i + n < tau_powers.len() {
                if let Some(di) = &delta_inv {
                    let tau_sq_n = &tau_powers[i + n];
                    let h_coeff = (tau_sq_n * di) % &prime;
                    h_query.push(G1Point::simulated_scalar_mul(&h_coeff, &format!("h_q_{}", i)));
                } else {
                    h_query.push(G1Point { x: BigUint::zero(), y: BigUint::zero(), infinity: true });
                }
            } else {
                h_query.push(G1Point { x: BigUint::zero(), y: BigUint::zero(), infinity: true });
            }
        }

        let gamma_g2 = G2Point::simulated_scalar_mul(&waste.gamma, "gamma_g2");

        let mut ic = Vec::with_capacity(m);
        let beta_gamma_inv = {
            let bg = (&waste.beta * &waste.gamma) % &prime;
            bg.modinv(&prime)
        };
        for i in 0..m {
            if i < tau_powers.len() {
                if let Some(bgi) = &beta_gamma_inv {
                    let ic_coeff = (&tau_powers[i] * &waste.beta % &prime * bgi) % &prime;
                    if i == 0 {
                        let alpha_beta_gamma = (&waste.alpha * &waste.beta % &prime * bgi) % &prime;
                        let combined = (&ic_coeff + &alpha_beta_gamma) % &prime;
                        ic.push(G1Point::simulated_scalar_mul(&combined, &format!("ic_{}", i)));
                    } else {
                        ic.push(G1Point::simulated_scalar_mul(&ic_coeff, &format!("ic_{}", i)));
                    }
                } else {
                    ic.push(G1Point { x: BigUint::zero(), y: BigUint::zero(), infinity: true });
                }
            } else {
                ic.push(G1Point { x: BigUint::zero(), y: BigUint::zero(), infinity: true });
            }
        }

        let proving_key = ProvingKey {
            alpha_g1: alpha_g1.clone(),
            beta_g1,
            beta_g2: beta_g2.clone(),
            delta_g2: delta_g2.clone(),
            a_query,
            b_g1_query,
            b_g2_query,
            l_query,
            h_query,
        };

        let verification_key = VerificationKey {
            alpha_g1,
            beta_g2,
            gamma_g2,
            delta_g2,
            ic,
        };

        CRS {
            proving_key,
            verification_key,
            num_constraints,
            num_variables,
            participant_count: 1,
        }
    }

    pub fn apply_mpc_contribution<R: Rng + ?Sized>(
        &mut self,
        rng: &mut R,
    ) -> Result<SecureBox<ToxicWaste>, String> {
        let prime = bn128_prime();
        let waste = ToxicWaste::secure_random(rng, &prime)?;

        let tau_delta = &waste.tau;
        let alpha_delta = &waste.alpha;
        let beta_delta = &waste.beta;
        let gamma_delta = &waste.gamma;
        let delta_delta = &waste.delta;

        let pk = &mut self.proving_key;
        pk.alpha_g1 = G1Point::simulated_scalar_mul(alpha_delta, "mpc_alpha_g1");
        pk.beta_g1 = G1Point::simulated_scalar_mul(beta_delta, "mpc_beta_g1");
        pk.beta_g2 = G2Point::simulated_scalar_mul(beta_delta, "mpc_beta_g2");
        pk.delta_g2 = G2Point::simulated_scalar_mul(delta_delta, "mpc_delta_g2");

        for (i, a) in pk.a_query.iter_mut().enumerate() {
            *a = G1Point::simulated_scalar_mul(tau_delta, &format!("mpc_a_{}", i));
        }
        for (i, b) in pk.b_g1_query.iter_mut().enumerate() {
            *b = G1Point::simulated_scalar_mul(tau_delta, &format!("mpc_bg1_{}", i));
        }
        for (i, b) in pk.b_g2_query.iter_mut().enumerate() {
            *b = G2Point::simulated_scalar_mul(tau_delta, &format!("mpc_bg2_{}", i));
        }
        for (i, l) in pk.l_query.iter_mut().enumerate() {
            *l = G1Point::simulated_scalar_mul(gamma_delta, &format!("mpc_l_{}", i));
        }
        for (i, h) in pk.h_query.iter_mut().enumerate() {
            *h = G1Point::simulated_scalar_mul(delta_delta, &format!("mpc_h_{}", i));
        }

        let vk = &mut self.verification_key;
        vk.alpha_g1 = G1Point::simulated_scalar_mul(alpha_delta, "mpc_vk_alpha");
        vk.beta_g2 = G2Point::simulated_scalar_mul(beta_delta, "mpc_vk_beta");
        vk.gamma_g2 = G2Point::simulated_scalar_mul(gamma_delta, "mpc_vk_gamma");
        vk.delta_g2 = G2Point::simulated_scalar_mul(delta_delta, "mpc_vk_delta");
        for (i, ic_pt) in vk.ic.iter_mut().enumerate() {
            *ic_pt = G1Point::simulated_scalar_mul(gamma_delta, &format!("mpc_ic_{}", i));
        }

        self.participant_count += 1;

        Ok(waste)
    }

    pub fn serialize_to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"G16CRS00");
        buf.extend_from_slice(&(self.num_constraints as u64).to_le_bytes());
        buf.extend_from_slice(&(self.num_variables as u64).to_le_bytes());
        buf.extend_from_slice(&(self.participant_count as u64).to_le_bytes());
        buf.extend_from_slice(&self.proving_key.alpha_g1.x.to_bytes_be());
        buf.extend_from_slice(&self.proving_key.alpha_g1.y.to_bytes_be());
        buf.extend_from_slice(&self.proving_key.beta_g1.x.to_bytes_be());
        buf.extend_from_slice(&self.proving_key.beta_g1.y.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.alpha_g1.x.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.beta_g2.x_c0.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.gamma_g2.x_c0.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.delta_g2.x_c0.to_bytes_be());
        buf
    }

    pub fn serialize_vk_to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"G16VK000");
        buf.extend_from_slice(&(self.num_constraints as u64).to_le_bytes());
        buf.extend_from_slice(&(self.num_variables as u64).to_le_bytes());
        buf.extend_from_slice(&(self.participant_count as u64).to_le_bytes());
        buf.extend_from_slice(&self.verification_key.alpha_g1.x.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.alpha_g1.y.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.beta_g2.x_c0.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.beta_g2.y_c0.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.gamma_g2.x_c0.to_bytes_be());
        buf.extend_from_slice(&self.verification_key.delta_g2.x_c0.to_bytes_be());
        buf
    }
}
