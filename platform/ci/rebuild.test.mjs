/**
 * Contracts for the manual rebuild selection.
 *
 * The workflow exposes checkboxes, not unit ids: the grouping must cover every
 * deployable unit exactly once, resolve to real ids, and produce the same
 * matrix shapes `_unit.yml` / `_native.yml` / `_pin.yml` consume. A new unit in
 * `units.mjs` without a group would be undeployable by hand, which is what these
 * tests prevent.
 */

import assert from "node:assert/strict";
import test from "node:test";

import { loadConfig } from "./affected.mjs";
import * as rebuild from "./rebuild.mjs";

const config = loadConfig();

test("rebuild: every deployable unit belongs to exactly one group", () => {
  const grouped = Object.values(rebuild.GROUPS).flat();
  assert.equal(new Set(grouped).size, grouped.length, "a unit is in two groups");
  assert.deepEqual(
    [...grouped].sort(),
    [...rebuild.deployableIds(config)].sort(),
    "groups must cover exactly the deployable units (deploy images + native)",
  );
});

test("rebuild: group ids are real deployable units", () => {
  const known = new Set(rebuild.deployableIds(config));
  for (const [group, ids] of Object.entries(rebuild.GROUPS)) {
    assert.ok(ids.length, `${group} is empty`);
    for (const id of ids) {
      assert.ok(known.has(id), `${group}: '${id}' is not deployable`);
    }
  }
});

test("rebuild: --all selects images, native units and the station", () => {
  const plan = rebuild.plan(config, { all: true, groups: [], units: "" });
  assert.deepEqual(plan.ids, rebuild.deployableIds(config));
  assert.ok(plan.images.some((entry) => entry.id === "scheduler"));
  assert.deepEqual(plan.native.map((entry) => entry.id), ["logon-provider", "pulsar"]);
  assert.equal(plan.station, true);
});

test("rebuild: a group rebuilds only its units and no station", () => {
  const plan = rebuild.plan(config, { all: false, groups: ["home-services"], units: "" });
  assert.deepEqual(plan.ids, rebuild.GROUPS["home-services"]);
  assert.equal(plan.station, false);
  assert.deepEqual(plan.native, []);
});

test("rebuild: the pulsar group selects the station release", () => {
  const plan = rebuild.plan(config, { all: false, groups: ["pulsar"], units: "" });
  assert.deepEqual(plan.ids, ["pulsar"]);
  assert.deepEqual(plan.images, []);
  assert.deepEqual(plan.native.map((entry) => entry.id), ["pulsar"]);
  assert.equal(plan.station, true);
});

test("rebuild: an explicit id list overrides the groups", () => {
  const plan = rebuild.plan(config, {
    all: false,
    groups: ["pulsar"],
    units: "wakegrant, scheduler",
  });
  assert.deepEqual(plan.ids, ["scheduler", "wakegrant"]);
  assert.equal(plan.station, false);
});

test("rebuild: unknown ids and empty selections fail loudly", () => {
  assert.throws(
    () => rebuild.plan(config, { all: false, groups: [], units: "nope" }),
    /unknown deployable unit/,
  );
  assert.throws(
    () => rebuild.plan(config, { all: false, groups: [], units: "" }),
    /nothing selected/,
  );
  assert.throws(
    () => rebuild.plan(config, { all: false, groups: ["nope"], units: "" }),
    /unknown group/,
  );
});

test("rebuild: image entries carry the fields _unit.yml and _pin.yml need", () => {
  const plan = rebuild.plan(config, { all: false, groups: ["hermes"], units: "" });
  const [entry] = plan.images;
  assert.equal(entry.id, "hermes");
  assert.equal(entry.kind, "python");
  assert.equal(entry.images, "hermes voice-proxy health-exporter home-deploy");
  assert.equal(entry.image_count, 4);
  assert.deepEqual(JSON.parse(entry.image_list), [
    "hermes",
    "voice-proxy",
    "health-exporter",
    "home-deploy",
  ]);
  assert.equal(entry.builder, "mixed");
  // Pinning needs the image names and the optional siblings.
  assert.equal(typeof entry.optional_images, "string");
});

test("rebuild: a bazel unit passes its own targets and image set", () => {
  const plan = rebuild.plan(config, { all: false, groups: ["scheduler"], units: "" });
  const [entry] = plan.images;
  assert.equal(entry.id, "scheduler");
  assert.equal(entry.kind, "bazel");
  assert.ok(entry.test_targets.includes("//"), "test targets come from the unit map");
  assert.ok(entry.images.startsWith("scheduler"));
  // build-images.sh owns home-deploy for a bazel unit that does not list it.
  assert.ok(JSON.parse(entry.image_list).includes("home-deploy"));
});

test("rebuild: native entries carry the publish contract", () => {
  const plan = rebuild.plan(config, { all: false, groups: ["pulsar"], units: "" });
  const [entry] = plan.native;
  assert.equal(entry.kind, "pulsar");
  assert.equal(entry.workdir, "projects/station/pulsar");
  assert.equal(entry.publish_repository, "ghcr.io/proflyder/pulsar-firmware");
  assert.equal(entry.publish_tags, "{version} latest");
  assert.equal(typeof entry.publish_script, "string");
});
