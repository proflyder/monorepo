#!/usr/bin/env node

import { readFile, readdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DEF_DIR = dirname(fileURLToPath(import.meta.url));
const SERVICE = "home.v1.ScenarioService";

export function normalizeLabel(label) {
  return String(label ?? "").trim().toLowerCase();
}

export function planSeed(definitions, existing) {
  const seen = new Set(
    (Array.isArray(existing) ? existing : []).map((row) => normalizeLabel(row?.label)),
  );
  const planned = [];
  for (const definition of definitions) {
    const key = normalizeLabel(definition?.label);
    if (key === "") {
      throw new Error("scenario definition without a label");
    }
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    planned.push(definition);
  }
  return planned;
}

export function seedKey(definition) {
  const slug = String(definition?.slug ?? "").trim();
  if (slug === "") {
    throw new Error(`scenario ${definition?.label ?? "<unknown>"} has no slug`);
  }
  return `seed-${slug}`;
}

export async function loadDefinitions(dir = DEF_DIR) {
  const files = (await readdir(dir)).filter((name) => name.endsWith(".json")).sort();
  const definitions = [];
  for (const name of files) {
    const definition = JSON.parse(await readFile(join(dir, name), "utf8"));
    for (const field of ["slug", "label", "steps"]) {
      if (definition[field] === undefined) {
        throw new Error(`${name}: missing ${field}`);
      }
    }
    if (!Array.isArray(definition.steps) || definition.steps.length === 0) {
      throw new Error(`${name}: steps must be a non-empty array`);
    }
    definitions.push(definition);
  }
  return definitions;
}

function schedulerBase(raw) {
  const base = String(raw ?? "").trim().replace(/\/+$/, "");
  if (!base.startsWith("http://") && !base.startsWith("https://")) {
    throw new Error("SCHEDULER_URL must be an http(s) base URL");
  }
  return base;
}

async function readToken() {
  const file = String(process.env.SCHEDULER_TOKEN_FILE ?? "").trim();
  if (file !== "") {
    const token = (await readFile(file, "utf8")).trim();
    if (token === "") {
      throw new Error("SCHEDULER_TOKEN_FILE is empty");
    }
    return token;
  }
  const token = String(process.env.SCHEDULER_TOKEN ?? "").trim();
  if (token === "") {
    throw new Error("set SCHEDULER_TOKEN_FILE or SCHEDULER_TOKEN");
  }
  return token;
}

async function rpc(base, token, method, body) {
  const response = await fetch(`${base}/${SERVICE}/${method}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
      "Connect-Protocol-Version": "1",
    },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let payload = {};
  if (text.trim() !== "") {
    try {
      payload = JSON.parse(text);
    } catch {
      throw new Error(`${method}: scheduler returned non-JSON HTTP ${response.status}`);
    }
  }
  if (!response.ok) {
    const code = typeof payload?.code === "string" ? payload.code : `http_${response.status}`;
    throw new Error(`${method}: ${code}`);
  }
  return payload;
}

function createBody(definition) {
  const body = {
    label: definition.label,
    enabled: definition.enabled !== false,
    triggers: definition.triggers ?? [{ manual: {} }],
    conditions: definition.conditions ?? [],
    steps: definition.steps,
    continueOnError: definition.continueOnError === true,
    idempotencyKey: seedKey(definition),
  };
  if (definition.description !== undefined) {
    body.description = definition.description;
  }
  return body;
}

export async function run({ dryRun = false, base, token, definitions }) {
  const listed = await rpc(base, token, "ListScenarios", { includeDisabled: true });
  const existing = Array.isArray(listed?.scenarios) ? listed.scenarios : [];
  const planned = planSeed(definitions, existing);

  for (const definition of definitions) {
    if (!planned.includes(definition)) {
      console.log(`keep     ${definition.label} (seed-${definition.slug})`);
    }
  }
  if (planned.length === 0) {
    console.log("nothing to create");
    return 0;
  }
  for (const definition of planned) {
    if (dryRun) {
      console.log(`would    ${definition.label} (seed-${definition.slug})`);
      continue;
    }
    const created = await rpc(base, token, "CreateScenario", createBody(definition));
    const scenario = created?.scenario;
    const id = typeof scenario?.id === "string" ? scenario.id : "<unknown id>";
    console.log(`created  ${definition.label} (${id})`);
  }
  return planned.length;
}

function usage() {
  return "usage: [--dry-run]  (env: SCHEDULER_URL, SCHEDULER_TOKEN_FILE|SCHEDULER_TOKEN)";
}

async function main(argv) {
  const args = new Set(argv.slice(2));
  for (const arg of args) {
    if (arg !== "--dry-run" && arg !== "-h" && arg !== "--help") {
      console.error(`unknown argument: ${arg}`);
      console.error(usage());
      return 2;
    }
  }
  if (args.has("-h") || args.has("--help")) {
    console.log(usage());
    return 0;
  }
  const dryRun = args.has("--dry-run");
  const base = schedulerBase(process.env.SCHEDULER_URL);
  const token = await readToken();
  const definitions = await loadDefinitions();
  if (dryRun) {
    console.log("dry-run: no scenario is created");
  }
  await run({ dryRun, base, token, definitions });
  return 0;
}

function isEntrypoint() {
  const invoked = process.argv[1];
  return invoked !== undefined && import.meta.url === pathToFileURL(invoked).href;
}

if (isEntrypoint()) {
  main(process.argv).catch((error) => {
    console.error(`seed failed: ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  });
}
