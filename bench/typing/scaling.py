#!/usr/bin/env python3
"""Generate and verify the immutable typing scaling input closures."""

import argparse
import hashlib
import json
from pathlib import Path
import re

SEED = 877114
SIZES = (1000, 2000, 4000, 8000)
FAMILIES = (
    "immutable_aliases", "generic_forwarding", "wide_records",
    "nested_constraints", "declaration_components", "module_diamonds",
    "resolvable_overloads",
)
ADVERSARIAL = (
    "unresolved_overloads", "inferred_expansion", "deep_rows_containers",
    "recursive_aliases_equations", "invalid_nearly_matching",
    "captured_state", "diagnostic_fanout",
)
HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "scaling-manifest.json"
LIMITS = HERE / "resource-limits.json"
TOKEN = re.compile(r'"(?:\\.|[^"\\])*"|[A-Za-z_][A-Za-z_0-9]*|[0-9]+|[^\s]')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()


def number(index, seed):
    # Index arithmetic makes each size a prefix without interpreter RNG state.
    return (index * 1103515245 + seed * 12345) % 997


def generate(family, units, variant, seed=SEED):
    if family not in FAMILIES + ADVERSARIAL:
        raise ValueError("unknown scaling family")
    if units < 1 or variant not in ("annotated", "inferred"):
        raise ValueError("positive units and an annotated/inferred variant are required")
    inferred = variant == "inferred"
    files = {}
    lines = []
    if family == "immutable_aliases":
        lines.append("let anchor: Int = 1")
        for i in range(units):
            previous = "anchor" if i == 0 else f"alias_{i - 1}"
            annotation = "" if inferred else ": Int"
            lines.append(f"let alias_{i}{annotation} = {previous}")
    elif family == "generic_forwarding":
        domains = (("int", "Int", "1"), ("str", "Str", '"a"'),
                   ("list", "List[Int]", "[1]"))
        for i in range(units):
            if inferred:
                lines.extend((f"pure twice_{i}(value) {{ value + value }}",
                              f"pure forward_{i}(value) {{ twice_{i}(value) }}"))
            for suffix, typ, value in domains:
                target = f"forward_{i}"
                if not inferred:
                    lines.extend((f"pure twice_{i}_{suffix}(value: {typ}) -> {typ} {{ value + value }}",
                                  f"pure forward_{i}_{suffix}(value: {typ}) -> {typ} {{ twice_{i}_{suffix}(value) }}"))
                    target += f"_{suffix}"
                lines.append(f"let result_{i}_{suffix}: {typ} = {target}({value})")
    elif family == "wide_records":
        if not inferred:
            lines.append("type Wide = {" + ", ".join(f"field_{i}: Int" for i in range(units)) + "}")
        annotation = "" if inferred else ": Wide"
        fields = ", ".join(f"field_{i}: {number(i, seed)}" for i in range(units))
        lines.append(f"let record{annotation} = {{{fields}}}")
        for i in range(units):
            lines.extend((f"let updated_{i}{annotation} = {{...record, field_{i}: {number(i + 1, seed)}}}",
                          f"let projected_{i}: Int = updated_{i}.field_{i}"))
    elif family == "nested_constraints":
        for i in range(units):
            lines.append(f"type Nested_{i}[T] = {{items: List[Map[List[T]]], anchor: T}}")
            annotation = "" if inferred else f": Nested_{i}[Int]"
            lines.append(f"let nested_{i}{annotation} = Nested_{i}(items: [{{key: [{number(i, seed)}]}}], anchor: {number(i, seed)})")
            lines.append(f'let projected_{i}: Int = (nested_{i}.items[0].get("key") ?? [0])[0]')
    elif family == "declaration_components":
        effect = "" if inferred else " [error]"
        for i in range(units):
            lines.extend((
                f"pure independent_{i}(value: Int) -> Int {{ value + {number(i, seed)} }}",
                f'proc even_{i}(value: Int){effect} -> Int {{ if value == 0 {{ assert true, "leaf"; return {number(i, seed)} }}; odd_{i}(value - 1) }}',
                f"proc odd_{i}(value: Int){effect} -> Int {{ even_{i}(value - 1) }}",
                f"let sample_{i}: Int = even_{i}(0)",
            ))
    elif family == "module_diamonds":
        for i in range(units):
            namespace = f"typing_scaling.d{i:06d}"
            base = namespace.replace(".", "/")
            if inferred:
                leaf = "##! Shared immutable identity.\n## Preserve the supplied value.\nexport pure identity(value) { value }\n"
                left = f"##! Forward the shared identity.\nuse {namespace}.leaf as shared\n## Preserve the supplied value through the shared leaf.\nexport pure forward(value) {{ shared.identity(value) }}\n"
                right = left
            else:
                leaf = "##! Shared immutable identities.\n## Preserve the supplied integer.\nexport pure identity_int(value: Int) -> Int { value }\n## Preserve the supplied text.\nexport pure identity_str(value: Str) -> Str { value }\n"
                left = f"##! Forward the shared integer identity.\nuse {namespace}.leaf as shared\n## Preserve the supplied integer through the shared leaf.\nexport pure forward(value: Int) -> Int {{ shared.identity_int(value) }}\n"
                right = f"##! Forward the shared text identity.\nuse {namespace}.leaf as shared\n## Preserve the supplied text through the shared leaf.\nexport pure forward(value: Str) -> Str {{ shared.identity_str(value) }}\n"
            files[f"{base}/leaf.xsh"] = leaf.encode()
            files[f"{base}/left.xsh"] = left.encode()
            files[f"{base}/right.xsh"] = right.encode()
            lines.extend((f"use {namespace}.left as left_{i}",
                          f"use {namespace}.right as right_{i}",
                          f"let integer_{i}: Int = left_{i}.forward({number(i, seed)})",
                          f'let text_{i}: Str = right_{i}.forward("unit_{i}")'))
    elif family == "resolvable_overloads":
        annotation = "" if inferred else ": Str"
        result = "" if inferred else " -> Str"
        for i in range(units):
            lines.extend((
                f'pure normalize_{i}(value{annotation}){result} {{ value.trim().split(",").join(":") }}',
                f'let text_{i}: Str = normalize_{i}(" a,b ")',
                f"let table_{i}: Map[Int] = map.empty()",
                f'let updated_{i} = table_{i}.set(value: {number(i, seed)}, key: "unit_{i}")',
                f'let selected_{i}: Int = updated_{i}.get(key: "unit_{i}") ?? 0',
            ))
    elif family == "unresolved_overloads":
        for i in range(units):
            lines.append(f"pure unresolved_{i}() {{ let hidden = []; hidden.get(0)?.len() }}")
        lines.append("let attempted = unresolved_0()")
    elif family == "inferred_expansion":
        lines.append("pure expansion_0(value) { value }")
        for i in range(1, units + 1):
            lines.append(f"pure expansion_{i}(value) {{ let previous = expansion_{i - 1}(value); {{left: previous, right: previous, level: {i}}} }}")
        lines.append(f"let invalid: Int = expansion_{units}(0)")
    elif family == "deep_rows_containers":
        for i in range(units):
            previous = "Int" if i == 0 else f"Deep_{i - 1}"
            lines.append(f"type Deep_{i} = {{items: List[{previous}]}}")
        lines.append(f"let deep: Deep_{units - 1} = {{items: []}}")
    elif family == "recursive_aliases_equations":
        for i in range(units):
            if variant == "annotated":
                lines.append(f"type Infinite_{i}[T] = Infinite_{i}[List[T]]")
            else:
                lines.append(f"pure equation_{i}(value) {{ value.call(value) }}")
    elif family == "invalid_nearly_matching":
        for i in range(units):
            lines.extend((f"let table_{i}: Map[Int] = map.empty()",
                          f'let invalid_{i} = table_{i}.set(value: "wrong", key: "unit_{i}")'))
    elif family == "captured_state":
        for i in range(units):
            lines.extend((f"var stored_{i} = []",
                          f"proc remember_{i}(value) {{ stored_{i} = stored_{i}.push(value); value }}",
                          f"let first_{i}: Int = remember_{i}(1)",
                          f'let second_{i}: Str = remember_{i}("wrong")'))
    elif family == "diagnostic_fanout":
        for i in range(units):
            lines.append(f'let invalid_{i}: Int = "wrong_{number(i, seed)}"')
    files["entry.xsh"] = ("\n".join(lines) + "\n").encode()
    return files


