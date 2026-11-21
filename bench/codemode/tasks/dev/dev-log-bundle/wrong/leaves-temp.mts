import { copyFileSync, existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const names = readFileSync("/work/bundle.txt", "utf8").split("\n").map((l) => l.trim()).filter((l) => l !== "");
for (const name of names) {
  if (!existsSync(`/work/logs/${name}`)) {
    console.error(`missing log file: ${name}`);
    process.exit(4);
  }
}
const temporary = "/work/out/logs.tar.gz.partial";
spawnSync("tar", ["-czf", temporary, "-C", "/work", ...names.map((n) => `logs/${n}`)]);
copyFileSync(temporary, "/work/out/logs.tar.gz");
console.log(names.length);
