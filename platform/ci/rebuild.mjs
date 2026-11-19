#!/usr/bin/env node
/**
 * Unit selection for the manual "rebuild and deploy" workflow.
 *
 * `deploy-home` selects units from a diff; `deploy-services.yml` selects them
 * from checkboxes, because an operator rebuilding a broken service asked for
 * them explicitly. Both use the same unit map (`units.mjs`) and the same matrix
 * shapes (`affected.mjs`), so a rebuilt unit is published and pinned through the
 * identical reusable workflows — only the selection differs.
 *
 * GitHub allows at most ten `workflow_dispatch` inputs, and the map has more
 * deployable units than that. The groups below are the operator-facing
 * checkboxes; every deployable unit belongs to exactly one group, which the
 * tests enforce, so a new unit cannot be undeployable by hand. `--units` is the
 * exact escape hatch for a single unit id.
 */

import fs from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

import { deployImageFields, loadConfig } from "./affected.mjs";

/**
 * Checkbox groups of the manual rebuild workflow. Ids are deployable unit ids
 * from `platform/ci/units.mjs`'s `deploy` blocks plus the native units.
 */
export const GROUPS = {
  scheduler: ["scheduler"],
  "ha-bridge": ["ha-bridge"],
  // Small stateless services that share the same build shape.
  "home-services": ["wakegrant", "intercom", "phone-push", "currency-bot", "mcp-gateway"],
  // Configuration renderers.
  "ha-config": ["ha-config-sync", "hermes-config"],
  hermes: ["hermes"],
  pulsar: ["pulsar"],
  "logon-provider": ["logon-provider"],
};

/** Native units are published and promoted, not pinned by `_pin.yml`. */
export function isNative(unit) {
  return unit.platform === "native";
}

/** Deployable units: images for Flux, or a native OCI release. */
export function deployableUnits(config) {
  return config.units.filter((unit) => isNative(unit) || unit.deployImages.length > 0);
}

/** Every deployable unit id, in unit-map order. */
export function deployableIds(config) {
  return deployableUnits(config).map((unit) => unit.id);
}

/**
 * Resolve the requested selection into unit ids.
 *
 * `all` wins over the groups; `units` (a comma separated id list) narrows or
 * replaces them, which is how an operator picks one unit without a checkbox.
 * Unknown ids and empty selections throw: a rebuild that deploys nothing is
 * always a mistake, never a no-op.
 */
export function selectIds({ all = false, groups = [], units = "", config }) {
  const known = deployableIds(config);
  const wanted = String(units || "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

  let selected;
  if (all) {
    selected = [...known];
  } else if (wanted.length) {
    selected = wanted;
  } else {
    selected = groups.flatMap((group) => {
      const ids = GROUPS[group];
      if (!ids) {
        throw new Error(`unknown group '${group}'`);
      }
      return ids;
    });
  }

  const unknown = [...new Set(selected)].filter((id) => !known.includes(id));
  if (unknown.length) {
    throw new Error(
      `unknown deployable unit(s): ${unknown.map((id) => `'${id}'`).join(", ")}; ` +
        `known ids: ${known.join(", ")}`,
    );
  }
  const unique = known.filter((id) => selected.includes(id));
  if (!unique.length) {
    throw new Error("nothing selected: pass a group, `--all` or `--units <id,...>`");
  }
  return unique;
}

/** Matrix entry for `_unit.yml` (image rebuild) of one deployable unit. */
export function imageEntry(unit) {
  const kind = unit.platform === "python" ? "python" : "bazel";
  return {
    id: unit.id,
    kind,
    test_targets: unit.testTargets.join(" "),
    build_targets: unit.buildTargets.join(" "),
    artifact: unit.artifact,
    artifact_name: unit.artifactName,
    suites: unit.suites.join(" "),
    compile: unit.compileDirs.join(" "),
    deploy_tool: unit.pythonDeployTool,
    ...deployImageFields(unit),
    optional_images: unit.deployOptionalImages.join(" "),
  };
}

/** Matrix entry for `_native.yml` (build + promote) of one native unit. */
export function nativeEntry(unit) {
  return {
    id: unit.id,
    kind: unit.nativeKind,
    workdir: unit.nativeWorkdir,
    build: unit.nativeBuild,
    test: unit.nativeTest,
    artifacts: unit.nativeArtifacts.join(" "),
    checksums: unit.nativeChecksums,
    publish_repository: unit.nativePublishRepository,
    publish_tags: unit.nativePublishTags.join(" "),
    publish_script: unit.nativePublishScript,
    publish_attest_glob: unit.nativePublishAttestGlob,
  };
}

/** Split a selection into the image matrix, the native matrix and the station. */
export function plan(config, selection) {
  const ids = selectIds({ ...selection, config });
  const units = ids.map((id) => config.units.find((unit) => unit.id === id));
  const images = units.filter((unit) => !isNative(unit)).map(imageEntry);
  const native = units.filter(isNative).map(nativeEntry);
  return { ids, images, native, station: ids.includes("pulsar") };
}

function parseArgs(argv) {
  const args = { all: false, groups: [], units: "", githubOutput: false };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--all") {
      args.all = true;
    } else if (flag === "--units") {
      args.units = argv[index + 1];
      index += 1;
    } else if (flag === "--github-output") {
      args.githubOutput = true;
    } else if (flag.startsWith("--")) {
      args.groups.push(flag.slice(2));
    } else {
      throw new Error(`unexpected argument '${flag}'`);
    }
  }
  return args;
}

export async function main(argv = process.argv.slice(2)) {
  const args = parseArgs(argv);
  const config = loadConfig();
  const result = plan(config, args);
  const lines = [
    `selected: ${result.ids.join(", ")}`,
    `image units: ${result.images.map((entry) => entry.id).join(", ") || "-"}`,
    `native units: ${result.native.map((entry) => entry.id).join(", ") || "-"}`,
    `station: ${result.station}`,
  ];
  for (const line of lines) {
    console.log(line);
  }
  if (args.githubOutput) {
    const entries = {
      image_units: JSON.stringify(result.images),
      native_units: JSON.stringify(result.native),
      station: String(result.station),
      selected: result.ids.join(","),
    };
    const file = process.env.GITHUB_OUTPUT;
    if (file) {
      fs.appendFileSync(
        file,
        Object.entries(entries)
          .map(([key, value]) => `${key}=${value}\n`)
          .join(""),
      );
    }
  }
  return 0;
}

/** Run the selection in a test without a subprocess. */
export function runWithConfig(config, argv) {
  return plan(config, parseArgs(argv));
}

if (process.argv[1] && pathURLMatches(process.argv[1])) {
  main().then(
    (code) => {
      process.exitCode = code;
    },
    (error) => {
      console.error(error?.stack || String(error));
      process.exitCode = 1;
    },
  );
}

function pathURLMatches(script) {
  return pathToFileURL(script).href === import.meta.url;
}
