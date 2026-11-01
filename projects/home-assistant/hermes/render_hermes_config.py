#!/usr/bin/env python3
"""Render the reviewed Hermes overlay into a live HERMES_HOME.

Hermes keeps one generated ``config.yaml`` plus ``SOUL.md`` and the infra
context ``.hermes.md`` on its PVC.  Editing those by hand is invisible to Git
and is lost when the home is re-provisioned, so this script is the single
reviewed path from the repository files to the volume:

    config.fragment.yaml  ->  <home>/config.yaml   (deep merge, unrelated keys kept)
    private values (opt.) ->  <home>/config.yaml   (deep merge AFTER the fragment)
    voice_prompt.txt      ->  agent.personalities.voice (single source of truth)
    HERMES.md             ->  <home>/.hermes.md    (infra context, cwd = /opt/data)
    SOUL.md               ->  <home>/SOUL.md       (identity)

The tracked fragment uses TEST-NET placeholders (192.0.2.0/24) for the Home
Assistant and Music Assistant endpoints.  Real addresses come from a private
values file outside Git and are merged *after* the fragment, so a render never
replaces a working live endpoint with a placeholder.  As a safety net the
script refuses to write when a placeholder would replace a different live
value (or introduce a new one) and names the keys; ``--allow-placeholders`` is
only for tests that render into an empty home.

It is idempotent: files whose content already matches are not touched, and the
previous version is kept as ``<name>.prev`` (mode 600) for one-step rollback.
``--dry-run`` prints the unified diff and writes nothing; ``--check`` is the
same but exits 1 when a render would change something (CI/pre-flight use).

The merge is done on parsed YAML, so comments in a hand-edited config.yaml are
not preserved; the .prev copy is the rollback path.

Run it against the mounted volume from the K3s host, then restart the gateway:

    python3 projects/home-assistant/hermes/render_hermes_config.py \\
        --home /srv/k3s/volumes/runtime-hermes \\
        --private-values secrets/home/hermes-private.yaml --dry-run
    python3 projects/home-assistant/hermes/render_hermes_config.py \\
        --home /srv/k3s/volumes/runtime-hermes \\
        --private-values secrets/home/hermes-private.yaml
    kubectl -n home-assistant rollout restart deployment/hermes

Secrets never live in these files: tokens stay in Secret/hermes-environment
and are referenced by the fragment as ${HERMES_*_TOKEN} placeholders.
"""
from __future__ import annotations

import argparse
import copy
import difflib
from pathlib import Path
import re
import sys
import time

import yaml

HERE = Path(__file__).resolve().parent
DEFAULT_FRAGMENT = HERE / "config.fragment.yaml"
DEFAULT_CONTEXT = HERE / "HERMES.md"
DEFAULT_SOUL = HERE / "SOUL.md"
DEFAULT_VOICE_PROMPT = HERE / "voice_prompt.txt"

# Files installed verbatim into HERMES_HOME, keyed by their in-home name.
INSTALLED = (".hermes.md", "SOUL.md")

# Values that must never silently replace a live endpoint: TEST-NET addresses
# (RFC 5737, used across this repository as public placeholders) and the
# reserved example.* domains.
_PLACEHOLDER = re.compile(
    r"\b192\.0\.2\.\d{1,3}\b|\bexample\.(?:com|net|org)\b", re.IGNORECASE
)

_SECRET_LINE = re.compile(
    r"""(?ix)
    (
      (?:^|\b)(?:api[_-]?key|token|secret|password|authorization|bearer)\b
      \s*[:=]\s*
    )
    (?:"[^"]*"|'[^']*'|\S+)
    """
)


def deep_merge(base, overlay):
    """Merge *overlay* into *base*; mappings merge, everything else replaces."""
    if isinstance(base, dict) and isinstance(overlay, dict):
        merged = dict(base)
        for key, value in overlay.items():
            if key in merged:
                merged[key] = deep_merge(merged[key], value)
            else:
                merged[key] = copy.deepcopy(value)
        return merged
    return copy.deepcopy(overlay)


def iter_leaves(data, prefix: tuple[str, ...] = ()):
    """Yield (dotted path, value) for every non-mapping leaf."""
    if isinstance(data, dict):
        for key, value in data.items():
            yield from iter_leaves(value, prefix + (str(key),))
    else:
        yield prefix, data


def placeholder_violations(merged: dict, live: dict) -> list[str]:
    """Keys whose placeholder value differs from (or is absent in) the live config."""
    current = {path: value for path, value in iter_leaves(live)}
    bad = []
    for path, value in iter_leaves(merged):
        if not isinstance(value, str) or not _PLACEHOLDER.search(value):
            continue
        if current.get(path) != value:
            bad.append(".".join(path))
    return sorted(set(bad))


def dump_config(data: dict) -> str:
    return yaml.safe_dump(
        data,
        sort_keys=False,
        allow_unicode=True,
        default_flow_style=False,
        width=4096,
        indent=2,
    )


def redact(text: str) -> str:
    """Mask token-shaped values so a dry-run diff is safe to paste into chat."""
    return _SECRET_LINE.sub(lambda m: m.group(1) + "<redacted>", text)


def _read_mapping(path: Path, what: str) -> dict:
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if data is None:
        return {}
    if not isinstance(data, dict):
        raise SystemExit(f"{path}: {what} must be a YAML mapping, got {type(data).__name__}")
    return data


