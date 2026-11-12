import Xsh.Semantics.Connected.SafetyCaller
import Xsh.Semantics.Connected.SafetyRecord
import Xsh.Typing.Connected.NodeBlockJudgment
import Xsh.Typing.Connected.BlockCertificate

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Syntax.Foundation
open Xsh.Typing.Connected
open Xsh.Semantics.Environments

inductive ParameterTypesTyped (solution : Substitution) : Scope → RuntimeType → Prop where
  | nil : ParameterTypesTyped solution [] .argumentsEmpty
  | cons {name binder raw rest ty types} :
      monoRuntime (substitute solution raw) = some ty → ParameterTypesTyped solution rest types →
      ParameterTypesTyped solution (⟨name, binder, .immutable, ⟨[], raw, []⟩⟩ :: rest) (.argument ty types)

def SchemeCallable (functions : FunctionTable) (declaration : DeclarationId) (scheme : Scheme) : Prop :=
  (∃ function, functions declaration = some (.function function) ∧ function.scheme = scheme) ∨
  (functions declaration = some .resultOk ∧
    ∀ captured arguments parameters result normalized,
      instantiateFunction captured scheme arguments = some (parameters, result, normalized) →
      ∃ success failure, parameters = .argument success .argumentsEmpty ∧ result = .result success failure) ∨
  (functions declaration = some .resultError ∧
    ∀ captured arguments parameters result normalized,
      instantiateFunction captured scheme arguments = some (parameters, result, normalized) →
      ∃ success failure, parameters = .argument failure .argumentsEmpty ∧ result = .result success failure) ∨
  (functions declaration = some .timeNow ∧
    ∀ captured arguments parameters result normalized,
      instantiateFunction captured scheme arguments = some (parameters, result, normalized) →
      parameters = .argumentsEmpty ∧ result = .integer)

mutual
  inductive ValueCallables (functions : FunctionTable) : Value → Prop where
    | scalar {value} : ValueCallables functions (.scalar value)
    | unit : ValueCallables functions .unit
    | validationError : ValueCallables functions .validationError
    | record {fields} : FieldCallables functions fields → ValueCallables functions (.record fields)
    | ok {value failureType} : ValueCallables functions value → ValueCallables functions (.ok value failureType)
    | error {value success} : ValueCallables functions value → ValueCallables functions (.error value success)
    | nominal {identity base value} : ValueCallables functions value → ValueCallables functions (.nominal identity base value)
    | scheme {declaration scheme} : SchemeCallable functions declaration scheme →
        ValueCallables functions (.schemeFunction declaration scheme)
    | functionOk {declaration success failure} : functions declaration = some .resultOk →
        ValueCallables functions (.function declaration (.argument success .argumentsEmpty) (.result success failure))
    | functionError {declaration success failure} : functions declaration = some .resultError →
        ValueCallables functions (.function declaration (.argument failure .argumentsEmpty) (.result success failure))
    | functionTime {declaration} : functions declaration = some .timeNow →
        ValueCallables functions (.function declaration .argumentsEmpty .integer)
    | instantiatedFunction {declaration parameters result captured arguments normalized function} :
        functions declaration = some (.function function) → WellSorted captured →
        instantiateFunction captured function.scheme arguments = some (parameters, result, normalized) →
        ValueCallables functions (.instantiatedFunction declaration parameters result captured arguments)
    | instantiatedOk {declaration success failure captured arguments} : functions declaration = some .resultOk →
        ValueCallables functions (.instantiatedFunction declaration (.argument success .argumentsEmpty)
          (.result success failure) captured arguments)
    | instantiatedError {declaration success failure captured arguments} : functions declaration = some .resultError →
        ValueCallables functions (.instantiatedFunction declaration (.argument failure .argumentsEmpty)
          (.result success failure) captured arguments)
    | instantiatedTime {declaration captured arguments} : functions declaration = some .timeNow →
        ValueCallables functions (.instantiatedFunction declaration .argumentsEmpty .integer captured arguments)

  inductive FieldCallables (functions : FunctionTable) : List (NameId × Value) → Prop where
    | nil : FieldCallables functions []
    | cons {name value rest} : ValueCallables functions value → FieldCallables functions rest →
        FieldCallables functions ((name, value) :: rest)
end

