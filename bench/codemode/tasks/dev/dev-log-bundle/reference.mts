import { existsSync, readFileSync, renameSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";

const names = readFileSync("/work/bundle.txt", "utf8").split("\n").map((l) => l.trim()).filter((l) => l !== "");
for (const name of names) {
  if (!existsSync(`/work/logs/${name}`)) {
    console.error(`missing log file: ${name}`);
    process.exit(4);
  }
}

const temporary = `/work/out/.logs.tar.gz.${process.pid}.tmp`;
const tar = spawnSync("tar", ["-czf", temporary, "-C", "/work", ...names.map((n) => `logs/${n}`)], { encoding: "utf8" });
if (tar.error !== undefined || tar.status !== 0) {
  rmSync(temporary, { force: true });
  console.error(`tar failed: ${tar.stderr ?? tar.error?.message}`);
  process.exit(4);
}
renameSync(temporary, "/work/out/logs.tar.gz");
console.log(names.length);
