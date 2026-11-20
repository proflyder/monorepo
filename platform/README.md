# platform

Общая платформенная обвязка: Bazel toolchains, CI-контракты, генераторы,
кросс-компиляция и правила для мобильных платформ.

- `platform/bazel/` — toolchains и общие Bazel-настройки.
- `platform/ci/` — карта юнитов (`units.mjs`), маршрутизация затронутых
  юнитов и `ci-ok` gate.
- `platform/quality/` — аудит репозитория, статические проверки, Gitleaks.
- `platform/deploy-rs/` — `home-deploy` (Rust): render/converge/rollout
  юнитов; `platform/deploy/` — `resolve.sh` и общие манифесты.
- `platform/android/` — Bazel-правила, lint и инструменты Android-сборки.
- Values Prometheus/Loki/Alloy — `projects/home/kubernetes/observability/values/`.

CI работает на GitHub-hosted runners, использует `.github/workflows/_*.yml`
и публикует обязательный `ci-ok`; release/pins — `deploy-home.yml`. Подробности —
`docs/TESTING.md` и `docs/GIT.md`.

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `android/BUILD.bazel`
  - The Firebase config is private and never committed. When the gitignored symlink is present the genrule embeds its resources; otherwise the app still builds and Firebase stays uninitialized.
  - Target device ABI for the Home Android app. Without --android_platforms, rules_android builds native libraries for the host CPU (x86_64 on CI), so an APK with JNI code (libwebrtc) fails on phones with INSTALL_FAILED_NO_MATCHING_ABIS.
- `android/apk_abi_test.sh` — Fails if the APK ships native libraries without an arm64-v8a build, or with libraries for any other ABI (a host-CPU-only APK cannot install on phones).
- `android/apk_manifest_test.sh` — Verifies that the built APK actually carries the call feature's manifest: library manifests are only merged when the library sets exports_manifest.
- `android/apk_runtime_closure.sh` — Parse real DEX class_defs, then prove the check rejects a class that remains referenced but has no definition.
- `android/defs.bzl` — Shared Bazel macros for the Android client.
  - Plain Kotlin/JVM library with the serialization compiler plugin. Used by infrastructure modules that do not use Metro DI.
- `android/detekt.bzl` — Bazel-driven detekt for the Home Android client.
- `android/lint.bzl` — Bazel driven Android Lint for the Home Android client.
- `android/sign-release.sh` — Sign a Bazel-built APK with the personal release key and verify the result.
  - The expected signer is mandatory: a build signed by any other key must never be published, so a missing expectation is an error, not a warning.
- `android/sign-release_test.sh` — Self-test for platform/android/sign-release.sh.
  - --- Fail closed ----------------------------------------------------------
- `android/tools/apk_runtime_closure/src/main/kotlin/dev/proflyder/home/tools/apkruntimeclosure/Main.kt` — Command apk_runtime_closure is the APK-level runtime closure gate for the Android client.
  - OkHttp 5's JVM artifact references the GraalVM native-image hooks from its optional native-image configuration; they are never loaded on Android.
  - firebase-messaging is published R8-minified and these two nested helper types are absent from classes.jar while MessagingAnalytics and TopicsSubscriber still name them in their constant pools.
- `android/tools/gen_view_binding/src/main/kotlin/dev/proflyder/home/tools/genviewbinding/Main.kt` — Command gen_view_binding generates Kotlin ViewBinding classes from Android layout XML files.
- `android/tools/google_services/src/main/kotlin/dev/proflyder/home/tools/googleservices/Main.kt` — Command google_services generates Android string resources from google-services.json.
- `android/tools/lint_runner/src/main/kotlin/dev/proflyder/home/tools/lintrunner/Main.kt` — Command lint_runner runs Android Lint over the Bazel modules of the Home Android client.
  - Environment access seam so tests never mutate the JVM process environment.
