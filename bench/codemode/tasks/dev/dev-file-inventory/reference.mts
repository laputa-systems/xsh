import { lstatSync, readdirSync, readlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";

type Row = { path: string; kind: "file"; size: number } | { path: string; kind: "symlink"; target: string };

const root = "/work/data";
const rows: Row[] = [];

function visit(relative: string): void {
  for (const entry of readdirSync(join(root, relative), { withFileTypes: true })) {
    const path = relative === "" ? entry.name : `${relative}/${entry.name}`;
    if (entry.isSymbolicLink()) {
      rows.push({ path, kind: "symlink", target: readlinkSync(join(root, path), "utf8") });
    } else if (entry.isDirectory()) {
      visit(path);
    } else if (entry.isFile()) {
      rows.push({ path, kind: "file", size: lstatSync(join(root, path)).size });
    }
  }
}

visit("");
rows.sort((a, b) => Buffer.compare(Buffer.from(a.path), Buffer.from(b.path)));
for (const row of rows) console.log(JSON.stringify(row));

const files = rows.filter((r) => r.kind === "file");
const bytes = files.reduce((sum, r) => sum + (r.kind === "file" ? r.size : 0), 0);
writeFileSync("/work/summary.json", JSON.stringify({ files: files.length, symlinks: rows.length - files.length, bytes }));
