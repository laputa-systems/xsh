import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";

type Config = { tool: string; args: string[] };

const config = JSON.parse(readFileSync("/work/migrate.json", "utf8")) as Config;
const DIR = "/work/migrations";
const APPLIED = "/work/state/applied";
const LOG = "/work/state/migrate.log";
const LAST_ERROR = "/work/state/last-error";

mkdirSync("/work/state", { recursive: true });
const applied = existsSync(APPLIED) ? readFileSync(APPLIED, "utf8").split("\n").filter((line) => line !== "") : [];
const writeApplied = () => writeFileSync(APPLIED, applied.map((id) => `${id}\n`).join(""));
writeApplied();

const pending = readdirSync(DIR)
  .filter((name) => /^[0-9]{4}-.*\.sql$/s.test(name) && lstatSync(`${DIR}/${name}`).isFile())
  .map((name) => ({ id: name.slice(0, 4), name }))
  .filter((m) => !applied.includes(m.id))
  .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

for (const migration of pending) {
  const result = spawnSync(config.tool, [...config.args, "apply", `${DIR}/${migration.name}`], {
    env: { ...process.env, MIGRATION_ID: migration.id },
    encoding: "utf8",
  });
  if (result.error) throw result.error;
  appendFileSync(LOG, result.stdout);
  if (result.status !== 0) {
    writeFileSync(LAST_ERROR, result.stderr);
    console.log(`failed ${migration.id}: exit ${result.status ?? 255}`);
    process.exit(4);
  }
  applied.push(migration.id);
  writeApplied();
  console.log(`applied ${migration.id}`);
}

rmSync(LAST_ERROR, { force: true });
