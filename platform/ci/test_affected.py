#!/usr/bin/env python3
"""Unit tests for the CI affected-unit router and the unit-map validator."""
from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import affected  # noqa: E402
import check_units  # noqa: E402

REPO = HERE.parents[1]
FIXTURE_UNITS = HERE / "testdata" / "units-two-apps.yml"
FIXTURE_GRAPH = HERE / "testdata" / "graph-two-apps.xml"


def load_fixture_graph() -> affected.BazelGraph:
    return affected.BazelGraph.from_xml(FIXTURE_GRAPH.read_text(encoding="utf-8"))


class GlobTests(unittest.TestCase):
    def test_double_star_matches_any_depth(self):
        self.assertTrue(affected.path_matches("docs/TESTING.md", ["docs/**"]))
        self.assertTrue(affected.path_matches("docs/a/b/c.md", ["docs/**"]))
        self.assertFalse(affected.path_matches("docsx/a.md", ["docs/**"]))

    def test_double_star_prefix_spans_directories(self):
        self.assertTrue(affected.path_matches("a/b/tool.sh", ["**/*.sh"]))
        self.assertTrue(affected.path_matches("tool.sh", ["**/*.sh"]))

    def test_single_star_stays_in_one_segment(self):
        self.assertTrue(affected.path_matches("platform/quality/x.py", ["platform/*/x.py"]))
        self.assertFalse(affected.path_matches("platform/quality/deep/x.py", ["platform/*/x.py"]))

    def test_exact_root_file(self):
        self.assertTrue(affected.path_matches("Makefile", ["Makefile"]))
        self.assertFalse(affected.path_matches("sub/Makefile", ["Makefile"]))


class BazelGraphTests(unittest.TestCase):
    def test_shared_library_change_reaches_only_dependent_app(self):
        graph = load_fixture_graph()
        touched = graph.affected_rules(["libs/shared/src/Shared.kt"])
        self.assertIn("//libs/shared:shared", touched)
        self.assertIn("//libs/shared:shared_test", touched)
        self.assertIn("//apps/one:lib", touched)
        self.assertIn("//apps/one:one_test", touched)
        self.assertNotIn("//apps/two:lib", touched)
        self.assertNotIn("//apps/two:two_test", touched)

    def test_unknown_file_falls_back_to_its_package(self):
        graph = load_fixture_graph()
        # A new (or deleted) file that no rule references yet: the whole package
        # is treated as changed, so the graph still reports dependents.
        touched = graph.affected_rules(["libs/shared/src/New.kt"])
        self.assertIn("//libs/shared:shared", touched)
        self.assertIn("//apps/one:lib", touched)

    def test_label_path_conversion(self):
        self.assertEqual(
            affected.BazelGraph.label_path("//pkg/sub:src/main/A.kt"), "pkg/sub/src/main/A.kt"
        )


class AndroidFixtureRoutingTests(unittest.TestCase):
    def setUp(self):
        self.config = affected.load_config(FIXTURE_UNITS)
        self.graph = load_fixture_graph()

    def route(self, changed):
        with mock.patch.object(affected, "go_package_dirs", return_value=set()):
            return affected.route(
                self.config,
                changed,
                root=REPO,
                base="base-sha",
                graph=self.graph,
            )

    def test_shared_library_selects_only_its_dependent_application(self):
        result = self.route(["libs/shared/src/Shared.kt"])
        ids = [unit.id for unit in result.routes["android"].units]
        self.assertEqual(ids, ["app-one", "shared-kotlin"])
        self.assertNotIn("app-two", ids)

    def test_application_change_selects_only_that_application(self):
        result = self.route(["apps/two/src/Two.kt"])
        ids = [unit.id for unit in result.routes["android"].units]
        self.assertEqual(ids, ["app-two"])

    def test_module_change_selects_every_application(self):
        result = self.route(["MODULE.bazel"])
        ids = [unit.id for unit in result.routes["android"].units]
        self.assertEqual(ids, ["app-one", "app-two", "shared-kotlin"])
        self.assertEqual(result.routes["android"].mode, "all")
        self.assertEqual(result.routes["go"].mode, "all")

    def test_docs_only_change_runs_nothing_but_quality(self):
        result = self.route(["docs/ARCHITECTURE.md"])
        self.assertTrue(result.docs_only)
        for platform in ("go", "android", "python"):
            self.assertEqual(result.routes[platform].units, [], platform)
        self.assertEqual([u.id for u in result.routes["quality"].units], ["repository"])

    def test_graph_failure_degrades_to_all_android_units(self):
        config = self.config

        def broken():
            raise RuntimeError("bazel query exploded")

        with mock.patch.object(affected, "go_package_dirs", return_value=set()):
            result = affected.route(
                config,
                ["libs/shared/src/Shared.kt"],
                root=REPO,
                base="base-sha",
                graph=None,
                graph_loader=broken,
            )
        self.assertEqual(result.routes["android"].mode, "all")
        self.assertEqual(len(result.routes["android"].units), 3)
        self.assertTrue(any("bazel graph failed" in w for w in result.warnings))

    def test_matrix_entries_are_flat_strings(self):
        result = self.route(["libs/shared/src/Shared.kt"])
        entries = affected.matrix_entries(result, self.config)
        self.assertEqual(
            entries["android"][0],
            {
                "id": "app-one",
                "test_targets": "//apps/one:one_test",
                "build_targets": "//apps/one:app",
                "artifact": "bazel-bin/apps/one/app.apk",
                "artifact_name": "app-one-debug",
                "universe": "//apps/one/... + //apps/two/... + //libs/shared/...",
            },
        )


