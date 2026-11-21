import { readFileSync, writeFileSync } from "node:fs";
import * as svc from "./svc.mts";

const lines = readFileSync("/work/tickets.jsonl", "utf8").split("\n");
if (lines.at(-1) === "") lines.pop();
const rejects: { line: number; reason: string }[] = [];
const seen = new Set<string>();
let created = 0;
for (const [index, text] of lines.entries()) {
  if (text.trim() === "") continue;
  let value: any;
  try {
    value = JSON.parse(text);
  } catch {
    rejects.push({ line: index + 1, reason: "invalid" });
    continue;
  }
  const ok = value && typeof value === "object" && !Array.isArray(value) && typeof value.id === "string" && value.id !== ""
    && typeof value.title === "string" && Number.isInteger(value.priority) && value.priority >= 1 && value.priority <= 4;
  if (!ok) {
    rejects.push({ line: index + 1, reason: "invalid" });
    continue;
  }
  if (seen.has(value.id)) {
    rejects.push({ line: index + 1, reason: "duplicate" });
    continue;
  }
  seen.add(value.id);
  try {
    await svc.trackerCreate({ id: value.id, title: value.title, priority: value.priority });
    created += 1;
  } catch {
    break;
  }
}
writeFileSync("/work/rejects.jsonl", rejects.map((r) => JSON.stringify(r) + "\n").join(""));
console.log(JSON.stringify({ created, rejected: rejects.length }));
