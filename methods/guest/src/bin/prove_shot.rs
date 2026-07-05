// Guest: Shot Response (Proof 2)
//
// Runs inside the RISC Zero zkVM. Proves, in zero knowledge:
//   "For the board committed as C, given that the shots in `history` were
//    already fired at me, the new shot at (x,y) is a hit/miss, sinks ship Z
//    (or nothing), and defeats me (or not) -- and this is the SAME board I
//    committed to."
//
// Private input : fleet, salt                              (never revealed)
// Public input  : commitment C, shot (x,y), shot history   (bound into journal)
// Public output : ShotJournal { commitment, shot, hit, sunk, defeated, history }
//
// TWO invariants make the protocol sound without any shared ledger:
//
//   1. The commitment binding below: every shot proof re-derives the
//      commitment from the private board and asserts it equals the C fixed at
//      setup. This locks the player to ONE board for the entire game.
//
//   2. The history binding: the shot history used to compute `sunk` and
//      `defeated` is echoed into the journal, so the verifier (who fired
//      those shots and therefore knows the true history) rejects any proof
//      computed against a fabricated history. Together with `defeated` being
//      computed in-guest, the END of the game is itself a proven statement:
//      a defeated player cannot deny defeat, and a player cannot be falsely
//      declared defeated.

use risc0_zkvm::guest::env;
use risc0_zkvm::sha::{Impl, Sha256};
use zk_battleship_core::{ShotInput, ShotJournal};

fn main() {
    let input: ShotInput = env::read();

    // 1. Recompute the commitment from the private board and bind it to the
    //    one published at setup. (Soundness of the no-cheating claim.)
    let preimage = input.fleet.digest_preimage(&input.salt);
    let digest = *Impl::hash_bytes(&preimage);
    let recomputed: [u8; 32] = digest
        .as_bytes()
        .try_into()
        .expect("SHA-256 digest is 32 bytes");
    assert_eq!(
        recomputed, input.commitment,
        "board does not match the committed board"
    );

    // 2. Honestly compute the result against that same board. All of these
    //    walk fixed-shape loops in `core`, so the cycle count does not depend
    //    on the secret layout.
    let hit = input.fleet.occupies(input.shot);
    let sunk = input.fleet.ship_sunk(input.shot, &input.history);
    let mut full_history = input.history.clone();
    full_history.push(input.shot);
    let defeated = input.fleet.all_sunk(&full_history);

    // 3. Publish the result AND the history it was computed against; the
    //    board stays secret.
    env::commit(&ShotJournal {
        commitment: input.commitment,
        shot: input.shot,
        hit,
        sunk,
        defeated,
        history: input.history,
    });
}