def StoreCallables (functions : FunctionTable) (store : Store) : Prop :=
  ∀ cell entry, store cell = some entry → ValueCallables functions entry.contents.value

inductive BindingStorage where
  | ordinary
  | scheme
  deriving DecidableEq

abbrev BindingStorageMap := BinderId → BindingStorage

def ReadContract (scope : Scope) (runtimeSolution : Substitution) (storage : BindingStorageMap)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (ty : MonoType) : Prop :=
  ∀ name binding, scope.find? (fun item => decide (item.name = name)) = some binding →
    binding.binder = binder → storage binder = .scheme →
    ∃ parameters result normalized,
      instantiateFunction runtimeSolution binding.scheme arguments = some (parameters, result, normalized) ∧
      monoRuntime ty = some (.function parameters result)

mutual
  inductive GroundNode (solution runtimeSolution : Substitution) (storage : BindingStorageMap) : Scope → MonoType → Node → MonoType → Prop where
    | integer {scope functionResult text value} : parseInteger text = some value → GroundNode solution runtimeSolution storage scope functionResult (.integer text) (.primitive .integer)
    | boolean {scope functionResult value} : GroundNode solution runtimeSolution storage scope functionResult (.boolean value) (.primitive .boolean)
    | string {scope functionResult value} : GroundNode solution runtimeSolution storage scope functionResult (.string value) (.primitive .string)
    | read {scope functionResult binder arguments ty runtimeType} : NodeHasTypeIn solution (some functionResult) scope (.read binder arguments) ty →
        monoRuntime ty = some runtimeType → ReadContract scope runtimeSolution storage binder arguments ty →
        GroundNode solution runtimeSolution storage scope functionResult (.read binder arguments) ty
    | binary {scope functionResult operator left right leftType rightType result runtimeType} :
        GroundNode solution runtimeSolution storage scope functionResult left leftType → GroundNode solution runtimeSolution storage scope functionResult right rightType →
        BinaryTyping operator leftType rightType result → monoRuntime result = some runtimeType →
        GroundNode solution runtimeSolution storage scope functionResult (.binary operator left right) result
    | call {scope functionResult callee arguments argumentTypes result runtimeType} :
        GroundNode solution runtimeSolution storage scope functionResult callee (.function argumentTypes result) →
        GroundArguments solution runtimeSolution storage scope functionResult arguments argumentTypes → monoRuntime result = some runtimeType →
        GroundNode solution runtimeSolution storage scope functionResult (.call callee arguments) result
    | record {scope functionResult fields types runtimeType} : GroundFields solution runtimeSolution storage scope functionResult fields types →
        (types.map Prod.fst).Nodup →
        monoRuntime (.record ((Xsh.Typing.Connected.canonicalFields types).foldr
          (fun (name, ty) row => .rowField name ty row) .rowEmpty)) = some runtimeType →
        GroundNode solution runtimeSolution storage scope functionResult (.record fields)
          (.record ((Xsh.Typing.Connected.canonicalFields types).foldr
            (fun (name, ty) row => .rowField name ty row) .rowEmpty))
    | field {scope functionResult base name row result runtimeType} : GroundNode solution runtimeSolution storage scope functionResult base (.record row) →
        FieldOccurs name result row → monoRuntime result = some runtimeType →
        GroundNode solution runtimeSolution storage scope functionResult (.field base name) result
    | conditional {scope functionResult branches otherwise result runtimeType} :
        GroundBranches solution runtimeSolution storage scope functionResult branches result → GroundNode solution runtimeSolution storage scope functionResult otherwise result →
        monoRuntime result = some runtimeType → GroundNode solution runtimeSolution storage scope functionResult (.conditional branches otherwise) result
    | requireValue {scope functionResult binder identity value input result runtimeType} :
        NodeHasTypeIn solution (some functionResult) scope (.requireValue binder identity value) result →
        GroundNode solution runtimeSolution storage scope functionResult value input → monoRuntime result = some runtimeType →
        GroundNode solution runtimeSolution storage scope functionResult (.requireValue binder identity value) result
    | propagate {scope functionResult value result failure expectedError runtimeType} :
        GroundNode solution runtimeSolution storage scope functionResult value (.result result failure) → failure = substitute solution expectedError →
        (∃ success, functionResult = .result success failure) →
        monoRuntime result = some runtimeType → GroundNode solution runtimeSolution storage scope functionResult (.propagate value expectedError) result

    | valueBlock {scope functionResult raw body runtimeType} :
        GroundBlock solution runtimeSolution storage scope functionResult (some (substitute solution raw)) body →
        monoRuntime (substitute solution raw) = some runtimeType →
        GroundNode solution runtimeSolution storage scope functionResult (.valueBlock raw body) (substitute solution raw)

  inductive GroundArguments (solution runtimeSolution : Substitution) (storage : BindingStorageMap) : Scope → MonoType → List Node → MonoType → Prop where
    | nil {scope functionResult} : GroundArguments solution runtimeSolution storage scope functionResult [] .argumentsEmpty
    | cons {scope functionResult node rest ty types} : GroundNode solution runtimeSolution storage scope functionResult node ty → GroundArguments solution runtimeSolution storage scope functionResult rest types →
        GroundArguments solution runtimeSolution storage scope functionResult (node :: rest) (.argument ty types)

  inductive GroundFields (solution runtimeSolution : Substitution) (storage : BindingStorageMap) : Scope → MonoType → List (NameId × Node) → List (NameId × MonoType) → Prop where
    | nil {scope functionResult} : GroundFields solution runtimeSolution storage scope functionResult [] []
    | cons {scope functionResult name node rest ty types} : GroundNode solution runtimeSolution storage scope functionResult node ty → GroundFields solution runtimeSolution storage scope functionResult rest types →
        GroundFields solution runtimeSolution storage scope functionResult ((name, node) :: rest) ((name, ty) :: types)

  inductive GroundBranches (solution runtimeSolution : Substitution) (storage : BindingStorageMap) : Scope → MonoType → List (Node × Node) → MonoType → Prop where
    | nil {scope functionResult result} : GroundBranches solution runtimeSolution storage scope functionResult [] result
    | cons {scope functionResult condition branch rest result} : GroundNode solution runtimeSolution storage scope functionResult condition (.primitive .boolean) →
        GroundNode solution runtimeSolution storage scope functionResult branch result → GroundBranches solution runtimeSolution storage scope functionResult rest result →
        GroundBranches solution runtimeSolution storage scope functionResult ((condition, branch) :: rest) result

  inductive GroundBlock (solution runtimeSolution : Substitution) (storage : BindingStorageMap) : Scope → MonoType → Option MonoType → List StatementNode → Prop where
    | nil {scope result completion} : CompletionAccepts completion (.primitive .unit) →
        GroundBlock solution runtimeSolution storage scope result completion []
    | binding {scope result completion name binder kind raw initializer rest initializerType runtimeType} :
        GroundNode solution runtimeSolution storage scope result initializer initializerType →
        StoreCompatible (substitute solution raw) initializerType →
        monoRuntime (substitute solution raw) = some runtimeType →
        monoRuntime (substitute runtimeSolution raw) = some runtimeType →
        (kind = .immutable ∨ kind = .mutable) → storage binder = .ordinary →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        GroundBlock solution runtimeSolution storage (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: scope) result completion rest →
        GroundBlock solution runtimeSolution storage scope result completion (.binding binder kind raw initializer :: rest)
    | assignment {scope result completion name binding value rest valueType} :
        scope.find? (fun item => decide (item.name = name)) = some binding → binding.kind = .mutable →
        GroundNode solution runtimeSolution storage scope result value valueType →
        StoreCompatible (substitute solution binding.scheme.body) valueType →
        GroundBlock solution runtimeSolution storage scope result completion rest →
        GroundBlock solution runtimeSolution storage scope result completion (.assignment binding.binder value :: rest)
    | returned {scope result completion value rest} : GroundNode solution runtimeSolution storage scope result value result →
        GroundBlock solution runtimeSolution storage scope result none rest → GroundBlock solution runtimeSolution storage scope result completion (.returned value :: rest)
    | evaluatedTail {scope result completion value ty} : GroundNode solution runtimeSolution storage scope result value ty →
        CompletionAccepts completion ty → GroundBlock solution runtimeSolution storage scope result completion [.evaluated value]
    | evaluatedNext {scope result completion value next rest ty} : GroundNode solution runtimeSolution storage scope result value ty →
        GroundBlock solution runtimeSolution storage scope result completion (next :: rest) →
        GroundBlock solution runtimeSolution storage scope result completion (.evaluated value :: next :: rest)
    | conditionalTail {scope result completion branches fallback} :
        GroundStatementBranches solution runtimeSolution storage scope result completion branches → GroundBlock solution runtimeSolution storage scope result completion fallback →
        GroundBlock solution runtimeSolution storage scope result completion [.conditional branches fallback]
    | conditionalNext {scope result completion branches fallback next rest} :
        GroundStatementBranches solution runtimeSolution storage scope result none branches → GroundBlock solution runtimeSolution storage scope result none fallback →
        GroundBlock solution runtimeSolution storage scope result completion (next :: rest) →
        GroundBlock solution runtimeSolution storage scope result completion (.conditional branches fallback :: next :: rest)

  inductive GroundStatementBranches (solution runtimeSolution : Substitution) (storage : BindingStorageMap) :
      Scope → MonoType → Option MonoType → List (Node × List StatementNode) → Prop where
    | nil {scope result completion} : GroundStatementBranches solution runtimeSolution storage scope result completion []
    | cons {scope result completion condition body rest} :
        GroundNode solution runtimeSolution storage scope result condition (.primitive .boolean) → GroundBlock solution runtimeSolution storage scope result completion body →
        GroundStatementBranches solution runtimeSolution storage scope result completion rest →
        GroundStatementBranches solution runtimeSolution storage scope result completion ((condition, body) :: rest)
