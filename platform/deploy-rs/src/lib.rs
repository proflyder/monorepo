//! Home deployment tooling.
//!
//! This crate is the Rust replacement for the Go `platform/deploy` command. It
//! is being built next to the Go implementation (`platform/deploy-rs` until the
//! cutover) so a parity harness can compare both binaries against the frozen
//! contract fixtures in `testdata/frozen`.
//!
//! Every subcommand is ported: `env get`, `build-id`, `render`, `oci`,
//! `converge`, `state`, `preflight`, `runtime split`, `eso names`,
//! `secret opaque`, `hermes rollout`, `hermes render-config`,
//! `hermes drift-exporter` and `hermes assist-smoke`.

pub mod buildid;
pub mod cli;
pub mod converge;
pub mod diff;
pub mod envfile;
pub mod gojson;
pub mod hermes;
pub mod hermesconfig;
pub mod hermesdrift;
pub mod http;
pub mod manifests;
pub mod oci;
pub mod preflight;
pub mod process;
pub mod render;
pub mod runtimevalues;
pub mod state;
pub mod yaml;

/// Usage text shared with the Go binary. The commands, flag names and wording
/// are part of the CLI contract; keep them byte-identical while both
/// implementations exist.
pub const USAGE: &str = r#"home-deploy <command> [options]

Commands:
  render   substitute ${KEY} placeholders in deploy templates
             --unit scheduler|currency-bot|mcp-gateway|wakegrant|ingress
             --env FILE   private KEY=VALUE file (optional)
             --out DIR    output directory
             --set K=V    override a placeholder (repeatable)
             --rollout-key K
                          fill K with a content fingerprint of the rendered unit
             FILE...      templates (an output file keeps the template name)
  env get FILE KEY
           print the value of KEY (empty output when absent)
  build-id --fingerprint STR [--revision REV]
           print <revision>-<first 12 hex of sha256(STR)>, or just the digest
           when --revision is empty
  oci digest LAYOUT
  oci pack LAYOUT --ref REF --out FILE
  oci docker LAYOUT --ref REF --out FILE
           inspect or pack a rules_oci image layout (byte-preserving)
  converge --kubectl K --namespace NS --name N --file F
           [--preserve-replicas] [--preserve-env] [--summary-file F]
           replace a live Deployment with the rendered manifest (prunes drift);
           --summary-file records created|unchanged|rolled-out
  state record --state-dir D --namespace NS --name N [--kubectl K]
  state mark --state-dir D --name N
  state rollback --state-dir D [--namespace NS] [--kubectl K]
           deploy rollback bookkeeping (.before.json / .absent / .applied)
  preflight --ha-url URL --ha-token-file FILE --scheduler-url URL
            --scheduler-token-file FILE [--expected-endpoint URL]
           read-only cutover checks for deploy.sh --execute
  runtime split --render F --private F --storage-out F --values-out F
            [--health-exporter-image DIGEST]
           split a Helm render into kubectl-applied storage and Helm values
  eso names --out F MANIFEST...
           list ExternalSecrets as <namespace>/<name>
  secret opaque --name N --namespace NS --key K --from-env-file F --env KEY
           print a one-key Opaque Secret JSON (bootstrap path)
  hermes rollout --values-file F --hermes-digest D --voice-proxy-digest D
            --exporter-digest D --smoke-image IMAGE --state-dir D [--force]
            [--simulate-smoke-failure] [--rollback-only]
           promote the Hermes/voice-proxy/exporter digests into the runtime release
  hermes render-config --home DIR [--source-dir DIR] [--private-values F]
            [--fragment F] [--context F] [--soul F] [--voice-prompt F]
            [--telegram-prompt F] [--skills DIR] [--dry-run|--check]
           merge config.fragment.yaml into a live HERMES_HOME
           (unset overlay paths default under --source-dir, else
            $HERMES_OVERLAY_DIR, else the repository overlay)
  hermes drift-exporter --listen ADDR:PORT [--interval SECS] [--home DIR]
            [--source-dir DIR] [--private-values F] [--fragment F]
            [--context F] [--soul F] [--voice-prompt F]
            [--telegram-prompt F] [--skills DIR]
           serve Prometheus config-drift metrics on /metrics (read-only)
  hermes assist-smoke
           one Assist turn through the in-cluster HA API (smoke Job client)
"#;
