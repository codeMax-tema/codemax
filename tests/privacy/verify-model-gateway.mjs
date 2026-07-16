import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const productionRoots = ["agent/app", "apps/desktop/src"];
const allowed = new Set([
  "agent/app/providers/openai_compatible.py",
  "agent/app/providers/errors.py",
]);
const rules = [
  [/(?:from\s+openai\s+import|import\s+openai\b)/, "OpenAI SDK import outside provider boundary"],
  [/\bOpenAI\s*\(/, "OpenAI client construction outside provider boundary"],
  [/\.chat\.completions\.create\s*\(/, "direct chat completions call outside provider boundary"],
  [/(?:api\.openai\.com|\/v1\/chat\/completions)/i, "direct model endpoint outside provider boundary"],
  [/(?:requests|httpx)\.(?:post|request)\s*\([^\n]*(?:chat|completion|model)/i, "direct Python model HTTP call"],
  [/(?:fetch|axios\.(?:post|request))\s*\([^\n]*(?:chat|completion|model)/i, "direct frontend model HTTP call"],
];
const providerAdapterBypass = /from\s+app\.providers(?:\.[\w.]+)?\s+import[^\n]*\b(?:build_chat_client|OpenAICompatibleTransport)\b/;

async function filesUnder(relative) {
  const absolute = path.join(root, relative);
  const output = [];
  async function walk(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const target = path.join(directory, entry.name);
      if (entry.isDirectory()) await walk(target);
      else if (/\.(?:py|rs|ts|tsx|js|mjs)$/.test(entry.name)) output.push(target);
    }
  }
  await walk(absolute);
  return output;
}

export function findViolations(relative, content) {
  const normalized = relative.replaceAll("\\", "/");
  if (allowed.has(normalized)) return [];
  const violations = rules
    .filter(([pattern]) => pattern.test(content))
    .map(([, reason]) => `${relative}: ${reason}`);
  if (
    normalized !== "agent/app/model_gateway.py" &&
    !normalized.startsWith("agent/app/providers/") &&
    providerAdapterBypass.test(content)
  ) {
    violations.push(`${relative}: direct provider adapter access outside model gateway`);
  }
  return violations;
}

const fixtureViolations = findViolations(
  "agent/app/bypass_fixture.py",
  "from openai import OpenAI\nOpenAI().chat.completions.create(model='x', messages=[])",
);
if (fixtureViolations.length < 2) {
  console.error("Model gateway bypass self-test failed; release is blocked.");
  process.exit(1);
}
const adapterFixtureViolations = findViolations(
  "agent/app/adapter_bypass_fixture.py",
  "from app.providers import OpenAICompatibleTransport",
);
if (!adapterFixtureViolations.some((violation) => violation.includes("provider adapter"))) {
  console.error("Model provider adapter bypass self-test failed; release is blocked.");
  process.exit(1);
}

const violations = [];
for (const productionRoot of productionRoots) {
  for (const file of await filesUnder(productionRoot)) {
    const relative = path.relative(root, file).replaceAll("\\", "/");
    violations.push(...findViolations(relative, await readFile(file, "utf8")));
  }
}
if (violations.length) {
  console.error("REL-P0-006 model gateway bypass violations detected:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}
console.log("REL-P0-006 model gateway bypass gate passed (including fail-closed fixture).");
