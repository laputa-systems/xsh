import Xsh.Conformance.Connected.RawStoreDerived
import Xsh.Conformance.Connected.RawLocalDocuments
import Xsh.Conformance.Connected.RawStoreFixture

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Semantics.Connected Xsh.Conformance.Transport

def emptyRawRows : RawInstructionRows := ⟨[], [], [], [], [], [], [], [], [], [], []⟩

def exportedRawRows (codes : List Nat) : RawInstructionRows :=
  ((parseCodes codes).bind decodeRawInstructionRows).getD emptyRawRows

def exportedRawFunctions (codes : List Nat) : RawFunctionRows :=
  ((parseCodes codes).bind decodeRawFunctionRows).getD ⟨[], []⟩

def exportedRawLocals (codes : List Nat) : RawLocalDeclarations :=
  ((parseCodes codes).bind decodeRawLocalDeclarations).getD ⟨0, [], []⟩

set_option maxRecDepth 100000 in
theorem exported_raw_documents_decode :
    ((parseCodes scopedRawInstructionCodes).bind decodeRawInstructionRows).isSome = true ∧
    ((parseCodes scopedRawFunctionCodes).bind decodeRawFunctionRows).isSome = true ∧
    ((parseCodes scopedLocalDeclarationsCodes).bind decodeRawLocalDeclarations).isSome = true ∧
    ((parseCodes pairRawInstructionCodes).bind decodeRawInstructionRows).isSome = true ∧
    ((parseCodes pairRawFunctionCodes).bind decodeRawFunctionRows).isSome = true ∧
    ((parseCodes pairLocalDeclarationsCodes).bind decodeRawLocalDeclarations).isSome = true := by
  decide +kernel

def scopedOccurrencesAndPrincipal : Option (Substitution × List ScopedOccurrence) :=
  match actualScopedCheckDecoded scopedUnit scopedDocument scopedScopes,
      sourceScopedOccurrences 128 scopedFunction scopedFixtureParameters [] [] scopedFixtureBinders
        scopedFixtureCandidate with
  | .ok principal, .ok occurrences => some (principal, occurrences)
  | _, _ => none

def derivedScopedSlots (rows : RawInstructionRows) (functions : RawFunctionRows)
    (declarations : RawLocalDeclarations) : Option (List (BinderId × Nat)) :=
  scopedOccurrencesAndPrincipal.bind fun (_, occurrences) =>
    deriveRawSlots 128 (sourceRawNameBytes scopedUnit) rows functions 1 scopedFixtureParameters declarations
      occurrences

def derivedScopedStoreCheck (rows : RawInstructionRows) (functions : RawFunctionRows)
    (declarations : RawLocalDeclarations) : Bool :=
  match scopedOccurrencesAndPrincipal with
  | some (principal, occurrences) =>
      checkDerivedRawStoreValueFact 128 scopedFixtureNames scopedUnit
        (substitute principal (.variable scopedFixtureTypeVar)) scopedFixtureBinders principal
        scopedFixtureScheme.requirements (fun _ => none) rows functions 1 9 scopedFixtureParameters declarations
        occurrences ⟨scopedBodyDocument, [.store scopedStore]⟩ (1, 0)
  | none => false

def withLocalClaimRow (declarations : RawLocalDeclarations) (row : Nat) : RawLocalDeclarations :=
  { declarations with locals := declarations.locals.map fun claim => { claim with row := row } }

def withLocalClaimSite (declarations : RawLocalDeclarations) (site : SourceSite) : RawLocalDeclarations :=
  { declarations with locals := declarations.locals.map fun claim => { claim with site := site } }

def withLocalFactMutable (declarations : RawLocalDeclarations) (instruction : Nat) (mutable : Bool) :
    RawLocalDeclarations :=
  { declarations with facts := declarations.facts.map fun fact =>
      if fact.instruction = instruction then { fact with mutable := mutable } else fact }

def withRawPayload (rows : RawInstructionRows) (statement index value : Nat) : RawInstructionRows :=
  match rows.data[statement]? with
  | some data => { rows with extra := rows.extra.set (data.lhs + index) value }
  | none => rows

def withParameterCount (functions : RawFunctionRows) (count : Nat) : RawFunctionRows :=
  { functions with functions := functions.functions.map fun function =>
      { function with parameters := ⟨function.parameters.start, count⟩ } }

def scopedExportedRows : RawInstructionRows := exportedRawRows scopedRawInstructionCodes
def scopedExportedFunctions : RawFunctionRows := exportedRawFunctions scopedRawFunctionCodes
def scopedExportedLocals : RawLocalDeclarations := exportedRawLocals scopedLocalDeclarationsCodes

def pairBinder (index : Nat) (valid : index < 4294967296 := by decide) : BinderId := ⟨⟨index, valid⟩⟩