UNIT_DEFINITIONS = {
    "immutable_aliases": "One immutable alias binding and edge to its predecessor; one fixed anchor.",
    "generic_forwarding": "One twice/forward helper pair instantiated at Int, Str and List[Int]; annotated helpers are handwritten specializations.",
    "wide_records": "One unique row label, one value-preserving row update, and one projection; the common row width is exactly units.",
    "nested_constraints": "One independently owned parametric record schema with List/Map/List nesting, one constructor instantiation and one projection.",
    "declaration_components": "One independent pure declaration, one two-procedure recursive error-effect component and one anchored call.",
    "module_diamonds": "One distinct leaf plus two distinct forwarding modules imported by the entry; both forwarding modules import the same canonical leaf.",
    "resolvable_overloads": "One trim/split/join requirement chain, one anchored call, and one valid Map set/get pair with named argument order reversed.",
    "unresolved_overloads": "One zero-argument declaration whose hidden empty-list element must support len without an exported type relationship.",
    "inferred_expansion": "One acyclic shared pair expansion; unfolded output doubles per level but normalized output retains a single predecessor edge pair.",
    "deep_rows_containers": "One alternating record/List layer linked to its predecessor; last declaration demands the complete chain.",
    "recursive_aliases_equations": "One infinite parametric alias in the annotated variant or one self-application equation in the inferred variant; separate closures prevent alias failure from hiding equation behavior.",
    "invalid_nearly_matching": "One concretely anchored Map[Int] set call with correct labels and incompatible value.",
    "captured_state": "One captured mutable collection written through a helper at incompatible Int and Str calls.",
    "diagnostic_fanout": "One independently named Int binding supplied an incompatible Str value.",
}