end



def StorageAgreement (cells : ActiveCells) (store : Store) (storage : BindingStorageMap) : Prop :=
  (∀ binder declaration scheme, Connected.read store cells binder = some (.schemeFunction declaration scheme) →
    storage binder = .scheme) ∧
  (∀ binder, storage binder = .scheme →
    ∃ declaration scheme, Connected.read store cells binder = some (.schemeFunction declaration scheme))

def FunctionBodyCertified (globals : Scope) (globalStorage : BindingStorageMap) (state : Machine) (function : Function) : Prop :=
  ∀ captured typeArguments parameters result normalized,
    WellSorted captured → instantiateFunction captured function.scheme typeArguments = some (parameters, result, normalized) →
    ∃ (extension : Substitution) (parameterScope : Scope) (bodyResult : MonoType),
      let storage := globalStorage
      let runtimeSolution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration)
      let solution := compose extension runtimeSolution
      WellSorted extension ∧ WellSorted runtimeSolution ∧ WellSorted solution ∧
      ScopeValuesAgree globals solution state.globals state.store ∧
      ScopeWritableContract globals solution state.globals state.store ∧
      MutableCellsAgree globals solution state.globals state.store ∧
      SchemaMatches globals solution state.schemas ∧
      (∀ binder ∈ function.parameters, storage binder = .ordinary) ∧
      (∀ binder ∈ declaredBinders function.body, storage binder = .ordinary) ∧
      function.parameters = parameterScope.map Binding.binder ∧
      ((parameterScope ++ globals).map Binding.binder).Nodup ∧
      ParameterTypesTyped
        (compose extension (compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration)))
        parameterScope parameters ∧
      GroundBlock
        (compose extension (compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration)))
        (compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration))
        storage (parameterScope ++ globals) bodyResult (some bodyResult) function.body ∧
      monoRuntime bodyResult = some result ∧
      (declaredBinders function.body).Nodup ∧
      (∀ binder ∈ declaredBinders function.body, binder ∉ function.parameters) ∧
      PropagationMatches (propagationContext bodyResult)
        (compose extension (compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration)))
        (blockPropagationErrors function.body)

