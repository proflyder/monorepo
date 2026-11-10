"""Static contracts for the Flux GitOps bundle (no cluster needed)."""
import json
import pathlib
import re
import subprocess
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
NEXUS = ROOT / "projects/home/kubernetes/nexus"
K8S = ROOT / "projects/home/kubernetes"
VERSIONS = (K8S / "versions.sh").read_text()

CONTROLLERS = {"source-controller", "kustomize-controller", "helm-controller"}
FLUX_SETTINGS = {"NEXUS_HOST", "NEXUS_PUBLIC_HOST", "NODE_LAN_IP", "HOME_DOMAIN",
                 "LAN_CIDR", "HAOS_ADDRESS", "PVE_NODE_TARGET"}
PLATFORM = {"platform-external-secrets", "platform-cert-manager",
            "platform-traefik", "platform-routes"}


def kustomize(path):
    result = subprocess.run(
        ["kubectl", "kustomize", str(path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return [doc for doc in yaml.safe_load_all(result.stdout) if doc]


def docs_from(path):
    return [doc for doc in yaml.safe_load_all(path.read_text()) if doc]


def of_kind(docs, kind):
    return [doc for doc in docs if doc.get("kind") == kind]


def by_name(docs, kind):
    return {doc["metadata"]["name"]: doc for doc in of_kind(docs, kind)}


def version(name):
    return re.search(rf"(?m)^{name}=(\S+)$", VERSIONS).group(1)


class Toolkit(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(HERE)
        cls.deployments = {d["metadata"]["name"]: d for d in of_kind(cls.docs, "Deployment")}

    def test_only_the_three_controllers_are_deployed(self):
        self.assertEqual(set(self.deployments), CONTROLLERS)
        text = (HERE / "gotk-components.yaml").read_text()
        self.assertNotIn("notification-controller:v", text)
        self.assertNotIn("image-reflector-controller:v", text)
        self.assertNotIn("image-automation-controller:v", text)
        self.assertNotIn("source-watcher:v", text)

    def test_images_are_pinned_to_the_reviewed_release(self):
        for name, deployment in self.deployments.items():
            images = [
                container["image"]
                for container in deployment["spec"]["template"]["spec"]["containers"]
            ]
            self.assertEqual(len(images), 1, name)
            self.assertRegex(images[0], rf"^ghcr\.io/fluxcd/{re.escape(name)}:v\d+\.\d+\.\d+$")
            self.assertNotIn(":latest", images[0])

    def test_controllers_fit_a_small_node(self):
        for name, deployment in self.deployments.items():
            resources = deployment["spec"]["template"]["spec"]["containers"][0]["resources"]
            limits = resources["limits"]
            requests = resources["requests"]
            self.assertEqual(limits["memory"], "512Mi", name)
            self.assertIn(limits["cpu"], {"500m", "1000m"}, name)
            self.assertLessEqual(int(requests["memory"].rstrip("Mi")), 128, name)
            self.assertLessEqual(int(requests["cpu"].rstrip("m")), 200, name)

    def test_required_crds_are_present(self):
        crds = {doc["metadata"]["name"] for doc in of_kind(self.docs, "CustomResourceDefinition")}
        for name in (
            "gitrepositories.source.toolkit.fluxcd.io",
            "ocirepositories.source.toolkit.fluxcd.io",
            "helmrepositories.source.toolkit.fluxcd.io",
            "helmcharts.source.toolkit.fluxcd.io",
            "kustomizations.kustomize.toolkit.fluxcd.io",
            "helmreleases.helm.toolkit.fluxcd.io",
        ):
            self.assertIn(name, crds)
        self.assertFalse(any("notification" in name for name in crds))


class Cluster(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(HERE / "cluster")
        cls.kustomizations = by_name(cls.docs, "Kustomization")

    def test_controllers_can_fetch_artifacts(self):
        # Without these the kustomize/helm controllers cannot download the
        # Git artifact from the source-controller and every Kustomization
        # except the root one fails with "failed to download archive".
        policies = {d["metadata"]["name"]: d for d in of_kind(self.docs, "NetworkPolicy")}
        artifacts = policies["flux-system-artifacts"]["spec"]
        self.assertEqual(artifacts["podSelector"], {"matchLabels": {"app": "source-controller"}})
        self.assertEqual(artifacts["ingress"][0]["ports"], [{"protocol": "TCP", "port": 9090}])
        egress = policies["flux-system-egress"]["spec"]["egress"]
        self.assertIn(
            {"to": [{"podSelector": {"matchLabels": {"app": "source-controller"}}}],
             "ports": [{"protocol": "TCP", "port": 9090}]},
            egress,
        )

    def test_network_policy_is_narrow(self):
        policies = {d["metadata"]["name"]: d for d in of_kind(self.docs, "NetworkPolicy")}
        ingress = policies["flux-system-ingress"]
        egress = policies["flux-system-egress"]
        self.assertEqual(ingress["spec"]["policyTypes"], ["Ingress"])
        # The only allowed ingress is Prometheus scraping the controller
        # metrics; every other connection into flux-system stays denied.
        self.assertEqual(ingress["spec"]["ingress"], [{
            "from": [{
                "namespaceSelector": {
                    "matchLabels": {"kubernetes.io/metadata.name": "monitoring"},
                },
            }],
            "ports": [{"protocol": "TCP", "port": 8080}],
        }])

        ports = {
            port["port"]
            for rule in egress["spec"]["egress"]
            for port in rule.get("ports", [])
        }
        self.assertTrue({53, 22, 443, 6443}.issubset(ports))

        dns = egress["spec"]["egress"][0]
        self.assertEqual(
            dns["to"][0]["namespaceSelector"]["matchLabels"]["kubernetes.io/metadata.name"],
            "kube-system",
        )
        rendered = yaml.safe_dump(egress)
        self.assertIn("${NODE_LAN_IP}/32", rendered)
        self.assertIn("140.82.112.0/20", rendered)
        # Chart CDNs are not pinned by address; only the source-controller gets
        # general HTTPS egress through a dedicated policy.
        self.assertNotIn("104.18.", rendered)
        self.assertNotIn("142.250.", rendered)

        charts = policies["flux-system-chart-egress"]["spec"]
        self.assertEqual(charts["podSelector"], {"matchLabels": {"app": "source-controller"}})
        self.assertEqual(charts["policyTypes"], ["Egress"])
        rule = charts["egress"][0]
        block = rule["to"][0]["ipBlock"]
        self.assertEqual(block["cidr"], "0.0.0.0/0")
        self.assertEqual(block["except"], [
            "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16",
            "100.64.0.0/10", "169.254.0.0/16",
        ])
        self.assertEqual(rule["ports"], [{"protocol": "TCP", "port": 443}])

    def test_every_kustomization_uses_the_git_source_and_prunes(self):
        self.assertEqual(set(self.kustomizations), {
            "platform", "observability", "observability-rules",
            "vaultwarden", "nexus", "nexus-provision",
            # Applications consume the machine pins branch; their manifests
            # still come from `develop` (GitRepository/flux-system).
            "home-app-pins", "app-wakegrant",
            # FLUX-APPS-PLAN 6.2-6.6: application units applied by Flux.
            "app-intercom", "app-currency-bot", "app-mcp-gateway",
            "app-ha-executors", "app-hermes",
        })
        sources = {
            "home-app-pins": ("GitRepository", "flux-pins"),
        }
        for name, k in self.kustomizations.items():
            kind, source = sources.get(name, ("GitRepository", "flux-system"))
            self.assertEqual(k["spec"]["sourceRef"]["kind"], kind, name)
            self.assertEqual(k["spec"]["sourceRef"]["name"], source, name)
            self.assertTrue(k["spec"]["prune"], name)

    def test_pins_source_is_a_read_only_branch_checkout(self):
        repos = by_name(self.docs, "GitRepository")
        pins = repos["flux-pins"]
        self.assertEqual(pins["metadata"]["namespace"], "flux-system")
        self.assertEqual(pins["spec"]["url"], "ssh://git@github.com/proflyder/monorepo")
        self.assertEqual(pins["spec"]["ref"], {"branch": "deploy/home-apps"})
        # Same read-only deploy key as flux-system: Flux never writes to Git.
        self.assertEqual(pins["spec"]["secretRef"]["name"], "flux-system")
        self.assertEqual(pins["spec"]["interval"], "1m")

    def test_pins_kustomization_publishes_the_digest_config_map(self):
        pins = self.kustomizations["home-app-pins"]
        self.assertEqual(pins["spec"]["path"], "./pins")
        self.assertTrue(pins["spec"]["wait"])
        self.assertEqual(pins["spec"]["dependsOn"], [{"name": "platform"}])

    def test_wakegrant_app_is_applied_by_flux(self):
        app = self.kustomizations["app-wakegrant"]
        spec = app["spec"]
        self.assertEqual(spec["path"], "./projects/home/kubernetes/wakegrant/manifests")
        # Pilot cutover done (FLUX-APPS-PLAN 6.1): Flux owns wakegrant and CI
        # only commits its verified digest to `deploy/home-apps`.
        self.assertFalse(spec.get("suspend", False))
        self.assertTrue(spec["wait"])
        self.assertEqual(spec["timeout"], "5m")
        self.assertEqual(spec["dependsOn"], [
            {"name": "platform"}, {"name": "home-app-pins"},
        ])
        self.assertEqual(spec["postBuild"]["substituteFrom"], [
            {"kind": "ConfigMap", "name": "home-app-images"},
            {"kind": "Secret", "name": "flux-app-settings"},
        ])
        self.assertEqual(spec["healthChecks"], [{
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "name": "wakegrant",
            "namespace": "home-assistant",
        }])

    def test_app_units_are_applied_by_flux_and_wait(self):
        expected = {
            "app-intercom": "./projects/home/kubernetes/intercom/manifests",
            "app-currency-bot": "./projects/home/kubernetes/currency-bot/manifests",
            "app-mcp-gateway": "./projects/home/kubernetes/mcp-gateway/manifests",
            "app-ha-executors": "./projects/home/kubernetes/scheduler/manifests",
            "app-hermes": "./projects/home-assistant/deploy/flux",
        }
        for name, path in expected.items():
            with self.subTest(app=name):
                spec = self.kustomizations[name]["spec"]
                self.assertEqual(spec["path"], path)
                # Cut over (FLUX-APPS-PLAN 6.2-6.6): Flux owns the live
                # workload; a `suspend: true` here would silently freeze it.
                self.assertFalse(spec.get("suspend", False))
                self.assertTrue(spec["wait"])
                self.assertEqual(spec["dependsOn"], [
                    {"name": "platform"}, {"name": "home-app-pins"},
                ])
                self.assertEqual(spec["postBuild"]["substituteFrom"], [
                    {"kind": "ConfigMap", "name": "home-app-images"},
                    {"kind": "Secret", "name": "flux-app-settings"},
                ])
                self.assertTrue(spec["healthChecks"])

    def test_pins_cover_every_flux_unit_image(self):
        # Every image a `deploy.runtime: flux` unit pins must be referenced by
        # its rendered manifests, with the ConfigMap key `pins.mjs` writes.
        # The scheduler's `home-deploy` image is only used by the admin
        # preflight Job, which is not part of the Flux set.
        script = (
            "const { config } = await import(process.argv[1] + '/platform/ci/units.mjs');\n"
            "console.log(JSON.stringify(config.units.filter((u) => "
            "u.deploy && u.deploy.runtime === 'flux')"
            ".map((u) => ({ id: u.id, images: u.deploy.images }))));"
        )
        units = json.loads(subprocess.run(
            ["node", "--input-type=module", "-e", script, str(ROOT)],
            check=True, capture_output=True, text=True,
        ).stdout)
        paths = {
            "wakegrant": K8S / "wakegrant/manifests",
            "intercom": K8S / "intercom/manifests",
            "currency-bot": K8S / "currency-bot/manifests",
            "mcp-gateway": K8S / "mcp-gateway/manifests",
            "scheduler": K8S / "scheduler/manifests",
            "ha-bridge": K8S / "scheduler/manifests",
            "hermes": ROOT / "projects/home-assistant/deploy/flux",
        }
        self.assertEqual({unit["id"] for unit in units}, set(paths))
        unused = {("scheduler", "home-deploy")}
        for unit in units:
            rendered = yaml.safe_dump_all(kustomize(paths[unit["id"]]))
            for image in unit["images"]:
                if (unit["id"], image) in unused:
                    continue
                key = re.sub(r"[^A-Z0-9]+", "_", image.upper()) + "_IMAGE"
                with self.subTest(unit=unit["id"], image=image):
                    self.assertIn("${" + key + "}", rendered)

    def test_platform_umbrella_waits_for_the_chain(self):
        platform = self.kustomizations["platform"]
        self.assertEqual(platform["spec"]["path"],
                         "./projects/home/kubernetes/flux/cluster/platform")
        self.assertTrue(platform["spec"]["wait"])
        # Everything that needs platform internals depends on the umbrella,
        # not on the individual children.
        self.assertEqual(self.kustomizations["observability"]["spec"]["dependsOn"],
                         [{"name": "platform"}])
        self.assertEqual(self.kustomizations["vaultwarden"]["spec"]["dependsOn"],
                         [{"name": "platform"}])
        self.assertEqual(self.kustomizations["observability-rules"]["spec"]["dependsOn"],
                         [{"name": "observability"}])
        self.assertEqual(self.kustomizations["nexus-provision"]["spec"]["dependsOn"],
                         [{"name": "nexus"}])


class Platform(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(HERE / "cluster" / "platform")
        cls.kustomizations = by_name(cls.docs, "Kustomization")

    def test_children_and_order(self):
        self.assertEqual(set(self.kustomizations), PLATFORM)
        expected = {
            "platform-external-secrets": [],
            "platform-cert-manager": ["platform-external-secrets"],
            "platform-traefik": ["platform-cert-manager"],
            "platform-routes": ["platform-traefik", "platform-external-secrets"],
        }
        for name, deps in expected.items():
            self.assertEqual(
                [d["name"] for d in self.kustomizations[name]["spec"].get("dependsOn", [])],
                deps, name,
            )
        self.assertEqual(self.kustomizations["platform-routes"]["spec"]["postBuild"]
                         ["substituteFrom"], [{"kind": "Secret", "name": "flux-settings"}])

    def test_paths(self):
        self.assertEqual(self.kustomizations["platform-external-secrets"]["spec"]["path"],
                         "./projects/home/kubernetes/external-secrets")
        self.assertEqual(self.kustomizations["platform-cert-manager"]["spec"]["path"],
                         "./projects/home/kubernetes/ingress/cert-manager")
        self.assertEqual(self.kustomizations["platform-traefik"]["spec"]["path"],
                         "./projects/home/kubernetes/ingress/traefik")
        self.assertEqual(self.kustomizations["platform-routes"]["spec"]["path"],
                         "./projects/home/kubernetes/ingress/routes")


class HelmReleases(unittest.TestCase):
    """The live releases must be adopted in place: same name/namespace/values."""

    @classmethod
    def setUpClass(cls):
        cls.releases = {}
        cls.sources = {}
        for relative in (
            "external-secrets",
            "ingress/cert-manager",
            "ingress/traefik",
            "observability",
        ):
            docs = kustomize(K8S / relative)
            for release in of_kind(docs, "HelmRelease"):
                cls.releases[release["metadata"]["name"]] = release
            for repo in of_kind(docs, "HelmRepository"):
                cls.sources[repo["metadata"]["name"]] = repo

    def release(self, name):
        return self.releases[name]

    def test_live_names_and_namespaces_are_preserved(self):
        expected = {
            "external-secrets": "external-secrets",
            "cert-manager": "cert-manager",
            "traefik": "traefik",
            "monitoring": "monitoring",
            "loki": "monitoring",
            "alloy": "monitoring",
            "ha-observability": "monitoring",
        }
        self.assertEqual(set(self.releases), set(expected))
        for name, namespace in expected.items():
            release = self.release(name)
            self.assertEqual(release["metadata"]["namespace"], "flux-system", name)
            self.assertEqual(release["spec"]["targetNamespace"], namespace, name)
            # helm-controller must adopt the live release, not create
            # `<targetNamespace>-<name>` with release secrets in flux-system.
            self.assertEqual(release["spec"]["releaseName"], name, name)
            self.assertEqual(release["spec"]["storageNamespace"], namespace, name)

    def test_chart_versions_match_the_reviewed_pins(self):
        expected = {
            "external-secrets": ("external-secrets", "EXTERNAL_SECRETS_CHART_VERSION"),
            "cert-manager": ("cert-manager", "CERT_MANAGER_CHART_VERSION"),
            "traefik": ("traefik", "TRAEFIK_CHART_VERSION"),
            "monitoring": ("kube-prometheus-stack", "PROMETHEUS_CHART_VERSION"),
            "loki": ("loki", "LOKI_CHART_VERSION"),
            "alloy": ("alloy", "ALLOY_CHART_VERSION"),
        }
        for name, (chart, pin) in expected.items():
            spec = self.release(name)["spec"]["chart"]["spec"]
            self.assertEqual(spec["chart"], chart, name)
            self.assertEqual(str(spec["version"]), version(pin), name)
            self.assertEqual(spec["sourceRef"]["namespace"], "flux-system", name)

    def test_values_come_from_generated_config_maps(self):
        expected = {
            "external-secrets": ("external-secrets-values", "external-secrets.yaml"),
            "cert-manager": ("cert-manager-values", "values.yaml"),
            "traefik": ("traefik-values", "values.yaml"),
            "monitoring": ("prometheus-values", "prometheus.yaml"),
            "loki": ("loki-values", "loki.yaml"),
            "alloy": ("alloy-values", "alloy.yaml"),
        }
        for name, (configmap, key) in expected.items():
            values_from = self.release(name)["spec"]["valuesFrom"]
            self.assertEqual(values_from, [{
                "kind": "ConfigMap", "name": configmap, "valuesKey": key,
            }], name)

    def test_dashboards_chart_comes_from_git(self):
        spec = self.release("ha-observability")["spec"]["chart"]["spec"]
        self.assertEqual(spec["chart"], "./projects/home-assistant/observability/chart")
        # The in-repo chart keeps version 0.1.0: without the Revision strategy
        # template edits (dashboards) never reach the live release.
        self.assertEqual(spec.get("reconcileStrategy"), "Revision")
        self.assertEqual(spec["sourceRef"], {
            "kind": "GitRepository", "name": "flux-system", "namespace": "flux-system",
        })

    def test_failed_upgrades_are_not_remediated_automatically(self):
        for name, release in self.releases.items():
            spec = release["spec"]
            self.assertEqual(spec["install"]["remediation"]["retries"], 0, name)
            self.assertEqual(spec["upgrade"]["remediation"]["retries"], 0, name)
            self.assertNotIn("remediateLastFailure", spec["install"]["remediation"], name)
            self.assertNotIn("remediateLastFailure", spec["upgrade"]["remediation"], name)
            self.assertNotIn("driftDetection", spec, name)

    def test_cert_manager_crds_are_upgraded(self):
        spec = self.release("cert-manager")["spec"]
        self.assertEqual(spec["install"]["crds"], "Create")
        self.assertEqual(spec["upgrade"]["crds"], "CreateReplace")

    def test_helm_repositories_are_pinned_to_https(self):
        expected = {
            "external-secrets": "https://charts.external-secrets.io",
            "jetstack": "https://charts.jetstack.io",
            "traefik": "https://traefik.github.io/charts",
            "prometheus-community": "https://prometheus-community.github.io/helm-charts",
            "grafana-community": "https://grafana-community.github.io/helm-charts",
            "grafana": "https://grafana.github.io/helm-charts",
        }
        self.assertEqual({name: repo["spec"]["url"] for name, repo in self.sources.items()},
                         expected)
        for repo in self.sources.values():
            self.assertNotIn("latest", repo["spec"]["url"])


class Substitution(unittest.TestCase):
    """Only the documented private variables may survive rendering."""

    def placeholders(self, path):
        rendered = subprocess.run(
            ["kubectl", "kustomize", str(path)],
            check=True, capture_output=True, text=True,
        ).stdout
        return set(re.findall(r"\$\{([A-Z_]+)\}", rendered))

    def test_component_placeholders(self):
        expected = {
            "ingress/routes": {"HOME_DOMAIN", "LAN_CIDR", "HAOS_ADDRESS"},
            "vaultwarden": {"HOME_DOMAIN", "LAN_CIDR", "NODE_LAN_IP"},
            "observability": {"HOME_DOMAIN", "PVE_NODE_TARGET"},
            "external-secrets": set(),
            "ingress/cert-manager": set(),
            "ingress/traefik": set(),
        }
        for relative, keys in expected.items():
            self.assertEqual(self.placeholders(K8S / relative), keys, relative)

    def test_flux_settings_keys_are_documented(self):
        catalog = (ROOT / "platform/workstation/catalog.json").read_text()
        secrets_doc = (ROOT / "docs/SECRETS.md").read_text()
        readme = (HERE / "README.md").read_text()
        for key in FLUX_SETTINGS:
            self.assertIn(key, catalog)
            self.assertIn(key, secrets_doc)
            self.assertIn(key, readme)

    def test_no_secret_value_is_committed(self):
        managed = [HERE, K8S / "external-secrets", K8S / "ingress",
                   K8S / "observability", K8S / "vaultwarden"]
        for directory in managed:
            for path in directory.rglob("*.yaml"):
                text = path.read_text()
                self.assertNotRegex(text, r"BEGIN (OPENSSH|RSA|EC) PRIVATE KEY", path.name)
                self.assertNotRegex(text, r"(?m)^\s*password:\s*\S", path.name)


class Images(unittest.TestCase):
    def test_external_secrets_values_pin_the_live_digest(self):
        values = yaml.safe_load((K8S / "external-secrets/values/external-secrets.yaml").read_text())
        tag = f"{version('EXTERNAL_SECRETS_IMAGE_VERSION')}@{version('EXTERNAL_SECRETS_IMAGE_DIGEST')}"
        for section in ("image", "webhook", "certController"):
            block = values["image"] if section == "image" else values[section]["image"]
            self.assertEqual(block["tag"], tag, section)

    def test_grafana_pvc_survives_uninstall(self):
        values = yaml.safe_load(
            (K8S / "observability/values/prometheus.yaml").read_text()
        )
        self.assertEqual(
            values["grafana"]["persistence"]["annotations"],
            {"helm.sh/resource-policy": "keep"},
        )

    def test_manual_vaultwarden_pvc_is_never_pruned(self):
        docs = docs_from(K8S / "vaultwarden/manifests/vaultwarden.yaml")
        pvc = next(d for d in docs if d["kind"] == "PersistentVolumeClaim")
        self.assertEqual(pvc["metadata"]["annotations"],
                         {"kustomize.toolkit.fluxcd.io/prune": "disabled",
                          "helm.sh/resource-policy": "keep"})

    def test_vaultwarden_image_matches_versions(self):
        docs = docs_from(K8S / "vaultwarden/manifests/vaultwarden.yaml")
        deployment = by_name(docs, "Deployment")["vaultwarden"]
        image = deployment["spec"]["template"]["spec"]["containers"][0]["image"]
        self.assertEqual(
            image,
            f"docker.io/vaultwarden/server:{version('VAULTWARDEN_IMAGE_VERSION')}"
            f"@{version('VAULTWARDEN_IMAGE_DIGEST')}",
        )


class Secrets(unittest.TestCase):
    def test_external_secrets_read_openbao_paths(self):
        text = (HERE / "external-secrets.yaml").read_text()
        self.assertIn("k8s/flux-system/git-deploy-key", text)
        self.assertIn("k8s/flux-system/settings", text)
        self.assertIn("k8s/flux-system/app-settings", text)
        docs = docs_from(HERE / "external-secrets.yaml")
        targets = {
            d["metadata"]["name"]: d["spec"]["target"]["name"]
            for d in of_kind(docs, "ExternalSecret")
        }
        self.assertEqual(targets, {
            "flux-git-deploy-key": "flux-system",
            "flux-settings": "flux-settings",
            "flux-app-settings": "flux-app-settings",
            "flux-runtime-values": "runtime-values",
        })
        # The live Secret survives removal of the ExternalSecret, so an ESO
        # outage does not strip a running pod of its configuration.
        app = by_name(docs, "ExternalSecret")["flux-app-settings"]
        self.assertEqual(app["spec"]["target"]["deletionPolicy"], "Retain")
        runtime = by_name(docs, "ExternalSecret")["flux-runtime-values"]
        self.assertEqual(runtime["spec"]["target"]["deletionPolicy"], "Retain")
        self.assertEqual(runtime["spec"]["data"], [{
            "secretKey": "values",
            "remoteRef": {"key": "k8s/flux-system/runtime-values", "property": "values"},
        }])
        self.assertNotRegex(text, r"BEGIN (OPENSSH|RSA|EC) PRIVATE KEY")
        self.assertNotRegex(text, r"password\s*:\s*\S")

    def test_catalog_and_docs_name_the_new_paths(self):
        catalog = (ROOT / "platform/workstation/catalog.json").read_text()
        secrets_doc = (ROOT / "docs/SECRETS.md").read_text()
        for path in (
            "k8s/flux-system/git-deploy-key",
            "k8s/flux-system/settings",
        ):
            self.assertIn(path, catalog)
            self.assertIn(path, secrets_doc)


class Sync(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = docs_from(HERE / "sync.yaml")

    def test_git_repository_is_read_only_and_on_develop(self):
        repo = of_kind(self.docs, "GitRepository")[0]
        self.assertEqual(repo["metadata"]["namespace"], "flux-system")
        self.assertEqual(repo["spec"]["url"], "ssh://git@github.com/proflyder/monorepo")
        self.assertEqual(repo["spec"]["ref"], {"branch": "develop"})
        self.assertEqual(repo["spec"]["secretRef"]["name"], "flux-system")

    def test_root_kustomization_reconciles_cluster_dir(self):
        root = of_kind(self.docs, "Kustomization")[0]
        self.assertEqual(root["metadata"]["name"], "flux-system")
        self.assertEqual(root["spec"]["path"], "./projects/home/kubernetes/flux/cluster")
        self.assertEqual(
            root["spec"]["postBuild"]["substituteFrom"],
            [{"kind": "Secret", "name": "flux-settings"}],
        )


class NexusManifests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(NEXUS / "manifests")
        cls.rendered = yaml.safe_dump_all(cls.docs)

    def test_private_values_are_the_only_placeholders(self):
        left = set(re.findall(r"\$\{([A-Z_]+)\}", self.rendered))
        self.assertEqual(left, {"NEXUS_HOST", "NEXUS_PUBLIC_HOST", "NODE_LAN_IP"})

    def test_images_are_pinned_from_versions(self):
        digest = re.search(r"NEXUS_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS).group(1)
        version_ = re.search(r"NEXUS_IMAGE_VERSION=([0-9.]+)", VERSIONS).group(1)
        images = {
            container["image"]
            for d in of_kind(self.docs, "StatefulSet")
            for container in d["spec"]["template"]["spec"]["containers"]
            + d["spec"]["template"]["spec"]["initContainers"]
        }
        self.assertEqual(images, {f"docker.io/sonatype/nexus3:{version_}@{digest}"})

    def test_no_secret_value_is_committed(self):
        for path in (NEXUS / "manifests").glob("*.yaml"):
            text = path.read_text()
            self.assertNotRegex(text, r"BEGIN (OPENSSH|RSA|EC) PRIVATE KEY", path.name)
            self.assertNotRegex(text, r"password\s*:\s*\S", path.name)


class Provisioning(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(NEXUS / "provision")

    def test_job_mounts_the_generated_config_map(self):
        configmap = of_kind(self.docs, "ConfigMap")[0]
        self.assertRegex(configmap["metadata"]["name"], r"^nexus-provision-[a-z0-9]+$")
        job = of_kind(self.docs, "Job")[0]
        self.assertEqual(job["metadata"]["name"], "nexus-provision")
        volumes = job["spec"]["template"]["spec"]["volumes"]
        scripts = next(v for v in volumes if v["name"] == "scripts")
        self.assertEqual(scripts["configMap"]["name"], configmap["metadata"]["name"])
        self.assertIn("ensure_repo maven group maven-public", configmap["data"]["provision.sh"])

    def test_image_pin_matches_versions(self):
        digest = re.search(
            r"NEXUS_PROVISION_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS
        ).group(1)
        version_ = re.search(r"NEXUS_PROVISION_IMAGE_VERSION=([0-9.]+)", VERSIONS).group(1)
        job = of_kind(self.docs, "Job")[0]
        image = job["spec"]["template"]["spec"]["containers"][0]["image"]
        self.assertEqual(image, f"docker.io/curlimages/curl:{version_}@{digest}")

    def test_script_hash_changes_with_content(self):
        first = of_kind(self.docs, "ConfigMap")[0]["metadata"]["name"]
        self.assertNotEqual(first, "nexus-provision")


class WakegrantApp(unittest.TestCase):
    """The Flux manifest set for the wakegrant pilot (FLUX-APPS-PLAN 6.1)."""

    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(K8S / "wakegrant/manifests")
        cls.rendered = yaml.safe_dump_all(cls.docs)

    def test_object_set_matches_the_ci_deploy(self):
        self.assertEqual(
            {d["kind"] for d in self.docs},
            {"PersistentVolumeClaim", "Deployment", "Service", "ExternalSecret"},
        )

    def test_only_pins_and_app_settings_variables_remain(self):
        left = set(re.findall(r"\$\{([A-Z_]+)\}", self.rendered))
        self.assertEqual(left, {"WAKEGRANT_IMAGE", "BUILD_ID", "WAKE_GRANT_NODE_PORT"})
        # ROLLOUT_ID is a CI-only variable; kustomize-controller would fail on
        # an undefined one, so the Flux patch replaces it with the digest.
        self.assertNotIn("ROLLOUT_ID", left)
        self.assertNotIn("WAKE_GRANT_IMAGE", left)

    def test_pvc_is_never_pruned(self):
        pvc = by_name(self.docs, "PersistentVolumeClaim")["wakegrant-data"]
        self.assertEqual(
            pvc["metadata"]["annotations"],
            {"kustomize.toolkit.fluxcd.io/prune": "disabled"},
        )

    def test_rollout_annotation_tracks_the_digest(self):
        deployment = by_name(self.docs, "Deployment")["wakegrant"]
        annotations = deployment["spec"]["template"]["metadata"]["annotations"]
        self.assertEqual(annotations["proflyder.dev/rollout"], "${WAKEGRANT_IMAGE}")
        self.assertEqual(
            deployment["spec"]["template"]["spec"]["containers"][0]["image"],
            "${WAKEGRANT_IMAGE}",
        )


class MigratedApps(unittest.TestCase):
    """Flux manifest sets for FLUX-APPS-PLAN 6.2-6.5."""

    EXPECTED = {
        "intercom": ({"Deployment", "Service", "ExternalSecret"},
                     {"INTERCOM_IMAGE", "BUILD_ID", "INTERCOM_NODE_PORT"}),
        "currency-bot": ({"PersistentVolumeClaim", "Deployment", "Service",
                          "NetworkPolicy", "ServiceMonitor", "ExternalSecret"},
                         {"CURRENCY_BOT_IMAGE", "BUILD_ID", "CURRENCY_BOT_NODE_PORT",
                          "CURRENCY_WEBHOOK_URL", "LAN_CIDR", "NODE_LAN_IP"}),
        "mcp-gateway": ({"Deployment", "Service", "NetworkPolicy", "ServiceMonitor",
                         "ConfigMap", "ServiceAccount", "Role", "RoleBinding",
                         "ExternalSecret"},
                        {"MCP_GATEWAY_IMAGE", "BUILD_ID", "LAN_CIDR", "NODE_LAN_IP",
                         "HAOS_IP", "STATION_IP", "BB_CT_IP",
                         "BB_PROJECT_STATION_LAB", "BB_PROJECT_ENGLISH_NOTES",
                         "BB_DISPATCHER_THREAD_ID"}),
        "scheduler": ({"PersistentVolumeClaim", "Deployment", "Service",
                       "ExternalSecret"},
                      {"SCHEDULER_IMAGE", "HA_BRIDGE_IMAGE", "HA_CONTROL_IMAGE",
                       "HA_SCENARIO_IMAGE", "BUILD_ID", "HA_URL",
                       "SCHEDULER_NODE_PORT", "CONTROL_NODE_PORT"}),
    }

    @classmethod
    def setUpClass(cls):
        cls.rendered = {
            name: yaml.safe_dump_all(kustomize(K8S / name / "manifests"))
            for name in cls.EXPECTED
        }

    def test_object_sets_match_the_ci_deploy(self):
        for name, (kinds, _) in self.EXPECTED.items():
            with self.subTest(unit=name):
                docs = kustomize(K8S / name / "manifests")
                self.assertEqual({d["kind"] for d in docs}, kinds)

    def test_only_pins_and_app_settings_variables_remain(self):
        for name, (_, keys) in self.EXPECTED.items():
            with self.subTest(unit=name):
                left = set(re.findall(r"\$\{([A-Z_]+)\}", self.rendered[name]))
                self.assertEqual(left, keys)
                self.assertNotIn("ROLLOUT_ID", left)

    def test_pvcs_are_never_pruned(self):
        for name in ("currency-bot", "scheduler"):
            for pvc in of_kind(kustomize(K8S / name / "manifests"),
                               "PersistentVolumeClaim"):
                with self.subTest(unit=name, pvc=pvc["metadata"]["name"]):
                    self.assertEqual(
                        pvc["metadata"]["annotations"],
                        {"kustomize.toolkit.fluxcd.io/prune": "disabled"},
                    )

    def test_namespace_manifest_is_not_in_the_flux_set(self):
        # currency-bot's namespace is cluster-scoped and admin-owned.
        self.assertNotIn("kind: Namespace", self.rendered["currency-bot"])

    def test_ci_rbac_is_not_in_the_mcp_gateway_set(self):
        # `ci/rbac.yaml` only grants `ci-deploy`; it goes away with that path.
        self.assertNotIn("ci-deploy", self.rendered["mcp-gateway"])

    def test_executor_steady_state_is_pinned(self):
        docs = kustomize(K8S / "scheduler/manifests")
        deployments = by_name(docs, "Deployment")
        self.assertEqual(deployments["ha-bridge"]["spec"]["replicas"], 1)
        self.assertEqual(deployments["ha-control"]["spec"]["replicas"], 1)
        env = {
            d["metadata"]["name"]: {
                item["name"]: item.get("value")
                for item in d["spec"]["template"]["spec"]["containers"][0]["env"]
            }
            for d in deployments.values()
        }
        self.assertEqual(env["ha-bridge"]["BRIDGE_EXECUTE"], "true")
        self.assertEqual(env["ha-control"]["CONTROL_EXECUTE"], "true")
        self.assertEqual(env["ha-scenario"]["SCENARIO_EXECUTE"], "true")

    def test_rollout_annotation_tracks_the_unit_digest(self):
        for name, (_, _) in self.EXPECTED.items():
            docs = kustomize(K8S / name / "manifests")
            for deployment in of_kind(docs, "Deployment"):
                image = deployment["spec"]["template"]["spec"]["containers"][0]["image"]
                annotation = deployment["spec"]["template"]["metadata"]["annotations"][
                    "proflyder.dev/rollout"]
                with self.subTest(unit=name, deployment=deployment["metadata"]["name"]):
                    # cloudflared is a reviewed versions.sh literal, everything
                    # else is the pins-branch placeholder.
                    if "cloudflared" in image:
                        self.assertIn("cloudflare/cloudflared:", annotation)
                    else:
                        self.assertEqual(annotation, image)

    def test_catalog_config_map_keeps_its_stable_name(self):
        configmaps = by_name(kustomize(K8S / "mcp-gateway/manifests"), "ConfigMap")
        catalog = configmaps["mcp-gateway-catalog"]
        self.assertEqual(catalog["metadata"]["namespace"], "home-assistant")
        self.assertEqual(set(catalog["data"]), {"catalog.yaml", "bb-targets.yaml"})

    def test_cloudflared_pin_matches_versions(self):
        image = version("CLOUDFLARED_IMAGE")
        self.assertIn(image, (K8S / "currency-bot/manifests/kustomization.yaml").read_text())


class HermesApp(unittest.TestCase):
    """The Flux Helm release for the Hermes runtime (FLUX-APPS-PLAN 6.6)."""

    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(ROOT / "projects/home-assistant/deploy/flux")
        cls.release = by_name(cls.docs, "HelmRelease")["runtime"]

    def test_adopts_the_live_release_in_place(self):
        spec = self.release["spec"]
        self.assertEqual(self.release["metadata"]["namespace"], "flux-system")
        self.assertEqual(spec["releaseName"], "runtime")
        self.assertEqual(spec["targetNamespace"], "home-assistant")
        self.assertEqual(spec["storageNamespace"], "home-assistant")

    def test_chart_comes_from_git(self):
        chart = self.release["spec"]["chart"]["spec"]
        self.assertEqual(chart["chart"], "./projects/home-assistant/deploy/chart")
        self.assertEqual(chart["reconcileStrategy"], "Revision")
        self.assertEqual(chart["sourceRef"], {
            "kind": "GitRepository", "name": "flux-system", "namespace": "flux-system",
        })

    def test_values_come_from_the_eso_secret(self):
        self.assertEqual(self.release["spec"]["valuesFrom"], [{
            "kind": "Secret", "name": "runtime-values", "valuesKey": "values",
        }])

    def test_images_are_pinned_by_the_post_renderer(self):
        rendered = yaml.safe_dump(self.release)
        for key in ("HERMES_IMAGE", "VOICE_PROXY_IMAGE",
                    "HEALTH_EXPORTER_IMAGE", "HOME_DEPLOY_IMAGE"):
            self.assertIn("${" + key + "}", rendered)

    def test_failed_upgrades_roll_back(self):
        remediation = self.release["spec"]["upgrade"]["remediation"]
        self.assertEqual(remediation["strategy"], "rollback")
        self.assertTrue(remediation["remediateLastFailure"])
        # HelmRelease v2 has no `wait` field; only Kustomization does. A stray
        # one fails the CRD dry-run and blocks the whole app Kustomization.
        self.assertNotIn("wait", self.release["spec"])

    def test_smoke_hook_is_a_post_upgrade_job(self):
        template = (ROOT / "projects/home-assistant/deploy/chart/templates"
                    / "smoke-hook.yaml").read_text()
        self.assertIn("helm.sh/hook: post-upgrade", template)
        self.assertIn("hermes assist-smoke", template)
        # Disabled by default: the CI/admin `helm upgrade` path is unchanged.
        self.assertIn("default dict", template)
        smoke = self.release["spec"]["values"]["smoke"]
        self.assertTrue(smoke["enabled"])
        self.assertEqual(smoke["image"], "${HOME_DEPLOY_IMAGE}")


class FluxAlerts(unittest.TestCase):
    """Delivery alerts for Flux-managed applications."""

    RULES = K8S / "observability/rules"

    @classmethod
    def setUpClass(cls):
        cls.document = yaml.safe_load((cls.RULES / "prometheusrule-flux.yaml").read_text())
        cls.rules = {
            rule["alert"]: rule
            for group in cls.document["spec"]["groups"]
            for rule in group["rules"]
        }

    def test_required_alerts_are_present(self):
        self.assertEqual(set(self.rules), {
            "FluxKustomizationNotReady", "HelmReleaseNotReady", "PinsStale",
        })
        self.assertEqual(self.document["metadata"]["namespace"], "monitoring")

    def test_alerts_are_warnings_on_flux_metrics(self):
        for name, rule in self.rules.items():
            with self.subTest(alert=name):
                self.assertEqual(rule["labels"]["severity"], "warning")
                self.assertTrue(rule["annotations"]["summary"].strip())
                self.assertTrue(rule["annotations"]["description"].strip())
        self.assertIn("gotk_reconcile_condition", self.rules["FluxKustomizationNotReady"]["expr"])
        self.assertIn("gotk_reconcile_condition", self.rules["HelmReleaseNotReady"]["expr"])
        self.assertIn("kind=\"Kustomization\"", self.rules["FluxKustomizationNotReady"]["expr"])
        self.assertIn("kind=\"HelmRelease\"", self.rules["HelmReleaseNotReady"]["expr"])
        # Pins staleness is the missing ConfigMap or an unhealthy source.
        self.assertIn("home-app-images", self.rules["PinsStale"]["expr"])
        self.assertIn("flux-pins", self.rules["PinsStale"]["expr"])

    def test_rules_kustomization_ships_the_flux_scrape_and_alerts(self):
        kustomization = yaml.safe_load((self.RULES / "kustomization.yaml").read_text())
        for name in ("prometheusrule-flux.yaml", "podmonitor-flux.yaml"):
            self.assertIn(name, kustomization["resources"])

    def test_pod_monitor_scrapes_the_controller_metrics_port(self):
        monitor = yaml.safe_load((self.RULES / "podmonitor-flux.yaml").read_text())
        self.assertEqual(monitor["kind"], "PodMonitor")
        self.assertEqual(monitor["spec"]["namespaceSelector"], {"matchNames": ["flux-system"]})
        self.assertEqual(
            monitor["spec"]["selector"]["matchLabels"],
            {"app.kubernetes.io/part-of": "flux"},
        )
        self.assertEqual(monitor["spec"]["podMetricsEndpoints"][0]["port"], "http-prom")


if __name__ == "__main__":
    unittest.main()
