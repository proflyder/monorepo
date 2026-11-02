---
name: profiles
description: Inspect or change which isolated profile (work/personal) a bb project uses, and its provider/machine policy
---

Each project runs its providers under one profile. Profiles isolate provider
config, logins, memory and history (CLAUDE_CONFIG_DIR, CODEX_HOME, GH_CONFIG_DIR).
The default profile is `work` and uses the normal ~/.claude and ~/.codex.

- `bb profiles list` — projects and their profiles
- `bb profiles show [project-id]` — a profile's providers, machines and env
- `bb profiles use <project-id> <profile>` — switch (applies from the next message)
- `bb profiles create <name>` — new profile with its own config directories
- `bb profiles env <profile> <provider|*> <NAME> <value>` — extra env var (paths only, no secrets)
- `bb profiles unset <profile> <provider|*> <NAME>` — remove an env var
- `bb profiles providers <profile> <id,id,...|all>` — restrict providers
- `bb profiles machines <profile> <hostId,...|all>` — restrict execution machines

`message.dispatch` rejects a message when the thread's provider or machine is
outside the project profile's allowlist. `all` (null) means no restriction.
New profiles store `~/.bb-profiles/...` paths; on a split server/provider setup
the `hostHomes` plugin setting maps each host id to that machine's home.