def FunctionTableCertified (globals : Scope) (globalStorage : BindingStorageMap) (state : Machine) : Prop :=
  ∀ declaration function, state.functions declaration = some (.function function) →
    FunctionBodyCertified globals globalStorage state function

structure TypingContext where
  scope : Scope
  storage : BindingStorageMap
  solution : Substitution
  runtimeSolution : Substitution
  result : MonoType
  completion : Option MonoType
  activation : Nat
  resultClosed : ∃ runtimeType, monoRuntime result = some runtimeType

inductive TypedDemand where
  | expression (context : TypingContext) (ty : MonoType)
  | block (context : TypingContext)

inductive TypedValues (functions : FunctionTable) : List Value → List MonoType → Prop where
  | nil : TypedValues functions [] []
  | cons {value values ty types runtimeType} :
      HasType runtimeType value → monoRuntime ty = some runtimeType →
      ValueCallables functions value → TypedValues functions values types →
      TypedValues functions (value :: values) (ty :: types)

inductive TypedFields (functions : FunctionTable) : List (NameId × Value) → List (NameId × MonoType) → Prop where
  | nil : TypedFields functions [] []
  | cons {name value values ty types runtimeType} :
      HasType runtimeType value → monoRuntime ty = some runtimeType →
      ValueCallables functions value → TypedFields functions values types →
      TypedFields functions ((name, value) :: values) ((name, ty) :: types)

