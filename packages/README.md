# packages

Общие библиотеки и контракты, не принадлежащие одному solution.

```text
packages/proto/      protobuf/gRPC contracts
packages/contracts/  межъязыковые протоколы (wakegrant)
packages/go/         shared Go packages
packages/kotlin/     shared Kotlin packages (Android)
```

Правило: код, нужный только `solutions/alarm`, живёт в его `internal/`.
В `packages/` выносим только то, что используется несколькими solutions
или projects.
