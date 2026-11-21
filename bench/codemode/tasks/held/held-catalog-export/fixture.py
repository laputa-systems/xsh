import csv
import io
import random
from pathlib import Path

from fixtures import Outcome, Setup, Verdict, calls_to, rejected_inputs, tree_effects, write

LIMIT = 20
OLD_EXPORT = "sku,name,price\nOLD-1,Discontinued lamp,14.00\n"
TARGET = "export/catalog.csv"
TRANSIENT = ("Timeout", "Unavailable", "RateLimited")
WAITS_MS = (100, 300)


def product(sku: str, name: str, cents: int, version: int = 1, active: bool = True) -> dict:
    return {"sku": sku, "name": name, "price_cents": cents, "version": version, "active": active}


def filler(seed: int, prefix: str, count: int) -> list[dict]:
    rng = random.Random(seed)
    skus = [f"{prefix}-{n:03d}" for n in range(count)]
    rng.shuffle(skus)
    return [product(s, f"Item {s.lower()}", rng.randrange(1, 250000), rng.randrange(1, 4)) for s in skus]


def paged(items: list[dict]) -> list[list[dict]]:
    pages = [items[i:i + LIMIT] for i in range(0, len(items), LIMIT)]
    if not pages or len(pages[-1]) == LIMIT:
        pages.append([])
    return pages


def public_items() -> list[dict]:
    items = filler(11, "SKU", 44)
    items[3] = product("SKU-007", "Bolt, hex M8", 1999, 1)
    items[25] = product("SKU-007", "Bolt, hex M8 (zinc)", 2150, 2)
    items[9] = product("SKU-030", "Cable \"Cat 6\"", 5, 1)
    items[30] = product("SKU-041", "Retired drill", 8900, 3, active=False)
    items.insert(12, product("A-100", "Adapter", 120000, 1))
    items.insert(40, product("SKU-030", "Cable \"Cat 6\"", 5, 1))
    return items


def flaky_items() -> list[dict]:
    items = filler(23, "P", 56)
    items[1] = product("P-010", "Old name", 400, 5)
    items[33] = product("P-010", "Stale copy", 300, 4)
    items[5] = product("P-020", "Hinge", 1000, 1)
    items[45] = product("P-020", "Hinge", 1000, 2, active=False)
    items[14] = product("P-021", "Retired", 77, 1, active=False)
    items[50] = product("P-021", "Back in stock, again", 79, 2)
    items[18] = product("P-005", "Two\nline label", 1, 1)
    items.extend([product("Z-1", "Zinc strip", 300), product("a-1", "lowercase sku", 10),
                  product("P-100", "Extra", 6600, 1), product("P-101", "Extra 2", 6601, 1)])
    return items


VARIANTS = {
    # failures: offset -> list of failing results before the page answers; "slow" answers after the timeout
    "public": {"items": public_items(), "failures": {20: ["Unavailable"]}, "stop": None},
    "flaky": {"items": flaky_items(), "failures": {0: ["slow"], 40: ["RateLimited", "Unavailable"]}, "stop": None},
    "broken": {"items": filler(31, "B", 70), "failures": {0: ["RateLimited"], 20: ["Unavailable", "slow", "Unavailable"]},
               "stop": 20},
    "denied": {"items": filler(47, "D", 90), "failures": {40: ["PermissionDenied"]}, "stop": 40},
}


def pages(variant: str) -> list[list[dict]]:
    return paged(VARIANTS[variant]["items"])


def page_results(variant: str, offset: int, page: list[dict]) -> list[dict]:
    results = []
    for failure in VARIANTS[variant]["failures"].get(offset, []):
        if failure == "slow":
            results.append({"output": page, "delay_ms": 900})
        else:
            results.append({"error": {"kind": failure, "message": f"catalog page {offset}: {failure}",
                                      "performed": False}, "delay_ms": 20})
    if VARIANTS[variant]["stop"] != offset:
        results.append({"output": page, "delay_ms": 30})
    return results


def expected_rows(variant: str) -> list[list[str]]:
    chosen: dict[str, dict] = {}
    for page in pages(variant):
        for item in page:
            current = chosen.get(item["sku"])
            if current is None or item["version"] > current["version"]:
                chosen[item["sku"]] = item
    rows = []
    for sku in sorted(chosen, key=lambda s: s.encode()):
        item = chosen[sku]
        if item["active"]:
            cents = item["price_cents"]
            rows.append([sku, item["name"], f"{cents // 100}.{cents % 100:02d}"])
    return rows


