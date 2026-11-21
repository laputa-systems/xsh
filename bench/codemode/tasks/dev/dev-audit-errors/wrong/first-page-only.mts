import * as svc from "./svc.mts";

const page = await svc.auditPage({ cursor: null });
const errors = page.events.filter((e) => e.kind === "error").map((e) => e.id);
console.log(JSON.stringify({ errors, pages: 1 }));