COUNTERS = [
    "input_ast_nodes", "normalized_output_dag_nodes", "normalized_output_dag_edges",
    "total_solver_work_units", "type_nodes", "row_nodes", "variables", "constraints",
    "unification_visits", "occurs_check_visits", "generalization_visits", "instantiations",
    "candidate_probes", "failed_candidate_probes", "candidate_filter_visits", "dependency_wakeups",
    "effect_edge_visits", "module_dependency_edges", "module_solve_count", "reason_edges",
    "diagnostic_work_units", "rendered_diagnostic_bytes", "retained_type_bytes",
    "retained_constraint_bytes", "retained_reason_bytes", "retained_interface_bytes",
]
FAMILY_COUNTERS = {
    "immutable_aliases": ["unification_visits", "generalization_visits"],
    "generic_forwarding": ["instantiations", "candidate_probes", "failed_candidate_probes", "dependency_wakeups"],
    "wide_records": ["row_nodes", "unification_visits", "occurs_check_visits"],
    "nested_constraints": ["unification_visits", "occurs_check_visits", "instantiations"],
    "declaration_components": ["effect_edge_visits", "dependency_wakeups", "generalization_visits"],
    "module_diamonds": ["module_dependency_edges", "module_solve_count", "instantiations"],
    "resolvable_overloads": ["candidate_probes", "failed_candidate_probes", "candidate_filter_visits", "dependency_wakeups"],
}


def source_index(files):
    return [{"path": path, "canonical_namespace": None if path == "entry.xsh" else path[:-4].replace("/", "."),
             "bytes": len(data), "sha256": sha(data)}
            for path, data in sorted(files.items())]


