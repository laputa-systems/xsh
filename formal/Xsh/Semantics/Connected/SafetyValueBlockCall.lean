import Xsh.Semantics.Connected.SafetyValueBlockCallerReturn

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected
open Xsh.Semantics.Environments

theorem pending_stack_activation_below (activation limit : Nat) (frames : List Frame)
    (activeBelow : activation < limit) (savedBelow : StackActivationsBelow limit frames) :
    ∀ binding ∈ pendingStackBinders activation frames, binding.1 < limit := by
  induction frames generalizing activation with
  | nil => simp [pendingStackBinders]
  | cons frame rest ih =>
    have frameBelow := savedBelow frame (by simp)
    have restBelow : StackActivationsBelow limit rest := by
      intro next member
      exact savedBelow next (List.mem_cons_of_mem frame member)
    have nextBelow : frameCallerActivation activation frame < limit := by
      cases frame <;> simp_all [frameCallerActivation, FrameActivationBelow]
    intro binding member
    simp only [pendingStackBinders, List.mem_append, List.mem_map] at member
    rcases member with ⟨binder, _, same⟩ | later
    · subst binding
      exact activeBelow
    · exact ih _ nextBelow restBelow binding later

theorem activation_binders_nodup (activation : Nat) (binders : List BinderId)
    (unique : binders.Nodup) : (binders.map (fun binder => (activation, binder))).Nodup := by
  induction unique with
  | nil => simp
  | @cons binder rest absent unique ih =>
    simp only [List.map_cons, List.nodup_cons]
    refine ⟨?_, ih⟩
    intro member
    obtain ⟨other, otherMember, equal⟩ := List.mem_map.mp member
    have same : other = binder := (congrArg Prod.snd equal)
    subst other
    exact (absent binder otherMember) rfl

theorem call_entry_preserves_pending (base : Machine) (target : Value) (arguments : List Value)
    (function : Function) (parameterTypes resultType : RuntimeType) (cells : ActiveCells) (store : Store)
    (solution : Substitution)
    (bound : bindParameters base.nextActivation function.parameters arguments parameterTypes base.store = .ok (cells, store))
    (bodyUnique : (declaredBinders function.body).Nodup)
    (disjoint : ∀ binder ∈ declaredBinders function.body, binder ∉ function.parameters)
    (activation : ActivationInvariant { base with control := .invoke target arguments })
    (pending : PendingBindingsSafe { base with control := .invoke target arguments }) :
    PendingBindingsSafe { base with control := .statements function.body, cells := withGlobals cells base.globals, store := store, activation := base.nextActivation, nextActivation := base.nextActivation + 1, solution := solution, stack := .caller base.cells base.activation base.solution resultType :: base.stack } := by
  have oldUnique : (pendingStackBinders base.activation base.stack).Nodup := by
    simpa [pendingMachineBinders, pendingControlBinders] using pending.unique
  have oldFresh : ∀ binding ∈ pendingStackBinders base.activation base.stack,
      base.store ⟨binding.1, binding.2⟩ = none := by
    simpa [pendingMachineBinders, pendingControlBinders] using pending.fresh
  have oldBelow := pending_stack_activation_below base.activation base.nextActivation base.stack activation.active activation.saved
  have newFresh : ∀ binder ∈ declaredBinders function.body, store ⟨base.nextActivation, binder⟩ = none := by
    intro binder member
    have beforeEmpty : base.store ⟨base.nextActivation, binder⟩ = none := by
      cases found : base.store ⟨base.nextActivation, binder⟩ with
      | none => rfl
      | some entry =>
        have impossible := activation.store ⟨base.nextActivation, binder⟩ entry found
        exact False.elim (Nat.lt_irrefl _ impossible)
    rw [parameter_binding_preserves_other_cell base.nextActivation function.parameters arguments parameterTypes
      base.store store cells ⟨base.nextActivation, binder⟩ (Or.inr (disjoint binder member)) bound]
    exact beforeEmpty
  have retainedFresh : ∀ binding ∈ pendingStackBinders base.activation base.stack,
      store ⟨binding.1, binding.2⟩ = none := by
    intro binding member
    have different : binding.1 ≠ base.nextActivation := Nat.ne_of_lt (oldBelow binding member)
    rw [parameter_binding_preserves_other_cell base.nextActivation function.parameters arguments parameterTypes
      base.store store cells ⟨binding.1, binding.2⟩ (Or.inl different) bound]
    exact oldFresh binding member
  constructor
  · simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      frameCallerActivation, List.map_nil, List.nil_append, List.nodup_append]
    refine ⟨activation_binders_nodup base.nextActivation (declaredBinders function.body) bodyUnique, oldUnique, ?_⟩
    intro newBinding newMember oldBinding oldMember equal
    obtain ⟨binder, _, same⟩ := List.mem_map.mp newMember
    have below := oldBelow oldBinding oldMember
    have newActivation : base.nextActivation = newBinding.1 := congrArg Prod.fst same
    have equalActivation : newBinding.1 = oldBinding.1 := congrArg Prod.fst equal
    exact (Nat.ne_of_lt below) (equalActivation.symm.trans newActivation.symm)
  · intro binding member
    simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      frameCallerActivation, List.map_nil, List.nil_append, List.mem_append] at member
    rcases member with newMember | oldMember
    · obtain ⟨binder, binderMember, same⟩ := List.mem_map.mp newMember
      subst binding
      exact newFresh binder binderMember
    · exact retainedFresh binding oldMember