def FutureBindingsFresh (store : Store) (activation : Nat) (body : List StatementNode) : Prop :=
  ∀ binder ∈ declaredBinders body, store ⟨activation, binder⟩ = none

def FutureBranchBindingsFresh (store : Store) (activation : Nat) (branches : List (Node × List StatementNode)) : Prop :=
  ∀ binder ∈ branchBinders branches, store ⟨activation, binder⟩ = none

def RemainingBranchBindingsFresh (store : Store) (activation : Nat)
    (branches : List (Node × List StatementNode)) : Prop :=
  match branches with
  | [] => True
  | (_, body) :: rest => FutureBindingsFresh store activation body ∧ FutureBranchBindingsFresh store activation rest

inductive FrameTyped (state : Machine) : TypedDemand → TypedDemand → Frame → Prop where
  | binaryLeft {context operator leftType rightType result right} :
      GroundNode context.solution context.runtimeSolution context.storage context.scope context.result right rightType → BinaryTyping operator leftType rightType result →
      FrameTyped state (.expression context leftType) (.expression context result) (.binaryLeft operator right)
  | binaryRight {context operator leftType rightType result left runtimeType} :
      HasType runtimeType left → monoRuntime leftType = some runtimeType →
      ValueCallables state.functions left → BinaryTyping operator leftType rightType result →
      FrameTyped state (.expression context rightType) (.expression context result) (.binaryRight operator left)
  | callTarget {context arguments argumentTypes result} :
      GroundArguments context.solution context.runtimeSolution context.storage context.scope context.result arguments argumentTypes →
      FrameTyped state (.expression context (.function argumentTypes result)) (.expression context result) (.callTarget arguments)
  | callArguments {context target done rest doneTypes input restTypes result} :
      TypedValues state.functions done doneTypes → GroundArguments context.solution context.runtimeSolution context.storage context.scope context.result rest restTypes →
      (∃ runtimeType, monoRuntime (.function (doneTypes.foldr MonoType.argument (.argument input restTypes)) result) = some runtimeType ∧
        valueType target = some runtimeType) → ValueCallables state.functions target →
      FrameTyped state (.expression context input) (.expression context result) (.callArguments target done rest)
  | recordFields {context done doneTypes name rest restTypes input} :
      TypedFields state.functions done doneTypes → GroundFields context.solution context.runtimeSolution context.storage context.scope context.result rest restTypes →
      ((doneTypes ++ [(name, input)] ++ restTypes).map Prod.fst).Nodup →
      FrameTyped state (.expression context input)
        (.expression context (.record ((Xsh.Typing.Connected.canonicalFields (doneTypes ++ [(name, input)] ++ restTypes)).foldr
          (fun (name, ty) row => .rowField name ty row) .rowEmpty))) (.recordFields done name rest)
  | field {context name row result} : FieldOccurs name result row →
      FrameTyped state (.expression context (.record row)) (.expression context result) (.field name)
  | conditional {context branches otherwise result} :
      branches ≠ [] → GroundBranches context.solution context.runtimeSolution context.storage context.scope context.result branches result →
      GroundNode context.solution context.runtimeSolution context.storage context.scope context.result otherwise result →
      FrameTyped state (.expression context (.primitive .boolean)) (.expression context result) (.conditional branches otherwise)
  | propagate {context success failure result} : context.result = .result result failure →
      FrameTyped state (.expression context (.result success failure)) (.expression context success) .propagate
  | requireGuard {context input identity guard base} : runtimeMonotype (guardBase guard) = some base →
      FrameTyped state (.expression context input)
        (.expression context (.result (.nominal identity base) (.primitive .error))) (.requireGuard identity guard)
  | bind {context name binder kind raw rest input runtimeType} :
      StoreCompatible (substitute context.solution raw) input →
      monoRuntime (substitute context.solution raw) = some runtimeType →
      (kind = .immutable ∨ kind = .mutable) → context.storage binder = .ordinary →
      (∀ binding ∈ context.scope, binding.binder ≠ binder) → state.store ⟨context.activation, binder⟩ = none →
      GroundBlock context.solution context.runtimeSolution context.storage (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: context.scope) context.result context.completion rest →
      FutureBindingsFresh state.store context.activation rest →
      FrameTyped state (.expression context input)
        (.block { context with scope := ⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: context.scope }) (.bind binder kind runtimeType rest)
  | assign {context name binding rest input} :
      StoreCompatible (substitute context.solution binding.scheme.body) input →
      context.scope.find? (fun item => decide (item.name = name)) = some binding → binding.kind = .mutable →
      GroundBlock context.solution context.runtimeSolution context.storage context.scope context.result context.completion rest →
      FutureBindingsFresh state.store context.activation rest →
      FrameTyped state (.expression context input) (.block context) (.assign binding.binder rest)
  | discard {context input rest} : GroundBlock context.solution context.runtimeSolution context.storage context.scope context.result context.completion rest →
      FutureBindingsFresh state.store context.activation rest →
      FrameTyped state (.expression context input) (.block context) (.discard rest)
  | returnValue {context} : FrameTyped state (.expression context context.result) (.block context) .returnValue
  | tailValue {context ty} : CompletionAccepts context.completion ty →
      FrameTyped state (.expression context ty) (.block context) .tailValue
  | branch {context branches otherwise rest} :
      branches ≠ [] → GroundStatementBranches context.solution context.runtimeSolution context.storage context.scope context.result
        (if rest.isEmpty then context.completion else none) branches →
      GroundBlock context.solution context.runtimeSolution context.storage context.scope context.result
        (if rest.isEmpty then context.completion else none) otherwise →
      (rest ≠ [] → GroundBlock context.solution context.runtimeSolution context.storage context.scope context.result context.completion rest) →
      RemainingBranchBindingsFresh state.store context.activation branches →
      FutureBindingsFresh state.store context.activation otherwise → FutureBindingsFresh state.store context.activation rest →
      FrameTyped state (.expression context (.primitive .boolean)) (.block context) (.branch branches otherwise rest)
  | continueBlock {inner outer cells rest} :
      inner.solution = outer.solution → inner.runtimeSolution = outer.runtimeSolution → inner.storage = outer.storage →
      inner.result = outer.result → inner.activation = outer.activation →
      inner.completion = (if rest.isEmpty then outer.completion else none) →
      ScopeStateAgrees outer.scope { state with cells := cells, solution := outer.solution } →
      ScopeWritableContract outer.scope outer.solution cells state.store →
      SchemeCellsImmutable cells state.store → StorageAgreement cells state.store outer.storage →
      SchemaMatches outer.scope outer.solution state.schemas →
      (rest ≠ [] → GroundBlock outer.solution outer.runtimeSolution outer.storage outer.scope outer.result outer.completion rest) →
      FutureBindingsFresh state.store outer.activation rest →
      FrameTyped state (.block inner) (.block outer) (.continueBlock cells rest)
  | valueBlock {inner outer cells blockType} :
      inner.solution = outer.solution → inner.runtimeSolution = outer.runtimeSolution → inner.storage = outer.storage →
      inner.result = outer.result → inner.activation = outer.activation → inner.completion = some blockType →
      ScopeStateAgrees outer.scope { state with cells := cells, solution := outer.solution } →
      ScopeWritableContract outer.scope outer.solution cells state.store →
      SchemeCellsImmutable cells state.store → StorageAgreement cells state.store outer.storage →
      SchemaMatches outer.scope outer.solution state.schemas →
      FrameTyped state (.block inner) (.expression outer blockType) (.valueBlock cells)
  | caller {inner outer cells activation result callType} :
      inner.storage = outer.storage → WellSorted outer.solution → WellSorted outer.runtimeSolution →
      outer.activation = activation → inner.completion = some inner.result → monoRuntime inner.result = some result → monoRuntime callType = some result →
      ScopeStateAgrees outer.scope { state with cells := cells, solution := outer.solution } →
      ScopeWritableContract outer.scope outer.solution cells state.store →
      SchemeCellsImmutable cells state.store → StorageAgreement cells state.store outer.storage →
      SchemaMatches outer.scope outer.solution state.schemas →
      FrameTyped state (.block inner) (.expression outer callType) (.caller cells activation outer.runtimeSolution result)

