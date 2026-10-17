from argparse import ArgumentParser, Namespace
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from unittest.mock import patch

bridge_path = Path(__file__).with_name("final_scheduler_receipt_bridge.py").resolve()
spec = spec_from_file_location("scheduler_receipt_bridge", bridge_path)
bridge = module_from_spec(spec)
spec.loader.exec_module(bridge)


def expect_value_error(action, message):
    try:
        action()
    except ValueError as error:
        assert message in str(error), str(error)
    else:
        raise AssertionError(f"expected ValueError containing {message!r}")


def test_controller_comparator_with_frozen_source(args):
    canonical_input_path = bridge.canonical_input_path
    close_path = args.functional_close
    missing_close = not close_path.exists()

    def resolve_test_input(path, label):
        if missing_close and label == "functional-close record":
            assert path == close_path and path.is_absolute()
            return path
        return canonical_input_path(path, label)

    with patch.object(bridge, "canonical_input_path", side_effect=resolve_test_input):
        bridge.configure(args)

    expected_comparator = bridge_path.parent / "winddown/gate-tools/native-scheduler-comparison/compare.py"
    assert bridge.COMPARE_PATH == expected_comparator, bridge.COMPARE_PATH
    assert bridge.ROOT == args.source_root
    assert bridge.REFERENCE_ROOT == args.reference_source_root
    assert bridge.FUNCTIONAL_CLOSE_PATH == args.functional_close
    assert bridge.BUILD_ROOT == args.source_root / f".work/consolidation/native-scheduler-builds/{args.commit}/{args.run_id}"
    assert bridge.RUNNER == args.source_root / "dev/consolidation/winddown/gate-tools/final-correctness-gates/run-native-final.sh"
    assert bridge.PATCH_PATH == args.source_root / "dev/consolidation/winddown/unfinished/475-native-scheduler-reference-2083821edb768d2485479c7386289b82e43b1c7c.patch"
    compare = bridge.load_comparator()
    assert Path(compare.__file__).resolve() == expected_comparator
    assert compare.ROOT == bridge_path.parents[2]
    source = compare.capture_source(bridge.ROOT, bridge.COMMIT)
    reference = bridge.validate_reference_inputs(compare, source)
    assert reference["base_head"] == args.commit
    assert source == compare.capture_source(bridge.ROOT, bridge.COMMIT)
    assert reference == compare.capture_reference_source(bridge.REFERENCE_ROOT, bridge.COMMIT)
    print(f"controller comparator: {expected_comparator}")
    print(f"controller comparator sha256: {bridge.sha256(expected_comparator)}")
    print(f"frozen source preserved: {bridge.ROOT} {args.commit} {args.tree_id}")
    print(f"reference source qualified: {bridge.REFERENCE_ROOT} {reference['changed_paths']}")

    expect_value_error(
        lambda: bridge.configure(Namespace(**(vars(args) | {"commit": "0" * 40}))),
        "HEAD differs from requested commit",
    )
    expect_value_error(
        lambda: bridge.configure(Namespace(**(vars(args) | {"tree_id": "0" * 40}))),
        "tree differs from requested tree",
    )
    print("wrong source HEAD and tree rejected")
    if missing_close:
        try:
            bridge.configure(args)
        except FileNotFoundError:
            pass
        else:
            raise AssertionError("production configure accepted an absent functional-close record")
        print("production configure rejects absent functional close")
        print("Test only: absent functional-close path resolution was stubbed; no close record was constructed or validated")
        print("Full bridge qualify remains pending a genuine functional-close record")
    else:
        bridge.configure(args)
        bridge.qualify_reference(bridge.load_comparator())
        print("Full bridge qualify passed against the genuine supplied functional-close record")


parser = ArgumentParser()
parser.add_argument("--source-root", type=Path, required=True)
parser.add_argument("--commit", required=True)
parser.add_argument("--tree-id", required=True)
parser.add_argument("--functional-close", type=Path, required=True)
parser.add_argument("--reference-source-root", type=Path, required=True)
parser.add_argument("--run-id", required=True)
test_controller_comparator_with_frozen_source(parser.parse_args())
print("scheduler bridge controller selection and frozen source qualification passed")
