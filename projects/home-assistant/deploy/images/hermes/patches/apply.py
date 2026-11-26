import sys
from pathlib import Path

API_SERVER = Path("gateway/platforms/api_server.py")

SKILLS_CALL = "skip_disabled=False, include_editorial=True"
SKILLS_FIXED = "skip_disabled=False"

TOOLSETS_ANCHOR = """                    "tools": tools})
        except Exception:
            logger.exception("GET /v1/toolsets failed")"""

TOOLSETS_WITH_MCP = """                    "tools": tools})
            platform_lists = config.get("platform_toolsets") or {}
            saved = {str(name) for name in (platform_lists.get("api_server") or [])}
            for server, spec in (config.get("mcp_servers") or {}).items():
                key = f"mcp-{server}"
                try:
                    mcp_tools = sorted(set(resolve_toolset(key)))
                except Exception:
                    mcp_tools = []
                data.append({
                    "name": key, "label": f"MCP: {server}", "description": "",
                    "enabled": key in saved and (spec or {}).get("enabled", True) is not False,
                    "configured": True,
                    "tools": mcp_tools})
        except Exception:
            logger.exception("GET /v1/toolsets failed")"""

REPLACEMENTS = (
    (SKILLS_CALL, SKILLS_FIXED),
    (TOOLSETS_ANCHOR, TOOLSETS_WITH_MCP),
)


class PatchDrift(Exception):
    pass


def patched(source: str) -> str:
    for anchor, replacement in REPLACEMENTS:
        if source.count(anchor) != 1:
            raise PatchDrift(f"anchor not found exactly once: {anchor.splitlines()[0].strip()}")
        source = source.replace(anchor, replacement)
    return source


def main(root: str) -> int:
    target = Path(root) / API_SERVER
    target.write_text(patched(target.read_text(encoding="utf-8")), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
