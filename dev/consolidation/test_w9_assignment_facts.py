from __future__ import annotations

import hashlib
import unittest

from w9_assignment_facts import (
    AssignmentFactsError,
    OwnerKey,
    ProofBinding,
    parse_assignment_proof,
    parse_candidate_assignment_facts,
    validate_assignment_facts,
)


COMMIT = "1" * 40
TREE = "2" * 40
HELPER = "3" * 64
CORPUS = "4" * 64
INPUT_PATH = "bench/consolidation/xsh/directory.xsh"
SOURCE_PATH = "/capture/bench/consolidation/xsh/directory.xsh"
BINDING = ProofBinding(COMMIT, TREE, HELPER, CORPUS, 0, INPUT_PATH)
OWNER = OwnerKey(0, 32, 1)
STATEMENT_NODE_COUNT = 12
CANDIDATE_EMPTY = (
    "AssignmentFacts{owners:{AssignmentFactOwner{source_id:SourceId(0),"
    "statement_start:32,statement_len:1}:{}}}"
)


def _hex(value: str) -> str:
    return value.encode("utf-8").hex()


def _candidate(names: tuple[str, ...]) -> str:
    rendered_names = ",".join(f'Name("{name}")' for name in names)
    return (
        "AssignmentFacts{owners:{AssignmentFactOwner{source_id:SourceId(0),"
        "statement_start:32,statement_len:1}:{"
        + rendered_names
        + "}}}"
    )


def _proof(names: tuple[str, ...] = ()) -> str:
    encoded_names = ",".join(_hex(name) for name in sorted(names))
    root_identity = "root\0" + SOURCE_PATH
    return (
        "w9-assignment-facts\t1\n"
        f"binding\t{COMMIT}\t{TREE}\t{HELPER}\t{CORPUS}\t0\t{_hex(INPUT_PATH)}\n"
        f"source\t0\t{_hex(SOURCE_PATH)}\t100\n"
        "counts\t1\t1\t0\n"
        f"owner\t0\t32\t1\t{_hex(root_identity)}\t{encoded_names}\n"
    )


def _module_proof() -> tuple[str, dict[OwnerKey, frozenset[str]]]:
    module_path = "/capture/modules/child.xsh"
    module_identity = (
        "module\0"
        + module_path
        + "\0child-key\0child\0"
        + "1"
    )
    root_identity = "root\0" + SOURCE_PATH
    return (
        "w9-assignment-facts\t1\n"
        f"binding\t{COMMIT}\t{TREE}\t{HELPER}\t{CORPUS}\t0\t{_hex(INPUT_PATH)}\n"
        f"source\t0\t{_hex(SOURCE_PATH)}\t100\n"
        f"source\t1\t{_hex(module_path)}\t0\n"
        "counts\t2\t1\t1\n"
        f"owner\t0\t32\t1\t{_hex(root_identity)}\t\n"
        f"owner\t1\t40\t0\t{_hex(module_identity)}\t\n",
        {OWNER: frozenset(), OwnerKey(1, 40, 0): frozenset()},
    )


def _parse(contents: str, binding: ProofBinding = BINDING):
    digest = hashlib.sha256(contents.encode("ascii")).hexdigest()
    return parse_assignment_proof(contents, binding, digest)


