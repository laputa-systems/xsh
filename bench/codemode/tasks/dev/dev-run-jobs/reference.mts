import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

type Job = { name: string; argv: string[]; cwd: string; env: Record<string, string> };
type Outcome = { name: string; exit: number | null; stdout: string; stderr: string };

const jobs = JSON.parse(readFileSync("/work/jobs.json", "utf8")) as Job[];
const results: Outcome[] = jobs.map((job) => {
  const [program, ...args] = job.argv;
  const run = spawnSync(program, args, {
    cwd: join("/work", job.cwd),
    env: { ...process.env, ...job.env },
    encoding: "utf8",
  });
  if (run.error !== undefined) return { name: job.name, exit: null, stdout: "", stderr: "" };
  return { name: job.name, exit: run.status, stdout: run.stdout, stderr: run.stderr };
});

writeFileSync("/work/results.json", JSON.stringify(results));
process.exit(results.every((r) => r.exit === 0) ? 0 : 4);
