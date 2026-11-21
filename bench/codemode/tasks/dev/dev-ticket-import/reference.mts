import { readFileSync, writeFileSync } from "node:fs";
import * as svc from "./svc.mts";

type Line = { id: string; title: string; priority: number };

function parse(text: string): Line | null {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const { id, title, priority } = value as Record<string, unknown>;
  if (typeof id !== "string" || id === "" || typeof title !== "string") return null;
  if (typeof priority !== "number" || !Number.isInteger(priority) || priority < 1 || priority > 4) return null;
  return { id, title, priority };
}

const lines = readFileSync("/work/tickets.jsonl", "utf8").split("\n");
if (lines.at(-1) === "") lines.pop();
const rejects: { line: number; reason: string }[] = [];
const seen = new Set<string>();
let created = 0;

for (const [index, text] of lines.entries()) {
  if (text.trim() === "") continue;
  const line = parse(text);
  if (line === null) {
    rejects.push({ line: index + 1, reason: "invalid" });
    continue;
  }
  if (seen.has(line.id)) {
    rejects.push({ line: index + 1, reason: "duplicate" });
    continue;
  }
  seen.add(line.id);
  try {
    await svc.trackerCreate(line);
    created += 1;
  } catch (error) {
    if (!(error instanceof svc.ServiceError)) throw error;
    rejects.push({ line: index + 1, reason: "failed" });
  }
}

writeFileSync("/work/rejects.jsonl", rejects.map((r) => JSON.stringify(r) + "\n").join(""));
console.log(JSON.stringify({ created, rejected: rejects.length }));
