//! Public, serializable messages exchanged between two players.
//!
//! These are the *only* things that travel between opponents (carried by the
//! relay for online play): a board commitment with its proof, and a
//! proof-backed answer to a shot. Receipts implement `Serialize`, so both
//! round-trip cleanly through JSON. Secret boards/salts never appear here.

use risc0_zkvm::Receipt;
use serde::{Deserialize, Serialize};

/// Published once at setup. The opponent verifies `receipt` and stores
/// `commitment`.
#[derive(Serialize, Deserialize)]
pub struct CommitMsg {
    pub commitment: [u8; 32],
    pub receipt: Receipt,
}

/// The proof-backed answer to a shot. The opponent verifies `receipt` against
/// the committed board and the shot that was fired.
#[derive(Serialize, Deserialize)]
pub struct ResponseMsg {
    pub receipt: Receipt,
}
