#!/usr/bin/env python3
import argparse
import csv
from dataclasses import dataclass
import io
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

from compare_tree import entries


ROOT = Path(__file__).resolve().parents[3]
CHECKPOINT = "15243e97f6fd751016ab28690ada2cadb6fc973a"
ELIGIBILITY_PATH = "dev/consolidation/typing-corpus/eligibility.tsv"
RETAINED_PATH = "dev/consolidation/typing-corpus/retained.tsv"
INVENTORY_PATH = "dev/consolidation/evidence/typing-cohort-inventory.tsv"
INDIVIDUAL_EFFECTS_PATH = "dev/consolidation/evidence/typing-corpus-host-obligations/individual-effects-sites.tsv"
EFFECT_TARGETS_PATH = ROOT / "dev/performance/corpus/effect-facts-targets.tsv"
EFFECT_FACTS_PATH = ROOT / "dev/performance/corpus/effect-facts-r6.tsv"
VIEW_DIGEST_SCRIPT = ROOT / "dev/consolidation/typing-corpus/view_digest.py"
EXPECTED_VIEW_DIGESTS = {
    "original": "e3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899",
    "reduced-1040": "b67b498467a1dfe613573690288a8c514b9c03d0f895a4a3b20e4fd3b55968f9",
}
EXPECTED_VIEW_ENTRIES = 14287
EXPECTED_COUNTS = {
    "proc-parameter-contract": 549,
    "proc-return-contract": 209,
    "effect-joint-proof-pending": 202,
    "empty-map-record-selection": 149,
    "map-literal-selection": 1,
    "replace-overload-control": 2,
    "interpolation-contract-pending": 1,
    "source-context-pending": 17,
    "historical-diagnostic-only-pending": 713,
}
SOURCE_CONTEXT_CLASSES = {
    2483: "source-read-schema-shape-pending",
    2332: "null-concrete-join-pending",
    2543: "null-concrete-join-pending",
    3406: "null-concrete-join-pending",
    3407: "null-concrete-join-pending",
    3476: "null-concrete-join-pending",
    6436: "null-concrete-join-pending",
    6564: "null-concrete-join-pending",
    7427: "null-concrete-join-pending",
    7822: "null-concrete-join-pending",
    7823: "null-concrete-join-pending",
    2285: "producer-or-source-order-pending",
    6338: "producer-or-source-order-pending",
    973: "nominal-variant-return-context-pending",
    3914: "nominal-variant-return-context-pending",
    9111: "membership-grammar-boundary-pending",
    9112: "membership-grammar-boundary-pending",
}
PROC_PARAMETER_REASON = "current grammar requires nondefault proc/stream parameter type"
PROC_RETURN_REASON = "omitted proc return defaults to Unit; preserve explicit value-return contract"
EFFECT_REASON = "retain explicit private effect set; individual lint proof does not establish joint equivalence through declared-call cycles"


@dataclass(frozen=True)
class Span:
    start: int
    end: int

    @property
    def length(self):
        return self.end - self.start


@dataclass(frozen=True)
class Site:
    site: int
    project: str
    path: str
    line: int
    kind: str
    category: str
    owner: str
    reason: str
    span: Span
    owner_span: Span
    edit_span: Span


def frozen_table(path):
    try:
        return subprocess.check_output(
            ["git", "-C", str(ROOT), "show", f"{CHECKPOINT}:{path}"],
            text=True,
        )
    except subprocess.CalledProcessError as error:
        raise ValueError(f"cannot read frozen checkpoint table {path}") from error


def read_rows(text, required):
    reader = csv.DictReader(io.StringIO(text), delimiter="\t")
    if reader.fieldnames is None or not required.issubset(reader.fieldnames):
        raise ValueError(f"table is missing columns: {sorted(required)}")
    return list(reader)


def parse_span(value, field):
    if value == "-":
        raise ValueError(f"missing {field} byte span")
    pieces = value.split("..")
    if len(pieces) != 2 or not all(piece.isdecimal() for piece in pieces):
        raise ValueError(f"invalid {field} byte span: {value}")
    start, end = map(int, pieces)
    if end <= start:
        raise ValueError(f"empty or reversed {field} byte span: {value}")
    return Span(start, end)


