//! Verifying side: checks receipts and reads the (public) journal.
//!
//! `receipt.verify(IMAGE_ID)` is the cryptographic check: it confirms the
//! seal proves correct execution of exactly the agreed guest binary
//! (identified by its image ID). If either players' binary differed, the
//! image ID would differ and verification would reject.

use anyhow::{bail, Result};
use methods::{COMMIT_BOARD_ID, PROVE_SHOT_ID};
use risc0_zkvm::Receipt;
use zk_battleship_core::{CommitJournal, Shot, ShotJournal};

/// Verify an opponent's commitment proof; return the committed value C.
pub fn verify_commit(receipt: &Receipt) -> Result<[u8; 32]> {
    receipt.verify(COMMIT_BOARD_ID)?;
    let journal: CommitJournal = receipt.journal.decode()?;
    Ok(journal.commitment)
}

/// Verify an opponent's shot response.
///
/// Beyond the cryptographic check we bind the journal to *this* game and
/// *this* turn:
///
///   * `commitment` must be the one fixed at setup — otherwise the proof is
///     about some other board;
///   * `shot` must be the exact coordinate fired this turn — otherwise an
///     old proof could be replayed;
///   * `history` must equal, in order, the shots WE have fired at this
///     opponent so far. The verifier is the one party that knows this
///     sequence with certainty (it chose the shots), so no ledger or third
///     party is needed. This is what makes `sunk` and `defeated` sound: they
///     are functions of the history, and an unbound history would let a
///     cheating responder misreport sunk ships or postpone defeat forever.
pub fn verify_shot(
    receipt: &Receipt,
    expected_commitment: [u8; 32],
    expected_shot: Shot,
    expected_history: &[Shot],
) -> Result<ShotJournal> {
    receipt.verify(PROVE_SHOT_ID)?;
    let journal: ShotJournal = receipt.journal.decode()?;

    if journal.commitment != expected_commitment {
        bail!("proof references a different board than the one committed at setup");
    }
    if journal.shot != expected_shot {
        bail!(
            "proof is for shot {:?}, but {:?} was fired this turn",
            journal.shot,
            expected_shot
        );
    }
    if journal.history != expected_history {
        bail!(
            "proof was computed against a different shot history ({} shots) \
             than the one actually fired ({} shots)",
            journal.history.len(),
            expected_history.len()
        );
    }
    Ok(journal)
}