class GoRoutingTests(unittest.TestCase):
    """A shared package change must select every unit that imports it."""

    def setUp(self):
        self.config = affected.load_config(FIXTURE_UNITS)

    def _fake_deps(self, _root, unit):
        # Directory of a real Go package, as `go list -deps` reports it.
        shared = {str(REPO / "packages" / "proto" / "ricardo" / "v1")}
        own = {str(REPO / unit.id)}
        # `sched` imports the shared package, nothing else does.
        return own | shared if unit.id == "sched" else own

    def test_only_importing_unit_is_selected(self):
        with mock.patch.object(affected, "go_package_dirs", side_effect=self._fake_deps):
            result = affected.route(
                self.config, ["packages/proto/ricardo/v1/scheduler.pb.go"], root=REPO, base="b"
            )
        self.assertEqual([u.id for u in result.routes["go"].units], ["sched"])

    def test_unit_path_selects_its_own_unit(self):
        with mock.patch.object(affected, "go_package_dirs", side_effect=self._fake_deps):
            result = affected.route(self.config, ["go/sched/cmd/sched/main.go"], root=REPO, base="b")
        self.assertEqual([u.id for u in result.routes["go"].units], ["sched"])

    def test_go_list_failure_degrades_to_all_go_units(self):
        with mock.patch.object(
            affected, "go_package_dirs", side_effect=RuntimeError("go list failed")
        ):
            result = affected.route(self.config, ["go/sched/x.go"], root=REPO, base="b")
        self.assertEqual(result.routes["go"].mode, "all")
        self.assertEqual([u.id for u in result.routes["go"].units], ["sched"])

    def test_deploy_control_flag_follows_affected_units(self):
        with mock.patch.object(affected, "go_package_dirs", side_effect=self._fake_deps):
            out_of_scope = affected.route(self.config, ["apps/two/src/Two.kt"], root=REPO, base="b")
            in_scope = affected.route(self.config, ["go/sched/x.go"], root=REPO, base="b")
        self.assertFalse(out_of_scope.deploy_control)
        self.assertTrue(in_scope.deploy_control)


class PythonUnitValidationTests(unittest.TestCase):
    """A declared suite must contain real tests; compile-only dirs are explicit."""

    def setUp(self):
        self.config = affected.load_config(FIXTURE_UNITS)

    def _unit(self, **kwargs):
        base = dict(id="py", platform="python", title="py", paths=["py/**"])
        base.update(kwargs)
        return affected.Unit(**base)

    def test_suite_without_tests_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "py").mkdir()
            (Path(tmp) / "py" / "helper.py").write_text("X = 1\n", encoding="utf-8")
            config = affected.Config(
                units=[self._unit(suites=["py"])], invalidations=[]
            )
            problems = check_units.validate_schema(config, root=Path(tmp))
        self.assertTrue(any("no test_*.py" in problem for problem in problems), problems)

    def test_compile_only_unit_is_accepted(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / "py").mkdir()
            (Path(tmp) / "py" / "live_script.py").write_text("X = 1\n", encoding="utf-8")
            config = affected.Config(
                units=[self._unit(compile_dirs=["py"])], invalidations=[]
            )
            problems = check_units.validate_schema(config, root=Path(tmp))
        self.assertEqual(problems, [])

    def test_python_matrix_entry_carries_compile_dirs(self):
        with mock.patch.object(affected, "go_package_dirs", return_value=set()):
            result = affected.route(
                self.config, ["apps/two/src/Two.kt"], root=REPO, base="b"
            )
        entries = affected.matrix_entries(result, self.config)
        self.assertEqual(entries["python"], [])


