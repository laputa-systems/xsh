import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

type Job = { name: string; argv: string[]; cwd: string; env: Record<string, string> };
const jobs = JSON.parse(readFileSync("/work/jobs.json", "utf8")) as Job[];
const results = jobs.map((job) => {
  const run = spawnSync(job.argv.join(" "), { shell: true, cwd: join("/work", job.cwd), env: { ...process.env, ...job.env }, encoding: "utf8" });
  if (run.status === 127) return { name: job.name, exit: null, stdout: "", stderr: "" };
  return { name: job.name, exit: run.status, stdout: run.stdout, stderr: run.stderr };
});
writeFileSync("/work/results.json", JSON.stringify(results));
process.exit(results.every((r) => r.exit === 0) ? 0 : 4);
