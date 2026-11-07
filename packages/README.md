# packages

Общие библиотеки и контракты, не принадлежащие одному solution.

```text
packages/proto/      protobuf/gRPC contracts
packages/contracts/  межъязыковые протоколы (wakegrant)
packages/rust/       shared Rust packages
packages/kotlin/     shared Kotlin packages (Android)
```

Правило: код, нужный только одному solution, живёт в его собственном крейте/пакете.
В `packages/` выносим только то, что используется несколькими solutions
или projects.
