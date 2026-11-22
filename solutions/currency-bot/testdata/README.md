# Frozen currency-bot contract data

Language-neutral contract the Rust crate must replay byte for byte. It is
frozen data: no generator exists in this tree.

- `contract/*.json` — HTTP goldens (paths, statuses, headers, bodies) replayed
  by `crates/currency-bot/src/contract_tests.rs`.
- `vectors/*.json` — parsing/IQR/best-five, alert thresholds and dedup, exact
  Telegram HTML (including `.x5` rounding), command texts and schedules (UTC,
  Asia/Almaty and DST), replayed by `src/domain/vectors.rs`.
- `rates/kurskz.html` — anonymised kurs.kz page used by the parser vectors.
- `store/schema_v1.sql` — schema v1 with sample rows; the store test opens it
  and must keep reading and writing the same file in place.

Do not hand-edit a fixture to match new behaviour: a contract change is a
deliberate product decision in its own PR.
