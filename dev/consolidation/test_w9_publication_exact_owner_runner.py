import pathlib
import sys
import os
import hashlib
import tempfile
import unittest
from types import SimpleNamespace


sys.path.insert(0, str(pathlib.Path(__file__).parent))
import w9_publication_exact_owner_runner as exact_runner


SOURCE = "SourceId(0)"


def span(start, length):
    return f"Span {{ source_id: {SOURCE}, start: {start}, len: {length} }}"


def owner_index():
    rows = ["source\tSourceId(0)\t100"]
    expressions = (
        (1, 0, 50, "Record", ""),
        (2, 5, 5, "Ident", "first"),
        (3, 10, 5, "Ident", "alias"),
        (4, 50, 40, "Record", ""),
        (5, 55, 5, "Ident", "second"),
        (6, 60, 5, "Ident", "again"),
    )
    for expression_id, start, length, tag, name in expressions:
        current_span = span(start, length)
        rows.append(f"owner\t{current_span}\texpr:{tag}")
        rows.append(f"expr\tExprId({expression_id})\t{current_span}\t{tag}\t{name}")
    tail_span = span(90, 5)
    rows.extend(
        (
            f"stmt\tStmtId(1)\t{tail_span}\tstmt:TailBareIdent",
            f"tail\tStmtId(1)\t{tail_span}\tlast\tstmt:TailBareIdent",
            f"owner\t{tail_span}\tstmt:TailBareIdent",
            "exact-counts\t6\t4",
            f"record\tExprId(1)\t{span(0, 50)}\t2",
            f"record-field\tExprId(1)\t0\tnamed\tfirst\t{span(1, 4)}\t\tExprId(2)",
            f"record-field\tExprId(1)\t1\tshorthand\talias\t{span(10, 5)}\t\t",
            f"record-shorthand-alias\tExprId(1)\t1\tExprId(3)\t{span(10, 5)}\talias",
            f"record\tExprId(4)\t{span(50, 40)}\t2",
            f"record-field\tExprId(4)\t2\tnamed\tsecond\t{span(51, 4)}\t\tExprId(5)",
            f"record-field\tExprId(4)\t3\tshorthand\tagain\t{span(60, 5)}\t\t",
            f"record-shorthand-alias\tExprId(4)\t3\tExprId(6)\t{span(60, 5)}\tagain",
        )
    )
    return "\n".join(rows) + "\n"


def legacy_owner_index(contents):
    return "".join(
        line + "\n"
        for line in contents.splitlines()
        if line.split("\t", 1)[0] in {"source", "stmt", "tail", "owner"}
    )


