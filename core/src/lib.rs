//! Shared, toolchain-independent game logic for ZK Battleship.
//!
//! This crate is compiled into BOTH the host (native) and the guest
//! (RISC-V, inside the zkVM). It deliberately depends only on `serde`
//! so it builds in either target and is the single source of truth for:
//!   * board / fleet / shot data types
//!   * fleet validity rules
//!   * hit detection and "sunk" / "defeated" detection
//!   * the canonical byte preimage that gets hashed into the commitment
//!
//! All of the security-critical logic lives here and is unit tested with a
//! plain `cargo test` (no RISC Zero toolchain required).
//!
//! # Secret-independent execution
//!
//! Everything that touches the private fleet is written with *fixed-shape*
//! control flow: loops run for a fixed number of iterations and results are
//! accumulated with non-short-circuiting boolean operators (`&`, `|`) instead
//! of early exits. Because the standard fleet always occupies exactly 17
//! cells, the zkVM executes (essentially) the same instruction trace for
//! every board, so the *number of cycles* — and therefore proving time —
//! does not leak information about ship positions. See the tests at the
//! bottom, which check these fixed-shape implementations against naive
//! reference implementations.

use serde::{Deserialize, Serialize};

/// Standard 10x10 Battleship board.
pub const BOARD_SIZE: u8 = 10;
/// Number of ships in a standard fleet.
pub const NUM_SHIPS: usize = 5;
/// Total occupied cells across the whole fleet (5+4+3+3+2).
pub const TOTAL_SHIP_CELLS: usize = 17;

/// The five standard ships. The discriminant order also defines the canonical
/// ordering used when building the commitment preimage.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, PartialOrd, Ord)]
pub enum ShipKind {
    Carrier,    // 5
    Battleship, // 4
    Cruiser,    // 3
    Submarine,  // 3
    Destroyer,  // 2
}

impl ShipKind {
    /// Length (number of cells) of the ship.
    pub fn len(self) -> u8 {
        match self {
            ShipKind::Carrier => 5,
            ShipKind::Battleship => 4,
            ShipKind::Cruiser => 3,
            ShipKind::Submarine => 3,
            ShipKind::Destroyer => 2,
        }
    }

    /// Stable numeric id used for canonical encoding / sorting.
    pub fn id(self) -> u8 {
        match self {
            ShipKind::Carrier => 0,
            ShipKind::Battleship => 1,
            ShipKind::Cruiser => 2,
            ShipKind::Submarine => 3,
            ShipKind::Destroyer => 4,
        }
    }

    /// Inverse of [`ShipKind::id`].
    pub fn from_id(id: u8) -> Option<ShipKind> {
        match id {
            0 => Some(ShipKind::Carrier),
            1 => Some(ShipKind::Battleship),
            2 => Some(ShipKind::Cruiser),
            3 => Some(ShipKind::Submarine),
            4 => Some(ShipKind::Destroyer),
            _ => None,
        }
    }

    /// The full standard fleet, one of each kind.
    pub fn all() -> [ShipKind; NUM_SHIPS] {
        [
            ShipKind::Carrier,
            ShipKind::Battleship,
            ShipKind::Cruiser,
            ShipKind::Submarine,
            ShipKind::Destroyer,
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

/// A single ship placed on the board by its top-left cell + orientation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Placement {
    pub ship: ShipKind,
    pub row: u8,
    pub col: u8,
    pub orientation: Orientation,
}

impl Placement {
    /// The i-th cell of this ship (0 <= i < ship.len()).
    ///
    /// Uses wrapping arithmetic so that out-of-bounds placements cannot
    /// panic; such placements are rejected by [`Placement::in_bounds`], and
    /// callers always gate on validity, never on the wrapped coordinates.
    #[inline]
    pub fn cell(&self, i: u8) -> (u8, u8) {
        match self.orientation {
            Orientation::Horizontal => (self.row, self.col.wrapping_add(i)),
            Orientation::Vertical => (self.row.wrapping_add(i), self.col),
        }
    }

    /// The cells this ship occupies. At most 5 cells, so a small Vec is fine.
    pub fn cells(&self) -> Vec<(u8, u8)> {
        (0..self.ship.len()).map(|i| self.cell(i)).collect()
    }

    /// True iff every cell of the ship lies inside the board.
    pub fn in_bounds(&self) -> bool {
        if self.row >= BOARD_SIZE || self.col >= BOARD_SIZE {
            return false;
        }
        let n = self.ship.len() as u16;
        match self.orientation {
            Orientation::Horizontal => self.col as u16 + n <= BOARD_SIZE as u16,
            Orientation::Vertical => self.row as u16 + n <= BOARD_SIZE as u16,
        }
    }
}

/// A coordinate fired upon.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Shot {
    pub row: u8,
    pub col: u8,
}

impl Shot {
    pub fn new(row: u8, col: u8) -> Self {
        Shot { row, col }
    }
}

/// Fixed-shape membership test: scans the WHOLE history with no early exit,
/// so the trace shape depends only on the (public) history length, never on
/// where — or whether — a match occurs.
#[inline]
fn history_covers(history: &[Shot], row: u8, col: u8) -> bool {
    let mut found = false;
    for h in history {
        found |= (h.row == row) & (h.col == col);
    }
    found
}

/// A complete fleet: exactly one of each ship kind.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Fleet {
    pub placements: [Placement; NUM_SHIPS],
}

