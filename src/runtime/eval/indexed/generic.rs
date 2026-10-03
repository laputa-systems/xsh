use crate::sema::types::Type;
use super::semantic::{SemanticPools, TypeTag};
use super::{IrFunctionId, IrVerifyError, SignatureId, TypeId as GroundTypeId};
use crate::symbol::Name;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

mod callable_types;
mod scoped_invocations;
mod iterations;
mod records;
mod comprehensions;
mod context_producers;
mod stages;
mod durations;
mod native_receivers;
mod native_results;
mod native_record_arguments;
mod native_arguments;
mod record_get;
pub(in crate::runtime::eval) use native_arguments::PreparedNativeArgumentLineage;
pub(in crate::runtime::eval) use record_get::PreparedNativeRecordGet;
pub(in crate::runtime::eval) use native_record_arguments::PreparedNativeRecordFieldArgument;
mod ranges;
pub(in crate::runtime::eval) use ranges::PreparedRangeLowering;
pub(in crate::runtime::eval) use native_results::{PreparedNativeResultRecord, PreparedNativeRecordCarrier};
mod constants;
mod record_updates;
mod conditionals;
mod paths;
pub(in crate::runtime::eval) use paths::{FormattedPathId, FormattedPathPart, PreparedFormattedPath};
pub(in crate::runtime::eval) use record_updates::{PreparedRecordReplacement, PreparedRecordUpdateSource, PreparedUpdateProjection, PreparedUpdateValue, PreparedUpdateWrapper, RecordUpdateSourceId};
pub(in crate::runtime::eval) use conditionals::{ConditionalSourceId, ConditionalKind, ConditionalValue, ConditionalTerminalValue, ConditionalBody, ConditionalArm, ConditionalResultSource};
mod host_bindings;
mod lexical_captures;
mod mutable_paths;
mod error_constructors;
mod tag_constructors;
mod bridges;
pub(in crate::runtime::eval) use bridges::{PreparedBridgeCall, PreparedHashPolicyCall};
mod fs_root_methods;
mod process_producers;
pub(in crate::runtime::eval) use process_producers::{PreparedProcessCommandArgv, PreparedProcessArgvRow};
pub(in crate::runtime::eval) use fs_root_methods::is_fs_root_method_owner;
pub(in crate::runtime::eval) use native_receivers::PreparedNativeReceiverTransport;
pub(in crate::runtime::eval) use stages::{OriginalPreparedStage, OriginalStagePipeline, OriginalStageBlockCallback, OriginalStageFusion, OriginalStageIdentityFlatMap, PreparedStageResultRecord};
pub(in crate::runtime::eval) use constants::{OriginalConstantSource, PreparedConstantSource, ConstantSourceId};
pub(in crate::runtime::eval) use host_bindings::{HostBindingSourceId, HostBindingSource, HostBindingCaptureId, HostBindingCapture};
pub(in crate::runtime::eval) use lexical_captures::{LexicalCaptureId, LexicalCapture, LexicalCaptureSourceId, LexicalCaptureSource};
pub(in crate::runtime::eval) use mutable_paths::{MutablePathEncoding, MutablePathReceipt, MutablePathStep, MutablePathCompound};
pub(in crate::runtime::eval) use error_constructors::PreparedErrorConstructor;
pub(in crate::runtime::eval) use tag_constructors::{PreparedTagConstructor, ScopedTagConstructorRequirement};
mod cli_calls;
mod captures;
mod compiler_wrappers;
mod indexing;
mod containers;
mod native_scalars;
mod mutable_bindings;
mod record_constructors;
pub(in crate::runtime::eval) use native_scalars::{ByteAtFallbackComposite, NativeScalarReceiver, NativeScalarSource};
pub(in crate::runtime::eval) use mutable_bindings::{MutableNominalInvariant, MutableBindingReceipt, MutableCompoundAssignment, MutableDriverReceipt, MutableReadRefinement};
pub(in crate::runtime::eval) use record_constructors::{PreparedRecordConstructor, RecordConstructorRow, RecordConstructorId, RecordConstructorSpread};
pub(in crate::runtime::eval) use containers::{GroundContainerCreationCheck, ContainerKind, ContainerOperandOrigin, ContainerOperandRole, GroundContainerOperand, GroundContainerSource, OriginalNamedMapKey};
pub(in crate::runtime::eval) use indexing::OriginalIndex;
pub(in crate::runtime::eval) use context_producers::{PreparedSpawnRunSource, ContextProducerRoot, PreparedContextProducer, PreparedRunProducer, RunProducerOperand, RunProducerArgument, PreparedRunPacket, PreparedRunEnvironment, PreparedRunStdin};
pub(in crate::runtime::eval) use cli_calls::{PreparedCliDescriptor, CliDescriptorRow};
pub(in crate::runtime::eval) use captures::{RetryCaptureDelay, RetryCapturePolicy, TryCaptureSourceId, TryCaptureSource};
pub(in crate::runtime::eval) use compiler_wrappers::{OriginalCompilerArgumentWrapper, OriginalOptionalReceiverGuard};
pub(in crate::runtime::eval) use records::{RecordChildWrapper, PreparedRecordEntry, PreparedRecordSource, RecordEntryKind, RecordSourceId};
pub(in crate::runtime::eval) use comprehensions::{ComprehensionRoot, ComprehensionGenerator, ComprehensionBinding, ComprehensionTarget, PreparedComprehension};
mod operation_requirements;
mod eligibility_requirements;
pub(in crate::runtime::eval) use eligibility_requirements::OriginalScopedEligibility;
mod value_bindings;
mod native_callables;
mod callable_receivers;
mod module_invocations;
pub(in crate::runtime::eval) use module_invocations::{ModuleExportParameter, ModuleExportContract, ModuleInvocationSource, ModuleReceiverAllocation};
pub(in crate::runtime::eval) use callable_receivers::{OriginalCallableReceiver, CapturedCallableReceiver, OriginalCapturedCallableSource};
pub(in crate::runtime::eval) use native_callables::{ScopedNativeMethodRequirement, ScopedNativeMethodSource, ScopedNativeMethodObligation, ScopedNativeMethodReceiver, ScopedNativeMethodWitness, ScopedNativeMethodSourceId, ScopedNativeMethodWitnessId, NativeCallableContract, NativeCallableSource, PreparedNativeCallableValue, GroundNativeInvocationContract, NativeInvocationSource, PreparedNativeInvocationPlan};
pub(in crate::runtime::eval) use value_bindings::{ValueFieldPresence, ValueBindingIdentity, ValueBindingAllocation, ValueBindingSourceId, ValueBindingId, ValueBindingContract, ValueInitializerWrapper, ValueInitializerWrapperKind, ValueBindingSource, PreparedValueBinding, ValueBindingUse};
pub(in crate::runtime::eval) use operation_requirements::{ScopedOperationRequirement, ScopedOperationSource, ScopedOperationObligation, ScopedOperationWitness, ScopedOperationCode, ScopedOperationSourceId, ScopedOperationWitnessId};
pub(in crate::runtime::eval) use iterations::{OriginalIterationBinding, OriginalIterationUse, OriginalIterationProducer, OriginalLineIterationSource, OriginalLineScan, OriginalLineScanCheck, OriginalLineScanOperation};
mod result_receivers;
pub(in crate::runtime::eval) use result_receivers::PreparedResultReceiver;
pub(in crate::runtime::eval) use scoped_invocations::{TemplateInvocationArgument, ScopedInvocationSource, ScopedInvocationObligation, ScopedInvocationWitness, UserInvocationAuthority};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

/// A proof belongs to one immutable program. Serials are never reused after rewind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct OwnerProof {
    root: u64,
    serial: u64,
}

macro_rules! evidence_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub(in crate::runtime::eval) struct $name {
            index: u32,
            proof: OwnerProof,
        }
    };
}

