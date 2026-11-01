"""Render contracts that protect state and dashboard delivery during upgrades."""
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
import yaml

HERE = Path(__file__).resolve().parent
OBS = HERE.parent / "observability/chart"


@unittest.skipUnless(shutil.which("helm"), "Helm is required for chart contracts")
class ChartContracts(unittest.TestCase):
    def test_public_version_manifest_is_tracked_and_not_ignored(self):
        root = HERE.parents[2]
        manifest = root / "projects/home/kubernetes/versions.sh"
        self.assertTrue(manifest.is_file())
        result = subprocess.run(["git", "check-ignore", str(manifest)], cwd=root, capture_output=True)
        self.assertEqual(result.returncode, 1, "Public versions must not match private env ignore rules")
        tracked = subprocess.check_output(["git", "ls-files", "--error-unmatch", "projects/home/kubernetes/versions.sh"], cwd=root)
        self.assertTrue(tracked)

    def test_runtime_is_single_writer_and_retains_data(self):
        values = {"nodeName": "example-node", "storage": {
            "data": {"size": "1Gi", "path": "/srv/example/data"}},
            "workloads": {"example": {"replicas": 1, "containers": [{
                "name": "example", "image": "example.invalid/runtime:1"}],
                "volumes": []}}}
        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml") as f:
            yaml.safe_dump(values, f)
            f.flush()
            rendered = subprocess.check_output(["helm", "template", "test", str(HERE / "chart"), "-f", f.name])
        docs = list(yaml.safe_load_all(rendered))
        deployment = next(x for x in docs if x["kind"] == "Deployment")
        self.assertEqual(deployment["spec"]["replicas"], 1)
        self.assertEqual(deployment["spec"]["strategy"]["type"], "Recreate")
        pod = deployment["spec"]["template"]["spec"]
        self.assertTrue(pod["hostNetwork"])
        self.assertEqual(pod["dnsPolicy"], "ClusterFirstWithHostNet")
        self.assertFalse(pod["automountServiceAccountToken"])
        volume = next(x for x in docs if x["kind"] == "PersistentVolume")
        self.assertEqual(volume["spec"]["persistentVolumeReclaimPolicy"], "Retain")
        self.assertEqual(volume["spec"]["local"]["path"], "/srv/example/data")

    def test_every_dashboard_is_packaged_once_with_stable_uid(self):
        rendered = subprocess.check_output(["helm", "template", "dashboards", str(OBS), "-n", "monitoring"])
        maps = [x for x in yaml.safe_load_all(rendered) if x and x["kind"] == "ConfigMap"]
        files = list((OBS / "dashboards").glob("*.json"))
        self.assertEqual(len(maps), len(files))
        uids = set()
        for config in maps:
            self.assertEqual(config["metadata"]["labels"]["grafana_dashboard"], "1")
            self.assertIn("grafana_folder", config["metadata"]["annotations"])
            for data in config["data"].values():
                dashboard = json.loads(data)
                self.assertNotIn(dashboard["uid"], uids)
                uids.add(dashboard["uid"])
                self.assertTrue(dashboard["panels"])


def _hcl_unescape(value):
    """Decode the small HCL string escape subset used by the Alloy config."""
    return re.sub(
        r"\\(.)",
        lambda m: {"n": "\n", "t": "\t", "r": "\r"}.get(m.group(1), m.group(1)),
        value,
    )


class AlloyPrivacy(unittest.TestCase):
    """Alloy redaction is the only privacy boundary for the collected pod logs."""

    @classmethod
    def setUpClass(cls):
        root = HERE.parents[2]
        values = yaml.safe_load(
            (root / "platform/observability/values/alloy.yaml").read_text()
        )
        cls.content = values["alloy"]["configMap"]["content"]
        cls.rules = []
        for block in re.findall(r"stage\.replace\s*\{\n(.*?)\n\s*\}", cls.content, re.S):
            expression = re.search(r'expression\s*=\s*"((?:[^"\\]|\\.)*)"', block)
            replacement = re.search(r'replace\s*=\s*"((?:[^"\\]|\\.)*)"', block)
            cls.rules.append(
                (_hcl_unescape(expression.group(1)), _hcl_unescape(replacement.group(1)))
            )

    def redact(self, line):
        # Alloy's stage.replace substitutes capture groups, not the whole match;
        # an expression without a group would redact nothing.
        for expression, replacement in self.rules:
            pattern = re.compile(expression)
            self.assertGreater(pattern.groups, 0, expression)

            def sub(match, replacement=replacement):
                text, start = match.group(0), match.start(0)
                out, pos = [], 0
                for g in range(1, pattern.groups + 1):
                    if match.start(g) < 0:
                        continue
                    a, b = match.start(g) - start, match.end(g) - start
                    out.append(text[pos:a] + replacement)
                    pos = b
                return "".join(out) + text[pos:]

            line = pattern.sub(sub, line)
        return line

    def test_replacement_stages_are_configured(self):
        self.assertGreaterEqual(len(self.rules), 2)

    def test_currency_bot_namespace_is_collected(self):
        keep = re.search(
            r'source_labels = \["__meta_kubernetes_namespace"\]\s*'
            r'regex = "([^"]+)"',
            self.content,
        )
        self.assertIn("currency-bot", keep.group(1).split("|"))

    def test_openbao_audit_namespace_is_collected(self):
        keep = re.search(
            r'source_labels = \["__meta_kubernetes_namespace"\]\s*'
            r'regex = "([^"]+)"',
            self.content,
        )
        self.assertIn("openbao", keep.group(1).split("|"))

    def test_telegram_bot_token_is_redacted(self):
        secret = "TESTTESTTESTTESTTESTTESTTESTTESTTES"
        self.assertEqual(len(secret), 35)
        token = f"000000000:{secret}"
        output = self.redact(
            "level=ERROR msg=send_failed "
            f"url=https://api.telegram.org/bot{token}/sendMessage"
        )
        self.assertNotIn(token, output)
        self.assertNotIn(secret, output)
        self.assertIn("[REDACTED]", output)

    def test_bearer_token_is_redacted(self):
        output = self.redact(
            'level=WARN msg="request" header="Authorization: Bearer test-bearer-value"'
        )
        self.assertNotIn("test-bearer-value", output)

    def test_key_value_secret_is_redacted(self):
        output = self.redact(
            'level=INFO msg=config token="test-api-token" password=test-password'
        )
        self.assertNotIn("test-api-token", output)
        self.assertNotIn("test-password", output)

    def test_ordinary_logs_are_unchanged(self):
        line = (
            'level=INFO msg="rate updated" event=currency_rate_updated '
            "currency_from=USD currency_to=KZT"
        )
        self.assertEqual(self.redact(line), line)


if __name__ == "__main__":
    unittest.main()