def TypedDemand.context : TypedDemand → TypingContext
  | .expression context _ => context
  | .block context => context

def DemandClosed : TypedDemand → Prop
  | .expression _ ty => ∃ runtimeType, monoRuntime ty = some runtimeType
  | .block context =>
      (∃ runtimeType, monoRuntime context.result = some runtimeType) ∧
      ∀ ty, context.completion = some ty → ∃ runtimeType, monoRuntime ty = some runtimeType

inductive StackTyped (state : Machine) : TypedDemand → TypedDemand → List Frame → Prop where
  | nil {demand} : StackTyped state demand demand []
  | cons {input middle output frame rest} : FrameTyped state input middle frame →
      DemandClosed middle → StackTyped state middle output rest → StackTyped state input output (frame :: rest)

inductive RuntimeArguments (functions : FunctionTable) : List Value → RuntimeType → Prop where
  | nil : RuntimeArguments functions [] .argumentsEmpty
  | cons {value values ty types} : HasType ty value → ValueCallables functions value →
      RuntimeArguments functions values types → RuntimeArguments functions (value :: values) (.argument ty types)

def ReadOperationallyCompatible (state : Machine) (node : Node) (ty : MonoType) : Prop :=
  match node with
  | .read binder arguments =>
      ∀ declaration scheme,
        Connected.read state.store state.cells binder = some (.schemeFunction declaration scheme) →
        ∃ parameters result normalized,
          instantiateFunction state.solution scheme arguments = some (parameters, result, normalized) ∧
          monoRuntime ty = some (.function parameters result)
  | _ => True