- `ci/affected.mjs` — Compute the CI units affected by a change range.
  - `deploy: false` means "run these CI jobs, but do not redeploy units just because this path changed". Workflow and CI-harness edits only need the checks to re-run; they must not roll the home units after merge.
  - The station's target-only toolchain inputs. They change how the native station/provider are cross-built, never the host Rust/images/Android builds.
  - Scope of a MODULE.bazel diff: "maven", "native", "maven+native" or null. Comments and whitespace never count. A diff is scoped only when the residual tokens after peeling that category are identical: maven - rules_jvm_external (`maven.install(...)`, …
  - Kept as the narrow predicate used by the Maven-only regression tests.
  - Defensive: the manifest-only case returned above, so a Cargo change without a lock diff must not silently route nothing.
  - True only when an Android application input actually changed. A global invalidation (MODULE.bazel) or a CI-harness edit still selects the android units for tests, but must never publish an APK.
  - Paths whose change means the Android application itself changed. Android unit paths cover the application and the shared Kotlin libraries it builds; deploy-relevant Android invalidations cover the platform build tooling (for example `platform/android/**`).
  - Whether a changed path is an actual Android application input. Markdown is documentation even inside the app tree: it never reaches the APK, so a README edit must not publish a build.
  - Deploy selection ignores `deploy: false` invalidations. CI keeps running the invalidated jobs (routes above), but a workflow/harness-only change must not roll every deployable unit after merge.
  - Shared deploy fields: only a deploy-selected unit publishes images (a CI-harness change still runs the checks but must not push every image). A unit without images gets empty values and the `unit` workflow skips the image/attestation steps.
  - Deployable units selected by the same routing as CI. A shared Rust crate change reaches every importer through the Bazel loading graph, so it also deploys every importer that declares a `deploy` block.
- `ci/affected.sh` — Local reproduction of the CI routing decision. See docs/TESTING.md.
- `ci/affected.test.mjs` — Unit tests for the CI affected-unit router and the unit-map validator.
  - Android-specific flags live in platform/android/android.bazelrc, activated with --config=android. A change there must not invalidate every unit the way a root `.bazelrc` change does.
  - Comments never reach the build, so a comment-only edit is Maven-safe.
  - Unchanged mixed lines do not block a cache-only edit elsewhere.
- `ci/build-images.sh` — Build and optionally publish the container images of one deploy unit.
- `ci/check_units.mjs` — Validate the CI unit map and prove no source/test file falls out of CI.
  - `_unit.yml` attests each pushed image with one static step per index; the unit map must not exceed that bound (today the maximum is Hermes' four).
  - Every job that writes to a registry must log in before the first write. Jobs are delimited by their two-space keys under the top-level `jobs:` map; a nested `steps:` list never uses that indentation, so a line scan is enough and needs no YAML dependency.
- `ci/check_units.test.mjs`
  - The incident shape: promote.mjs re-tags with crane, and the public package reads anonymously, so only the manifest PUT fails.
  - The #338/#339 incident: the caller job had no permissions block, so it inherited workflow-level `contents: read` while the reusable pinned with `contents: write`.
- `ci/ci_gate.mjs` — Decide whether the aggregate `ci-ok` job passes.
  - The pipeline was cancelled or never produced results: report red so the required check cannot pass, and let the caller's ci-ok job fail.
- `ci/ci_gate.test.mjs` — Tests for the aggregate `ci-ok` gate.
- `ci/fad_cleanup.mjs` — Firebase App Distribution release retention.
  - Every release sharing the maximum createTime is "the newest": an upload race must not make the cleanup pick one of two equally fresh builds.
  - CLI used by `.github/workflows/cleanup.yml`: node platform/ci/fad_cleanup.mjs \ --credentials "$FIREBASE_SERVICE_ACCOUNT" \ --google-services "$GOOGLE_SERVICES_JSON" [--apply] `--fixture <json>` ({releases: [...]}) replays the selection locally without …
- `ci/fad_cleanup.test.mjs` — Firebase App Distribution retention tests.
- `ci/ghcr_cleanup.mjs` — GHCR cleanup policy for the deploy images (DECISIONS 36).
  - Digests that must never be deleted even though the retention window would otherwise select them. `voice-proxy` → sha256:238e…fca9 is the last Python voice-proxy image: the Hermes health-exporter container still runs from it (pinned in private runtime values) …
  - Packages the policy never considers even if a future unit map lists them. Empty: native release packages are included, with their `latest`, version and `sha-<commit>` tags protected by `PROTECTED_TAGS` and only stale `tree-*` versions eligible for deletion.
  - Newest by creation time, with the id as the tie-breaker (GitHub ids grow monotonically). A missing date never makes a version "newest".
  - The newest deploy versions form the per-package retention window. Referrers never count toward it.
  - Local dry run against a fixture (never calls GitHub): node platform/ci/ghcr_cleanup.mjs --fixture platform/ci/testdata/ghcr-versions.json `--delete` is refused: deletion only happens in the workflow.
- `ci/ghcr_cleanup.test.mjs` — GHCR cleanup policy tests (DECISIONS 36).
- `ci/pins.mjs` — Commit verified image digests to the machine branch `deploy/home-apps`.
  - Machine branch Flux reads; never `develop`.
  - ConfigMap key for an image: `ha-bridge` -> `HA_BRIDGE_IMAGE`. The pin names never reveal a secret; they are the substitution variables the app manifests already use.
  - A failed cleanup must not mask the real error or the push result.
  - Retry `syncOnce` on a race with another unit's pins commit.
- `ci/pins.test.mjs` — Unit tests for the pins writer (решение 54).
- `ci/prebuilt-digests.sh` — Prebuilt image digests for the deploy scripts (CI promotion path).
- `ci/promote.mjs` — Promote the images a pull request already built (DECISIONS 36).
  - Tags that must never be recreated from a promoted digest here.
  - Pull request number from a squash-merge subject line: `feat(x): … (#204)`. The API is authoritative; this is the offline/limited-token fallback.
  - Fork PRs never publish tree tags: their token is read-only in CI.
  - The GitHub repository (`owner/repo`) used for PR lookups and provenance. `--repository` means two different things by mode: for images it is already the GitHub repository (the signer identity), for native units it is the GHCR publish repository …
  - Resolve the PR head of the deploy commit and decide whether the delta between that head and the merge commit touches `unit`. The delta is the TREE difference `git diff <head> <commit>`, not a range from a merge base: the head already contains everything CI …
  - Resolve the PR-head fallback for a CLI run. Never throws: a failure becomes `{error}`, which only matters when the current tree has no verified build.
  - Decide and perform the promotion. `exec(command, args)` returns stdout and throws on a non-zero exit; it is injected so tests never touch a registry.
  - Version annotation the publish scripts write next to a native release; the promote path reads it so the release tag and the binary's `--version` (which the station updater compares) can never disagree.
  - The merged tree normally has the artifact CI built for the PR. When `develop` moved before the squash merge, the PR head H is the fallback — but only if the delta H..commit does not touch this native unit (see `resolvePrHeadDelta`).
  - The version travels with the artifact: the annotation is what the binary prints, so a release tag can never disagree with the updater's check. An explicit `--version` is a legacy override and must agree when both exist.
  - This entry point is only used by deploy-home: the merged PR must have published the artifact. It never builds and does not search history: an older tree must never be re-tagged as the current release.
- `ci/promote.test.mjs` — Unit tests for the build-once promotion decision (DECISIONS 36).
  - Newest first, as `readHistory` returns it: the current commit, then a commit whose tree was never built, then the built one.
  - The conflicting immutable tag was not overwritten, and `latest` was never reached because the version is checked first.
  - -------------------------------------------------------------------------- PR-head delta fallback (#195): the squash merge landed on a newer develop -------------------------------------------------------------------------- CI verified the PR head H;
  - Siblings are handed to the renderer, never retagged.
  - Native: the publish registry must never stand in for the GitHub repository.
  - The PR was looked up in the GitHub repository, never in the registry.
  - ------------------------------------------------- computed version ------ A native release no longer takes its version from an argument computed by the caller: `publish-oci.sh` records it on the artifact and the promotion reads it, so the release tag can …
- `ci/push-image.sh` — Publish one already-built image under the immutable `tree-<hash>` tag a pull request uses to hand its build to `deploy-home` (DECISIONS 36).
- `ci/rebuild.mjs` — Unit selection for the manual "rebuild and deploy" workflow.
  - Resolve the requested selection into unit ids. `all` wins over the groups; `units` (a comma separated id list) narrows or replaces them, which is how an operator picks one unit without a checkbox.
- `ci/rebuild.test.mjs` — Contracts for the manual rebuild selection.
- `ci/safe-merge.mjs` — Merge a pull request only when its CI result matches the tree that lands.
  - Two independent budgets. `--max-updates` bounds how often the branch is refreshed; the move budget only grows when `develop` really got a new commit (a stale compare right after `update-branch` must not consume it).
  - Collect the paths a compare response says are changed, including the old side of a rename. `known` is false when the API returned no `files` array: an absent list proves nothing and must not be read as "no changed files".
  - Decide whether `develop` may stay ahead of the PR head at merge time. The squash merge then produces a tree that was never tested as a whole, but which differs from the tested PR tree only by `develop` commits that touch other files and no CI-global path — so …
  - Wait for a compare that lagged a branch update to catch up. `update-branch` changes the head immediately, but `compare` can keep reporting the old `behind_by` for a few seconds.
- `ci/safe-merge.test.mjs` — Unit tests for the safe squash-merge flow.
  - No `files` array proves nothing; it must not read as "no changed files".
- `ci/units.mjs` — CI unit map: the single source of truth for "what runs when".
  - Paths whose change invalidates more than the units whose `paths` match. `platforms: ["*"]` means every platform. Shared libraries are deliberately absent here: they reach their consumers through the dependency engines (Bazel reverse dependencies), not through …
  - Android platform inputs: the Bazel rules, the lint rule and `platform/android/android.bazelrc` (Android-only flags activated with `--config=android`). Never a global invalidation.
  - The scheduler deploy-control regression is expensive and only meaningful for the scheduler/bridge binaries, the Kubernetes deployment manifests and the deploy CLI the script drives (a deploy-rs change must not skip it).
  - The shared scheduler deploy script also renders ha-bridge, ha-control and ha-scenario (it never applies them for this unit). They are not this unit's artifact, so deploy-home resolves them from the newest verified build instead of requiring a tree tag for …
  - The K8s package also builds ha-bridge/ha-control images; a shared crate that reaches those must not select the scheduler unit. The scheduler image is selected through its own solution subtree.
  - Flux owns wakegrant since the pilot cutover (решение 54): Kustomization/app-wakegrant applies the digest from the `deploy/home-apps` pins branch. deploy-home only commits the verified digest through its `pins` job and never applies the workload.
  - One stateless image and Deployment. The FCM service account is an ESO Secret owned by the owner, so it is declared in the unit's manifests but never written by CI.
  - The shared script renders the scheduler too; this unit never applies it, so its image comes from the newest verified build (see the scheduler unit's comment).
  - The bridge/control binaries and images live under //solutions; the K8s package is shared with the scheduler image and must not make a scheduler-only label select the bridge.
  - The remaining Python contract test is the shell regression driver; converge/state/preflight live in platform/deploy-rs (unit above).
  - ...including every app Kustomization (`flux/cluster/apps.yaml`): an app manifest change must re-run the object-set and substitution contract (#297 broke it unnoticed because these were not routed).
  - ----------------------------------------------------------- Native -------- Native units: the Windows logon provider is a repository-script Bazel cross-build (rules_rust + hermetic zig cc_toolchain);
  - The station binary is rolled back on-device only, never through CI: the updater offers newer versions only, so the operator swaps the `pulsar.prev` copy and reloads the supervisor.
  - ----------------------------------------------------------- Quality -------- Always runs: repository audit, static gates, shellcheck, secret scan and the CI unit-map invariant check.
  - The root Cargo workspace/lockfiles and the shared Rust crates are routed precisely through the Bazel graph (see `affected.mjs`). They still need an owning unit for the coverage invariant, and the always-on quality job is the natural owner.
- `ci/version.mjs` — Deterministic, monotone release versions for native units.
- `ci/version.test.mjs` — Contracts for the deterministic native release version.
- `mise-tasks/lib/backup.mjs` — Backup front door behind `mise run backup:list|status|now|verify`.
  - Plan, confirm, run. Never runs a UI-only target; prints its note instead.
- `mise-tasks/lib/ops.mjs` — Deploy and rollback front door behind `mise run deploy`, `deploy:status` and `rollback`.
  - Run the dispatch. Returns a process exit code. `--dry-run` prints the command and never calls `gh`. Without `--yes` the operator must confirm.
  - Print the plan, then execute the documented command only with `--yes`. `forward-only` exits non-zero; `pulsar` prints the device instruction and never executes it.
- `mise-tasks/lib/ops.test.mjs`
  - All live home units are Flux-managed now, so the executable rollback kinds are exercised with a synthetic unit: the code path still exists for admin tooling and must not rot.
- `mise-tasks/lib/units-cli.mjs` — Unit dispatcher behind the mise tasks (`build`, `test`, `units`).
- `mise-tasks/lib/units-cli.test.mjs`
  - Non-Android Bazel units must not get the Android-only flags.
- `mise-tasks/test/bazel.sh` — `//...` includes the Android targets, so the Android-only flags from platform/android/android.bazelrc (version defines, Java toolchain, dex workers, the APK ABI) must be active exactly as they were before the split.
- `ops/backups.mjs` — Backup target registry: the single source of truth for "what is backed up, where, when, how to check it and how it is restored".
- `workstation/env.sh` — Sourced by mise ([env] _.source): exports the non-secret OpenBao address so the `bao` CLI works without flags. The token is never exported into the shell.
- `workstation/fnox.sh` — Run fnox with the repository's fnox.toml from any directory (also from a linked worktree): `platform/workstation/fnox.sh -P <profile> exec -- <cmd>`.
- `workstation/run.sh` — Run a repository Python tool with the interpreter pinned in mise.toml. `mise which` is independent of PATH order: another version manager's shims (asdf, pyenv) earlier in PATH must not pick a different python3.
- `workstation/test_workstation.py`
  - human session never outlives login previous token of this machine
- `workstation/with-kube-tunnel.sh` — Run a command with the K3s API reachable at 127.0.0.1:16443, where the admin kubeconfig points. The API (6443) is firewalled from the LAN, so an SSH tunnel through the node is opened for the command and closed afterwards. An already listening 16443 (a tunnel you keep open) is reused and left alone.
- `workstation/workstation.py` — Workstation access to OpenBao — the only source of secrets.
  - Print the operator token for fnox (`credential_command`); renews it first. Only fnox calls this: the value goes to its stdout pipe, never to a terminal or log of ours.
- `workstation/bin/bazel` — scripts call `bazel`; mise installs Bazelisk under its own name.
- `workstation/bin/vault` — fnox's vault provider calls the HashiCorp `vault` CLI; OpenBao's `bao` is command-compatible for the KV operations it uses.