class AssignmentFactsProofTests(unittest.TestCase):
    def test_owner_range_is_not_bounded_by_statement_node_count(self):
        self.assertGreater(OWNER.statement_start, STATEMENT_NODE_COUNT)
        proof = _parse(_proof())
        self.assertEqual(validate_assignment_facts(CANDIDATE_EMPTY, proof), {OWNER: frozenset()})

    def test_input_index_requires_an_exact_nonnegative_integer(self):
        for value in (True, 1.0, -1):
            with self.subTest(value=value):
                binding = ProofBinding(COMMIT, TREE, HELPER, CORPUS, value, INPUT_PATH)
                contents = _proof()
                digest = hashlib.sha256(contents.encode("ascii")).hexdigest()
                with self.assertRaisesRegex(AssignmentFactsError, "input index"):
                    parse_assignment_proof(contents, binding, digest)

    def test_candidate_owner_coordinates_require_exact_integers(self):
        proof = _parse(_proof())
        for field, original in enumerate((OWNER.source_id, OWNER.statement_start, OWNER.statement_len)):
            for value in (True, float(original)):
                with self.subTest(field=field, value=value):
                    coordinates = [OWNER.source_id, OWNER.statement_start, OWNER.statement_len]
                    coordinates[field] = value
                    malformed = OwnerKey(*coordinates)
                    with self.assertRaisesRegex(
                        AssignmentFactsError, "noncanonical unsigned integer"
                    ):
                        validate_assignment_facts({malformed: frozenset()}, proof)

    def test_diagnosed_empty_owner_validates_in_all_publication_profiles(self):
        proof = _parse(_proof())
        for profile in ("default", "reveal", "execution", "check-cli"):
            with self.subTest(profile=profile):
                self.assertEqual(
                    validate_assignment_facts(CANDIDATE_EMPTY, proof),
                    {OWNER: frozenset()},
                )

    def test_candidate_parser_reads_empty_owner(self):
        self.assertEqual(parse_candidate_assignment_facts(CANDIDATE_EMPTY), {OWNER: frozenset()})

    def test_nonempty_legacy_name_set_validates(self):
        proof = _parse(_proof(("kept",)))
        self.assertEqual(
            validate_assignment_facts(_candidate(("kept",)), proof),
            {OWNER: frozenset({"kept"})},
        )

    def test_empty_module_owner_is_explicit(self):
        contents, expected = _module_proof()
        proof = _parse(contents)
        candidate = (
            "AssignmentFacts{owners:{"
            "AssignmentFactOwner{source_id:SourceId(0),statement_start:32,statement_len:1}:{},"
            "AssignmentFactOwner{source_id:SourceId(1),statement_start:40,statement_len:0}:{}}}"
        )
        self.assertEqual(validate_assignment_facts(candidate, proof), expected)
        with self.assertRaisesRegex(AssignmentFactsError, "owners differ"):
            validate_assignment_facts(CANDIDATE_EMPTY, proof)

    def test_added_candidate_name_fails(self):
        proof = _parse(_proof())
        candidate = _candidate(("introduced",))
        with self.assertRaisesRegex(AssignmentFactsError, "names differ"):
            validate_assignment_facts(candidate, proof)

    def test_removed_candidate_name_fails(self):
        proof = _parse(_proof(("kept",)))
        candidate = CANDIDATE_EMPTY
        with self.assertRaisesRegex(AssignmentFactsError, "names differ"):
            validate_assignment_facts(candidate, proof)

    def test_changed_candidate_source_id_fails(self):
        proof = _parse(_proof())
        candidate = CANDIDATE_EMPTY.replace("SourceId(0)", "SourceId(1)")
        with self.assertRaisesRegex(AssignmentFactsError, "owners differ"):
            validate_assignment_facts(candidate, proof)

    def test_changed_candidate_range_fails(self):
        proof = _parse(_proof())
        candidate = CANDIDATE_EMPTY.replace("statement_start:32", "statement_start:33")
        with self.assertRaisesRegex(AssignmentFactsError, "owners differ"):
            validate_assignment_facts(candidate, proof)

    def test_missing_empty_candidate_owner_fails(self):
        proof = _parse(_proof())
        candidate = "AssignmentFacts{owners:{}}"
        with self.assertRaisesRegex(AssignmentFactsError, "owners differ"):
            validate_assignment_facts(candidate, proof)

    def test_changed_proof_bytes_fail_capture_hash(self):
        contents = _proof()
        changed = contents.replace("\t32\t1\t", "\t33\t1\t")
        digest = hashlib.sha256(contents.encode("ascii")).hexdigest()
        with self.assertRaisesRegex(AssignmentFactsError, "capture receipt"):
            parse_assignment_proof(changed, BINDING, digest)

    def test_changed_oracle_name_with_new_receipt_fails_candidate_compare(self):
        proof = _parse(_proof(("introduced",)))
        with self.assertRaisesRegex(AssignmentFactsError, "names differ"):
            validate_assignment_facts(CANDIDATE_EMPTY, proof)

    def test_changed_oracle_range_with_new_receipt_fails_candidate_compare(self):
        contents = _proof().replace("\t0\t32\t1\t", "\t0\t33\t1\t")
        proof = _parse(contents)
        with self.assertRaisesRegex(AssignmentFactsError, "owners differ"):
            validate_assignment_facts(CANDIDATE_EMPTY, proof)

    def test_removed_proof_owner_fails_declared_count(self):
        contents = _proof()
        changed = "\n".join(contents.splitlines()[:-1]) + "\n"
        with self.assertRaisesRegex(AssignmentFactsError, "owner rows"):
            _parse(changed)

    def test_changed_source_binding_fails(self):
        contents = _proof().replace(COMMIT, "5" * 40)
        with self.assertRaisesRegex(AssignmentFactsError, "binding differs"):
            _parse(contents)

    def test_changed_helper_or_corpus_binding_fails(self):
        for old, new in ((HELPER, "5" * 64), (CORPUS, "6" * 64)):
            with self.subTest(binding=old[:4]):
                contents = _proof().replace(old, new)
                with self.assertRaisesRegex(AssignmentFactsError, "binding differs"):
                    _parse(contents)

    def test_unknown_schema_and_rows_fail(self):
        contents = _proof()
        for changed in (
            contents.replace("w9-assignment-facts\t1", "w9-assignment-facts\t2"),
            contents.replace("counts\t1\t1\t0", "counts\t1\t1\t0\nunknown\trow"),
        ):
            with self.subTest(changed=changed.splitlines()[0]):
                with self.assertRaises(AssignmentFactsError):
                    _parse(changed)


if __name__ == "__main__":
    unittest.main()
