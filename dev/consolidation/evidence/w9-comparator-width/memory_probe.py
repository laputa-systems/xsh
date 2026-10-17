import contextlib
import gc
import os
import pathlib
import sys


comparator_root = pathlib.Path(sys.argv[1])
capture_root = pathlib.Path(sys.argv[2])
sys.path.insert(0, str(comparator_root / "dev/consolidation"))
import w9_publication_adapter_runner as comparator
import w9_publication_exact_owner_runner as exact
import w9_assignment_facts as assignment


def fields(path):
    rows = {}
    for line in path.read_text().splitlines():
        parts = line.split("\t")
        if len(parts) == 2:
            if parts[0] in rows:
                raise ValueError(f"duplicate field {parts[0]} in {path}")
            rows[parts[0]] = parts[1]
    return rows


candidate_capture = fields(capture_root / "adapter-capture/candidate/capture.receipt")
source_receipt = pathlib.Path(candidate_capture["source_receipt"])
source = fields(source_receipt)
runner = comparator._load_publication_runner(
    pathlib.Path(source["controller_root"]) / "dev/consolidation/w9-publication-run.py"
)
expected_inputs, _paths = runner.validate_source_cohort_inputs(source)
count = len(expected_inputs)
assert count == 1690
print("diagnostic_only=true fresh_parity_receipt=false", flush=True)


def capture(label):
    root = capture_root / label
    outcomes = comparator.aggregate_candidate_receipts(
        runner, root / "receipts", expected_inputs, None
    )
    role = "derived-reference" if label.startswith("derived-reference") else root.name
    hashes = comparator._parse_raw_hash_output(
        (capture_root / "compare-derived/comparison/hashjobs" / role / "worker.log").read_text(),
        count,
    )
    return root / "raw", outcomes, hashes


original_raw, original_outcomes, original_hashes = capture("adapter-capture/reference")
reference_raw, reference_outcomes, reference_hashes = capture(
    "derived-reference-capture/derived-reference"
)
candidate_raw, candidate_outcomes, candidate_hashes = capture("adapter-capture/candidate")


def owners(role):
    record = fields(capture_root / f"exact-{role}/capture.receipt")
    raw = pathlib.Path(record["raw_root"])
    result = {}
    for index in expected_inputs:
        result[index] = exact.parse_exact_owner_index(
            (raw / f"owners-{index}.tsv").read_text(), allow_source_audit=True
        )
    print(f"loaded_exact_owners role={role} count={len(result)}", flush=True)
    return result


reference_owners = owners("reference")
candidate_owners = owners("candidate")
assignment_record = fields(capture_root / "assignment-facts/assignment-capture.receipt")
proofs = exact._assignment_sidecar_proofs(
    pathlib.Path(assignment_record["sidecar_root"]), expected_inputs, source,
    assignment_record["assignment_helper_sha256"], assignment_record["corpus_blake3"],
    assignment, None, runner,
)
print(f"loaded_assignment_proofs count={len(proofs)}", flush=True)

weights = {
    index: sum(
        (candidate_raw / f"record-{index}-{profile}" / "publication").stat().st_size
        for profile in comparator.PROFILES
    )
    for index in expected_inputs
}
selected = tuple(sorted(sorted(weights, key=lambda index: (-weights[index], index))[:8]))
print(f"selected_original_indices={selected}", flush=True)
print(f"selected_publication_bytes={tuple(weights[index] for index in selected)}", flush=True)

with contextlib.ExitStack() as stack:
    def open_root(path):
        descriptor = runner.open_absolute_nofollow(path, directory=True)
        stack.callback(os.close, descriptor)
        return descriptor

    context = comparator._PublicationComparisonContext(
        open_root(reference_raw), open_root(candidate_raw), expected_inputs,
        reference_outcomes, candidate_outcomes.outcomes, reference_hashes, candidate_hashes,
        reference_owners, candidate_owners,
        (pathlib.Path(source["controller_root"]) / ".work/wt/w9-derived-api-imports/stdlib/tui.xsh").read_bytes(),
        (pathlib.Path(source["candidate_root"]) / "stdlib/tui.xsh").read_bytes(),
        proofs, assignment, True,
        comparator._OriginalPublicationCapture(
            open_root(original_raw), original_outcomes.outcomes, original_hashes
        ),
    )
    official_compare = comparator._compare_publication_input

    def compare_selected(position, comparison_context):
        index = selected[position]
        result = official_compare(index, comparison_context)
        if result.mismatches:
            raise RuntimeError(f"input {index} strict mismatch: {result.mismatches}")
        return result

    comparator._compare_publication_input = compare_selected
    observed = []

    def accept(position, result):
        observed.append((selected[position], result))
        print(f"completed_original_index={selected[position]} mismatches={len(result.mismatches)}", flush=True)

    before = gc.isenabled(), gc.get_freeze_count()
    comparator._run_parallel_publication_comparisons(context, len(selected), accept)
    assert tuple(index for index, _result in observed) == selected
    assert (gc.isenabled(), gc.get_freeze_count()) == before
    assert not comparator.multiprocessing.active_children()
    print("diagnostic_complete inputs=8 outcomes=32 mismatches=0 children=0", flush=True)
    for name in ("memory.current", "memory.peak", "memory.events"):
        print(f"cgroup_{name}={(pathlib.Path('/sys/fs/cgroup') / name).read_text().strip()}", flush=True)
