import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { config } from "./units.mjs";

const SYSTEM_BUILD = new URL("../../solutions/mcp-gateway/crates/mcp-gateway/src/system/build.rs", import.meta.url);

function k8sQueries() {
  const source = readFileSync(SYSTEM_BUILD, "utf8");
  const start = source.indexOf("pub const K8S_QUERIES");
  const end = source.indexOf("];", start);
  assert.ok(start >= 0 && end > start, "K8S_QUERIES table not found");
  return source.slice(start, end);
}

function deployedNamespaces() {
  return new Set(
    config.units
      .map((unit) => unit.deploy)
      .filter((deploy) => deploy && Array.isArray(deploy.workloads) && deploy.namespace)
      .map((deploy) => deploy.namespace),
  );
}

test("every deployed Kubernetes unit appears in the System tab cluster section", () => {
  const queries = k8sQueries();
  assert.doesNotMatch(queries, /namespace\s*(=|!=|=~|!~)\s*\\?"/, "cluster queries must not filter namespaces");
  assert.doesNotMatch(queries, /\bpod\s*(=|!=|=~|!~)\s*\\?"/, "cluster queries must not filter pods");
  assert.match(queries, /by \(namespace, pod/, "cluster rows are grouped by namespace and pod");
  assert.ok(deployedNamespaces().size > 0, "units.mjs declares deployed namespaces");
});
