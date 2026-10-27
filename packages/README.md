# packages

Общие библиотеки и контракты, не принадлежащие одному solution.

```text
packages/proto/    protobuf/gRPC contracts
packages/go/       shared Go packages (later)
packages/rust/     performance-sensitive Rust crates (later)
```

Правило: код, нужный только `solutions/alarm`, живёт в его `internal/`.
В `packages/` выносим только то, что используется несколькими solutions
или projects.
