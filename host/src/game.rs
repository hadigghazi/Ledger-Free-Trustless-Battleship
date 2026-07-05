//! Game state and orchestration helpers.

use anyhow::Result;
use rand::Rng;
use risc0_zkvm::Receipt;
use sha2::{Digest as _, Sha256};
use zk_battleship_core::{CommitInput, Fleet, Shot, ShotInput, ShotJournal, BOARD_SIZE};

use crate::prover;

/// Host-side SHA-256 of the commitment preimage. Uses the `sha2` crate; the
/// guest uses RISC Zero's accelerated SHA-256. Both hash the SAME bytes
/// (`fleet.digest_preimage(salt)`), so both yield the same 32-byte digest.
pub fn host_commitment(fleet: &Fleet, salt: &[u8; 32]) -> [u8; 32] {
    let preimage = fleet.digest_preimage(salt);
    let out = Sha256::digest(&preimage);
    let mut c = [0u8; 32];
    c.copy_from_slice(&out);
    c
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// One player: holds the secret board + salt, the public commitment, and the
/// running history of shots fired *at* this player.
pub struct Player {
    fleet: Fleet,
    salt: [u8; 32],
    pub commitment: [u8; 32],
    incoming: Vec<Shot>,
}

impl Player {
    pub fn new(fleet: Fleet, salt: [u8; 32]) -> Self {
        let commitment = host_commitment(&fleet, &salt);
        Player { fleet, salt, commitment, incoming: Vec::new() }
    }

    /// Prove the board commitment (run once, at setup).
    pub fn prove_commit(&self) -> Result<Receipt> {
        prover::prove_commit(&CommitInput {
            fleet: self.fleet.clone(),
            salt: self.salt,
        })
    }

    /// True if this player has already answered a shot at this cell. The
    /// protocol requires distinct shots (each cell may be targeted at most
    /// once per board), which keeps the two sides' history sequences in
    /// lockstep.
    pub fn already_answered(&self, shot: Shot) -> bool {
        self.incoming.contains(&shot)
    }

    /// Honestly answer an incoming shot with a zk proof, then record it.
    pub fn respond(&mut self, shot: Shot) -> Result<(Receipt, ShotJournal)> {
        let input = ShotInput {
            fleet: self.fleet.clone(),
            salt: self.salt,
            commitment: self.commitment,
            shot,
            history: self.incoming.clone(),
        };
        let receipt = prover::prove_shot(&input)?;
        let journal: ShotJournal = receipt.journal.decode()?;
        self.incoming.push(shot);
        Ok((receipt, journal))
    }

    pub fn fleet(&self) -> &Fleet {
        &self.fleet
    }
    pub fn salt(&self) -> [u8; 32] {
        self.salt
    }

    /// True once every cell of this player's fleet has been hit by the shots
    /// fired at them so far. (The *proven* form of this fact is the
    /// `defeated` flag in each shot journal.)
    pub fn is_defeated(&self) -> bool {
        self.fleet.all_sunk(&self.incoming)
    }
}

/// Simple but effective "hunt / target" gunner: fire randomly until a hit,
/// then sweep the orthogonal neighbours of any hit before resuming the hunt.
pub struct Gunner {
    tried: Vec<Shot>,
    queue: Vec<Shot>,
}

impl Gunner {
    pub fn new() -> Self {
        Gunner { tried: Vec::new(), queue: Vec::new() }
    }

    pub fn next<R: Rng>(&mut self, rng: &mut R) -> Shot {
        while let Some(s) = self.queue.pop() {
            if !self.tried.contains(&s) {
                return s;
            }
        }
        loop {
            let s = Shot::new(rng.gen_range(0..BOARD_SIZE), rng.gen_range(0..BOARD_SIZE));
            if !self.tried.contains(&s) {
                return s;
            }
        }
    }

    pub fn record(&mut self, shot: Shot, hit: bool) {
        self.tried.push(shot);
        if hit {
            let deltas: [(i8, i8); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
            for (dr, dc) in deltas {
                let nr = shot.row as i8 + dr;
                let nc = shot.col as i8 + dc;
                if (0..BOARD_SIZE as i8).contains(&nr) && (0..BOARD_SIZE as i8).contains(&nc) {
                    let s = Shot::new(nr as u8, nc as u8);
                    if !self.tried.contains(&s) {
                        self.queue.push(s);
                    }
                }
            }
        }
    }
}

impl Default for Gunner {
    fn default() -> Self {
        Self::new()
    }
}