class ExactOwnerIndexTests(unittest.TestCase):
    def test_parses_complete_global_ids_with_nonzero_record_field_ranges(self):
        legacy, exact = exact_runner.parse_exact_owner_index(owner_index())

        self.assertEqual(legacy[0], {0: 100})
        self.assertEqual(legacy[2]["StmtId(1)"], (
            "Span{source_id:SourceId(0),start:90,len:5}", "last", "stmt:TailBareIdent"
        ))
        self.assertEqual(exact["expr_count"], 6)
        self.assertEqual(exact["record_field_count"], 4)
        self.assertEqual(exact["exprs"][0], (
            "Span{source_id:SourceId(0),start:0,len:50}", "Record", ""
        ))
        self.assertEqual(exact["tails"][0], legacy[2]["StmtId(1)"])
        self.assertEqual(set(exact["records"][3]["fields"]), {2, 3})
        self.assertEqual(exact["aliases"][(3, 3)], (
            5, "Span{source_id:SourceId(0),start:60,len:5}", "again"
        ))

    def test_rejects_missing_expression_or_record_field_coverage(self):
        missing_expression = owner_index().replace(
            f"expr\tExprId(6)\t{span(60, 5)}\tIdent\tagain\n", ""
        )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "expression ids"):
            exact_runner.parse_exact_owner_index(missing_expression)
        missing_field = owner_index().replace(
            f"record-field\tExprId(4)\t3\tshorthand\tagain\t{span(60, 5)}\t\t\n", ""
        )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "field count"):
            exact_runner.parse_exact_owner_index(missing_field)

    def test_rejects_unparented_record_field_arena_slots(self):
        contents = owner_index().replace("exact-counts\t6\t4", "exact-counts\t6\t6")

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "do not cover the declared arena table"):
            exact_runner.parse_exact_owner_index(contents)

    def test_rejects_record_field_ids_outside_the_arena_table(self):
        contents = owner_index().replace("exact-counts\t6\t4", "exact-counts\t6\t3")

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "outside the declared arena table"):
            exact_runner.parse_exact_owner_index(contents)

    def test_rejects_malformed_expression_references_and_zero_based_debug_ids(self):
        dangling = owner_index().replace("\t\tExprId(2)\n", "\t\tExprId(99)\n")
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "missing expression"):
            exact_runner.parse_exact_owner_index(dangling)
        zero = owner_index().replace("ExprId(1)", "ExprId(0)", 1)
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "malformed"):
            exact_runner.parse_exact_owner_index(zero)

    def test_rejects_nonmatching_or_missing_shorthand_aliases(self):
        wrong_name = owner_index().replace(
            f"record-shorthand-alias\tExprId(1)\t1\tExprId(3)\t{span(10, 5)}\talias",
            f"record-shorthand-alias\tExprId(1)\t1\tExprId(3)\t{span(10, 5)}\tother",
        )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "disagrees"):
            exact_runner.parse_exact_owner_index(wrong_name)
        missing_alias = owner_index().replace(
            f"record-shorthand-alias\tExprId(4)\t3\tExprId(6)\t{span(60, 5)}\tagain\n", ""
        )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "aliases do not match"):
            exact_runner.parse_exact_owner_index(missing_alias)


class ExactOwnerHashOutputTests(unittest.TestCase):
    def test_accepts_one_exactly_prefixed_libtest_row_and_complete_hashes(self):
        rows = [f"w9exacthash\t{index}\t{'a' * 64}" for index in range(1690)]
        output = f"test {exact_runner.EXACT_OWNER_TEST} ... {rows[0]}\n" + "\n".join(rows[1:])

        hashes = exact_runner.parse_exact_owner_hash_output(output, 1690)

        self.assertEqual(len(hashes), 1690)
        self.assertEqual(hashes[0], "a" * 64)

    def test_rejects_duplicate_missing_or_unknown_prefixed_hash_rows(self):
        duplicate = "w9exacthash\t0\t" + "a" * 64
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "repeats"):
            exact_runner.parse_exact_owner_hash_output(duplicate + "\n" + duplicate, 1)
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "coverage differs"):
            exact_runner.parse_exact_owner_hash_output("", 1)
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "unknown prefix"):
            exact_runner.parse_exact_owner_hash_output(
                "test sema::different ... w9exacthash\t0\t" + "a" * 64, 1
            )


