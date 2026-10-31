# Equality saturation feasibility

A non-shipping experiment is viable for a closed, total Boolean fragment. It is not yet a case for an optimizer or a dependency. First compare a deterministic local rewriter with a bounded acyclic e-graph on the same fragment and inputs. The comparison must preserve XSH behavior and show useful alternatives that local rules cannot already select. Performance work is deferred.

## Boundary

The [indexed schema](../../src/runtime/eval/indexed/full/schema.rs) contains specialized Boolean rows for literals, slots, `Not`, `And`, and `Or`, as well as generic expression rows. The specialized integer rows include checked arithmetic. Generic binary rows carry a span. The [explicit evaluator](../../src/runtime/eval/lowered_run/indexed_run/explicit_run.rs) evaluates the left binary operand first and conditionally skips the right operand of `and` or `or`. [Integer operations](../../src/runtime/eval/lowered_ops.rs) report overflow and division by zero at the operator span.

For an initial experiment, admit only a tree of Boolean literals, reads of already initialized immutable Boolean slots in the same frame, `!`, `and`, and `or`. Every node and child must be within this grammar. Slot identities and types must come from the checked indexed program, with a verified binding that is available at the expression. The experiment must reject anything whose binding, type, or availability it cannot establish. It must not treat a call, comparison, arithmetic operation, conversion, mutable or captured read, or an unknown producer as a Boolean atom. It must not move a tree across a statement, scope, or typed boundary. The fragment assumes normal execution with sufficient evaluator resources; host resource exhaustion and implementation depth limits need separate consideration before shipping.

For this fragment, candidates include `!!x → x`, `x and true → x`, `x or false → x`, `x and false → false`, `x or true → true`, and commutation of `and` or `or`. These rules may discard or reorder slot reads only because every admitted read is stable, effect-free, and non-failing. Short-circuiting makes the same rules unsafe for arbitrary children. Preserve the result type and binding identity, and retain source metadata needed by the surrounding program. A rule must be checked against actual lowered semantics, not just Boolean algebra.

`UInt` requires particular care. Its arithmetic shares the signed representation, while a negative value fails at a typed storage or call boundary. An expression's static type alone does not prove the intermediate value satisfies that boundary. The checker does not track arithmetic ranges. See [numbers and ranges in the specification](../SPEC.md#42-numbers).

## Observations

The following bounded probes ran with the existing `target/verification/xsh` and `target/verification/xsht` binaries from the parent checkout. They are observations of those binaries, not a fresh build or universal proof. The table summarizes the scripts. The commands below reproduce their exact source and run them, including the trace of the final script.

| Script | Result |
| --- | --- |
| `let x = 9223372036854775807 + 1 + -1` | Exit 3, `integer-overflow`, span at the first addition. |
| `let x = 9223372036854775807 + (1 + -1)` then `print $x` | Exit 0, prints `9223372036854775807`. |
| `let n: UInt = 1` then `let x = n - 2` then `print $x` | Exit 0, prints `-1`. |
| `let n: UInt = 1` then `let m: UInt = n - 2` | Exit 3, `type-error`, span at `n - 2`. |
| `print (1 / 0) + (9223372036854775807 + 1)` | Exit 3, `division-by-zero` at the left operand. Reversing operands reports `integer-overflow` at the left operand. |
| `let a = false and 1 / 0 == 0` then `let b = true or 1 / 0 == 0` then `print $a $b` | Exit 0, prints `false true`. |
| A `proc mark(label: Str, value: Int)` that prints its label and returns its value, called as `mark("left", 1) + mark("right", 2)` | Exit 0, prints `left`, `right`, then `3`, in that order. |
| `pure fail() -> Int {` newline `  1 / 0` newline `}` newline `print fail() + 1` | Exit 3, `division-by-zero` at line 2, columns 3–8. The trace includes `pure.enter`, `pure.exit`, and `runtime.error`; the error event has the failing expression span and the call event has the call-site span. |

The following commands replay the observations from the repository root. Set `BIN` to the verification binaries you intend to inspect. The recorded probes used the parent checkout's existing binaries. `.work/` must already be ignored.

