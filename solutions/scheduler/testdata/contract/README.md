# Contract fixtures

Замороженные golden HTTP-фикстуры контракта `home.v1`. Формат и семантика
`capture`, `{{name}}` и плейсхолдеров описаны в
`packages/rust/home-contracttest/README.md`.

Их читает раннер `solutions/scheduler/crates/scheduler/tests/contract.rs`.
Фикстуры не перегенерируются автоматически: изменение wire-поведения требует
осознанной правки фикстуры в том же PR.