theorem function_entry_storage_agreement (state : Machine) (solution : Substitution)
    (parameters : Scope) (values : List Value) (types : RuntimeType) (locals : ActiveCells) (store : Store)
    (storage : BindingStorageMap) (typed : ParameterBindingsTyped solution parameters values types)
    (unique : (parameters.map Binding.binder).Nodup)
    (parametersOrdinary : ∀ binder ∈ parameters.map Binding.binder, storage binder = .ordinary)
    (bound : bindParameters state.nextActivation (parameters.map Binding.binder) values types state.store = .ok (locals, store))
    (globalLive : ActiveCellsPresent state.globals state.store)
    (globalSchemes : SchemeCellsImmutable state.globals state.store)
    (globalStorage : StorageAgreement state.globals state.store storage) :
    StorageAgreement (withGlobals locals state.globals) store storage := by
  have parameterCells := bound_parameters_agree state.nextActivation (parameters.map Binding.binder) values types
    state.store store locals (parameter_bindings_have_values solution parameters values types typed) unique bound
  have layout := parameters_preserve_layout state.nextActivation (parameters.map Binding.binder)
    values types state.store store locals bound
  have extension := parameters_preserve_existing state.nextActivation (parameters.map Binding.binder)
    values types state.store store locals bound
  have globalAfter := storage_agreement_survives_stable_store state.globals storage state.store store
    globalLive globalSchemes globalStorage layout (store_extension_preserves_immutable extension)
  constructor
  · intro binder declaration scheme found
    cases localFound : locals binder with
    | some cell =>
      have member := bound_parameter_cell_is_listed state.nextActivation (parameters.map Binding.binder)
        values types state.store store locals bound binder cell localFound
      obtain ⟨value, localRead, ordinary⟩ := parameter_binder_reads_ordinary solution state.nextActivation
        parameters values types locals store typed parameterCells binder member
      have mergedRead : Connected.read store (withGlobals locals state.globals) binder = some value := by
        simpa [Connected.read, withGlobals, localFound] using localRead
      rw [found] at mergedRead
      cases mergedRead
      simp [ordinaryValue] at ordinary
    | none =>
      have globalRead : Connected.read store state.globals binder = some (.schemeFunction declaration scheme) := by
        simpa [Connected.read, withGlobals, localFound] using found
      exact globalAfter.1 binder declaration scheme globalRead
  · intro binder isScheme
    have localMissing : locals binder = none := by
      cases localFound : locals binder with
      | none => rfl
      | some cell =>
        have member := bound_parameter_cell_is_listed state.nextActivation (parameters.map Binding.binder)
          values types state.store store locals bound binder cell localFound
        have ordinary := parametersOrdinary binder member
        rw [ordinary] at isScheme
        cases isScheme
    obtain ⟨declaration, scheme, globalRead⟩ := globalAfter.2 binder isScheme
    exact ⟨declaration, scheme, by simpa [Connected.read, withGlobals, localMissing] using globalRead⟩

