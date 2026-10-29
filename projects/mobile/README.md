# projects/mobile

Mobile clients for the home server.

- **Android — active:** `android/` (application ID `dev.proflyder.home`).
  Kotlin, Android Views, one Activity, Fragment navigation behind typed
  contracts, Metro compile-time DI, root Bazel/Bazelisk build. It manages
  server state only: the home server owns scheduling and execution.
- **iOS — not initialized.** No code or Bazel Apple rules yet.

Android build, test, architecture, permissions, endpoint format, and current
device-verification status are documented in
[`android/README.md`](android/README.md). Current system facts and open work are
in [`docs/STATE.md`](../../docs/STATE.md) and
[`docs/PENDING.md`](../../docs/PENDING.md); Android tests are documented in
[`docs/TESTING.md`](../../docs/TESTING.md).
