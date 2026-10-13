import Xsh.Effects.Lexical

/-!
A bounded model of five named host primitives and their effect requirements.
The requirements are checked with the existing lexical permission model.
This fragment does not classify arbitrary source syntax, prove registry or
implementation correspondence, infer call graphs, model diagnostics, or
execute host operations. It is not a full effect-system or conformance proof.
-/

namespace Xsh.Semantics.Effects

open Xsh.Syntax

/-- The named primitive operations represented by this bounded classifier. -/
inductive PrimitiveOperation where
  | fileRead
  | run
  | timeNow
  | socketConnect
  | print
  deriving DecidableEq, Repr

/-- Required effects for the represented primitive operations. -/
def classifyPrimitive : PrimitiveOperation → EffectSet
  | .fileRead => [.fs]
  | .run => [.process]
  | .timeNow => [.time]
  | .socketConnect => [.process, .net]
  | .print => []

/-- Attach a supplied lexical source label to one classified requirement. -/
def obligationsFor (classifier : PrimitiveOperation → EffectSet)
    (operation : PrimitiveOperation) (source : Expr) : List EffectObligation :=
  (classifier operation).map fun required => ⟨source, required⟩

/-- Check the represented primitive requirements at one lexical context. -/
def permitsPrimitiveWith (classifier : PrimitiveOperation → EffectSet)
    (context : LexicalContext) (source : Expr) (operation : PrimitiveOperation) : Prop :=
  permitsObligations context (obligationsFor classifier operation source)

/-- Check a named primitive with the classifier defined in this fragment. -/
def permitsPrimitive (context : LexicalContext) (source : Expr)
    (operation : PrimitiveOperation) : Prop :=
  permitsPrimitiveWith classifyPrimitive context source operation

private def ioBoundContext : LexicalContext :=
  ⟨[.io], []⟩

private def withoutNetContext : LexicalContext :=
  ⟨[.io], [.net]⟩

private def fileSystemBoundContext : LexicalContext :=
  ⟨[.fs], []⟩

private def processBoundContext : LexicalContext :=
  ⟨[.process], []⟩

/-- A deliberate mutation that assigns process permission to file reads. -/
def classifyFileReadAsProcess : PrimitiveOperation → EffectSet
  | .fileRead => [.process]
  | operation => classifyPrimitive operation

/-- A deliberate mutation that omits the network requirement for connect. -/
def classifySocketConnectWithoutNet : PrimitiveOperation → EffectSet
  | .socketConnect => [.process]
  | operation => classifyPrimitive operation

theorem file_read_requires_filesystem : classifyPrimitive .fileRead = [.fs] := by
  rfl

theorem run_requires_process : classifyPrimitive .run = [.process] := by
  rfl

theorem time_now_requires_time : classifyPrimitive .timeNow = [.time] := by
  rfl

theorem socket_connect_requires_process_and_network :
    classifyPrimitive .socketConnect = [.process, .net] := by
  rfl

theorem print_requires_no_effects : classifyPrimitive .print = [] := by
  rfl

theorem io_bound_permits_run (source : Expr) :
    permitsPrimitive ioBoundContext source .run := by
  simp [permitsPrimitive, permitsPrimitiveWith, ioBoundContext,
    obligationsFor, classifyPrimitive, permitsObligations, permits,
    clauseCovers, ioCovers, blockedBy]

theorem without_net_rejects_socket_connect (source : Expr) :
    ¬ permitsPrimitive withoutNetContext source .socketConnect := by
  simp [permitsPrimitive, permitsPrimitiveWith, withoutNetContext,
    obligationsFor, classifyPrimitive, permitsObligations, permits,
    clauseCovers, ioCovers, blockedBy, withoutBlocks]

theorem file_read_fs_to_process_mutation_changes_permission (source : Expr) :
    permitsPrimitiveWith classifyPrimitive fileSystemBoundContext source .fileRead ∧
      ¬ permitsPrimitiveWith classifyFileReadAsProcess
        fileSystemBoundContext source .fileRead := by
  simp [permitsPrimitiveWith, fileSystemBoundContext, obligationsFor,
    classifyPrimitive, classifyFileReadAsProcess, permitsObligations,
    permits, clauseCovers, ioCovers, blockedBy]

theorem socket_connect_dropped_net_mutation_changes_permission (source : Expr) :
    ¬ permitsPrimitiveWith classifyPrimitive processBoundContext source .socketConnect ∧
      permitsPrimitiveWith classifySocketConnectWithoutNet
        processBoundContext source .socketConnect := by
  simp [permitsPrimitiveWith, processBoundContext, obligationsFor,
    classifyPrimitive, classifySocketConnectWithoutNet, permitsObligations,
    permits, clauseCovers, ioCovers, blockedBy]

end Xsh.Semantics.Effects
