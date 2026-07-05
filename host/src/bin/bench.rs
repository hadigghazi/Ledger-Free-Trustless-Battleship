//! Measurement harness for the paper's evaluation section. Not part of the
//! game. Produces:
//!
//!   1. proving / verification times and receipt sizes for the commit proof
//!      and for shot proofs at several history lengths (composite receipts,
//!      plus succinct receipts for comparison);
//!   2. executor cycle counts for many random boards, to check that guest
//!      execution is witness-independent (the side-channel hardening claim).
//!
//! Run:  cargo run --release -p host --bin zk-battleship-bench

use std::time::Instant;

use methods::{COMMIT_BOARD_ELF, PROVE_SHOT_ELF};
use rand::rngs::StdRng;
use rand::SeedableRng;
use risc0_zkvm::{default_executor, default_prover, ExecutorEnv, ProverOpts, Receipt};
use zk_battleship_core::{CommitInput, Shot, ShotInput};

use host::board_gen::{random_fleet, random_salt};
use host::game::host_commitment;
use host::verifier::{verify_commit, verify_shot};

fn json_size(receipt: &Receipt) -> usize {
    serde_json::to_vec(receipt).map(|v| v.len()).unwrap_or(0)
}

fn seal_bytes(receipt: &Receipt) -> usize {
    match &receipt.inner {
        risc0_zkvm::InnerReceipt::Composite(c) => {
            c.segments.iter().map(|s| s.get_seal_bytes().len()).sum()
        }
        risc0_zkvm::InnerReceipt::Succinct(s) => s.get_seal_bytes().len(),
        risc0_zkvm::InnerReceipt::Groth16(g) => g.seal.len(),
        _ => 0,
    }
}

/// A distinct-shot history of the given length (row-major cell order).
fn history_of_len(n: usize) -> Vec<Shot> {
    (0..n as u8).map(|i| Shot::new(i / 10, i % 10)).collect()
}

fn main() -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(42);
    let fleet = random_fleet(&mut rng);
    let salt = random_salt(&mut rng);
    let commitment = host_commitment(&fleet, &salt);

    println!("# ZK Battleship benchmarks");
    println!("# host: {} logical cores", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0));
    println!();
    println!("proof,receipt_kind,history_len,prove_s,verify_ms,journal_B,seal_B,wire_json_B,user_cycles,total_cycles");

    let prover = default_prover();

    for (kind, opts) in [("composite", ProverOpts::default()), ("succinct", ProverOpts::succinct())] {
        // ---- commit proof ----
        let env = ExecutorEnv::builder()
            .write(&CommitInput { fleet: fleet.clone(), salt })?
            .build()?;
        let t = Instant::now();
        let info = prover.prove_with_opts(env, COMMIT_BOARD_ELF, &opts)?;
        let prove_s = t.elapsed().as_secs_f64();
        let receipt = info.receipt;
        let t = Instant::now();
        verify_commit(&receipt)?;
        let verify_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "commit,{kind},0,{prove_s:.2},{verify_ms:.1},{},{},{},{},{}",
            receipt.journal.bytes.len(),
            seal_bytes(&receipt),
            json_size(&receipt),
            info.stats.user_cycles,
            info.stats.total_cycles,
        );

        // ---- shot proofs at several history lengths ----
        for hist_len in [0usize, 25, 50, 99] {
            let history = history_of_len(hist_len);
            let shot = Shot::new(9, 9);
            let input = ShotInput {
                fleet: fleet.clone(),
                salt,
                commitment,
                shot,
                history: history.clone(),
            };
            let env = ExecutorEnv::builder().write(&input)?.build()?;
            let t = Instant::now();
            let info = prover.prove_with_opts(env, PROVE_SHOT_ELF, &opts)?;
            let prove_s = t.elapsed().as_secs_f64();
            let receipt = info.receipt;
            let t = Instant::now();
            verify_shot(&receipt, commitment, shot, &history)?;
            let verify_ms = t.elapsed().as_secs_f64() * 1e3;
            println!(
                "shot,{kind},{hist_len},{prove_s:.2},{verify_ms:.1},{},{},{},{},{}",
                receipt.journal.bytes.len(),
                seal_bytes(&receipt),
                json_size(&receipt),
                info.stats.user_cycles,
                info.stats.total_cycles,
            );
        }
    }

    // ---- witness-independence: executor cycle counts across random boards ----
    println!();
    println!("# executor user-cycle counts for prove_shot across 20 random boards");
    println!("# (fixed shot + history; identical counts = witness-independent trace)");
    let history = history_of_len(30);
    let shot = Shot::new(9, 9);
    let executor = default_executor();
    let mut counts = Vec::new();
    for i in 0..20u64 {
        let mut rng = StdRng::seed_from_u64(1000 + i);
        let f = random_fleet(&mut rng);
        let s = random_salt(&mut rng);
        let c = host_commitment(&f, &s);
        let input = ShotInput { fleet: f, salt: s, commitment: c, shot, history: history.clone() };
        let env = ExecutorEnv::builder().write(&input)?.build()?;
        let session = executor.execute(env, PROVE_SHOT_ELF)?;
        counts.push(session.cycles());
    }
    let min = counts.iter().min().unwrap();
    let max = counts.iter().max().unwrap();
    println!("cycles: min={min} max={max} spread={}", max - min);

    Ok(())
}
