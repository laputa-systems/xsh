import Xsh.Conformance.Vertical.SourceAdmissionBase
import Xsh.Conformance.Vertical.LoweringSound

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

/-- The source admission boundary composes with the existing universal lowerer theorem. The
artifact's entire function meaning is the meaning of the tree derived from these exact bytes. -/
theorem admitted_lowering_means_source {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))} {artifact : RawArtifact}
    (accepted : check bytes moduleInputs functions scopes = true)
    (lowered : lowersTo artifact functions = true) :
    moduleInputs = [] ∧ AdmittedSource (bytes.map UInt8.toNat) functions ∧
      ScopesProjection functions scopes ∧ functionPrograms artifact = sourcePrograms functions := by
  have admission := check_sound accepted
  exact ⟨admission.1, admission.2.2.1, admission.2.2.2, lowers_to_means_the_source lowered⟩

/-- A certified source/IR pair starts at the original code bytes: its grammar derivation is
independent of the compiler's tree, and its IR meaning follows from the universal lowerer proof.
Source-document decoding and extraction from a Rust parser remain separate boundaries. -/
theorem certified_codes_admitted {sourceId : Xsh.Conformance.Transport.BridgeId} {codes : List Nat}
    {artifact : RawArtifact} {results : List ObservedResult}
    (certified : certifyCodes sourceId codes artifact = some results) :
    ∃ functions, AdmittedSource codes functions ∧
      functionPrograms artifact = sourcePrograms functions ∧
      artifact.sourceId = sourceId ∧ validateArtifact artifact = true ∧
      namedResults artifact = some results := by
  unfold certifyCodes at certified
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at certified
  rcases certified with ⟨functions, parsed, certified⟩
  split at certified
  · rename_i checks
    simp only [Bool.and_eq_true, decide_eq_true_eq] at checks
    exact ⟨functions, parse_source_admitted parsed, lowers_to_means_the_source checks.1.2,
      checks.1.1, checks.2, certified⟩
  · cases certified

theorem admitted_bytes_are_ascii {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    ∀ byte, byte ∈ bytes → byte.toNat < 128 := by
  have admission := (check_sound accepted).2.2.1
  cases admission with
  | admitted tokens lexical productions earlier recordCalls =>
    intro byte member
    exact lexed_ascii lexical byte.toNat (List.mem_map.mpr ⟨byte, member, rfl⟩)

def arithmeticTree : List SourceFunction :=
  [⟨"f", ["x"], [], none, .value (.binary .add (.var "x") (.int 1))⟩]

def arithmeticBytes : List UInt8 := witnessBytes "pure f(x: Int) -> Int {\n  return x + 1\n}\n"

theorem arithmetic_bytes_admitted :
    check arithmeticBytes [] arithmeticTree [[("x", 0)]] = true := by
  decide +kernel

theorem assignment_bytes_admitted :
    check (witnessBytes "pure f() -> Int {\n  var x = 1\n  x = x + 2\n  return x\n}\n") []
      [⟨"f", [], [("x", 1)], none, .assigned "x" (.binary .add (.var "x") (.int 2))
        (.atom (.var "x"))⟩] [[("x", 1)]] = true := by
  decide +kernel

theorem computed_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let a = x + 1\n  let b = a * 2\n  return b\n}\n") []
      [⟨"f", ["x"], [], none,
        .computedLets [("a", .binary .add (.var "x") (.int 1)),
          ("b", .binary .mul (.var "a") (.int 2))] (.atom (.var "b"))⟩]
      [[("x", 0), ("a", 2), ("b", 3)]] = true := by
  decide +kernel

theorem boolean_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Bool {\n  return x > 1\n}\n") []
      [⟨"f", ["x"], [], none, .comparison .gt (.var "x") (.int 1)⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem result_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Result[Int, Int] {\n  return Err(x)\n}\n") []
      [⟨"f", ["x"], [], none, .constructed false (.atom (.var "x"))⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem propagation_bytes_admitted :
    check (witnessBytes ("pure g(x: Int) -> Result[Int, Int] {\n  return Ok(x)\n}\n" ++
        "pure f(x: Int) -> Result[Int, Int] {\n  let y = g(x)?\n  return Ok(y)\n}\n")) []
      [⟨"g", ["x"], [], none, .constructed true (.atom (.var "x"))⟩,
        ⟨"f", ["x"], [], some ⟨"y", "g", [.var "x"]⟩, .constructed true (.atom (.var "y"))⟩]
      [[("x", 0)], [("x", 0), ("y", 2)]] = true := by
  decide +kernel

theorem conditional_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return if x > 0 { x + 1 } else { 0 }\n}\n") []
      [⟨"f", ["x"], [], none, .conditional .gt (.var "x") (.int 0)
        (.binary .add (.var "x") (.int 1)) (.atom (.int 0))⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem call_bytes_admitted :
    check (witnessBytes ("pure g(x: Int) -> Int {\n  return x + 1\n}\n" ++
        "pure f(y: Int) -> Int {\n  return g(y)\n}\n")) []
      [⟨"g", ["x"], [], none, .value (.binary .add (.var "x") (.int 1))⟩,
        ⟨"f", ["y"], [], none, .call "g" [.var "y"]⟩] [[("x", 0)], [("y", 0)]] = true := by
  decide +kernel

theorem single_computed_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let y = x + 1\n  return y\n}\n") []
      [⟨"f", ["x"], [], none, .computedLet "y" (.binary .add (.var "x") (.int 1))
        (.atom (.var "y"))⟩] [[("x", 0), ("y", 2)]] = true := by
  decide +kernel

theorem admitted_byte_domain_nonempty :
    ∃ bytes functions scopes, bytes ≠ [] ∧ functions ≠ [] ∧ check bytes [] functions scopes = true := by
  exact ⟨arithmeticBytes, arithmeticTree, [[("x", 0)]], by decide +kernel, by decide +kernel,
    arithmetic_bytes_admitted⟩

theorem substituted_source_and_projection_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return x - 1\n}\n") []
      arithmeticTree [[("x", 0)]] = false ∧
    check arithmeticBytes [] arithmeticTree [[("x", 1)]] = false ∧
    check arithmeticBytes [] arithmeticTree [[("y", 0)]] = false ∧
    check arithmeticBytes [] arithmeticTree [] = false ∧
    check arithmeticBytes [arithmeticBytes] arithmeticTree [[("x", 0)]] = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem unsupported_trailing_bytes_refused :
    check (arithmeticBytes ++ [0]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ [64]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ [195, 169]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ witnessBytes "pure") [] arithmeticTree [[("x", 0)]] = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end SourceAdmission
end Xsh.Conformance.Vertical
