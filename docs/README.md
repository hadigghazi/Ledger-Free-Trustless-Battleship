# Documentation

- [`paper/zk-battleship.tex`](paper/zk-battleship.tex) — the research paper:
  *Ledger-Free Trustless Battleship: Self-Contained Zero-Knowledge Shot
  Proofs with Public-History Binding*. Compile with
  `pdflatex zk-battleship.tex` (run twice for references); a compiled
  [`paper/zk-battleship.pdf`](paper/zk-battleship.pdf) is checked in.

All measurements in the paper are reproducible from this repository:

```bash
cargo run --release -p host --bin zk-battleship-bench          # Table 1 + cycle study
cargo test --release -p host -- --ignored --test-threads=1     # adversarial protocol tests
cargo test -p zk-battleship-core                               # rule-logic tests
```