def parse_inventory(text):
    inventory = []
    for row in csv.reader(io.StringIO(text), delimiter="\t"):
        if len(row) < 12 or row[0] != "site" or not row[1].isdecimal():
            continue
        source_parts = PurePosixPath(row[2]).parts
        if "typing-cohort-xsh" in source_parts:
            project = "xsh"
            marker = "typing-cohort-xsh"
        elif "typing-cohort-laputa" in source_parts:
            project = "laputa"
            marker = "typing-cohort-laputa"
        else:
            raise ValueError(f"inventory source path has no project root: {row[2]}")
        marker_index = source_parts.index(marker)
        relative = PurePosixPath(*source_parts[marker_index + 1:])
        if relative.is_absolute() or not relative.parts or any(part in {"", ".", ".."} for part in relative.parts):
            raise ValueError(f"invalid project-relative source path: {relative}")
        edit_span = None if row[7] == "-" else parse_span(row[7], "edit")
        inventory.append(
            (
                project,
                relative.as_posix(),
                row[3],
                row[4],
                parse_span(row[5], "annotation"),
                parse_span(row[6], "owner"),
                edit_span,
                int(row[8]),
                row[10],
            )
        )
    if len(inventory) != 9194:
        raise ValueError(f"frozen inventory has {len(inventory)} sites; expected 9194")
    return inventory


