import { readFileSync, writeFileSync } from "node:fs";

const BEGIN = "# BEGIN fleet";
const END = "# END fleet";
const HOSTS = "/work/etc/hosts";

type Entry = { ip: string; names: string[] };

function refuse(message: string): never {
  console.error(message);
  process.exit(4);
}

function isEntry(value: unknown): value is Entry {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const e = value as Record<string, unknown>;
  return typeof e.ip === "string" && Array.isArray(e.names) && e.names.every((n) => typeof n === "string");
}

let fleet: unknown;
try {
  fleet = JSON.parse(readFileSync("/work/fleet.json", "utf8"));
} catch (error) {
  refuse(`/work/fleet.json: ${String(error)}`);
}
if (!Array.isArray(fleet) || !fleet.every(isEntry)) refuse("/work/fleet.json is not a list of entries");

const block = fleet.filter((e) => e.names.length > 0).map((e) => `${e.ip}\t${e.names.join(" ")}\n`).join("");
const text = readFileSync(HOSTS, "utf8");
const parts = text.split("\n");
const begins = parts.flatMap((line, i) => (line === BEGIN ? [i] : []));
const ends = parts.flatMap((line, i) => (line === END ? [i] : []));

let updated: string;
if (begins.length === 0 && ends.length === 0) {
  const separator = text !== "" && !text.endsWith("\n") ? "\n" : "";
  updated = `${text}${separator}${BEGIN}\n${block}${END}\n`;
} else if (begins.length === 1 && ends.length === 1 && begins[0] < ends[0]) {
  updated = parts.slice(0, begins[0] + 1).join("\n") + "\n" + block + parts.slice(ends[0]).join("\n");
} else {
  refuse(`${HOSTS}: the fleet markers are malformed; nothing was changed`);
}

if (updated === text) {
  console.log("unchanged");
} else {
  writeFileSync(HOSTS, updated);
  console.log("updated");
}
