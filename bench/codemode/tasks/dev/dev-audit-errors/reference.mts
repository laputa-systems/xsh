import * as svc from "./svc.mts";

const errors: string[] = [];
let cursor: string | null = null;
let pages = 0;

for (let read = 0; read < 20; read++) {
  const page = await svc.auditPage({ cursor });
  pages += 1;
  for (const event of page.events) {
    if (event.kind === "error" && !errors.includes(event.id)) errors.push(event.id);
  }
  if (page.next === null) break;
  cursor = page.next;
}

console.log(JSON.stringify({ errors, pages }));