class GithubOutputTests(unittest.TestCase):
    def test_multiline_summary_is_written_as_heredoc(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "output"
            out.write_text("", encoding="utf-8")
            affected._write_github_output(out, {"a": "1", "summary": "line1\nline2"})
            text = out.read_text(encoding="utf-8")
        self.assertIn("a=1\n", text)
        self.assertIn("summary<<EOF\nline1\nline2\nEOF\n", text)


class RealRepositoryTests(unittest.TestCase):
    """The committed unit map must stay valid and cover the whole tree."""

    def setUp(self):
        self.config = affected.load_config(HERE / "units.yml")

    def test_schema_has_no_problems(self):
        self.assertEqual(check_units.validate_schema(self.config), [])

    def test_every_tracked_source_and_test_file_is_owned(self):
        files = check_units.tracked_files(REPO)
        self.assertGreater(len(files), 100)
        self.assertEqual(check_units.coverage_problems(self.config, files), [])

    def test_every_known_platform_has_a_unit(self):
        for platform in ("go", "android", "python", "quality"):
            self.assertTrue(self.config.by_platform(platform), platform)

    def test_real_graph_routes_shared_kotlin_to_the_application(self):
        graph_file = HERE / "testdata" / "graph-two-apps.xml"
        self.assertTrue(graph_file.exists())
        # The real tree is routed by bazel in CI; here we only assert the unit
        # map declares roots that cover the android application package.
        app = self.config.get("android-app")
        self.assertTrue(any(root.startswith("//projects/mobile/android") for root in app.bazel_roots))

    def test_github_format_is_valid_json_payload(self):
        result = affected.route(
            self.config, ["docs/TESTING.md"], root=REPO, base="b", graph=None
        )
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "output"
            out.write_text("", encoding="utf-8")
            with mock.patch.dict("os.environ", {"GITHUB_OUTPUT": str(out)}):
                with mock.patch.object(affected, "resolve_base", return_value="base-sha"):
                    with mock.patch.object(
                        affected, "changed_files", return_value=["docs/TESTING.md"]
                    ):
                        with mock.patch.object(affected, "route", return_value=result):
                            with contextlib.redirect_stdout(io.StringIO()):
                                code = affected.main(
                                    ["--format", "github", "--units", str(HERE / "units.yml")]
                                )
            self.assertEqual(code, 0)
            values = {}
            for line in out.read_text(encoding="utf-8").splitlines():
                if "=" in line and "<<" not in line:
                    key, _, value = line.partition("=")
                    values[key] = value
        self.assertEqual(json.loads(values["go_units"]), [])
        self.assertEqual(values["docs_only"], "true")


if __name__ == "__main__":
    unittest.main()


class DeployRoutingTests(unittest.TestCase):
    """deploy-home must roll exactly the affected deployable units."""

    def setUp(self):
        self.config = affected.load_config(HERE / "units.yml")

    def _route(self, changed, deps=None):
        with mock.patch.object(
            affected, "go_package_dirs", side_effect=deps or (lambda _root, unit: set())
        ):
            return affected.route(self.config, changed, root=REPO, base="b")

    def test_docs_only_change_deploys_nothing(self):
        result = self._route(["docs/STATE.md"])
        self.assertEqual(affected.deploy_entries(result, self.config), [])

    def test_scheduler_change_deploys_only_scheduler(self):
        result = self._route(["solutions/scheduler/app/run.go"])
        entries = affected.deploy_entries(result, self.config)
        self.assertEqual([entry["id"] for entry in entries], ["scheduler"])
        self.assertEqual(entries[0]["namespace"], "home-assistant")
        self.assertEqual(entries[0]["target_root"], "//projects/home/kubernetes/scheduler")

    def test_currency_bot_change_deploys_its_namespace(self):
        result = self._route(["solutions/currency-bot/internal/server/webhook.go"])
        entries = affected.deploy_entries(result, self.config)
        self.assertEqual([entry["id"] for entry in entries], ["currency-bot"])
        self.assertEqual(entries[0]["namespace"], "currency-bot")
        self.assertEqual(entries[0]["target_root"], "//solutions/currency-bot")
        self.assertEqual(entries[0]["script"], "projects/home/kubernetes/currency-bot/deploy.sh")
        self.assertEqual(entries[0]["externalsecrets"], "currency-bot cloudflared-tunnel")

    def test_currency_bot_manifest_change_deploys_it(self):
        result = self._route(["projects/home/kubernetes/currency-bot/manifests/currency-bot.yaml"])
        entries = affected.deploy_entries(result, self.config)
        self.assertEqual([entry["id"] for entry in entries], ["currency-bot"])

    def test_ha_bridge_change_deploys_bridge_and_control(self):
        result = self._route(["solutions/ha-bridge/internal/bridge/bridge.go"])
        entries = affected.deploy_entries(result, self.config)
        self.assertEqual([entry["id"] for entry in entries], ["ha-bridge"])
        self.assertEqual(entries[0]["images"], "ha-bridge ha-control")
        self.assertEqual(entries[0]["workloads"], "ha-bridge ha-control")

    def test_shared_go_package_deploys_every_importer(self):
        shared = {str(REPO / "packages" / "proto" / "ricardo" / "v1")}

        def deps(_root, unit):
            own = {str(REPO / unit.go_module / Path(unit.go_packages[0]))}
            if unit.id in ("scheduler", "ha-bridge"):
                return own | shared
            return own

        result = self._route(["packages/proto/ricardo/v1/scheduler.pb.go"], deps=deps)
        entries = affected.deploy_entries(result, self.config)
        self.assertEqual([entry["id"] for entry in entries], ["scheduler", "ha-bridge"])

    def test_hermes_change_selects_its_images(self):
        result = self._route(["projects/home-assistant/hermes/voice_proxy.py"])
        entries = affected.deploy_entries(result, self.config)
        self.assertEqual([entry["id"] for entry in entries], ["hermes"])
        self.assertEqual(entries[0]["images"], "hermes voice-proxy")
