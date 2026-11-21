import { readFileSync } from "node:fs";
import { execFile } from "node:child_process";

type Row = { file: string; sha256: string } | { file: string; error: string };

function slowsum(file: string): Promise<Row> {
  return new Promise((resolve) => {
    execFile("/work/bin/slowsum", [file], { encoding: "utf8" }, (error, stdout, stderr) => {
      if (error === null) resolve({ file, sha256: stdout.trim() });
      else resolve({ file, error: (stderr.split("\n")[0] ?? "").trim() });
    });
  });
}

const files = readFileSync("/work/files.txt", "utf8").split("\n").map((l) => l.trim()).filter((l) => l !== "");
const rows: Row[] = new Array(files.length);
let next = 0;
async function worker(): Promise<void> {
  while (next < files.length) {
    const index = next++;
    rows[index] = await slowsum(files[index]);
  }
}
await Promise.all([worker(), worker(), worker()]);
for (const row of rows) console.log(JSON.stringify(row));
process.exit(rows.every((r) => "sha256" in r) ? 0 : 4);
