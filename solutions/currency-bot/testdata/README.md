# Frozen currency-bot contract data

These files are the language-neutral contract captured from the former Go
implementation. The Go sources under `solutions/currency-bot/{cmd,internal}` were
deleted when the deployed service moved to Rust; this directory is **frozen
data**, not generated output that the current tree can reproduce.

- `contract/*.json` — HTTP goldens (paths, statuses, headers, bodies) replayed
  by `crates/currency-bot/src/contract_tests.rs`.
- `vectors/*.json` — parsing/IQR/best-five, alert thresholds and dedup, exact
  Telegram HTML, command texts and schedules, replayed by the crate's unit
  tests.
- `rates/kurskz.html` — anonymised kurs.kz page used by the parser vectors.
- `store/schema_v1.sql` — schema v1 with sample rows; the Rust store opens it
  and must keep reading and writing the same file in place.

The Rust crate must replay the committed bytes unchanged. Do not hand-edit a
fixture to match new behaviour: a contract change is a deliberate product
decision backed by its own PR.

The former generators lived in the Go packages (`go test -update` in
`solutions/currency-bot/internal/...`). They were removed together with the Go
code. To regenerate, restore the Go tree from Git history (the parent of the
cutover commit), run the `-update` tests there, and copy the produced JSON back;
then delete the restored tree again.
