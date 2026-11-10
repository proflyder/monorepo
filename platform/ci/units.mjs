// CI unit map: the single source of truth for "what runs when".
//
// A *unit* is the smallest thing CI can run on its own: one service, one
// application, one shared library or one test suite directory. Each unit maps
// 1:1 to a matrix entry of one platform workflow (`.github/workflows/_*.yml`).
//
// Routing is computed by `platform/ci/affected.mjs`:
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
  // (Bazel reverse dependencies), not through path globs.
  //
  // `deploy: false` (default true) keeps the invalidation for CI only:
  // `deploy-home` never redeploys a unit selected *solely* by such a path. Workflow
  // and CI-harness edits must exercise the checks, but they do not change runtime
  // code and must not roll the home units.
  //
  // `MODULE.bazel` is global by default, with three scoped exceptions
  // (`#maven` also covers rules_android/rules_android_ndk, the SDK/NDK
  // extensions and `register_toolchains("@androidndk//...")`)
  // (`affected.mjs` rewrites the changed path, fail safe is global):
  //   * Maven-only diff -> `MODULE.bazel#maven` -> android.
  //   * rules_rust/crate_universe, zig toolchains, rules_rust Windows patches
  //     and the station's `_SENDSPIN_BAZEL` http_archive()s ->
  //     `MODULE.bazel#native` -> native units (station/provider cross-build)
  //     plus the `pulsar-bazel` host checks (same Rust crates on the host).
  //   * a mix of both -> both virtual paths (union of the platforms).
  // Anything else in MODULE.bazel - bazel_dep, oci, python -
  // and an unparseable file, a missing base or an added/deleted file keep the
  // global rule below.
  //
  // `.bazelrc` is global unless its diff only changes cache/output flags
  // (`--disk_cache`, the disk-cache GC, `--repository_cache`, console output);
  // then `affected.mjs` rewrites it to `.bazelrc#cache`, which matches no unit
  // and runs only the always-on quality job. Android-only flags are not global:
  // they live in `platform/android/android.bazelrc`, which the root rc imports
  // and Android tasks activate with `--config=android`; the file is covered by
  // the `platform/android/**` invalidation below, so a flag change reruns only
  // the Android units. `platform/bazel/zig/**` and
  // `platform/bazel/windows/**` become `platform/bazel#native`: they are
  // target-constrained toolchains/patches for the native station and the
  // Windows provider, never for the host Rust/images/Android builds.
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
        // The mise task entry points (build/test/check/units) are the local
        // front door for every unit; a broken task must fail PR CI.
        "platform/mise-tasks/**",
        "platform/workstation/bin/**",
        // fnox.toml feeds every admin:* entry point (secret references only).
        "fnox.toml",
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
    // Android platform inputs: the Bazel rules, the lint rule and
    // `platform/android/android.bazelrc` (Android-only flags activated with
    // `--config=android`). Never a global invalidation.
    {
      paths: ["platform/android/**"],
      platforms: ["android"],
    },
    // The compiler toolchain changes how every first-party Rust crate is
    // built, so it stays a Rust-wide invalidation.
    {
      paths: ["rust-toolchain.toml"],
      platforms: ["native", "bazel"],
    },
    {
      paths: ["requirements-test.txt"],
      platforms: ["python", "quality"],
    },
    // A reusable workflow change must run the jobs it defines, otherwise a broken
    // workflow passes PR CI unexercised.
    {
      paths: [".github/workflows/ci.yml"],
      platforms: ["*"],
      deploy: false,
    },
    {
      // The unit job owns image builds and attestation; a change must exercise
      // the image-owning bazel and python units.
      paths: [".github/workflows/_unit.yml"],
      platforms: ["bazel", "python"],
      deploy: false,
    },
    {
      paths: [".github/workflows/_deploy-control.yml"],
      platforms: ["*"],
      deploy: false,
    },
    {
      paths: [".github/workflows/_android.yml"],
      platforms: ["android"],
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
      paths: [".github/workflows/**", ".github/actions/**", "platform/ci/**"],
      platforms: ["quality"],
      deploy: false,
    },
  ],

  // The scheduler deploy-control regression is expensive and only meaningful for
  // the scheduler/bridge binaries, the Kubernetes deployment manifests and the
  // deploy CLI the script drives (a deploy-rs change must not skip it).
  deploy_control: {
    units: ["scheduler", "ha-bridge"],
    paths: [
      "solutions/scheduler/**",
      "projects/home/kubernetes/scheduler/**",
      "platform/deploy-rs/**",
      "platform/deploy/**",
    ],
  },

  units: [
    // ------------------------------------------------------- Deploy tooling -----
    {
      id: "platform-deploy",
      platform: "bazel",
      title: "platform/deploy-rs (home-deploy CLI: render, env, oci, converge, state, preflight, hermes)",
      // The Rust crate is the executable contract. Every deploy script and the
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
        // Shared Rust crates reach this unit through the Bazel reverse
        // dependencies, not as a path glob: only a crate the scheduler
        // actually imports selects it.
        "packages/proto/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // `deploy-home` pins exactly these images when the unit is affected and
      // Flux rolls the Deployments from the pins branch. Infrastructure
      // (ingress, observability, runtime chart) has no deploy block on purpose:
      // it is not auto-deployed.
      deploy: {
        builder: "bazel",
        // Flux owns the executors (решение 54): Kustomization
        // `app-ha-executors` applies scheduler, ha-bridge, ha-control and
        // ha-scenario from the digest the `pins` job commits. `deploy.sh`
        // remains the admin tool for mode drills and manual cutover.
        target_root: "//projects/home/kubernetes/scheduler",
        images: ["scheduler", "home-deploy"],
        // The shared scheduler deploy script also renders ha-bridge, ha-control
        // and ha-scenario (it never applies them for this unit). They are not
        // this unit's artifact, so deploy-home resolves them from the newest
        // verified build instead of requiring a tree tag for this change or
        // reading the live cluster.
        optional_images: ["ha-bridge", "ha-control", "ha-scenario"],
        workloads: ["scheduler"],
        namespace: "home-assistant",
      },
      // `deploy.sh` records every Deployment it is about to change and marks it
      // applied only after a successful converge. Under Flux the rollback is a
      // revert of the pins commit (manifests stay in git).
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed SCHEDULER_IMAGE; " +
          "Flux applies the previous digest within 1-2m. For a windowed drill " +
          "use `flux suspend kustomization app-ha-executors` plus deploy.sh.",
        source: "DECISIONS 54/55 and projects/home/kubernetes/flux/README.md",
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
        // The K8s package also builds ha-bridge/ha-control images; a shared
        // crate that reaches those must not select the scheduler unit. The
        // scheduler image is selected through its own solution subtree.
        routing_roots: ["//solutions/scheduler"],
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
        // Flux owns wakegrant since the pilot cutover (решение 54):
        // Kustomization/app-wakegrant applies the digest from the
        // `deploy/home-apps` pins branch. deploy-home only commits the verified
        // digest through its `pins` job and never applies the workload.
        target_root: "//solutions/wakegrant",
        images: ["wakegrant"],
        workloads: ["wakegrant"],
        namespace: "home-assistant",
      },
      // Flux applies the digest and reports readiness through the
      // Kustomization `healthChecks`; the documented rollback is a revert of
      // the pins commit (manifests stay in `develop`).
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed WAKEGRANT_IMAGE; " +
          "Flux applies the previous digest within 1-2m. `suspend: true` plus a " +
          "pins revert, never `kubectl rollout undo`.",
        source: "DECISIONS 54 and projects/home/kubernetes/flux/README.md",
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
      id: "intercom",
      platform: "bazel",
      title: "solutions/intercom (Rust WebRTC signaling) + home deployment",
      paths: [
        "solutions/intercom/**",
        "projects/home/kubernetes/intercom/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // One stateless image and Deployment; the ESO Secret
      // (k8s/home-assistant/intercom: phone_token, station_token) is part of
      // the same unit because the service is unusable without it.
      deploy: {
        builder: "bazel",
        // Flux owns intercom since the решение 54 migration:
        // Kustomization/app-intercom applies the digest from the pins branch.
        target_root: "//solutions/intercom",
        images: ["intercom"],
        workloads: ["intercom"],
        namespace: "home-assistant",
      },
      rollback: {
        kind: "flux-pins",
        note:
          "Stateless: revert the `deploy/home-apps` commit that changed " +
          "INTERCOM_IMAGE and let Flux re-apply the previous digest.",
        source: "DECISIONS 54 and projects/home/kubernetes/intercom/README.md",
      },
      bazel: {
        roots: [
          "//solutions/intercom",
          "//packages/rust",
          "//projects/home/kubernetes/intercom",
        ],
        test_targets: [
          "//solutions/intercom/crates/intercom:unit_tests",
          "//solutions/intercom/crates/intercom:integration_tests",
          "//solutions/intercom/crates/intercom:clippy",
          "//solutions/intercom/crates/intercom:rustfmt",
        ],
        build_targets: [
          "//solutions/intercom/...",
          "//projects/home/kubernetes/intercom/...",
        ],
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
        // Flux owns currency-bot since решение 54:
        // Kustomization/app-currency-bot applies the digest from the pins
        // branch. The `currency-bot` namespace stays admin-owned.
        target_root: "//solutions/currency-bot",
        images: ["currency-bot"],
        workloads: ["currency-bot"],
        namespace: "currency-bot",
      },
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed CURRENCY_BOT_IMAGE; " +
          "Flux re-applies the previous digest. `cloudflared` is pinned by a " +
          "reviewed versions.sh literal, so roll it back with a PR.",
        source: "DECISIONS 54 and projects/home/kubernetes/currency-bot/README.md",
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
        // Flux owns mcp-gateway since решение 54:
        // Kustomization/app-mcp-gateway renders the catalog ConfigMap and
        // applies the digest from the pins branch.
        target_root: "//solutions/mcp-gateway",
        images: ["mcp-gateway"],
        workloads: ["mcp-gateway"],
        namespace: "home-assistant",
      },
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed MCP_GATEWAY_IMAGE; " +
          "Flux re-applies the previous digest and the catalog from git.",
        source: "projects/home/kubernetes/mcp-gateway/README.md (Rollback)",
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
      title: "solutions/ha-bridge (Rust home executor, ha-control and ha-scenario)",
      paths: [
        "solutions/ha-bridge/**",
        // Direct dependencies of the Rust executors: a change here rebuilds
        // the unit even though the Bazel graph only sees it through its own
        // targets.
        "packages/proto/**",
        // Shared crates reach this unit through the Bazel graph; only a crate
        // the bridge/control targets import selects it. The deploy tooling that
        // renders the shared scheduler manifests is a direct path.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // The bridge unit owns ha-control and ha-scenario too: they share the
      // executor code.
      deploy: {
        builder: "bazel",
        // Flux owns ha-bridge/ha-control/ha-scenario since решение 54:
        // the single Kustomization/app-ha-executors applies the digests from
        // the pins branch. `deploy.sh` remains the admin tool for mode drills.
        target_root: "//projects/home/kubernetes/scheduler",
        images: ["ha-bridge", "ha-control", "ha-scenario"],
        // The shared script renders the scheduler too; this unit never applies
        // it, so its image comes from the newest verified build (see the
        // scheduler unit's comment).
        optional_images: ["scheduler"],
        workloads: ["ha-bridge", "ha-control", "ha-scenario"],
        namespace: "home-assistant",
      },
      // Under Flux the rollback is a pins revert; executor modes/replicas are
      // Git state, so a drill uses `flux suspend` plus deploy.sh.
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed HA_BRIDGE_IMAGE / " +
          "HA_CONTROL_IMAGE / HA_SCENARIO_IMAGE. Mode changes are a Git change " +
          "plus `flux suspend kustomization app-ha-executors` for a drill.",
        source: "DECISIONS 54 and projects/home/kubernetes/scheduler/README.md",
      },
      // All deployed services are the Rust crate: ha-bridge, ha-control and
      // ha-scenario.
      // The former Go implementation is deleted; testdata/ holds frozen golden
      // vectors for the Rust tests.
      bazel: {
        roots: [
          "//solutions/ha-bridge",
          "//packages/rust",
          "//projects/home/kubernetes/scheduler",
        ],
        // The bridge/control binaries and images live under //solutions; the
        // K8s package is shared with the scheduler image and must not make a
        // scheduler-only label select the bridge.
        routing_roots: ["//solutions/ha-bridge"],
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
      paths: ["projects/station/ble-gateway/**"],
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
    // `android-app` is also where the distributable APK is built and tested
    // once (`_android.yml`, no secrets) and uploaded unsigned as
    // `android-app-apk`. The signing + Firebase upload job `android-distribute`
    // follows `android_distribute` from
    // `affected.mjs`: real `projects/mobile/android/**`, `packages/kotlin/**`
    // or `platform/android/**` changes publish an APK; a global invalidation or
    // a CI-workflow edit only runs this unit's checks.
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
        // Owns the distributable APK inside `_android.yml`; selected whenever
        // distribution is selected (see affected.mjs matrixEntries).
        release: true,
        // No generic artifact: the `android-app` entry of `_android.yml` builds
        // and tests the distributable APK itself and uploads it as
        // `android-app-apk`; `android-distribute` signs it.
        // The APK targets //platform/android:android_arm64; rules_android
        // resolves a C++ toolchain for that platform, which comes from the NDK
        // (`rules_android_ndk` in MODULE.bazel). The exact version is pinned
        // here so CI installs the same NDK as the workstation.
        sdk_packages: ["ndk;28.2.13676358"],
      },
    },

    // Android Lint over the whole application graph is its own unit so a lint
    // finding fails only `android / android-lint` instead of aborting the app's
    // `bazel test` before the other tests report. It shares the application's
    // paths and Bazel root: every Android module source/resource change selects
    // it together with `android-app`, and `platform/android/**` (the lint rule)
    // invalidates it through the android platform rule. The unit declares the
    // SDK packages it needs that the workflow does not install unconditionally:
    // the standalone Lint CLI and the NDK C++ toolchain for the app's arm64
    // target platform, so `_android.yml` only installs them where needed.
    {
      id: "android-lint",
      platform: "android",
      title: "projects/mobile/android (Android Lint, whole app graph)",
      paths: ["projects/mobile/android/**"],
      bazel: {
        roots: ["//projects/mobile/android"],
        test_targets: ["//projects/mobile/android:android_lint"],
        sdk_packages: ["cmdline-tools;23.0", "ndk;28.2.13676358"],
      },
    },

    // detekt (built-in rules, formatting and the first-party `home-rules`
    // plug-in) over the same tree. A separate unit like Android Lint so a
    // finding fails only `android / android-detekt`.
    {
      id: "android-detekt",
      platform: "android",
      title: "projects/mobile/android (detekt, whole app graph)",
      paths: ["projects/mobile/android/**"],
      bazel: {
        roots: ["//projects/mobile/android"],
        test_targets: ["//projects/mobile/android:detekt"],
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
        // unit owns the deploy. Shared `packages/rust` crates are routed through
        // the Bazel graph (the unit declares its image roots below), so only a
        // crate that voice-proxy or the health exporter imports selects it.
        "solutions/voice-proxy/**",
        // The runtime chart's Rust health exporter is part of this deploy unit.
        "projects/home-assistant/deploy/health-exporter/**",
        "projects/home-assistant/deploy/chart/**",
        // build-and-push.sh reads image digests with `home-deploy oci digest`.
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      // The deploy unit has no Bazel closure of its own, but it owns two Bazel
      // images. A shared crate that reaches either image must also select it
      // (the health exporter pulls home-tls, voice-proxy pulls home-ha).
      bazel: {
        roots: [
          "//solutions/voice-proxy",
          "//projects/home-assistant/deploy/health-exporter",
        ],
      },
      // Hermes/voice-proxy/health-exporter: build+push, then promote the digests
      // into the runtime Helm release (`home-deploy hermes rollout`). `hermes`
      // is a Docker build; the other images are Bazel/rules_oci images named in
      // `bazel_images`; build-and-push.sh owns the split, so the workflow keeps
      // one deploy branch. The upgrade is skipped when the digests did not
      // change; the smoke checks must pass or the job rolls the release back to
      // the previous revision.
      deploy: {
        builder: "mixed",
        // Flux owns the release since решение 54: HelmRelease/runtime
        // updates the chart from git, images from the pins branch, and the
        // post-upgrade smoke hook plus remediation replace `hermes rollout`.
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
      },
      // `HelmRelease/runtime` rolls back automatically to the previous
      // revision when the upgrade or the post-upgrade smoke hook fails; the
      // documented manual emergency path stays `helm rollback runtime`.
      rollback: {
        kind: "hermes",
        release: "runtime",
        namespace: "home-assistant",
        source: "docs/KUBERNETES-OPERATIONS.md (Автодеплой Hermes / voice-proxy / health exporter)",
        note:
          "Flux: `HelmRelease/runtime` uses upgrade.remediation.strategy " +
          "rollback (smoke hook failure included). Manual emergency path: " +
          "`helm rollback runtime <revision>` or a `deploy/home-apps` revert.",
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
      // HA/MA updates are pulled by `ctl.sh update`; the documented recovery
      // path is a Supervisor/Proxmox backup restore, not an image rollback.
      rollback: {
        kind: "forward-only",
        note:
          "Update pulls newer HA/MA images; restore from the HA Supervisor " +
          "backup or Proxmox vzdump instead of rolling images back.",
        source: "docs/OPERATIONS.md (Backup), projects/home-assistant/stack/ctl.sh",
      },
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
      title: "projects/station/tools (voice-recorder, wakeword-personalize, root kit, station-audit)",
      paths: ["projects/station/tools/**", "projects/station/manifest.yaml"],
      python: {
        suites: [
          // The station manifest and the read-only audit that reconciles it.
          "projects/station/tools",
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
        // converge/state/preflight live in platform/deploy-rs (unit above).
        compile: ["projects/home/kubernetes/scheduler"],
      },
    },

    {
      id: "wakegrant-deploy",
      platform: "python",
      title: "projects/home/kubernetes/wakegrant (manifest contract)",
      paths: ["projects/home/kubernetes/wakegrant/**"],
      python: {
        suites: ["projects/home/kubernetes/wakegrant"],
      },
    },

    {
      id: "mcp-gateway-deploy",
      platform: "python",
      title: "projects/home/kubernetes/mcp-gateway (catalog/RBAC/manifest contract)",
      paths: [
        // The catalog's schema is checked by the Rust deploy-contract test in
        // the mcp-gateway unit (real loader); this unit owns the catalog↔RBAC
        // correspondence and manifest hardening.
        "projects/home/kubernetes/mcp-gateway/**",
      ],
      python: {
        suites: ["projects/home/kubernetes/mcp-gateway"],
      },
    },

    {
      id: "kubernetes-ingress",
      platform: "python",
      title: "projects/home/kubernetes/ingress (Traefik/cert-manager templates)",
      paths: ["projects/home/kubernetes/ingress/**"],
      python: {
        suites: ["projects/home/kubernetes/ingress"],
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
      id: "nexus-deploy",
      platform: "python",
      title: "projects/home/kubernetes/nexus (artifact cache manifests/provisioning contract)",
      paths: ["projects/home/kubernetes/nexus/**"],
      python: {
        suites: ["projects/home/kubernetes/nexus"],
      },
    },

    {
      id: "flux-deploy",
      platform: "python",
      title: "projects/home/kubernetes/flux (GitOps toolkit/bootstrap/substitution contract)",
      // The suite renders the Flux bundle *and* the Nexus Kustomizations it
      // reconciles, so a Nexus manifest change exercises the GitOps contract.
      paths: [
        "projects/home/kubernetes/flux/**",
        "projects/home/kubernetes/nexus/**",
        // The suite renders every component that a Flux Kustomization applies.
        "projects/home/kubernetes/external-secrets/**",
        "projects/home/kubernetes/ingress/**",
        "projects/home/kubernetes/observability/**",
        "projects/home/kubernetes/vaultwarden/**",
      ],
      python: {
        suites: ["projects/home/kubernetes/flux"],
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
        "projects/home/kubernetes/ingress/routes/external-secret-cloudflare.yaml",
        "projects/home/kubernetes/observability/manifests/external-secret-grafana-admin.yaml",
        "projects/home/kubernetes/observability/manifests/external-secret-alertmanager-telegram.yaml",
        "projects/home/kubernetes/observability/alertmanager/**",
        "projects/home/kubernetes/currency-bot/manifests/external-secrets.yaml",
        "projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml",
        "projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml",
        "projects/home/kubernetes/vaultwarden/manifests/external-secrets.yaml",
        "projects/home/kubernetes/nexus/manifests/external-secrets.yaml",
        "projects/home/kubernetes/flux/external-secrets.yaml",
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
    // to `develop` only promotes that digest to the release tags (deploy-home)
    // and fails on a missing/unverified tree tag (DECISIONS 36). The deploy
    // path never builds or publishes.
    //
    // `publish_when` narrows publication to a release trigger: the unit is
    // still built and tested by PR CI on every path change, but deploy-home
    // promotes it only when one of these paths changed. Without it a unit is
    // releasable on any affected change. `publish_script` owns the registry
    // contract; `publish_attest_glob` is the artifact file (or glob)
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
      // The provider is a Bazel cross-build, so a shared crate it imports
      // (`wakegrant-protocol` today) selects the unit through the same reverse
      // dependencies as the host units.
      bazel: {
        roots: ["//solutions/windows-logon-provider"],
      },
      native: {
        workdir: "solutions/windows-logon-provider",
        kind: "script",
        test: "./run-tests.sh",
        build: "./build.sh",
        artifacts: [
          "out/HomeKompLogon.dll",
          "out/cp-selftest.exe",
        ],
        checksums: "out/SHA256SUMS",
        publish_script: "solutions/windows-logon-provider/publish-oci.sh",
        publish_repository: "ghcr.io/proflyder/windows-logon-provider",
        // Only a VERSION change is a release. Every other source or toolchain
        // change is still built, tested and verified by PR CI.
        publish_when: ["solutions/windows-logon-provider/VERSION"],
        publish_tags: ["{version}", "sha-{commit12}"],
        publish_attest_glob: "HomeKompLogon.dll",
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
      // The station binaries and installer are Bazel cross-builds. A shared
      // crate they import (home-runtime, home-telemetry, ...) selects this unit
      // through the Bazel graph, without matching the radio's Makefile build.
      bazel: {
        roots: ["//projects/station/pulsar"],
      },
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
      // The station binary is rolled back on-device only, never through CI:
      // the updater offers newer versions only, so the operator swaps the
      // `pulsar.prev` copy and reloads the supervisor.
      rollback: {
        kind: "pulsar",
        source: "docs/OPERATIONS.md (Станция / Pulsar, откат на pulsar.prev)",
        instruction:
          "adb -s <station>:5555 shell 'cd /data/pulsar && cp -p pulsar.prev " +
          "pulsar.rollback && mv pulsar.rollback pulsar && /data/pulsar/ctl.sh reload'",
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
        "requirements-test.txt",
        ".gitleaks.toml",
        "**/*.sh",
        // The root Cargo workspace/lockfiles and the shared Rust crates are
        // routed precisely through the Bazel graph (see `affected.mjs`). They
        // still need an owning unit for the coverage invariant, and the
        // always-on quality job is the natural owner.
        "Cargo.toml",
        "Cargo.lock",
        "crate-universe.lock",
        "packages/rust/**",
        // Infrastructure bootstrap helpers written in Python; the shell
        // entrypoints that call them are covered by `**/*.sh` above.
        "projects/home/kubernetes/openbao/configure.py",
      ],
    },
  ],
};

export default config;