theorem grounded_read_operationally_compatible (state : Machine) (scope : Scope)
    (solution : Substitution) (storage : BindingStorageMap) (functionResult : MonoType) (binder : BinderId)
    (arguments : List (Variable × MonoType)) (ty : MonoType)
    (values : ScopeValuesAgree scope solution state.cells state.store)
    (storageAgrees : StorageAgreement state.cells state.store storage)
    (ground : GroundNode solution state.solution storage scope functionResult (.read binder arguments) ty) :
    ReadOperationallyCompatible state (.read binder arguments) ty := by
  intro declaration scheme actual
  cases ground with
  | read nodeTyped closed contract =>
    cases nodeTyped with
    | read found notSchema keys sorts instantiated solved wellFormed sort =>
      rename_i name binding raw requirements
      have storageScheme := storageAgrees.1 binding.binder declaration scheme actual
      obtain ⟨value, readValue, agrees⟩ := values name binding found notSchema
      rw [actual] at readValue
      cases readValue
      rcases agrees with ⟨other, parameters, result, schemeValue, schemeBody⟩ | ordinary
      · cases schemeValue
        exact contract name binding found rfl storageScheme
      · simp [OrdinaryBindingValueAgrees, ordinaryValue] at ordinary

def FutureNodeBindingsFresh (store : Store) (activation : Nat) (node : Node) : Prop :=
  ∀ binder ∈ nodeDeclaredBinders node, store ⟨activation, binder⟩ = none

inductive ControlTyped (state : Machine) : TypedDemand → Control → Prop where
  | expression {context node ty runtimeType} :
      GroundNode context.solution context.runtimeSolution context.storage context.scope context.result node ty → monoRuntime ty = some runtimeType →
      FutureNodeBindingsFresh state.store context.activation node → ControlTyped state (.expression context ty) (.expression node)
  | value {context value ty runtimeType} :
      HasType runtimeType value → monoRuntime ty = some runtimeType → ValueCallables state.functions value →
      ControlTyped state (.expression context ty) (.value value)
  | invoke {context target arguments argumentTypes resultType result} :
      valueType target = some (.function argumentTypes resultType) → ValueCallables state.functions target →
      RuntimeArguments state.functions arguments argumentTypes → monoRuntime result = some resultType →
      ControlTyped state (.expression context result) (.invoke target arguments)
  | statements {context body} : GroundBlock context.solution context.runtimeSolution context.storage context.scope context.result context.completion body →
      FutureBindingsFresh state.store context.activation body → ControlTyped state (.block context) (.statements body)
  | returned {demand value runtimeType} :
      HasType runtimeType value → monoRuntime demand.context.result = some runtimeType →
      ValueCallables state.functions value → ControlTyped state demand (.returned value)
  | completed {context value ty runtimeType} :
      HasType runtimeType value → monoRuntime ty = some runtimeType →
      CompletionAccepts context.completion ty → ValueCallables state.functions value →
      ControlTyped state (.block context) (.completed value)
  | propagated {demand error oldType result failure runtimeFailure} :
      demand.context.result = .result result failure → HasType runtimeFailure error →
      monoRuntime failure = some runtimeFailure → ValueCallables state.functions error →
      ControlTyped state demand (.propagated (.error error oldType))
  | integerFault {demand fault} : ControlTyped state demand (.fault (.integer fault))