class ExactOwnerReceiptTests(unittest.TestCase):
    def setUp(self):
        self.reference = SimpleNamespace(
            inputs={0: SimpleNamespace(relative_path="src/a.xsh", blake3="a" * 64)}
        )
        self.receipt = (
            "corpus=1\n"
            "shard=0/1\n"
            "range=0..1\n"
            "shard_size=100\n"
            f"input_digest={'c' * 64}\n"
            f"owner\t0\tsrc/a.xsh\t{'a' * 64}\t{'b' * 64}\n"
            "complete=1\n"
        )

    def test_validates_exact_source_identity_and_range_coverage(self):
        hashes = exact_runner.parse_exact_owner_range_receipt(
            self.receipt, 0, 1, self.reference, "c" * 64
        )

        self.assertEqual(hashes, {0: "b" * 64})

    def test_rejects_stale_manifest_path_or_digest(self):
        stale_path = self.receipt.replace("src/a.xsh", "src/old.xsh", 1)
        stale_hash = self.receipt.replace("\t" + "a" * 64 + "\t", "\t" + "d" * 64 + "\t")

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "source differs"):
            exact_runner.parse_exact_owner_range_receipt(
                stale_path, 0, 1, self.reference, "c" * 64
            )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "source differs"):
            exact_runner.parse_exact_owner_range_receipt(
                stale_hash, 0, 1, self.reference, "c" * 64
            )

    def test_rejects_partial_range_coverage_and_stale_corpus_digest(self):
        incomplete = self.receipt.replace("complete=1\n", "complete=0\n")
        stale = self.receipt.replace("c" * 64, "e" * 64, 1)

        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "completion count"):
            exact_runner.parse_exact_owner_range_receipt(
                incomplete, 0, 1, self.reference, "c" * 64
            )
        with self.assertRaisesRegex(exact_runner.ExactOwnerError, "input_digest"):
            exact_runner.parse_exact_owner_range_receipt(
                stale, 0, 1, self.reference, "c" * 64
            )

    def test_rejects_malformed_or_incomplete_capture_ledgers(self):
        lines = [f"range\t0\t1\t{'a' * 64}", f"range\t1\t2\t{'b' * 64}"]
        rows, digest = exact_runner.parse_exact_owner_ledger(lines, [(0, 1), (1, 2)])
        canonical = "".join(f"range\t{start}\t{end}\t{hash_value}\n" for start, end, hash_value in rows)
        self.assertEqual(digest, hashlib.sha256(canonical.encode()).hexdigest())

        for invalid in (
            lines[:1],
            [lines[1], lines[0]],
            [lines[0], lines[1].replace("b" * 64, "invalid")],
        ):
            with self.subTest(invalid=invalid), self.assertRaises(exact_runner.ExactOwnerError):
                exact_runner.parse_exact_owner_ledger(invalid, [(0, 1), (1, 2)])


class ExactOwnerProjectionTests(unittest.TestCase):
    def test_returns_validated_legacy_and_exact_owners(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        work = root / ".work/tmp"
        work.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=work) as temporary:
            root_path = pathlib.Path(temporary)
            exact_root = root_path / "exact"
            legacy_root = root_path / "legacy"
            exact_root.mkdir()
            legacy_root.mkdir()
            (exact_root / "owners-0.tsv").write_text(owner_index(), encoding="utf-8")
            (legacy_root / "owners-0.tsv").write_text(
                legacy_owner_index(owner_index()), encoding="utf-8"
            )
            runner = SimpleNamespace(
                open_absolute_nofollow=lambda path, directory=False: os.open(
                    path, os.O_RDONLY | os.O_DIRECTORY
                )
            )

            validated = exact_runner._validate_owner_projections(
                exact_root, legacy_root, {0: object()}, {0: "b" * 64}, runner
            )

        self.assertEqual(validated[0][1]["expr_count"], 6)
        self.assertEqual(validated[0][1]["aliases"][(3, 3)][2], "again")

    def test_rejects_disagreement_with_original_semantic_owner_rows(self):
        root = pathlib.Path(__file__).resolve().parents[2]
        work = root / ".work/tmp"
        work.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=work) as temporary:
            root_path = pathlib.Path(temporary)
            exact_root = root_path / "exact"
            legacy_root = root_path / "legacy"
            exact_root.mkdir()
            legacy_root.mkdir()
            (exact_root / "owners-0.tsv").write_text(owner_index(), encoding="utf-8")
            changed_legacy = legacy_owner_index(owner_index()).replace(
                "\tlast\tstmt:TailBareIdent", "\tchanged\tstmt:TailBareIdent"
            )
            (legacy_root / "owners-0.tsv").write_text(changed_legacy, encoding="utf-8")
            runner = SimpleNamespace(
                open_absolute_nofollow=lambda path, directory=False: os.open(
                    path, os.O_RDONLY | os.O_DIRECTORY
                )
            )

            with self.assertRaisesRegex(exact_runner.ExactOwnerError, "rows differ"):
                exact_runner._validate_owner_projections(
                    exact_root, legacy_root, {0: object()}, {0: "b" * 64}, runner
                )


if __name__ == "__main__":
    unittest.main()
