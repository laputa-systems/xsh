from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, duplicate_writes, rejected_inputs, write

PLANS = {
    "public": {
        "warehouse": "east-2",
        "stock": [
            ("bolt-m8", 40, 50, 200, [], "ok"),
            ("nut-m8", 300, 50, 400, None, None),
            ("washer-12", 0, 10, 120, [("po-77", 120)], None),
            ("anchor-6", 9, 10, 25, [], "ok"),
            ("rivet-4", 10, 10, 90, None, None),
        ],
    },
    "failures": {
        "warehouse": "north-1",
        "stock": [
            ("cable-3m", 2, 5, 30, [], "Unavailable"),
            ("plug-eu", 0, 20, 100, [], "ok"),
            ("adapter-c", 1, 4, 12, [], "Conflict"),
            ("fuse-10a", 3, 30, 60, [("po-9", 10), ("po-12", 20)], None),
            ("hub-7", 7, 7, 14, None, None),
            ("dock-2", 0, 1, 1, [], "ok"),
        ],
    },
    "quiet": {
        "warehouse": "west-9",
        "stock": [("tape-25", 80, 20, 100, None, None), ("glue-50", 21, 20, 60, None, None)],
    },
}


def order_id(warehouse: str, sku: str) -> str:
    return f"PO-{warehouse}-{sku}"


def setup(variant: str, work: Path) -> Setup:
    plan = PLANS[variant]
    warehouse = plan["warehouse"]
    write(work, "warehouse.txt", f"  {warehouse}\n")
    rows = [{"sku": s, "on_hand": h, "reorder_at": r, "target": t} for s, h, r, t, _, _ in plan["stock"]]
    responses = [{"capability": "inventory.list", "input": {"warehouse": warehouse}, "results": [{"output": rows}]}]
    for sku, on_hand, reorder_at, target, open_orders, placement in plan["stock"]:
        if open_orders is not None:
            responses.append({
                "capability": "orders.open", "input": {"warehouse": warehouse, "sku": sku},
                "results": [{"output": [{"id": i, "quantity": q} for i, q in open_orders]}],
            })
        if placement is not None:
            quantity = target - on_hand
            outcome = (
                {"output": {"id": order_id(warehouse, sku), "sku": sku, "quantity": quantity}}
                if placement == "ok" else {"error": {"kind": placement, "message": f"supplier said {placement}"}}
            )
            responses.append({
                "capability": "orders.place", "input": {"warehouse": warehouse, "sku": sku, "quantity": quantity},
                "results": [outcome],
            })
    return Setup(responses=responses)


def expected(variant: str) -> dict:
    plan = PLANS[variant]
    placed, skipped, failed = [], [], []
    for sku, on_hand, reorder_at, target, open_orders, placement in plan["stock"]:
        if on_hand >= reorder_at:
            continue
        if open_orders:
            skipped.append(sku)
        elif placement == "ok":
            placed.append({"sku": sku, "order": order_id(plan["warehouse"], sku)})
        else:
            failed.append({"sku": sku, "kind": placement})
    return {
        "placed": sorted(placed, key=lambda p: p["sku"]),
        "skipped": sorted(skipped),
        "failed": sorted(failed, key=lambda f: f["sku"]),
    }


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    plan = PLANS[variant]
    places = calls_to(outcome, "orders.place")
    if duplicate_writes(outcome):
        return Verdict.wrong(f"{len(duplicate_writes(outcome))} placements repeated a placement", placements=len(places))
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls", placements=len(places))
    restock = {s for s, h, r, *_ in plan["stock"] if h < r}
    asked = {c["input"]["sku"] for c in calls_to(outcome, "orders.open")}
    if asked - restock:
        return Verdict.wrong("open orders were requested for a SKU that does not need restocking")
    wanted = {s for s, h, r, t, o, p in plan["stock"] if h < r and not o}
    placed = [c["input"]["sku"] for c in places]
    if sorted(placed) != sorted(wanted):
        return Verdict.wrong(f"placed orders for {sorted(placed)}, expected {sorted(wanted)}", placements=len(places))
    if outcome.exit != 0:
        return Verdict.wrong(f"exit status {outcome.exit}, expected 0")
    report = work / "restock.json"
    if not report.is_file():
        return Verdict.wrong("restock.json was not written")
    import json
    try:
        found = json.loads(report.read_text())
    except json.JSONDecodeError:
        return Verdict.wrong("restock.json is not JSON")
    if found != expected(variant):
        return Verdict.wrong("restock.json differs from the expected report")
    return Verdict.ok(placements=len(places))