theorem schema_matches_after_parameters (solution : Substitution) (parameters globals : Scope)
    (types : RuntimeType) (schemas : SchemaTable) (typed : ParameterTypesTyped solution parameters types)
    (globalSchemas : SchemaMatches globals solution schemas) : SchemaMatches (parameters ++ globals) solution schemas := by
  induction typed with
  | nil => exact globalSchemas
  | @cons name binder raw rest ty types closed typed ih =>
    simpa using compatible_schema_matches_after_binding (rest ++ globals) solution schemas name binder .immutable raw (Or.inl rfl) ih

theorem parameter_cells_are_immutable (activation : Nat) (cells : ActiveCells) (store : Store)
    (binders : List Xsh.Foundation.BinderId) (values : List Value) (types : RuntimeType)
    (agrees : ParameterCellsAgree activation cells store binders values types)
    (binder : Xsh.Foundation.BinderId) (member : binder ∈ binders) :
    ∃ entry, store ⟨activation, binder⟩ = some entry ∧ entry.mutability = .immutable := by
  induction agrees with
  | nil => simp at member
  | @cons first rest value values ty types active present remaining ih =>
    simp only [List.mem_cons] at member
    rcases member with same | later
    · subst binder
      obtain ⟨entry, atCell, immutable, _, _⟩ := present
      exact ⟨entry, atCell, immutable⟩
    · exact ih later

theorem function_entry_writable_contract (state : Machine) (solution : Substitution)
    (parameters globals : Scope) (values : List Value) (types : RuntimeType) (locals : ActiveCells) (store : Store)
    (typed : ParameterBindingsTyped solution parameters values types)
    (unique : (parameters.map Binding.binder).Nodup)
    (disjoint : ∀ binding ∈ globals, binding.binder ∉ parameters.map Binding.binder)
    (bound : bindParameters state.nextActivation (parameters.map Binding.binder) values types state.store = .ok (locals, store))
    (globalLive : ActiveCellsPresent state.globals state.store)
    (globalContract : ScopeWritableContract globals solution state.globals state.store) :
    ScopeWritableContract (parameters ++ globals) solution (withGlobals locals state.globals) store := by
  have parameterCells := bound_parameters_agree state.nextActivation (parameters.map Binding.binder) values types
    state.store store locals (parameter_bindings_have_values solution parameters values types typed) unique bound
  have extension := parameters_preserve_existing state.nextActivation (parameters.map Binding.binder)
    values types state.store store locals bound
  intro name binding cell entry found notSchema active present mutable value valueTyped
  cases localFound : parameters.find? (fun item => decide (item.name = name)) with
  | some localBinding =>
    have same : localBinding = binding := by simpa [List.find?_append, localFound] using found
    subst localBinding
    have member : binding.binder ∈ parameters.map Binding.binder :=
      List.mem_map.mpr ⟨binding, List.mem_of_find?_eq_some localFound, rfl⟩
    obtain ⟨expected, immutableEntry, immutableAt⟩ :=
      parameter_cell_for_binder state.nextActivation locals store (parameters.map Binding.binder) values types
        parameterCells binding.binder member
    have actual : cell = ⟨state.nextActivation, binding.binder⟩ := by
      have localActive : withGlobals locals state.globals binding.binder = some ⟨state.nextActivation, binding.binder⟩ := by
        simp [withGlobals, expected]
      exact Option.some.inj (active.symm.trans localActive)
    subst cell
    obtain ⟨old, oldAt, immutable⟩ := parameter_cells_are_immutable state.nextActivation locals store
      (parameters.map Binding.binder) values types parameterCells binding.binder member
    rw [present] at oldAt
    cases oldAt
    rw [mutable] at immutable
    cases immutable
  | none =>
    have globalFound : globals.find? (fun item => decide (item.name = name)) = some binding := by
      simpa [List.find?_append, localFound] using found
    have notParameter := disjoint binding (List.mem_of_find?_eq_some globalFound)
    have localMissing : locals binding.binder = none := by
      cases localActive : locals binding.binder with
      | none => rfl
      | some localCell =>
        exact False.elim (notParameter (bound_parameter_cell_is_listed state.nextActivation
          (parameters.map Binding.binder) values types state.store store locals bound binding.binder localCell localActive))
    have oldActive : state.globals binding.binder = some cell := by simpa [withGlobals, localMissing] using active
    obtain ⟨old, oldAt⟩ := globalLive binding.binder cell oldActive
    have oldRetained := extension cell old oldAt
    rw [present] at oldRetained
    cases oldRetained
    exact globalContract name binding cell entry globalFound notSchema oldActive oldAt mutable value valueTyped

