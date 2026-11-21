import { readdirSync, readlinkSync, statSync, lstatSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = "/work/data";
const rows: any[] = [];
const seen = new Set<string>();
function visit(relative: string, depth: number): void {
  if (depth > 3) return;
  for (const name of readdirSync(join(root, relative))) {
    const path = relative === "" ? name : `${relative}/${name}`;
    const full = join(root, path);
    let info;
    try {
      info = statSync(full);
    } catch {
      rows.push({ path, kind: "symlink", target: readlinkSync(full, "utf8") });
      continue;
    }
    if (info.isDirectory()) visit(path, depth + 1);
    else if (lstatSync(full).isSymbolicLink()) rows.push({ path, kind: "symlink", target: readlinkSync(full, "utf8") });
    else rows.push({ path, kind: "file", size: info.size });
  }
}
visit("", 0);
rows.sort((a, b) => Buffer.compare(Buffer.from(a.path), Buffer.from(b.path)));
for (const row of rows) console.log(JSON.stringify(row));
const files = rows.filter((r) => r.kind === "file");
writeFileSync("/work/summary.json", JSON.stringify({ files: files.length, symlinks: rows.length - files.length, bytes: files.reduce((s, r) => s + r.size, 0) }));