def load_effect_evidence(inventory_sites):
    individual_rows = read_rows(
        frozen_table(INDIVIDUAL_EFFECTS_PATH),
        {"site", "project", "path", "owner"},
    )
    individual_by_id = {int(row["site"]): row for row in individual_rows}
    individual_ids = set(individual_by_id)
    if len(individual_ids) != len(individual_rows):
        raise ValueError("historical individual effect table contains duplicate sites")
    if len(individual_ids) != 142:
        raise ValueError(f"historical individual effect table has {len(individual_ids)} sites; expected 142")
    inventory_by_id = {site.site: site for site in inventory_sites}
    for site_id, row in individual_by_id.items():
        site = inventory_by_id.get(site_id)
        if site is None or (row["project"], row["path"], row["owner"]) != (
            site.project,
            site.path,
            site.owner,
        ):
            raise ValueError(f"historical individual effect identity differs at site {site_id}")

    targets = []
    for line_number, line in enumerate(EFFECT_TARGETS_PATH.read_text(encoding="utf-8").splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 3:
            raise ValueError(f"{EFFECT_TARGETS_PATH}:{line_number}: expected three columns")
        targets.append(tuple(fields))
    target_keys = set(targets)
    if len(targets) != 60 or len(target_keys) != 60:
        raise ValueError(f"effect target table has {len(targets)} unique rows; expected 60")

    fact_rows = read_rows(
        EFFECT_FACTS_PATH.read_text(encoding="utf-8"),
        {"project", "path", "function", "declared", "effective", "required", "source_call_sites"},
    )
    fact_keys = {
        (row["project"], row["path"], row["function"])
        for row in fact_rows
    }
    if len(fact_rows) != 60 or fact_keys != target_keys:
        raise ValueError("R6 effect facts do not match the frozen 60 target declarations")
    facts = {
        (row["project"], row["path"], row["function"]): row
        for row in fact_rows
    }

    effect_ids = {
        site.site
        for site in inventory_sites
        if site.kind == "effect-list" and site.reason == EFFECT_REASON
    }
    if len(effect_ids) != 202 or not individual_ids.issubset(effect_ids):
        raise ValueError("historical effect rows do not match the frozen 202 clauses")
    fact_ids = set()
    for site in inventory_sites:
        if site.site not in effect_ids:
            continue
        tokens = site.owner.split()
        if len(tokens) != 3 or tokens[0] != "proc" or tokens[2] != "effects":
            raise ValueError(f"unexpected effect owner at site {site.site}: {site.owner}")
        key = (site.project, site.path, tokens[1])
        if key in target_keys:
            fact_ids.add(site.site)
        elif site.site not in individual_ids:
            raise ValueError(f"effect site {site.site} has no frozen source evidence")
    if len(fact_ids) != 60 or fact_ids & individual_ids or fact_ids | individual_ids != effect_ids:
        raise ValueError("the historical 142 and R6 60 evidence sets do not partition all 202 clauses")
    return individual_ids, facts


def load_corpus():
    eligibility_rows = read_rows(
        frozen_table(ELIGIBILITY_PATH),
        {"site", "project", "path", "line", "kind", "category", "owner"},
    )
    retained_rows = read_rows(
        frozen_table(RETAINED_PATH),
        {"site", "reason"},
    )
    if len(eligibility_rows) != 9194 or len(retained_rows) != 1843:
        raise ValueError("frozen eligibility or retained-site count changed")
    eligibility = {int(row["site"]): row for row in eligibility_rows}
    retained = {int(row["site"]): row for row in retained_rows}
    if len(eligibility) != len(eligibility_rows) or len(retained) != len(retained_rows):
        raise ValueError("frozen site tables contain duplicate site identities")
    if set(eligibility) != set(range(9194)):
        raise ValueError("frozen eligibility site identities are not contiguous from zero")

    inventory = parse_inventory(frozen_table(INVENTORY_PATH))
    if len(inventory) != len(eligibility):
        raise ValueError("frozen inventory and eligibility counts differ")
    sites = []
    for site_id, values in enumerate(inventory):
        project, path, kind, _inventory_class, span, owner_span, edit_span, line, owner = values
        row = eligibility[site_id]
        if (
            row["project"] != project
            or row["path"] != path
            or row["kind"] != kind
            or int(row["line"]) != line
            or row["owner"] != owner
        ):
            raise ValueError(f"inventory identity differs from frozen eligibility at site {site_id}")
        retained_row = retained.get(site_id)
        sites.append(
            Site(
                site=site_id,
                project=project,
                path=path,
                line=line,
                kind=kind,
                category=row["category"],
                owner=owner,
                reason=retained_row["reason"] if retained_row else "",
                span=span,
                owner_span=owner_span,
                edit_span=edit_span or Span(0, 0),
            )
        )
    if not set(retained).issubset(set(eligibility)):
        raise ValueError("retained table names a site outside the frozen inventory")
    for site_id in retained:
        if eligibility[site_id]["category"] not in {"eligible-local", "eligible-internal"}:
            raise ValueError(f"retained site {site_id} is outside the frozen eligible denominator")
    if sum(site.category == "eligible-local" for site in sites) != 693:
        raise ValueError("frozen local denominator changed")
    if sum(site.category == "eligible-internal" for site in sites) != 2190:
        raise ValueError("frozen internal denominator changed")
    individual_effect_ids, effect_facts = load_effect_evidence(sites)
    return sites, eligibility, retained, individual_effect_ids, effect_facts


def project_root(view_root, project):
    root = Path(view_root)
    if root.is_symlink() or not root.is_dir():
        raise ValueError(f"source view root must be a real directory: {root}")
    project_root = root / project
    if project_root.is_symlink() or not project_root.is_dir():
        raise ValueError(f"project source root must be a real directory: {project_root}")
    return project_root


def source_view_digest(view_root):
    try:
        result = subprocess.check_output(
            [sys.executable, str(VIEW_DIGEST_SCRIPT), str(view_root)],
            text=True,
        )
    except subprocess.CalledProcessError as error:
        raise ValueError(f"cannot digest source view {view_root}") from error
    fields = result.splitlines()
    if len(fields) != 1:
        raise ValueError(f"unexpected digest output for {view_root}")
    name, count, digest = fields[0].split("\t")
    return name, int(count), digest


def verify_view_digest(view_root, expected_name, expected_count, expected_digest):
    name, count, digest = source_view_digest(view_root)
    if name != expected_name or count != expected_count or digest != expected_digest:
        raise ValueError(
            f"{expected_name} source view does not match the accepted 1,040-edit digest"
        )


def safe_source_file(view_root, project, relative):
    base = project_root(view_root, project)
    relative_path = PurePosixPath(relative)
    if relative_path.is_absolute() or not relative_path.parts or any(
        part in {"", ".", ".."} for part in relative_path.parts
    ):
        raise ValueError(f"source path is not project-relative: {relative}")
    path = base
    for index, part in enumerate(relative_path.parts):
        path = path / part
        try:
            path.lstat()
        except OSError as error:
            raise ValueError(f"source path is unavailable: {path}") from error
        if path.is_symlink():
            raise ValueError(f"source file path contains a symlink: {path}")
        if index < len(relative_path.parts) - 1 and not path.is_dir():
            raise ValueError(f"source path parent is not a directory: {path}")
        if index == len(relative_path.parts) - 1 and not path.is_file():
            raise ValueError(f"source path is not a regular file: {path}")
    return path


def spans_by_file(sites, eligible_ids, retained_ids):
    removed = {}
    for site in sites:
        if site.site not in eligible_ids or site.site in retained_ids:
            continue
        if site.edit_span.length == 0:
            raise ValueError(f"removed site {site.site} has no edit span")
        removed.setdefault((site.project, site.path), []).append(site.edit_span)
    for key, spans in removed.items():
        spans.sort(key=lambda span: (span.start, span.end))
        if any(left.end > right.start for left, right in zip(spans, spans[1:])):
            raise ValueError(f"accepted removal spans overlap in {key[0]}:{key[1]}")
    if sum(map(len, removed.values())) != 1040:
        raise ValueError("accepted source view does not contain the frozen 1,040-removal set")
    return removed


def erase_spans(source, spans):
    result = source
    ordered = sorted(spans, key=lambda span: (span.start, span.end), reverse=True)
    if any(left.start < right.end for left, right in zip(ordered, ordered[1:])):
        raise ValueError("source edit spans overlap")
    for span in ordered:
        if span.end > len(result):
            raise ValueError("source edit span is outside its source file")
        result = result[:span.start] + result[span.end:]
    return result


def bytes_before(spans, offset):
    return sum(span.length for span in spans if span.end <= offset)


def validate_views(views_root, sites, eligibility, retained):
    original_root = Path(views_root) / "original"
    reduced_root = Path(views_root) / "reduced-1040"
    for view in (original_root, reduced_root):
        if view.is_symlink() or not view.is_dir():
            raise ValueError(f"expected immutable source view directory: {view}")
        for project in ("xsh", "laputa"):
            project_root(view, project)
    for name, view in (("original", original_root), ("reduced-1040", reduced_root)):
        verify_view_digest(
            view,
            name,
            EXPECTED_VIEW_ENTRIES,
            EXPECTED_VIEW_DIGESTS[name],
        )

    eligible_ids = {
        site_id
        for site_id, row in eligibility.items()
        if row["category"] in {"eligible-local", "eligible-internal"}
    }
    retained_ids = set(retained)
    if len(eligible_ids) != 2883 or len(eligible_ids - retained_ids) != 1040:
        raise ValueError("frozen eligible and retained counts no longer describe 1,040 removals")
    removed = spans_by_file(sites, eligible_ids, retained_ids)

    for project in ("xsh", "laputa"):
        original_project = project_root(original_root, project)
        reduced_project = project_root(reduced_root, project)
        original_entries = entries(original_project)
        reduced_entries = entries(reduced_project)
        if original_entries.keys() != reduced_entries.keys():
            raise ValueError(f"{project} source views contain different paths")
        removed_paths = {
            PurePosixPath(path): spans
            for (entry_project, path), spans in removed.items()
            if entry_project == project
        }
        for relative in sorted(original_entries):
            left_entry = original_entries[relative]
            right_entry = reduced_entries[relative]
            if left_entry[0] != right_entry[0] or left_entry[1] != right_entry[1]:
                raise ValueError(f"source view metadata differs at {project}:{relative}")
            left = original_project / relative
            right = reduced_project / relative
            if left_entry[0] == "file":
                spans = removed_paths.get(relative, [])
                expected_size = left_entry[2] - sum(span.length for span in spans)
                if right_entry[2] != expected_size:
                    raise ValueError(f"source view size differs from accepted spans at {project}:{relative}")
                if spans:
                    expected = erase_spans(left.read_bytes(), spans)
                    if right.read_bytes() != expected:
                        raise ValueError(f"source bytes differ from accepted spans at {project}:{relative}")
                elif left.read_bytes() != right.read_bytes():
                    raise ValueError(f"unselected source bytes differ at {project}:{relative}")

def line_at(source, offset):
    return source[:offset].count(b"\n") + 1


def exact_site_bytes(view_root, site):
    path = safe_source_file(view_root, site.project, site.path)
    source = path.read_bytes()
    if site.span.end > len(source) or site.owner_span.end > len(source):
        raise ValueError(f"source span is outside {site.project}:{site.path} at site {site.site}")
    if line_at(source, site.span.start) != site.line:
        raise ValueError(f"source line differs at frozen site {site.site}")
    if not site.owner_span.start <= site.span.start < site.span.end <= site.owner_span.end:
        raise ValueError(f"site {site.site} falls outside its frozen owner span")
    if site.edit_span.end and not site.edit_span.start <= site.span.start < site.span.end <= site.edit_span.end:
        raise ValueError(f"site {site.site} falls outside its edit span")
    annotation = source[site.span.start:site.span.end]
    owner = source[site.owner_span.start:site.owner_span.end]
    if not annotation or not owner:
        raise ValueError(f"site {site.site} has empty source evidence")
    line_end = source.find(b"\n", site.span.end)
    if line_end < 0:
        line_end = len(source)
    context = source[site.owner_span.start:line_end]
    return source, annotation, owner, context


def reduced_site_bytes(site, original, reduced, removed_spans):
    file_spans = removed_spans.get((site.project, site.path), [])
    if any(
        span.start < site.span.end and site.span.start < span.end
        for span in file_spans
    ):
        raise ValueError(f"retained annotation overlaps an accepted removal at site {site.site}")
    original_annotation = original[site.span.start:site.span.end]
    reduced_start = site.span.start - bytes_before(file_spans, site.span.start)
    if reduced[reduced_start:reduced_start + len(original_annotation)] != original_annotation:
        raise ValueError(f"retained annotation bytes changed in the reduced view at site {site.site}")

    owner_spans = [
        Span(span.start - site.owner_span.start, span.end - site.owner_span.start)
        for span in file_spans
        if site.owner_span.start <= span.start and span.end <= site.owner_span.end
    ]
    owner_overlaps = [
        span
        for span in file_spans
        if span.start < site.owner_span.end and site.owner_span.start < span.end
    ]
    if len(owner_spans) != len(owner_overlaps):
        raise ValueError(f"an accepted removal crosses the owner boundary at site {site.site}")
    expected_owner = erase_spans(
        original[site.owner_span.start:site.owner_span.end],
        owner_spans,
    )
    reduced_owner_start = site.owner_span.start - bytes_before(file_spans, site.owner_span.start)
    if reduced[reduced_owner_start:reduced_owner_start + len(expected_owner)] != expected_owner:
        raise ValueError(f"owner source bytes changed in the reduced view at site {site.site}")
    return Span(reduced_start, reduced_start + len(original_annotation))


def classification(site, annotation, owner):
    if site.reason == PROC_PARAMETER_REASON:
        if site.kind != "parameter-type" or not site.owner.startswith("proc "):
            raise ValueError(f"unexpected Proc parameter contract at site {site.site}")
        return "proc-parameter-contract", "declaration-recheck"
    if site.reason == PROC_RETURN_REASON:
        if site.kind != "return-type" or not site.owner.startswith("proc "):
            raise ValueError(f"unexpected Proc return contract at site {site.site}")
        return "proc-return-contract", "declaration-recheck"
    if site.kind == "effect-list" and site.reason == EFFECT_REASON:
        return "effect-joint-proof-pending", "all-202-effects-joint"
    if site.site == 3598:
        if annotation.strip() != b"Map[Str]" or b"fence_labels" not in owner:
            raise ValueError("frozen nonempty Map selector at site 3598 changed")
        return "map-literal-selection", "one-site-source-meaning-control"
    if site.kind == "local-type" and annotation.startswith(b"Map[") and re.search(rb"=\s*\{\s*\}", owner):
        return "empty-map-record-selection", "one-site-source-meaning-control"
    if site.site in {3599, 3665}:
        if annotation.strip() != b"Str" or b".replace(" not in owner:
            raise ValueError(f"frozen ambiguous replace control changed at site {site.site}")
        return "replace-overload-control", "one-site-negative-control"
    if site.site == 2850:
        if b"{field}" not in owner or b"meminfo." not in owner:
            raise ValueError("frozen interpolation-only input at site 2850 changed")
        return "interpolation-contract-pending", "source-observation-contract-recheck"
    if site.site in SOURCE_CONTEXT_CLASSES:
        return "source-context-pending", SOURCE_CONTEXT_CLASSES[site.site]
    if site.reason.startswith("joint check check."):
        return "historical-diagnostic-only-pending", "declaration-group-recheck"
    raise ValueError(f"no frozen source classification for retained site {site.site}")


def declaration_key(site):
    tokens = site.owner.split()
    if tokens and tokens[0] in {"pure", "proc", "stream"} and len(tokens) > 1:
        return f"{site.project}:{site.path}:{tokens[0]} {tokens[1]}"
    return f"{site.project}:{site.path}:{site.owner}"


def format_bytes(value):
    return (
        value.decode("utf-8")
        .replace("\\", "\\\\")
        .replace("\t", "\\t")
        .replace("\r", "\\r")
        .replace("\n", "\\n")
    )


def witness_rows(views_root, sites, retained, eligibility, individual_effect_ids, effect_facts, selection):
    original_root = Path(views_root) / "original"
    reduced_root = Path(views_root) / "reduced-1040"
    eligible_ids = {
        site_id
        for site_id, row in eligibility.items()
        if row["category"] in {"eligible-local", "eligible-internal"}
    }
    removed = spans_by_file(sites, eligible_ids, set(retained))
    rows = []
    for site in sites:
        if site.site not in retained:
            continue
        original, annotation, owner, context = exact_site_bytes(original_root, site)
        reduced_path = safe_source_file(reduced_root, site.project, site.path)
        reduced = reduced_path.read_bytes()
        reduced_span = reduced_site_bytes(site, original, reduced, removed)
        kind, witness_type = classification(site, annotation, owner)
        effect_evidence = ""
        if kind == "effect-joint-proof-pending":
            tokens = site.owner.split()
            key = (site.project, site.path, tokens[1])
            if site.site in individual_effect_ids:
                effect_evidence = "historical individual lint finding; not a joint proof"
            else:
                fact = effect_facts[key]
                effect_evidence = (
                    f"R6 declared={fact['declared']}; effective={fact['effective']}; "
                    f"required={fact['required']}; calls={fact['source_call_sites']}"
                )
            group = "all-202-effect-clauses"
        elif kind.startswith("proc-"):
            group = declaration_key(site)
        else:
            group = declaration_key(site)
        rows.append(
            (
                site.site,
                site.project,
                site.path,
                site.line,
                site.kind,
                site.owner,
                site.reason,
                kind,
            "in-scope; source-only; pending executable recheck; no necessity or removal claim",
                witness_type,
                group,
                f"site:{site.site}",
                f"{site.span.start}..{site.span.end}",
                f"{reduced_span.start}..{reduced_span.end}",
                format_bytes(annotation),
                format_bytes(context),
                effect_evidence,
            )
        )

    counts = {}
    for row in rows:
        counts[row[7]] = counts.get(row[7], 0) + 1
    if counts != EXPECTED_COUNTS:
        raise ValueError(f"frozen classifications changed: {counts}")
    filters = {
        "all": lambda row: True,
        "proc": lambda row: row[7] in {"proc-parameter-contract", "proc-return-contract"},
        "effects": lambda row: row[7] == "effect-joint-proof-pending",
        "remaining": lambda row: row[7] not in {"proc-parameter-contract", "proc-return-contract", "effect-joint-proof-pending"},
    }
    if selection not in filters:
        raise ValueError(f"unknown witness selection: {selection}")
    return [row for row in rows if filters[selection](row)]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("views_root", type=Path)
    parser.add_argument(
        "--select",
        choices=("all", "proc", "effects", "remaining"),
        default="all",
    )
    args = parser.parse_args()
    sites, eligibility, retained, individual_effect_ids, effect_facts = load_corpus()
    validate_views(args.views_root, sites, eligibility, retained)
    rows = witness_rows(
        args.views_root,
        sites,
        retained,
        eligibility,
        individual_effect_ids,
        effect_facts,
        args.select,
    )
    writer = csv.writer(sys.stdout, delimiter="\t", lineterminator="\n")
    writer.writerow(
        (
            "site",
            "project",
            "path",
            "line",
            "kind",
            "owner",
            "historical_retained_reason",
            "classification",
            "status",
            "witness_type",
            "witness_group",
            "witness_site",
            "original_span_bytes",
            "reduced_span_bytes",
            "annotation_bytes_utf8_escaped",
            "minimal_source_context_utf8_escaped",
            "effect_evidence",
        )
    )
    writer.writerows(rows)


if __name__ == "__main__":
    main()