theorem typed_machine_function_call (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (declaration : Xsh.Syntax.Foundation.DeclarationId) (arguments : List Value)
    (parameterTypes resultType : RuntimeType) (captured : Substitution)
    (typeArguments normalized : List (Variable × MonoType)) (function : Function) (cells : ActiveCells) (store : Store)
    (found : base.functions declaration = some (.function function))
    (signature : instantiateFunction captured function.scheme typeArguments = some (parameterTypes, resultType, normalized))
    (bound : bindParameters base.nextActivation function.parameters arguments parameterTypes base.store = .ok (cells, store))
    (typed : TypedMachine globals storage types
      { base with control := .invoke (.instantiatedFunction declaration parameterTypes resultType captured typeArguments) arguments }) :
    TypedMachine globals storage types
      { base with control := .statements function.body, cells := withGlobals cells base.globals, store := store, activation := base.nextActivation, nextActivation := base.nextActivation + 1, solution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration), stack := .caller base.cells base.activation base.solution resultType :: base.stack } := by
  let runtimeSolution := compose captured (compose (schemeSubstitution function.scheme (witnessSubstitution typeArguments)) function.elaboration)
  have step := Step.callFunction base declaration arguments parameterTypes resultType captured typeArguments normalized function cells store found signature bound
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSame, activationSame, storageSame, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | invoke targetTyped callable argumentsTyped callerResult =>
    rename_i callerContext callerArgumentType callerResultType callerType
    simp only [TypedDemand.context] at runtimeSame activationSame storageSame proofSorted
    simp [valueType] at targetTyped
    obtain ⟨rfl, rfl⟩ := targetTyped
    have capturedSorted : WellSorted captured := by
      cases callable with
      | instantiatedFunction found' sorted signature' => exact sorted
      | instantiatedOk found' => rw [found] at found'; cases found'
      | instantiatedError found' => rw [found] at found'; cases found'
      | instantiatedTime found' => rw [found] at found'; cases found'
    obtain ⟨extension, parameters, bodyResult, extensionSorted, runtimeSorted', solutionSorted, globalValues,
      globalWritable, globalMutable, globalSchemas, parametersOrdinary, localsOrdinary, binders, unique, parameterTypesTyped,
      bodyTyped, resultClosed, bodyUnique, disjoint, propagation⟩ :=
      typed.functions declaration function found captured typeArguments parameterTypes resultType normalized capturedSorted signature
    let solution := compose extension runtimeSolution
    have parameterBindings := runtime_arguments_match_parameter_bindings base.functions solution parameters arguments parameterTypes
      parameterTypesTyped argumentsTyped
    have boundParameters : bindParameters base.nextActivation (parameters.map Binding.binder) arguments parameterTypes base.store = .ok (cells, store) := by
      simpa [binders] using bound
    have parameterUnique : (parameters.map Binding.binder).Nodup := (List.nodup_append.mp (by simpa using unique)).1
    have ordinaryParameters : ∀ binder ∈ parameters.map Binding.binder, storage binder = .ordinary := by
      simpa [binders] using parametersOrdinary
    have baseMemory : MachineMemoryInvariant types base :=
      ⟨⟨typed.memory.activation.active, typed.memory.activation.saved, typed.memory.activation.store⟩,
        ⟨typed.memory.globals.typed, typed.memory.globals.live⟩⟩
    have scopeAfter := parameter_global_scope_agrees base types solution parameters globals arguments parameterTypes cells store
      baseMemory parameterBindings boundParameters unique globalValues globalMutable typed.globalDistinct
    have globalDisjoint : ∀ binding ∈ globals, binding.binder ∉ parameters.map Binding.binder := by
      intro binding member
      have both : (parameters.map Binding.binder ++ globals.map Binding.binder).Nodup := by simpa using unique
      have separation := (List.nodup_append.mp both).2.2
      intro inParameters
      exact separation _ inParameters _ (List.mem_map.mpr ⟨binding, member, rfl⟩) rfl
    have writableAfter := function_entry_writable_contract base solution parameters globals arguments parameterTypes cells store
      parameterBindings parameterUnique globalDisjoint boundParameters typed.memory.globals.live globalWritable
    have schemesAfter := function_entry_scheme_cells_immutable base solution parameters arguments parameterTypes cells store
      parameterBindings parameterUnique boundParameters typed.memory.globals.live typed.globalSchemes
    have storageAfter := function_entry_storage_agreement base solution parameters arguments parameterTypes cells store storage
      parameterBindings parameterUnique ordinaryParameters boundParameters typed.memory.globals.live typed.globalSchemes typed.globalStorageAgrees
    have schemasAfter := schema_matches_after_parameters solution parameters globals parameterTypes base.schemas parameterTypesTyped globalSchemas
    have newPending := call_entry_preserves_pending base (.instantiatedFunction declaration parameterTypes resultType captured typeArguments)
      arguments function parameterTypes resultType cells store runtimeSolution bound bodyUnique disjoint typed.memory.activation pending
    have bodyFresh : FutureBindingsFresh store base.nextActivation function.body := by
      intro binder member
      apply newPending.fresh (base.nextActivation, binder)
      simp [pendingMachineBinders, pendingControlBinders, member]
    obtain ⟨callerScope, callerWritable, callerSchemas, callerSchemes, callerStorage⟩ := active
    obtain ⟨savedScope, savedWritable, savedSchemes, savedStorage, savedSchemas⟩ :=
      saved_frame_scope_survives_step step callerContext base.cells callerScope callerWritable callerSchemes callerStorage callerSchemas
    let innerContext : TypingContext := ⟨parameters ++ globals, storage, solution, runtimeSolution, bodyResult, some bodyResult, base.nextActivation, ⟨resultType, resultClosed⟩⟩
    have oldStack : StackTyped
        { base with control := .statements function.body, cells := withGlobals cells base.globals, store := store, activation := base.nextActivation, nextActivation := base.nextActivation + 1, solution := runtimeSolution, stack := .caller base.cells base.activation base.solution resultType :: base.stack }
        (.expression callerContext callerType) output base.stack := by
      apply stack_typing_survives_pending step stack
      intro binding member
      apply newPending.fresh binding
      simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
        frameCallerActivation, List.map_nil, List.nil_append, List.mem_append]
      right
      simpa [TypedDemand.context, activationSame] using member
    have callerFrame : FrameTyped
        { base with control := .statements function.body, cells := withGlobals cells base.globals, store := store, activation := base.nextActivation, nextActivation := base.nextActivation + 1, solution := runtimeSolution, stack := .caller base.cells base.activation base.solution resultType :: base.stack }
        (.block innerContext) (.expression callerContext callerType) (.caller base.cells base.activation base.solution resultType) := by
      rw [← runtimeSame]
      refine FrameTyped.caller (inner := innerContext) (outer := callerContext) ?_ proofSorted
        (by simpa [runtimeSame] using runtimeSorted) ?_ rfl resultClosed callerResult savedScope savedWritable savedSchemes savedStorage savedSchemas
      · simpa [innerContext, TypedDemand.context] using storageSame.symm
      · simpa [TypedDemand.context] using activationSame
    refine ⟨.block innerContext, output, rfl, rfl, rfl, solutionSorted, runtimeSorted', ?_, ?_, outputClosed,
      .statements bodyTyped bodyFresh, .cons callerFrame inputClosed oldStack, newPending⟩
    · exact ⟨by simpa [innerContext, ScopeStateAgrees, TypedDemand.context] using scopeAfter,
        writableAfter, schemasAfter, schemesAfter, storageAfter⟩
    · exact ⟨⟨resultType, resultClosed⟩, by intro ty same; cases same; exact ⟨resultType, resultClosed⟩⟩

end Xsh.Semantics.Connected
