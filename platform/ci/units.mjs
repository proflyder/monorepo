export const config = {
  version: 1,

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
        "mise.toml",
        "mise.lock",
        "platform/mise-tasks/**",
        "platform/workstation/bin/**",
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
    {
      paths: ["platform/android/**"],
      platforms: ["android"],
    },
    {
      paths: ["rust-toolchain.toml"],
      platforms: ["native", "bazel"],
    },
    {
      paths: ["requirements-test.txt"],
      platforms: ["python", "quality"],
    },
    {
      paths: [".github/workflows/ci.yml"],
      platforms: ["*"],
      deploy: false,
    },
    {
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
    {
      id: "platform-deploy",
      platform: "bazel",
      title: "platform/deploy-rs (home-deploy CLI: render, env, oci, converge, state, preflight, hermes)",
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
        "packages/proto/**",
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      deploy: {
        builder: "bazel",
        target_root: "//projects/home/kubernetes/scheduler",
        images: ["scheduler", "home-deploy"],
        optional_images: ["ha-bridge", "ha-control", "ha-scenario"],
        workloads: ["scheduler"],
        namespace: "home-assistant",
      },
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed SCHEDULER_IMAGE; " +
          "Flux applies the previous digest within 1-2m. For a windowed drill " +
          "use `flux suspend kustomization app-ha-executors` plus deploy.sh.",
        source: "DECISIONS 54/55 and projects/home/kubernetes/flux/README.md",
      },
      bazel: {
        roots: [
          "//solutions/scheduler",
          "//packages/rust",
          "//projects/home/kubernetes/scheduler",
        ],
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
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      deploy: {
        builder: "bazel",
        target_root: "//solutions/wakegrant",
        images: ["wakegrant"],
        workloads: ["wakegrant"],
        namespace: "home-assistant",
      },
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed WAKEGRANT_IMAGE; " +
          "Flux applies the previous digest within 1-2m. `suspend: true` plus a " +
          "pins revert, never `kubectl rollout undo`.",
        source: "DECISIONS 54 and projects/home/kubernetes/flux/README.md",
      },
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
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      deploy: {
        builder: "bazel",
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
      id: "phone-push",
      platform: "bazel",
      title: "solutions/phone-push (Rust FCM HTTP v1 sender) + home deployment",
      paths: [
        "solutions/phone-push/**",
        "projects/home/kubernetes/phone-push/**",
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      deploy: {
        builder: "bazel",
        target_root: "//solutions/phone-push",
        images: ["phone-push"],
        workloads: ["phone-push"],
        namespace: "home-assistant",
      },
      rollback: {
        kind: "flux-pins",
        note:
          "Stateless: revert the `deploy/home-apps` commit that changed " +
          "PHONE_PUSH_IMAGE and let Flux re-apply the previous digest.",
        source: "DECISIONS 54 and projects/home/kubernetes/phone-push/README.md",
      },
      bazel: {
        roots: [
          "//solutions/phone-push",
          "//packages/rust",
          "//projects/home/kubernetes/phone-push",
        ],
        test_targets: [
          "//solutions/phone-push/crates/phone-push:unit_tests",
          "//solutions/phone-push/crates/phone-push:sender_tests",
          "//solutions/phone-push/crates/phone-push:clippy",
          "//solutions/phone-push/crates/phone-push:rustfmt",
        ],
        build_targets: [
          "//solutions/phone-push/...",
          "//projects/home/kubernetes/phone-push/...",
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
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      deploy: {
        builder: "bazel",
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
        "solutions/bb-home-dispatcher/contract/params.json",
        "projects/home/kubernetes/mcp-gateway/**",
        "platform/deploy/**",
        "platform/deploy-rs/**",
        "packages/rust/home-clients/**",
        "packages/rust/home-telegram/**",
        "packages/rust/home-tls/**",
        "packages/rust/home-contracttest/**",
      ],
      deploy: {
        builder: "bazel",
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
        "packages/proto/**",
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      deploy: {
        builder: "bazel",
        target_root: "//projects/home/kubernetes/scheduler",
        images: ["ha-bridge", "ha-control", "ha-scenario"],
        optional_images: ["scheduler"],
        workloads: ["ha-bridge", "ha-control", "ha-scenario"],
        namespace: "home-assistant",
      },
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed HA_BRIDGE_IMAGE / " +
          "HA_CONTROL_IMAGE / HA_SCENARIO_IMAGE. Mode changes are a Git change " +
          "plus `flux suspend kustomization app-ha-executors` for a drill.",
        source: "DECISIONS 54 and projects/home/kubernetes/scheduler/README.md",
      },
      bazel: {
        roots: [
          "//solutions/ha-bridge",
          "//packages/rust",
          "//projects/home/kubernetes/scheduler",
        ],
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
        release: true,
        sdk_packages: ["ndk;28.2.13676358"],
      },
    },

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

    {
      id: "ha-config-sync",
      platform: "bazel",
      title: "solutions/ha-config-sync (Rust HA config delivery) + ha-voice image",
      paths: [
        "solutions/ha-config-sync/**",
        "packages/rust/forced-ssh/**",
        "projects/home-assistant/ha-voice/**",
        "projects/home/kubernetes/ha-config-sync/**",
        "platform/bazel/oci/**",
      ],
      deploy: {
        builder: "bazel",
        target_root: "//projects/home/kubernetes/ha-config-sync",
        images: ["ha-config-sync"],
        workloads: ["ha-config-sync"],
        namespace: "home-assistant",
      },
      rollback: {
        kind: "flux-pins",
        note:
          "Revert the `deploy/home-apps` commit that changed HA_CONFIG_SYNC_IMAGE; " +
          "Flux re-applies the previous digest and the previous ha-voice file set " +
          "within 1-2m. The agent and the SSH key on HAOS are unchanged by an " +
          "image rollback.",
        source: "projects/home/kubernetes/ha-config-sync/README.md (Rollback)",
      },
      bazel: {
        roots: [
          "//solutions/ha-config-sync",
          "//packages/rust/forced-ssh",
          "//projects/home/kubernetes/ha-config-sync",
        ],
        test_targets: [
          "//solutions/ha-config-sync/crates/ha-config-sync:unit_tests",
          "//solutions/ha-config-sync/crates/ha-config-sync:agent_tests",
          "//solutions/ha-config-sync/crates/ha-config-sync:clippy",
          "//solutions/ha-config-sync/crates/ha-config-sync:rustfmt",
          "//packages/rust/forced-ssh:unit_tests",
          "//packages/rust/forced-ssh:clippy",
          "//packages/rust/forced-ssh:rustfmt",
        ],
        build_targets: [
          "//solutions/ha-config-sync/...",
          "//packages/rust/forced-ssh/...",
          "//projects/home/kubernetes/ha-config-sync/...",
        ],
      },
    },

    {
      id: "ha-config-sync-deploy",
      platform: "python",
      title: "projects/home/kubernetes/ha-config-sync (manifest/alerts contract)",
      paths: ["projects/home/kubernetes/ha-config-sync/**"],
      python: {
        suites: ["projects/home/kubernetes/ha-config-sync"],
      },
    },

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
        "projects/home-assistant/hermes/HERMES.md",
        "projects/home-assistant/hermes/SOUL.md",
        "projects/home-assistant/hermes/skills/**/*.md",
        "projects/home-assistant/deploy/images/hermes/**",
        "solutions/voice-proxy/**",
        "projects/home-assistant/deploy/health-exporter/**",
        "projects/home-assistant/deploy/chart/**",
        "platform/deploy/**",
        "platform/deploy-rs/**",
      ],
      bazel: {
        roots: [
          "//solutions/voice-proxy",
          "//projects/home-assistant/deploy/health-exporter",
        ],
      },
      deploy: {
        builder: "mixed",
        bazel_images: {
          "voice-proxy": "//solutions/voice-proxy",
          "health-exporter": "//projects/home-assistant/deploy/health-exporter",
          "home-deploy": "//projects/home/kubernetes/scheduler",
        },
        images: ["hermes", "voice-proxy", "health-exporter", "home-deploy"],
        workloads: ["hermes"],
        namespace: "home-assistant",
      },
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
      id: "hermes-config",
      platform: "bazel",
      title: "projects/home-assistant/hermes (config renderer image + drift exporter)",
      paths: [
        "projects/home-assistant/hermes/BUILD.bazel",
        "projects/home-assistant/hermes/config.fragment.yaml",
        "projects/home-assistant/hermes/HERMES.md",
        "projects/home-assistant/hermes/SOUL.md",
        "projects/home-assistant/hermes/voice_prompt.txt",
        "projects/home-assistant/hermes/telegram_prompt.txt",
        "projects/home-assistant/hermes/skills/**",
        "projects/home-assistant/hermes/skills/**/*.md",
        "platform/deploy-rs/**",
        "platform/bazel/oci/**",
      ],
      bazel: {
        roots: ["//projects/home-assistant/hermes", "//platform/deploy-rs"],
        test_targets: [
          "//platform/deploy-rs:unit_tests",
          "//platform/deploy-rs:clippy",
        ],
        build_targets: ["//projects/home-assistant/hermes:hermes-config_image"],
      },
      deploy: {
        builder: "bazel",
        target_root: "//projects/home-assistant/hermes",
        images: ["hermes-config"],
        workloads: ["hermes"],
        namespace: "home-assistant",
      },
      rollback: {
        kind: "hermes",
        release: "runtime",
        namespace: "home-assistant",
        source: "docs/KUBERNETES-OPERATIONS.md (Автодеплой Hermes / voice-proxy / health exporter)",
        note:
          "Flux: `HelmRelease/runtime` uses upgrade.remediation.strategy " +
          "rollback. Manual emergency path: `helm rollback runtime <revision>` " +
          "or a `deploy/home-apps` revert.",
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
      paths: [
        "projects/home/kubernetes/flux/**",
        "projects/home/kubernetes/nexus/**",
        "projects/home/kubernetes/external-secrets/**",
        "projects/home/kubernetes/ingress/**",
        "projects/home/kubernetes/observability/**",
        "projects/home/kubernetes/vaultwarden/**",
        "projects/home/kubernetes/wakegrant/manifests/**",
        "projects/home/kubernetes/intercom/manifests/**",
        "projects/home/kubernetes/currency-bot/manifests/**",
        "projects/home/kubernetes/mcp-gateway/manifests/**",
        "projects/home/kubernetes/scheduler/manifests/**",
        "projects/home-assistant/deploy/flux/**",
        "projects/home-assistant/deploy/chart/**",
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

    {
      id: "heartbeat",
      platform: "js",
      title: "solutions/heartbeat (Cloudflare Worker dead-man switch)",
      paths: ["solutions/heartbeat/**", "projects/home/cloudflare/heartbeat/**"],
      js: {
        tests: ["solutions/heartbeat/src"],
      },
    },

    {
      id: "bb-home-dispatcher",
      platform: "js",
      title: "solutions/bb-home-dispatcher (bb home dispatcher plugin)",
      paths: ["solutions/bb-home-dispatcher/**"],
      js: {
        tests: ["solutions/bb-home-dispatcher/tests"],
      },
    },

    {
      id: "bb-profiles",
      platform: "js",
      title: "solutions/bb-profiles (bb work/personal profiles plugin)",
      paths: ["solutions/bb-profiles/**"],
      js: {
        tests: ["solutions/bb-profiles/tests"],
      },
    },

    {
      id: "logon-provider",
      platform: "native",
      title: "solutions/windows-logon-provider (Windows credential provider)",
      paths: [
        "solutions/windows-logon-provider/**",
        "platform/bazel/windows/**",
        "platform/bazel/zig/**",
        "MODULE.bazel",
      ],
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
        publish_tags: ["{version}", "sha-{commit12}"],
        publish_attest_glob: "HomeKompLogon.dll",
      },
    },

    {
      id: "pulsar",
      platform: "native",
      title: "projects/station/pulsar (Rust station daemon, radio, installer)",
      paths: ["projects/station/pulsar/**"],
      bazel: {
        roots: ["//projects/station/pulsar"],
      },
      native: {
        kind: "pulsar",
        workdir: "projects/station/pulsar",
        publish_script: "projects/station/pulsar/publish-oci.sh",
        publish_repository: "ghcr.io/proflyder/pulsar-firmware",
        publish_tags: ["{version}", "latest"],
        publish_attest_glob: "*",
      },
      rollback: {
        kind: "pulsar",
        source: "docs/OPERATIONS.md (Станция / Pulsar, откат на pulsar.prev)",
        instruction:
          "adb -s <station>:5555 shell 'cd /data/pulsar && cp -p pulsar.prev " +
          "pulsar.rollback && mv pulsar.rollback pulsar && /data/pulsar/ctl.sh reload'",
      },
    },

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
        "Cargo.toml",
        "Cargo.lock",
        "crate-universe.lock",
        "packages/rust/**",
        "projects/home/kubernetes/openbao/configure.py",
      ],
    },
  ],
};

export default config;
