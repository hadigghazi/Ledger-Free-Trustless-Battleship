//! End-to-end protocol tests against the REAL prover (no dev mode exists in
//! this build). These are the security argument in executable form: every
//! cheating strategy the threat model considers must fail here.
//!
//! Real STARK proving takes seconds-to-minutes per proof, so these tests are
//! `#[ignore]`d by default. Run them explicitly:
//!
//!     cargo test --release -p host -- --ignored --test-threads=1

use rand::rngs::StdRng;
use rand::SeedableRng;
use zk_battleship_core::{Shot, ShotInput};

use host::board_gen::{random_fleet, random_salt};
use host::game::{host_commitment, Player};
use host::prover;
use host::verifier::{verify_commit, verify_shot};

fn player(seed: u64) -> Player {
    let mut rng = StdRng::seed_from_u64(seed);
    Player::new(random_fleet(&mut rng), random_salt(&mut rng))
}

#[test]
#[ignore = "runs the real prover"]
fn commit_proof_roundtrip() {
    let p = player(1);
    let receipt = p.prove_commit().expect("honest commit must prove");
    let c = verify_commit(&receipt).expect("honest commit must verify");
    assert_eq!(c, p.commitment, "journal commitment == host-side commitment");
}

#[test]
#[ignore = "runs the real prover"]
fn illegal_fleet_cannot_commit() {
    // Two ships stacked on the same cells: is_valid() is false, so the guest
    // asserts and NO receipt can exist for an illegal board.
    let mut rng = StdRng::seed_from_u64(2);
    let mut fleet = random_fleet(&mut rng);
    fleet.placements[4].row = fleet.placements[0].row;
    fleet.placements[4].col = fleet.placements[0].col;
    fleet.placements[4].orientation = fleet.placements[0].orientation;
    assert!(!fleet.is_valid());
    let p = Player::new(fleet, random_salt(&mut rng));
    assert!(p.prove_commit().is_err(), "illegal fleet must not prove");
}

#[test]
#[ignore = "runs the real prover"]
fn honest_shots_verify_and_bind() {
    let mut p = player(3);
    let commitment = p.commitment;

    // Find one occupied and one empty cell.
    let occupied = p.fleet().placements[0].cells()[0];
    let mut empty = (0u8, 0u8);
    'outer: for r in 0..10u8 {
        for c in 0..10u8 {
            if !p.fleet().occupies(Shot::new(r, c)) {
                empty = (r, c);
                break 'outer;
            }
        }
    }

    // Miss with empty history.
    let miss = Shot::new(empty.0, empty.1);
    let (receipt, journal) = p.respond(miss).expect("honest response proves");
    let j = verify_shot(&receipt, commitment, miss, &[]).expect("honest response verifies");
    assert_eq!(j, journal);
    assert!(!j.hit);
    assert!(!j.defeated);
    assert!(j.history.is_empty());

    // Hit with one-element history; the verifier must supply that history.
    let hit = Shot::new(occupied.0, occupied.1);
    let (receipt, _) = p.respond(hit).expect("honest response proves");
    let j = verify_shot(&receipt, commitment, hit, &[miss]).expect("honest response verifies");
    assert!(j.hit);
    assert_eq!(j.history, vec![miss]);

    // The same receipt must NOT verify against a wrong shot (replay), a wrong
    // history (fabrication), or a wrong commitment (board swap on the
    // verifier side).
    assert!(verify_shot(&receipt, commitment, miss, &[miss]).is_err(), "replayed shot must fail");
    assert!(verify_shot(&receipt, commitment, hit, &[]).is_err(), "wrong history must fail");
    assert!(verify_shot(&receipt, [0u8; 32], hit, &[miss]).is_err(), "wrong commitment must fail");

    // A tampered journal must fail cryptographic verification.
    let mut forged = receipt.clone();
    forged.journal.bytes[0] ^= 1;
    assert!(
        verify_shot(&forged, commitment, hit, &[miss]).is_err(),
        "tampered journal must fail"
    );
}

#[test]
#[ignore = "runs the real prover"]
fn board_swap_cannot_prove() {
    // The classic cheat: answer with a DIFFERENT board while claiming the
    // original commitment. The guest's binding assertion fails, so the
    // prover cannot produce any receipt at all.
    let mut rng = StdRng::seed_from_u64(4);
    let real_fleet = random_fleet(&mut rng);
    let salt = random_salt(&mut rng);
    let commitment = host_commitment(&real_fleet, &salt);

    let lie_fleet = random_fleet(&mut rng);
    assert_ne!(lie_fleet, real_fleet);

    let input = ShotInput {
        fleet: lie_fleet,
        salt,
        commitment, // still the ORIGINAL commitment
        shot: Shot::new(0, 0),
        history: vec![],
    };
    assert!(
        prover::prove_shot(&input).is_err(),
        "a lie has no proof: board-swap must not produce a receipt"
    );
}

#[test]
#[ignore = "runs the real prover"]
fn defeat_is_proven_not_claimed() {
    // With 16 of 17 ship cells already in the public history, the shot at the
    // final cell must yield a journal with defeated = true — and one cell
    // earlier it must not. The loss itself is a proven statement.
    let mut rng = StdRng::seed_from_u64(5);
    let fleet = random_fleet(&mut rng);
    let salt = random_salt(&mut rng);
    let commitment = host_commitment(&fleet, &salt);

    let mut cells: Vec<Shot> = Vec::new();
    for pl in fleet.placements.iter() {
        for (r, c) in pl.cells() {
            cells.push(Shot::new(r, c));
        }
    }
    let (last, prior) = cells.split_last().unwrap();

    // Second-to-last hit: 15 cells of history, shot at cell 16 -> not defeated.
    let (pre_last, pre_prior) = prior.split_last().unwrap();
    let receipt = prover::prove_shot(&ShotInput {
        fleet: fleet.clone(),
        salt,
        commitment,
        shot: *pre_last,
        history: pre_prior.to_vec(),
    })
    .expect("honest response proves");
    let j = verify_shot(&receipt, commitment, *pre_last, pre_prior).expect("verifies");
    assert!(j.hit);
    assert!(!j.defeated, "not defeated while a ship cell survives");

    // Final hit: 16 cells of history, shot at the 17th -> defeated.
    let receipt = prover::prove_shot(&ShotInput {
        fleet: fleet.clone(),
        salt,
        commitment,
        shot: *last,
        history: prior.to_vec(),
    })
    .expect("honest response proves");
    let j = verify_shot(&receipt, commitment, *last, prior).expect("verifies");
    assert!(j.hit);
    assert!(j.defeated, "the final hit must prove defeat");
    assert!(j.sunk.is_some(), "the final hit sinks the last ship");
}
