import Xsh.Conformance.Vertical.Facts

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

def factsSchema : String := "xsh-checked-facts-v1"

def factsScope : String := "integer-calls-conditional-facts-v1"

/-- The checked facts the Rust checker published for the functions of one source document, as the
closed v1 facts document carries them. -/
structure FactsArtifact where
  sourceId : BridgeId
  functions : List FunctionFacts
  deriving DecidableEq

def FactType.ofName? : String → Option FactType
  | "Int" => some .int
  | "Bool" => some .bool
  | _ => none

section Decoding

private def textField (fields : List (String × WireValue)) (name : String) : Option String :=
  (WireValue.field? fields name).bind WireValue.text?

private def wordField (fields : List (String × WireValue)) (name : String) : Option Word32 :=
  (textField fields name).bind decodeHex32?

private def wideField (fields : List (String × WireValue)) (name : String) : Option Word64 :=
  (textField fields name).bind decodeHex64?

private def listField (fields : List (String × WireValue)) (name : String)
    (decode : WireValue → Option α) : Option (List α) :=
  ((WireValue.field? fields name).bind WireValue.items?).bind fun items => items.mapM decode

def decodeFactType (value : WireValue) : Option FactType :=
  value.text?.bind FactType.ofName?

/-- A node is accepted only as an object with exactly the fields of its kind. An operation carries
its own ordinal, a read one of the two resolutions it can name. -/
def decodeNodeFact (value : WireValue) : Option NodeFact :=
  match value with
  | .object fields =>
      match textField fields "kind" with
      | some "literal" => do
          let fields ← value.closedObject? ["kind", "value"]
          pure (.literal (← wideField fields "value").val)
      | some "read" => do
          let fields ← value.closedObject? ["kind", "resolution", "index"]
          let index := (← wordField fields "index").val
          match textField fields "resolution" with
          | some "parameter" => pure (.read (.parameter index))
          | some "letBinding" => pure (.read (.letBinding index))
          | _ => none
      | some "operate" => do
          let fields ← value.closedObject? ["kind", "op", "ordinal", "operands", "result"]
          let op ← (textField fields "op").bind FragmentOp.ofName?
          if (← wordField fields "ordinal") == op.word then pure () else none
          pure (.operate op (← (textField fields "operands").bind FactType.ofName?)
            (← (textField fields "result").bind FactType.ofName?))
      | some "call" => do
          let fields ← value.closedObject? ["kind", "callee", "arguments"]
          pure (.call (← wordField fields "callee").val (← wordField fields "arguments").val)
      | some "branch" => do
          let _ ← value.closedObject? ["kind"]
          pure .branch
      | _ => none
  | _ => none

def decodeFunctionFacts (value : WireValue) : Option FunctionFacts := do
  let fields ← value.closedObject?
    ["name", "parameterTypes", "localTypes", "resultType", "effects", "nodes"]
  pure ⟨← textField fields "name", ← listField fields "parameterTypes" decodeFactType,
    ← listField fields "localTypes" decodeFactType,
    ← (textField fields "resultType").bind FactType.ofName?,
    ← listField fields "effects" WireValue.text?, ← listField fields "nodes" decodeNodeFact⟩

def decodeFactsDocument (text : ExchangeCodes) : Option FactsArtifact := do
  let value ← parseCodes text
  let fields ← value.closedObject? ["schema", "sourceId", "scope", "functions"]
  if textField fields "schema" == some factsSchema then pure () else none
  if textField fields "scope" == some factsScope then pure () else none
  pure ⟨← (textField fields "sourceId").bind BridgeId.fromHex?,
    ← listField fields "functions" decodeFunctionFacts⟩

end Decoding

/-- The CERT-FACTS verdict on exchanged bytes: the source document decodes, its text parses in the
admitted productions, the facts document decodes, names that source document, and lists exactly
the facts the rules determine for the parsed functions. Nothing is read from the candidate except
the facts it lists. -/
def certifiesFacts (sourceCodes factsCodes : ExchangeCodes) : Bool :=
  match decodeSourceDocument sourceCodes, decodeFactsDocument factsCodes with
  | some source, some facts =>
      facts.sourceId == source.id &&
        match parseSource source.text with
        | some functions => validateFacts functions facts.functions
        | none => false
  | _, _ => false

/-- An accepted pair is a valid derivation: both documents decode, the facts document names the
source document, the source parses, and the listed facts are a valid derivation of the rules for
the parsed functions. -/
theorem certifies_facts_sound {sourceCodes factsCodes : ExchangeCodes}
    (accepted : certifiesFacts sourceCodes factsCodes = true) :
    ∃ source facts functions, decodeSourceDocument sourceCodes = some source ∧
      decodeFactsDocument factsCodes = some facts ∧ facts.sourceId = source.id ∧
      parseSource source.text = some functions ∧ ProgramHolds functions facts.functions := by
  unfold certifiesFacts at accepted
  cases hs : decodeSourceDocument sourceCodes with
  | none => simp [hs] at accepted
  | some source =>
    cases hf : decodeFactsDocument factsCodes with
    | none => simp [hs, hf] at accepted
    | some facts =>
      simp only [hs, hf, Bool.and_eq_true, beq_iff_eq] at accepted
      obtain ⟨sameId, parsed⟩ := accepted
      cases hp : parseSource source.text with
      | none => simp [hp] at parsed
      | some functions =>
        simp only [hp] at parsed
        exact ⟨source, facts, functions, rfl, rfl, sameId, hp, validate_facts_iff.mp parsed⟩

theorem empty_documents_certify_nothing :
    certifiesFacts [] [] = false := by
  decide

theorem a_facts_document_with_an_unknown_node_kind_is_rejected :
    (decodeNodeFact (.object [("kind", .string "write")])).isNone = true ∧
      (decodeNodeFact (.object [("kind", .string "branch"), ("extra", .string "")])).isNone =
        true ∧
      (decodeNodeFact (.string "branch")).isNone = true := by
  refine ⟨?_, ?_, ?_⟩ <;> decide

theorem a_read_decodes_only_as_a_parameter_or_a_let :
    (decodeNodeFact (.object [("kind", .string "read"), ("resolution", .string "parameter"),
      ("index", .string "00000001")])) = some (.read (.parameter 1)) ∧
    (decodeNodeFact (.object [("kind", .string "read"), ("resolution", .string "letBinding"),
      ("index", .string "00000000")])) = some (.read (.letBinding 0)) ∧
    (decodeNodeFact (.object [("kind", .string "read"), ("resolution", .string "global"),
      ("index", .string "00000000")])).isNone = true := by
  refine ⟨?_, ?_, ?_⟩ <;> decide

theorem an_operation_decodes_only_with_its_own_ordinal :
    (decodeNodeFact (.object [("kind", .string "operate"), ("op", .string "Gt"),
      ("ordinal", .string "00000007"), ("operands", .string "Int"),
      ("result", .string "Bool")])) = some (.operate .gt .int .bool) ∧
    (decodeNodeFact (.object [("kind", .string "operate"), ("op", .string "Gt"),
      ("ordinal", .string "0000000b"), ("operands", .string "Int"),
      ("result", .string "Bool")])).isNone = true ∧
    (decodeNodeFact (.object [("kind", .string "operate"), ("op", .string "Gt"),
      ("ordinal", .string "00000007"), ("operands", .string "Str"),
      ("result", .string "Bool")])).isNone = true := by
  refine ⟨?_, ?_, ?_⟩ <;> decide

end Xsh.Conformance.Vertical