impl Fleet {
    /// Validate the fleet against the rules of standard Battleship:
    ///   1. exactly one of each ShipKind,
    ///   2. every ship fully on-board,
    ///   3. no two ships overlap.
    ///
    /// (Standard rules allow ships to touch, so adjacency is NOT rejected.)
    ///
    /// Once the composition check passes, the remaining work touches exactly
    /// 17 ship cells regardless of where the ships are, so the trace shape is
    /// the same for every valid fleet.
    pub fn is_valid(&self) -> bool {
        // 1. exact fleet composition: each kind appears exactly once.
        let mut composition_ok = true;
        for kind in ShipKind::all() {
            let mut count = 0u8;
            for p in self.placements.iter() {
                count += (p.ship.id() == kind.id()) as u8;
            }
            composition_ok &= count == 1;
        }
        if !composition_ok {
            return false;
        }

        // 2. bounds (accumulated, no early exit)
        let mut in_bounds = true;
        for p in self.placements.iter() {
            in_bounds &= p.in_bounds();
        }

        // 3. no overlap: full pairwise cell comparison. The multiset of ship
        //    lengths is fixed by step 1, so the number of comparisons is a
        //    constant independent of the layout.
        let mut overlap = false;
        for a in 0..NUM_SHIPS {
            for b in (a + 1)..NUM_SHIPS {
                let (pa, pb) = (&self.placements[a], &self.placements[b]);
                for i in 0..pa.ship.len() {
                    for j in 0..pb.ship.len() {
                        overlap |= pa.cell(i) == pb.cell(j);
                    }
                }
            }
        }

        in_bounds & !overlap
    }

    /// Does any ship occupy the targeted cell? (fixed-shape scan of all 17
    /// ship cells)
    pub fn occupies(&self, shot: Shot) -> bool {
        let mut hit = false;
        for p in self.placements.iter() {
            for i in 0..p.ship.len() {
                let (r, c) = p.cell(i);
                hit |= (r == shot.row) & (c == shot.col);
            }
        }
        hit
    }

    /// Which ship (if any) occupies the targeted cell?
    ///
    /// Ships never overlap in a valid fleet, so at most one term of the sum
    /// below is non-zero; the arithmetic select avoids a secret-dependent
    /// branch.
    pub fn ship_at(&self, shot: Shot) -> Option<ShipKind> {
        let mut sel = 0u8; // ship id + 1, or 0 for open water
        for p in self.placements.iter() {
            let mut on_ship = false;
            for i in 0..p.ship.len() {
                let (r, c) = p.cell(i);
                on_ship |= (r == shot.row) & (c == shot.col);
            }
            sel += on_ship as u8 * (p.ship.id() + 1);
        }
        ShipKind::from_id(sel.wrapping_sub(1))
    }

    /// If `shot` hits a ship AND every cell of that ship is covered by
    /// `history ∪ {shot}`, return the sunk ship. Otherwise None.
    ///
    /// `history` is the (public) list of all shots previously fired at this
    /// board, which lets the proof be self-contained: sunk-state is recomputed
    /// from public data + the private board rather than carried as state.
    pub fn ship_sunk(&self, shot: Shot, history: &[Shot]) -> Option<ShipKind> {
        let mut sel = 0u8; // sunk ship id + 1, or 0
        for p in self.placements.iter() {
            let mut on_ship = false;
            let mut covered = true;
            for i in 0..p.ship.len() {
                let (r, c) = p.cell(i);
                let is_shot = (r == shot.row) & (c == shot.col);
                on_ship |= is_shot;
                covered &= is_shot | history_covers(history, r, c);
            }
            sel += (on_ship & covered) as u8 * (p.ship.id() + 1);
        }
        ShipKind::from_id(sel.wrapping_sub(1))
    }

