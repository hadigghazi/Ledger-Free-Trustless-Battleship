//! Library surface for the ZK Battleship host: proving, verifying, board
//! generation, and player/session state. Used by the local prover agent
//! (`zk-battleship-agent`) and by the integration tests / benchmarks.
//!
//! There is deliberately NO development or "fast" mode anywhere in this
//! crate: every receipt is a genuine STARK. The agent binary additionally
//! scrubs `RISC0_DEV_MODE` from its environment at startup so the RISC Zero
//! prover cannot be switched into mock proving underneath us.

pub mod board_gen;
pub mod game;
pub mod prover;
pub mod transport;
pub mod verifier;
