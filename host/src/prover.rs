//! Proving side: runs the zkVM to produce receipts.

use anyhow::Result;
use methods::{COMMIT_BOARD_ELF, PROVE_SHOT_ELF};
use risc0_zkvm::{default_prover, ExecutorEnv, Receipt};
use zk_battleship_core::{CommitInput, ShotInput};

/// Prove "I committed to a valid board with commitment C".
pub fn prove_commit(input: &CommitInput) -> Result<Receipt> {
    let env = ExecutorEnv::builder().write(input)?.build()?;
    let receipt = default_prover().prove(env, COMMIT_BOARD_ELF)?.receipt;
    Ok(receipt)
}

/// Prove "for board C, shot (x,y) is hit/miss (and sinks ship Z)".
///
/// If `input.fleet`+`input.salt` do not hash to `input.commitment`, the guest
/// asserts and proving FAILS — there is no receipt for a lie. That returned
/// `Err` is the protocol rejecting a cheat attempt.
pub fn prove_shot(input: &ShotInput) -> Result<Receipt> {
    let env = ExecutorEnv::builder().write(input)?.build()?;
    let receipt = default_prover().prove(env, PROVE_SHOT_ELF)?.receipt;
    Ok(receipt)
}
