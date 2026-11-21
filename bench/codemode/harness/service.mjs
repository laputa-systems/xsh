import { createServer } from "node:http";
import { appendFileSync, readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";

const [specPath, logPath, portText = "8080"] = process.argv.slice(2);
const spec = JSON.parse(readFileSync(specPath, "utf8"));
const capabilities = new Map(spec.capabilities.map((c) => [c.id, c]));
const entries = spec.responses.map((response) => ({ ...response, served: 0 }));
const STATUS = {
  Timeout: 504, Unavailable: 503, RateLimited: 429, NotFound: 404, PermissionDenied: 403,
  Invalid: 422, Conflict: 409, Failed: 500,
};
const started = Date.now();
let sequence = 0;
let inFlight = 0;

const now = () => Date.now() - started;
const log = (record) => appendFileSync(logPath, JSON.stringify(record) + "\n");
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function reply(response, status, body) {
  const text = JSON.stringify(body);
  response.writeHead(status, { "content-type": "application/json", "content-length": Buffer.byteLength(text) });
  response.end(text);
}

async function handle(request, response, body) {
  const seq = ++sequence;
  const start = now();
  inFlight += 1;
  const record = { seq, start_ms: start, in_flight: inFlight };
  let counted = true;
  const release = () => {
    if (counted) {
      counted = false;
      inFlight -= 1;
    }
  };
  const abandoned = new Promise((resolve) => {
    response.on("close", () => {
      if (!response.writableFinished) {
        record.abandoned_ms = now();
        release();
        resolve();
      }
    });
  });
  try {
    const match = /^\/call\/([a-z][a-z0-9_]*\.[a-z][a-z0-9_]*)$/.exec(request.url ?? "");
    if (request.method !== "POST" || match === null) {
      Object.assign(record, { capability: null, input: null, outcome: "protocol-error" });
      return reply(response, 404, { kind: "NotFound", message: `no operation at ${request.method} ${request.url}` });
    }
    const id = match[1];
    record.capability = id;
    let input;
    try {
      input = JSON.parse(body);
    } catch {
      Object.assign(record, { input: body, outcome: "protocol-error" });
      return reply(response, 400, { kind: "Invalid", message: "the request body is not JSON" });
    }
    record.input = input;
    const capability = capabilities.get(id);
    if (capability === undefined) {
      record.outcome = "unknown-operation";
      return reply(response, 404, { kind: "NotFound", message: `unknown operation ${id}` });
    }
    const entry = entries.find((e) => e.capability === id && isDeepStrictEqual(e.input, input));
    if (entry === undefined) {
      record.outcome = "rejected-input";
      return reply(response, 422, { kind: "Invalid", message: `${id} does not accept the input ${JSON.stringify(input)}` });
    }
    if (capability.effect === "write" && entry.served >= entry.results.length) {
      entry.served += 1;
      record.outcome = "duplicate-write";
      return reply(response, 409, { kind: "Conflict", message: `${id} was already performed for this input` });
    }
    const index = Math.min(entry.served, entry.results.length - 1);
    entry.served += 1;
    const result = entry.results[index];
    record.result_index = index;
    if (result.delay_ms) await Promise.race([sleep(result.delay_ms), abandoned]);
    if (record.abandoned_ms !== undefined) {
      record.outcome = "abandoned";
      record.performed = capability.effect === "write";
      return;
    }
    if ("error" in result) {
      record.outcome = result.error.kind;
      record.performed = Boolean(result.error.performed);
      return reply(response, STATUS[result.error.kind] ?? 500, { kind: result.error.kind, message: result.error.message });
    }
    record.outcome = "ok";
    record.performed = capability.effect === "write";
    return reply(response, 200, { output: result.output });
  } finally {
    release();
    record.end_ms = now();
    log(record);
  }
}

const server = createServer((request, response) => {
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", () => {
    handle(request, response, Buffer.concat(chunks).toString("utf8")).catch((error) => {
      reply(response, 500, { kind: "Failed", message: String(error) });
    });
  });
});
server.keepAliveTimeout = 1000;
server.listen(Number(portText), "127.0.0.1", () => log({ ready: true, port: Number(portText), start_ms: now() }));
for (const signal of ["SIGTERM", "SIGINT"]) process.on(signal, () => server.close(() => process.exit(0)));