def set_voice_personality(merged: dict, voice: str) -> dict:
    agent = merged.setdefault("agent", {})
    if not isinstance(agent, dict):
        raise SystemExit("config.yaml: agent must be a mapping")
    personalities = agent.setdefault("personalities", {})
    if not isinstance(personalities, dict):
        raise SystemExit("config.yaml: agent.personalities must be a mapping")
    personalities["voice"] = voice
    return merged


def build_plan(
    home: Path,
    fragment: Path,
    context: Path,
    soul: Path,
    private_values: Path | None = None,
    voice_prompt: Path = DEFAULT_VOICE_PROMPT,
    allow_placeholders: bool = False,
) -> dict[Path, str]:
    """Return {destination: new content} for every file a render would change."""
    config = home / "config.yaml"
    current = _read_mapping(config, "config.yaml") if config.exists() else {}

    merged = deep_merge(current, _read_mapping(fragment, "fragment"))
    if private_values is not None:
        if not private_values.exists():
            raise SystemExit(f"{private_values}: private values file not found")
        merged = deep_merge(merged, _read_mapping(private_values, "private values"))

    if voice_prompt.exists():
        merged = set_voice_personality(merged, voice_prompt.read_text(encoding="utf-8").strip())
    else:
        raise SystemExit(f"{voice_prompt}: voice prompt not found")

    if not allow_placeholders:
        victims = placeholder_violations(merged, current)
        if victims:
            raise SystemExit(
                "refusing to render: placeholder values would replace live ones:\n"
                + "\n".join(f"  {key}" for key in victims)
                + "\n\nPass --private-values <file> with the real endpoints "
                  "(copy hermes-private.example.yaml), or --allow-placeholders "
                  "when rendering a throw-away test home."
            )

    plan: dict[Path, str] = {}
    rendered = dump_config(merged)
    if not config.exists() or rendered != config.read_text(encoding="utf-8"):
        plan[config] = rendered
    for name, source in zip(INSTALLED, (context, soul)):
        if not source.exists():
            continue
        text = source.read_text(encoding="utf-8")
        destination = home / name
        if not destination.exists() or destination.read_text(encoding="utf-8") != text:
            plan[destination] = text
    return plan


def diff_for(path: Path, new_text: str) -> str:
    old_text = path.read_text(encoding="utf-8") if path.exists() else ""
    diff = difflib.unified_diff(
        old_text.splitlines(keepends=True),
        new_text.splitlines(keepends=True),
        fromfile=str(path),
        tofile=str(path) + " (rendered)",
    )
    return redact("".join(diff))


def apply_plan(plan: dict[Path, str]) -> list[Path]:
    written = []
    stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    for path, text in plan.items():
        if path.exists():
            backup = path.with_name(path.name + ".prev")
            backup.write_bytes(path.read_bytes())
            backup.chmod(0o600)
        path.write_text(text, encoding="utf-8")
        written.append(path)
    print(f"rendered {len(written)} file(s) at {stamp}")
    return written


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Merge config.fragment.yaml (plus private values) into a live "
                    "Hermes config.yaml, set the voice personality from "
                    "voice_prompt.txt and install .hermes.md + SOUL.md (idempotent).",
    )
    parser.add_argument("--home", type=Path, required=True,
                        help="HERMES_HOME (K3s: /srv/k3s/volumes/runtime-hermes, "
                             "in the Pod: /opt/data)")
    parser.add_argument("--fragment", type=Path, default=DEFAULT_FRAGMENT)
    parser.add_argument("--private-values", type=Path, default=None,
                        help="private YAML with real endpoints (merged after the fragment)")
    parser.add_argument("--voice-prompt", type=Path, default=DEFAULT_VOICE_PROMPT,
                        help="source for agent.personalities.voice")
    parser.add_argument("--context", type=Path, default=DEFAULT_CONTEXT,
                        help="infra context source installed as .hermes.md")
    parser.add_argument("--soul", type=Path, default=DEFAULT_SOUL)
    parser.add_argument("--allow-placeholders", action="store_true",
                        help="allow TEST-NET/example placeholders (tests only)")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--dry-run", action="store_true", help="print diffs, change nothing")
    mode.add_argument("--check", action="store_true",
                      help="like --dry-run, but exit 1 when a render is pending")
    args = parser.parse_args(argv)

    if not args.home.is_dir():
        raise SystemExit(f"{args.home}: not a directory (pass the mounted HERMES_HOME)")
    if not args.fragment.exists():
        raise SystemExit(f"{args.fragment}: fragment not found")

    plan = build_plan(
        args.home,
        args.fragment,
        args.context,
        args.soul,
        private_values=args.private_values,
        voice_prompt=args.voice_prompt,
        allow_placeholders=args.allow_placeholders,
    )
    if not plan:
        print("up to date: config.yaml, .hermes.md and SOUL.md already match the repository")
        return 0
    for path, text in plan.items():
        print(f"--- would update {path} ---")
        print(diff_for(path, text), end="")
    if args.dry_run or args.check:
        return 1 if args.check else 0
    apply_plan(plan)
    print("restart the gateway to load the new config and context")
    return 0


if __name__ == "__main__":
    sys.exit(main())
