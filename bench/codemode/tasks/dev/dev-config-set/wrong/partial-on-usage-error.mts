import { readFileSync, renameSync, writeFileSync } from "node:fs";

const file = "/work/app.conf";
let lines = readFileSync(file, "utf8").split("\n").slice(0, -1);
let changed = false;
for (const argument of process.argv.slice(2)) {
  const eq = argument.indexOf("=");
  const dot = argument.indexOf(".");
  if (eq < 0 || dot < 0 || dot > eq) {
    console.error(`bad argument ${argument}`);
    process.exit(2);
  }
  const section = argument.slice(0, dot);
  const key = argument.slice(dot + 1, eq);
  const wanted = `${key} = ${argument.slice(eq + 1)}`;
  let current = "";
  let done = false;
  let last = -1;
  let headerAt = -1;
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    if (t.startsWith("[") && t.endsWith("]")) {
      current = t.slice(1, -1);
      if (current === section && headerAt < 0) headerAt = i;
      continue;
    }
    if (current === section && !t.startsWith("#") && !t.startsWith(";") && lines[i].includes("=")) {
      if (lines[i].slice(0, lines[i].indexOf("=")).trim() === key) {
        if (lines[i] !== wanted) changed = true;
        lines[i] = wanted;
        done = true;
        break;
      }
      last = i;
    }
  }
  if (!done) {
    changed = true;
    if (headerAt < 0) lines.push(`[${section}]`, wanted);
    else lines.splice((last >= 0 ? last : headerAt) + 1, 0, wanted);
  }
  writeFileSync(`${file}.tmp`, lines.join("\n") + "\n");
  renameSync(`${file}.tmp`, file);
}
console.log(changed ? "changed" : "unchanged");
