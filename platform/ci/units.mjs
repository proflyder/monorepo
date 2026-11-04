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
  // `MODULE.bazel` is global by default, with one exception: when its diff only
  // touches the rules_jvm_external Maven extension (`maven.install(...)`,
  // `maven.<tag>(...)` and `use_repo(maven, ...)`, plus comments/whitespace),
  // `affected.mjs` rewrites the changed path to `MODULE.bazel#maven`. That virtual
  // path reaches only the android platform (the Maven/APK closure changed, no Go,
  // Python, JS or native unit did). Anything else in MODULE.bazel - bazel_dep,
  // toolchains, go_deps, oci, python - and an unparseable file, a missing base or
  // an added/deleted file keep the global rule below. Fail safe is global.
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
      ],
      platforms: ["*"],
    },
    {
      paths: ["MODULE.bazel#maven"],
      platforms: ["android"],
    },
    {
      paths: ["platform/android/**"],
      platforms: ["android"],
    },
    {
      paths: ["go.mod", "go.sum", "go.work", "go.work.sum"],
      platforms: ["go"],
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
    paths: ["projects/home/kubernetes/scheduler/**"],
  },

  units: [
    // ---------------------------------------------------------------- Go --------
    {
      id: "platform-deploy",
      platform: "go",
      title: "platform/deploy (home-deploy CLI: render, env, build-id, oci)",
      // Every deploy script and the image build in CI call this binary, so a
      // change is both tested here and routed to the units that deploy through
      // it (their `paths` include platform/deploy/**).
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
      platform: "go",
      title: "solutions/voice-proxy (TTS-safe OpenAI passthrough)",
      paths: ["solutions/voice-proxy/**"],
      // Tests and the Bazel image targets only. The image is deployed by the
      // `hermes` unit below: both containers share one Helm release, one smoke
      // and one rollback, so a second deploy unit would race on `runtime`.
      go: {
        module: ".",
        packages: ["./solutions/voice-proxy/..."],
        race: true,
        build: ["./solutions/voice-proxy/cmd/voice-proxy"],
      },
    },
    {
      id: "health-exporter",
      platform: "go",
      title: "projects/home-assistant/deploy/health-exporter (runtime chart)",
      paths: ["projects/home-assistant/deploy/health-exporter/**"],
      // Tests and the Bazel image target only. The image is deployed by the
      // `hermes` unit (one Helm release, one smoke, one rollback).
      go: {
        module: ".",
        packages: ["./projects/home-assistant/deploy/health-exporter/..."],
        race: true,
        build: ["./projects/home-assistant/deploy/health-exporter/cmd/health-exporter"],
      },
    },
    {
      id: "scheduler",
      platform: "go",
      title: "solutions/scheduler (scheduler service)",
      paths: [
        "solutions/scheduler/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
      ],
      // deploy-home rolls exactly these container images and Deployments when the
      // unit is affected. Infrastructure (ingress, observability, runtime chart)
      // has no deploy block on purpose: it is not auto-deployed.
      deploy: {
        builder: "bazel",
        target_root: "//projects/home/kubernetes/scheduler",
        images: ["scheduler", "home-deploy"],
        workloads: ["scheduler"],
        namespace: "home-assistant",
        script: "projects/home/kubernetes/scheduler/deploy.sh",
        env_key: "scheduler_env",
        env_file: "scheduler.env",
        externalsecrets: ["scheduler-auth", "ha-bridge-ha"],
      },
      go: {
        module: ".",
        packages: ["./solutions/scheduler/..."],
        race: true,
        build: ["./solutions/scheduler/cmd/scheduler"],
      },
    },

    {
      id: "wakegrant",
      platform: "go",
      title: "solutions/wakegrant (one-time logon grants) + home deployment",
      paths: [
        "solutions/wakegrant/**",
        "projects/home/kubernetes/wakegrant/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
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
      go: {
        module: ".",
        packages: ["./solutions/wakegrant/..."],
        race: true,
        build: ["./solutions/wakegrant/cmd/wakegrant"],
      },
    },

    {
      id: "currency-bot",
      platform: "go",
      title: "solutions/currency-bot (Telegram currency bot)",
      paths: [
        "solutions/currency-bot/**",
        "projects/home/kubernetes/currency-bot/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
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
      go: {
        module: ".",
        packages: ["./solutions/currency-bot/..."],
        race: true,
        build: [
          "./solutions/currency-bot/cmd/currency-bot",
          "./solutions/currency-bot/cmd/currency-bot-import",
        ],
      },
    },

    {
      id: "mcp-gateway",
      platform: "go",
      title: "solutions/mcp-gateway (MCP infra tool groups + home deployment)",
      paths: [
        "solutions/mcp-gateway/**",
        // The plugin's strict per-method params schema is the gateway's wire
        // contract: a fixture change must retest the serializing side too.
        "solutions/bb-home-dispatcher/contract/params.json",
        "projects/home/kubernetes/mcp-gateway/**",
        // The deploy tooling that renders this unit's manifests.
        "platform/deploy/**",
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
      go: {
        module: ".",
        packages: [
          "./solutions/mcp-gateway/...",
          // Project-side deploy contract: loads the tracked catalog with the
          // gateway's real loader (builds and starts the binary).
          "./projects/home/kubernetes/mcp-gateway",
        ],
        race: true,
        build: ["./solutions/mcp-gateway/cmd/mcp-gateway"],
      },
    },

    {
      id: "ha-bridge",
      platform: "go",
      title: "solutions/ha-bridge (home executor + ha-control)",
      paths: [
        "solutions/ha-bridge/**",
        // The deploy tooling that renders the shared scheduler manifests.
        "platform/deploy/**",
      ],
      // The bridge unit owns ha-control too: they share the executor code.
      deploy: {
        builder: "bazel",
        target_root: "//projects/home/kubernetes/scheduler",
        images: ["ha-bridge", "ha-control"],
        workloads: ["ha-bridge", "ha-control"],
        namespace: "home-assistant",
        script: "projects/home/kubernetes/scheduler/deploy.sh",
        env_key: "scheduler_env",
        env_file: "scheduler.env",
        externalsecrets: ["scheduler-auth", "ha-bridge-ha"],
      },
      go: {
        module: ".",
        packages: ["./solutions/ha-bridge/..."],
        race: true,
        build: [
          "./solutions/ha-bridge/cmd/ha-bridge",
          "./solutions/ha-bridge/cmd/ha-control",
        ],
      },
    },

    {
      id: "alarm",
      platform: "go",
      title: "solutions/alarm (legacy AlarmService facade)",
      paths: ["solutions/alarm/**"],
      go: {
        module: ".",
        packages: ["./solutions/alarm/..."],
        race: false,
        build: ["./solutions/alarm/cmd/alarm"],
      },
    },

    {
      id: "ble-gateway",
      platform: "go",
      title: "projects/station/ble-gateway (separate Go module, outside Bazel)",
      paths: ["projects/station/ble-gateway/**"],
      go: {
        module: "./projects/station/ble-gateway",
        packages: ["./..."],
        race: false,
        build: ["./..."],
      },
    },

    {
      id: "shared-go-packages",
      platform: "go",
      title: "packages/go + packages/proto (shared libraries, no own consumers)",
      paths: ["packages/go/**", "packages/proto/**"],
      go: {
        module: ".",
        packages: ["./packages/..."],
        race: false,
      },
    },

    // ----------------------------------------------------------- Android --------
    // One unit per application; shared Kotlin libraries are their own units and
    // reach applications through the Bazel graph, not through path globs.
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
      title: "projects/station/pulsar/satellite (Rust host build, tests, fmt, clippy)",
      paths: ["projects/station/pulsar/satellite/**"],
      bazel: {
        roots: ["//projects/station/pulsar"],
        test_targets: [
          "//projects/station/pulsar/satellite:clippy",
          "//projects/station/pulsar/satellite:rustfmt",
          "//projects/station/pulsar/satellite/crates/...",
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
        // The voice-proxy Go solution and the shared clients it compiles into
        // the deployed binary. The `voice-proxy` go unit runs the tests; this
        // unit owns the deploy. `packages/go/**` is explicit because a deploy
        // unit is selected only from its own platform route (a python unit has
        // no `go list` closure), so the shared-client closure never reaches it.
        "solutions/voice-proxy/**",
        "packages/go/**",
        // The runtime chart's Go health exporter is part of this deploy unit.
        "projects/home-assistant/deploy/health-exporter/**",
        "projects/home-assistant/deploy/chart/**",
        // build-and-push.sh reads image digests with `home-deploy oci digest`.
        "platform/deploy/**",
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
        // The catalog's schema is checked by the go unit (real loader); this unit
        // owns the catalog↔RBAC correspondence, manifest hardening and the
        // deploy-script/render contract.
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
    // Native units: the Windows logon provider is a repository-script C++
    // cross-build; Pulsar's station daemon is cross-built with Bazel and its
    // installer/radio/ssh use their own scripts. A same-repo PR publishes the
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
      paths: ["solutions/windows-logon-provider/**"],
      native: {
        workdir: "solutions/windows-logon-provider",
        kind: "script",
        test: "./run-tests.sh",
        build: "./build.sh",
        artifacts: [
          "out/RicardoKompLogon.dll",
          "out/cp-selftest.exe",
          "out/hmac-selftest.exe",
        ],
        checksums: "out/SHA256SUMS",
        publish_script: "solutions/windows-logon-provider/publish-oci.sh",
        publish_repository: "ghcr.io/proflyder/windows-logon-provider",
        publish_tags: ["{version}", "sha-{commit12}"],
        publish_attest_glob: "RicardoKompLogon.dll",
        toolchain: {
          name: "llvm-mingw",
          version: "20261020",
          // GitHub release API `digest` for this asset (Linux x86_64, same release
          // as the owner's macOS toolchain). Pinned here, verified before use.
          url: "https://github.com/mstorsjo/llvm-mingw/releases/download/20261020/llvm-mingw-20261020-ucrt-ubuntu-22.04-x86_64.tar.xz",
          sha256: "55064b60b991a1e1e8090b8c2f84e82b22cfe5903d12c159d65064cf5e8ae335",
          root: "llvm-mingw-20261020-ucrt-ubuntu-22.04-x86_64",
        },
      },
    },

    // The Rust station daemon and its surrounding builds (radio firmware,
    // Go installer, dropbear). Both station ABIs are built with Bazel; host
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