def pairDerivation (declarations : RawLocalDeclarations) (rows : RawInstructionRows) :
    Option (List (BinderId × Nat)) := do
  let unit ← (admitSource 0 "pure pair(a, b) { var x = a; var y = b; y = x; y }\nassert pair(1, 2) == 1\n").toOption
  let .function source :: _ := unit.declarations | none
  let first ← sourceName unit.names "a"
  let second ← sourceName unit.names "b"
  let integer : MonoType := .primitive .integer
  let parameters : List Binding :=
    [⟨first, pairBinder 0, .immutable, ⟨[], integer, []⟩⟩, ⟨second, pairBinder 1, .immutable, ⟨[], integer, []⟩⟩]
  let candidate : List StatementNode :=
    [.binding (pairBinder 2) .mutable integer (.read (pairBinder 0) []),
     .binding (pairBinder 3) .mutable integer (.read (pairBinder 1) []),
     .assignment (pairBinder 3) (.read (pairBinder 2) []),
     .evaluated (.read (pairBinder 3) [])]
  let occurrences ← (sourceScopedOccurrences 128 source parameters [] [] (fun _ => some integer) candidate).toOption
  deriveRawSlots 128 (sourceRawNameBytes unit) rows (exportedRawFunctions pairRawFunctionCodes) 1 parameters
    declarations occurrences

def pairExportedRows : RawInstructionRows := exportedRawRows pairRawInstructionCodes
def pairExportedLocals : RawLocalDeclarations := exportedRawLocals pairLocalDeclarationsCodes

def withFirstClaimRow (declarations : RawLocalDeclarations) (row : Nat) : RawLocalDeclarations :=
  { declarations with locals := match declarations.locals with
      | first :: rest => { first with row := row } :: rest
      | other => other }

def swappedPairClaims (declarations : RawLocalDeclarations) : RawLocalDeclarations :=
  { declarations with locals := match declarations.locals with
      | first :: second :: rest => ⟨first.site, second.row⟩ :: ⟨second.site, first.row⟩ :: rest
      | other => other }

set_option maxRecDepth 100000 in
theorem derived_scoped_store_authenticates_slots_and_rejects_forgeries :
    derivedScopedSlots scopedExportedRows scopedExportedFunctions scopedExportedLocals =
      some [(scopedFixtureInput, 0), (scopedFixtureLocal, 2)] ∧
    derivedScopedStoreCheck scopedExportedRows scopedExportedFunctions scopedExportedLocals = true ∧
    derivedScopedStoreCheck scopedExportedRows scopedExportedFunctions (withLocalClaimRow scopedExportedLocals 1) = false ∧
    derivedScopedStoreCheck scopedExportedRows scopedExportedFunctions (withLocalClaimRow scopedExportedLocals 5) = false ∧
    derivedScopedStoreCheck scopedExportedRows scopedExportedFunctions (withLocalFactMutable scopedExportedLocals 3 false) = false ∧
    derivedScopedStoreCheck scopedExportedRows scopedExportedFunctions { scopedExportedLocals with locals := [] } = false ∧
    derivedScopedStoreCheck scopedExportedRows scopedExportedFunctions
      { scopedExportedLocals with locals := scopedExportedLocals.locals ++ scopedExportedLocals.locals } = false ∧
    derivedScopedStoreCheck scopedExportedRows scopedExportedFunctions
      (withLocalClaimSite scopedExportedLocals ⟨0, 57, 15⟩) = false ∧
    derivedScopedStoreCheck (withRawPayload scopedExportedRows 3 0 3) scopedExportedFunctions scopedExportedLocals = false ∧
    derivedScopedStoreCheck (withRawPayload scopedExportedRows 3 0 1) scopedExportedFunctions scopedExportedLocals = false ∧
    derivedScopedStoreCheck (withRawPayload scopedExportedRows 3 0 4) scopedExportedFunctions scopedExportedLocals = false ∧
    derivedScopedStoreCheck (withRawPayload scopedExportedRows 9 0 1) scopedExportedFunctions scopedExportedLocals = false ∧
    derivedScopedStoreCheck scopedExportedRows (withParameterCount scopedExportedFunctions 2) scopedExportedLocals = false := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem derived_pair_slots_reject_swapped_and_temporary_claims :
    (pairDerivation pairExportedLocals pairExportedRows).map (List.map Prod.snd) = some [0, 1, 3, 5] ∧
    pairDerivation (swappedPairClaims pairExportedLocals) pairExportedRows = none ∧
    pairDerivation (withFirstClaimRow pairExportedLocals 1) pairExportedRows = none ∧
    pairDerivation (withLocalClaimSite pairExportedLocals ⟨0, 0, 0⟩) pairExportedRows = none := by
  decide +kernel

end Xsh.Conformance.Connected