def expected_calls(variant: str) -> dict[int, int]:
    """Calls per offset that the contract requires."""
    stop = VARIANTS[variant]["stop"]
    wanted = {}
    for index, page in enumerate(pages(variant)):
        offset = index * LIMIT
        failures = VARIANTS[variant]["failures"].get(offset, [])
        if offset == stop:
            terminal = [f for f in failures if f not in TRANSIENT and f != "slow"]
            wanted[offset] = (failures.index(terminal[0]) + 1) if terminal else 3
            break
        wanted[offset] = len(failures) + 1
    return wanted


def setup(variant: str, work: Path) -> Setup:
    write(work, TARGET, OLD_EXPORT)
    responses = []
    for index, page in enumerate(pages(variant)):
        offset = index * LIMIT
        responses.append({"capability": "catalog.list", "input": {"offset": offset, "limit": LIMIT},
                          "results": page_results(variant, offset, page)})
    return Setup(responses=responses)


def judge(variant: str, work: Path, outcome: Outcome) -> Verdict:
    failing = VARIANTS[variant]["stop"] is not None
    status = 4 if failing else 0
    if outcome.exit != status:
        return Verdict.wrong(f"exit status {outcome.exit}, expected {status}")
    if rejected_inputs(outcome):
        return Verdict.wrong(f"the service rejected {len(rejected_inputs(outcome))} calls")
    calls = sorted(calls_to(outcome, "catalog.list"), key=lambda c: c["seq"])
    by_offset: dict[int, list[dict]] = {}
    for call in calls:
        by_offset.setdefault(call["input"]["offset"], []).append(call)
    wanted = expected_calls(variant)
    for offset, count in wanted.items():
        got = len(by_offset.get(offset, []))
        if got != count:
            return Verdict.wrong(f"page {offset} was read {got} times, expected {count}")
    extra = sorted(set(by_offset) - set(wanted))
    if extra:
        return Verdict.wrong(f"pages past the end or past a failure were read: {extra}")
    for offset, attempts in by_offset.items():
        for number, (previous, retry) in enumerate(zip(attempts, attempts[1:])):
            gap = retry["start_ms"] - previous["end_ms"]
            if gap < WAITS_MS[min(number, 1)] - 10:
                return Verdict.wrong(f"page {offset} was read again after {gap} ms")
    slow = [o for o, fs in VARIANTS[variant]["failures"].items() if "slow" in fs and o in wanted]
    for offset in slow:
        if not any(c.get("outcome") == "abandoned" for c in by_offset[offset]):
            return Verdict.wrong(f"the slow read of page {offset} was not abandoned at the 400 ms timeout")
    effects = tree_effects(outcome.before, outcome.after)
    if failing:
        if effects["created"] or effects["deleted"] or effects["modified"] or effects["rewritten_same_content"]:
            return Verdict.wrong(f"a failed export changed /work: {effects}")
        return Verdict.ok(calls=len(calls))
    if effects["created"] or effects["deleted"] or effects["modified"] != [TARGET]:
        return Verdict.wrong(f"unexpected file changes: {effects}")
    if outcome.before[TARGET]["inode"] == outcome.after[TARGET]["inode"]:
        return Verdict.wrong("the export was rewritten in place instead of replaced in one step")
    data = (work / TARGET).read_bytes()
    if not data.endswith(b"\n"):
        return Verdict.wrong("the export does not end with a newline")
    try:
        parsed = list(csv.reader(io.StringIO(data.decode(), newline="")))
    except (UnicodeDecodeError, csv.Error) as error:
        return Verdict.wrong(f"the export is not CSV: {error}")
    rows = expected_rows(variant)
    if not parsed or parsed[0] != ["sku", "name", "price"]:
        return Verdict.wrong("the header line is wrong")
    if parsed[1:] != rows:
        return Verdict.wrong("the exported rows differ from the expected catalog")
    if outcome.stdout.strip() != f"exported {len(rows)} products":
        return Verdict.wrong("the summary line is wrong")
    return Verdict.ok(calls=len(calls), rows=len(rows))
