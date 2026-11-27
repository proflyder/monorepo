# Work Hermes

A separate Hermes instance for work: namespace `hermes-work`, its own Telegram
bot, its own PVC and a Claude model. It has no access to Home Assistant, Music
Assistant or the home MCP servers, and home alerts stay in the personal bot.

| Item | Value |
|---|---|
| Flux | Kustomization `app-hermes-work` → this directory → `HelmRelease/hermes-work` |
| Chart | [`../chart`](../chart) with `instance: work` |
| Image | the same `HERMES_IMAGE` pin as the personal instance |
| Config | [`values.yaml`](values.yaml) → ConfigMap `hermes-work-config` → `/opt/data/config.yaml` |
| Data | PVC `hermes-work-data`, StorageClass `local-retain` (`Retain`), K3s VM disk |
| Backup | the nightly vzdump of the K3s VM, the same mechanism as the personal PVC |
| Network | no `hostNetwork`; default deny; egress DNS and TCP 443 to public addresses (RFC1918, CGNAT and link-local excluded); ingress only from the smoke Job |
| Secrets | `SecretStore openbao` in `hermes-work`, role `hermes-work-eso`, read only `secret/k8s/hermes-work/*` |
| Smoke | post-install/post-upgrade Job `home-deploy hermes api-smoke` → `http://hermes-work:8642/v1/chat/completions` |

## Model: Claude through the Claude subscription

Upstream hermes-agent (revision in
`projects/home-assistant/deploy/images/hermes/revision`) reads an Anthropic
OAuth token from `CLAUDE_CODE_OAUTH_TOKEN` (`agent/anthropic_credentials.py`)
and sends it as Claude Code (Bearer plus the OAuth beta header). The token is
the long-lived one printed by `claude setup-token`. `auth.adopt_external_logins:
false` stops Hermes from reading or refreshing any other Claude login.

Upstream documents the limits of this path
(`website/docs/integrations/providers.md`, "Anthropic — Claude Max + OAuth"):
it works only on Claude Max with purchased extra usage, and Hermes spends only
the extra usage credits, never the base Max allowance. Claude Pro cannot use
it. Without extra usage the fallback is an `ANTHROPIC_API_KEY` billed per token:
put it into `secret/k8s/hermes-work/anthropic` as `api_key` and switch the
`hermes-anthropic` ExternalSecret and `secretEnv` entry to `ANTHROPIC_API_KEY`.

## Owner steps (manual)

1. BotFather: create a new bot for work. `/setjoingroups` → Disable,
   `/setinline` stays off, privacy stays enabled, no commands or description
   that reveal the purpose.
2. OpenBao policies and role: rerun
   `projects/home/kubernetes/openbao/configure.py` with an admin token
   (`projects/home/kubernetes/openbao/README.md`). It creates policy and role
   `hermes-work-eso`, adds the `deny` on `secret/k8s/hermes-work/*` to
   `k8s-read` and writes the regenerated `secrets-gateway` policy. Do this
   before the merge: the deny must exist before the secrets do.
3. Secrets in OpenBao (operator token, GET → merge → POST with `cas`):
   - `secret/k8s/hermes-work/telegram`: `token` = the bot token,
     `allowed_users` = the owner's numeric Telegram id (the same value as
     `TELEGRAM_ALLOWED_USERS` in `secret/k8s/home-assistant/hermes-environment`);
   - `secret/k8s/hermes-work/anthropic`: `oauth_token` = output of
     `claude setup-token` on the subscription account;
   - `secret/k8s/hermes-work/api`: `key` = 32+ random characters.
   Then `mise run vault:sync`.
4. After the merge: Flux creates the namespace and the release; the smoke Job
   must answer. Message the new bot from the owner account; any other account
   gets no answer.

## Not in this stage

- Voice of the work speaker profile (stage 2): voice-proxy will route `work`
  to `http://hermes-work.hermes-work.svc:8642` and needs an ingress rule.
- A work MCP gateway: enable `networkPolicy.mcpGateway` in `values.yaml` when
  it exists.
- Slack and GitHub credentials (`secret/k8s/hermes-work/{slack,github}`).