def ActiveScopeTyped (state : Machine) (context : TypingContext) : Prop :=
  match state.control with
  | .returned _ | .propagated _ | .fault _ => True
  | _ => ScopeStateAgrees context.scope { state with solution := context.solution } ∧
      ScopeWritableContract context.scope context.solution state.cells state.store ∧
      SchemaMatches context.scope context.solution state.schemas ∧
      SchemeCellsImmutable state.cells state.store ∧ StorageAgreement state.cells state.store context.storage

def pendingControlBinders : Control → List BinderId
  | .expression node => nodeDeclaredBinders node
  | .statements body => declaredBinders body
  | _ => []

def pendingFrameBinders : Frame → List BinderId
  | .binaryLeft _ right => nodeDeclaredBinders right
  | .callTarget arguments => argumentBinders arguments
  | .callArguments _ _ rest => argumentBinders rest
  | .recordFields _ _ rest => fieldBinders rest
  | .conditional branches otherwise =>
      match branches with
      | [] => nodeDeclaredBinders otherwise
      | (_, selected) :: rest => nodeDeclaredBinders selected ++ expressionBranchBinders rest ++ nodeDeclaredBinders otherwise
  | .bind binder _ _ rest => binder :: declaredBinders rest
  | .assign _ rest | .discard rest => declaredBinders rest
  | .branch branches otherwise rest =>
      match branches with
      | [] => declaredBinders otherwise ++ declaredBinders rest
      | (_, body) :: remaining => declaredBinders body ++ branchBinders remaining ++ declaredBinders otherwise ++ declaredBinders rest
  | .continueBlock _ rest => declaredBinders rest
  | _ => []

def frameCallerActivation (activation : Nat) : Frame → Nat
  | .caller _ saved _ _ => saved
  | _ => activation

def pendingStackBinders : Nat → List Frame → List (Nat × BinderId)
  | _, [] => []
  | activation, frame :: rest =>
      (pendingFrameBinders frame).map (fun binder => (activation, binder)) ++
        pendingStackBinders (frameCallerActivation activation frame) rest

def pendingMachineBinders (state : Machine) : List (Nat × BinderId) :=
  (pendingControlBinders state.control).map (fun binder => (state.activation, binder)) ++
    pendingStackBinders state.activation state.stack

structure PendingBindingsSafe (state : Machine) : Prop where
  unique : (pendingMachineBinders state).Nodup
  fresh : ∀ binding ∈ pendingMachineBinders state, state.store ⟨binding.1, binding.2⟩ = none

structure TypedMachine (globalScope : Scope) (globalStorage : BindingStorageMap) (globalTypes : CellTypes) (state : Machine) : Prop where
  memory : MachineMemoryInvariant globalTypes state
  functions : FunctionTableCertified globalScope globalStorage state
  globalDistinct : DistinctActiveCells state.globals
  globalSchemes : SchemeCellsImmutable state.globals state.store
  globalStorageAgrees : StorageAgreement state.globals state.store globalStorage
  storedValues : StoreCallables state.functions state.store
  focus : ∃ input output,
    input.context.runtimeSolution = state.solution ∧ input.context.activation = state.activation ∧
    input.context.storage = globalStorage ∧ WellSorted input.context.solution ∧ WellSorted state.solution ∧
    ActiveScopeTyped state input.context ∧
    DemandClosed input ∧ DemandClosed output ∧
    ControlTyped state input state.control ∧ StackTyped state input output state.stack ∧ PendingBindingsSafe state

end Xsh.Semantics.Connected