evidence_id!(IterationBindingId);
evidence_id!(SchemeScopeId);
evidence_id!(PhysicalLayoutId);
evidence_id!(InstantiationId);
evidence_id!(ForwardingId);
evidence_id!(TypeTemplateId);
evidence_id!(OperationSourceId);
evidence_id!(OperationId);
evidence_id!(CheckedFunctionId);
evidence_id!(StageCallSourceId);
evidence_id!(GroundStageCallId);
evidence_id!(CallableValueSourceId);
evidence_id!(CallableValueId);
evidence_id!(InvocationSourceId);
evidence_id!(InvocationPlanId);
evidence_id!(ScopedInvocationSourceId);
evidence_id!(ScopedInvocationWitnessId);
evidence_id!(GroundProjectionSourceId);
evidence_id!(GroundProjectionId);
evidence_id!(PatternSourceId);
evidence_id!(PatternApplicationId);
evidence_id!(PatternCaptureId);
evidence_id!(NativeCallSourceId);
evidence_id!(GroundNativeCallId);
evidence_id!(NativeCallableValueId);
evidence_id!(NativeInvocationPlanId);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum TypeRef {
    Ground(GroundTypeId),
    Rigid(u32),
    Template(TypeTemplateId),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum QuantifierKind { Type, Row }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum ParameterMode { PositionalOrNamed, NamedOnly }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum CallableKind { Pure, Proc }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct TemplateParameter { pub label: Name, pub ty: TypeRef, pub mode: ParameterMode, pub defaulted: bool, pub rest: bool }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum TypeTemplate {
    Optional(TypeRef),
    List(TypeRef),
    Stream(TypeRef),
    Map { key: TypeRef, value: TypeRef },
    Result { ok: TypeRef, error: TypeRef },
    Record { fields: Box<[(Name, TypeRef)]>, row_tail: Option<u32> },
    Arrow { kind: CallableKind, parameters: Box<[TemplateParameter]>, result: TypeRef, effects: Box<[crate::syntax::node::Effect]> },
}

/// The declaration fixes wrapping before any concrete payload is supplied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum GenericReturnPlan {
    Value,
    Result,
    Unit,
    ResultUnit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum Requirement {
    Operation(ScopedOperationRequirement),
    NativeMethod(ScopedNativeMethodRequirement),
    TagConstructor(ScopedTagConstructorRequirement),
    Eligibility { predicate: crate::sema::inference::Eligibility, ty: TypeRef },
    Projection { receiver: TypeRef, receiver_parameter: u32, field: Name, result: TypeRef },
    Add { left: TypeRef, right: TypeRef, result: TypeRef },
    Invocation { callable: TypeRef, arguments: Box<[TemplateInvocationArgument]>, result: TypeRef, domain: crate::sema::inference::CallableDomain },
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct SchemeScope {
    pub owner: IrFunctionId,
    pub quantifiers: Box<[QuantifierKind]>,
    pub parameters: Box<[TypeRef]>,
    pub parameter_names: Box<[Name]>,
    pub parameter_flags: Box<[u8]>,
    pub kind: CallableKind,
    pub result: TypeRef,
    pub requirements: Box<[Requirement]>,
    pub return_plan: GenericReturnPlan,
}

/// Field order describes the actual constructor storage, independently of semantic shape order.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PhysicalLayout {
    pub record_type: GroundTypeId,
    pub fields: Box<[(Name, GroundTypeId)]>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum ConcreteOperationId {
    AddInt,
    AddFloat,
    AddStr,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum RequirementWitness {
    Operation(ScopedOperationWitnessId),
    NativeMethod(ScopedNativeMethodWitnessId),
    TagConstructor,
    Eligibility { predicate: crate::sema::inference::Eligibility, ty: GroundTypeId },
    Invocation(ScopedInvocationWitnessId),
    Projection { layout: PhysicalLayoutId, field_slot: u32, result: GroundTypeId },
    Add { operation: ConcreteOperationId, left: GroundTypeId, right: GroundTypeId, result: GroundTypeId },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct Instantiation {
    pub scope: SchemeScopeId,
    pub substitutions: Box<[GroundTypeId]>,
    pub parameter_types: Box<[GroundTypeId]>,
    pub result_type: GroundTypeId,
    pub requirements: Box<[RequirementWitness]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ForwardedRequirement {
    Caller(u32),
    Fixed(RequirementWitness),
}

/// All concrete forwarding edges are prepared once; execution selects a stored edge.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ForwardingPlan {
    pub caller: SchemeScopeId,
    pub callee: SchemeScopeId,
    pub substitutions: Box<[TypeRef]>,
    pub requirements: Box<[ForwardedRequirement]>,
    pub instances: Box<[(InstantiationId, InstantiationId)]>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) enum CallEvidence {
    Ground(InstantiationId),
    Forwarded(ForwardingId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum InstructionOwner {
    Function(IrFunctionId),
    Driver(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum OperationSourceOrigin {
    Expression(crate::sema::check::ExpressionIdentity),
    Statement(crate::sema::check::StatementIdentity),
    Stage(crate::sema::check::StageIdentity),
    Comprehension(crate::sema::check::ComprehensionIdentity),
    Run { source: crate::source::SourceId, namespace: Option<Name>, run: crate::syntax::arena::RunFormId },
    ConstructorDefault(crate::sema::check::ConstructorDefaultIdentity),
}

/// Source registration is independent of the proof that must satisfy it.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OperationSource {
    pub origin: OperationSourceOrigin,
    pub identity: Name,
    pub expected: PreparedOperationAuthority,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum PreparedOperationAuthority {
    Sealed { operation: crate::sema::inference::SealedOperation },
    Language { identity: Name, authority: &'static str, operation: crate::sema::operation_graph::PreparedLanguageOperation,
        argument_order: crate::sema::operation_graph::OperationArgumentOrder, statement_result_is_unit: bool },
    Registry { identity: Name, operation: crate::modules::RuntimeOp, binding: crate::modules::signature::ImplBinding,
        argument_check: crate::modules::signature::ApiArgCheck, semantic_rule: crate::modules::signature::SemanticRule,
        lifecycle: crate::sema::registry_graph::RegistryLifecycle, producer_transfer: crate::sema::registry_graph::RegistryProducerTransferPlan },
    Stage { identity: Name, stage: crate::syntax::node::StreamStageKind, source: crate::sema::stage_graph::StageSource,
        form: crate::sema::stage_graph::StageForm, variant: crate::sema::stage_graph::StageVariant,
        callback_slot: Option<usize>, additional_producer: Option<crate::sema::stage_graph::AdditionalProducer> },
}

impl PreparedOperationAuthority {
    pub fn identity(&self) -> Name { match self {
        Self::Sealed { operation } => Name::intern(match operation {
            crate::sema::inference::SealedOperation::AddInt => "sealed.add.Int",
            crate::sema::inference::SealedOperation::AddFloat => "sealed.add.Float",
            crate::sema::inference::SealedOperation::AddStr => "sealed.add.Str",
            crate::sema::inference::SealedOperation::AddDuration => "sealed.add.Duration",
            crate::sema::inference::SealedOperation::AddList => "sealed.add.List",
        }),
        Self::Language { identity, .. } | Self::Registry { identity, .. } | Self::Stage { identity, .. } => *identity,
    } }
    fn retained_bytes(&self) -> usize {
        match self {
            Self::Registry { producer_transfer: crate::sema::registry_graph::RegistryProducerTransferPlan::Transfers(transfers), .. } =>
                transfers.len() * std::mem::size_of::<crate::sema::registry_graph::RegistryProducerTransfer>()
                    + transfers.iter().map(|transfer| (transfer.input_path.len() + transfer.output_path.len()) * std::mem::size_of::<crate::sema::registry_graph::RegistryProducerComponent>()).sum::<usize>(),
            _ => 0,
        }
    }
}

/// These summaries are closed checked facts; latent effect scopes require
/// their own prepared substitution before entering this representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedOperationEffects {
    pub creation: crate::sema::inference::EffectSet,
    pub inputs: Box<[(crate::sema::inference::EffectRole, crate::sema::inference::EffectSet)]>,
    pub outputs: Box<[(crate::sema::inference::ProducerRole, crate::sema::inference::EffectSet)]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedOperationBinding {
    pub supplied_slots: Box<[u32]>,
    pub default_slots: Box<[u32]>,
    pub rest_slot: Option<u32>,
    pub dynamic: Option<crate::sema::inference::DynamicInvocationBinding>,
    pub operands: Box<[u32]>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedOperation {
    pub source: OperationSourceId,
    pub authority: PreparedOperationAuthority,
    pub receiver: Option<TypeRef>,
    pub arguments: Box<[Option<TypeRef>]>,
    pub result: TypeRef,
    pub effects: PreparedOperationEffects,
    pub binding: PreparedOperationBinding,
    pub tag_equality: Option<super::full::PreparedTagEquality>,
    pub fallback_lowering: Option<PreparedFallbackLowering>,
    pub original_integer_addition: Option<super::full::PreparedIntegerAddition>,
    pub range_lowering: Option<PreparedRangeLowering>,
    pub literal_comparison_slot: Option<super::full::PreparedLiteralComparisonSlot>,
    pub membership_lowering: Option<super::full::PreparedMembershipLowering>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedNativeReceiver {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub source_instruction: u32,
    pub source_wrappers: Box<[ValueInitializerWrapper]>,
    pub source_type: TypeRef,
    pub ty: TypeRef,
    pub method_name: Name,
    pub saved: Option<PreparedNativeReceiverTransport>,
    pub postfix: Option<PreparedResultReceiver>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum PreparedFallbackLowering {
    Result { instruction_payload: Box<[u32]>, creation_check: Option<super::full::PreparedFallbackCreationCheck> },
    Optional {
        instruction_payload: Box<[u32]>, arms_flags: u8, arms_payload: Box<[u32]>,
        null_pattern_payload: Box<[u32]>, present_pattern_payload: Box<[u32]>, present_payload: Box<[u32]>,
        creation_check: Option<super::full::PreparedFallbackCreationCheck>,
    },
}

impl PreparedFallbackLowering {
    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        match self {
            Self::Result { instruction_payload, creation_check } => instruction_payload.len() * size_of::<u32>() + creation_check.as_ref().map_or(0, super::full::PreparedFallbackCreationCheck::retained_bytes),
            Self::Optional { instruction_payload, arms_payload, null_pattern_payload, present_pattern_payload, present_payload, creation_check, .. } =>
                (instruction_payload.len() + arms_payload.len() + null_pattern_payload.len() + present_pattern_payload.len() + present_payload.len()) * size_of::<u32>() + creation_check.as_ref().map_or(0, super::full::PreparedFallbackCreationCheck::retained_bytes),
        }
    }
}

/// The native call retains the selected original signature and actual written
/// operands separately from the formal slots, including omitted defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GroundNativeCallContract {
    pub authority: PreparedOperationAuthority,
    pub registry_owner: crate::sema::registry_graph::RegistryOwner,
    pub receiver: Option<PreparedNativeReceiver>,
    pub cli_descriptor: Option<PreparedCliDescriptor>,
    pub process_command_argv: Option<PreparedProcessCommandArgv>,
    pub signature: SignatureId,
    pub kind: CallableKind,
    pub result: TypeRef,
    pub effects: PreparedOperationEffects,
    pub arguments: Box<[PreparedInvocationArgument]>,
    pub binding: PreparedOperationBinding,
    pub argument_sources: Box<[Option<u32>]>,
    pub argument_relations: Box<[crate::sema::inference::ArgumentRelation]>,
    pub input_eligibility: Box<[(usize, crate::sema::inference::Eligibility)]>,
}

impl GroundNativeCallContract {
    fn has_closed_collect_effects(&self, pools: &SemanticPools) -> Result<bool, IrVerifyError> {
        use crate::sema::inference::EffectRole;
        if self.registry_owner != crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Stream)
            || !matches!(self.authority, PreparedOperationAuthority::Registry {
                operation: crate::modules::RuntimeOp::StreamCollect,
                binding: crate::modules::signature::ImplBinding::Native,
                semantic_rule: crate::modules::signature::SemanticRule::Standard,
                lifecycle: crate::sema::registry_graph::RegistryLifecycle::CollectReceiver { materialized: false }, .. })
            || !self.effects.outputs.is_empty() || pools.signature_param_count(self.signature)? != 1 {
            return Ok(false);
        }
        let [(EffectRole::Pull { source: 0 }, pull), (EffectRole::Close { source: 0 }, close)] = self.effects.inputs.as_ref() else { return Ok(false); };
        let Some(receiver) = &self.receiver else { return Ok(false); };
        let TypeRef::Ground(receiver) = receiver.ty else { return Ok(false); };
        let TypeRef::Ground(result) = self.result else { return Ok(false); };
        let (Type::Stream(item), Type::List(result)) = (pools.to_type(receiver)?, pools.to_type(result)?) else { return Ok(false); };
        Ok(item == result && self.effects.creation.0 == pull.0 | close.0)
    }

    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.receiver.as_ref().map_or(0, |receiver| {
            receiver.source_wrappers.len() * size_of::<ValueInitializerWrapper>()
                + receiver.source_wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()
                + receiver.saved.as_ref().map_or(0, PreparedNativeReceiverTransport::retained_bytes)
                + receiver.postfix.as_ref().map_or(0, PreparedResultReceiver::retained_bytes)
        })
            + self.cli_descriptor.as_ref().map_or(0, PreparedCliDescriptor::retained_bytes)
            + self.process_command_argv.as_ref().map_or(0, PreparedProcessCommandArgv::retained_bytes)
            + self.authority.retained_bytes() + self.arguments.len() * size_of::<PreparedInvocationArgument>()
            + self.argument_relations.len() * size_of::<crate::sema::inference::ArgumentRelation>()
            + self.input_eligibility.len() * size_of::<(usize, crate::sema::inference::Eligibility)>()
            + self.argument_sources.len() * size_of::<Option<u32>>()
            + self.binding.operands.len() * size_of::<u32>()
            + (self.binding.supplied_slots.len() + self.binding.default_slots.len()) * size_of::<u32>()
            + self.effects.inputs.len() * size_of::<(crate::sema::inference::EffectRole, crate::sema::inference::EffectSet)>()
            + self.effects.outputs.len() * size_of::<(crate::sema::inference::ProducerRole, crate::sema::inference::EffectSet)>()
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct NativeCallSource {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
    pub expected: GroundNativeCallContract,
    pub result_record_layout: Option<PreparedNativeResultRecord>,
    pub record_arguments: Box<[PreparedNativeRecordFieldArgument]>,
    pub argument_lineages: Box<[PreparedNativeArgumentLineage]>,
    pub result_refinement: Option<PreparedNativeRecordGet>,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedGroundNativeCall {
    pub source: NativeCallSourceId,
    pub contract: GroundNativeCallContract,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct CheckedFunctionSource {
    pub declaration: crate::sema::check::DeclarationIdentity,
    pub target: IrFunctionId,
    pub signature: SignatureId,
}

/// The signature retains the declaration's closed callable contract; the
/// runtime handle supplies its creation environment separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct UserCallableContract {
    pub declaration: crate::sema::check::DeclarationIdentity,
    pub target: IrFunctionId,
    pub signature: SignatureId,
    pub kind: CallableKind,
    pub creation: crate::sema::inference::EffectSet,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct CallableValueSource {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
    pub expected: UserCallableContract,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedCallableValue {
    pub source: CallableValueSourceId,
    pub contract: UserCallableContract,
}

/// Original allocation and initializer identities tie a local callable read to
/// its checked binding even when another binding has the same signature.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalCallableBinding {
    pub binding: crate::sema::check::BindingIdentity,
    pub statement: crate::sema::check::StatementIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub slot: u32,
    pub initializer: u32,
    pub initializer_source: crate::sema::check::ExpressionIdentity,
    pub contract: UserCallableContract,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct OriginalCallableUse {
    pub instruction: u32,
    pub origin: crate::sema::check::ExpressionIdentity,
    pub binding: crate::sema::check::BindingIdentity,
    pub owner: InstructionOwner,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedInvocationArgument {
    pub original: crate::sema::check::SolvedArgumentSource,
    pub instruction: u32,
    pub ty: TypeRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GroundUserInvocationContract {
    pub callee_instruction: u32,
    pub callee_origin: crate::sema::check::ExpressionIdentity,
    pub callable: UserCallableContract,
    pub result: TypeRef,
    pub effects: crate::sema::inference::EffectSet,
    pub arguments: Box<[PreparedInvocationArgument]>,
    pub binding: PreparedOperationBinding,
    pub timing: crate::sema::inference::InvocationDefaultTiming,
}

impl GroundUserInvocationContract {
    fn retained_bytes(&self) -> usize {
        self.arguments.len() * std::mem::size_of::<PreparedInvocationArgument>()
            + (self.binding.supplied_slots.len() + self.binding.default_slots.len() + self.binding.operands.len()) * std::mem::size_of::<u32>()
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct InvocationSource {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
    pub expected: GroundUserInvocationContract,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedInvocationPlan {
    pub source: InvocationSourceId,
    pub contract: GroundUserInvocationContract,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct GroundProjectionSource {
    pub origin: crate::sema::check::ExpressionIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
    pub receiver_origin: crate::sema::check::ExpressionIdentity,
    pub receiver_instruction: u32,
    pub receiver_source_instruction: u32,
    pub receiver_wrappers: Box<[ValueInitializerWrapper]>,
    pub postfix: Option<PreparedResultReceiver>,
    pub field: Name,
    pub receiver: GroundTypeId,
    pub result: GroundTypeId,
    pub layout: PhysicalLayoutId,
    pub field_slot: u32,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedGroundProjection {
    pub source: GroundProjectionSourceId,
    pub receiver_instruction: u32,
    pub receiver: TypeRef,
    pub result: TypeRef,
    pub layout: PhysicalLayoutId,
    pub field_slot: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GroundStageCallContract {
    // Arena stage indices span multiple pipelines. Preserve the original ordinal
    // separately so encoded ordering cannot borrow a callback from another stage.
    pub original_position: u32,
    pub stage_authority: PreparedOperationAuthority,
    pub input_sequence: TypeRef,
    pub input_item: TypeRef,
    pub result_sequence: TypeRef,
    pub result_item: TypeRef,
    pub operation_creation: crate::sema::inference::EffectSet,
    pub declaration: crate::sema::check::DeclarationIdentity,
    pub target: IrFunctionId,
    pub signature: SignatureId,
    pub kind: CallableKind,
    pub argument_types: Box<[GroundTypeId]>,
    pub supplied_slots: Box<[u32]>,
    pub default_slots: Box<[u32]>,
    pub argument_sources: Box<[Option<u32>]>,
    pub timing: crate::sema::inference::InvocationDefaultTiming,
    pub creation: crate::sema::inference::EffectSet,
}

impl GroundStageCallContract {
    fn retained_bytes(&self) -> usize {
        self.stage_authority.retained_bytes() + self.argument_types.len() * std::mem::size_of::<GroundTypeId>()
            + (self.supplied_slots.len() + self.default_slots.len()) * std::mem::size_of::<u32>()
            + self.argument_sources.len() * std::mem::size_of::<Option<u32>>()
    }
}

/// The original declaration and invocation bind independently of decoded call rows.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct StageCallSource {
    pub origin: crate::sema::check::StageIdentity,
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub expected: GroundStageCallContract,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedGroundStageCall {
    pub source: StageCallSourceId,
    pub contract: GroundStageCallContract,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct SolvedCall {
    pub instruction: u32,
    pub caller: InstructionOwner,
    pub target: SchemeScopeId,
    pub evidence: CallEvidence,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct SolvedArgument {
    pub call_instruction: u32,
    pub parameter: u32,
    pub source_instruction: Option<u32>,
    pub ty: TypeRef,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct SolvedRequirementUse {
    pub instruction: u32,
    pub scope: SchemeScopeId,
    pub requirement: u32,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct SolvedRecordLayout {
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub layout: PhysicalLayoutId,
}

/// A compiler argument read keeps the original recipe and its actual binding
/// wrapper separate from the syntax expression evaluated by that wrapper.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct OriginalArgumentBinding {
    pub call: crate::sema::check::ExpressionIdentity,
    pub ordinal: u32,
    pub recipe: crate::sema::check::SolvedArgumentSource,
    pub instruction: u32,
    pub initializer: u32,
    pub initializer_source_instruction: u32,
    pub initializer_wrappers: Box<[ValueInitializerWrapper]>,
    pub slot: u32,
    pub wrapper: u32,
    pub pattern: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
    pub ty: TypeRef,
}

#[derive(Clone, Debug)]
struct Entry<T> {
    serial: u64,
    value: T,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct GenericEvidenceStore {
    native_callables: native_callables::NativeCallableEvidence,
    callable_receivers: callable_receivers::CallableReceiverEvidence,
    module_invocations: module_invocations::ModuleInvocationEvidence,
    values: value_bindings::ValueBindingEvidence,
    eligibility: eligibility_requirements::EligibilityEvidence,
    iterations: iterations::IterationEvidence,
    records: records::RecordEvidence,
    comprehensions: comprehensions::ComprehensionEvidence,
    compiler_wrappers: compiler_wrappers::CompilerWrapperEvidence,
    indices: indexing::IndexEvidence,
    containers: containers::ContainerEvidence,
    record_constructors: record_constructors::RecordConstructorEvidence,
    mutables: mutable_bindings::MutableBindingEvidence,
    native_scalars: native_scalars::NativeScalarEvidence,
    try_captures: captures::TryCaptureEvidence,
    context_producers: context_producers::ContextProducerEvidence,
    stages: stages::StageEvidence,
    error_constructors: error_constructors::ErrorConstructorEvidence,
    tag_constructors: tag_constructors::TagConstructorEvidence,
    bridges: bridges::BridgeEvidence,
    mutable_paths: mutable_paths::MutablePathEvidence,
    host_bindings: host_bindings::HostBindingEvidence,
    lexical_captures: lexical_captures::LexicalCaptureEvidence,
    constants: constants::ConstantEvidence,
    record_updates: record_updates::RecordUpdateEvidence,
    conditionals: conditionals::ConditionalEvidence,
    paths: paths::PathEvidence,
    operation_requirements: operation_requirements::OperationRequirementEvidence,
    original_argument_bindings: Vec<Entry<Arc<OriginalArgumentBinding>>>,
    argument_binding_receipts: Vec<Arc<OriginalArgumentBinding>>,
    original_argument_wrappers: Vec<(u32, u32)>,
    pattern_nominals: Vec<Entry<super::pattern::PreparedPatternNominalMember>>,
    original_callable_bindings: Vec<Entry<OriginalCallableBinding>>,
    original_callable_uses: Vec<Entry<OriginalCallableUse>>,
    native_call_sources: Vec<Entry<Arc<NativeCallSource>>>,
    original_native_call_sources: Vec<Arc<NativeCallSource>>,
    ground_native_calls: Vec<Entry<PreparedGroundNativeCall>>,
    ground_native_call_instructions: Vec<(u32, GroundNativeCallId)>,
    callable_sources: Vec<Entry<CallableValueSource>>,
    callable_values: Vec<Entry<PreparedCallableValue>>,
    callable_instructions: Vec<(u32, CallableValueId)>,
    scoped_invocation_sources: Vec<Entry<Arc<ScopedInvocationSource>>>,
    original_scoped_invocation_sources: Vec<Arc<ScopedInvocationSource>>,
    scoped_invocation_witnesses: Vec<Entry<ScopedInvocationWitness>>,
    scoped_invocation_instructions: Vec<(u32, ScopedInvocationSourceId)>,
    invocation_sources: Vec<Entry<InvocationSource>>,
    invocation_plans: Vec<Entry<PreparedInvocationPlan>>,
    invocation_instructions: Vec<(u32, InvocationPlanId)>,
    ground_projection_sources: Vec<Entry<Arc<GroundProjectionSource>>>,
    original_ground_projection_sources: Vec<Arc<GroundProjectionSource>>,
    ground_projections: Vec<Entry<PreparedGroundProjection>>,
    ground_projection_instructions: Vec<(u32, GroundProjectionId)>,
    pattern_sources: Vec<Entry<Arc<super::pattern::PreparedPatternSource>>>,
    original_pattern_sources: Vec<Arc<super::pattern::PreparedPatternSource>>,
    pattern_applications: Vec<Entry<Arc<super::pattern::PreparedPatternApplication>>>,
    original_pattern_applications: Vec<Arc<super::pattern::PreparedPatternApplication>>,
    pattern_conditional_results: Vec<(u32, PatternApplicationId)>,
    pattern_captures: Vec<Entry<super::pattern::PreparedPatternCapture>>,
    pattern_uses: Vec<super::pattern::PreparedPatternUse>,
    original_pattern_uses: Vec<super::pattern::PreparedPatternUse>,
    instruction_origins: Vec<(u32, OperationSourceOrigin, InstructionOwner)>,
    pattern_origins: Vec<(u32, crate::sema::check::PatternIdentity, InstructionOwner)>,
    root: u64,
    scopes: Vec<Entry<SchemeScope>>,
    layouts: Vec<Entry<PhysicalLayout>>,
    instances: Vec<Entry<Instantiation>>,
    forwarding: Vec<Entry<ForwardingPlan>>,
    templates: Vec<Entry<TypeTemplate>>,
    calls: Vec<SolvedCall>,
    arguments: Vec<SolvedArgument>,
    uses: Vec<SolvedRequirementUse>,
    constructors: Vec<SolvedRecordLayout>,
    function_scopes: Vec<(IrFunctionId, SchemeScopeId)>,
    operation_sources: Vec<Entry<Arc<OperationSource>>>,
    original_operation_sources: Vec<Arc<OperationSource>>,
    operations: Vec<Entry<Arc<PreparedOperation>>>,
    original_operations: Vec<Arc<PreparedOperation>>,
    operation_instructions: Vec<(u32, OperationId)>,
    original_operation_instructions: Vec<(u32, OperationId)>,
    checked_functions: Vec<Entry<CheckedFunctionSource>>,
    checked_function_index: Vec<(crate::sema::check::DeclarationIdentity, CheckedFunctionId)>,
    stage_call_sources: Vec<Entry<StageCallSource>>,
    ground_stage_calls: Vec<Entry<PreparedGroundStageCall>>,
    ground_stage_call_instructions: Vec<(u32, GroundStageCallId)>,
}

fn failure(message: impl Into<String>) -> IrVerifyError { IrVerifyError::new(message) }

fn owned<'a, T>(root: u64, entries: &'a [Entry<T>], index: u32, proof: OwnerProof) -> Result<&'a T, IrVerifyError> {
    if proof.root != root { return Err(failure("generic evidence belongs to a foreign program")); }
    let entry = entries.get(index as usize).ok_or_else(|| failure("generic evidence id is out of bounds"))?;
    if entry.serial != proof.serial { return Err(failure("generic evidence was retired by rewind")); }
    Ok(&entry.value)
}

impl GenericEvidenceStore {
    pub(in crate::runtime::eval) fn program_owner(&self) -> u64 { self.root }
    pub fn pattern_nominal(&self, identity: crate::sema::check::QualifiedNominalIdentity) -> Result<&super::pattern::PreparedPatternNominalMember, IrVerifyError> {
        self.pattern_nominals.binary_search_by_key(&identity, |entry| entry.value.identity).ok().map(|index| &self.pattern_nominals[index].value).ok_or_else(|| failure("original pattern nominal member is missing"))
    }
    pub fn original_callable_binding(&self, binding: crate::sema::check::BindingIdentity) -> Option<&OriginalCallableBinding> {
        self.original_callable_bindings.binary_search_by_key(&binding, |entry| entry.value.binding).ok().map(|index| &self.original_callable_bindings[index].value)
    }
    pub fn original_callable_bindings(&self) -> impl Iterator<Item = &OriginalCallableBinding> { self.original_callable_bindings.iter().map(|entry| &entry.value) }
    pub fn original_callable_use(&self, instruction: u32) -> Option<&OriginalCallableUse> {
        self.original_callable_uses.binary_search_by_key(&instruction, |entry| entry.value.instruction).ok().map(|index| &self.original_callable_uses[index].value)
    }
    pub fn original_callable_uses(&self) -> impl Iterator<Item = &OriginalCallableUse> { self.original_callable_uses.iter().map(|entry| &entry.value) }
    pub fn original_argument_binding(&self, instruction: u32) -> Option<&OriginalArgumentBinding> {
        let index = self.original_argument_bindings.binary_search_by_key(&instruction, |entry| entry.value.instruction).ok()?;
        let saved = &self.original_argument_bindings[index].value;
        Arc::ptr_eq(saved, self.argument_binding_receipts.get(index)?).then_some(saved.as_ref())
    }
    pub fn original_argument_wrapper(&self, instruction: u32) -> Option<&OriginalArgumentBinding> {
        self.original_argument_wrappers.binary_search_by_key(&instruction, |entry| entry.0).ok().and_then(|index| self.original_argument_binding(self.original_argument_wrappers[index].1))
    }
    pub fn original_argument_bindings(&self) -> impl Iterator<Item = &OriginalArgumentBinding> { self.original_argument_bindings.iter().map(|entry| entry.value.as_ref()) }
    pub fn has_original_argument_bindings(&self) -> bool { !self.original_argument_bindings.is_empty() }
    pub fn argument_has_original_source(&self, instruction: u32, call: crate::sema::check::ExpressionIdentity, ordinal: usize, recipe: &crate::sema::check::SolvedArgumentSource, owner: InstructionOwner, ty: TypeRef) -> bool {
        if matches!(recipe.value, crate::sema::arguments::ArgumentValueSource::RecordField { .. }) {
            return self.native_record_argument_has_original_source(instruction, call, ordinal, recipe, owner, ty);
        }
        let crate::sema::arguments::ArgumentValueSource::Expression(expression) = recipe.value else { return false; };
        let origin = OperationSourceOrigin::Expression(crate::sema::check::ExpressionIdentity { expression, ..call });
        let mut source = instruction;
        for _ in 0..=256 {
            if self.registered_instruction_origin(source, false) == Some((origin, owner)) { return true; }
            let Ok(Some(wrapper)) = self.original_compiler_argument_wrapper(source) else { break; };
            if wrapper.owner != owner || wrapper.body >= source { return false; }
            source = wrapper.body;
        }
        self.original_argument_binding(instruction).is_some_and(|saved| saved.call == call && saved.ordinal as usize == ordinal && saved.recipe == *recipe && saved.owner == owner && saved.ty == ty
            && self.registered_instruction_origin(saved.initializer_source_instruction, false) == Some((origin, owner)))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_original_argument_binding_mut(&mut self, instruction: u32) -> Result<&mut OriginalArgumentBinding, IrVerifyError> {
        let index = self.original_argument_bindings.binary_search_by_key(&instruction, |entry| entry.value.instruction).map_err(|_| failure("original argument binding is missing"))?;
        Ok(Arc::make_mut(&mut self.original_argument_bindings[index].value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_original_argument_bindings(&mut self) { self.original_argument_bindings.clear(); }
    pub fn has_local_callable_bindings(&self) -> bool { !self.original_callable_bindings.is_empty() || !self.original_callable_uses.is_empty() }
    pub fn native_call_source(&self, id: NativeCallSourceId) -> Result<&NativeCallSource, IrVerifyError> {
        let source = owned(self.root, &self.native_call_sources, id.index, id.proof)?;
        let original = self.original_native_call_sources.get(id.index as usize).ok_or_else(|| failure("original prepared source receipt is missing"))?;
        if !Arc::ptr_eq(source, original) { return Err(failure("prepared source differs from its original receipt")); }
        Ok(source.as_ref())
    }
    pub(in crate::runtime::eval) fn native_call_originally_prepared(&self, instruction: u32) -> bool {
        self.original_native_call_sources.iter().any(|source| source.instruction == instruction)
    }
    pub fn ground_native_call(&self, id: GroundNativeCallId) -> Result<&PreparedGroundNativeCall, IrVerifyError> { owned(self.root, &self.ground_native_calls, id.index, id.proof) }
    pub fn native_call_sources(&self) -> impl Iterator<Item = (NativeCallSourceId, &NativeCallSource)> {
        self.native_call_sources.iter().enumerate().map(|(index, entry)| (NativeCallSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn ground_native_calls(&self) -> impl Iterator<Item = (GroundNativeCallId, &PreparedGroundNativeCall)> {
        self.ground_native_calls.iter().enumerate().map(|(index, entry)| (GroundNativeCallId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn ground_native_call_at(&self, instruction: u32) -> Result<Option<GroundNativeCallId>, IrVerifyError> {
        self.ground_native_call_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| { let id = self.ground_native_call_instructions[index].1; self.ground_native_call(id)?; Ok(id) }).transpose()
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_native_call_source_mut(&mut self, id: NativeCallSourceId) -> Result<&mut NativeCallSource, IrVerifyError> { self.native_call_source(id)?; Ok(Arc::make_mut(&mut self.native_call_sources[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_ground_native_call_mut(&mut self, id: GroundNativeCallId) -> Result<&mut PreparedGroundNativeCall, IrVerifyError> { self.ground_native_call(id)?; Ok(&mut self.ground_native_calls[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_ground_native_calls(&mut self) { self.ground_native_calls.clear(); self.ground_native_call_instructions.clear(); }
    pub fn callable_source(&self, id: CallableValueSourceId) -> Result<&CallableValueSource, IrVerifyError> { owned(self.root, &self.callable_sources, id.index, id.proof) }
    pub fn callable_value(&self, id: CallableValueId) -> Result<&PreparedCallableValue, IrVerifyError> { owned(self.root, &self.callable_values, id.index, id.proof) }
    pub fn invocation_source(&self, id: InvocationSourceId) -> Result<&InvocationSource, IrVerifyError> { owned(self.root, &self.invocation_sources, id.index, id.proof) }
    pub fn invocation_plan(&self, id: InvocationPlanId) -> Result<&PreparedInvocationPlan, IrVerifyError> { owned(self.root, &self.invocation_plans, id.index, id.proof) }
    pub fn callable_values(&self) -> impl Iterator<Item = (CallableValueId, &PreparedCallableValue)> {
        self.callable_values.iter().enumerate().map(|(index, entry)| (CallableValueId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn invocation_plans(&self) -> impl Iterator<Item = (InvocationPlanId, &PreparedInvocationPlan)> {
        self.invocation_plans.iter().enumerate().map(|(index, entry)| (InvocationPlanId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn callable_value_at(&self, instruction: u32) -> Result<Option<CallableValueId>, IrVerifyError> {
        self.callable_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| { let id = self.callable_instructions[index].1; self.callable_value(id)?; Ok(id) }).transpose()
    }
    pub fn invocation_plan_at(&self, instruction: u32) -> Result<Option<InvocationPlanId>, IrVerifyError> {
        self.invocation_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| { let id = self.invocation_instructions[index].1; self.invocation_plan(id)?; Ok(id) }).transpose()
    }
    pub fn validate_user_invocation(&self, value: CallableValueId, plan: InvocationPlanId) -> Result<&GroundUserInvocationContract, IrVerifyError> {
        let value = self.callable_value(value)?;
        let plan = self.invocation_plan(plan)?;
        if value.contract != plan.contract.callable { return Err(failure("invoked callable differs from its original checked authority")); }
        Ok(&plan.contract)
    }
    pub fn ground_projection_source(&self, id: GroundProjectionSourceId) -> Result<&GroundProjectionSource, IrVerifyError> {
        let source = owned(self.root, &self.ground_projection_sources, id.index, id.proof)?;
        let original = self.original_ground_projection_sources.get(id.index as usize).ok_or_else(|| failure("original prepared source receipt is missing"))?;
        if !Arc::ptr_eq(source, original) { return Err(failure("prepared source differs from its original receipt")); }
        Ok(source.as_ref())
    }
    pub fn pattern_source(&self, id: PatternSourceId) -> Result<&super::pattern::PreparedPatternSource, IrVerifyError> {
        let source = owned(self.root, &self.pattern_sources, id.index, id.proof)?;
        let original = self.original_pattern_sources.get(id.index as usize).ok_or_else(|| failure("original pattern source receipt is missing"))?;
        if !Arc::ptr_eq(source, original) { return Err(failure("pattern source differs from its original prepared receipt")); }
        Ok(source.as_ref())
    }
    pub fn pattern_application(&self, id: PatternApplicationId) -> Result<&super::pattern::PreparedPatternApplication, IrVerifyError> {
        let application = owned(self.root, &self.pattern_applications, id.index, id.proof)?;
        let original = self.original_pattern_applications.get(id.index as usize).ok_or_else(|| failure("original pattern application receipt is missing"))?;
        if !Arc::ptr_eq(application, original) { return Err(failure("pattern application differs from its original prepared receipt")); }
        Ok(application.as_ref())
    }
    pub fn pattern_capture(&self, id: PatternCaptureId) -> Result<&super::pattern::PreparedPatternCapture, IrVerifyError> { owned(self.root, &self.pattern_captures, id.index, id.proof) }
    pub fn pattern_conditional_result_at(&self, control: u32) -> Result<Option<&super::pattern::PreparedPatternConditionalResult>, IrVerifyError> {
        let Some(index) = self.pattern_conditional_results.binary_search_by_key(&control, |entry| entry.0).ok() else { return Ok(None); };
        let application = self.pattern_application(self.pattern_conditional_results[index].1)?;
        let result = application.result.as_deref().ok_or_else(|| failure("pattern conditional result index has no original result"))?;
        if result.control != control { return Err(failure("pattern conditional result index changes its original control")); }
        Ok(Some(result))
    }
    pub fn pattern_sources(&self) -> impl Iterator<Item = (PatternSourceId, &super::pattern::PreparedPatternSource)> {
        self.pattern_sources.iter().enumerate().map(|(index, entry)| (PatternSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn pattern_applications(&self) -> impl Iterator<Item = (PatternApplicationId, &super::pattern::PreparedPatternApplication)> {
        self.pattern_applications.iter().enumerate().map(|(index, entry)| (PatternApplicationId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn pattern_captures(&self) -> impl Iterator<Item = (PatternCaptureId, &super::pattern::PreparedPatternCapture)> {
        self.pattern_captures.iter().enumerate().map(|(index, entry)| (PatternCaptureId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn pattern_uses(&self) -> &[super::pattern::PreparedPatternUse] { &self.pattern_uses }
    pub fn pattern_use(&self, instruction: u32) -> Option<&super::pattern::PreparedPatternUse> {
        let index = self.pattern_uses.binary_search_by_key(&instruction, |use_| use_.instruction).ok()?;
        let source = self.pattern_uses.get(index)?;
        (self.original_pattern_uses.get(index) == Some(source)).then_some(source)
    }
    pub fn has_pattern_applications(&self) -> bool { !self.pattern_applications.is_empty() }
    pub fn has_original_patterns(&self) -> bool { !self.pattern_origins.is_empty() }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_pattern_source_mut(&mut self, id: PatternSourceId) -> Result<&mut super::pattern::PreparedPatternSource, IrVerifyError> { self.pattern_source(id)?; Ok(Arc::make_mut(&mut self.pattern_sources[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_pattern_application_mut(&mut self, id: PatternApplicationId) -> Result<&mut super::pattern::PreparedPatternApplication, IrVerifyError> { self.pattern_application(id)?; Ok(Arc::make_mut(&mut self.pattern_applications[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_pattern_capture_mut(&mut self, id: PatternCaptureId) -> Result<&mut super::pattern::PreparedPatternCapture, IrVerifyError> { self.pattern_capture(id)?; Ok(&mut self.pattern_captures[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_pattern_use_mut(&mut self, instruction: u32) -> Result<&mut super::pattern::PreparedPatternUse, IrVerifyError> {
        let index = self.pattern_uses.binary_search_by_key(&instruction, |use_| use_.instruction).map_err(|_| failure("original pattern use is missing"))?;
        Ok(&mut self.pattern_uses[index])
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_pattern_evidence(&mut self) { self.pattern_sources.clear(); self.pattern_applications.clear(); self.pattern_captures.clear(); self.pattern_uses.clear(); }
    pub fn ground_projection(&self, id: GroundProjectionId) -> Result<&PreparedGroundProjection, IrVerifyError> { owned(self.root, &self.ground_projections, id.index, id.proof) }
    pub fn ground_projections(&self) -> impl Iterator<Item = (GroundProjectionId, &PreparedGroundProjection)> {
        self.ground_projections.iter().enumerate().map(|(index, entry)| (GroundProjectionId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn ground_projection_at(&self, instruction: u32) -> Result<Option<GroundProjectionId>, IrVerifyError> {
        self.ground_projection_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| {
            let id = self.ground_projection_instructions[index].1; self.ground_projection(id).map(|_| id)
        }).transpose()
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_ground_projection_mut(&mut self, id: GroundProjectionId) -> Result<&mut PreparedGroundProjection, IrVerifyError> { self.ground_projection(id)?; Ok(&mut self.ground_projections[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_ground_projection_source_mut(&mut self, id: GroundProjectionSourceId) -> Result<&mut GroundProjectionSource, IrVerifyError> { self.ground_projection_source(id)?; Ok(Arc::make_mut(&mut self.ground_projection_sources[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_ground_projections(&mut self) { self.ground_projections.clear(); self.ground_projection_instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_callable_value_mut(&mut self, id: CallableValueId) -> Result<&mut PreparedCallableValue, IrVerifyError> { self.callable_value(id)?; Ok(&mut self.callable_values[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_callable_source_mut(&mut self, id: CallableValueSourceId) -> Result<&mut CallableValueSource, IrVerifyError> { self.callable_source(id)?; Ok(&mut self.callable_sources[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_invocation_plan_mut(&mut self, id: InvocationPlanId) -> Result<&mut PreparedInvocationPlan, IrVerifyError> { self.invocation_plan(id)?; Ok(&mut self.invocation_plans[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_invocation_source_mut(&mut self, id: InvocationSourceId) -> Result<&mut InvocationSource, IrVerifyError> { self.invocation_source(id)?; Ok(&mut self.invocation_sources[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_callable_values(&mut self) { self.callable_values.clear(); self.callable_sources.clear(); self.callable_instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_invocation_plans(&mut self) { self.invocation_plans.clear(); self.invocation_sources.clear(); self.invocation_instructions.clear(); }
    pub fn registered_instruction_origin(&self, instruction: u32, stage: bool) -> Option<(OperationSourceOrigin, InstructionOwner)> {
        self.instruction_origins.binary_search_by_key(&(instruction, stage), |entry| (entry.0, matches!(entry.1, OperationSourceOrigin::Stage(_))))
            .ok().map(|index| (self.instruction_origins[index].1, self.instruction_origins[index].2))
    }
    pub fn registered_pattern_origin(&self, pattern: u32) -> Option<(crate::sema::check::PatternIdentity, InstructionOwner)> {
        self.pattern_origins.binary_search_by_key(&pattern, |entry| entry.0).ok().map(|index| (self.pattern_origins[index].1, self.pattern_origins[index].2))
    }
    pub fn checked_function(&self, declaration: crate::sema::check::DeclarationIdentity) -> Result<&CheckedFunctionSource, IrVerifyError> {
        let index = self.checked_function_index.binary_search_by_key(&declaration, |entry| entry.0).map_err(|_| failure("stage callback has no checked declaration root"))?;
        let id = self.checked_function_index[index].1;
        owned(self.root, &self.checked_functions, id.index, id.proof)
    }
    pub fn checked_functions(&self) -> impl Iterator<Item = (CheckedFunctionId, &CheckedFunctionSource)> {
        self.checked_functions.iter().enumerate().map(|(index, entry)| (CheckedFunctionId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn stage_call_source(&self, id: StageCallSourceId) -> Result<&StageCallSource, IrVerifyError> { owned(self.root, &self.stage_call_sources, id.index, id.proof) }
    pub fn ground_stage_call(&self, id: GroundStageCallId) -> Result<&PreparedGroundStageCall, IrVerifyError> { owned(self.root, &self.ground_stage_calls, id.index, id.proof) }
    pub fn stage_call_sources(&self) -> impl Iterator<Item = (StageCallSourceId, &StageCallSource)> {
        self.stage_call_sources.iter().enumerate().map(|(index, entry)| (StageCallSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn ground_stage_calls(&self) -> impl Iterator<Item = (GroundStageCallId, &PreparedGroundStageCall)> {
        self.ground_stage_calls.iter().enumerate().map(|(index, entry)| (GroundStageCallId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn ground_stage_call_at(&self, instruction: u32) -> Result<Option<&PreparedGroundStageCall>, IrVerifyError> {
        self.ground_stage_call_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.ground_stage_call(self.ground_stage_call_instructions[index].1)).transpose()
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_ground_stage_call_mut(&mut self, id: GroundStageCallId) -> Result<&mut PreparedGroundStageCall, IrVerifyError> { self.ground_stage_call(id)?; Ok(&mut self.ground_stage_calls[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_stage_call_source_mut(&mut self, id: StageCallSourceId) -> Result<&mut StageCallSource, IrVerifyError> { self.stage_call_source(id)?; Ok(&mut self.stage_call_sources[id.index as usize].value) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_ground_stage_calls(&mut self) { self.ground_stage_calls.clear(); self.ground_stage_call_instructions.clear(); }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_stage_call_sources(&mut self) { self.stage_call_sources.clear(); }
    pub fn operation_source(&self, id: OperationSourceId) -> Result<&OperationSource, IrVerifyError> {
        let value = owned(self.root, &self.operation_sources, id.index, id.proof)?;
        if !self.original_operation_sources.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(original, value)) { return Err(failure("prepared operation differs from its original receipt")); }
        Ok(value.as_ref())
    }
    pub fn operation(&self, id: OperationId) -> Result<&PreparedOperation, IrVerifyError> {
        let value = owned(self.root, &self.operations, id.index, id.proof)?;
        if !self.original_operations.get(id.index as usize).is_some_and(|original| Arc::ptr_eq(original, value)) { return Err(failure("prepared operation differs from its original receipt")); }
        Ok(value.as_ref())
    }
    pub fn operation_sources(&self) -> impl Iterator<Item = (OperationSourceId, &OperationSource)> {
        self.operation_sources.iter().enumerate().map(|(index, entry)| (OperationSourceId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn operations(&self) -> impl Iterator<Item = (OperationId, &PreparedOperation)> {
        self.operations.iter().enumerate().map(|(index, entry)| (OperationId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, entry.value.as_ref()))
    }
    pub fn operation_at(&self, instruction: u32) -> Result<Option<&PreparedOperation>, IrVerifyError> {
        let original = self.original_operation_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.original_operation_instructions[index].1);
        let current = self.operation_instructions.binary_search_by_key(&instruction, |entry| entry.0).ok().map(|index| self.operation_instructions[index].1);
        if current != original { return Err(failure("operation loses its original instruction receipt")); }
        current.map(|id| self.operation(id)).transpose()
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_operation_mut(&mut self, id: OperationId) -> Result<&mut PreparedOperation, IrVerifyError> { self.operation(id)?; Ok(Arc::make_mut(&mut self.operations[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_operation_source_mut(&mut self, id: OperationSourceId) -> Result<&mut OperationSource, IrVerifyError> { self.operation_source(id)?; Ok(Arc::make_mut(&mut self.operation_sources[id.index as usize].value)) }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_remove_operations(&mut self) { self.operations.clear(); self.operation_instructions.clear(); }
    pub fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            + self.records.retained_bytes() + self.comprehensions.retained_bytes() + self.compiler_wrappers.retained_bytes() + self.indices.retained_bytes() + self.containers.retained_bytes() + self.record_constructors.retained_bytes() + self.mutables.retained_bytes() + self.native_scalars.retained_bytes() + self.try_captures.retained_bytes() + self.context_producers.retained_bytes() + self.stages.retained_bytes() + self.bridges.retained_bytes() + self.error_constructors.retained_bytes() + self.tag_constructors.retained_bytes() + self.mutable_paths.retained_bytes() + self.host_bindings.retained_bytes() + self.lexical_captures.retained_bytes() + self.constants.retained_bytes() + self.record_updates.retained_bytes() + self.conditionals.retained_bytes() + self.paths.retained_bytes()
            + self.eligibility.retained_bytes() + self.iterations.retained_bytes() + self.operation_requirements.retained_bytes() + self.values.retained_bytes() + self.module_invocations.retained_bytes() + self.callable_receivers.retained_bytes() + self.native_callables.retained_bytes()
            + self.pattern_nominals.capacity() * size_of::<Entry<super::pattern::PreparedPatternNominalMember>>()
            + self.pattern_nominals.iter().map(|entry| entry.value.retained_bytes()).sum::<usize>()
            + self.original_argument_wrappers.capacity() * size_of::<(u32, u32)>()
            + self.original_argument_bindings.capacity() * size_of::<Entry<Arc<OriginalArgumentBinding>>>()
            + self.argument_binding_receipts.capacity() * size_of::<Arc<OriginalArgumentBinding>>()
            + self.argument_binding_receipts.len() * (size_of::<OriginalArgumentBinding>() + 2 * size_of::<usize>())
            + self.argument_binding_receipts.iter().map(|entry| entry.initializer_wrappers.len() * size_of::<ValueInitializerWrapper>() + entry.initializer_wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()).sum::<usize>()
            + self.original_callable_bindings.capacity() * size_of::<Entry<OriginalCallableBinding>>()
            + self.original_callable_uses.capacity() * size_of::<Entry<OriginalCallableUse>>()
            + self.native_call_sources.capacity() * size_of::<Entry<Arc<NativeCallSource>>>()
            + self.original_native_call_sources.capacity() * size_of::<Arc<NativeCallSource>>()
            + self.original_native_call_sources.len() * (size_of::<NativeCallSource>() + 2 * size_of::<usize>())
            + self.ground_native_calls.capacity() * size_of::<Entry<PreparedGroundNativeCall>>()
            + self.ground_native_call_instructions.capacity() * size_of::<(u32, GroundNativeCallId)>()
            + self.original_native_call_sources.iter().map(|entry| entry.expected.retained_bytes() + entry.result_record_layout.as_ref().map_or(0, PreparedNativeResultRecord::retained_bytes)
                + entry.record_arguments.len() * size_of::<PreparedNativeRecordFieldArgument>() + entry.record_arguments.iter().map(PreparedNativeRecordFieldArgument::retained_bytes).sum::<usize>()
                + entry.argument_lineages.iter().map(PreparedNativeArgumentLineage::retained_bytes).sum::<usize>()).sum::<usize>()
            + self.ground_native_calls.iter().map(|entry| entry.value.contract.retained_bytes()).sum::<usize>()
            + self.scopes.capacity() * size_of::<Entry<SchemeScope>>()
            + self.callable_sources.capacity() * size_of::<Entry<CallableValueSource>>()
            + self.callable_values.capacity() * size_of::<Entry<PreparedCallableValue>>()
            + self.callable_instructions.capacity() * size_of::<(u32, CallableValueId)>()
            + self.scoped_invocation_sources.capacity() * size_of::<Entry<Arc<ScopedInvocationSource>>>()
            + self.original_scoped_invocation_sources.capacity() * size_of::<Arc<ScopedInvocationSource>>()
            + self.scoped_invocation_witnesses.capacity() * size_of::<Entry<ScopedInvocationWitness>>()
            + self.scoped_invocation_instructions.capacity() * size_of::<(u32, ScopedInvocationSourceId)>()
            + self.scoped_invocation_sources.iter().map(|entry| entry.value.retained_bytes()).sum::<usize>()
            + self.scoped_invocation_witnesses.iter().map(|entry| (entry.value.binding.supplied_slots.len() + entry.value.binding.default_slots.len() + entry.value.binding.operands.len()) * size_of::<u32>()).sum::<usize>()
            + self.scopes.iter().flat_map(|entry| &entry.value.requirements).map(|requirement| match requirement { Requirement::Operation(operation) => operation.retained_bytes(), Requirement::NativeMethod(method) => method.retained_bytes(), Requirement::TagConstructor(constructor) => constructor.retained_bytes(), Requirement::Invocation { arguments, .. } => arguments.len() * size_of::<TemplateInvocationArgument>(), _ => 0 }).sum::<usize>()
            + self.invocation_sources.capacity() * size_of::<Entry<InvocationSource>>()
            + self.invocation_plans.capacity() * size_of::<Entry<PreparedInvocationPlan>>()
            + self.invocation_instructions.capacity() * size_of::<(u32, InvocationPlanId)>()
            + self.ground_projection_sources.capacity() * size_of::<Entry<Arc<GroundProjectionSource>>>()
            + self.original_ground_projection_sources.capacity() * size_of::<Arc<GroundProjectionSource>>()
            + self.original_ground_projection_sources.len() * (size_of::<GroundProjectionSource>() + 2 * size_of::<usize>())
            + self.original_ground_projection_sources.iter().map(|source| source.postfix.as_ref().map_or(0, PreparedResultReceiver::retained_bytes) + source.receiver_wrappers.len() * size_of::<ValueInitializerWrapper>() + source.receiver_wrappers.iter().map(|wrapper| wrapper.payload.len() * size_of::<u32>()).sum::<usize>()).sum::<usize>()
            + self.ground_projections.capacity() * size_of::<Entry<PreparedGroundProjection>>()
            + self.ground_projection_instructions.capacity() * size_of::<(u32, GroundProjectionId)>()
            + self.pattern_sources.capacity() * size_of::<Entry<Arc<super::pattern::PreparedPatternSource>>>()
            + self.original_pattern_sources.capacity() * size_of::<Arc<super::pattern::PreparedPatternSource>>()
            + self.original_pattern_sources.iter().map(|source| size_of::<super::pattern::PreparedPatternSource>() + 2 * size_of::<usize>() + source.retained_bytes()).sum::<usize>()
            + self.pattern_applications.capacity() * size_of::<Entry<Arc<super::pattern::PreparedPatternApplication>>>()
            + self.original_pattern_applications.capacity() * size_of::<Arc<super::pattern::PreparedPatternApplication>>()
            + self.original_pattern_applications.len() * (size_of::<super::pattern::PreparedPatternApplication>() + 2 * size_of::<usize>())
            + self.original_pattern_applications.iter().map(|application| application.retained_bytes()).sum::<usize>()
            + self.pattern_conditional_results.capacity() * size_of::<(u32, PatternApplicationId)>()
            + self.pattern_captures.capacity() * size_of::<Entry<super::pattern::PreparedPatternCapture>>()
            + self.pattern_uses.capacity() * size_of::<super::pattern::PreparedPatternUse>()
            + self.original_pattern_uses.capacity() * size_of::<super::pattern::PreparedPatternUse>()
            + self.invocation_sources.iter().map(|entry| entry.value.expected.retained_bytes()).sum::<usize>()
            + self.invocation_plans.iter().map(|entry| entry.value.contract.retained_bytes()).sum::<usize>()
            + self.instruction_origins.capacity() * size_of::<(u32, OperationSourceOrigin, InstructionOwner)>()
            + self.pattern_origins.capacity() * size_of::<(u32, crate::sema::check::PatternIdentity, InstructionOwner)>()
            + self.checked_functions.capacity() * size_of::<Entry<CheckedFunctionSource>>()
            + self.checked_function_index.capacity() * size_of::<(crate::sema::check::DeclarationIdentity, CheckedFunctionId)>()
            + self.stage_call_sources.capacity() * size_of::<Entry<StageCallSource>>()
            + self.ground_stage_calls.capacity() * size_of::<Entry<PreparedGroundStageCall>>()
            + self.ground_stage_call_instructions.capacity() * size_of::<(u32, GroundStageCallId)>()
            + self.stage_call_sources.iter().map(|entry| entry.value.expected.retained_bytes()).sum::<usize>()
            + self.ground_stage_calls.iter().map(|entry| entry.value.contract.retained_bytes()).sum::<usize>()
            + self.operation_sources.capacity() * size_of::<Entry<Arc<OperationSource>>>() + self.original_operation_sources.capacity() * size_of::<Arc<OperationSource>>() + self.operation_sources.len() * (size_of::<OperationSource>() + 2 * size_of::<usize>())
            + self.operation_sources.iter().map(|entry| entry.value.expected.retained_bytes()).sum::<usize>()
            + self.operations.capacity() * size_of::<Entry<Arc<PreparedOperation>>>() + self.original_operations.capacity() * size_of::<Arc<PreparedOperation>>() + self.operations.len() * (size_of::<PreparedOperation>() + 2 * size_of::<usize>())
            + (self.operation_instructions.capacity() + self.original_operation_instructions.capacity()) * size_of::<(u32, OperationId)>()
            + self.operations.iter().map(|entry| {
                let operation = &entry.value;
                operation.tag_equality.as_ref().map_or(0, super::full::PreparedTagEquality::retained_bytes) + operation.membership_lowering.as_ref().map_or(0, super::full::PreparedMembershipLowering::retained_bytes) + operation.literal_comparison_slot.as_ref().map_or(0, super::full::PreparedLiteralComparisonSlot::retained_bytes) + operation.range_lowering.as_ref().map_or(0, PreparedRangeLowering::retained_bytes) + operation.fallback_lowering.as_ref().map_or(0, PreparedFallbackLowering::retained_bytes) + operation.authority.retained_bytes() + operation.arguments.len() * size_of::<Option<TypeRef>>()
                    + (operation.binding.supplied_slots.len() + operation.binding.default_slots.len() + operation.binding.operands.len()) * size_of::<u32>()
                    + operation.effects.inputs.len() * size_of::<(crate::sema::inference::EffectRole, crate::sema::inference::EffectSet)>()
                    + operation.effects.outputs.len() * size_of::<(crate::sema::inference::ProducerRole, crate::sema::inference::EffectSet)>()
            }).sum::<usize>()
            + self.layouts.capacity() * size_of::<Entry<PhysicalLayout>>()
            + self.instances.capacity() * size_of::<Entry<Instantiation>>()
            + self.forwarding.capacity() * size_of::<Entry<ForwardingPlan>>()
            + self.templates.capacity() * size_of::<Entry<TypeTemplate>>()
            + self.arguments.capacity() * size_of::<SolvedArgument>() + self.calls.capacity() * size_of::<SolvedCall>() + self.uses.capacity() * size_of::<SolvedRequirementUse>()
            + self.constructors.capacity() * size_of::<SolvedRecordLayout>() + self.function_scopes.capacity() * size_of::<(IrFunctionId, SchemeScopeId)>()
            + self.scopes.iter().map(|entry| entry.value.quantifiers.len() * size_of::<QuantifierKind>() + entry.value.parameters.len() * size_of::<TypeRef>() + entry.value.parameter_names.len() * size_of::<Name>() + entry.value.parameter_flags.len() * size_of::<u8>() + entry.value.requirements.len() * size_of::<Requirement>()).sum::<usize>()
            + self.layouts.iter().map(|entry| entry.value.fields.len() * size_of::<(Name, GroundTypeId)>()).sum::<usize>()
            + self.instances.iter().map(|entry| (entry.value.substitutions.len() + entry.value.parameter_types.len()) * size_of::<GroundTypeId>() + entry.value.requirements.len() * size_of::<RequirementWitness>()).sum::<usize>()
            + self.forwarding.iter().map(|entry| entry.value.substitutions.len() * size_of::<TypeRef>() + entry.value.requirements.len() * size_of::<ForwardedRequirement>() + entry.value.instances.len() * size_of::<(InstantiationId, InstantiationId)>()).sum::<usize>()
            + self.templates.iter().map(|entry| match &entry.value { TypeTemplate::Record { fields, .. } => fields.len() * size_of::<(Name, TypeRef)>(), TypeTemplate::Arrow { parameters, effects, .. } => parameters.len() * size_of::<TemplateParameter>() + effects.len() * size_of::<crate::syntax::node::Effect>(), _ => 0 }).sum::<usize>()
    }
    pub fn shrink_to_fit(&mut self) { self.eligibility.shrink_to_fit(); self.callable_receivers.shrink_to_fit(); self.module_invocations.shrink_to_fit(); self.native_callables.shrink_to_fit(); self.values.shrink_to_fit(); self.iterations.shrink_to_fit(); self.records.shrink_to_fit(); self.comprehensions.shrink_to_fit(); self.compiler_wrappers.shrink_to_fit(); self.indices.shrink_to_fit(); self.containers.shrink_to_fit(); self.record_constructors.shrink_to_fit(); self.mutables.shrink_to_fit(); self.native_scalars.shrink_to_fit(); self.try_captures.shrink_to_fit(); self.context_producers.shrink_to_fit(); self.stages.shrink_to_fit(); self.error_constructors.shrink_to_fit(); self.tag_constructors.shrink_to_fit(); self.bridges.shrink_to_fit(); self.mutable_paths.shrink_to_fit(); self.host_bindings.shrink_to_fit(); self.lexical_captures.shrink_to_fit(); self.constants.shrink_to_fit(); self.record_updates.shrink_to_fit(); self.conditionals.shrink_to_fit(); self.paths.shrink_to_fit(); self.operation_requirements.shrink_to_fit(); self.scoped_invocation_sources.shrink_to_fit(); self.original_scoped_invocation_sources.shrink_to_fit(); self.scoped_invocation_witnesses.shrink_to_fit(); self.scoped_invocation_instructions.shrink_to_fit(); self.original_argument_bindings.shrink_to_fit(); self.argument_binding_receipts.shrink_to_fit(); self.original_argument_wrappers.shrink_to_fit(); self.pattern_nominals.shrink_to_fit(); self.original_callable_bindings.shrink_to_fit(); self.original_callable_uses.shrink_to_fit(); self.native_call_sources.shrink_to_fit(); self.original_native_call_sources.shrink_to_fit(); self.ground_native_calls.shrink_to_fit(); self.ground_native_call_instructions.shrink_to_fit(); self.pattern_origins.shrink_to_fit(); self.pattern_sources.shrink_to_fit(); self.original_pattern_sources.shrink_to_fit(); self.pattern_applications.shrink_to_fit(); self.original_pattern_applications.shrink_to_fit(); self.pattern_conditional_results.shrink_to_fit(); self.pattern_captures.shrink_to_fit(); self.pattern_uses.shrink_to_fit(); self.original_pattern_uses.shrink_to_fit(); self.ground_projection_sources.shrink_to_fit(); self.original_ground_projection_sources.shrink_to_fit(); self.ground_projections.shrink_to_fit(); self.ground_projection_instructions.shrink_to_fit(); self.callable_sources.shrink_to_fit(); self.callable_values.shrink_to_fit(); self.invocation_sources.shrink_to_fit(); self.invocation_plans.shrink_to_fit(); self.callable_instructions.shrink_to_fit(); self.invocation_instructions.shrink_to_fit(); self.instruction_origins.shrink_to_fit(); self.checked_functions.shrink_to_fit(); self.checked_function_index.shrink_to_fit(); self.stage_call_sources.shrink_to_fit(); self.ground_stage_calls.shrink_to_fit(); self.ground_stage_call_instructions.shrink_to_fit(); self.operation_sources.shrink_to_fit(); self.original_operation_sources.shrink_to_fit(); self.operations.shrink_to_fit(); self.original_operations.shrink_to_fit(); self.operation_instructions.shrink_to_fit(); self.original_operation_instructions.shrink_to_fit(); self.scopes.shrink_to_fit(); self.layouts.shrink_to_fit(); self.instances.shrink_to_fit(); self.forwarding.shrink_to_fit(); self.templates.shrink_to_fit(); self.calls.shrink_to_fit(); self.arguments.shrink_to_fit(); self.uses.shrink_to_fit(); self.constructors.shrink_to_fit(); self.function_scopes.shrink_to_fit(); }
    pub fn instances(&self) -> impl Iterator<Item = (InstantiationId, &Instantiation)> {
        self.instances.iter().enumerate().map(|(index, entry)| (InstantiationId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_scope_mut(&mut self, id: SchemeScopeId) -> Result<&mut SchemeScope, IrVerifyError> {
        self.scope(id)?;
        Ok(&mut self.scopes[id.index as usize].value)
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_forwarding_mut(&mut self, id: ForwardingId) -> Result<&mut ForwardingPlan, IrVerifyError> {
        self.forwarding(id)?;
        Ok(&mut self.forwarding[id.index as usize].value)
    }
    pub fn scopes(&self) -> impl Iterator<Item = (SchemeScopeId, &SchemeScope)> {
        self.scopes.iter().enumerate().map(|(index, entry)| (SchemeScopeId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn scope_for_function(&self, owner: IrFunctionId) -> Option<SchemeScopeId> { self.function_scopes.binary_search_by_key(&owner.raw(), |entry| entry.0.raw()).ok().map(|index| self.function_scopes[index].1) }
    pub fn calls(&self) -> &[SolvedCall] { &self.calls }
    pub fn arguments(&self) -> &[SolvedArgument] { &self.arguments }
    pub fn call_arguments(&self, instruction: u32) -> &[SolvedArgument] {
        let start = self.arguments.partition_point(|argument| argument.call_instruction < instruction);
        let end = self.arguments.partition_point(|argument| argument.call_instruction <= instruction);
        &self.arguments[start..end]
    }
    pub fn requirement_uses(&self) -> &[SolvedRequirementUse] { &self.uses }
    pub fn constructors(&self) -> &[SolvedRecordLayout] { &self.constructors }
    pub fn scope(&self, id: SchemeScopeId) -> Result<&SchemeScope, IrVerifyError> { owned(self.root, &self.scopes, id.index, id.proof) }
    pub fn layout(&self, id: PhysicalLayoutId) -> Result<&PhysicalLayout, IrVerifyError> { owned(self.root, &self.layouts, id.index, id.proof) }
    pub fn instance(&self, id: InstantiationId) -> Result<&Instantiation, IrVerifyError> { owned(self.root, &self.instances, id.index, id.proof) }
    pub fn forwarding(&self, id: ForwardingId) -> Result<&ForwardingPlan, IrVerifyError> { owned(self.root, &self.forwarding, id.index, id.proof) }
    pub fn template(&self, id: TypeTemplateId) -> Result<&TypeTemplate, IrVerifyError> { owned(self.root, &self.templates, id.index, id.proof) }
    pub fn call(&self, instruction: u32) -> Option<&SolvedCall> { self.calls.binary_search_by_key(&instruction, |call| call.instruction).ok().map(|i| &self.calls[i]) }
    pub fn requirement_use(&self, instruction: u32) -> Option<&SolvedRequirementUse> { self.uses.binary_search_by_key(&instruction, |use_| use_.instruction).ok().map(|i| &self.uses[i]) }
    pub fn constructor(&self, instruction: u32) -> Option<&SolvedRecordLayout> { self.constructors.binary_search_by_key(&instruction, |record| record.instruction).ok().map(|i| &self.constructors[i]) }

    pub fn forwarded_instance(&self, plan: ForwardingId, caller: InstantiationId) -> Result<InstantiationId, IrVerifyError> {
        self.instance(caller)?;
        { let edges = &self.forwarding(plan)?.instances; edges.binary_search_by_key(&caller.index, |edge| edge.0.index).ok().filter(|&index| edges[index].0 == caller).map(|index| edges[index].1) }
            .ok_or_else(|| failure("generic forwarding edge was not prepared"))
    }

    fn verify_type(pools: &SemanticPools, ty: GroundTypeId) -> Result<(), IrVerifyError> {
        Self::normalized_ground_type(pools, ty)?;
        Ok(())
    }

    fn verify_user_callable(&self, pools: &SemanticPools, contract: UserCallableContract) -> Result<(), IrVerifyError> {
        let declaration = self.checked_function(contract.declaration)?;
        if declaration.target != contract.target || declaration.signature != contract.signature { return Err(failure("callable value impersonates another checked declaration")); }
        if pools.signature_closed_effects(contract.signature)? != contract.creation
            || (contract.kind == CallableKind::Pure && contract.creation != crate::sema::inference::EffectSet::EMPTY) { return Err(failure("callable value changes its original closed effects")); }
        Self::verify_type(pools, pools.signature_return_type(contract.signature)?)?;
        for slot in 0..pools.signature_param_count(contract.signature)? { Self::verify_type(pools, pools.signature_param(contract.signature, slot)?.1)?; }
        Ok(())
    }

    fn verify_native_call_store(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let mut sources = std::collections::BTreeSet::new();
        let mut expected = Vec::new();
        for (id, proof) in self.ground_native_calls() {
            let source = self.native_call_source(proof.source)?;
            source.verify_result_record_layout(pools)?;
            let contract = &proof.contract;
            if source.expected != *contract || !sources.insert(proof.source.index)
                || owners.get(source.instruction as usize) != Some(&Some(source.owner))
                || self.registered_instruction_origin(source.instruction, false) != Some((OperationSourceOrigin::Expression(source.origin), source.owner)) {
                return Err(failure("native call changes or duplicates its original source contract"));
            }
            if let Some(scope) = source.scope {
                if source.owner != InstructionOwner::Function(self.scope(scope)?.owner) { return Err(failure("native call scope belongs to another declaration")); }
            }
            let cli = contract.verify_cli_descriptor(pools)?;
            let dynamic_cli = contract.verify_dynamic_cli_carrier(pools)?;
            let record_get = source.verify_record_get_refinement(pools)?;
            self.verify_process_command_argv_contract(pools, source, contract, owners)?;
            contract.verify_fs_root_method(pools)?;
            if !(matches!(contract.registry_owner, crate::sema::registry_graph::RegistryOwner::Module(_)) && contract.receiver.is_none()
                || matches!(contract.registry_owner, crate::sema::registry_graph::RegistryOwner::Method(_)) && contract.receiver.is_some())
                || !(cli || dynamic_cli || record_get || matches!(contract.authority, PreparedOperationAuthority::Registry { binding: crate::modules::signature::ImplBinding::Native, semantic_rule: crate::modules::signature::SemanticRule::Standard, .. })) {
                return Err(failure("native call boundary protocol is not prepared"));
            }
            if contract.binding.rest_slot.is_some() || contract.binding.dynamic.is_some()
                || pools.signature_closed_effects(contract.signature)? != contract.effects.creation
                || (contract.kind == CallableKind::Pure && contract.effects.creation != crate::sema::inference::EffectSet::EMPTY
                    && !contract.has_closed_collect_effects(pools)?)
                || (!cli && contract.result != TypeRef::Ground(pools.signature_return_type(contract.signature)?)) {
                return Err(failure("native call signature or creation effects changed"));
            }
            let count = pools.signature_param_count(contract.signature)?;
            if count > 65536 || contract.arguments.len() != contract.binding.supplied_slots.len()
                || contract.arguments.len() != contract.binding.operands.len() || contract.argument_sources.len() != count
                || contract.effects.inputs.len() > 65536 || contract.effects.outputs.len() > 65536 {
                return Err(failure("native call argument or effect payload is incomplete"));
            }
            Self::verify_type(pools, pools.signature_return_type(contract.signature)?)?;
            let mut slots = std::collections::BTreeSet::new();
            if let Some(receiver) = &contract.receiver {
                if count == 0 || contract.argument_sources[0] != Some(receiver.instruction)
                    || owners.get(receiver.instruction as usize) != Some(&Some(source.owner))
                    || !self.native_receiver_has_original_source(receiver, source.owner, owners)? {
                    return Err(failure("native method receiver lost its original source or owner"));
                }
                let TypeRef::Ground(actual) = receiver.ty else { return Err(failure("native method receiver requires a scoped protocol")); };
                let (label, formal, _) = pools.signature_param(contract.signature, 0)?;
                let relation = contract.argument_relations.first().copied().unwrap_or(crate::sema::inference::ArgumentRelation::Assignable);
                let erased_keys = matches!(contract.registry_owner, crate::sema::registry_graph::RegistryOwner::Method(crate::modules::signature::MethodReceiver::Record))
                    && matches!(contract.authority, PreparedOperationAuthority::Registry { operation: crate::modules::RuntimeOp::RecordKeys, .. })
                    && super::full::FullVerifier::record_keys_receiver_accepts(&pools.to_type(formal)?, &pools.to_type(actual)?, relation);
                if label != Name::intern("<receiver>") || pools.signature_parameter_rest(contract.signature, 0)? || pools.signature_parameter_defaulted(contract.signature, 0)?
                    || !erased_keys && !native_callables::native_parameter_accepts(relation, &pools.to_type(formal)?, &pools.to_type(actual)?, false, 0)? {
                    return Err(failure("native method receiver changes its hidden formal contract"));
                }
                slots.insert(0);
            }
            let mut guards = std::collections::BTreeSet::new();
            self.verify_native_record_arguments(pools, source, owners)?;
            if contract.input_eligibility.iter().any(|&(slot, _)| slot >= count.saturating_sub(usize::from(contract.receiver.is_some())) || !guards.insert(slot)) {
                return Err(failure("native call eligibility slots are ambiguous or invalid"));
            }
            for (ordinal, ((argument, &slot), &instruction)) in contract.arguments.iter().zip(&contract.binding.supplied_slots).zip(&contract.binding.operands).enumerate() {
                if slot as usize >= count || !slots.insert(slot) || instruction != argument.instruction
                    || contract.argument_sources[slot as usize] != Some(instruction)
                    || owners.get(instruction as usize) != Some(&Some(source.owner)) {
                    return Err(failure("native call supplied slot is invalid"));
                }
                if !matches!(argument.original.value, crate::sema::arguments::ArgumentValueSource::Expression(_) | crate::sema::arguments::ArgumentValueSource::RecordField { .. }) { return Err(failure("native call source recipe is not prepared")); }
                let original_source = if cli {
                    self.argument_has_original_source(instruction, source.origin, ordinal, &argument.original, source.owner, argument.ty)
                } else { self.native_argument_has_original_source(source, ordinal, argument, owners)? };
                if !original_source { return Err(failure("native call supplied operand lost its original source")); }
                let TypeRef::Ground(actual) = argument.ty else { return Err(failure("native call argument requires a scoped instance")); };
                Self::verify_type(pools, actual)?;
                let (label, formal, _) = pools.signature_param(contract.signature, slot as usize)?;
                Self::verify_type(pools, formal)?;
                let relation = contract.argument_relations.get(slot as usize).copied().unwrap_or(crate::sema::inference::ArgumentRelation::Assignable);
                let original_argv = slot == 1
                    && matches!(contract.authority, PreparedOperationAuthority::Registry { operation: crate::modules::RuntimeOp::ProcessCommandArgv, .. })
                    && matches!(relation, crate::sema::inference::ArgumentRelation::CommandArgv { .. })
                    && pools.to_type(formal)? == crate::sema::types::Type::List(Box::new(crate::sema::types::Type::Str))
                    && self.verify_process_argv_container(pools, source, contract, actual)?;
                if pools.signature_parameter_rest(contract.signature, slot as usize)? || argument.original.name.is_some_and(|name| name != label)
                    || !original_argv && !native_callables::native_parameter_accepts(relation, &pools.to_type(formal)?, &pools.to_type(actual)?, false, 0)? {
                    return Err(failure(&format!("native call {:?}, supplied slot {slot} ({label:?}) disagrees with its selected signature: formal {:?}, actual {:?}, relation {:?}", source.origin, pools.to_type(formal)?, pools.to_type(actual)?, contract.argument_relations.get(slot as usize))));
                }
            }
            for &slot in &contract.binding.default_slots {
                if slot as usize >= count || !slots.insert(slot) || contract.argument_sources[slot as usize].is_some()
                    || !pools.signature_parameter_defaulted(contract.signature, slot as usize)? {
                    return Err(failure("native call default slot is invalid"));
                }
                Self::verify_type(pools, pools.signature_param(contract.signature, slot as usize)?.1)?;
            }
            if slots.len() != count { return Err(failure("native call omits a required slot")); }
            let mut inputs = std::collections::BTreeSet::new();
            let mut outputs = std::collections::BTreeSet::new();
            if contract.effects.creation.0 & !0x7f != 0
                || contract.effects.inputs.iter().any(|&(role, effects)| !inputs.insert(role) || effects.0 & !0x7f != 0)
                || contract.effects.outputs.iter().any(|&(role, effects)| !outputs.insert(role) || effects.0 & !0x7f != 0) {
                return Err(failure("native call effect roles are ambiguous or invalid"));
            }
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if sources.len() != self.native_call_sources.len() || expected.windows(2).any(|pair| pair[0].0 == pair[1].0)
            || expected != self.ground_native_call_instructions {
            return Err(failure("native call source or instruction index is missing, ambiguous, or stale"));
        }
        Ok(())
    }

    fn verify_callable_store(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let origin = |instruction, expression, owner| {
            if self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(expression), owner))
                || owners.get(instruction as usize) != Some(&Some(owner)) { return Err(failure("callable source differs from its original instruction or owner")); }
            Ok(())
        };
        let mut previous = None;
        for binding in self.original_callable_bindings() {
            if previous.is_some_and(|value| value >= binding.binding)
                || binding.binding.source != binding.statement.source || binding.binding.namespace != binding.statement.namespace
                || binding.initializer_source.source != binding.binding.source || binding.initializer_source.namespace != binding.binding.namespace
                || owners.get(binding.instruction as usize) != Some(&Some(binding.owner))
                || self.registered_instruction_origin(binding.instruction, false) != Some((OperationSourceOrigin::Statement(binding.statement), binding.owner)) {
                return Err(failure("original callable binding is ambiguous, foreign, or changed"));
            }
            origin(binding.initializer, binding.initializer_source, binding.owner)?;
            self.verify_user_callable(pools, binding.contract)?;
            previous = Some(binding.binding);
        }
        let mut previous = None;
        for use_ in self.original_callable_uses() {
            if previous.is_some_and(|instruction| instruction >= use_.instruction) { return Err(failure("original callable uses are ambiguous or stale")); }
            let binding = self.original_callable_binding(use_.binding).ok_or_else(|| failure("callable read lost its original binding"))?;
            if use_.owner != binding.owner || use_.origin.source != binding.binding.source || use_.origin.namespace != binding.binding.namespace {
                return Err(failure("callable read changes its original lexical owner"));
            }
            if let Some(receiver) = self.original_callable_receiver(use_.instruction)? {
                if receiver.origin != use_.origin || receiver.binding != use_.binding || receiver.owner != use_.owner
                    || owners.get(use_.instruction as usize) != Some(&Some(use_.owner))
                    || self.registered_instruction_origin(use_.instruction, false).is_some() {
                    return Err(failure("saved callable receiver changes its original read transport"));
                }
            } else { origin(use_.instruction, use_.origin, use_.owner)?; }
            previous = Some(use_.instruction);
        }
        let mut expected = Vec::new();
        let mut sources = std::collections::BTreeSet::new();
        for (id, value) in self.callable_values() {
            let source = self.callable_source(value.source)?;
            if source.scope.is_some() || source.expected != value.contract || !sources.insert(value.source.index) { return Err(failure("callable value changes or duplicates its original source contract")); }
            origin(source.instruction, source.origin, source.owner)?;
            self.verify_user_callable(pools, value.contract)?;
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if sources.len() != self.callable_sources.len() || expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.callable_instructions { return Err(failure("callable value source or instruction index is missing, ambiguous, or stale")); }
        let mut expected = Vec::new();
        let mut sources = std::collections::BTreeSet::new();
        for (id, plan) in self.invocation_plans() {
            let source = self.invocation_source(plan.source)?;
            let contract = &plan.contract;
            if source.scope.is_some() || source.expected != *contract || !sources.insert(plan.source.index) { return Err(failure("invocation changes or duplicates its original source contract")); }
            origin(source.instruction, source.origin, source.owner)?;
            if let Some(receiver) = self.original_callable_receiver(contract.callee_instruction)? {
                if receiver.origin != contract.callee_origin || receiver.owner != source.owner {
                    return Err(failure("invocation callee changes its original saved receiver"));
                }
            } else { origin(contract.callee_instruction, contract.callee_origin, source.owner)?; }
            self.verify_user_callable(pools, contract.callable)?;
            if contract.timing != crate::sema::inference::InvocationDefaultTiming::AtCall || contract.effects != contract.callable.creation
                || contract.result != TypeRef::Ground(pools.signature_return_type(contract.callable.signature)?)
                || contract.binding.rest_slot.is_some() || contract.binding.dynamic.is_some() { return Err(failure("ground user invocation contract is inconsistent")); }
            let count = pools.signature_param_count(contract.callable.signature)?;
            if count > 65536 || contract.arguments.len() != contract.binding.supplied_slots.len() || contract.arguments.len() != contract.binding.operands.len() { return Err(failure("invocation argument binding is incomplete")); }
            let mut slots = std::collections::BTreeSet::new();
            for (ordinal, ((argument, &slot), &instruction)) in contract.arguments.iter().zip(&contract.binding.supplied_slots).zip(&contract.binding.operands).enumerate() {
                if slot as usize >= count || !slots.insert(slot) || instruction != argument.instruction { return Err(failure("invocation supplied slot is invalid")); }
                let crate::sema::arguments::ArgumentValueSource::Expression(expression) = argument.original.value else { return Err(failure("invocation source recipe is not prepared")); };
                if !self.argument_has_original_source(instruction, source.origin, ordinal, &argument.original, source.owner, argument.ty) { return Err(failure("invocation operand changes its original argument recipe")); }
                let TypeRef::Ground(actual) = argument.ty else { return Err(failure("invocation argument requires a scoped instance")); };
                Self::verify_type(pools, actual)?;
                let (label, formal, _) = pools.signature_param(contract.callable.signature, slot as usize)?;
                if pools.signature_parameter_rest(contract.callable.signature, slot as usize)? || argument.original.name.is_some_and(|name| name != label)
                    || !parameter_accepts(&pools.to_type(formal)?, &pools.to_type(actual)?) { return Err(failure("invocation argument disagrees with its original signature")); }
            }
            for &slot in &contract.binding.default_slots {
                if slot as usize >= count || !slots.insert(slot) || !pools.signature_parameter_defaulted(contract.callable.signature, slot as usize)? { return Err(failure("invocation default slot is invalid")); }
            }
            if slots.len() != count { return Err(failure("invocation omits a required slot")); }
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if sources.len() != self.invocation_sources.len() || expected.windows(2).any(|pair| pair[0].0 == pair[1].0) || expected != self.invocation_instructions { return Err(failure("invocation source or instruction index is missing, ambiguous, or stale")); }
        Ok(())
    }

    fn verify_ground_projection_store(&self, pools: &SemanticPools, owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        let mut sources = std::collections::BTreeSet::new();
        let mut expected = Vec::new();
        for (id, proof) in self.ground_projections() {
            let source = self.ground_projection_source(proof.source)?;
            if !sources.insert(proof.source.index) || proof.receiver_instruction != source.receiver_instruction
                || proof.receiver != TypeRef::Ground(source.receiver) || proof.result != TypeRef::Ground(source.result)
                || proof.layout != source.layout || proof.field_slot != source.field_slot { return Err(failure("ground projection changes or duplicates its original source contract")); }
            if let Some(scope) = source.scope {
                if source.owner != InstructionOwner::Function(self.scope(scope)?.owner) { return Err(failure("ground projection scope belongs to another declaration")); }
            }
            if owners.get(source.receiver_instruction as usize) != Some(&Some(source.owner))
                || source.receiver_wrappers.iter().any(|wrapper| owners.get(wrapper.instruction as usize) != Some(&Some(source.owner))) {
                return Err(failure("ground projection receiver transport belongs to another owner"));
            }
            for (instruction, origin) in [(source.instruction, source.origin), (source.receiver_source_instruction, source.receiver_origin)] {
                if owners.get(instruction as usize) != Some(&Some(source.owner))
                    || self.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(origin), source.owner)) { return Err(failure("ground projection changes its original instruction or owner")); }
            }
            Self::verify_type(pools, source.receiver)?;
            Self::verify_type(pools, source.result)?;
            let layout = self.layout(source.layout)?;
            if layout.record_type != source.receiver || layout.fields.get(source.field_slot as usize) != Some(&(source.field, source.result)) { return Err(failure("ground projection changes its original physical field contract")); }
            expected.push((source.instruction, id));
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        if sources.len() != self.ground_projection_sources.len() || expected.windows(2).any(|pair| pair[0].0 == pair[1].0)
            || expected != self.ground_projection_instructions { return Err(failure("ground projection source or instruction index is missing, ambiguous, or stale")); }
        Ok(())
    }

    fn verify_reference(&self, pools: &SemanticPools, scope: &SchemeScope, reference: TypeRef) -> Result<(), IrVerifyError> {
        self.verify_reference_inner(pools, scope, reference, &mut Vec::new())
    }

    fn verify_reference_inner(&self, pools: &SemanticPools, scope: &SchemeScope, reference: TypeRef, active: &mut Vec<TypeTemplateId>) -> Result<(), IrVerifyError> {
        if active.len() > 256 { return Err(failure("generic type template exceeds depth limit")); }
        match reference {
            TypeRef::Ground(ty) => Self::verify_type(pools, ty),
            TypeRef::Rigid(index) if scope.quantifiers.get(index as usize) == Some(&QuantifierKind::Type) => Ok(()),
            TypeRef::Rigid(_) => Err(failure("rigid generic variable is out of scope")),
            TypeRef::Template(id) => {
                if active.contains(&id) { return Err(failure("generic type template contains a cycle")); }
                active.push(id);
                let template = self.template(id)?;
                let references = match template {
                    TypeTemplate::Optional(inner) | TypeTemplate::List(inner) | TypeTemplate::Stream(inner) => vec![*inner],
                    TypeTemplate::Map { key, value } => vec![*key, *value],
                    TypeTemplate::Result { ok, error } => vec![*ok, *error],
                    TypeTemplate::Record { fields, row_tail } => {
                        if let Some(index) = row_tail && scope.quantifiers.get(*index as usize) != Some(&QuantifierKind::Row) { return Err(failure("record row tail is not a scoped row quantifier")); }
                        let names = fields.iter().map(|field| field.0).collect::<std::collections::BTreeSet<_>>();
                        if names.len() != fields.len() { return Err(failure("record template has duplicate fields")); }
                        fields.iter().map(|field| field.1).collect()
                    }
                    TypeTemplate::Arrow { parameters, result, .. } => parameters.iter().map(|parameter| parameter.ty).chain([*result]).collect(),
                };
                for reference in references { self.verify_reference_inner(pools, scope, reference, active)?; }
                active.pop();
                Ok(())
            }
        }
    }

    fn verify_witness(&self, pools: &SemanticPools, scope: &SchemeScope, requirement: &Requirement, requirement_index: usize, substitutions: &[GroundTypeId], parameter_types: &[GroundTypeId], witness: RequirementWitness) -> Result<(), IrVerifyError> {
        let mut memo = rustc_hash::FxHashMap::default();
        let mut expected = |reference| self.expand(pools, reference, substitutions, &mut memo);
        match (requirement, witness) {
            (Requirement::Eligibility { predicate, ty }, RequirementWitness::Eligibility { predicate: actual_predicate, ty: actual }) => {
                let actual_type = pools.to_type(actual)?;
                if *predicate != actual_predicate || expected(*ty)? != actual_type || !predicate.accepts_closed_display(&actual_type) {
                    return Err(failure("eligibility witness disagrees with its scoped requirement"));
                }
            }
            (Requirement::TagConstructor(constructor), RequirementWitness::TagConstructor) => self.verify_scoped_tag_constructor_witness(pools, scope, constructor, requirement_index)?,
            (Requirement::NativeMethod(method), RequirementWitness::NativeMethod(id)) => self.verify_scoped_native_method_witness(pools, scope, method, requirement_index, substitutions, id)?,
            (Requirement::Operation(operation), RequirementWitness::Operation(id)) => self.verify_scoped_operation_witness(pools, scope, operation, requirement_index, substitutions, id)?,
            (Requirement::Invocation { .. }, RequirementWitness::Invocation(id)) => self.verify_scoped_invocation_witness(pools, scope, requirement, requirement_index, substitutions, parameter_types, id)?,
            (Requirement::Projection { receiver, receiver_parameter, field, result }, RequirementWitness::Projection { layout, field_slot, result: actual }) => {
                let layout = self.layout(layout)?;
                if parameter_types.get(*receiver_parameter as usize) != Some(&layout.record_type) { return Err(failure("projection layout belongs to a different actual argument")); }
                let selected = layout.fields.get(field_slot as usize).ok_or_else(|| failure("projection field slot is out of bounds"))?;
                let receiver_type = self.instantiated_normalized_type(pools, scope, *receiver, substitutions)?;
                let actual_type = Self::normalized_ground_type(pools, layout.record_type)?;
                let exact_row = if let TypeRef::Template(id) = receiver { matches!(self.template(*id)?, TypeTemplate::Record { row_tail: Some(_), .. }) } else { false };
                let compatible = if exact_row { receiver_type == actual_type } else { normalized_parameter_accepts(&receiver_type, &actual_type) };
                if !compatible || selected.0 != *field || selected.1 != actual || Self::normalized_ground_type(pools, actual)? != self.instantiated_normalized_type(pools, scope, *result, substitutions)? {
                    return Err(failure("projection evidence disagrees with its scoped requirement"));
                }
            }
            (Requirement::Add { left, right, result }, RequirementWitness::Add { operation, left: actual_left, right: actual_right, result: actual_result }) => {
                if [expected(*left)?, expected(*right)?, expected(*result)?] != [pools.to_type(actual_left)?, pools.to_type(actual_right)?, pools.to_type(actual_result)?] {
                    return Err(failure("operation evidence disagrees with its scoped requirement"));
                }
                let left = pools.type_tag(actual_left)?;
                let right = pools.type_tag(actual_right)?;
                let result = pools.type_tag(actual_result)?;
                let valid = match operation {
                    ConcreteOperationId::AddInt => matches!(left, TypeTag::Int | TypeTag::UInt) && matches!(right, TypeTag::Int | TypeTag::UInt) && result == TypeTag::Int,
                    ConcreteOperationId::AddFloat => left == TypeTag::Float && right == TypeTag::Float && result == TypeTag::Float,
                    ConcreteOperationId::AddStr => left == TypeTag::Str && right == TypeTag::Str && result == TypeTag::Str,
                };
                if !valid { return Err(failure("concrete operation has unsupported operand or result types")); }
            }
            _ => return Err(failure("generic witness has the wrong requirement kind")),
        }
        Ok(())
    }

    pub(super) fn expand(&self, pools: &SemanticPools, reference: TypeRef, substitutions: &[GroundTypeId], memo: &mut rustc_hash::FxHashMap<TypeRef, crate::sema::types::Type>) -> Result<crate::sema::types::Type, IrVerifyError> {
        use crate::sema::types::Type;
        if let Some(ty) = memo.get(&reference) { return Ok(ty.clone()); }
        let ty = match reference {
            TypeRef::Ground(ty) => pools.to_type(ty)?,
            TypeRef::Rigid(index) => pools.to_type(*substitutions.get(index as usize).ok_or_else(|| failure("template substitution is out of scope"))?)?,
            TypeRef::Template(id) => match self.template(id)? {
                TypeTemplate::Optional(inner) => Type::Optional(Box::new(self.expand(pools, *inner, substitutions, memo)?)),
                TypeTemplate::List(inner) => Type::List(Box::new(self.expand(pools, *inner, substitutions, memo)?)),
                TypeTemplate::Stream(inner) => Type::Stream(Box::new(self.expand(pools, *inner, substitutions, memo)?)),
                TypeTemplate::Map { key, value } => Type::Map(Box::new(self.expand(pools, *key, substitutions, memo)?), Box::new(self.expand(pools, *value, substitutions, memo)?)),
                TypeTemplate::Result { ok, error } => Type::Result(Box::new(self.expand(pools, *ok, substitutions, memo)?), Box::new(self.expand(pools, *error, substitutions, memo)?)),
                TypeTemplate::Record { fields, row_tail } => {
                    let mut row = std::collections::BTreeMap::new();
                    for &(name, field) in fields { row.insert(name, self.expand(pools, field, substitutions, memo)?); }
                    if let Some(index) = row_tail {
                        let Type::Record(tail) = pools.to_type(*substitutions.get(*index as usize).ok_or_else(|| failure("row substitution is out of scope"))?)? else { return Err(failure("row substitution is not a closed known record")); };
                        for (name, ty) in tail {
                            if row.insert(name, ty).is_some() { return Err(failure("row substitution violates an explicit field lacks constraint")); }
                        }
                    }
                    Type::Record(row)
                }
                TypeTemplate::Arrow { .. } => return Err(failure("signature-bearing callable ground evidence is not prepared")),
            },
        };
        memo.insert(reference, ty.clone());
        Ok(ty)
    }

    pub(super) fn verify(&self, pools: &SemanticPools, function_count: usize, instruction_owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        pools.verify_original_contract()?;
        if self.operation_sources.len() != self.original_operation_sources.len()
            || self.operations.len() != self.original_operations.len()
            || self.operation_instructions != self.original_operation_instructions
            || self.operation_sources.iter().zip(&self.original_operation_sources).any(|(entry, original)| !Arc::ptr_eq(&entry.value, original))
            || self.operations.iter().zip(&self.original_operations).any(|(entry, original)| !Arc::ptr_eq(&entry.value, original)) {
            return Err(failure("prepared operation differs from its original receipt"));
        }
        self.verify_callable_receivers(instruction_owners)?;
        self.verify_module_invocations(instruction_owners)?;
        self.verify_native_callable_evidence(pools, instruction_owners)?;
        self.verify_value_binding_evidence(pools, instruction_owners)?;
        self.verify_iteration_evidence(pools, instruction_owners)?;
        self.verify_record_evidence(pools, instruction_owners)?;
        self.verify_comprehension_evidence(pools, instruction_owners)?;
        self.verify_scoped_operation_evidence(pools, instruction_owners)?;
        self.verify_original_eligibility(pools)?;
        pools.verify()?;
        self.verify_scoped_invocation_sources(instruction_owners)?;
        if self.original_argument_bindings.len() != self.argument_binding_receipts.len()
            || self.original_argument_bindings.iter().zip(&self.argument_binding_receipts).any(|(saved, original)| !Arc::ptr_eq(&saved.value, original)) {
            return Err(failure("saved argument differs from its original prepared receipt"));
        }
        let mut previous_argument = None;
        for saved in self.original_argument_bindings() {
            if previous_argument.is_some_and(|previous| previous >= saved.instruction) { return Err(failure("original argument bindings are ambiguous or stale")); }
            let crate::sema::arguments::ArgumentValueSource::Expression(expression) = saved.recipe.value else { return Err(failure("saved argument recipe is not prepared")); };
            if [saved.instruction, saved.initializer, saved.initializer_source_instruction, saved.wrapper].into_iter().any(|instruction| instruction_owners.get(instruction as usize) != Some(&Some(saved.owner)))
                || self.registered_instruction_origin(saved.initializer_source_instruction, false) != Some((OperationSourceOrigin::Expression(crate::sema::check::ExpressionIdentity { expression, ..saved.call }), saved.owner)) {
                return Err(failure("saved argument initializer changes its original source or owner"));
            }
            match saved.scope {
                Some(scope_id) => {
                    let scope = self.scope(scope_id)?;
                    if saved.owner != InstructionOwner::Function(scope.owner) || self.scope_for_function(scope.owner) != Some(scope_id) { return Err(failure("saved argument changes its original caller scope")); }
                    self.verify_reference(pools, scope, saved.ty)?;
                }
                None => {
                    let TypeRef::Ground(ty) = saved.ty else { return Err(failure("saved argument has an unowned symbolic type")); };
                    Self::verify_type(pools, ty)?;
                }
            }
            previous_argument = Some(saved.instruction);
        }
        let mut wrappers = self.original_argument_bindings().map(|saved| (saved.wrapper, saved.instruction)).collect::<Vec<_>>();
        wrappers.sort_unstable();
        if wrappers.windows(2).any(|pair| pair[0].0 == pair[1].0) || wrappers != self.original_argument_wrappers { return Err(failure("saved argument wrapper index is ambiguous or stale")); }
        let mut previous_nominal = None;
        for entry in &self.pattern_nominals {
            if previous_nominal.is_some_and(|identity| identity >= entry.value.identity) { return Err(failure("original pattern nominal registrations are ambiguous or stale")); }
            super::pattern::verify_pattern_nominal_member(pools, &entry.value)?;
            previous_nominal = Some(entry.value.identity);
        }
        let mut previous_origin = None;
        for &(instruction, origin, owner) in &self.instruction_origins {
            if !matches!(origin, OperationSourceOrigin::Expression(_) | OperationSourceOrigin::Stage(_) | OperationSourceOrigin::Statement(_)) { return Err(failure("instruction origin kind is unsupported")); }
            let key = (instruction, matches!(origin, OperationSourceOrigin::Stage(_)));
            if previous_origin.is_some_and(|previous| previous >= key) || instruction_owners.get(instruction as usize) != Some(&Some(owner)) { return Err(failure("instruction origins are conflicting or foreign")); }
            previous_origin = Some(key);
        }
        let mut expected_index = self.scopes().map(|(id, scope)| (scope.owner, id)).collect::<Vec<_>>();
        expected_index.sort_unstable_by_key(|entry| entry.0.raw());
        if self.function_scopes != expected_index { return Err(failure("generic function scope index is foreign or stale")); }
        let mut owners = std::collections::BTreeSet::new();
        for entry in &self.scopes {
            let scope = &entry.value;
            if scope.owner.index() >= function_count { return Err(failure("generic scope function is out of bounds")); }
            if scope.parameters.len() != scope.parameter_names.len() || scope.parameters.len() != scope.parameter_flags.len() || scope.parameter_flags.iter().any(|flags| flags & !0b111 != 0) { return Err(failure("generic parameter metadata is inconsistent")); }
            if !owners.insert(scope.owner) { return Err(failure("generic function has multiple scheme scopes")); }
            for reference in scope.parameters.iter().copied().chain([scope.result]) { self.verify_reference(pools, scope, reference)?; }
            self.verify_return_plan(pools, scope)?;
            for requirement in &scope.requirements {
                match requirement {
                    Requirement::Projection { receiver, receiver_parameter, result, .. } => {
                        if scope.parameters.get(*receiver_parameter as usize) != Some(receiver) { return Err(failure("projection receiver does not denote its scoped parameter")); }
                        for reference in [receiver, result] { self.verify_reference(pools, scope, *reference)?; }
                    }
                    Requirement::Eligibility { predicate, ty } => {
                        if *predicate != crate::sema::inference::Eligibility::Display { return Err(failure("generic eligibility predicate is not prepared")); }
                        self.verify_reference(pools, scope, *ty)?;
                    }
                    Requirement::TagConstructor(constructor) => self.verify_scoped_tag_constructor_requirement(pools, scope, constructor)?,
                    Requirement::NativeMethod(method) => self.verify_scoped_native_method_requirement(pools, scope, method)?,
                    Requirement::Operation(operation) => { operation.verify_supported()?; for reference in operation.references() { self.verify_reference(pools, scope, reference)?; } },
                    Requirement::Invocation { callable, arguments, result, .. } => {
                        for reference in [*callable, *result].into_iter().chain(arguments.iter().map(|argument| argument.ty)) { self.verify_reference(pools, scope, reference)?; }
                    }
                    Requirement::Add { left, right, result } => for reference in [left, right, result] { self.verify_reference(pools, scope, *reference)?; },
                }
            }
        }
        for entry in &self.layouts {
            let layout = &entry.value;
            let (names, types) = pools.record_fields(layout.record_type)?;
            if names.len() != layout.fields.len() { return Err(failure("physical record layout width disagrees with semantic shape")); }
            let mut seen = std::collections::BTreeSet::new();
            for &(name, ty) in &layout.fields {
                let index = names.iter().position(|field| *field == name).ok_or_else(|| failure("physical layout field is absent from semantic shape"))?;
                if !seen.insert(name) || Some(ty) != GroundTypeId::from_raw(types[index]) { return Err(failure("physical layout field type or identity is inconsistent")); }
                Self::verify_type(pools, ty)?;
            }
        }
        for entry in &self.instances {
            let instance = &entry.value;
            let scope = self.scope(instance.scope)?;
            if instance.substitutions.len() != scope.quantifiers.len() || instance.requirements.len() != scope.requirements.len() || instance.parameter_types.len() != scope.parameters.len() { return Err(failure("generic instantiation arity disagrees with its scope")); }
            for (&ty, kind) in instance.substitutions.iter().zip(&scope.quantifiers) {
                Self::verify_type(pools, ty)?;
                if *kind == QuantifierKind::Row && pools.type_tag(ty)? != TypeTag::Record { return Err(failure("row substitution is not a closed known record")); }
            }
            for (&formal, &actual) in scope.parameters.iter().zip(&instance.parameter_types) {
                Self::verify_type(pools, actual)?;
                let expected = self.instantiated_normalized_type(pools, scope, formal, &instance.substitutions)?;
                let actual = Self::normalized_ground_type(pools, actual)?;
                let exact_row = if let TypeRef::Template(id) = formal { matches!(self.template(id)?, TypeTemplate::Record { row_tail: Some(_), .. }) } else { false };
                if !(if exact_row { expected == actual } else { normalized_parameter_accepts(&expected, &actual) }) { return Err(failure("generic argument ground type disagrees with instantiated template")); }
            }
            Self::verify_type(pools, instance.result_type)?;
            if self.instantiated_normalized_type(pools, scope, scope.result, &instance.substitutions)? != Self::normalized_ground_type(pools, instance.result_type)? { return Err(failure("generic result ground type disagrees with instantiated template")); }
            for (index, (requirement, &witness)) in scope.requirements.iter().zip(&instance.requirements).enumerate() { self.verify_witness(pools, scope, requirement, index, &instance.substitutions, &instance.parameter_types, witness)?; }
        }
        for entry in &self.forwarding {
            let plan = &entry.value;
            let caller = self.scope(plan.caller)?;
            let callee = self.scope(plan.callee)?;
            if plan.substitutions.len() != callee.quantifiers.len() || plan.requirements.len() != callee.requirements.len() { return Err(failure("generic forwarding arity disagrees with callee")); }
            for (&reference, kind) in plan.substitutions.iter().zip(&callee.quantifiers) {
                if let TypeRef::Rigid(index) = reference {
                    if caller.quantifiers.get(index as usize) != Some(kind) { return Err(failure("forwarding changes quantifier kind or scope")); }
                } else { self.verify_reference(pools, caller, reference)?; }
            }
            for requirement in &plan.requirements {
                if let ForwardedRequirement::Caller(index) = requirement && *index as usize >= caller.requirements.len() { return Err(failure("forwarded requirement is out of caller scope")); }
            }
            self.verify_symbolic_forwarding(pools, plan)?;
            let mut previous = None;
            for &(from, to) in &plan.instances {
                if previous.is_some_and(|index| index >= from.index) { return Err(failure("generic forwarding contains ambiguous or unsorted edges")); }
                previous = Some(from.index);
                let from = self.instance(from)?;
                let to = self.instance(to)?;
                if from.scope != plan.caller || to.scope != plan.callee { return Err(failure("generic forwarding instance belongs to the wrong scope")); }
                let substitutions = plan.substitutions.iter().map(|reference| self.instantiated_normalized_type(pools, caller, *reference, &from.substitutions)).collect::<Result<Vec<_>, _>>()?;
                let witnesses = plan.requirements.iter().map(|requirement| match requirement { ForwardedRequirement::Caller(i) => from.requirements[*i as usize], ForwardedRequirement::Fixed(witness) => *witness }).collect::<Vec<_>>();
                let actual = to.substitutions.iter().map(|&ty| Self::normalized_ground_type(pools, ty)).collect::<Result<Vec<_>, _>>()?;
                if substitutions != actual || witnesses.as_slice() != to.requirements.as_ref() { return Err(failure("precomputed forwarding edge disagrees with canonical map")); }
            }
        }
        let check_owner = |instruction: u32, owner| if instruction_owners.get(instruction as usize) == Some(&Some(owner)) { Ok(()) } else { Err(failure("solved generic instruction has a foreign or missing owner")) };
        let mut expected_functions = self.checked_functions().map(|(id, value)| (value.declaration, id)).collect::<Vec<_>>();
        expected_functions.sort_unstable_by_key(|entry| entry.0);
        if expected_functions != self.checked_function_index || expected_functions.windows(2).any(|entries| entries[0].0 == entries[1].0) { return Err(failure("checked function declaration index is conflicting or stale")); }
        let mut targets = std::collections::BTreeSet::new();
        for (_, function) in self.checked_functions() {
            if function.target.index() >= function_count || !targets.insert(function.target) || self.scope_for_function(function.target).is_some() { return Err(failure("checked ground function target is invalid")); }
            pools.signature_param_count(function.signature)?;
            pools.signature_return_type(function.signature)?;
        }
        self.verify_callable_store(pools, instruction_owners)?;
        if self.native_call_sources.len() != self.original_native_call_sources.len() { return Err(failure("original prepared source receipt closure is incomplete")); }
        self.verify_native_call_store(pools, instruction_owners)?;
        self.verify_context_producer_evidence(pools, instruction_owners)?;
        self.verify_stage_evidence(instruction_owners)?;
        self.verify_error_constructor_evidence(pools, instruction_owners)?;
        self.verify_tag_constructor_evidence(pools, instruction_owners)?;
        self.verify_bridge_evidence(pools, instruction_owners)?;
        self.verify_mutable_path_evidence(pools, instruction_owners)?;
        self.verify_host_binding_evidence(pools, instruction_owners)?;
        self.verify_lexical_capture_evidence(pools, instruction_owners)?;
        self.verify_constant_evidence(pools, instruction_owners)?;
        self.verify_record_update_evidence(pools, instruction_owners)?;
        self.verify_conditional_evidence(pools, instruction_owners)?;
        self.verify_path_evidence(pools, instruction_owners)?;
        self.verify_try_capture_evidence(pools, instruction_owners)?;
        self.verify_compiler_wrappers(instruction_owners)?;
        self.verify_original_indices(pools, instruction_owners)?;
        self.verify_container_evidence(pools, instruction_owners)?;
        self.verify_record_constructor_evidence(pools, instruction_owners)?;
        self.verify_mutable_binding_evidence(pools, instruction_owners)?;
        self.verify_native_scalar_evidence(pools, instruction_owners)?;
        if self.ground_projection_sources.len() != self.original_ground_projection_sources.len() { return Err(failure("original prepared source receipt closure is incomplete")); }
        self.verify_ground_projection_store(pools, instruction_owners)?;
        if self.pattern_origins.windows(2).any(|pair| pair[0].0 >= pair[1].0) { return Err(failure("pattern instruction origins are ambiguous or unordered")); }
        if self.original_pattern_sources.len() != self.pattern_sources.len() { return Err(failure("pattern source receipt closure is incomplete")); }
        for (id, _) in self.pattern_sources() { self.pattern_source(id)?; }
        if self.original_pattern_applications.len() != self.pattern_applications.len() { return Err(failure("pattern application receipt closure is incomplete")); }
        for (id, _) in self.pattern_applications() { self.pattern_application(id)?; }
        let mut expected_results = self.pattern_applications().filter_map(|(id, application)| application.result.as_ref().map(|result| (result.control, id))).collect::<Vec<_>>();
        expected_results.sort_unstable_by_key(|entry| entry.0);
        if expected_results.windows(2).any(|pair| pair[0].0 >= pair[1].0) || expected_results != self.pattern_conditional_results { return Err(failure("pattern conditional result index is ambiguous or incomplete")); }
        if self.pattern_uses != self.original_pattern_uses { return Err(failure("pattern use differs from its original prepared receipt")); }
        super::pattern::verify_pattern_store(self, pools, instruction_owners)?;
        let mut expected_stage_calls = Vec::new();
        let mut stage_sources = std::collections::BTreeSet::new();
        for (id, proof) in self.ground_stage_calls() {
            let source = self.stage_call_source(proof.source)?;
            if !stage_sources.insert(proof.source.index) || source.expected != proof.contract { return Err(failure("ground stage call rewrites or duplicates its original contract")); }
            check_owner(source.instruction, source.owner)?;
            if self.registered_instruction_origin(source.instruction, true) != Some((OperationSourceOrigin::Stage(source.origin), source.owner)) { return Err(failure("stage call origin disagrees with its original instruction")); }
            let contract = &proof.contract;
            let PreparedOperationAuthority::Stage { stage, source: crate::sema::stage_graph::StageSource::Sequence { domain, outer_result: false }, form, callback_slot: Some(_), .. } = &contract.stage_authority else { return Err(failure("ground stage call lacks its selected sequence authority")); };
            if !matches!(stage, crate::syntax::node::StreamStageKind::Map | crate::syntax::node::StreamStageKind::Where) || form.callback_protocol { return Err(failure("ground stage callback protocol is not prepared")); }
            let mut stage_types = Vec::new();
            for reference in [contract.input_sequence, contract.input_item, contract.result_sequence, contract.result_item] {
                let TypeRef::Ground(ty) = reference else { return Err(failure("ground stage sequence requires a scoped substitution")); };
                Self::verify_type(pools, ty)?;
                stage_types.push(pools.to_type(ty)?);
            }
            let input_matches = match (domain, &stage_types[0]) {
                (crate::sema::stage_graph::SequenceDomain::List, crate::sema::types::Type::List(item))
                | (crate::sema::stage_graph::SequenceDomain::Stream, crate::sema::types::Type::Stream(item)) => **item == stage_types[1],
                _ => false,
            };
            if !input_matches
                || !matches!(&stage_types[2], crate::sema::types::Type::Stream(item) if **item == stage_types[3])
                || contract.operation_creation.0 & !127 != 0 { return Err(failure("ground stage sequence contract is inconsistent")); }
            let function = self.checked_function(contract.declaration)?;
            if function.target != contract.target || function.signature != contract.signature { return Err(failure("ground stage call impersonates another declaration")); }
            let count = pools.signature_param_count(contract.signature)?;
            if count > 65536 || count != contract.argument_types.len() || count != contract.argument_sources.len()
                || contract.timing != crate::sema::inference::InvocationDefaultTiming::AtCall
                || contract.creation.0 & !127 != 0 || self.call(source.instruction).is_some() { return Err(failure("ground stage call contract is inconsistent")); }
            let mut slots = std::collections::BTreeSet::new();
            for &slot in &contract.supplied_slots {
                if slot as usize >= count || !slots.insert(slot) || contract.argument_sources[slot as usize].is_none() { return Err(failure("ground stage supplied slot is invalid")); }
            }
            for &slot in &contract.default_slots {
                if slot as usize >= count || !slots.insert(slot) || contract.argument_sources[slot as usize].is_some()
                    || !pools.signature_parameter_defaulted(contract.signature, slot as usize)? { return Err(failure("ground stage default slot is invalid")); }
            }
            if slots.len() != count || contract.supplied_slots.len() != 1 { return Err(failure("ground stage call binding is incomplete")); }
            let supplied = contract.supplied_slots[0] as usize;
            if pools.to_type(contract.argument_types[supplied])? != stage_types[1] { return Err(failure("ground stage item disagrees with its original argument")); }
            let result = pools.to_type(pools.signature_return_type(contract.signature)?)?;
            match stage {
                crate::syntax::node::StreamStageKind::Map if result == stage_types[3] => {},
                crate::syntax::node::StreamStageKind::Where if result == crate::sema::types::Type::Bool && stage_types[1] == stage_types[3] => {},
                _ => return Err(failure("ground stage callback result disagrees with its selected stage authority")),
            }
            for (index, &ty) in contract.argument_types.iter().enumerate() {
                Self::verify_type(pools, ty)?;
                let (_, formal, _) = pools.signature_param(contract.signature, index)?;
                if pools.signature_parameter_rest(contract.signature, index)? || !parameter_accepts(&pools.to_type(formal)?, &pools.to_type(ty)?) { return Err(failure("ground stage argument disagrees with its fixed signature")); }
                if let Some(instruction) = contract.argument_sources[index] { check_owner(instruction, source.owner)?; }
            }
            expected_stage_calls.push((source.instruction, id));
        }
        for (id, _) in self.stage_call_sources() {
            if !stage_sources.contains(&id.index) { return Err(failure("original ground stage call is missing its proof")); }
        }
        expected_stage_calls.sort_unstable_by_key(|entry| entry.0);
        verify_instruction_order(expected_stage_calls.iter().map(|entry| entry.0))?;
        if expected_stage_calls != self.ground_stage_call_instructions { return Err(failure("ground stage call instruction index is stale")); }
        for &(instruction, origin, _) in &self.instruction_origins {
            if matches!(origin, OperationSourceOrigin::Stage(_)) && self.call(instruction).is_none() && self.ground_stage_call_at(instruction)?.is_none() {
                return Err(failure("original stage callback instruction lacks a prepared call"));
            }
        }
        let mut expected_operations = Vec::new();
        let mut proof_sources = std::collections::BTreeSet::new();
        for (id, operation) in self.operations() {
            self.operation(id)?;
            let source = self.operation_source(operation.source)?;
            if self.registered_instruction_origin(source.instruction, false) != Some((source.origin, source.owner)) { return Err(failure("operation origin disagrees with its original instruction")); }
            if !proof_sources.insert(operation.source.index) { return Err(failure("source operation has multiple prepared proofs")); }
            check_owner(source.instruction, source.owner)?;
            let owner_scope = match source.owner { InstructionOwner::Function(owner) => self.scope_for_function(owner), InstructionOwner::Driver(_) => None };
            if source.scope != owner_scope { return Err(failure("source operation scope disagrees with its declaration")); }
            if operation.authority.identity() != source.identity { return Err(failure("prepared operation authority disagrees with its source")); }
            if operation.authority != source.expected { return Err(failure("prepared operation rewrites its original selected source contract")); }
            if operation.arguments.len() > 65536 || operation.binding.operands.len() > 65536
                || operation.effects.inputs.len() > 65536 || operation.effects.outputs.len() > 65536 { return Err(failure("prepared operation payload exceeds its bound")); }
            for reference in operation.receiver.iter().copied().chain(operation.arguments.iter().flatten().copied()).chain([operation.result]) {
                if let Some(scope) = source.scope { self.verify_reference(pools, self.scope(scope)?, reference)?; }
                else if let TypeRef::Ground(ty) = reference { Self::verify_type(pools, ty)?; }
                else { return Err(failure("source operation type lacks a declaration scope")); }
            }
            for &operand in &operation.binding.operands { check_owner(operand, source.owner)?; }
            if operation.tag_equality.is_some() {
                super::full::FullVerifier::verify_prepared_tag_equality_contract(pools, operation)?;
                expected_operations.push((source.instruction, id));
                continue;
            }
            if operation.literal_comparison_slot.is_some() {
                if !super::full::FullVerifier::is_literal_comparison(pools, operation)? { return Err(failure("fused literal slot proof has another selected operation")); }
                super::full::FullVerifier::verify_prepared_literal_comparison_contract(pools, operation)?;
                expected_operations.push((source.instruction, id));
                continue;
            }
            if operation.binding.dynamic.is_some() || operation.binding.rest_slot.is_some() { return Err(failure("operation dynamic binding is not prepared")); }
            match &operation.authority {
                PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::ErrorField { .. }, .. } => {
                    super::full::FullVerifier::verify_prepared_error_field_contract(pools, operation)?;
                },
                PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Membership { .. }, .. } => {
                    super::full::FullVerifier::verify_prepared_membership_contract(pools, operation)?;
                },
                PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Arithmetic { domain: crate::sema::operation_graph::ArithmeticDomain::Float, .. }, .. } => {
                    if operation.receiver.is_some() || operation.arguments.len() != 2 || operation.binding.supplied_slots.as_ref() != [0, 1]
                        || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != 2
                        || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty()
                        || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY { return Err(failure("prepared Float operation contract is inconsistent")); }
                    for reference in operation.arguments.iter().flatten().copied().chain([operation.result]) {
                        let TypeRef::Ground(ty) = reference else { return Err(failure("selected Float operation lacks ground operand proof")); };
                        if pools.type_tag(ty)? != TypeTag::Float { return Err(failure("selected Float operation has another operand or result domain")); }
                    }
                    if operation.arguments.iter().any(Option::is_none) { return Err(failure("selected Float operation is missing an operand")); }
                },
                PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Index { .. } | crate::sema::operation_graph::PreparedLanguageOperation::ConstantKeyProjection { .. }, .. } => {
                    Self::verify_index_operation_contract(pools, operation)?;
                },
                PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Fallback { .. }, argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. } => {
                    super::full::FullVerifier::verify_prepared_fallback_contract(pools, operation)?;
                },
                PreparedOperationAuthority::Sealed { operation: crate::sema::inference::SealedOperation::AddDuration }
                |PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Arithmetic { domain: crate::sema::operation_graph::ArithmeticDomain::DurationPair | crate::sema::operation_graph::ArithmeticDomain::DurationScale { .. } | crate::sema::operation_graph::ArithmeticDomain::DurationRatio, .. }, .. } => {
                    Self::verify_duration_operation_contract(pools, operation)?;
                },
                PreparedOperationAuthority::Sealed { operation: crate::sema::inference::SealedOperation::AddInt }
                | PreparedOperationAuthority::Language { operation: crate::sema::operation_graph::PreparedLanguageOperation::Arithmetic { domain: crate::sema::operation_graph::ArithmeticDomain::Integer { left: crate::sema::inference::Atom::Int | crate::sema::inference::Atom::UInt, right: crate::sema::inference::Atom::Int | crate::sema::inference::Atom::UInt }, .. }, .. } => {
                    super::full::FullVerifier::verify_prepared_integer_arithmetic_contract(pools, operation)?;
                },
                PreparedOperationAuthority::Language { operation: selected, argument_order: crate::sema::operation_graph::OperationArgumentOrder::SourceOrder, statement_result_is_unit: false, .. } => {
                    if matches!(selected, crate::sema::operation_graph::PreparedLanguageOperation::Constructor { kind: crate::sema::operation_graph::ValueConstructor::Range, .. }) {
                        self.verify_range_operation_contract(pools, operation)?;
                        expected_operations.push((source.instruction, id));
                        continue;
                    }
                    if super::full::FullVerifier::is_null_optional_equality(pools, operation)? {
                        super::full::FullVerifier::verify_prepared_null_optional_equality_contract(pools, operation)?;
                        expected_operations.push((source.instruction, id));
                        continue;
                    }
                    use crate::sema::inference::Atom;
                    use crate::sema::operation_graph::{ArithmeticDomain, PreparedLanguageOperation};
                    let (operand, result) = match selected {
                        PreparedLanguageOperation::Arithmetic { domain: ArithmeticDomain::Integer { left: Atom::Int, right: Atom::Int }, .. } => (TypeTag::Int, TypeTag::Int),
                        PreparedLanguageOperation::Ordering { left: Atom::Int, right: Atom::Int, op: crate::syntax::node::BinaryOp::Lt | crate::syntax::node::BinaryOp::Le | crate::syntax::node::BinaryOp::Gt | crate::syntax::node::BinaryOp::Ge } => (TypeTag::Int, TypeTag::Bool),
                        PreparedLanguageOperation::Ordering { left: Atom::Str, right: Atom::Str, .. }
                        | PreparedLanguageOperation::Equality { op: crate::syntax::node::BinaryOp::Eq | crate::syntax::node::BinaryOp::Ne } => (TypeTag::Str, TypeTag::Bool),
                        _ => return Err(failure("selected operation authority is not prepared")),
                    };
                    if operation.receiver.is_some() || operation.arguments.len() != 2 || operation.binding.supplied_slots.as_ref() != [0, 1]
                        || !operation.binding.default_slots.is_empty() || operation.binding.operands.len() != 2
                        || !operation.effects.inputs.is_empty() || !operation.effects.outputs.is_empty()
                        || operation.effects.creation != crate::sema::inference::EffectSet::EMPTY { return Err(failure("prepared primitive operation contract is inconsistent")); }
                    for reference in operation.arguments.iter() {
                        let Some(TypeRef::Ground(ty)) = reference else { return Err(failure("selected primitive operation lacks ground operand proof")); };
                        if pools.type_tag(*ty)? != operand { return Err(failure("selected primitive operation has another operand domain")); }
                    }
                    let TypeRef::Ground(ty) = operation.result else { return Err(failure("selected primitive operation lacks a ground result")); };
                    if pools.type_tag(ty)? != result { return Err(failure("selected primitive operation has another result domain")); }
                },
                _ => return Err(failure("selected operation authority is not prepared")),
            }
            expected_operations.push((source.instruction, id));
        }
        for (id, source) in self.operation_sources() {
            if !matches!(source.origin, OperationSourceOrigin::Expression(_)) { return Err(failure("source operation origin is not prepared")); }
            if !proof_sources.contains(&id.index) { return Err(failure("source operation is missing its prepared proof")); }
        }
        expected_operations.sort_unstable_by_key(|entry| entry.0);
        verify_instruction_order(expected_operations.iter().map(|entry| entry.0))?;
        if expected_operations != self.operation_instructions { return Err(failure("source operation instruction index is foreign or stale")); }
        verify_instruction_order(self.calls.iter().map(|call| call.instruction))?;
        verify_instruction_order(self.uses.iter().map(|use_| use_.instruction))?;
        verify_instruction_order(self.constructors.iter().map(|record| record.instruction))?;
        for call in &self.calls {
            check_owner(call.instruction, call.caller)?;
            self.scope(call.target)?;
            match call.evidence {
                CallEvidence::Ground(instance) if self.instance(instance)?.scope == call.target => {},
                CallEvidence::Forwarded(plan) if self.forwarding(plan)?.callee == call.target && InstructionOwner::Function(self.scope(self.forwarding(plan)?.caller)?.owner) == call.caller => {},
                _ => return Err(failure("solved generic call evidence targets the wrong scope")),
            }
        }
        let mut previous_argument = None;
        for argument in &self.arguments {
            let key = (argument.call_instruction, argument.parameter);
            if previous_argument.is_some_and(|previous| previous >= key) { return Err(failure("generic arguments are not unique and sorted")); }
            previous_argument = Some(key);
            let call = self.call(argument.call_instruction).ok_or_else(|| failure("generic argument has no solved call"))?;
            if let Some(source) = argument.source_instruction { check_owner(source, call.caller)?; }
            if argument.parameter as usize >= self.scope(call.target)?.parameters.len() { return Err(failure("generic argument parameter is out of bounds")); }
            match call.evidence {
                CallEvidence::Ground(instance) => {
                    let TypeRef::Ground(ty) = argument.ty else { return Err(failure("ground generic call argument has an unbound template")); };
                    Self::verify_type(pools, ty)?;
                    if self.instance(instance)?.parameter_types[argument.parameter as usize] != ty { return Err(failure("ground argument disagrees with its call instance")); }
                }
                CallEvidence::Forwarded(id) => {
                    let forwarding = self.forwarding(id)?;
                    self.verify_reference(pools, self.scope(forwarding.caller)?, argument.ty)?;
                    for &(from, to) in &forwarding.instances {
                        let ty = self.instantiated_normalized_type(pools, self.scope(forwarding.caller)?, argument.ty, &self.instance(from)?.substitutions)?;
                        if ty != Self::normalized_ground_type(pools, self.instance(to)?.parameter_types[argument.parameter as usize])? { return Err(failure("forwarded argument disagrees with its call instance")); }
                    }
                }
            }
        }
        for call in &self.calls {
            let arguments = self.call_arguments(call.instruction);
            if arguments.len() != self.scope(call.target)?.parameters.len() || arguments.iter().enumerate().any(|(index, argument)| argument.parameter as usize != index) { return Err(failure("generic call lacks complete argument type evidence")); }
        }
        for use_ in &self.uses {
            let scope = self.scope(use_.scope)?;
            check_owner(use_.instruction, InstructionOwner::Function(scope.owner))?;
            if use_.requirement as usize >= scope.requirements.len() { return Err(failure("generic instruction requirement is out of scope")); }
        }
        for record in &self.constructors { check_owner(record.instruction, record.owner)?; self.layout(record.layout)?; }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::semantic::SemanticPoolBuilder;
    use crate::sema::types::Type;
    use std::collections::BTreeMap;

    fn scalar(pools: &mut SemanticPools, builder: &mut SemanticPoolBuilder, ty: Type) -> GroundTypeId {
        builder.intern_type(pools, &ty).unwrap()
    }

    fn identity(owner: usize) -> SchemeScope {
        SchemeScope { owner: IrFunctionId::new(owner).unwrap(), quantifiers: Box::new([QuantifierKind::Type]), parameters: Box::new([TypeRef::Rigid(0)]), parameter_names: Box::new([Name::intern("value")]), parameter_flags: Box::new([0]), kind: CallableKind::Pure, result: TypeRef::Rigid(0), requirements: Box::new([]), return_plan: GenericReturnPlan::Value }
    }

    fn addition(owner: usize) -> SchemeScope {
        let mut scope = identity(owner);
        scope.parameters = Box::new([TypeRef::Rigid(0), TypeRef::Rigid(0)]);
        scope.parameter_names = Box::new([Name::intern("left"), Name::intern("right")]);
        scope.parameter_flags = Box::new([0, 0]);
        scope.requirements = Box::new([Requirement::Add { left: TypeRef::Rigid(0), right: TypeRef::Rigid(0), result: TypeRef::Rigid(0) }]);
        scope
    }

    #[test]
    fn prepared_component_templates_remap_sparse_member_binders_and_reject_siblings() {
        use crate::sema::inference::{Arrow, Atom, CallableKind, ComponentMember, EffectSet, EffectSummary, Generalization, InferenceContext, Parameter, RowField};
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let span = crate::source::Span::new(crate::source::SourceId::new(0), 0, 1);
        for row_member in [false, true] {
            let mut graph = InferenceContext::default();
            let first = graph.fresh(1, span).unwrap();
            let second = if row_member {
                let tail = graph.fresh_row(1, span).unwrap();
                let int = graph.atom(Atom::Int).unwrap();
                let row = graph.row(vec![RowField { label: Name::intern("tag"), ty: int }], Some(tail)).unwrap();
                graph.record(row).unwrap()
            } else { graph.fresh(1, span).unwrap() };
            let roots = [first, second].map(|ty| graph.arrow(Arrow {
                kind: CallableKind::Pure,
                params: vec![Parameter { label: Name::intern("value"), ty, defaulted: false, rest: false }],
                result: ty, effects: EffectSummary::Closed(EffectSet::EMPTY),
            }).unwrap());
            let members = roots.map(|root| ComponentMember { root, requirements: Vec::new(), policy: Generalization::Allowed });
            let schemes = graph.generalize_component(&members, 0, None).unwrap();
            let second = graph.resolved(second).unwrap();
            let mut pools = SemanticPools::default();
            let mut semantic = SemanticPoolBuilder::default();
            let mut builder = GenericEvidenceBuilder::default();
            let prepared = builder.prepare_reference(&graph, schemes[1], second, &mut pools, &mut semantic).unwrap();
            if row_member {
                let TypeRef::Template(template) = prepared else { panic!("row member must retain a template"); };
                let TypeTemplate::Record { row_tail, .. } = builder.template(template).unwrap() else { panic!("row member must retain a record"); };
                assert_eq!(*row_tail, Some(0));
            } else { assert_eq!(prepared, TypeRef::Rigid(0)); }
            let sibling = graph.scheme_type_binders(schemes[0]).unwrap()[0];
            assert!(builder.prepare_reference(&graph, schemes[1], sibling, &mut pools, &mut semantic).is_err());
        }
    }

    #[test]
    fn prepared_record_template_flattens_solved_field_extensions() {
        let source = "pure both(entry) { let _: Int = entry.age; entry.name }";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
        let bodies = crate::sema::check::Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let declaration = bodies.solved.declarations.values().next().unwrap();
        let graph = &bodies.solved.graph;
        let crate::sema::inference::TypeNode::Arrow(arrow) = graph.node(graph.resolved(declaration.signature).unwrap()).unwrap() else { panic!("declaration must be callable"); };
        let mut pools = SemanticPools::default();
        let mut semantic = SemanticPoolBuilder::default();
        let mut builder = GenericEvidenceBuilder::default();
        let TypeRef::Template(template) = builder.prepare_reference(graph, declaration.scheme, arrow.params[0].ty, &mut pools, &mut semantic).unwrap() else { panic!("open record must retain a scoped template"); };
        let TypeTemplate::Record { fields, row_tail } = builder.template(template).unwrap() else { panic!("record parameter must retain its fields"); };
        let _symbols = parsed.arena.symbol_owner().enter();
        assert_eq!(fields.iter().map(|(name, _)| *name).collect::<Vec<_>>(), vec![Name::intern("age"), Name::intern("name")]);
        assert!(row_tail.is_some());
        let TypeRef::Ground(age) = fields[0].1 else { panic!("written age type must stay ground"); };
        assert_eq!(pools.to_type(age).unwrap(), Type::Int);
    }

    #[test]
    fn identity_evidence_retains_one_scope_for_distinct_payloads_and_fixed_return_plan() {
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default();
        let mut builder = GenericEvidenceBuilder::default(); let scope = builder.add_scope(identity(0)).unwrap();
        let mut instances = Vec::new();
        for ty in [Type::Int, Type::Str, Type::Bool, Type::Any] {
            let ty = scalar(&mut pools, &mut types, ty);
            instances.push(builder.add_instance(Instantiation { scope, substitutions: Box::new([ty]), parameter_types: Box::new([ty]), result_type: ty, requirements: Box::new([]) }).unwrap());
        }
        let store = builder.finish(&pools, 1, &[]).unwrap();
        assert_eq!(store.scopes.len(), 1);
        for instance in instances { assert_eq!(store.instance(instance).unwrap().scope, scope); }
        assert_eq!(store.scope(scope).unwrap().return_plan, GenericReturnPlan::Value);
    }

    #[test]
    fn foreign_retired_and_out_of_range_evidence_cannot_become_valid_by_slot_reuse() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut builder = GenericEvidenceBuilder::default(); let other = GenericEvidenceBuilder::default();
        assert!(builder.rewind(other.checkpoint()).is_err());
        let checkpoint = builder.checkpoint(); let retired = builder.add_scope(identity(0)).unwrap();
        let retired_template = builder.add_template(TypeTemplate::List(TypeRef::Rigid(0))).unwrap();
        builder.rewind(checkpoint).unwrap(); let replacement = builder.add_scope(identity(0)).unwrap();
        assert_eq!(retired.index, replacement.index); assert_ne!(retired.proof, replacement.proof);
        let replacement_template = builder.add_template(TypeTemplate::List(TypeRef::Rigid(0))).unwrap();
        assert_eq!(retired_template.index, replacement_template.index); assert_ne!(retired_template.proof, replacement_template.proof);
        assert!(builder.store.template(retired_template).is_err());
        assert!(builder.store.scope(retired).is_err()); assert!(other.store.scope(replacement).is_err());
        let malformed = SchemeScopeId { index: 9, ..replacement }; assert!(builder.store.scope(malformed).is_err());
    }

    #[test]
    fn rejected_cross_child_rewind_preserves_live_receipts_indexes_and_serials() {
        crate::runtime::eval::run_eval(|| {
            use crate::sema::check::{BindingIdentity, Checker, ExpressionIdentity, StatementIdentity};
            use crate::sema::inference::ScopedRoot;
            use crate::syntax::arena::{ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind};
            let source = "let marker: Str = \"kept\"\nproc quoted(values: List[Str]) [] { for value in values { let _ = value } }\n";
            let (_, parsed) = crate::loader::parse_load_entry_source_arena_only(
                "checkpoint-receipts.xsh", crate::loader::entry_source_from_text("checkpoint-receipts.xsh", source.to_owned()), Vec::new());
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let checked = Checker::check_arena(&parsed.arena, source);
            assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
            let _symbols = parsed.arena.symbol_owner().enter();
            let solved = &checked.solved;
            let (&statement, operation) = solved.statement_operations.iter().next().unwrap();
            let ArenaStmtKind::For { target, iter: iterator, .. } = parsed.arena.arena.stmt(statement.statement).kind else { panic!("fixture must retain its original loop"); };
            let binding = BindingIdentity { source: statement.source, namespace: statement.namespace, target };
            let iterator_origin = ExpressionIdentity { source: statement.source, namespace: statement.namespace, expression: iterator };
            let selected = solved.graph.candidate_evidence(operation.requirement).unwrap().unwrap();
            let crate::sema::check::SolvedOperationAuthority::Language(metadata) = solved.operation_catalog.candidate(&solved.graph, selected.candidate).unwrap() else { panic!("iteration must retain language authority"); };
            let authority = PreparedOperationAuthority::Language { identity: metadata.identity, authority: metadata.authority, operation: metadata.operation, argument_order: metadata.argument_order, statement_result_is_unit: metadata.statement_result_is_unit };
            let read_origin = *solved.expressions.keys().find(|origin| matches!(parsed.arena.arena.expr(origin.expression).kind, ArenaExprKind::Ident(name) if name == Name::intern("value"))).unwrap();
            let (&marker, marker_type) = solved.bindings.iter().find(|(identity, _)| matches!(parsed.arena.arena.binding_target(identity.target).kind, ArenaBindingTargetKind::Name(name) if name == Name::intern("marker"))).unwrap();
            let marker_statement = parsed.arena.arena.stmt_ids(parsed.arena.statements).find(|&statement| matches!(parsed.arena.arena.stmt(statement).kind, ArenaStmtKind::Let { target, .. } if target == marker.target)).unwrap();
            let ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(initializer), .. } = parsed.arena.arena.stmt(marker_statement).kind else { panic!("fixture must retain its original initializer"); };
            let initializer_source = ExpressionIdentity { source: marker.source, namespace: marker.namespace, expression: initializer };
            let mut pools = SemanticPools::default();
            let mut types = SemanticPoolBuilder::default();
            let item = scalar(&mut pools, &mut types, Type::Str);
            let input = scalar(&mut pools, &mut types, Type::List(Box::new(Type::Str)));
            let iteration = OriginalIterationBinding { statement, binding, iterator_origin, authority, instruction: 0, iterator: 1, slot: 1, body: super::super::IrBlockId::new(0).unwrap(), owner: InstructionOwner::Function(IrFunctionId::new(0).unwrap()), input, item, binding_type: item, iterator_parameter: Some((operation.caller.unwrap(), 0)), iterator_carrier: None, producer: None };
            let value = ValueBindingSource { binding: ValueBindingIdentity::Named(marker), statement: StatementIdentity { source: marker.source, namespace: marker.namespace, statement: marker_statement }, initializer_source,
                source_type: ScopedRoot { ty: marker_type.ty, scope: marker_type.scheme }, initializer_type: ScopedRoot { ty: solved.expressions[&initializer_source], scope: solved.expression_scope(initializer_source, None).unwrap() },
                expected: ValueBindingContract { allocation: ValueBindingAllocation::Value, instruction: 3, initializer: 4, initializer_source_instruction: 4, initializer_wrappers: Box::new([]), with_bindings: Box::new([]), slot: 0, owner: InstructionOwner::Driver(0), binding_type: item, initializer_type: item, scope: None } };
            let mut builder = GenericEvidenceBuilder::default();
            let empty = builder.checkpoint();
            let retired_value = builder.add_value_binding_source(value.clone()).unwrap();
            let stale = builder.checkpoint();
            builder.rewind(empty).unwrap();
            let replacement_value = builder.add_value_binding_source(value).unwrap();
            assert!(builder.store.value_binding_source(retired_value).is_err());
            let live_iteration = builder.add_iteration_binding(iteration.clone()).unwrap();
            builder.add_iteration_use(OriginalIterationUse { origin: read_origin, binding: live_iteration, instruction: 2, owner: iteration.owner, tag: crate::runtime::eval::indexed::full::FullTag::ExprParam }).unwrap();
            let scope = builder.add_scope(identity(1)).unwrap();
            let template = builder.add_template(TypeTemplate::List(TypeRef::Rigid(0))).unwrap();
            builder.store.iterations.finish();
            builder.store.values.finish();
            let before = (builder.checkpoint(), builder.next_serial, format!("{:?}", builder.store), builder.canonical_templates.clone(), builder.canonical_instances.clone());
            let error = builder.rewind(stale).unwrap_err();
            assert!(error.message.contains("replacement"));
            let after = (builder.checkpoint(), builder.next_serial, format!("{:?}", builder.store), builder.canonical_templates.clone(), builder.canonical_instances.clone());
            assert_eq!(after, before, "a later stale child checkpoint must not alter any earlier collection or derived index");
            assert!(builder.store.value_binding_source(replacement_value).is_ok());
            assert!(builder.store.iteration_binding(live_iteration).is_ok());
            assert!(builder.store.iteration_use(2).unwrap().is_some());
            assert!(builder.store.scope(scope).is_ok());
            assert_eq!(builder.add_template(TypeTemplate::List(TypeRef::Rigid(0))).unwrap(), template);
        });
    }

    #[test]
    fn projection_uses_constructor_layout_and_rejects_wrong_slot_type_and_owner() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default();
        let int = scalar(&mut pools, &mut types, Type::Int); let text = scalar(&mut pools, &mut types, Type::Str);
        let field = Name::intern("value"); let extra = Name::intern("extra");
        let record = scalar(&mut pools, &mut types, Type::Record(BTreeMap::from([(field, Type::Int), (extra, Type::Str)])));
        let mut builder = GenericEvidenceBuilder::default(); let mut body = identity(0); body.quantifiers = Box::new([QuantifierKind::Type, QuantifierKind::Type]); body.result = TypeRef::Rigid(1);
        body.requirements = Box::new([Requirement::Projection { receiver: TypeRef::Rigid(0), receiver_parameter: 0, field, result: TypeRef::Rigid(1) }]);
        let scope = builder.add_scope(body).unwrap();
        let layout = builder.add_layout(PhysicalLayout { record_type: record, fields: Box::new([(extra, text), (field, int)]) }).unwrap();
        let instance = builder.add_instance(Instantiation { scope, substitutions: Box::new([record, int]), parameter_types: Box::new([record]), result_type: int, requirements: Box::new([RequirementWitness::Projection { layout, field_slot: 1, result: int }]) }).unwrap();
        builder.add_requirement_use(SolvedRequirementUse { instruction: 0, scope, requirement: 0 });
        builder.add_constructor(SolvedRecordLayout { instruction: 1, owner: InstructionOwner::Function(IrFunctionId::new(0).unwrap()), layout });
        let store = builder.finish(&pools, 1, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap())); 2]).unwrap();
        assert!(matches!(store.instance(instance).unwrap().requirements[0], RequirementWitness::Projection { field_slot: 1, .. }));
        for slot in [0, 2] {
            let mut bad = store.clone(); bad.instances[instance.index as usize].value.requirements[0] = RequirementWitness::Projection { layout, field_slot: slot, result: int };
            assert!(bad.verify(&pools, 1, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap())); 2]).is_err());
        }
        let mut bad = store.clone(); bad.layouts[layout.index as usize].value.fields[1].1 = text;
        assert!(bad.verify(&pools, 1, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap())); 2]).is_err());
        assert!(store.verify(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(1).unwrap())); 2]).is_err());
    }

    #[test]
    fn concrete_add_witnesses_reject_foreign_signatures_and_erased_repairs() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        for (ty, operation, accepted) in [(Type::Int, ConcreteOperationId::AddInt, true), (Type::Float, ConcreteOperationId::AddFloat, true), (Type::Str, ConcreteOperationId::AddStr, true), (Type::Bool, ConcreteOperationId::AddInt, false), (Type::Int, ConcreteOperationId::AddStr, false), (Type::Any, ConcreteOperationId::AddInt, false)] {
            let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default(); let ty = scalar(&mut pools, &mut types, ty);
            let mut builder = GenericEvidenceBuilder::default(); let scope = builder.add_scope(addition(0)).unwrap();
            builder.add_instance(Instantiation { scope, substitutions: Box::new([ty]), parameter_types: Box::new([ty, ty]), result_type: ty, requirements: Box::new([RequirementWitness::Add { operation, left: ty, right: ty, result: ty }]) }).unwrap();
            assert_eq!(builder.finish(&pools, 1, &[]).is_ok(), accepted);
        }
    }

    #[test]
    fn forwarding_precomputes_scoped_edges_and_rejects_missing_or_ambiguous_maps() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default(); let int = scalar(&mut pools, &mut types, Type::Int);
        let mut builder = GenericEvidenceBuilder::default(); let caller = builder.add_scope(addition(0)).unwrap(); let callee = builder.add_scope(addition(1)).unwrap();
        let witness = RequirementWitness::Add { operation: ConcreteOperationId::AddInt, left: int, right: int, result: int };
        let from = builder.add_instance(Instantiation { scope: caller, substitutions: Box::new([int]), parameter_types: Box::new([int, int]), result_type: int, requirements: Box::new([witness]) }).unwrap();
        let to = builder.add_instance(Instantiation { scope: callee, substitutions: Box::new([int]), parameter_types: Box::new([int, int]), result_type: int, requirements: Box::new([witness]) }).unwrap();
        let forwarding = builder.add_forwarding(ForwardingPlan { caller, callee, substitutions: Box::new([TypeRef::Rigid(0)]), requirements: Box::new([ForwardedRequirement::Caller(0)]), instances: Box::new([(from, to)]) }).unwrap();
        builder.add_call(SolvedCall { instruction: 0, caller: InstructionOwner::Function(IrFunctionId::new(0).unwrap()), target: callee, evidence: CallEvidence::Forwarded(forwarding) });
        for parameter in 0..2 { builder.add_argument(SolvedArgument { call_instruction: 0, parameter, source_instruction: None, ty: TypeRef::Rigid(0) }); }
        let store = builder.finish(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap()))]).unwrap();
        assert_eq!(store.forwarded_instance(forwarding, from).unwrap(), to); assert!(store.forwarded_instance(forwarding, to).is_err());
        let mut bad = store.clone(); bad.forwarding[forwarding.index as usize].value.requirements[0] = ForwardedRequirement::Caller(1); assert!(bad.verify(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap()))]).is_err());
        let mut bad = store.clone(); bad.forwarding[forwarding.index as usize].value.instances = Box::new([(from, to), (from, to)]); assert!(bad.verify(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap()))]).is_err());
        let mut bad = store.clone(); bad.calls.push(store.calls[0]); assert!(bad.verify(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap()))]).is_err());
    }
    #[test]
    fn row_tail_substitution_preserves_width_and_explicit_dynamic_fields() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default();
        let name = Name::intern("name"); let extra = Name::intern("extra");
        let any = scalar(&mut pools, &mut types, Type::Any);
        let bool_type = scalar(&mut pools, &mut types, Type::Bool);
        let tail = scalar(&mut pools, &mut types, Type::Record(BTreeMap::from([(extra, Type::Bool)])));
        let record = scalar(&mut pools, &mut types, Type::Record(BTreeMap::from([(name, Type::Any), (extra, Type::Bool)])));
        let mut builder = GenericEvidenceBuilder::default();
        let receiver = TypeRef::Template(builder.add_template(TypeTemplate::Record { fields: Box::new([(name, TypeRef::Rigid(0))]), row_tail: Some(1) }).unwrap());
        let mut declaration = identity(0);
        declaration.quantifiers = Box::new([QuantifierKind::Type, QuantifierKind::Row]);
        declaration.parameters = Box::new([receiver]);
        declaration.requirements = Box::new([Requirement::Projection { receiver, receiver_parameter: 0, field: name, result: TypeRef::Rigid(0) }]);
        let scope = builder.add_scope(declaration).unwrap();
        let layout = builder.add_layout(PhysicalLayout { record_type: record, fields: Box::new([(name, any), (extra, bool_type)]) }).unwrap();
        let proof = Instantiation { scope, substitutions: Box::new([any, tail]), parameter_types: Box::new([record]), result_type: any, requirements: Box::new([RequirementWitness::Projection { layout, field_slot: 0, result: any }]) };
        let instance = builder.add_instance(proof.clone()).unwrap();
        assert_eq!(builder.add_instance(proof).unwrap(), instance);
        let store = builder.finish(&pools, 1, &[]).unwrap();
        assert_eq!(store.instances.len(), 1);
        assert_eq!(pools.to_type(store.instance(instance).unwrap().result_type).unwrap(), Type::Any);
        let mut whole_record_as_tail = store.clone(); whole_record_as_tail.instances[instance.index as usize].value.substitutions[1] = record;
        assert!(whole_record_as_tail.verify(&pools, 1, &[]).is_err());
        let mut wrong_kind = store.clone(); wrong_kind.scopes[scope.index as usize].value.quantifiers[1] = QuantifierKind::Type;
        assert!(wrong_kind.verify(&pools, 1, &[]).is_err());
        let mut erased_receiver = store.clone();
        let erased = scalar(&mut pools, &mut types, Type::ErasedRecord);
        erased_receiver.instances[instance.index as usize].value.parameter_types[0] = erased;
        assert!(erased_receiver.verify(&pools, 1, &[]).is_err());
    }

    #[test]
    fn nested_templates_and_any_substitution_cannot_launder_concrete_arguments() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default();
        let any = scalar(&mut pools, &mut types, Type::Any);
        let int = scalar(&mut pools, &mut types, Type::Int);
        let error = scalar(&mut pools, &mut types, Type::Error);
        let input = Type::List(Box::new(Type::Optional(Box::new(Type::Any))));
        let argument = scalar(&mut pools, &mut types, input.clone());
        let result_type = scalar(&mut pools, &mut types, Type::Result(Box::new(input), Box::new(Type::Error)));
        let mut builder = GenericEvidenceBuilder::default();
        let optional = TypeRef::Template(builder.add_template(TypeTemplate::Optional(TypeRef::Rigid(0))).unwrap());
        let list = TypeRef::Template(builder.add_template(TypeTemplate::List(optional)).unwrap());
        let result = TypeRef::Template(builder.add_template(TypeTemplate::Result { ok: list, error: TypeRef::Ground(error) }).unwrap());
        let mut declaration = identity(0); declaration.parameters = Box::new([list]); declaration.result = result; declaration.return_plan = GenericReturnPlan::Result;
        let scope = builder.add_scope(declaration).unwrap();
        let instance = builder.add_instance(Instantiation { scope, substitutions: Box::new([any]), parameter_types: Box::new([argument]), result_type, requirements: Box::new([]) }).unwrap();
        let store = builder.finish(&pools, 1, &[]).unwrap();
        let mut wrong_argument = store.clone(); wrong_argument.instances[instance.index as usize].value.parameter_types[0] = scalar(&mut pools, &mut types, Type::List(Box::new(Type::Optional(Box::new(Type::Int)))));
        assert!(wrong_argument.verify(&pools, 1, &[]).is_err());
        let mut wrong_result = store.clone(); wrong_result.instances[instance.index as usize].value.result_type = int;
        assert!(wrong_result.verify(&pools, 1, &[]).is_err());
        let mut foreign_template = store.clone(); let mut outsider = GenericEvidenceBuilder::default();
        foreign_template.scopes[scope.index as usize].value.result = TypeRef::Template(outsider.add_template(TypeTemplate::List(TypeRef::Ground(any))).unwrap());
        assert!(foreign_template.verify(&pools, 1, &[]).is_err());
    }

}

fn verify_instruction_order(instructions: impl Iterator<Item = u32>) -> Result<(), IrVerifyError> {
    let mut previous = None;
    for instruction in instructions {
        if previous.is_some_and(|previous| previous >= instruction) { return Err(failure("solved generic instruction map is not unique and sorted")); }
        previous = Some(instruction);
    }
    Ok(())
}

fn parameter_accepts(formal: &crate::sema::types::Type, actual: &crate::sema::types::Type) -> bool {
    use crate::sema::types::Type;
    match (formal, actual) {
        (Type::Record(expected), Type::Record(fields)) => expected.iter().all(|(name, ty)| fields.get(name) == Some(ty)),
        _ => formal == actual,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GenericCheckpoint { module_invocations: usize, eligibility: usize, callable_receivers: usize, native_callables: native_callables::NativeCallableCheckpoint, values: value_bindings::ValueBindingCheckpoint, operation_requirements: operation_requirements::OperationRequirementCheckpoint, iterations: iterations::IterationCheckpoint, records: records::RecordCheckpoint, comprehensions: comprehensions::ComprehensionCheckpoint, compiler_wrappers: usize, indices: usize, containers: containers::ContainerCheckpoint, record_constructors: record_constructors::RecordConstructorCheckpoint, mutables: mutable_bindings::MutableBindingCheckpoint, native_scalars: native_scalars::NativeScalarCheckpoint, try_captures: captures::TryCaptureCheckpoint, context_producers: context_producers::ContextProducerCheckpoint, stages: usize, error_constructors: usize, tag_constructors: usize, bridges: bridges::BridgeCheckpoint, mutable_paths: mutable_paths::MutablePathCheckpoint, host_bindings: host_bindings::HostBindingCheckpoint, lexical_captures: lexical_captures::LexicalCaptureCheckpoint, constants: constants::ConstantCheckpoint, record_updates: record_updates::RecordUpdateCheckpoint, conditionals: conditionals::ConditionalCheckpoint, paths: paths::PathCheckpoint, scoped_invocation_sources: usize, scoped_invocation_witnesses: usize, original_argument_bindings: usize, pattern_nominals: usize, original_callable_bindings: usize, original_callable_uses: usize, native_call_sources: usize, ground_native_calls: usize, pattern_origins: usize, pattern_sources: usize, pattern_applications: usize, pattern_captures: usize, pattern_uses: usize, ground_projection_sources: usize, ground_projections: usize, callable_sources: usize, callable_values: usize, invocation_sources: usize, invocation_plans: usize, root: u64, serial_limit: u64, scopes: usize, layouts: usize, instances: usize, forwarding: usize, templates: usize, calls: usize, arguments: usize, uses: usize, constructors: usize, operation_sources: usize, operations: usize, checked_functions: usize, stage_call_sources: usize, ground_stage_calls: usize, instruction_origins: usize }

pub(in crate::runtime::eval) struct GenericEvidenceBuilder { canonical_pattern_nominals: std::collections::BTreeMap<crate::sema::check::QualifiedNominalIdentity, usize>, store: GenericEvidenceStore, next_serial: u64, canonical_templates: rustc_hash::FxHashMap<TypeTemplate, TypeTemplateId>, canonical_instances: rustc_hash::FxHashMap<Instantiation, InstantiationId> }

impl Default for GenericEvidenceBuilder {
    fn default() -> Self {
        let root = NEXT_ROOT.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1)).expect("generic proof roots exhausted");
        Self { canonical_pattern_nominals: std::collections::BTreeMap::new(), store: GenericEvidenceStore { eligibility: eligibility_requirements::EligibilityEvidence::default(), callable_receivers: callable_receivers::CallableReceiverEvidence::default(), module_invocations: module_invocations::ModuleInvocationEvidence::default(), native_callables: native_callables::NativeCallableEvidence::default(), values: value_bindings::ValueBindingEvidence::default(), operation_requirements: operation_requirements::OperationRequirementEvidence::default(), iterations: iterations::IterationEvidence::default(), records: records::RecordEvidence::default(), comprehensions: comprehensions::ComprehensionEvidence::default(), compiler_wrappers: compiler_wrappers::CompilerWrapperEvidence::default(), indices: indexing::IndexEvidence::default(), containers: containers::ContainerEvidence::default(), record_constructors: record_constructors::RecordConstructorEvidence::default(), mutables: mutable_bindings::MutableBindingEvidence::default(), native_scalars: native_scalars::NativeScalarEvidence::default(), try_captures: captures::TryCaptureEvidence::default(), context_producers: context_producers::ContextProducerEvidence::default(), stages: stages::StageEvidence::default(), error_constructors: error_constructors::ErrorConstructorEvidence::default(), tag_constructors: tag_constructors::TagConstructorEvidence::default(), bridges: bridges::BridgeEvidence::default(), mutable_paths: mutable_paths::MutablePathEvidence::default(), host_bindings: host_bindings::HostBindingEvidence::default(), lexical_captures: lexical_captures::LexicalCaptureEvidence::default(), constants: constants::ConstantEvidence::default(), record_updates: record_updates::RecordUpdateEvidence::default(), conditionals: conditionals::ConditionalEvidence::default(), paths: paths::PathEvidence::default(), scoped_invocation_sources: Vec::new(), original_scoped_invocation_sources: Vec::new(), scoped_invocation_witnesses: Vec::new(), scoped_invocation_instructions: Vec::new(), original_argument_bindings: Vec::new(), argument_binding_receipts: Vec::new(), original_argument_wrappers: Vec::new(), pattern_nominals: Vec::new(), original_callable_bindings: Vec::new(), original_callable_uses: Vec::new(), native_call_sources: Vec::new(), original_native_call_sources: Vec::new(), ground_native_calls: Vec::new(), ground_native_call_instructions: Vec::new(), callable_sources: Vec::new(), callable_values: Vec::new(), invocation_sources: Vec::new(), invocation_plans: Vec::new(), callable_instructions: Vec::new(), invocation_instructions: Vec::new(), ground_projection_sources: Vec::new(), original_ground_projection_sources: Vec::new(), ground_projections: Vec::new(), ground_projection_instructions: Vec::new(), pattern_sources: Vec::new(), original_pattern_sources: Vec::new(), pattern_applications: Vec::new(), original_pattern_applications: Vec::new(), pattern_conditional_results: Vec::new(), pattern_captures: Vec::new(), pattern_uses: Vec::new(), original_pattern_uses: Vec::new(), root, instruction_origins: Vec::new(), pattern_origins: Vec::new(), scopes: Vec::new(), layouts: Vec::new(), instances: Vec::new(), forwarding: Vec::new(), templates: Vec::new(), calls: Vec::new(), arguments: Vec::new(), uses: Vec::new(), constructors: Vec::new(), function_scopes: Vec::new(), operation_sources: Vec::new(), original_operation_sources: Vec::new(), operations: Vec::new(), original_operations: Vec::new(), operation_instructions: Vec::new(), original_operation_instructions: Vec::new(), checked_functions: Vec::new(), checked_function_index: Vec::new(), stage_call_sources: Vec::new(), ground_stage_calls: Vec::new(), ground_stage_call_instructions: Vec::new() }, next_serial: 1, canonical_templates: rustc_hash::FxHashMap::default(), canonical_instances: rustc_hash::FxHashMap::default() }
    }
}

macro_rules! insert_evidence {
    ($visibility:vis $method:ident, $field:ident, $ty:ty, $id:ident) => {
        $visibility fn $method(&mut self, value: $ty) -> Result<$id, IrVerifyError> {
            let index = u32::try_from(self.store.$field.len()).map_err(|_| failure("generic evidence id overflow"))?;
            let serial = self.next_serial;
            self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
            self.store.$field.push(Entry { serial, value });
            Ok($id { index, proof: OwnerProof { root: self.store.root, serial } })
        }
    };
}

impl GenericEvidenceBuilder {
    pub fn add_original_argument_binding(&mut self, value: OriginalArgumentBinding) -> Result<(), IrVerifyError> {
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.argument_binding_receipts.push(Arc::clone(&value));
        self.store.original_argument_bindings.push(Entry { serial, value });
        Ok(())
    }
    pub fn register_pattern_nominal(&mut self, member: super::pattern::PreparedPatternNominalMember) -> Result<(), IrVerifyError> {
        if let Some(&index) = self.canonical_pattern_nominals.get(&member.identity) {
            return if self.store.pattern_nominals[index].value == member { Ok(()) } else { Err(failure("original pattern nominal member conflicts with its registration")) };
        }
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        self.canonical_pattern_nominals.insert(member.identity, self.store.pattern_nominals.len());
        self.store.pattern_nominals.push(Entry { serial, value: member });
        Ok(())
    }
    pub fn pattern_nominal(&self, identity: crate::sema::check::QualifiedNominalIdentity) -> Result<&super::pattern::PreparedPatternNominalMember, IrVerifyError> {
        self.canonical_pattern_nominals.get(&identity).map(|&index| &self.store.pattern_nominals[index].value).ok_or_else(|| failure("original pattern nominal member is missing"))
    }
    pub fn add_original_callable_binding(&mut self, value: OriginalCallableBinding) -> Result<(), IrVerifyError> {
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        self.store.original_callable_bindings.push(Entry { serial, value });
        Ok(())
    }
    pub fn add_original_callable_use(&mut self, value: OriginalCallableUse) -> Result<(), IrVerifyError> {
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        self.store.original_callable_uses.push(Entry { serial, value });
        Ok(())
    }
    pub fn add_native_call_source(&mut self, value: NativeCallSource) -> Result<NativeCallSourceId, IrVerifyError> {
        let index = u32::try_from(self.store.native_call_sources.len()).map_err(|_| failure("generic evidence id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.original_native_call_sources.push(Arc::clone(&value));
        self.store.native_call_sources.push(Entry { serial, value });
        Ok(NativeCallSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    insert_evidence!(pub add_ground_native_call, ground_native_calls, PreparedGroundNativeCall, GroundNativeCallId);
    pub fn pattern_source(&self, id: PatternSourceId) -> Result<&super::pattern::PreparedPatternSource, IrVerifyError> { self.store.pattern_source(id) }
    pub fn add_pattern_source(&mut self, value: super::pattern::PreparedPatternSource) -> Result<PatternSourceId, IrVerifyError> {
        let index = u32::try_from(self.store.pattern_sources.len()).map_err(|_| failure("generic evidence id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.original_pattern_sources.push(Arc::clone(&value));
        self.store.pattern_sources.push(Entry { serial, value });
        Ok(PatternSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_pattern_application(&mut self, value: super::pattern::PreparedPatternApplication) -> Result<PatternApplicationId, IrVerifyError> {
        let index = u32::try_from(self.store.pattern_applications.len()).map_err(|_| failure("generic evidence id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.original_pattern_applications.push(Arc::clone(&value));
        self.store.pattern_applications.push(Entry { serial, value });
        Ok(PatternApplicationId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    insert_evidence!(pub add_pattern_capture, pattern_captures, super::pattern::PreparedPatternCapture, PatternCaptureId);
    pub fn add_pattern_use(&mut self, use_: super::pattern::PreparedPatternUse) { self.store.original_pattern_uses.push(use_.clone()); self.store.pattern_uses.push(use_); }
    pub fn add_ground_projection_source(&mut self, value: GroundProjectionSource) -> Result<GroundProjectionSourceId, IrVerifyError> {
        let index = u32::try_from(self.store.ground_projection_sources.len()).map_err(|_| failure("generic evidence id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.original_ground_projection_sources.push(Arc::clone(&value));
        self.store.ground_projection_sources.push(Entry { serial, value });
        Ok(GroundProjectionSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    insert_evidence!(pub add_ground_projection, ground_projections, PreparedGroundProjection, GroundProjectionId);
    insert_evidence!(pub add_callable_source, callable_sources, CallableValueSource, CallableValueSourceId);
    insert_evidence!(pub add_callable_value, callable_values, PreparedCallableValue, CallableValueId);
    insert_evidence!(pub add_invocation_source, invocation_sources, InvocationSource, InvocationSourceId);
    insert_evidence!(pub add_invocation_plan, invocation_plans, PreparedInvocationPlan, InvocationPlanId);
    pub fn register_instruction_origin(&mut self, instruction: u32, origin: OperationSourceOrigin, owner: InstructionOwner) -> Result<(), IrVerifyError> {
        if !matches!(origin, OperationSourceOrigin::Expression(_) | OperationSourceOrigin::Stage(_) | OperationSourceOrigin::Statement(_)) { return Err(failure("instruction origin kind is unsupported")); }
        self.store.instruction_origins.push((instruction, origin, owner));
        Ok(())
    }
    insert_evidence!(pub add_checked_function, checked_functions, CheckedFunctionSource, CheckedFunctionId);
    insert_evidence!(pub add_stage_call_source, stage_call_sources, StageCallSource, StageCallSourceId);
    insert_evidence!(pub add_ground_stage_call, ground_stage_calls, PreparedGroundStageCall, GroundStageCallId);
    pub fn add_operation_source(&mut self, value: OperationSource) -> Result<OperationSourceId, IrVerifyError> {
        let index = u32::try_from(self.store.operation_sources.len()).map_err(|_| failure("generic evidence id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.original_operation_sources.push(Arc::clone(&value));
        self.store.operation_sources.push(Entry { serial, value });
        Ok(OperationSourceId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    pub fn add_operation(&mut self, value: PreparedOperation) -> Result<OperationId, IrVerifyError> {
        let index = u32::try_from(self.store.operations.len()).map_err(|_| failure("generic evidence id overflow"))?;
        let serial = self.next_serial;
        self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
        let value = Arc::new(value);
        self.store.original_operations.push(Arc::clone(&value));
        self.store.operations.push(Entry { serial, value });
        Ok(OperationId { index, proof: OwnerProof { root: self.store.root, serial } })
    }
    insert_evidence!(pub add_scope, scopes, SchemeScope, SchemeScopeId);
    insert_evidence!(pub add_layout, layouts, PhysicalLayout, PhysicalLayoutId);
    insert_evidence!(add_instance_uncached, instances, Instantiation, InstantiationId);
    insert_evidence!(pub add_forwarding, forwarding, ForwardingPlan, ForwardingId);
    insert_evidence!(add_template_uncached, templates, TypeTemplate, TypeTemplateId);
    pub fn add_instance(&mut self, instance: Instantiation) -> Result<InstantiationId, IrVerifyError> {
        if let Some(&id) = self.canonical_instances.get(&instance) { return Ok(id); }
        let id = self.add_instance_uncached(instance.clone())?;
        self.canonical_instances.insert(instance, id);
        Ok(id)
    }
    pub fn add_template(&mut self, mut template: TypeTemplate) -> Result<TypeTemplateId, IrVerifyError> {
        if let TypeTemplate::Record { fields, .. } = &mut template { fields.sort_unstable_by_key(|field| field.0); }
        if let Some(&id) = self.canonical_templates.get(&template) { return Ok(id); }
        let id = self.add_template_uncached(template.clone())?;
        self.canonical_templates.insert(template, id);
        Ok(id)
    }
    pub fn add_call(&mut self, call: SolvedCall) { self.store.calls.push(call); }
    pub fn add_argument(&mut self, argument: SolvedArgument) { self.store.arguments.push(argument); }
    pub fn add_requirement_use(&mut self, use_: SolvedRequirementUse) { self.store.uses.push(use_); }
    pub fn constructors(&self) -> &[SolvedRecordLayout] { &self.store.constructors }
    pub fn register_pattern_origin(&mut self, pattern: u32, origin: crate::sema::check::PatternIdentity, owner: InstructionOwner) { self.store.pattern_origins.push((pattern, origin, owner)); }
    pub fn add_constructor(&mut self, record: SolvedRecordLayout) { self.store.constructors.push(record); }
    pub fn checkpoint(&self) -> GenericCheckpoint { GenericCheckpoint { module_invocations: self.store.module_invocations.checkpoint(), eligibility: self.store.eligibility.checkpoint(), callable_receivers: self.store.callable_receivers.checkpoint(), native_callables: self.store.native_callables.checkpoint(), values: self.store.values.checkpoint(), operation_requirements: self.store.operation_requirements.checkpoint(), iterations: self.store.iterations.checkpoint(), records: self.store.records.checkpoint(), comprehensions: self.store.comprehensions.checkpoint(), compiler_wrappers: self.store.compiler_wrappers.checkpoint(), indices: self.store.indices.checkpoint(), containers: self.store.containers.checkpoint(), record_constructors: self.store.record_constructors.checkpoint(), mutables: self.store.mutables.checkpoint(), native_scalars: self.store.native_scalars.checkpoint(), try_captures: self.store.try_captures.checkpoint(), context_producers: self.store.context_producers.checkpoint(), stages: self.store.stages.checkpoint(), error_constructors: self.store.error_constructors.checkpoint(), tag_constructors: self.store.tag_constructors.checkpoint(), bridges: self.store.bridges.checkpoint(), mutable_paths: self.store.mutable_paths.checkpoint(), host_bindings: self.store.host_bindings.checkpoint(), lexical_captures: self.store.lexical_captures.checkpoint(), constants: self.store.constants.checkpoint(), record_updates: self.store.record_updates.checkpoint(), conditionals: self.store.conditionals.checkpoint(), paths: self.store.paths.checkpoint(), scoped_invocation_sources: self.store.scoped_invocation_sources.len(), scoped_invocation_witnesses: self.store.scoped_invocation_witnesses.len(), original_argument_bindings: self.store.original_argument_bindings.len(), pattern_nominals: self.store.pattern_nominals.len(), original_callable_bindings: self.store.original_callable_bindings.len(), original_callable_uses: self.store.original_callable_uses.len(), native_call_sources: self.store.native_call_sources.len(), ground_native_calls: self.store.ground_native_calls.len(), pattern_origins: self.store.pattern_origins.len(), pattern_sources: self.store.pattern_sources.len(), pattern_applications: self.store.pattern_applications.len(), pattern_captures: self.store.pattern_captures.len(), pattern_uses: self.store.pattern_uses.len(), ground_projection_sources: self.store.ground_projection_sources.len(), ground_projections: self.store.ground_projections.len(), callable_sources: self.store.callable_sources.len(), callable_values: self.store.callable_values.len(), invocation_sources: self.store.invocation_sources.len(), invocation_plans: self.store.invocation_plans.len(), root: self.store.root, serial_limit: self.next_serial, scopes: self.store.scopes.len(), layouts: self.store.layouts.len(), instances: self.store.instances.len(), forwarding: self.store.forwarding.len(), templates: self.store.templates.len(), calls: self.store.calls.len(), arguments: self.store.arguments.len(), uses: self.store.uses.len(), constructors: self.store.constructors.len(), operation_sources: self.store.operation_sources.len(), operations: self.store.operations.len(), checked_functions: self.store.checked_functions.len(), stage_call_sources: self.store.stage_call_sources.len(), ground_stage_calls: self.store.ground_stage_calls.len(), instruction_origins: self.store.instruction_origins.len() } }
    pub fn rewind(&mut self, checkpoint: GenericCheckpoint) -> Result<(), IrVerifyError> {
        if checkpoint.root != self.store.root { return Err(failure("generic checkpoint belongs to a foreign program")); }
        if checkpoint.scoped_invocation_sources > self.store.scoped_invocation_sources.len() || checkpoint.scoped_invocation_witnesses > self.store.scoped_invocation_witnesses.len() || checkpoint.original_argument_bindings > self.store.original_argument_bindings.len() || checkpoint.pattern_nominals > self.store.pattern_nominals.len() || checkpoint.original_callable_bindings > self.store.original_callable_bindings.len() || checkpoint.original_callable_uses > self.store.original_callable_uses.len() || checkpoint.native_call_sources > self.store.native_call_sources.len() || checkpoint.ground_native_calls > self.store.ground_native_calls.len() || checkpoint.pattern_origins > self.store.pattern_origins.len() || checkpoint.pattern_sources > self.store.pattern_sources.len() || checkpoint.pattern_applications > self.store.pattern_applications.len() || checkpoint.pattern_captures > self.store.pattern_captures.len() || checkpoint.pattern_uses > self.store.pattern_uses.len() || checkpoint.ground_projection_sources > self.store.ground_projection_sources.len() || checkpoint.ground_projections > self.store.ground_projections.len() || checkpoint.callable_sources > self.store.callable_sources.len() || checkpoint.callable_values > self.store.callable_values.len() || checkpoint.invocation_sources > self.store.invocation_sources.len() || checkpoint.invocation_plans > self.store.invocation_plans.len() || checkpoint.instruction_origins > self.store.instruction_origins.len() || checkpoint.checked_functions > self.store.checked_functions.len() || checkpoint.stage_call_sources > self.store.stage_call_sources.len() || checkpoint.ground_stage_calls > self.store.ground_stage_calls.len() || checkpoint.operation_sources > self.store.operation_sources.len() || checkpoint.operations > self.store.operations.len() || checkpoint.scopes > self.store.scopes.len() || checkpoint.layouts > self.store.layouts.len() || checkpoint.instances > self.store.instances.len() || checkpoint.forwarding > self.store.forwarding.len() || checkpoint.templates > self.store.templates.len() || checkpoint.calls > self.store.calls.len() || checkpoint.arguments > self.store.arguments.len() || checkpoint.uses > self.store.uses.len() || checkpoint.constructors > self.store.constructors.len() { return Err(failure("generic checkpoint references retired entries")); }
        for serial in [
            self.store.scoped_invocation_sources.get(checkpoint.scoped_invocation_sources.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.scoped_invocation_witnesses.get(checkpoint.scoped_invocation_witnesses.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.original_argument_bindings.get(checkpoint.original_argument_bindings.wrapping_sub(1)).map(|entry| entry.serial), self.store.pattern_nominals.get(checkpoint.pattern_nominals.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.original_callable_bindings.get(checkpoint.original_callable_bindings.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.original_callable_uses.get(checkpoint.original_callable_uses.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.native_call_sources.get(checkpoint.native_call_sources.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.ground_native_calls.get(checkpoint.ground_native_calls.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.callable_sources.get(checkpoint.callable_sources.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.callable_values.get(checkpoint.callable_values.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.invocation_sources.get(checkpoint.invocation_sources.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.invocation_plans.get(checkpoint.invocation_plans.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.ground_projection_sources.get(checkpoint.ground_projection_sources.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.ground_projections.get(checkpoint.ground_projections.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.pattern_sources.get(checkpoint.pattern_sources.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.pattern_applications.get(checkpoint.pattern_applications.wrapping_sub(1)).map(|entry| entry.serial),
            self.store.pattern_captures.get(checkpoint.pattern_captures.wrapping_sub(1)).map(|entry| entry.serial),
        ] { if serial.is_some_and(|serial| serial >= checkpoint.serial_limit) { return Err(failure("generic checkpoint references replacement entries")); } }
        for serial in [self.store.checked_functions.get(checkpoint.checked_functions.wrapping_sub(1)).map(|entry| entry.serial), self.store.stage_call_sources.get(checkpoint.stage_call_sources.wrapping_sub(1)).map(|entry| entry.serial), self.store.ground_stage_calls.get(checkpoint.ground_stage_calls.wrapping_sub(1)).map(|entry| entry.serial)] {
            if serial.is_some_and(|serial| serial >= checkpoint.serial_limit) { return Err(failure("generic checkpoint references replacement entries")); }
        }
        for serial in [self.store.operation_sources.get(checkpoint.operation_sources.wrapping_sub(1)).map(|entry| entry.serial), self.store.operations.get(checkpoint.operations.wrapping_sub(1)).map(|entry| entry.serial), self.store.scopes.get(checkpoint.scopes.wrapping_sub(1)).map(|entry| entry.serial), self.store.layouts.get(checkpoint.layouts.wrapping_sub(1)).map(|entry| entry.serial), self.store.instances.get(checkpoint.instances.wrapping_sub(1)).map(|entry| entry.serial), self.store.forwarding.get(checkpoint.forwarding.wrapping_sub(1)).map(|entry| entry.serial), self.store.templates.get(checkpoint.templates.wrapping_sub(1)).map(|entry| entry.serial)].into_iter().flatten() {
            if serial >= checkpoint.serial_limit { return Err(failure("generic checkpoint references replacement entries")); }
        }
        self.store.eligibility.validate_checkpoint(checkpoint.eligibility, checkpoint.serial_limit)?;
        self.store.iterations.validate_checkpoint(checkpoint.iterations, checkpoint.serial_limit)?;
        self.store.records.validate_checkpoint(checkpoint.records, checkpoint.serial_limit)?;
        self.store.comprehensions.validate_checkpoint(checkpoint.comprehensions, checkpoint.serial_limit)?;
        self.store.compiler_wrappers.validate_checkpoint(checkpoint.compiler_wrappers, checkpoint.serial_limit)?;
        self.store.indices.validate_checkpoint(checkpoint.indices, checkpoint.serial_limit)?;
        self.store.containers.validate_checkpoint(checkpoint.containers, checkpoint.serial_limit)?;
        self.store.record_constructors.validate_checkpoint(checkpoint.record_constructors, checkpoint.serial_limit)?;
        self.store.mutables.validate_checkpoint(checkpoint.mutables, checkpoint.serial_limit)?;
        self.store.native_scalars.validate_checkpoint(checkpoint.native_scalars, checkpoint.serial_limit)?;
        self.store.try_captures.validate_checkpoint(checkpoint.try_captures, checkpoint.serial_limit)?;
        self.store.context_producers.validate_checkpoint(checkpoint.context_producers, checkpoint.serial_limit)?;
        self.store.stages.validate_checkpoint(checkpoint.stages, checkpoint.serial_limit)?;
        self.store.error_constructors.validate_checkpoint(checkpoint.error_constructors, checkpoint.serial_limit)?;
        self.store.tag_constructors.validate_checkpoint(checkpoint.tag_constructors, checkpoint.serial_limit)?;
        self.store.bridges.validate_checkpoint(checkpoint.bridges, checkpoint.serial_limit)?;
        self.store.mutable_paths.validate_checkpoint(checkpoint.mutable_paths, checkpoint.serial_limit)?;
        self.store.host_bindings.validate_checkpoint(checkpoint.host_bindings, checkpoint.serial_limit)?;
        self.store.lexical_captures.validate_checkpoint(checkpoint.lexical_captures, checkpoint.serial_limit)?;
        self.store.constants.validate_checkpoint(checkpoint.constants, checkpoint.serial_limit)?;
        self.store.record_updates.validate_checkpoint(checkpoint.record_updates, checkpoint.serial_limit)?;
        self.store.conditionals.validate_checkpoint(checkpoint.conditionals, checkpoint.serial_limit)?;
        self.store.paths.validate_checkpoint(checkpoint.paths, checkpoint.serial_limit)?;
        self.store.operation_requirements.validate_checkpoint(checkpoint.operation_requirements, checkpoint.serial_limit)?;
        self.store.values.validate_checkpoint(checkpoint.values, checkpoint.serial_limit)?;
        self.store.module_invocations.validate_checkpoint(checkpoint.module_invocations, checkpoint.serial_limit)?;
        self.store.callable_receivers.validate_checkpoint(checkpoint.callable_receivers, checkpoint.serial_limit)?;
        self.store.native_callables.validate_checkpoint(checkpoint.native_callables, checkpoint.serial_limit)?;
        // Every collection must accept the checkpoint before any source receipt,
        // derived index, or serial-bearing entry can be retired.
        self.store.eligibility.rewind_validated(checkpoint.eligibility);
        self.store.iterations.rewind_validated(checkpoint.iterations);
        self.store.records.rewind_validated(checkpoint.records);
        self.store.comprehensions.rewind_validated(checkpoint.comprehensions);
        self.store.compiler_wrappers.rewind_validated(checkpoint.compiler_wrappers);
        self.store.indices.rewind_validated(checkpoint.indices);
        self.store.containers.rewind_validated(checkpoint.containers);
        self.store.record_constructors.rewind_validated(checkpoint.record_constructors);
        self.store.mutables.rewind_validated(checkpoint.mutables);
        self.store.native_scalars.rewind_validated(checkpoint.native_scalars);
        self.store.try_captures.rewind_validated(checkpoint.try_captures);
        self.store.context_producers.rewind_validated(checkpoint.context_producers);
        self.store.stages.rewind_validated(checkpoint.stages);
        self.store.error_constructors.rewind_validated(checkpoint.error_constructors);
        self.store.tag_constructors.rewind_validated(checkpoint.tag_constructors);
        self.store.bridges.rewind_validated(checkpoint.bridges);
        self.store.mutable_paths.rewind_validated(checkpoint.mutable_paths);
        self.store.host_bindings.rewind_validated(checkpoint.host_bindings);
        self.store.lexical_captures.rewind_validated(checkpoint.lexical_captures);
        self.store.constants.rewind_validated(checkpoint.constants);
        self.store.record_updates.rewind_validated(checkpoint.record_updates);
        self.store.conditionals.rewind_validated(checkpoint.conditionals);
        self.store.paths.rewind_validated(checkpoint.paths);
        self.store.operation_requirements.rewind_validated(checkpoint.operation_requirements);
        self.store.values.rewind_validated(checkpoint.values);
        self.store.module_invocations.rewind_validated(checkpoint.module_invocations);
        self.store.callable_receivers.rewind_validated(checkpoint.callable_receivers);
        self.store.native_callables.rewind_validated(checkpoint.native_callables);
        self.store.scoped_invocation_sources.truncate(checkpoint.scoped_invocation_sources); self.store.original_scoped_invocation_sources.truncate(checkpoint.scoped_invocation_sources); self.store.scoped_invocation_witnesses.truncate(checkpoint.scoped_invocation_witnesses); self.store.scoped_invocation_instructions.clear();
        self.store.original_argument_bindings.truncate(checkpoint.original_argument_bindings); self.store.argument_binding_receipts.truncate(checkpoint.original_argument_bindings); self.store.original_argument_wrappers.clear(); self.store.pattern_nominals.truncate(checkpoint.pattern_nominals); self.canonical_pattern_nominals.retain(|_, index| *index < checkpoint.pattern_nominals); self.store.original_callable_bindings.truncate(checkpoint.original_callable_bindings); self.store.original_callable_uses.truncate(checkpoint.original_callable_uses); self.store.native_call_sources.truncate(checkpoint.native_call_sources); self.store.original_native_call_sources.truncate(checkpoint.native_call_sources); self.store.ground_native_calls.truncate(checkpoint.ground_native_calls); self.store.ground_native_call_instructions.clear(); self.store.operation_sources.truncate(checkpoint.operation_sources); self.store.original_operation_sources.truncate(checkpoint.operation_sources); self.store.operations.truncate(checkpoint.operations); self.store.original_operations.truncate(checkpoint.operations); self.store.operation_instructions.clear(); self.store.original_operation_instructions.clear(); self.store.scopes.truncate(checkpoint.scopes); self.store.layouts.truncate(checkpoint.layouts); self.store.instances.truncate(checkpoint.instances); self.store.forwarding.truncate(checkpoint.forwarding); self.store.templates.truncate(checkpoint.templates); self.store.calls.truncate(checkpoint.calls); self.store.arguments.truncate(checkpoint.arguments); self.store.uses.truncate(checkpoint.uses); self.store.constructors.truncate(checkpoint.constructors);
        self.store.pattern_origins.truncate(checkpoint.pattern_origins); self.store.pattern_sources.truncate(checkpoint.pattern_sources); self.store.original_pattern_sources.truncate(checkpoint.pattern_sources); self.store.pattern_applications.truncate(checkpoint.pattern_applications); self.store.original_pattern_applications.truncate(checkpoint.pattern_applications); self.store.pattern_conditional_results.clear(); self.store.pattern_captures.truncate(checkpoint.pattern_captures); self.store.pattern_uses.truncate(checkpoint.pattern_uses); self.store.original_pattern_uses.truncate(checkpoint.pattern_uses); self.store.ground_projection_sources.truncate(checkpoint.ground_projection_sources); self.store.original_ground_projection_sources.truncate(checkpoint.ground_projection_sources); self.store.ground_projections.truncate(checkpoint.ground_projections); self.store.ground_projection_instructions.clear(); self.store.callable_sources.truncate(checkpoint.callable_sources); self.store.callable_values.truncate(checkpoint.callable_values); self.store.invocation_sources.truncate(checkpoint.invocation_sources); self.store.invocation_plans.truncate(checkpoint.invocation_plans); self.store.callable_instructions.clear(); self.store.invocation_instructions.clear(); self.store.instruction_origins.truncate(checkpoint.instruction_origins); self.store.checked_functions.truncate(checkpoint.checked_functions); self.store.checked_function_index.clear(); self.store.stage_call_sources.truncate(checkpoint.stage_call_sources); self.store.ground_stage_calls.truncate(checkpoint.ground_stage_calls); self.store.ground_stage_call_instructions.clear();
        self.canonical_templates.retain(|_, id| id.index < checkpoint.templates as u32);
        self.canonical_instances.retain(|_, id| id.index < checkpoint.instances as u32);
        Ok(())
    }
    pub(super) fn finish(mut self, pools: &SemanticPools, function_count: usize, instruction_owners: &[Option<InstructionOwner>]) -> Result<GenericEvidenceStore, IrVerifyError> {
        self.store.iterations.finish();
        self.store.records.finish(self.store.root);
        self.store.comprehensions.finish();
        self.store.compiler_wrappers.finish_indexes();
        self.store.indices.finish_indexes();
        self.store.containers.finish();
        self.store.record_constructors.finish(self.store.root);
        self.store.mutables.finish();
        self.store.native_scalars.finish();
        self.store.try_captures.finish(self.store.root);
        self.store.context_producers.finish();
        self.store.stages.finish_indexes();
        self.store.error_constructors.finish_indexes();
        self.store.tag_constructors.finish_indexes();
        self.store.bridges.finish(self.store.root);
        self.store.mutable_paths.finish();
        self.store.host_bindings.finish();
        self.store.lexical_captures.finish();
        self.store.constants.finish(self.store.root);
        self.store.record_updates.finish(self.store.root);
        self.store.conditionals.finish();
        self.store.paths.finish();
        self.store.operation_requirements.finish(self.store.root);
        self.store.values.finish();
        self.store.module_invocations.finish_indexes();
        self.store.callable_receivers.finish_indexes();
        self.store.native_callables.finish_indexes(self.store.root);
        self.store.scoped_invocation_instructions = self.store.scoped_invocation_sources().map(|(id, source)| (source.instruction, id)).collect();
        self.store.scoped_invocation_instructions.sort_unstable_by_key(|entry| entry.0);
        self.store.original_argument_bindings.sort_unstable_by_key(|entry| entry.value.instruction);
        self.store.argument_binding_receipts.sort_unstable_by_key(|saved| saved.instruction);
        self.store.original_argument_wrappers = self.store.original_argument_bindings().map(|saved| (saved.wrapper, saved.instruction)).collect();
        self.store.original_argument_wrappers.sort_unstable();
        self.store.pattern_nominals.sort_unstable_by_key(|entry| entry.value.identity);
        self.store.original_callable_bindings.sort_unstable_by_key(|entry| entry.value.binding);
        self.store.original_callable_uses.sort_unstable_by_key(|entry| entry.value.instruction);
        self.store.ground_native_call_instructions = self.store.ground_native_calls().map(|(id, proof)| self.store.native_call_source(proof.source).map(|source| (source.instruction, id))).collect::<Result<Vec<_>, _>>()?;
        self.store.ground_native_call_instructions.sort_unstable_by_key(|entry| entry.0);
        self.store.pattern_origins.sort_unstable_by_key(|entry| entry.0);
        self.store.pattern_origins.dedup();
        self.store.pattern_uses.sort_unstable_by_key(|use_| use_.instruction);
        self.store.original_pattern_uses.sort_unstable_by_key(|use_| use_.instruction);
        self.store.pattern_conditional_results = self.store.pattern_applications().filter_map(|(id, application)| application.result.as_ref().map(|result| (result.control, id))).collect();
        self.store.pattern_conditional_results.sort_unstable_by_key(|entry| entry.0);
        self.store.ground_projection_instructions = self.store.ground_projections().map(|(id, proof)| self.store.ground_projection_source(proof.source).map(|source| (source.instruction, id))).collect::<Result<Vec<_>, _>>()?;
        self.store.ground_projection_instructions.sort_unstable_by_key(|entry| entry.0);
        self.store.callable_instructions = self.store.callable_values().map(|(id, value)| self.store.callable_source(value.source).map(|source| (source.instruction, id))).collect::<Result<Vec<_>, _>>()?;
        self.store.callable_instructions.sort_unstable_by_key(|entry| entry.0);
        self.store.invocation_instructions = self.store.invocation_plans().map(|(id, value)| self.store.invocation_source(value.source).map(|source| (source.instruction, id))).collect::<Result<Vec<_>, _>>()?;
        self.store.invocation_instructions.sort_unstable_by_key(|entry| entry.0);
        self.store.instruction_origins.sort_unstable_by_key(|entry| (entry.0, matches!(entry.1, OperationSourceOrigin::Stage(_))));
        self.store.instruction_origins.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2 == b.2);
        self.store.checked_function_index = self.store.checked_functions().map(|(id, function)| (function.declaration, id)).collect();
        self.store.checked_function_index.sort_unstable_by_key(|entry| entry.0);
        self.store.ground_stage_call_instructions = self.store.ground_stage_calls().map(|(id, call)| self.store.stage_call_source(call.source).map(|source| (source.instruction, id))).collect::<Result<Vec<_>, _>>()?;
        self.store.ground_stage_call_instructions.sort_unstable_by_key(|entry| entry.0);
        self.store.operation_instructions = self.store.operations().map(|(id, operation)| self.store.operation_source(operation.source).map(|source| (source.instruction, id))).collect::<Result<Vec<_>, _>>()?;
        self.store.operation_instructions.sort_unstable_by_key(|entry| entry.0);
        self.store.original_operation_instructions = self.store.operation_instructions.clone();
        self.store.arguments.sort_unstable_by_key(|argument| (argument.call_instruction, argument.parameter));
        self.store.calls.sort_by_key(|call| call.instruction); self.store.uses.sort_by_key(|use_| use_.instruction); self.store.constructors.sort_by_key(|record| record.instruction);
        self.store.function_scopes = self.store.scopes().map(|(id, scope)| (scope.owner, id)).collect();
        self.store.function_scopes.sort_by_key(|entry| entry.0.raw());
        for entry in &mut self.store.forwarding { entry.value.instances.sort_unstable_by_key(|edge| edge.0.index); }
        self.store.verify(pools, function_count, instruction_owners)?;
        Ok(self.store)
    }
}

pub(in crate::runtime::eval) fn graph_ground_type(graph: &crate::sema::inference::InferenceContext, id: crate::sema::inference::TypeId) -> Result<crate::sema::types::Type, IrVerifyError> {
    fn ground(graph: &crate::sema::inference::InferenceContext, id: crate::sema::inference::TypeId, active: &mut Vec<crate::sema::inference::TypeId>) -> Result<crate::sema::types::Type, IrVerifyError> {
        use crate::sema::inference::{Atom, TypeNode};
        use crate::sema::types::Type;
        let id = graph.resolved(id).map_err(|_| failure("graph type handle is foreign or unresolved"))?;
        if active.len() >= 256 || active.contains(&id) { return Err(failure("graph ground type is cyclic or exceeds depth limit")); }
        active.push(id);
        let ty = match graph.node(id).map_err(|_| failure("graph type handle is foreign"))? {
            TypeNode::Atom(atom) => match *atom {
                Atom::Any => Type::Any, Atom::ErasedRecord => Type::ErasedRecord, Atom::DynamicModule => Type::DynamicModule, Atom::Pure => Type::Pure, Atom::Proc => Type::Proc, Atom::Command => Type::Command,
                Atom::Null => Type::Null, Atom::Bool => Type::Bool, Atom::Int => Type::Int, Atom::UInt => Type::UInt, Atom::Float => Type::Float, Atom::Duration => Type::Duration, Atom::Str => Type::Str, Atom::Bytes => Type::Bytes, Atom::Digest => Type::Digest, Atom::Regex => Type::Regex, Atom::Path => Type::Path,
                Atom::Unit => Type::Unit, Atom::Status => Type::Status, Atom::EnvPathList => Type::EnvPathList, Atom::Error => Type::Error, Atom::ProcessError => Type::ProcessError, Atom::ProcessHandle => Type::ProcessHandle, Atom::NetJob => Type::NetJob, Atom::FsRoot => Type::FsRoot,
                Atom::Tag(name) => Type::Tag(name), Atom::ErrorFamily(name) => Type::ErrorFamily(name), Atom::ErrorVariant { family, variant } => Type::ErrorVariant { family, variant }, Atom::ErrorFacet(name) => Type::ErrorFacet(name),
            },
            TypeNode::Optional(inner) => Type::Optional(Box::new(ground(graph, *inner, active)?)),
            TypeNode::List(inner) => Type::List(Box::new(ground(graph, *inner, active)?)),
            TypeNode::Stream(inner) => Type::Stream(Box::new(ground(graph, *inner, active)?)),
            TypeNode::Map(key, value) => Type::Map(Box::new(ground(graph, *key, active)?), Box::new(ground(graph, *value, active)?)),
            TypeNode::Result(ok, error) => Type::Result(Box::new(ground(graph, *ok, active)?), Box::new(ground(graph, *error, active)?)),
            TypeNode::Record(row) | TypeNode::Row(row) => {
                let row = graph.row_data(*row).map_err(|_| failure("graph row handle is foreign"))?;
                let mut fields = std::collections::BTreeMap::new();
                for field in &row.fields { if fields.insert(field.label, ground(graph, field.ty, active)?).is_some() { return Err(failure("graph row repeats a field")); } }
                if let Some(tail) = row.tail {
                    let Type::Record(tail) = ground(graph, tail, active)? else { return Err(failure("graph row tail is not a closed row")); };
                    for (name, ty) in tail { if fields.insert(name, ty).is_some() { return Err(failure("graph row tail violates a lacks constraint")); } }
                }
                Type::Record(fields)
            }
            _ => return Err(failure("graph type is not closed ground data")),
        };
        active.pop();
        Ok(ty)
    }
    ground(graph, id, &mut Vec::new())
}

impl GenericEvidenceBuilder {
    pub(super) fn prepare_reference(&mut self, graph: &crate::sema::inference::InferenceContext, scheme: crate::sema::inference::SchemeId, id: crate::sema::inference::TypeId, pools: &mut SemanticPools, semantic: &mut super::semantic::SemanticPoolBuilder) -> Result<TypeRef, IrVerifyError> {
        self.prepare_graph_reference(graph, Some(scheme), id, pools, semantic)
    }
    pub(super) fn prepare_closed_reference(&mut self, graph: &crate::sema::inference::InferenceContext, id: crate::sema::inference::TypeId, pools: &mut SemanticPools, semantic: &mut super::semantic::SemanticPoolBuilder) -> Result<TypeRef, IrVerifyError> {
        self.prepare_graph_reference(graph, None, id, pools, semantic)
    }
    fn prepare_graph_reference(&mut self, graph: &crate::sema::inference::InferenceContext, scheme: Option<crate::sema::inference::SchemeId>, id: crate::sema::inference::TypeId, pools: &mut SemanticPools, semantic: &mut super::semantic::SemanticPoolBuilder) -> Result<TypeRef, IrVerifyError> {
        fn prepare(builder: &mut GenericEvidenceBuilder, graph: &crate::sema::inference::InferenceContext, scheme: Option<crate::sema::inference::SchemeId>, id: crate::sema::inference::TypeId, pools: &mut SemanticPools, semantic: &mut super::semantic::SemanticPoolBuilder, active: &mut Vec<crate::sema::inference::TypeId>) -> Result<TypeRef, IrVerifyError> {
            use crate::sema::inference::{TypeNode, VariableKind};
            let id = graph.resolved(id).map_err(|_| failure("generic template graph handle is invalid"))?;
            if active.len() >= 256 || active.contains(&id) { return Err(failure("generic template graph is cyclic or exceeds depth limit")); }
            if let Ok(ty) = graph_ground_type(graph, id) { return semantic.intern_type(pools, &ty).map(TypeRef::Ground).map_err(|_| failure("ground graph data cannot enter the semantic pool")); }
            active.push(id);
            let reference = match graph.node(id).map_err(|_| failure("generic template graph handle is foreign"))? {
                TypeNode::Rigid { kind: VariableKind::Type, .. } => {
                    let index = graph.scheme_binder_index(scheme.ok_or_else(|| failure("closed callable contains a symbolic parameter"))?, id).map_err(|_| failure("generic template scheme is foreign"))?
                        .ok_or_else(|| failure("generic template variable is outside the member scheme"))?;
                    TypeRef::Rigid(u32::try_from(index).map_err(|_| failure("generic template quantifier index exceeds its representation"))?)
                },
                TypeNode::Arrow(arrow) => {
                    let kind = match arrow.kind {
                        crate::sema::inference::CallableKind::Pure => CallableKind::Pure,
                        crate::sema::inference::CallableKind::Proc => CallableKind::Proc,
                        crate::sema::inference::CallableKind::Stream => return Err(failure("stream callable template requires its producer protocol")),
                    };
                    let crate::sema::inference::EffectSummary::Closed(effects) = graph.closed_effect_summary(arrow.effects).map_err(|_| failure("callable template effects are foreign"))? else { return Err(failure("callable template effects require an exact closed authority")); };
                    let parameters = arrow.params.iter().map(|parameter| {
                        prepare(builder, graph, scheme, parameter.ty, pools, semantic, active).map(|ty| TemplateParameter {
                            label: parameter.label, ty, mode: ParameterMode::PositionalOrNamed, defaulted: parameter.defaulted, rest: parameter.rest,
                        })
                    }).collect::<Result<Vec<_>, _>>()?;
                    let result = prepare(builder, graph, scheme, arrow.result, pools, semantic, active)?;
                    TypeRef::Template(builder.add_template(TypeTemplate::Arrow { kind, parameters: parameters.into_boxed_slice(), result, effects: callable_types::effect_names(effects) })?)
                }
                TypeNode::NativeCallable(callable) if !callable.alternatives.is_empty()
                    && callable.alternatives.iter().all(|authority| matches!(authority, crate::sema::inference::CallableAuthority::User { .. })) => {
                    prepare(builder, graph, scheme, callable.signature, pools, semantic, active)?
                }
                TypeNode::Optional(inner) => { let inner = prepare(builder, graph, scheme, *inner, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::Optional(inner))?) },
                TypeNode::List(inner) => { let inner = prepare(builder, graph, scheme, *inner, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::List(inner))?) },
                TypeNode::Stream(inner) => { let inner = prepare(builder, graph, scheme, *inner, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::Stream(inner))?) },
                TypeNode::Map(key, value) => { let key = prepare(builder, graph, scheme, *key, pools, semantic, active)?; let value = prepare(builder, graph, scheme, *value, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::Map { key, value })?) },
                TypeNode::Result(ok, error) => { let ok = prepare(builder, graph, scheme, *ok, pools, semantic, active)?; let error = prepare(builder, graph, scheme, *error, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::Result { ok, error })?) },
                TypeNode::Record(row) | TypeNode::Row(row) => {
                    let mut row = *row;
                    let mut fields = std::collections::BTreeMap::new();
                    let mut tails = Vec::new();
                    let row_tail = loop {
                        let data = graph.row_data(row).map_err(|_| failure("generic template row is foreign"))?;
                        for field in &data.fields {
                            let ty = prepare(builder, graph, scheme, field.ty, pools, semantic, active)?;
                            if fields.insert(field.label, ty).is_some() { return Err(failure("generic row extension violates a lacks constraint")); }
                        }
                        let Some(tail) = data.tail else { break None; };
                        let tail = graph.resolved(tail).map_err(|_| failure("generic row tail is unresolved"))?;
                        if active.len() + tails.len() >= 256 || active.contains(&tail) || tails.contains(&tail) { return Err(failure("generic row tail is cyclic or exceeds depth limit")); }
                        match graph.node(tail).map_err(|_| failure("generic row tail is foreign"))? {
                            TypeNode::Rigid { kind: VariableKind::Row, .. } => {
                                let index = graph.scheme_binder_index(scheme.ok_or_else(|| failure("closed callable contains a symbolic row"))?, tail).map_err(|_| failure("generic template scheme is foreign"))?
                                    .ok_or_else(|| failure("generic row variable is outside the member scheme"))?;
                                break Some(u32::try_from(index).map_err(|_| failure("generic row quantifier index exceeds its representation"))?);
                            },
                            TypeNode::Record(next) | TypeNode::Row(next) => { tails.push(tail); row = *next; },
                            _ => return Err(failure("generic row tail is not a scoped row variable or row extension")),
                        }
                    };
                    TypeRef::Template(builder.add_template(TypeTemplate::Record { fields: fields.into_iter().collect::<Vec<_>>().into_boxed_slice(), row_tail })?)
                }
                _ => return Err(failure("generic graph node lacks a prepared scoped representation")),
            };
            active.pop();
            Ok(reference)
        }
        prepare(self, graph, scheme, id, pools, semantic, &mut Vec::new())
    }
    pub(super) fn scope(&self, id: SchemeScopeId) -> Result<&SchemeScope, IrVerifyError> { self.store.scope(id) }
    pub(super) fn operations(&self) -> impl Iterator<Item = (OperationId, &PreparedOperation)> { self.store.operations() }
    pub(super) fn operation_source(&self, id: OperationSourceId) -> Result<&OperationSource, IrVerifyError> { self.store.operation_source(id) }
    pub(super) fn instance(&self, id: InstantiationId) -> Result<&Instantiation, IrVerifyError> { self.store.instance(id) }
    pub(super) fn layout(&self, id: PhysicalLayoutId) -> Result<&PhysicalLayout, IrVerifyError> { self.store.layout(id) }
    pub(super) fn forwarding(&self, id: ForwardingId) -> Result<&ForwardingPlan, IrVerifyError> { self.store.forwarding(id) }
    pub(super) fn template(&self, id: TypeTemplateId) -> Result<&TypeTemplate, IrVerifyError> { self.store.template(id) }
    pub(super) fn scopes(&self) -> impl Iterator<Item = (SchemeScopeId, &SchemeScope)> { self.store.scopes() }
    pub(super) fn instances(&self) -> impl Iterator<Item = (InstantiationId, &Instantiation)> {
        self.store.instances.iter().enumerate().map(|(index, entry)| (InstantiationId { index: index as u32, proof: OwnerProof { root: self.store.root, serial: entry.serial } }, &entry.value))
    }
    pub(super) fn materialize_reference(&self, reference: TypeRef, substitutions: &[GroundTypeId], pools: &mut SemanticPools, semantic: &mut super::semantic::SemanticPoolBuilder) -> Result<GroundTypeId, IrVerifyError> {
        self.store.materialize_type(reference, substitutions, pools, semantic)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum NormalizedType {
    Scalar(crate::sema::types::Type),
    Rigid(u32, QuantifierKind),
    Optional(Box<Self>), List(Box<Self>), Stream(Box<Self>),
    Map(Box<Self>, Box<Self>), Result(Box<Self>, Box<Self>),
    Record(std::collections::BTreeMap<Name, Self>, Option<u32>),
    Arrow(CallableKind, Vec<(Name, ParameterMode, bool, bool, Self)>, Box<Self>, Box<[crate::syntax::node::Effect]>),
}

fn normalized_parameter_accepts(formal: &NormalizedType, actual: &NormalizedType) -> bool {
    match (formal, actual) {
        (NormalizedType::Record(expected, None), NormalizedType::Record(fields, None)) => expected.iter().all(|(name, ty)| fields.get(name) == Some(ty)),
        _ => formal == actual,
    }
}

fn normalize_ground(ty: crate::sema::types::Type) -> NormalizedType {
    use crate::sema::types::Type;
    match ty {
        Type::Optional(inner) => NormalizedType::Optional(Box::new(normalize_ground(*inner))),
        Type::List(inner) => NormalizedType::List(Box::new(normalize_ground(*inner))),
        Type::Stream(inner) => NormalizedType::Stream(Box::new(normalize_ground(*inner))),
        Type::Map(key, value) => NormalizedType::Map(Box::new(normalize_ground(*key)), Box::new(normalize_ground(*value))),
        Type::Result(ok, error) => NormalizedType::Result(Box::new(normalize_ground(*ok)), Box::new(normalize_ground(*error))),
        Type::Record(fields) => NormalizedType::Record(fields.into_iter().map(|(name, ty)| (name, normalize_ground(ty))).collect(), None),
        scalar => NormalizedType::Scalar(scalar),
    }
}

impl GenericEvidenceStore {
    pub(super) fn normalized_reference(&self, pools: &SemanticPools, scope: Option<SchemeScopeId>, reference: TypeRef) -> Result<NormalizedType, IrVerifyError> {
        match scope {
            Some(scope) => self.normalized(pools, self.scope(scope)?, reference, None, &mut Vec::new()),
            None => match reference {
                TypeRef::Ground(ty) => Self::normalized_ground_type(pools, ty),
                _ => Err(failure("symbolic reference has no original scope")),
            },
        }
    }

    fn normalized(&self, pools: &SemanticPools, scope: &SchemeScope, reference: TypeRef, substitutions: Option<&[NormalizedType]>, active: &mut Vec<TypeTemplateId>) -> Result<NormalizedType, IrVerifyError> {
        if active.len() >= 256 { return Err(failure("symbolic template exceeds depth limit")); }
        Ok(match reference {
            TypeRef::Ground(ty) => Self::normalized_ground_type(pools, ty)?,
            TypeRef::Rigid(index) => {
                let kind = *scope.quantifiers.get(index as usize).ok_or_else(|| failure("symbolic rigid is out of scope"))?;
                if let Some(substitutions) = substitutions { substitutions.get(index as usize).cloned().ok_or_else(|| failure("symbolic substitution is out of scope"))? } else { NormalizedType::Rigid(index, kind) }
            }
            TypeRef::Template(id) => {
                if active.contains(&id) { return Err(failure("symbolic template contains a cycle")); }
                active.push(id);
                let result = match self.template(id)? {
                    TypeTemplate::Optional(inner) => NormalizedType::Optional(Box::new(self.normalized(pools, scope, *inner, substitutions, active)?)),
                    TypeTemplate::List(inner) => NormalizedType::List(Box::new(self.normalized(pools, scope, *inner, substitutions, active)?)),
                    TypeTemplate::Stream(inner) => NormalizedType::Stream(Box::new(self.normalized(pools, scope, *inner, substitutions, active)?)),
                    TypeTemplate::Map { key, value } => NormalizedType::Map(Box::new(self.normalized(pools, scope, *key, substitutions, active)?), Box::new(self.normalized(pools, scope, *value, substitutions, active)?)),
                    TypeTemplate::Result { ok, error } => NormalizedType::Result(Box::new(self.normalized(pools, scope, *ok, substitutions, active)?), Box::new(self.normalized(pools, scope, *error, substitutions, active)?)),
                    TypeTemplate::Record { fields, row_tail } => {
                        let mut fields = fields.iter().map(|&(name, ty)| self.normalized(pools, scope, ty, substitutions, active).map(|ty| (name, ty))).collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
                        let mut tail = None;
                        if let Some(index) = row_tail {
                            if scope.quantifiers.get(*index as usize) != Some(&QuantifierKind::Row) { return Err(failure("symbolic record tail has the wrong quantifier kind")); }
                            let row = if let Some(substitutions) = substitutions { substitutions.get(*index as usize).cloned().ok_or_else(|| failure("symbolic row substitution is missing"))? } else { NormalizedType::Rigid(*index, QuantifierKind::Row) };
                            match row {
                                NormalizedType::Rigid(index, QuantifierKind::Row) => tail = Some(index),
                                NormalizedType::Record(row, row_tail) => { for (name, ty) in row { if fields.insert(name, ty).is_some() { return Err(failure("symbolic row substitution violates a lacks constraint")); } } tail = row_tail; }
                                _ => return Err(failure("symbolic row substitution is not a row")),
                            }
                        }
                        NormalizedType::Record(fields, tail)
                    }
                    TypeTemplate::Arrow { kind, parameters, result, effects } => {
                        let parameters = parameters.iter().map(|parameter| self.normalized(pools, scope, parameter.ty, substitutions, active).map(|ty| (parameter.label, parameter.mode, parameter.defaulted, parameter.rest, ty))).collect::<Result<Vec<_>, _>>()?;
                        NormalizedType::Arrow(*kind, parameters, Box::new(self.normalized(pools, scope, *result, substitutions, active)?), callable_types::canonical_effect_names(effects))
                    }
                };
                active.pop();
                result
            }
        })
    }

    fn verify_return_plan(&self, pools: &SemanticPools, scope: &SchemeScope) -> Result<(), IrVerifyError> {
        use crate::sema::types::Type;
        let result = self.normalized(pools, scope, scope.result, None, &mut Vec::new())?;
        let valid = match scope.return_plan {
            GenericReturnPlan::Value => true,
            GenericReturnPlan::Result => matches!(result, NormalizedType::Result(_, _)),
            GenericReturnPlan::Unit => result == NormalizedType::Scalar(Type::Unit),
            GenericReturnPlan::ResultUnit => matches!(result, NormalizedType::Result(ok, _) if *ok == NormalizedType::Scalar(Type::Unit)),
        };
        if valid { Ok(()) } else { Err(failure("generic return plan disagrees with its formal result shape")) }
    }

    fn verify_symbolic_forwarding(&self, pools: &SemanticPools, plan: &ForwardingPlan) -> Result<(), IrVerifyError> {
        let caller = self.scope(plan.caller)?; let callee = self.scope(plan.callee)?;
        let substitutions = plan.substitutions.iter().map(|&reference| self.normalized(pools, caller, reference, None, &mut Vec::new())).collect::<Result<Vec<_>, _>>()?;
        for (requirement, evidence) in callee.requirements.iter().zip(&plan.requirements) {
            let normalize = |reference| self.normalized(pools, callee, reference, Some(&substitutions), &mut Vec::new());
            match evidence {
                ForwardedRequirement::Caller(index) => {
                    let source = caller.requirements.get(*index as usize).ok_or_else(|| failure("symbolic forwarded requirement is out of scope"))?;
                    let caller_type = |reference| self.normalized(pools, caller, reference, None, &mut Vec::new());
                    let valid = match (requirement, source) {
                        (Requirement::TagConstructor(constructor), Requirement::TagConstructor(source)) => constructor.authority == source.authority && constructor.nominal == source.nominal && constructor.result == source.result && constructor.arguments.len() == source.arguments.len() && constructor.references().zip(source.references()).map(|(left, right)| Ok(normalize(left)? == caller_type(right)?)).collect::<Result<Vec<_>, IrVerifyError>>()?.into_iter().all(|valid| valid),
                        (Requirement::NativeMethod(method), Requirement::NativeMethod(source)) => method.candidates == source.candidates && method.parameter_labels == source.parameter_labels && method.arguments.len() == source.arguments.len() && method.references().zip(source.references()).map(|(left, right)| Ok(normalize(left)? == caller_type(right)?)).collect::<Result<Vec<_>, IrVerifyError>>()?.into_iter().all(|valid| valid),
                        (Requirement::Operation(operation), Requirement::Operation(source)) => operation.authority == source.authority && operation.effects == source.effects && operation.arguments.len() == source.arguments.len() && operation.references().zip(source.references()).map(|(left, right)| Ok(normalize(left)? == caller_type(right)?)).collect::<Result<Vec<_>, IrVerifyError>>()?.into_iter().all(|valid| valid),
                        (Requirement::Invocation { callable, arguments, result, domain }, Requirement::Invocation { callable: source_callable, arguments: source_arguments, result: source_result, domain: source_domain }) => {
                            domain == source_domain && arguments.len() == source_arguments.len() && normalize(*callable)? == caller_type(*source_callable)? && normalize(*result)? == caller_type(*source_result)?
                                && arguments.iter().zip(source_arguments).map(|(argument, source)| Ok(argument.kind == source.kind && normalize(argument.ty)? == caller_type(source.ty)?)).collect::<Result<Vec<_>, IrVerifyError>>()?.into_iter().all(|valid| valid)
                        },
                        (Requirement::Eligibility { predicate, ty }, Requirement::Eligibility { predicate: source_predicate, ty: source_ty }) => predicate == source_predicate && normalize(*ty)? == caller_type(*source_ty)?,
                        (Requirement::Add { left, right, result }, Requirement::Add { left: source_left, right: source_right, result: source_result }) => normalize(*left)? == caller_type(*source_left)? && normalize(*right)? == caller_type(*source_right)? && normalize(*result)? == caller_type(*source_result)?,
                        (Requirement::Projection { receiver, field, result, .. }, Requirement::Projection { receiver: source_receiver, field: source_field, result: source_result, .. }) => field == source_field && normalize(*receiver)? == caller_type(*source_receiver)? && normalize(*result)? == caller_type(*source_result)?,
                        _ => false,
                    };
                    if !valid { return Err(failure("forwarded requirement disagrees with the symbolic declaration map")); }
                }
                ForwardedRequirement::Fixed(witness) => match (requirement, *witness) {
                    (Requirement::TagConstructor(constructor), RequirementWitness::TagConstructor) => self.verify_scoped_tag_constructor_requirement(pools, callee, constructor)?,
                    (Requirement::Eligibility { predicate, ty }, RequirementWitness::Eligibility { predicate: actual_predicate, ty: actual }) => {
                        if *predicate != actual_predicate || normalize(*ty)? != Self::normalized_ground_type(pools, actual)? || !predicate.accepts_closed_display(&pools.to_type(actual)?) {
                            return Err(failure("fixed eligibility witness changes its scoped requirement"));
                        }
                    }
                    (Requirement::Add { left, right, result }, RequirementWitness::Add { operation, left: actual_left, right: actual_right, result: actual_result }) => {
                        if normalize(*left)? != normalize_ground(pools.to_type(actual_left)?) || normalize(*right)? != normalize_ground(pools.to_type(actual_right)?) || normalize(*result)? != normalize_ground(pools.to_type(actual_result)?) { return Err(failure("fixed operation evidence cannot discharge a varying symbolic requirement")); }
                        let valid = match operation { ConcreteOperationId::AddInt => matches!(pools.type_tag(actual_left)?, TypeTag::Int | TypeTag::UInt) && matches!(pools.type_tag(actual_right)?, TypeTag::Int | TypeTag::UInt) && pools.type_tag(actual_result)? == TypeTag::Int, ConcreteOperationId::AddFloat => pools.type_tag(actual_left)? == TypeTag::Float && pools.type_tag(actual_right)? == TypeTag::Float && pools.type_tag(actual_result)? == TypeTag::Float, ConcreteOperationId::AddStr => pools.type_tag(actual_left)? == TypeTag::Str && pools.type_tag(actual_right)? == TypeTag::Str && pools.type_tag(actual_result)? == TypeTag::Str };
                        if !valid { return Err(failure("fixed operation evidence has an unsupported concrete signature")); }
                    }
                    (Requirement::Projection { receiver, field, result, .. }, RequirementWitness::Projection { layout, field_slot, result: actual_result }) => {
                        let layout = self.layout(layout)?;
                        let (actual_field, ty) = *layout.fields.get(field_slot as usize).ok_or_else(|| failure("fixed projection slot is out of bounds"))?;
                        let actual_receiver = Self::normalized_ground_type(pools, layout.record_type)?;
                        let expected_receiver = normalize(*receiver)?;
                        let compatible = match (&expected_receiver, &actual_receiver) { (NormalizedType::Record(expected, None), NormalizedType::Record(actual, None)) => expected.iter().all(|(name, ty)| actual.get(name) == Some(ty)), _ => expected_receiver == actual_receiver };
                        if actual_field != *field || ty != actual_result || !compatible || normalize(*result)? != Self::normalized_ground_type(pools, actual_result)? { return Err(failure("fixed projection evidence disagrees with its symbolic requirement")); }
                    }
                    _ => return Err(failure("fixed forwarding witness has the wrong requirement kind")),
                },
            }
        }
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub(super) fn references_equal(&self, pools: &SemanticPools, scope: SchemeScopeId, left: TypeRef, right: TypeRef) -> Result<bool, IrVerifyError> {
        let scope = self.scope(scope)?;
        Ok(self.normalized(pools, scope, left, None, &mut Vec::new())? == self.normalized(pools, scope, right, None, &mut Vec::new())?)
    }
    pub(super) fn reference_equals_ground(&self, pools: &SemanticPools, scope: SchemeScopeId, reference: TypeRef, ground: crate::sema::types::Type) -> Result<bool, IrVerifyError> {
        Ok(self.normalized(pools, self.scope(scope)?, reference, None, &mut Vec::new())? == normalize_ground(ground))
    }
    fn normalized_call_result(&self, pools: &SemanticPools, scope: SchemeScopeId, instruction: u32) -> Result<NormalizedType, IrVerifyError> {
        let call = self.call(instruction).ok_or_else(|| failure("symbolic source call lacks prepared evidence"))?;
        match call.evidence {
            CallEvidence::Ground(id) => Self::normalized_ground_type(pools, self.instance(id)?.result_type),
            CallEvidence::Forwarded(id) => {
                let plan = self.forwarding(id)?;
                if plan.caller != scope { return Err(failure("symbolic source call belongs to another scope")); }
                let substitutions = plan.substitutions.iter().map(|&reference| self.normalized(pools, self.scope(scope)?, reference, None, &mut Vec::new())).collect::<Result<Vec<_>, _>>()?;
                let target = self.scope(plan.callee)?;
                self.normalized(pools, target, target.result, Some(&substitutions), &mut Vec::new())
            }
        }
    }
    pub(super) fn call_result_matches(&self, pools: &SemanticPools, scope: SchemeScopeId, instruction: u32, reference: TypeRef) -> Result<bool, IrVerifyError> {
        let expected = self.normalized(pools, self.scope(scope)?, reference, None, &mut Vec::new())?;
        Ok(expected == self.normalized_call_result(pools, scope, instruction)?)
    }
    pub(super) fn call_result_equals_ground(&self, pools: &SemanticPools, scope: SchemeScopeId, instruction: u32, ground: crate::sema::types::Type) -> Result<bool, IrVerifyError> {
        Ok(normalize_ground(ground) == self.normalized_call_result(pools, scope, instruction)?)
    }

}
