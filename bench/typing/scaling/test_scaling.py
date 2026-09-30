"""Source closure invariants, independent of the compiler's inference result."""

import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("typing_scaling", Path(__file__).parents[1] / "scaling.py")
scaling = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(scaling)


class ScalingSourcesTest(unittest.TestCase):
    def test_every_family_is_deterministic(self):
        for family in scaling.FAMILIES + scaling.ADVERSARIAL:
            with self.subTest(family=family):
                first = scaling.generate(family, 7, "inferred")
                second = scaling.generate(family, 7, "inferred")
                self.assertEqual(first, second)
                self.assertEqual(scaling.source_index(first), scaling.source_index(second))

    def test_immutable_aliases_form_a_single_source_ordered_chain(self):
        source = scaling.generate("immutable_aliases", 8, "inferred")["entry.xsh"].decode()
        self.assertEqual(source.splitlines()[1], "let alias_0 = anchor")
        for i in range(1, 8):
            self.assertIn(f"let alias_{i} = alias_{i - 1}\n", source)

    def test_module_diamonds_have_real_canonical_shared_dependencies(self):
        units = 7
        files = scaling.generate("module_diamonds", units, "inferred")
        imports = []
        for path, data in files.items():
            for module in re.findall(rb"^use ([A-Za-z_0-9.]+) as ", data, re.MULTILINE):
                target = module.decode().replace(".", "/") + ".xsh"
                self.assertIn(target, files)
                self.assertTrue(target.startswith("typing_scaling/"))
                imports.append((path, target))
        self.assertEqual(len(files), 3 * units + 1)
        self.assertEqual(len(imports), 4 * units)
        for i in range(units):
            leaf = f"typing_scaling/d{i:06d}/leaf.xsh"
            predecessors = {path for path, target in imports if target == leaf}
            self.assertEqual(predecessors, {leaf.replace("leaf", "left"), leaf.replace("leaf", "right")})

    def test_record_width_and_preserved_update_count_match_units(self):
        source = scaling.generate("wide_records", 8, "inferred")["entry.xsh"].decode()
        self.assertEqual(len(re.findall(r"field_\d+: ", source.splitlines()[0])), 8)
        self.assertEqual(len(re.findall(r"^let updated_", source, re.MULTILINE)), 8)
        self.assertEqual(len(re.findall(r"^let projected_", source, re.MULTILINE)), 8)
        for i in range(8):
            self.assertIn(f"updated_{i}.field_{i}", source)

    def test_inferred_expansion_is_shared_in_source_instead_of_unfolded(self):
        source = scaling.generate("inferred_expansion", 128, "inferred")["entry.xsh"].decode()
        self.assertEqual(source.count("left: previous, right: previous"), 128)
        self.assertLess(len(source), 20000)
        self.assertIn("let invalid: Int = expansion_128(0)", source)

    def test_wrong_nearly_matching_calls_keep_valid_arity_and_labels(self):
        source = scaling.generate("invalid_nearly_matching", 7, "inferred")["entry.xsh"].decode()
        self.assertEqual(source.count('.set(value: "wrong", key: '), 7)
        self.assertEqual(source.count(": Map[Int] = map.empty()"), 7)

    def test_frozen_manifest_keeps_every_required_size_and_gate(self):
        manifest = json.loads(scaling.MANIFEST.read_text())
        regular = [case for case in manifest["cases"] if case["kind"] == "regular"]
        self.assertEqual(len(regular), 56)
        for family in scaling.FAMILIES:
            for variant in ("annotated", "inferred"):
                self.assertEqual({case["units"] for case in regular if case["family"] == family and case["variant"] == variant}, {1000, 2000, 4000, 8000})
        gates = manifest["scaling_gates"]
        self.assertEqual(gates["doublings"], [[2000, 4000], [4000, 8000]])
        self.assertEqual(gates["maximum_total_and_family_work_ratio"], 2.6)
        self.assertEqual(gates["maximum_retained_type_and_constraint_bytes_ratio"], 2.5)
        self.assertEqual(gates["resource_guard_hit"], "failed")

    def test_materialized_closure_matches_frozen_file_hashes_and_refuses_reuse(self):
        with tempfile.TemporaryDirectory() as parent:
            root = Path(parent) / "closure"
            command = [sys.executable, str(Path(scaling.__file__)), "generate", "--case", "immutable_aliases-1000-inferred", "--output", str(root)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            index = json.loads((root / "sources.json").read_text())
            for item in index:
                self.assertEqual(scaling.sha((root / item["path"]).read_bytes()), item["sha256"])
            again = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(again.returncode, 0)
            self.assertIn("output must be a new directory", again.stderr)

    def test_seed_controls_values_without_changing_relationships(self):
        first = scaling.generate("wide_records", 8, "inferred", 1)["entry.xsh"].decode()
        second = scaling.generate("wide_records", 8, "inferred", 2)["entry.xsh"].decode()
        self.assertNotEqual(first, second)
        self.assertEqual(re.sub(r": \d+", ": VALUE", first), re.sub(r": \d+", ": VALUE", second))


if __name__ == "__main__":
    unittest.main()
