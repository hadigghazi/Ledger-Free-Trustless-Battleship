// Guest: Board Commitment (Proof 1)
//
// Runs inside the RISC Zero zkVM. Proves, in zero knowledge:
//   "I know a VALID Battleship fleet and a salt whose SHA-256 commitment is C."
//
// Private input : CommitInput { fleet, salt }   (never revealed)
// Public output : CommitJournal { commitment }  (written to the journal)
//
// The opponent verifies this receipt once and stores `commitment`. They now
// trust that C is a legal board, while learning nothing about its layout.

use risc0_zkvm::guest::env;
use risc0_zkvm::sha::{Impl, Sha256};
use zk_battleship_core::{CommitInput, CommitJournal};

fn main() {
    let input: CommitInput = env::read();

    // Reject illegal fleets (wrong ship set, off-board, overlapping).
    // If this fails, the prover cannot produce a receipt at all.
    assert!(input.fleet.is_valid(), "illegal fleet");

    // Commitment = SHA-256(canonical board bytes || salt).
    // SHA-256 is hardware-accelerated inside the zkVM, so this is cheap.
    let preimage = input.fleet.digest_preimage(&input.salt);
    let digest = *Impl::hash_bytes(&preimage);
    let commitment: [u8; 32] = digest
        .as_bytes()
        .try_into()
        .expect("SHA-256 digest is 32 bytes");

    env::commit(&CommitJournal { commitment });
}