    /// True iff every cell of every ship is contained in `history`.
    pub fn all_sunk(&self, history: &[Shot]) -> bool {
        let mut all = true;
        for p in self.placements.iter() {
            for i in 0..p.ship.len() {
                let (r, c) = p.cell(i);
                all &= history_covers(history, r, c);
            }
        }
        all
    }

    /// Canonical byte preimage that is hashed (with the salt) into the
    /// commitment. Placements are sorted by ship id so the encoding is
    /// independent of the order the prover happened to list them in.
    ///
    /// The hashing itself is done by the caller:
    ///   * guest -> RISC Zero accelerated SHA-256
    ///   * host  -> the `sha2` crate (for the optional endgame audit)
    /// Both hash these exact bytes, so both get the same 32-byte digest.
    pub fn digest_preimage(&self, salt: &[u8; 32]) -> Vec<u8> {
        let mut sorted = self.placements;
        sorted.sort_by_key(|p| p.ship.id());
        let mut out = Vec::with_capacity(NUM_SHIPS * 4 + 32);
        for p in sorted.iter() {
            out.push(p.ship.id());
            out.push(p.row);
            out.push(p.col);
            out.push(match p.orientation {
                Orientation::Horizontal => 0,
                Orientation::Vertical => 1,
            });
        }
        out.extend_from_slice(salt);
        out
    }
}

// ---------------------------------------------------------------------------
// I/O structs shared by host and guest (commitment kept as a plain [u8; 32]
// so this crate never needs to depend on RISC Zero).
// ---------------------------------------------------------------------------

/// Private input to the board-commitment proof.
#[derive(Clone, Serialize, Deserialize)]
pub struct CommitInput {
    pub fleet: Fleet,
    pub salt: [u8; 32],
}

/// Public output of the board-commitment proof.
#[derive(Clone, Serialize, Deserialize)]
pub struct CommitJournal {
    pub commitment: [u8; 32],
}

/// Input to the per-shot proof. `fleet` and `salt` are the private witness;
/// `commitment`, `shot`, and `history` are public and bound into the journal.
#[derive(Clone, Serialize, Deserialize)]
pub struct ShotInput {
    pub fleet: Fleet,
    pub salt: [u8; 32],
    pub commitment: [u8; 32],
    pub shot: Shot,
    pub history: Vec<Shot>,
}

/// Public output of the per-shot proof.
///
/// `history` is echoed back into the journal so the verifier can check that
/// the prover computed against the REAL public shot history (the sequence of
/// shots the verifier itself fired), not a fabricated one. Without this
/// binding, `sunk` and `defeated` would be attestations about an arbitrary
/// prover-chosen history and a losing player could simply never be provably
/// defeated. `defeated` makes game termination itself part of the proven
/// statement: it is true iff `history ∪ {shot}` covers every ship cell.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ShotJournal {
    pub commitment: [u8; 32],
    pub shot: Shot,
    pub hit: bool,
    pub sunk: Option<ShipKind>,
    pub defeated: bool,
    pub history: Vec<Shot>,
}