def output_model(family, units, variant):
    # These are required source-owned facts, not measurements of checker allocation.
    multiplier = {"immutable_aliases": 1, "generic_forwarding": 5 if variant == "inferred" else 9,
                  "wide_records": 3, "nested_constraints": 3, "declaration_components": 4,
                  "module_diamonds": 7 if variant == "inferred" else 8,
                  "resolvable_overloads": 5}.get(family)
    return {
        "model": "Unique source-owned binding/declaration facts plus canonical structural rows; aliases share their underlying types.",
        "source_owned_facts": None if multiplier is None else multiplier * units + (1 if family in ("immutable_aliases", "wide_records") else 0) + (1 if family == "wide_records" and variant == "annotated" else 0),
        "distinct_row_labels": units if family == "wide_records" else None,
        "expected_import_sources": 3 * units + 1 if family == "module_diamonds" else 1,
        "expected_import_edges": 4 * units if family == "module_diamonds" else 0,
        "expected_once_per_bundle_solves": 3 * units + 1 if family == "module_diamonds" else 1,
        "actual_normalized_dag_nodes": None,
        "actual_normalized_dag_edges": None,
        "actual_status": "not-run",
        "actual_reason": "Must be counted from solved output; source fact counts are not a substitute for measured normalized output or retained bytes.",
    }


def case(family, units, variant):
    files = generate(family, units, variant)
    index = source_index(files)
    regular = family in FAMILIES
    future = variant == "inferred" and family in ("generic_forwarding", "module_diamonds", "resolvable_overloads")
    baseline_status = "not-applicable" if future or not regular else "not-run"
    return {
        "id": f"{family}-{units}-{variant}", "family": family,
        "kind": "regular" if regular else "adversarial", "units": units,
        "unit_definition": UNIT_DEFINITIONS[family], "variant": variant, "seed": SEED,
        "source_files": len(files), "source_bytes": sum(len(data) for data in files.values()),
        "source_lines": sum(data.count(b"\n") for data in files.values()),
        "lexical_tokens": sum(len(TOKEN.findall(data.decode())) for data in files.values()),
        "input_ast_nodes": None,
        "source_index_sha256": sha(encoded(index)),
        "closure_sha256": sha(b"".join(path.encode() + b"\0" + len(data).to_bytes(8, "big") + data
                                        for path, data in sorted(files.items()))),
        "entry": "entry.xsh", "module_root": ".", "module_namespace": "typing_scaling" if family == "module_diamonds" else None,
        "command": ["xsht", "check", "{generated_case_root}/entry.xsh"],
        "environment": {"XSH_MODULE_PATH": "{generated_case_root}"},
        "baseline": {"status": baseline_status,
                     "reason": "Historical rejection of new omitted-parameter inference is expected; target remains mandatory and pending." if future else
                               "Adversarial candidate termination/rejection witness; old rejection does not establish future soundness." if not regular else
                               "Existing annotated grammar/behavior; full-size baseline counters have not run."},
        "candidate": {"status": "not-run", "expected": "accepted-without-resource-guard" if regular else "local-rejection-within-frozen-guards",
                      "reason": "Inference and counter implementation pending; every frozen case remains mandatory."},
        "normal_output": output_model(family, units, variant),
        "required_counters": COUNTERS,
        "family_work_counters": FAMILY_COUNTERS.get(family, ["total_solver_work_units", "diagnostic_work_units"]),
        "generator_resource_envelope": {
            "row_labels": units if family == "wide_records" else 3,
            "structural_depth": units * 2 if family == "deep_rows_containers" else units + 1 if family == "inferred_expansion" else 8,
            "source_fact_bound": 128 * units + 128,
            "note": "Conservative source-fact envelope, not a claimed compiler node/work count; compiler counters must independently stay below guards.",
        },
    }


