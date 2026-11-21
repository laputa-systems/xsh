import { readFileSync, writeFileSync } from "node:fs";
import * as svc from "./svc.mts";

type Payout = { ref: string; account: string; cents: number };
type Settled = { sent: string } | { failed: string } | { unknown: true };

const TRANSIENT = new Set(["Timeout", "Unavailable", "RateLimited"]);

function kindOf(error: unknown): string {
  return error instanceof svc.ServiceError ? error.kind : "Failed";
}

async function settle(payout: Payout): Promise<Settled> {
  for (let sends = 1; ; sends++) {
    let kind: string;
    try {
      const receipt = await svc.payoutsSend(payout, { timeoutMs: 500 });
      return { sent: receipt.transfer };
    } catch (error) {
      kind = kindOf(error);
    }
    if (!TRANSIENT.has(kind)) return { failed: kind };
    let state: svc.PayoutState;
    try {
      state = await svc.payoutsStatus({ ref: payout.ref });
    } catch {
      return { unknown: true };
    }
    if (state.transfer !== null) return { sent: state.transfer };
    if (sends === 3) return { failed: kind };
  }
}

const payouts = JSON.parse(readFileSync("/work/payouts.json", "utf8")) as Payout[];
const sent: { ref: string; transfer: string }[] = [];
const failed: { ref: string; kind: string }[] = [];
const unknown: string[] = [];

for (const payout of payouts) {
  const settled = await settle({ ref: payout.ref, account: payout.account, cents: payout.cents });
  if ("sent" in settled) sent.push({ ref: payout.ref, transfer: settled.sent });
  else if ("failed" in settled) failed.push({ ref: payout.ref, kind: settled.failed });
  else unknown.push(payout.ref);
}

writeFileSync("/work/payout-report.json", JSON.stringify({ sent, failed, unknown }) + "\n");
console.log(`sent=${sent.length} failed=${failed.length} unknown=${unknown.length}`);
process.exitCode = failed.length > 0 || unknown.length > 0 ? 4 : 0;
