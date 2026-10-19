import sys
sys.dont_write_bytecode = True

from pathlib import Path
import tempfile
import unittest

import frozen_witnesses as corpus


WORK_TMP = corpus.ROOT / ".work/tmp"


def site(site_id=1, kind="parameter-type", owner="pure f parameter x", reason=""):
    return corpus.Site(
        site=site_id,
        project="xsh",
        path="core/f.xsh",
        line=1,
        kind=kind,
        category="eligible-internal",
        owner=owner,
        reason=reason,
        span=corpus.Span(5, 8),
        owner_span=corpus.Span(0, 16),
        edit_span=corpus.Span(4, 8),
    )


class FrozenWitnessTests(unittest.TestCase):
    def test_frozen_tables_keep_the_accepted_denominators_and_effect_partition(self):
        sites, eligibility, retained, individual, facts = corpus.load_corpus()
        retained_sites = [candidate for candidate in sites if candidate.site in retained]
        self.assertEqual(len(sites), 9194)
        self.assertEqual(len(eligibility), 9194)
        self.assertEqual(len(retained), 1843)
        self.assertEqual(sum(candidate.category == "eligible-local" for candidate in sites), 693)
        self.assertEqual(sum(candidate.category == "eligible-internal" for candidate in sites), 2190)
        self.assertEqual(sum(candidate.kind == "effect-list" for candidate in retained_sites), 202)
        self.assertEqual(len(individual), 142)
        self.assertEqual(len(facts), 60)
        self.assertEqual(sum(corpus.EXPECTED_COUNTS.values()), len(retained))

    def test_classifications_keep_source_observations_separate_from_causes(self):
        parameter = site(owner="proc f parameter x", reason=corpus.PROC_PARAMETER_REASON)
        self.assertEqual(
            corpus.classification(parameter, b"Str", b"proc f(x: Str) {}"),
            ("proc-parameter-contract", "declaration-recheck"),
        )
        effect = site(kind="effect-list", owner="proc f effects", reason=corpus.EFFECT_REASON)
        self.assertEqual(
            corpus.classification(effect, b"[io]", b"proc f() [io] {}"),
            ("effect-joint-proof-pending", "all-202-effects-joint"),
        )
        diagnostic = site(reason="joint check check.infer-return in pure f; restore declaration group")
        self.assertEqual(
            corpus.classification(diagnostic, b"Str", b"pure f(x: Str) -> Str {}"),
            ("historical-diagnostic-only-pending", "declaration-group-recheck"),
        )

    def test_map_selector_class_requires_the_literal_source_shape(self):
        candidate = site(kind="local-type", owner="var labels")
        self.assertEqual(
            corpus.classification(candidate, b"Map[Str]", b"var labels: Map[Str] = {}"),
            ("empty-map-record-selection", "one-site-source-meaning-control"),
        )
        pending = corpus.Site(**{**candidate.__dict__, "reason": "joint check check.local-inference in var labels"})
        self.assertEqual(
            corpus.classification(pending, b"Map[Str]", b"var labels: Map[Str] = {x: \"x\"}"),
            ("historical-diagnostic-only-pending", "declaration-group-recheck"),
        )

    def test_interpolation_source_fact_does_not_assign_a_concrete_type_cause(self):
        input_site = site(site_id=2850)
        self.assertEqual(
            corpus.classification(
                input_site,
                b"Str",
                b'proc f(field: Str) { print f"{field}"; read meminfo.value }',
            ),
            ("interpolation-contract-pending", "source-observation-contract-recheck"),
        )

    def test_source_context_shapes_keep_the_seventeen_site_partition(self):
        counts = corpus.Counter(corpus.SOURCE_CONTEXT_CLASSES.values())
        self.assertEqual(counts, corpus.EXPECTED_SOURCE_CONTEXT_SHAPES)
        self.assertEqual(len(corpus.SOURCE_CONTEXT_CLASSES), 17)
        self.assertEqual(corpus.SOURCE_CONTEXT_CLASSES[973], "single-visible-variant-spelling")
        self.assertEqual(corpus.SOURCE_CONTEXT_CLASSES[3914], "shared-variant-spelling")
        self.assertEqual(corpus.SOURCE_CONTEXT_CLASSES[9111], "enum-membership-operands")
        self.assertEqual(corpus.SOURCE_CONTEXT_CLASSES[9112], "enum-membership-operands")

    def test_historical_shape_totals_preserve_the_713_site_partition(self):
        self.assertEqual(
            sum(sites for sites, _groups in corpus.EXPECTED_DIAGNOSTIC_SOURCE_SHAPES.values()),
            713,
        )
        self.assertEqual(
            sum(groups for _sites, groups in corpus.EXPECTED_DIAGNOSTIC_SOURCE_SHAPES.values()),
            388,
        )

    def test_diagnostic_source_shape_priority_is_reproducible(self):
        cases = (
            ("pure f() { for x in xs { yield x } }", {"parameter-type", "return-type"}, "coupled-parameter-return"),
            ("pure f() { if x == null { return Some(y) } }", set(), "mixed-strong-cues"),
            ("pure f() { if x == null { return x } }", set(), "optional-row-join"),
            ("pure f() { for x in xs { yield x } }", set(), "iteration-collection"),
            ("pure f() { {name: value} }", set(), "record-variant-construction"),
            ("pure f() { match x { _ => y } }", set(), "variant-match"),
            ("pure f() { helper() }", set(), "typed-call-method"),
            ("pure f() { x + y }", set(), "operator-member-index"),
            ('pure f() { print f"{x}" }', set(), "format-interpolation"),
            ("var values = []", {"local-type"}, "empty-collection-initializer"),
            ("var enabled = true", {"local-type"}, "other-typed-initializer"),
        )
        for owner, kinds, expected in cases:
            candidate = site(owner=owner.split("(", 1)[0])
            self.assertEqual(corpus.historical_source_shape(candidate, owner.encode(), kinds), expected)

    def test_effect_spelling_candidates_are_not_resolved_edges(self):
        sites, _eligibility, retained, _individual, facts = corpus.load_corpus()
        labels, pairs, components = corpus.effect_spelling_candidate_components(sites, retained, facts)
        self.assertEqual(len(labels), 202)
        self.assertEqual(len(pairs), 42)
        self.assertEqual(components, corpus.Counter({1: 157, 2: 4, 3: 1, 9: 1, 25: 1}))
        label_counts = corpus.Counter(labels.values())
        self.assertEqual(label_counts["lint-only-outgoing-spellings-unavailable"], 119)
        self.assertEqual(label_counts["source-fact-no-spelling-candidate"], 38)
        component_sizes = sorted(
            label_counts[label]
            for label in label_counts
            if label.startswith("lexical-candidate-component:")
        )
        self.assertEqual(component_sizes, [2, 2, 2, 2, 3, 9, 25])

    def test_reduced_span_tracks_prior_accepted_deletions(self):
        original = b"pure f(a: Int, b: Str)"
        removed = corpus.Span(8, 13)
        retained = site(
            kind="parameter-type",
            owner="pure f parameter b",
            reason=corpus.PROC_PARAMETER_REASON,
        )
        retained = corpus.Site(
            **{
                **retained.__dict__,
                "span": corpus.Span(original.index(b"Str"), original.index(b"Str") + 3),
                "owner_span": corpus.Span(0, len(original)),
            }
        )
        reduced = corpus.erase_spans(original, [removed])
        mapped = corpus.reduced_site_bytes(retained, original, reduced, {("xsh", "core/f.xsh"): [removed]})
        self.assertEqual(reduced[mapped.start:mapped.end], b"Str")

    def test_overlapping_source_edits_fail_closed(self):
        with self.assertRaises(ValueError):
            corpus.erase_spans(b"abcdef", [corpus.Span(1, 4), corpus.Span(3, 5)])

    def test_source_paths_reject_traversal_and_symlinks(self):
        WORK_TMP.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="frozen-witness-path-", dir=WORK_TMP) as temporary:
            view_root = Path(temporary) / "view"
            project = view_root / "xsh"
            project.mkdir(parents=True)
            (project / "core").mkdir()
            (project / "core/f.xsh").write_text("pure f() {}\n")
            with self.assertRaises(ValueError):
                corpus.safe_source_file(view_root, "xsh", "../outside.xsh")
            (project / "linked").symlink_to(project / "core", target_is_directory=True)
            with self.assertRaises(ValueError):
                corpus.safe_source_file(view_root, "xsh", "linked/f.xsh")

    def test_lockstep_source_view_mutation_breaks_both_baseline_digests(self):
        WORK_TMP.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="frozen-witness-digest-", dir=WORK_TMP) as temporary:
            roots = {}
            expected = {}
            for name in ("original", "reduced-1040"):
                root = Path(temporary) / name
                roots[name] = root
                (root / "xsh").mkdir(parents=True)
                (root / "laputa").mkdir()
                (root / "xsh/core.xsh").write_bytes(b"pure f() {}\n")
                (root / "laputa/plan.xsh").write_bytes(b"pure plan() {}\n")
                expected[name] = corpus.source_view_digest(root)
                corpus.verify_view_digest(root, *expected[name])

            for root in roots.values():
                (root / "xsh/core.xsh").write_bytes(b"pure changed() {}\n")

            for name, root in roots.items():
                with self.assertRaises(ValueError):
                    corpus.verify_view_digest(root, *expected[name])


if __name__ == "__main__":
    unittest.main()
