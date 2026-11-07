// CI unit map: the single source of truth for "what runs when".
//
// A *unit* is the smallest thing CI can run on its own: one service, one
// application, one shared library or one test suite directory. Each unit maps
// 1:1 to a matrix entry of one platform workflow (`.github/workflows/_*.yml`).
//
// Routing is computed by `platform/ci/affected.mjs`:
//   * go       - dependency closure from `go list -deps` of the unit's packages,
//                so a shared package change selects every importing unit.
//   * android  - reverse dependencies from the Bazel loading graph
//                (`bazel query --output=xml`), mapped back to unit names by
//                `bazel.roots`, so a Kotlin library change selects only the
//                applications that depend on it.
//   * python   - path rules below plus optional declarative `depends_on`.
//   * js       - path rules for dependency-free edge Workers (runs `node --test`).
//   * native   - path rules for non-Bazel native builds (repository-script
//                C/C++ and the Rust Pulsar station tree): pinned toolchains,
//                host tests, cross-builds and artifacts.
//   * quality  - always runs; owns repository-wide checks and every shell script.
//
// Adding a unit: copy a block, set unique `id`, `platform`, `paths` and the
// platform-specific commands. `platform/ci/check_units.mjs` enforces that every
// tracked test/source file is owned by at least one unit, so a new test cannot
// silently fall out of CI.
//
// This used to be `units.yml`; GitHub-hosted JavaScript cannot parse YAML
// without an npm dependency, so the map is a plain ES module (see DECISIONS 35).