// ---------------------------------------------------------------------------
// Tests: the security-critical logic, including adversarial cases and
// equivalence of the fixed-shape implementations with naive references.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple valid fleet: every ship horizontal on its own row.
    fn valid_fleet() -> Fleet {
        Fleet {
            placements: [
                Placement { ship: ShipKind::Carrier, row: 0, col: 0, orientation: Orientation::Horizontal },
                Placement { ship: ShipKind::Battleship, row: 2, col: 0, orientation: Orientation::Horizontal },
                Placement { ship: ShipKind::Cruiser, row: 4, col: 0, orientation: Orientation::Horizontal },
                Placement { ship: ShipKind::Submarine, row: 6, col: 0, orientation: Orientation::Horizontal },
                Placement { ship: ShipKind::Destroyer, row: 8, col: 0, orientation: Orientation::Horizontal },
            ],
        }
    }

    #[test]
    fn valid_fleet_passes() {
        assert!(valid_fleet().is_valid());
    }

    #[test]
    fn cells_and_bounds() {
        let p = Placement { ship: ShipKind::Cruiser, row: 4, col: 0, orientation: Orientation::Horizontal };
        assert_eq!(p.cells(), vec![(4, 0), (4, 1), (4, 2)]);
        assert!(p.in_bounds());

        // Carrier(5) starting at col 7 would need cols 7..11 -> off board.
        let off = Placement { ship: ShipKind::Carrier, row: 0, col: 7, orientation: Orientation::Horizontal };
        assert!(!off.in_bounds());
    }

    #[test]
    fn wrong_composition_rejected() {
        // Replace the Destroyer with a second Carrier -> not one-of-each.
        let mut f = valid_fleet();
        f.placements[4] = Placement { ship: ShipKind::Carrier, row: 8, col: 0, orientation: Orientation::Horizontal };
        assert!(!f.is_valid());
    }

    #[test]
    fn overlap_rejected() {
        // Put the Destroyer on top of the Carrier's cells.
        let mut f = valid_fleet();
        f.placements[4] = Placement { ship: ShipKind::Destroyer, row: 0, col: 0, orientation: Orientation::Horizontal };
        assert!(!f.is_valid());
    }

    #[test]
    fn out_of_bounds_rejected() {
        let mut f = valid_fleet();
        f.placements[0] = Placement { ship: ShipKind::Carrier, row: 0, col: 8, orientation: Orientation::Horizontal };
        assert!(!f.is_valid());
    }

    #[test]
    fn extreme_coordinates_do_not_panic() {
        // Wrapping cell arithmetic must never panic, and the fleet must be
        // rejected by the bounds check.
        let mut f = valid_fleet();
        f.placements[0] = Placement { ship: ShipKind::Carrier, row: 255, col: 255, orientation: Orientation::Vertical };
        assert!(!f.is_valid());
    }

    #[test]
    fn hit_and_miss() {
        let f = valid_fleet();
        assert!(f.occupies(Shot::new(0, 0))); // Carrier
        assert!(f.occupies(Shot::new(8, 1))); // Destroyer tail
        assert!(!f.occupies(Shot::new(9, 9))); // empty water
        assert_eq!(f.ship_at(Shot::new(4, 1)), Some(ShipKind::Cruiser));
        assert_eq!(f.ship_at(Shot::new(9, 9)), None);
    }

    #[test]
    fn sunk_detection() {
        let f = valid_fleet();
        // Destroyer occupies (8,0) and (8,1).
        // First hit: not sunk yet.
        assert_eq!(f.ship_sunk(Shot::new(8, 0), &[]), None);
        // Second hit completes it.
        let history = vec![Shot::new(8, 0)];
        assert_eq!(f.ship_sunk(Shot::new(8, 1), &history), Some(ShipKind::Destroyer));
        // Hitting empty water never sinks anything.
        assert_eq!(f.ship_sunk(Shot::new(9, 9), &history), None);
    }

    #[test]
    fn win_requires_all_cells() {
        let f = valid_fleet();
        let mut history: Vec<Shot> = Vec::new();
        for p in f.placements.iter() {
            for (r, c) in p.cells() {
                history.push(Shot::new(r, c));
            }
        }
        assert_eq!(history.len(), TOTAL_SHIP_CELLS);
        assert!(f.all_sunk(&history));
        // Drop one cell -> not won.
        history.pop();
        assert!(!f.all_sunk(&history));
    }

    #[test]
    fn defeat_is_exactly_the_last_cell() {
        // `defeated` semantics used by the guest: all_sunk(history + shot)
        // flips to true exactly when the final ship cell is hit.
        let f = valid_fleet();
        let mut all_cells: Vec<Shot> = Vec::new();
        for p in f.placements.iter() {
            for (r, c) in p.cells() {
                all_cells.push(Shot::new(r, c));
            }
        }
        let (last, prior) = all_cells.split_last().unwrap();
        let mut with_last = prior.to_vec();
        assert!(!f.all_sunk(&with_last)); // 16 of 17 cells: not defeated
        with_last.push(*last);
        assert!(f.all_sunk(&with_last)); // 17 of 17: defeated
    }

    #[test]
    fn commitment_preimage_is_order_independent() {
        // Binding-relevant: the same board in a different listing order must
        // produce the SAME preimage (so the commitment is well-defined).
        let salt = [7u8; 32];
        let f1 = valid_fleet();
        let mut f2 = valid_fleet();
        f2.placements.reverse();
        assert_eq!(f1.digest_preimage(&salt), f2.digest_preimage(&salt));
    }

    #[test]
    fn different_boards_have_different_preimages() {
        // Binding intuition: two genuinely different boards yield different
        // preimages, so (modulo SHA-256 collisions) different commitments.
        let salt = [7u8; 32];
        let f1 = valid_fleet();
        let mut f2 = valid_fleet();
        f2.placements[0] = Placement { ship: ShipKind::Carrier, row: 0, col: 1, orientation: Orientation::Horizontal };
        assert_ne!(f1.digest_preimage(&salt), f2.digest_preimage(&salt));
    }

    #[test]
    fn salt_changes_preimage() {
        // Hiding intuition: the salt actually feeds the hash.
        let f = valid_fleet();
        assert_ne!(f.digest_preimage(&[0u8; 32]), f.digest_preimage(&[1u8; 32]));
    }

    // -----------------------------------------------------------------
    // Equivalence of the fixed-shape implementations with naive references
    // -----------------------------------------------------------------

    /// Tiny deterministic RNG (xorshift64*) so `core` needs no dev-deps.
    struct TestRng(u64);
    impl TestRng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545F4914F6CDD1D)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    fn random_valid_fleet(rng: &mut TestRng) -> Fleet {
        'restart: loop {
            let mut occupied: Vec<(u8, u8)> = Vec::new();
            let mut placements: Vec<Placement> = Vec::new();
            for ship in ShipKind::all() {
                let mut placed = false;
                for _ in 0..100 {
                    let orientation = if rng.below(2) == 0 { Orientation::Horizontal } else { Orientation::Vertical };
                    let p = Placement {
                        ship,
                        row: rng.below(BOARD_SIZE as u64) as u8,
                        col: rng.below(BOARD_SIZE as u64) as u8,
                        orientation,
                    };
                    if !p.in_bounds() || p.cells().iter().any(|c| occupied.contains(c)) {
                        continue;
                    }
                    occupied.extend(p.cells());
                    placements.push(p);
                    placed = true;
                    break;
                }
                if !placed {
                    continue 'restart;
                }
            }
            return Fleet { placements: placements.try_into().unwrap() };
        }
    }

    /// Naive reference implementations (early exits allowed): behavioural
    /// ground truth for the fixed-shape versions.
    fn ref_occupies(f: &Fleet, s: Shot) -> bool {
        f.placements.iter().any(|p| p.cells().contains(&(s.row, s.col)))
    }
    fn ref_ship_sunk(f: &Fleet, s: Shot, history: &[Shot]) -> Option<ShipKind> {
        let p = f.placements.iter().find(|p| p.cells().contains(&(s.row, s.col)))?;
        let all_hit = p.cells().iter().all(|&(r, c)| {
            (r == s.row && c == s.col) || history.iter().any(|h| h.row == r && h.col == c)
        });
        all_hit.then_some(p.ship)
    }
    fn ref_all_sunk(f: &Fleet, history: &[Shot]) -> bool {
        f.placements
            .iter()
            .all(|p| p.cells().iter().all(|&(r, c)| history.iter().any(|h| h.row == r && h.col == c)))
    }

    #[test]
    fn fixed_shape_logic_matches_reference() {
        let mut rng = TestRng(0x5EEDu64);
        for _ in 0..25 {
            let f = random_valid_fleet(&mut rng);
            // A random public history of ~30 distinct shots.
            let mut history: Vec<Shot> = Vec::new();
            while history.len() < 30 {
                let s = Shot::new(rng.below(10) as u8, rng.below(10) as u8);
                if !history.contains(&s) {
                    history.push(s);
                }
            }
            for r in 0..BOARD_SIZE {
                for c in 0..BOARD_SIZE {
                    let s = Shot::new(r, c);
                    assert_eq!(f.occupies(s), ref_occupies(&f, s));
                    assert_eq!(f.ship_sunk(s, &history), ref_ship_sunk(&f, s, &history));
                }
            }
            assert_eq!(f.all_sunk(&history), ref_all_sunk(&f, &history));
        }
    }
}
