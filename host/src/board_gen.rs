//! Random board + salt generation for demos and quick play.

use rand::Rng;
use zk_battleship_core::{Fleet, Orientation, Placement, ShipKind, BOARD_SIZE, NUM_SHIPS};

/// A fresh 256-bit random salt.
pub fn random_salt<R: Rng>(rng: &mut R) -> [u8; 32] {
    let mut salt = [0u8; 32];
    rng.fill(&mut salt);
    salt
}

/// A uniformly-placed, valid fleet (one of each ship, on-board, no overlap).
/// Ships are placed one at a time with rejection; the resulting fleet is
/// always `is_valid()`.
pub fn random_fleet<R: Rng>(rng: &mut R) -> Fleet {
    loop {
        if let Some(fleet) = try_random_fleet(rng) {
            debug_assert!(fleet.is_valid());
            return fleet;
        }
    }
}

fn try_random_fleet<R: Rng>(rng: &mut R) -> Option<Fleet> {
    let mut occupied: Vec<(u8, u8)> = Vec::new();
    let mut placements: Vec<Placement> = Vec::with_capacity(NUM_SHIPS);

    for ship in ShipKind::all() {
        let mut placed = false;
        // Bounded attempts per ship before we give up and restart the fleet.
        for _ in 0..200 {
            let orientation = if rng.gen_bool(0.5) {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            };
            let row = rng.gen_range(0..BOARD_SIZE);
            let col = rng.gen_range(0..BOARD_SIZE);
            let candidate = Placement { ship, row, col, orientation };

            if !candidate.in_bounds() {
                continue;
            }
            let cells = candidate.cells();
            if cells.iter().any(|c| occupied.contains(c)) {
                continue;
            }
            occupied.extend(cells);
            placements.push(candidate);
            placed = true;
            break;
        }
        if !placed {
            return None;
        }
    }

    let arr: [Placement; NUM_SHIPS] = placements.try_into().ok()?;
    Some(Fleet { placements: arr })
}