export const config = {
  version: 1,

  // Paths whose change invalidates more than the units whose `paths` match.
  // `platforms: ["*"]` means every platform. Shared libraries are deliberately
  // absent here: they reach their consumers through the dependency engines
  // (`go list -deps`, Bazel reverse dependencies), not through path globs.
  //
  // `deploy: false` (default true) keeps the invalidation for CI only:
  // `deploy-home` never redeploys a unit selected *solely* by such a path. Workflow
  // and CI-harness edits must exercise the checks, but they do not change runtime
  // code and must not roll the home units.
  //
  // `MODULE.bazel` is global by default, with three scoped exceptions
  // (`affected.mjs` rewrites the changed path, fail safe is global):
  //   * Maven-only diff -> `MODULE.bazel#maven` -> android.
  //   * rules_rust/crate_universe, zig toolchains, rules_rust Windows patches
  //     and the station's `_SENDSPIN_BAZEL` http_archive()s ->
  //     `MODULE.bazel#native` -> native units (station/provider cross-build)
  //     plus the `pulsar-bazel` host checks (same Rust crates on the host).
  //   * a mix of both -> both virtual paths (union of the platforms).
  // Anything else in MODULE.bazel - bazel_dep, go_deps, oci, rules_go, python -
  // and an unparseable file, a missing base or an added/deleted file keep the
  // global rule below.
  //
  // `.bazelrc` is global unless its diff only changes cache/output flags
  // (`--disk_cache`, the disk-cache GC, `--repository_cache`, console output);
  // then `affected.mjs` rewrites it to `.bazelrc#cache`, which matches no unit
  // and runs only the always-on quality job. `platform/bazel/zig/**` and
  // `platform/bazel/windows/**` become `platform/bazel#native`: they are
  // target-constrained toolchains/patches for the native station and the
  // Windows provider, never for the host Go/images/Android builds.
  invalidations: [
    {
      paths: [
        "MODULE.bazel",
        "MODULE.bazel.lock",
        ".bazelrc",
        ".bazelversion",
        ".bazelignore",
        "BUILD.bazel",
        "platform/bazel/**",
        // Tool versions/checksums of every CI job (jdx/mise-action) and the
        // `bazel` shim on the mise PATH.
        "mise.toml",
        "mise.lock",
        "platform/workstation/bin/**",
      ],
      platforms: ["*"],
    },
    {
      paths: ["MODULE.bazel#maven"],
      platforms: ["android"],
    },
    {
      paths: ["MODULE.bazel#native"],
      platforms: ["native", "bazel"],
    },
    {
      paths: ["platform/bazel#native"],
      platforms: ["native"],
    },
    {
      paths: ["platform/android/**"],
      platforms: ["android"],
    },
    {
      paths: ["go.mod", "go.sum", "go.work", "go.work.sum"],
      platforms: ["go"],
    },
    // Root Cargo workspace: one lockfile and one toolchain for every
    // first-party Rust crate. A pin, lock or toolchain change reaches every
    // unit that compiles Rust (host checks and cross-builds).
    {
      paths: [
        "Cargo.toml",
        "Cargo.lock",
        "crate-universe.lock",
        "rust-toolchain.toml",
        "packages/rust/**",
      ],
      platforms: ["native", "bazel"],
    },
    {
      paths: ["requirements-test.txt"],
      platforms: ["python", "quality"],
    },
    // A reusable workflow change must run the jobs it defines, otherwise a broken
    // _go.yml/_android.yml passes PR CI unexercised.
    {
      paths: [".github/workflows/ci.yml"],
      platforms: ["*"],
      deploy: false,
    },
    {
      paths: [".github/workflows/_go.yml", ".github/workflows/_images.yml"],
      platforms: ["go"],
      deploy: false,
    },
    {
      paths: [".github/workflows/_android.yml"],
      platforms: ["android"],
      deploy: false,
    },
    {
      paths: [".github/workflows/_bazel.yml"],
      platforms: ["bazel"],
      deploy: false,
    },
    {
      paths: [".github/workflows/_python.yml"],
      platforms: ["python"],
      deploy: false,
    },
    {
      paths: [".github/workflows/_js.yml"],
      platforms: ["js"],
      deploy: false,
    },
    {
      paths: [".github/workflows/_native.yml"],
      platforms: ["native"],
      deploy: false,
    },
    {
      paths: [".github/workflows/**", "platform/ci/**"],
      platforms: ["quality"],
      deploy: false,
    },
  ],

  // The scheduler deploy-control regression is expensive and only meaningful for
  // the scheduler/bridge binaries and the Kubernetes deployment manifests.
  deploy_control: {
    units: ["scheduler", "ha-bridge"],
    paths: ["solutions/scheduler/**", "projects/home/kubernetes/scheduler/**"],
  },

  units: [
    // ------------------------------------------------------- Deploy tooling -----
    {
      id: "platform-deploy",
      platform: "bazel",
      title: "platform/deploy-rs (home-deploy CLI: render, env, oci, converge, state, preflight, hermes)",
      // The Rust crate is the executable contract; the legacy Go implementation
      // stays in the tree until the cleanup PR. Every deploy script and the
      // image build call this binary, so a change is both tested here and
      // routed to the units that deploy through it (their `paths` include
      // platform/deploy-rs/**).
      paths: ["platform/deploy-rs/**"],
      bazel: {
        roots: ["//platform/deploy-rs"],
        test_targets: [
          "//platform/deploy-rs:unit_tests",
          "//platform/deploy-rs:clippy",
          "//platform/deploy-rs:rustfmt",
          "//platform/deploy-rs:parity",
        ],
        build_targets: ["//platform/deploy-rs/..."],
      },
    },
    {
      id: "platform-deploy-go",
      platform: "go",
      title: "platform/deploy (legacy Go home-deploy, retained for revert)",
      // The executable contract moved to platform/deploy-rs; the Go tree stays
      // until the cleanup PR so a cutover revert is one PR. Keep its Go tests
      // green while it exists.
      paths: ["platform/deploy/**"],
      go: {
        module: ".",
        packages: ["./platform/deploy/..."],
        race: true,
        build: ["./platform/deploy/cmd/home-deploy"],
      },
    },
    {
      id: "voice-proxy",
      platform: "bazel",
      title: "solutions/voice-proxy (Rust TTS-safe OpenAI passthrough)",
      paths: ["solutions/voice-proxy/**"],
      // Rust service: host fmt/clippy/tests plus the static musl image. A
      // shared `packages/rust` edit reaches the unit through the Bazel loading
      // graph.
      bazel: {
        roots: ["//solutions/voice-proxy", "//packages/rust"],
        test_targets: [
          "//solutions/voice-proxy/crates/voice-proxy:unit_tests",
          "//solutions/voice-proxy/crates/voice-proxy:clippy",
          "//solutions/voice-proxy/crates/voice-proxy:rustfmt",
        ],
        build_targets: ["//solutions/voice-proxy/..."],
      },
    },
    {
      id: "health-exporter",
      platform: "bazel",
      title: "projects/home-assistant/deploy/health-exporter (runtime chart)",
      paths: ["projects/home-assistant/deploy/health-exporter/**"],
      // Rust service: host fmt/clippy/tests plus the static musl image. The
      // image is deployed by the `hermes` unit (one Helm release, one smoke,
      // one rollback).
      bazel: {
        roots: [
          "//projects/home-assistant/deploy/health-exporter",
          "//packages/rust",
        ],
        test_targets: [
          "//projects/home-assistant/deploy/health-exporter/crates/health-exporter:unit_tests",
          "//projects/home-assistant/deploy/health-exporter/crates/health-exporter:binary_tests",
          "//projects/home-assistant/deploy/health-exporter/crates/health-exporter:clippy",
          "//projects/home-assistant/deploy/health-exporter/crates/health-exporter:rustfmt",
        ],
        build_targets: ["//projects/home-assistant/deploy/health-exporter/..."],
      },
    },
    {
      id: "scheduler",
      platform: "bazel",
      title: "solutions/scheduler (Rust service) + home deployment",
      paths: [
        "solutions/scheduler/**",
        // Shared Rust crates and the home.v1 schema the service is generated
        // from; a change here rebuilds the scheduler.
        "packages/rust/**",
        "packages/proto/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // deploy-home rolls exactly these container images and Deployments when the
      // unit is affected. Infrastructure (ingress, observability, runtime chart)
      // has no deploy block on purpose: it is not auto-deployed.
      deploy: {
        builder: "bazel",
        target_root: "//projects/home/kubernetes/scheduler",
        images: ["scheduler", "home-deploy"],
        // The shared scheduler deploy script also renders ha-bridge and
        // ha-control (it never applies them for this unit). They are not this
        // unit's artifact, so deploy-home resolves them from the newest
        // verified build instead of requiring a tree tag for this change or
        // reading the live cluster.
        optional_images: ["ha-bridge", "ha-control"],
        workloads: ["scheduler"],
        namespace: "home-assistant",
        script: "projects/home/kubernetes/scheduler/deploy.sh",
        env_key: "scheduler_env",
        env_file: "scheduler.env",
        externalsecrets: ["scheduler-auth", "ha-bridge-ha"],
      },
      // The deployed service is the Rust crate; the former Go implementation
      // is deleted, and the golden fixtures under solutions/scheduler/testdata
      // are frozen data.
      bazel: {
        roots: [
          "//solutions/scheduler",
          "//packages/rust",
          "//projects/home/kubernetes/scheduler",
        ],
        test_targets: [
          "//solutions/scheduler/crates/scheduler:unit_tests",
          "//solutions/scheduler/crates/scheduler:contract_tests",
          "//solutions/scheduler/crates/scheduler:go_database_test",
          "//solutions/scheduler/crates/scheduler:clippy",
          "//solutions/scheduler/crates/scheduler:rustfmt",
        ],
        build_targets: [
          "//solutions/scheduler/...",
          "//projects/home/kubernetes/scheduler/...",
        ],
      },
    },

    {
      id: "wakegrant",
      platform: "bazel",
      title: "solutions/wakegrant (Rust one-time logon grants) + home deployment",
      paths: [
        "solutions/wakegrant/**",
        "projects/home/kubernetes/wakegrant/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // One image and one Deployment with a durable SQLite grant on a PVC; the
      // ESO Secret (issuer token + per-machine keys) is part of the same unit
      // because the service is unusable without it.
      deploy: {
        builder: "bazel",
        target_root: "//solutions/wakegrant",
        images: ["wakegrant"],
        workloads: ["wakegrant"],
        namespace: "home-assistant",
        script: "projects/home/kubernetes/wakegrant/deploy.sh",
        env_key: "wakegrant_env",
        env_file: "wakegrant.env",
        externalsecrets: ["wakegrant"],
      },
      // Rust service: host fmt/clippy/tests plus the static musl image. The
      // protocol and shared core crates live in //packages/rust.
      bazel: {
        roots: ["//solutions/wakegrant", "//packages/rust"],
        test_targets: [
          "//solutions/wakegrant/crates/wakegrant:unit_tests",
          "//solutions/wakegrant/crates/wakegrant:clippy",
          "//solutions/wakegrant/crates/wakegrant:rustfmt",
          "//packages/rust:clippy",
          "//packages/rust:rustfmt",
        ],
        build_targets: ["//solutions/wakegrant/..."],
      },
    },

    {
      id: "currency-bot",
      platform: "bazel",
      title: "solutions/currency-bot (Rust Telegram currency bot) + home deployment",
      paths: [
        "solutions/currency-bot/**",
        "projects/home/kubernetes/currency-bot/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // One image and one Deployment; the Cloudflare Tunnel sidecar is part of the
      // same unit because the webhook is unusable without it.
      deploy: {
        builder: "bazel",
        target_root: "//solutions/currency-bot",
        images: ["currency-bot"],
        workloads: ["currency-bot"],
        namespace: "currency-bot",
        script: "projects/home/kubernetes/currency-bot/deploy.sh",
        env_key: "currency_bot_env",
        env_file: "currency-bot.env",
        externalsecrets: ["currency-bot", "cloudflared-tunnel"],
      },
      // Rust service: host fmt/clippy/tests plus the static musl image. The Go
      // implementation is deleted; `testdata` stays as frozen contract data.
      bazel: {
        roots: ["//solutions/currency-bot", "//packages/rust"],
        test_targets: [
          "//solutions/currency-bot/crates/currency-bot:unit_tests",
          "//solutions/currency-bot/crates/currency-bot:clippy",
          "//solutions/currency-bot/crates/currency-bot:rustfmt",
          "//packages/rust:clippy",
          "//packages/rust:rustfmt",
        ],
        build_targets: ["//solutions/currency-bot/..."],
      },
    },

    {
      id: "mcp-gateway",
      platform: "bazel",
      title: "solutions/mcp-gateway (Rust MCP infra gateway) + home deployment",
      paths: [
        "solutions/mcp-gateway/**",
        // The plugin's strict per-method params schema is the gateway's wire
        // contract: a fixture change must retest the serializing side too.
        "solutions/bb-home-dispatcher/contract/params.json",
        "projects/home/kubernetes/mcp-gateway/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
        "platform/deploy-rs/**",
        // The shared clients and TLS roots this crate consumes.
        "packages/rust/home-clients/**",
        "packages/rust/home-telegram/**",
        "packages/rust/home-tls/**",
        "packages/rust/home-contracttest/**",
      ],
      // One image and one Deployment in home-assistant. The catalog ConfigMap,
      // RBAC, ExternalSecrets and NetworkPolicy are part of the same unit because
      // an image without its catalog or token is unusable.
      deploy: {
        builder: "bazel",
        target_root: "//solutions/mcp-gateway",
        images: ["mcp-gateway"],
        workloads: ["mcp-gateway"],
        namespace: "home-assistant",
        script: "projects/home/kubernetes/mcp-gateway/deploy.sh",
        env_key: "mcp_gateway_env",
        env_file: "mcp-gateway.env",
        externalsecrets: [
          "mcp-gateway-auth",
          "mcp-gateway-telegram",
          "mcp-gateway-ha",
          "mcp-gateway-bb-ssh",
          "mcp-gateway-voice",
        ],
      },
      // Rust service: host fmt/clippy/tests plus the static musl image. The Go
      // implementation is deleted; `testdata` stays as frozen contract data.
      bazel: {
        roots: ["//packages/rust", "//solutions/mcp-gateway"],
        test_targets: [
          "//packages/rust/home-clients:unit_tests",
          "//packages/rust/home-telegram:unit_tests",
          "//packages/rust:clippy",
          "//packages/rust:rustfmt",
          "//solutions/mcp-gateway/crates/mcp-gateway:unit_tests",
          "//solutions/mcp-gateway/crates/mcp-gateway:contract_tests",
          "//solutions/mcp-gateway/crates/mcp-gateway:voice_tests",
          "//solutions/mcp-gateway/crates/mcp-gateway:deploy_contract_tests",
          "//solutions/mcp-gateway/crates/mcp-gateway:clippy",
          "//solutions/mcp-gateway/crates/mcp-gateway:rustfmt",
        ],
        build_targets: [
          "//packages/rust/...",
          "//solutions/mcp-gateway/...",
        ],
      },
    },

    {
      id: "ha-bridge",
      platform: "bazel",
      title: "solutions/ha-bridge (Rust home executor + Rust ha-control)",
      paths: [
        "solutions/ha-bridge/**",
        // Direct dependencies of the Rust executors: a change here rebuilds
        // the unit even though the Bazel graph only sees it through its own
        // targets.
        "packages/go/**",
        "packages/proto/**",
        "packages/rust/**",
        // The deploy tooling that renders the shared scheduler manifests.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // The bridge unit owns ha-control too: they share the executor code.
      deploy: {
        builder: "bazel",
        target_root: "//projects/home/kubernetes/scheduler",
        images: ["ha-bridge", "ha-control"],
        // The shared script renders the scheduler too; this unit never applies
        // it, so its image comes from the newest verified build (see the
        // scheduler unit's comment).
        optional_images: ["scheduler"],
        workloads: ["ha-bridge", "ha-control"],
        namespace: "home-assistant",
        script: "projects/home/kubernetes/scheduler/deploy.sh",
        env_key: "scheduler_env",
        env_file: "scheduler.env",
        externalsecrets: ["scheduler-auth", "ha-bridge-ha"],
      },
      // Both deployed services are the Rust crate: ha-bridge and ha-control.
      // The former Go implementation is deleted; testdata/ holds frozen golden
      // vectors for the Rust tests.
      bazel: {
        roots: [
          "//solutions/ha-bridge",
          "//packages/rust",
          "//projects/home/kubernetes/scheduler",
        ],
        test_targets: [
          "//solutions/ha-bridge/crates/ha-bridge:unit_tests",
          "//solutions/ha-bridge/crates/ha-bridge:golden_tests",
          "//solutions/ha-bridge/crates/ha-bridge:clippy",
          "//solutions/ha-bridge/crates/ha-bridge:rustfmt",
          "//packages/rust:clippy",
          "//packages/rust:rustfmt",
        ],
        build_targets: [
          "//solutions/ha-bridge/...",
          "//projects/home/kubernetes/scheduler/...",
        ],
      },
    },

    {
      id: "ble-gateway",
      platform: "bazel",
      title: "projects/station/ble-gateway + packages/rust (Rust host build, tests, fmt, clippy)",
      paths: ["projects/station/ble-gateway/**", "packages/rust/**"],
      bazel: {
        roots: ["//projects/station/ble-gateway", "//packages/rust"],
        test_targets: [
          "//projects/station/ble-gateway:clippy",
          "//projects/station/ble-gateway:rustfmt",
          "//projects/station/ble-gateway/crates/ble-gateway:ble_gateway_unit_tests",
          "//packages/rust:clippy",
          "//packages/rust:rustfmt",
          "//packages/rust/home-config:unit_tests",
          "//packages/rust/home-contracttest:unit_tests",
          "//packages/rust/home-http:unit_tests",
          "//packages/rust/home-runtime:unit_tests",
          "//packages/rust/home-secret:unit_tests",
          "//packages/rust/home-sqlite:unit_tests",
          "//packages/rust/home-telemetry:unit_tests",
          "//packages/rust/wakegrant-protocol:protocol_tests"
        ],
        build_targets: [
          "//projects/station/ble-gateway/...",
          "//packages/rust/...",
        ],
      },
    },

    {
      id: "proto",
      platform: "bazel",
      title: "packages/proto (home.v1 schema; Rust code is generated into home-proto)",
      paths: ["packages/proto/**"],
      bazel: {
        roots: ["//packages/proto", "//packages/rust/home-proto"],
        test_targets: ["//packages/rust/home-proto:unit_tests"],
        build_targets: ["//packages/proto/..."],
      },
    },

    // ----------------------------------------------------------- Android --------
    // One unit per application; shared Kotlin libraries are their own units and
    // reach applications through the Bazel graph, not through path globs.
    // Android build tools (platform/android/tools) are Kotlin `kt_jvm_test`
    // targets; they are part of `android_tests`, so the Android app unit
    // exercises the toolchain it runs.
    // `android-app` also has the Android build inputs of PR App Distribution in
    // `ci.yml` (job `android-distribute`). That job follows `android_distribute`
    // from `affected.mjs`: real `projects/mobile/android/**`, `packages/kotlin/**`
    // or `platform/android/**` changes publish an APK; a global invalidation or a
    // CI-workflow edit only runs this unit's checks.
    {
      id: "android-app",
      platform: "android",
      title: "projects/mobile/android (Home client, dev.proflyder.home)",
      paths: ["projects/mobile/android/**"],
      bazel: {
        roots: ["//projects/mobile/android"],
        test_targets: [
          "//projects/mobile/android:android_tests",
          "//projects/mobile/android/app:main_activity_smoke_test",
        ],
        build_targets: ["//projects/mobile/android:app"],
        artifact: "bazel-bin/projects/mobile/android/app/app.apk",
        artifact_name: "android-app-debug",
      },
    },

    // Android Lint over the whole application graph is its own unit so a lint
    // finding fails only `android / android-lint` instead of aborting the app's
    // `bazel test` before the other tests report. It shares the application's
    // paths and Bazel root: every Android module source/resource change selects
    // it together with `android-app`, and `platform/android/**` (the lint rule)
    // invalidates it through the android platform rule. The unit declares the
    // SDK package that provides the standalone Lint CLI, so `_android.yml` only
    // installs it for jobs that need it.
    {
      id: "android-lint",
      platform: "android",
      title: "projects/mobile/android (Android Lint, whole app graph)",
      paths: ["projects/mobile/android/**"],
      bazel: {
        roots: ["//projects/mobile/android"],
        test_targets: ["//projects/mobile/android:android_lint"],
        sdk_packages: ["cmdline-tools;23.0"],
      },
    },

    {
      id: "kotlin-shared",
      platform: "android",
      title: "packages/kotlin (shared Kotlin libraries)",
      paths: ["packages/kotlin/**"],
      bazel: {
        roots: ["//packages/kotlin"],
        test_targets: [
          "//packages/kotlin/core:core_test",
          "//packages/kotlin/core:operation_test",
        ],
      },
    },

    // ------------------------------------------------------------- Bazel ------
    // Generic Bazel units on a Linux runner (no JDK/SDK). The Pulsar Rust
    // workspace is Linux-only, so on macOS the same targets are skipped by
    // target_compatible_with. This unit owns fmt/clippy/tests; the `pulsar`
    // native unit only cross-builds the station binaries and its side builds.
    {
      id: "pulsar-bazel",
      platform: "bazel",
      title: "projects/station/pulsar (Rust host build, tests, fmt, clippy)",
      paths: [
        "projects/station/pulsar/satellite/**",
        "projects/station/pulsar/installer/**",
      ],
      bazel: {
        roots: ["//projects/station/pulsar"],
        test_targets: [
          "//projects/station/pulsar/satellite:clippy",
          "//projects/station/pulsar/satellite:rustfmt",
          "//projects/station/pulsar/satellite/crates/...",
          "//projects/station/pulsar/installer:unit_tests",
          "//projects/station/pulsar/installer:clippy",
          "//projects/station/pulsar/installer:rustfmt",
        ],
        build_targets: ["//projects/station/pulsar/..."],
      },
    },

    // ------------------------------------------------------------ Python --------
    {
      id: "ha-voice",
      platform: "python",
      title: "projects/home-assistant/ha-voice",
      paths: ["projects/home-assistant/ha-voice/**"],
      python: {
        suites: ["projects/home-assistant/ha-voice"],
      },
    },

    {
      id: "hermes",
      platform: "python",
      title: "projects/home-assistant/hermes",
      paths: [
        "projects/home-assistant/hermes/**",
        // The Dockerfile, the pinned upstream `revision`, the reproducible
        // build wrapper and its verification script are this image's inputs.
        "projects/home-assistant/deploy/images/hermes/**",
        // The voice-proxy Rust solution and the shared crates it compiles into
        // the deployed binary. The `voice-proxy` Bazel unit runs the tests; this
        // unit owns the deploy. `packages/rust/**` is explicit because a
        // python unit has no Bazel reverse-dependency closure of its own, so a
        // shared-crate change never reaches the deploy unit otherwise.
        "solutions/voice-proxy/**",
        "packages/rust/**",
        // The runtime chart's Rust health exporter is part of this deploy unit.
        "projects/home-assistant/deploy/health-exporter/**",
        "projects/home-assistant/deploy/chart/**",
        // build-and-push.sh reads image digests with `home-deploy oci digest`.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // Hermes/voice-proxy/health-exporter: build+push, then promote the digests
      // into the runtime Helm release (`home-deploy hermes rollout`). `hermes`
      // is a Docker build; the other images are Bazel/rules_oci images named in
      // `bazel_images`; build-and-push.sh owns the split, so the workflow keeps
      // one deploy branch. The upgrade is skipped when the digests did not
      // change; the smoke checks must pass or the job rolls the release back to
      // the previous revision.
      deploy: {
        builder: "mixed",
        // image -> Bazel package of the `<image>_image`/`<image>_push` targets.
        // Images not listed here are Dockerfile builds.
        bazel_images: {
          "voice-proxy": "//solutions/voice-proxy",
          "health-exporter": "//projects/home-assistant/deploy/health-exporter",
          // The Assist smoke Job runs this binary, so no Python image is needed.
          "home-deploy": "//projects/home/kubernetes/scheduler",
        },
        images: ["hermes", "voice-proxy", "health-exporter", "home-deploy"],
        workloads: ["hermes"],
        namespace: "home-assistant",
        script: "projects/home-assistant/deploy/rollout.sh",
        env_key: "runtime_values",
        env_file: "runtime-values.yaml",
        externalsecrets: ["hermes-environment"],
      },
      python: {
        suites: ["projects/home-assistant/hermes"],
      },
    },

    {
      id: "ha-stack",
      platform: "python",
      title: "projects/home-assistant/stack",
      paths: ["projects/home-assistant/stack/**"],
      python: {
        suites: ["projects/home-assistant/stack"],
      },
    },

    {
      id: "ha-deploy",
      platform: "python",
      title: "projects/home-assistant/deploy",
      paths: ["projects/home-assistant/deploy/**"],
      python: {
        suites: ["projects/home-assistant/deploy"],
      },
    },

    {
      id: "proxmox",
      platform: "python",
      title: "projects/home/proxmox (+ vpn, wifi-uplink)",
      paths: ["projects/home/proxmox/**"],
      python: {
        suites: [
          "projects/home/proxmox",
          "projects/home/proxmox/vpn",
          "projects/home/proxmox/wifi-uplink",
        ],
      },
    },

    {
      id: "windows-pc",
      platform: "python",
      title: "projects/home/windows-pc (komp power) + logon provider contracts",
      paths: [
        "projects/home/windows-pc/**",
        // The reusable provider solution ships its own hermetic suite (static
        // contract checks for the elevated install/set-secret/uninstall/status
        // scripts); the native unit builds it, this unit checks the scripts.
        "solutions/windows-logon-provider/**",
      ],
      python: {
        suites: ["projects/home/windows-pc", "solutions/windows-logon-provider"],
      },
    },

    {
      id: "home-backup",
      platform: "python",
      title: "projects/home/backup (offsite restic + PVE vzdump metrics)",
      paths: ["projects/home/backup/**"],
      python: {
        suites: ["projects/home/backup/offsite", "projects/home/backup/pve"],
      },
    },

    {
      id: "home-observability",
      platform: "python",
      title: "projects/home/kubernetes/observability (backup alerts)",
      paths: ["projects/home/kubernetes/observability/**"],
      python: {
        suites: ["projects/home/kubernetes/observability"],
      },
    },

    {
      id: "station-scripts",
      platform: "python",
      title: "projects/station/scripts",
      paths: ["projects/station/scripts/**"],
      python: {
        suites: ["projects/station/scripts"],
      },
    },

    {
      id: "station-tools",
      platform: "python",
      title: "projects/station/tools (voice-recorder, wakeword-personalize, root kit)",
      paths: ["projects/station/tools/**"],
      python: {
        suites: [
          "projects/station/tools/voice-recorder",
          "projects/station/tools/wakeword-personalize",
        ],
        compile: ["projects/station/tools/quasar-root-kit"],
      },
    },

    {
      id: "home-e2e",
      platform: "python",
      title: "projects/home/e2e (local process smoke, read-only)",
      paths: ["projects/home/e2e/**"],
      python: {
        compile: ["projects/home/e2e"],
      },
    },

    // Live-only scripts: no hermetic tests exist, so these units run a compile
    // gate and are kept so a syntax error still fails CI.
    {
      id: "ha-e2e",
      platform: "python",
      title: "projects/home-assistant/e2e (live HA/Hermes scripts)",
      paths: ["projects/home-assistant/e2e/**"],
      python: {
        compile: ["projects/home-assistant/e2e"],
      },
    },

    {
      id: "ha-tools",
      platform: "python",
      title: "projects/home-assistant/tools (tts-lab)",
      paths: ["projects/home-assistant/tools/**"],
      python: {
        compile: ["projects/home-assistant/tools/tts-lab"],
      },
    },

    {
      id: "station-e2e",
      platform: "python",
      title: "projects/station/e2e (live gateway checks)",
      paths: ["projects/station/e2e/**"],
      python: {
        compile: ["projects/station/e2e"],
      },
    },

    {
      id: "scheduler-deploy",
      platform: "python",
      title: "projects/home/kubernetes/scheduler (deploy control regression)",
      paths: ["projects/home/kubernetes/scheduler/**"],
      python: {
        // The remaining Python contract test is the shell regression driver;
        // converge/state/preflight live in platform/deploy (Go, unit above).
        compile: ["projects/home/kubernetes/scheduler"],
      },
    },

    {
      id: "currency-bot-deploy",
      platform: "python",
      title: "projects/home/kubernetes/currency-bot (deploy script contract)",
      paths: ["projects/home/kubernetes/currency-bot/**"],
      python: {
        suites: ["projects/home/kubernetes/currency-bot"],
        // The suite drives deploy.sh, which runs the home-deploy binary.
        deploy_tool: true,
      },
    },

    {
      id: "wakegrant-deploy",
      platform: "python",
      title: "projects/home/kubernetes/wakegrant (deploy/manifest contract)",
      paths: ["projects/home/kubernetes/wakegrant/**"],
      python: {
        suites: ["projects/home/kubernetes/wakegrant"],
        // The suite drives deploy.sh, which runs the home-deploy binary.
        deploy_tool: true,
      },
    },

    {
      id: "mcp-gateway-deploy",
      platform: "python",
      title: "projects/home/kubernetes/mcp-gateway (catalog/RBAC/deploy contract)",
      paths: [
        // The catalog's schema is checked by the Rust deploy-contract test in
        // the mcp-gateway unit (real loader); this unit owns the catalog↔RBAC
        // correspondence, manifest hardening and the deploy-script/render
        // contract.
        "projects/home/kubernetes/mcp-gateway/**",
      ],
      python: {
        suites: ["projects/home/kubernetes/mcp-gateway"],
        // The suite drives deploy.sh, which runs the home-deploy binary.
        deploy_tool: true,
      },
    },

    {
      id: "kubernetes-ingress",
      platform: "python",
      title: "projects/home/kubernetes/ingress (Traefik/cert-manager templates)",
      paths: ["projects/home/kubernetes/ingress/**"],
      python: {
        compile: ["projects/home/kubernetes/ingress"],
      },
    },

    {
      id: "openbao",
      platform: "python",
      title: "projects/home/kubernetes/openbao (config, CI OIDC secret hand-off)",
      paths: ["projects/home/kubernetes/openbao/**"],
      python: {
        suites: ["projects/home/kubernetes/openbao"],
      },
    },

    {
      id: "workstation",
      platform: "python",
      title: "platform/workstation (mise tasks, OpenBao secrets CLI, catalog)",
      paths: ["platform/workstation/**", "docs/SECRETS.md"],
      python: {
        suites: ["platform/workstation"],
      },
    },

    {
      id: "vaultwarden",
      platform: "python",
      title: "projects/home/kubernetes/vaultwarden (password manager, admin deploy)",
      paths: [
        "projects/home/kubernetes/vaultwarden/**",
        "projects/home/kubernetes/cloudflare_dns.py",
      ],
      python: {
        suites: ["projects/home/kubernetes/vaultwarden"],
      },
    },

    {
      id: "external-secrets",
      platform: "python",
      title: "projects/home/kubernetes/external-secrets (OpenBao wiring)",
      paths: [
        "projects/home/kubernetes/external-secrets/**",
        "projects/home/kubernetes/scheduler/manifests/external-secrets.yaml",
        "projects/home-assistant/deploy/manifests/external-secrets.yaml",
        "projects/home/kubernetes/ingress/manifests/external-secret-cloudflare.yaml",
        "projects/home/kubernetes/observability/manifests/external-secret-grafana-admin.yaml",
        "projects/home/kubernetes/observability/manifests/external-secret-alertmanager-telegram.yaml",
        "projects/home/kubernetes/observability/alertmanager/**",
        "projects/home/kubernetes/currency-bot/manifests/external-secrets.yaml",
        "projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml",
        "projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml",
        "projects/home/kubernetes/vaultwarden/manifests/external-secrets.yaml",
      ],
      python: {
        suites: ["projects/home/kubernetes/external-secrets"],
      },
    },

    // ---------------------------------------------------------------- JS --------
    // Edge Workers only: Cloudflare runtime, plain ES modules, no npm
    // dependencies and no build step. Tests use the built-in `node --test`.
    {
      id: "heartbeat",
      platform: "js",
      title: "solutions/heartbeat (Cloudflare Worker dead-man switch)",
      paths: ["solutions/heartbeat/**", "projects/home/cloudflare/heartbeat/**"],
      js: {
        tests: ["solutions/heartbeat/src"],
      },
    },

    // bb server plugin. It runs inside bb (not an edge Worker), but the test
    // surface is the same: plain ES modules, Node built-ins only, `node --test`
    // and no npm install step in CI.
    {
      id: "bb-home-dispatcher",
      platform: "js",
      title: "solutions/bb-home-dispatcher (bb home dispatcher plugin)",
      paths: ["solutions/bb-home-dispatcher/**"],
      js: {
        tests: ["solutions/bb-home-dispatcher/tests"],
      },
    },

    // bb profiles plugin: TypeScript server/UI with a dependency-free policy
    // module. CI runs the same `node --test` suite as production, so no npm
    // install or external secret is needed.
    {
      id: "bb-profiles",
      platform: "js",
      title: "solutions/bb-profiles (bb work/personal profiles plugin)",
      paths: ["solutions/bb-profiles/**"],
      js: {
        tests: ["solutions/bb-profiles/tests"],
      },
    },

    // ----------------------------------------------------------- Native --------
    // Native units: the Windows logon provider is a repository-script Bazel
    // cross-build (rules_rust + hermetic zig cc_toolchain); Pulsar's station
    // daemon and installer are cross-built with Bazel, radio/ssh use their
    // own scripts. A same-repo PR publishes the
    // release-shaped OCI artifact under `tree-<hash>` with provenance; a push
    // to `develop` promotes that digest to the release tags (deploy-home) or
    // rebuilds on a missing/unverified tree tag (DECISIONS 36).
    //
    // `publish_when` narrows publication to a release trigger: the unit is
    // still built and tested by PR CI on every path change, but deploy-home
    // builds/publishes it only when one of these paths changed. Without it a
    // unit is publishable on any affected change. `publish_script` owns the
    // registry contract; `publish_attest_glob` is the artifact file (or glob)
    // whose build provenance is attested next to the pushed manifest.
    //
    // `publish_repository` is the artifact's full GHCR reference and
    // `publish_tags` are the tags deploy-home attaches when it promotes the
    // PR-built `tree-<hash>` digest (DECISIONS 36). `{version}` and
    // `{commit12}` expand from the deployed VERSION and commit. A PR publishes
    // the same artifact under `tree-<hash>` only.
    {
      id: "logon-provider",
      platform: "native",
      title: "solutions/windows-logon-provider (Windows credential provider)",
      // The Windows target platform and the hermetic zig toolchain are part of
      // the build contract: changing either must rebuild and verify the unit.
      // Publication is gated by VERSION below.
      paths: [
        "solutions/windows-logon-provider/**",
        "platform/bazel/windows/**",
        "platform/bazel/zig/**",
        "MODULE.bazel",
      ],
      native: {
        workdir: "solutions/windows-logon-provider",
        kind: "script",
        test: "./run-tests.sh",
        build: "./build.sh",
        artifacts: [
          "out/RicardoKompLogon.dll",
          "out/cp-selftest.exe",
        ],
        checksums: "out/SHA256SUMS",
        publish_script: "solutions/windows-logon-provider/publish-oci.sh",
        publish_repository: "ghcr.io/proflyder/windows-logon-provider",
        // Only a VERSION change is a release. Every other source or toolchain
        // change is still built, tested and verified by PR CI.
        publish_when: ["solutions/windows-logon-provider/VERSION"],
        publish_tags: ["{version}", "sha-{commit12}"],
        publish_attest_glob: "RicardoKompLogon.dll",
      },
    },

    // The Rust station daemon and its surrounding builds (radio firmware,
    // Rust installer, dropbear). Both station ABIs are built with Bazel; host
    // fmt/clippy/tests are the `pulsar-bazel` unit. Any Pulsar change is PR CI;
    // only a VERSION change publishes the release OCI artifact, and
    // deploy-home's `deploy-station` job then rolls it out through HA.
    {
      id: "pulsar",
      platform: "native",
      title: "projects/station/pulsar (Rust station daemon, radio, installer)",
      paths: ["projects/station/pulsar/**"],
      native: {
        kind: "pulsar",
        workdir: "projects/station/pulsar",
        // The version is the release trigger and the artifact tag.
        publish_when: ["projects/station/pulsar/VERSION"],
        publish_script: "projects/station/pulsar/publish-oci.sh",
        publish_repository: "ghcr.io/proflyder/pulsar-firmware",
        // `latest` moves with every promoted version; the version tag itself
        // is immutable by content (identical layers are a no-op).
        publish_tags: ["{version}", "latest"],
        publish_attest_glob: "*",
      },
    },

    // ----------------------------------------------------------- Quality --------
    // Always runs: repository audit, static gates, shellcheck, secret scan and
    // the CI unit-map invariant check.
    {
      id: "repository",
      platform: "quality",
      title: "repository-wide gates",
      paths: [
        "platform/**",
        "docs/**",
        ".github/**",
        "Makefile",
        "requirements-test.txt",
        ".gitleaks.toml",
        "**/*.sh",
        // Infrastructure bootstrap helpers written in Python; the shell
        // entrypoints that call them are covered by `**/*.sh` above.
        "projects/home/kubernetes/openbao/configure.py",
      ],
    },
  ],
};

export default config;
