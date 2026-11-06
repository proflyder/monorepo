# Contract fixtures

Замороженные golden HTTP-фикстуры контракта `home.v1`. Формат и семантика
`capture`, `{{name}}` и плейсхолдеров описаны в
`packages/go/contracttest/README.md`.

Фикстуры были сняты с Go-реализации до её удаления и теперь читаются
Rust-раннером `solutions/scheduler/crates/scheduler/tests/contract.rs`.
После удаления Go они не перегенерируются автоматически: изменение
wire-поведения требует осознанной правки фикстуры в том же PR.
