import importlib.util
import unittest
from pathlib import Path

PATCHER = Path(__file__).parent / "images" / "hermes" / "patches" / "apply.py"


def load_patcher():
    spec = importlib.util.spec_from_file_location("hermes_upstream_patch", PATCHER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


UPSTREAM_SHAPE = '''
class Adapter:
    async def _handle_skills(self, request):
        try:
            skills = sort_skills(
                find_all_skills(
                    skip_disabled=False, include_editorial=True
                )
            )
        except Exception:
            return None
        return skills

    async def _handle_toolsets(self, request):
        try:
            config = load_config()
            data = []
            for name in names:
                data.append({
                    "name": name,
                    "tools": tools})
        except Exception:
            logger.exception("GET /v1/toolsets failed")
            return None
        return data
'''


class HermesUpstreamPatchTest(unittest.TestCase):
    def setUp(self):
        self.patcher = load_patcher()

    def test_skills_call_matches_the_upstream_signature(self):
        patched = self.patcher.patched(UPSTREAM_SHAPE)
        self.assertNotIn("include_editorial", patched)
        self.assertIn("find_all_skills(\n                    skip_disabled=False\n", patched)

    def test_toolsets_list_configured_mcp_servers(self):
        patched = self.patcher.patched(UPSTREAM_SHAPE)
        compile(patched, "api_server.py", "exec")
        self.assertIn('key = f"mcp-{server}"', patched)
        self.assertIn('platform_lists.get("api_server")', patched)

    def test_upstream_drift_fails_the_build(self):
        drifted = UPSTREAM_SHAPE.replace("include_editorial=True", "include_hidden=True")
        with self.assertRaises(self.patcher.PatchDrift):
            self.patcher.patched(drifted)

    def test_a_second_application_is_refused(self):
        once = self.patcher.patched(UPSTREAM_SHAPE)
        with self.assertRaises(self.patcher.PatchDrift):
            self.patcher.patched(once)


if __name__ == "__main__":
    unittest.main()
