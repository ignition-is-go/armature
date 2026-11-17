// Drives the built demo in headless Chrome and compares what each check observes
// with expected.json. Needs Node 22 and Chrome; no packages.
//
//   trunk build            (in examples/leptos-demo)
//   node checks/run.mjs    (add --record to rewrite expected.json)
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, extname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = join(here, "..", "dist");
const checks = ["basics", "inputs", "gestures", "dialog", "sheet", "agent", "parity", "components"];
// What a check observes that depends on how fast the machine is.
const unstable = ["basics.number.heldFor700ms", "basics.number.field"];
// What a check observes about the whole page, which changes whenever the demo grows.
const incidental = ["basics.labels", "basics.agent", "basics.accordion.after", "agent.labels", "agent.secretRun.ref", "inputs.calendar.cells"];
const record = process.argv.includes("--record");

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css" };
const server = createServer(async (request, response) => {
  const path = request.url === "/" ? "/index.html" : request.url.split("?")[0];
  try {
    const body = await readFile(join(dist, path));
    response.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    response.end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const site = `http://127.0.0.1:${server.address().port}/`;

const profile = await mkdtemp(join(tmpdir(), "armature-checks-"));
const chrome = spawn(process.env.CHROME ?? "google-chrome-stable", [
  "--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`,
  "--no-first-run", "--no-default-browser-check", "--window-size=1400,3000", "about:blank",
]);
const debuggerUrl = await new Promise((resolve, reject) => {
  let log = "";
  chrome.stderr.on("data", (chunk) => {
    log += chunk;
    const found = log.match(/DevTools listening on (ws:\/\/\S+)/);
    if (found) resolve(found[1]);
  });
  chrome.on("exit", () => reject(new Error(`Chrome exited before it was ready:\n${log}`)));
});

const socket = new WebSocket(debuggerUrl);
await new Promise((resolve) => socket.addEventListener("open", resolve));
let nextId = 1;
const waiting = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  waiting.get(message.id)?.(message);
  waiting.delete(message.id);
});
const send = (method, params = {}, sessionId) =>
  new Promise((resolve, reject) => {
    const id = nextId++;
    waiting.set(id, (message) => (message.error ? reject(new Error(message.error.message)) : resolve(message.result)));
    socket.send(JSON.stringify({ id, method, params, sessionId }));
  });

const { targetId } = await send("Target.createTarget", { url: "about:blank" });
const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
// A headless page is not focused, and an unfocused page fires no focus events.
await send("Emulation.setFocusEmulationEnabled", { enabled: true }, sessionId);
const evaluate = async (expression) => {
  const { result, exceptionDetails } = await send(
    "Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }, sessionId);
  if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
  return result.value;
};

/// Every place where `actual` lacks or differs from what `expected` names.
function differences(expected, actual, path = "") {
  if (expected !== null && typeof expected === "object") {
    if (actual === null || typeof actual !== "object") return [`${path}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`];
    return Object.keys(expected).flatMap((key) => differences(expected[key], actual[key], path ? `${path}.${key}` : key));
  }
  return expected === actual ? [] : [`${path}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`];
}

function without(value, paths) {
  const copy = structuredClone(value);
  for (const path of paths) {
    const keys = path.split(".");
    let at = copy;
    for (const key of keys.slice(0, -1)) at = at?.[key];
    if (at) delete at[keys.at(-1)];
  }
  return copy;
}

const expectedPath = join(here, "expected.json");
const expected = record ? {} : JSON.parse(await readFile(expectedPath, "utf8"));
const observed = {};
let failed = 0;
try {
  for (const name of checks) {
    // Each check starts from a freshly loaded page.
    await send("Page.navigate", { url: site }, sessionId);
    await evaluate(`new Promise((resolve) => { const poll = () => window.armature ? resolve() : setTimeout(poll, 50); poll(); })`);
    await evaluate(`new Promise((resolve) => setTimeout(resolve, 300))`);
    const source = await readFile(join(here, `${name}.js`), "utf8");
    observed[name] = await evaluate(`(${source})()`);
    if (record) continue;
    const problems = differences(expected[name], observed[name], name);
    failed += problems.length;
    console.log(problems.length ? `FAIL ${name}\n  ${problems.join("\n  ")}` : `ok   ${name}`);
  }
  if (record) {
    await writeFile(expectedPath, JSON.stringify(without(observed, [...unstable, ...incidental]), null, 2) + "\n");
    console.log(`recorded ${expectedPath}`);
  }
} finally {
  socket.close();
  chrome.kill();
  server.close();
  await rm(profile, { recursive: true, force: true }).catch(() => {});
}
process.exit(failed ? 1 : 0);