def build_manifest():
    limits = json.loads(LIMITS.read_text())
    cases = [case(family, units, variant) for family in FAMILIES for units in SIZES
             for variant in ("annotated", "inferred")]
    adversarial_sizes = dict.fromkeys(ADVERSARIAL, 1024)
    adversarial_sizes["inferred_expansion"] = 128
    cases.extend(case(family, adversarial_sizes[family], "inferred") for family in ADVERSARIAL)
    cases.append(case("recursive_aliases_equations", 1024, "annotated"))
    for item in cases:
        if item["source_bytes"] > limits["source_bytes"] or item["source_files"] > limits["sources"]:
            raise ValueError("generated source closure exceeds frozen source guards")
        if item["kind"] == "regular":
            envelope = item["generator_resource_envelope"]
            if envelope["row_labels"] > limits["row_labels"] or envelope["source_fact_bound"] > limits["type_row_nodes"]:
                raise ValueError("regular input envelope exceeds frozen graph/row guards")
    return {
        "schema_version": 1, "status": "frozen-before-inference", "seed": SEED,
        "generator": {"path": "bench/typing/scaling.py", "sha256": sha(Path(__file__).read_bytes()),
                      "algorithm": "UTF-8, LF, deterministic index arithmetic; sizes are prefix-comparable except shared record width and fixed entry scaffolding."},
        "resource_limits": {"path": "bench/typing/resource-limits.json", "sha256": sha(LIMITS.read_bytes())},
        "source_retention": "Generate closures on demand before counter runs; verify every file digest and full closure against this manifest. sources.json preserves exact per-file hashes.",
        "regular_sizes": list(SIZES), "regular_families": list(FAMILIES), "adversarial_families": list(ADVERSARIAL),
        "regular_cases": len(FAMILIES) * len(SIZES) * 2, "adversarial_cases": len(ADVERSARIAL) + 1,
        "scaling_gates": {"doublings": [[2000, 4000], [4000, 8000]],
                          "maximum_total_and_family_work_ratio": 2.6,
                          "maximum_retained_type_and_constraint_bytes_ratio": 2.5,
                          "investigate_wall_time_ratio_above": 2.8,
                          "normalization": "Raw work and retained-byte ratios are mandatory. Report unavoidable normalized output DAG growth separately; never divide by copied/expanded avoidable type structures.",
                          "resource_guard_hit": "failed", "unavailable_counter": "not-run",
                          "module_identity": "Canonical resolved relative path within each fresh generated bundle, mapped to loader SourceId/ModuleId by instrumentation; no cross-bundle cache credit.",
                          "module_solve_expectation": "Exactly one solve per source; every diamond leaf has two incoming dependency edges and one solve."},
        "commands": {"verify": ["python3", "bench/typing/scaling.py", "verify"],
                     "materialize": ["python3", "bench/typing/scaling.py", "generate", "--case", "{case_id}", "--output", "{new_case_root}"],
                     "tests": ["python3", "-m", "unittest", "discover", "-s", "bench/typing/scaling", "-p", "test_scaling.py"]},
        "cases": cases,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("freeze")
    commands.add_parser("verify")
    generate_parser = commands.add_parser("generate")
    generate_parser.add_argument("--case", required=True)
    generate_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "freeze":
        if MANIFEST.exists():
            parser.error("manifest already frozen; do not replace input identities")
        MANIFEST.write_bytes(encoded(build_manifest()))
        print("Frozen 56 regular and 8 adversarial source closures across seven families each.")
    elif args.command == "verify":
        expected = json.loads(MANIFEST.read_text())
        actual = build_manifest()
        if actual != expected:
            parser.error("generator, guards, or input identities differ from the frozen manifest")
        print(f"Verified {len(actual['cases'])} immutable source closures.")
    else:
        manifest = json.loads(MANIFEST.read_text())
        if sha(Path(__file__).read_bytes()) != manifest["generator"]["sha256"]:
            parser.error("generator differs from frozen identity")
        if sha(LIMITS.read_bytes()) != manifest["resource_limits"]["sha256"]:
            parser.error("resource guards differ from frozen identity")
        expected = next((item for item in manifest["cases"] if item["id"] == args.case), None)
        if expected is None:
            parser.error("case is outside frozen scaling inputs")
        files = generate(expected["family"], expected["units"], expected["variant"], expected["seed"])
        if sha(encoded(source_index(files))) != expected["source_index_sha256"]:
            parser.error("generated file hashes differ from frozen identity")
        if args.output.exists():
            parser.error("output must be a new directory to preserve fresh bundle identity")
        args.output.mkdir(parents=True)
        for path, data in files.items():
            target = args.output / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        (args.output / "sources.json").write_bytes(encoded(source_index(files)))
        print(json.dumps({"case": args.case, "entry": str((args.output / "entry.xsh").resolve()),
                          "XSH_MODULE_PATH": str(args.output.resolve()), "closure_sha256": expected["closure_sha256"]}))


if __name__ == "__main__":
    main()
