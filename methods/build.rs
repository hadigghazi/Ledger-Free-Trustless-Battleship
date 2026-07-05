fn main() {
    // Compiles the guest crate to RISC-V ELF binaries and emits, for each
    // guest binary, a pair of constants for the host to use:
    //   COMMIT_BOARD_ELF / COMMIT_BOARD_ID
    //   PROVE_SHOT_ELF   / PROVE_SHOT_ID
    risc0_build::embed_methods();
}