```sh
mkdir -p .work/tmp/egraph-feasibility
python3 - <<'PYPROBES'
from pathlib import Path
p = Path('.work/tmp/egraph-feasibility')
cases = {
    'overflow': 'let x = 9223372036854775807 + 1 + -1\nprint $x\n',
    'reassociate': 'let x = 9223372036854775807 + (1 + -1)\nprint $x\n',
    'uint_expr': 'let n: UInt = 1\nlet x = n - 2\nprint $x\n',
    'uint': 'let n: UInt = 1\nlet m: UInt = n - 2\nprint $m\n',
    'order': 'print (1 / 0) + (9223372036854775807 + 1)\n',
    'order_reverse': 'print (9223372036854775807 + 1) + (1 / 0)\n',
    'short': 'let a = false and 1 / 0 == 0\nlet b = true or 1 / 0 == 0\nprint $a $b\n',
    'effects': 'proc mark(label: Str, value: Int) -> Int {\n  print $label\n  return value\n}\nlet x = mark("left", 1) + mark("right", 2)\nprint $x\n',
    'span': 'pure fail() -> Int {\n  1 / 0\n}\nprint fail() + 1\n',
}
for name, source in cases.items():
    (p / (name + '.xsh')).write_text(source)
PYPROBES
BIN=${BIN:-"$PWD/target/verification"}
for name in overflow reassociate uint_expr uint order order_reverse short effects span; do
  "$BIN/xsh" ".work/tmp/egraph-feasibility/$name.xsh"
  printf '%s status=%s\n' "$name" "$?"
done
"$BIN/xsht" trace --raw --trace-format jsonl .work/tmp/egraph-feasibility/span.xsh
```

The following probe evaluates every listed Boolean rule for all four assignments to `a` and `b`. It printed `true true true true` and exited 0. This is a finite evaluator check, not a proof of admission or of arbitrary trees.

```xsh
pure check(a: Bool, b: Bool) -> Bool {
  let r1 = !!a == a
  let r2 = (a and true) == a
  let r3 = (a or false) == a
  let r4 = (a and false) == false
  let r5 = (a or true) == true
  let r6 = (a and b) == (b and a)
  let r7 = (a or b) == (b or a)
  r1 and r2 and r3 and r4 and r5 and r6 and r7
}
print ${check(false, false)} ${check(false, true)} ${check(true, false)} ${check(true, true)}
```

Save that block as `.work/tmp/egraph-feasibility/boolean-rules.xsh` and run `"$BIN/xsh" .work/tmp/egraph-feasibility/boolean-rules.xsh`. To inspect a subtree-removing rule, trace these two scripts with `"$BIN/xsht" trace --raw --trace-format jsonl SCRIPT`:

```xsh
let a: Bool = true
let x = a and false
print $x
```

```xsh
let a: Bool = true
let x = false
print $x
```

Both print `false` and exit 0. Both traces contain `script.enter`, `core.call`, `core.result`, and `script.exit` in that order, with matching event relationships and payloads. File names, timestamps, durations, and later source byte offsets differ because these are separate source files of different lengths. This observation does not prove span preservation by an IR rewrite. The [typed evaluator](../../src/runtime/eval/lowered_run/indexed_run/explicit_run.rs) schedules the admitted Boolean operations without node-level trace emission; its [scalar evaluator](../../src/runtime/eval/lowered_run/indexed_run/values.rs) handles literals and slot reads. A future implementation must also check every execution route it admits.

The trace and traceback contracts include source spans and call events even for pure functions. See [runtime errors and traces](../SPEC.md#16-runtime-errors-and-traces). Purity is therefore insufficient to justify eliminating a call. The probe also shows why failure order and location are observable.

## Alternatives and obligations

A deterministic local rewriter applies a fixed, terminating rule order and can retain the original tree when no rule improves its chosen cost. It is the smaller baseline for this fragment. It cannot retain several equivalent intermediate forms for a later choice without extra machinery.

A bounded acyclic e-graph can retain alternatives and extract one tree under a deterministic cost and tie-break. Admission, node count, rule iterations, extraction, and failure behavior need explicit bounds. Reaching a bound must return the original tree or another already established equivalent tree. Cycles and recursive extraction must be rejected or handled with a proven terminating algorithm. E-class equality must include type and binding identity; a union is valid only when both expressions are equivalent for every admitted environment. Optimistic analysis facts must never authorize irreversible unions. The [PLDI 2026 analysis of optimistic equality saturation](https://www.russelarbore.com/PLDI2026_OptimismInEqsat_CameraReady.pdf) describes this distinction.

Before any shipping proposal, establish admission soundness and totality, each rewrite's value equivalence under every admitted environment, extraction equivalence, deterministic bounded termination, and preservation of types, bindings, surrounding evaluation, spans, and traces. Include negative tests for excluded failing and effectful children and differential tests against the evaluator. Define a resource and diagnostic policy for transformed trees. The existing `FullVerifier` checks indexed program invariants; passing it does not prove two programs semantically equivalent. The [formal project](../../formal/README.md) proves properties of selected Lean models and does not establish their correspondence to the Rust runtime.

The Boolean fragment establishes a semantic test boundary, not a demonstrated optimization opportunity. Before building an e-graph experiment, identify a concrete pattern in real indexed IR where interacting rewrites need alternative retention. Compare that case with a deterministic local rewriter. The present evidence does not justify general arithmetic saturation, production integration, or an e-graph dependency. A [Cranelift evaluation](https://cfallin.org/blog/2026/04/09/aegraph/) attributes its larger reported gain to a broader pipeline change. Its alternative-retention ablation is much smaller and does not establish a benefit for XSH.
