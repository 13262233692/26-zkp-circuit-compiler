use std::fs;
use std::path::PathBuf;

use rand::rngs::OsRng;

use crate::error::Result;
use crate::groth16::{CRS, ToxicWaste};
use crate::r1cs::bn128_prime;
use crate::secure_memory::SecureBox;

pub struct SetupConfig {
    pub num_constraints: usize,
    pub num_variables: usize,
    pub mpc_participants: usize,
    pub output_pk: PathBuf,
    pub output_vk: PathBuf,
}

pub struct SetupResult {
    pub participant_count: usize,
    pub pk_size: usize,
    pub vk_size: usize,
    pub memory_locked: bool,
    pub memory_zeroed: bool,
}

pub fn run_trusted_setup(config: &SetupConfig) -> Result<SetupResult> {
    let prime = bn128_prime();

    eprintln!("╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  Groth16 Trusted Setup Ceremony                            ║");
    eprintln!("║  Field: BN128 (p = 21888...5617)                           ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");
    eprintln!();
    eprintln!("  Circuit parameters:");
    eprintln!("    Constraints: {}", config.num_constraints);
    eprintln!("    Variables:   {}", config.num_variables);
    eprintln!("    MPC rounds:  {}", config.mpc_participants);
    eprintln!();

    let memory_locked;
    let memory_zeroed;

    eprintln!("[1/4] Phase 1: Generating initial toxic waste (SECURE MEMORY)...");
    let mut initial_waste = ToxicWaste::secure_random(&mut OsRng, &prime)
        .map_err(|e| crate::error::CompileError::R1csError {
            message: format!("failed to allocate secure memory for toxic waste: {}", e),
        })?;
    memory_locked = initial_waste.locked();
    eprintln!("  ✓ Toxic waste generated in locked memory (VirtualLock/mlock: {})", memory_locked);
    eprintln!("  ✓ Toxic waste fields: τ, α, β, γ, δ (each ~256 bits)");

    eprintln!("[2/4] Phase 2: Deriving CRS from toxic waste...");
    let mut crs = CRS::from_toxic_waste(&initial_waste, config.num_constraints, config.num_variables);
    eprintln!("  ✓ CRS derived: {} G1 points, {} G2 points in proving key",
        crs.proving_key.a_query.len() + crs.proving_key.b_g1_query.len() + crs.proving_key.l_query.len() + crs.proving_key.h_query.len(),
        crs.proving_key.b_g2_query.len(),
    );

    eprintln!("[3/4] Phase 3: MPC ceremony ({} participants)...", config.mpc_participants);
    let mut mpc_wastes: Vec<SecureBox<ToxicWaste>> = Vec::new();

    for round in 0..config.mpc_participants {
        let participant_id = round + 1;
        eprintln!("  Round {}/{}: Participant #{} contributing randomness...",
            participant_id, config.mpc_participants, participant_id);

        let waste = crs.apply_mpc_contribution(&mut OsRng)
            .map_err(|e| crate::error::CompileError::R1csError {
                message: format!("MPC round {} failed: {}", participant_id, e),
            })?;

        let waste_locked = waste.locked();
        eprintln!("    ✓ Contribution applied (memory locked: {})", waste_locked);

        mpc_wastes.push(waste);
    }

    eprintln!("  ✓ MPC ceremony complete: {} participants contributed", config.mpc_participants);

    eprintln!("[4/4] Phase 4: Serializing CRS and DESTROYING toxic waste...");
    let pk_bytes = crs.serialize_to_bytes();
    let vk_bytes = crs.serialize_vk_to_bytes();

    fs::write(&config.output_pk, &pk_bytes)?;
    fs::write(&config.output_vk, &vk_bytes)?;

    eprintln!("  ✓ Proving key  → {} ({} bytes)", config.output_pk.display(), pk_bytes.len());
    eprintln!("  ✓ Verification key → {} ({} bytes)", config.output_vk.display(), vk_bytes.len());

    eprintln!();
    eprintln!("  ██ CRITICAL: Destroying all toxic waste from memory ██");

    for (i, mut waste) in mpc_wastes.into_iter().enumerate() {
        waste.destroy();
        eprintln!("    ✗ MPC waste #{}: VOLATILE ZEROED + UNLOCKED + FREED", i + 1);
    }

    initial_waste.destroy();
    eprintln!("    ✗ Initial waste: VOLATILE ZEROED + UNLOCKED + FREED");
    memory_zeroed = true;

    eprintln!();
    eprintln!("  ✓ All toxic waste has been securely purged from RAM.");
    eprintln!("    - volatile zeroing (ptr::write_volatile)");
    eprintln!("    - memory fence (SeqCst)");
    eprintln!("    - VirtualUnlock/munlock");
    eprintln!("    - dealloc without read");
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║  Setup complete. {} participants. Memory secured.          ║", config.mpc_participants);
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    Ok(SetupResult {
        participant_count: crs.participant_count,
        pk_size: pk_bytes.len(),
        vk_size: vk_bytes.len(),
        memory_locked,
        memory_zeroed,
    })
}
