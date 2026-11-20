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

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `kotlin/core/src/main/kotlin/dev/proflyder/home/core/endpoint/ServerEndpoint.kt`
  - A validated base endpoint for the home server's Connect listener. The client never accepts credentials, query parameters, fragments, or ambiguous schemes. The bearer token is a separate secret and is never part of a URL;
  - Parses a user-entered LAN address or home HTTPS base URL. Accepted forms: `http://host[:port][/prefix]`, `https://host[:port][/prefix]`, IPv4, bracketed IPv6, and DNS names. A bare host is rejected so the transport is never guessed.
- `kotlin/core/src/main/kotlin/dev/proflyder/home/core/nav/PendingTransitions.kt` — A small, Android-free queue for navigation commands that arrive while the FragmentManager cannot safely commit (state saved or host not resumed).
- `kotlin/core/src/main/kotlin/dev/proflyder/home/core/nav/TabRouteStack.kt` — Pure selected-tab route state used by the Fragment router.
  - Adds [route] unless it is already current. Returns whether a route was actually pushed, so the caller never creates a second Fragment transaction/back-stack entry for the same destination.
- `kotlin/core/src/main/kotlin/dev/proflyder/home/core/operation/IdempotentRequestKey.kt` — Retains one non-secret idempotency key for a logical create/start operation.
- `kotlin/core/src/main/kotlin/dev/proflyder/home/core/operation/SuspendRunCatching.kt`
  - `runCatching` for suspending blocks that must not turn cancellation into a failure value. A cancelled coroutine has to keep propagating `CancellationException`; treating it as an ordinary failure leaves work running after its scope died.
- `kotlin/core/src/main/kotlin/dev/proflyder/home/core/text/SpeakText.kt` — Rules for text spoken on the station, shared by the Home screen and the scenario builder. The server re-validates everything: this is only the local guard that keeps the "Say" button disabled for input the executor rejects.
  - Characters left for the counter, never negative.
- `kotlin/core/src/main/kotlin/dev/proflyder/home/core/time/DurationText.kt`
  - `mm:ss` below one hour, `h:mm:ss` otherwise. Never negative.
