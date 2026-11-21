import { readFileSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import * as svc from "./svc.mts";

type Person = { person: string; pages: number; critical: number };
const TRANSIENT = new Set(["Timeout", "Unavailable", "RateLimited"]);

async function withRetry<T>(read: () => Promise<T>): Promise<T> {
  for (let attempt = 1; ; attempt++) {
    try {
      return await read();
    } catch (error) {
      const transient = error instanceof svc.ServiceError && TRANSIENT.has(error.kind);
      if (!transient || attempt === 4) throw error;
      await sleep(200);
    }
  }
}

function kindOf(error: unknown): string {
  return error instanceof svc.ServiceError ? error.kind : "Failed";
}

const teams: string[] = [];
for (const line of readFileSync("/work/teams.txt", "utf8").split("\n")) {
  const name = line.replace(/^[ \t]+|[ \t]+$/g, "");
  if (name !== "" && !name.startsWith("#") && !teams.includes(name)) teams.push(name);
}

const reports: { team: string; people: Person[]; unassigned: number }[] = [];
const unknown: string[] = [];
const failed: string[] = [];

for (const team of teams) {
  let shifts: svc.Shift[];
  let events: svc.PageEvent[];
  try {
    shifts = await withRetry(() => svc.oncallShifts({ team }));
    events = await withRetry(() => svc.oncallPages({ team }));
  } catch (error) {
    (kindOf(error) === "NotFound" ? unknown : failed).push(team);
    continue;
  }
  const people = new Map<string, Person>();
  for (const s of shifts) people.set(s.person, { person: s.person, pages: 0, critical: 0 });
  const seen = new Set<string>();
  let unassigned = 0;
  for (const event of events) {
    if (seen.has(event.id)) continue;
    seen.add(event.id);
    const owner = shifts.find((s) => s.start <= event.at && event.at < s.end);
    if (owner === undefined) {
      unassigned += 1;
      continue;
    }
    const person = people.get(owner.person)!;
    person.pages += 1;
    if (event.severity === "critical") person.critical += 1;
  }
  const ordered = [...people.values()].sort(
    (a, b) => b.pages - a.pages || Buffer.compare(Buffer.from(a.person), Buffer.from(b.person)),
  );
  reports.push({ team, people: ordered, unassigned });
}

console.log(JSON.stringify({ teams: reports, unknown, failed }));
process.exitCode = failed.length > 0 ? 4 : 0;
