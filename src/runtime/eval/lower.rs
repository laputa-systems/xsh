//! AST -> lowered-IR lowering pass, split out of `eval.rs`.

use crate::modules::{ModuleFnSig, RuntimeOp, api_spec};
use crate::runtime::value::{DurationValue, PathValue, RecordMap, RegexValue, RuntimeError, Value};
use crate::sema::check::{
    CheckedApiCall, CompactBodyFacts, CompactDeclOutput, CompactTypeDefInfo, Conversion,
};
use crate::sema::records::standard_record_type;
use crate::sema::types::{CallableParamType, ModuleExportType, Type};
use crate::source::{SourceMap, Span};
use crate::symbol::{Name, QualifiedName, Symbol};
use crate::syntax::arena::DeferTrigger;
use crate::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCallArg,
    ArenaCallArgKind, ArenaCommand, ArenaCommandArgKind, ArenaExprKind, ArenaExprOrRun,
    ArenaFmtPart, ArenaPatternKind, ArenaProgram, ArenaRecordFieldKind, ArenaSpawnTarget,
    ArenaStmtKind, ArenaStreamStage, ArenaTypeExprTag, ArenaWordPart, AstArena, BindingTargetId,
    BlockId, ExprId, FunctionDefId, PatternId, StmtId, TypeExprId,
};
use crate::syntax::node::{
    AssignOp, BinaryOp, CommandWordRefSegment, CoreCommand, RunKind, StreamStageKind, UnaryOp,
    parse_command_word_reference,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use xsh_registry::types::BuiltinTypeName;

use super::lowered_ops::{
    lowered_binary_op, lowered_value_from_runtime_any, lowered_value_matches,
};
use super::{
    LoweredProcessCommandArgv, LoweredProcessCommandBuilderEntry, LoweredRecordEntry,
    LoweredRecordUpdates, LoweredRunCapture, LoweredSpawnRun, lowered_record_vec_get,
    lowered_record_vec_get_mut, lowered_record_vec_insert,
};

/// Name -> dense slot-index map used while lowering a function or top-level
/// region. Slots are allocated densely and never reused; `high_water` is the
/// runtime slot-array size, so retired names do not need synthetic keys.
#[derive(Clone, Default)]
pub(super) struct SlotScope {
    pattern_slots: Option<FxHashMap<Name, usize>>,
    // Slots declared for pattern captures; slots are never reused.
    pattern_capture_slots: FxHashSet<usize>,
    indices: FxHashMap<Name, usize>,
    // Guarded receivers and explicit pipeline inputs bind once before their
    // selected operation; exact expression IDs reuse that retained value.
    postfix_receivers: FxHashMap<ExprId, BuildExprId>,
    guarded_postfixes: FxHashSet<ExprId>,
    // The condition operand being lowered under the propagation the checker
    // decided for it, so the operand itself is lowered once, unwrapped.
    propagated_condition: Option<ExprId>,
    /// The `?` written on a deferred call. `defer f()?` is `defer f()`, so its
    /// failure is placed at the call, as the bare form's is.
    deferred_propagation: Option<ExprId>,
    bound_call_entries: FxHashSet<ExprId>,
    types: FxHashMap<Name, Type>,
    captures: FxHashSet<Name>,
    // (name, previous slot) for each block-local declaration, so `exit` can
    // restore a shadowed outer binding (or drop a freshly-introduced one).
    declared: Vec<(Name, Option<usize>, Option<Type>, bool)>,
    // Index into `declared` where the innermost scope began: what follows
    // belongs to the scope being lowered now, and anything older is an
    // enclosing scope this one may shadow.
    level_start: usize,
    high_water: usize,
}

/// Snapshot of a `SlotScope` taken on entering a nested block (see `enter`/`exit`).
pub(super) struct SlotSnapshot {
    declared_len: usize,
    level_start: usize,
    high_water: usize,
}

/// Source-ordered bindings and one lowered value for each static expansion entry.
pub(super) struct LoweredArgumentValues {
    pub(super) values: Vec<BuildExprId>,
    pub(super) bindings: Vec<(BuildExprId, usize)>,
}

struct LoweredFsFilesArgs {
    root: ExprId,
    gitignore: Option<ExprId>,
    stat: Option<ExprId>,
    hidden: Option<ExprId>,
    exts: Option<ExprId>,
}

struct LoweredFsListArgs {
    path: ExprId,
    stat: Option<ExprId>,
    ordered: Option<ExprId>,
}

struct LoweredArchiveTarCreateArgs {
    path: ExprId,
    root: ExprId,
    entries: ExprId,
    compression: Option<ExprId>,
    overwrite: Option<ExprId>,
}

struct LoweredPathMkdirArgs {
    parents: Option<ExprId>,
}

struct LoweredPathRemoveArgs {
    missing_ok: Option<ExprId>,
}

struct LoweredPathWriteArgs {
    data: ExprId,
}

/// The checker's binding of a registered call's source entries to the
/// parameters of its selected overload.
struct CheckedApiArguments {
    sig: &'static ModuleFnSig,
    values: Vec<Option<ExprId>>,
}

impl CheckedApiArguments {
    fn get(&self, name: &str) -> Option<ExprId> {
        self.values[self
            .sig
            .params
            .iter()
            .position(|param| param.name == name)?]
    }

    /// Parameter-ordered values with absent trailing defaults dropped.
    fn ordered(&self) -> Vec<Option<ExprId>> {
        let mut values = self.values.clone();
        while values.last().is_some_and(Option::is_none) {
            values.pop();
        }
        values
    }
}

struct LoweredModuleCallArgs {
    semantic_rule: crate::modules::signature::SemanticRule,
    op: RuntimeOp,
    args: Vec<Option<ExprId>>,
}

struct LoweredHashVerifyFileArgs {
    path: ExprId,
    /// The algorithm the checksum argument's name selected.
    algorithm: &'static str,
    expected: ExprId,
}

struct LoweredProcessCommandArgvArgs {
    target: ExprId,
    argv: ExprId,
    cwd: Option<ExprId>,
    env: Option<ExprId>,
    stdin: Option<ExprId>,
    stdout: Option<ExprId>,
    stderr: Option<ExprId>,
    stdout_append: Option<ExprId>,
    stderr_append: Option<ExprId>,
    timeout: Option<ExprId>,
    detach: Option<ExprId>,
    new_session: Option<ExprId>,
    ignore_hup: Option<ExprId>,
    cpu_max: Option<ExprId>,
    accept: Option<ExprId>,
}

fn positional_call_args(args: &[ArenaCallArg]) -> Option<Vec<ExprId>> {
    let mut positional = Vec::with_capacity(args.len());
    for arg in args {
        let ArenaCallArgKind::Positional(expr) = arg.kind else {
            return None;
        };
        positional.push(expr);
    }
    Some(positional)
}

fn single_positional_arena_call_arg(args: &[ArenaCallArg]) -> Option<ExprId> {
    let [arg] = args else {
        return None;
    };
    let ArenaCallArgKind::Positional(value) = arg.kind else {
        return None;
    };
    Some(value)
}

fn lowered_str_byte_op(name: &str, args: &[BuildExprId]) -> bool {
    match name {
        "byte_len" => args.is_empty(),
        "byte_at" => args.len() == 1,
        _ => false,
    }
}

/// The checker's public-overload binding applies to a script implementation
/// that shares its parameters. A specialized form such as `hash.verify_file`
/// does not and lowers through its own path.
fn script_argument_slots<'p>(
    plan: &'p CheckedApiCall,
    params: &[CallableParamType],
) -> Option<&'p [usize]> {
    let public = crate::sema::builtin_templates::callable_parameters(plan.sig);
    (public.len() == params.len()
        && public
            .iter()
            .zip(params)
            .all(|(public, param)| public.name == param.name))
    .then_some(plan.argument_slots.as_slice())
}

fn lower_process_command_argv_args(
    args: &CheckedApiArguments,
) -> Option<LoweredProcessCommandArgvArgs> {
    Some(LoweredProcessCommandArgvArgs {
        target: args.get("target")?,
        argv: args.get("argv")?,
        cwd: args.get("cwd"),
        env: args.get("env"),
        stdin: args.get("stdin"),
        stdout: args.get("stdout"),
        stderr: args.get("stderr"),
        stdout_append: args.get("stdout_append"),
        stderr_append: args.get("stderr_append"),
        timeout: args.get("timeout"),
        detach: args.get("detach"),
        new_session: args.get("new_session"),
        ignore_hup: args.get("ignore_hup"),
        cpu_max: args.get("cpu_max"),
        accept: args.get("accept"),
    })
}

fn lowered_module_call_args(
    module: Name,
    name: Name,
    args: &[ArenaCallArg],
    plan: Option<&CheckedApiCall>,
) -> Option<LoweredModuleCallArgs> {
    let plan = plan?;
    let sig = plan.sig;
    let mut lowered_args = vec![None; sig.params.len()];
    for (arg, &slot) in args.iter().zip(&plan.argument_slots) {
        lowered_args[slot] = Some(compact_call_arg_expr(arg)?);
    }
    while lowered_args.last().is_some_and(Option::is_none) {
        lowered_args.pop();
    }
    let op = match (module.as_str().as_str(), name.as_str().as_str()) {
        ("fs", "hardlink") => RuntimeOp::FsHardlink,
        ("mime", "lookup_ext") => RuntimeOp::MimeLookupExt,
        ("mime", "lookup_path") => RuntimeOp::MimeLookupPath,
        _ if sig.script_impl().is_some() || lowered_module_sig_type(sig).is_none() => return None,
        _ => sig.op,
    };
    Some(LoweredModuleCallArgs {
        semantic_rule: sig.semantic_rule,
        op,
        args: lowered_args,
    })
}

fn lower_hash_verify_file_args(args: &[ArenaCallArg]) -> Option<LoweredHashVerifyFileArgs> {
    let [path, checksum] = crate::sema::arguments::bind_hash_verify_file_arguments(args)?;
    let (path, checksum) = (&args[path], &args[checksum]);
    let path = match path.kind {
        ArenaCallArgKind::Positional(expr) => expr,
        ArenaCallArgKind::Named { name, value, .. } if name == "path" => value,
        ArenaCallArgKind::Named { .. }
        | ArenaCallArgKind::Splice { .. }
        | ArenaCallArgKind::NamedSpread { .. } => return None,
    };
    let ArenaCallArgKind::Named { name, value, .. } = checksum.kind else {
        return None;
    };
    // The name is carried as text so the embedded implementation selects the
    // algorithm; the digest primitive it then calls is chosen by that name.
    let algorithm = match name.as_str().as_str() {
        "md5" => "md5",
        "sha1" => "sha1",
        "sha256" => "sha256",
        "sha512" => "sha512",
        _ => return None,
    };
    Some(LoweredHashVerifyFileArgs {
        path,
        algorithm,
        expected: value,
    })
}

fn compact_call_arg_expr(arg: &ArenaCallArg) -> Option<ExprId> {
    match arg.kind {
        ArenaCallArgKind::Positional(expr) | ArenaCallArgKind::Named { value: expr, .. } => {
            Some(expr)
        }
        ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => None,
    }
}

fn lowered_module_sig_type(sig: &ModuleFnSig) -> Option<LoweredType> {
    if matches!(sig.op, RuntimeOp::TimeSleep | RuntimeOp::TimeMeasure) {
        return lowered_module_op_supported(sig.op).then_some(LoweredType::Result);
    }
    lowered_module_op_supported(sig.op).then(|| lowered_checked_type(&sig.return_ty))?
}

fn lowered_module_op_supported(op: RuntimeOp) -> bool {
    #[cfg(feature = "native-tests")]
    if lowered_native_test_op_supported(op) {
        return true;
    }
    if crate::modules::process::is_prim(op)
        || crate::modules::unix::is_prim(op)
        || matches!(op, RuntimeOp::ProcessWaitTimeout | RuntimeOp::IoFlushStdout | RuntimeOp::IoFlushStderr)
    {
        return true;
    }
    matches!(
        op,
        RuntimeOp::CpuCount
            | RuntimeOp::AppletHashPassword
            | RuntimeOp::AppletVerifyPassword
            | RuntimeOp::AppletCurrentEuid
            | RuntimeOp::AppletCurrentExe
            | RuntimeOp::AppletLoginSession
            | RuntimeOp::AppletSuSession
            | RuntimeOp::AppletSuloginSession
            | RuntimeOp::AppletMdev
            | RuntimeOp::CliParse
            | RuntimeOp::CliApplet
            | RuntimeOp::CliParseFull
            | RuntimeOp::CliCommands
            | RuntimeOp::CliTokens
            | RuntimeOp::CliUsage
            | RuntimeOp::ArchiveCompress
            | RuntimeOp::ArchiveCpioCreate
            | RuntimeOp::ArchiveCpioExtract
            | RuntimeOp::ArchiveCpioList
            | RuntimeOp::ArchiveDecompress
            | RuntimeOp::ArchiveDecompressBytes
            | RuntimeOp::ArchiveTarCreate
            | RuntimeOp::ArchiveTarExtract
            | RuntimeOp::ArchiveTarList
            | RuntimeOp::ArchiveZipExtract
            | RuntimeOp::ArchiveZipList
            | RuntimeOp::ElfInspect
            | RuntimeOp::BytesFromText
            | RuntimeOp::BytesHuman
            | RuntimeOp::BytesCopy
            | RuntimeOp::BytesCopyFile
            | RuntimeOp::BytesFromInts
            | RuntimeOp::BytesSqueeze
            | RuntimeOp::BytesRepeatPrefixCount
            | RuntimeOp::BytesConcat
            | RuntimeOp::BytesPackLe
            | RuntimeOp::BytesPackBe
            | RuntimeOp::BytesUnpackLe
            | RuntimeOp::BytesUnpackBe
            | RuntimeOp::BytesUnpackFloat
            | RuntimeOp::BytesReadAt
            | RuntimeOp::BytesResize
            | RuntimeOp::BytesWriteAt
            | RuntimeOp::BytesZeroAt
            | RuntimeOp::BytesZero
            | RuntimeOp::DiffUnified
            | RuntimeOp::DnsLookup
            | RuntimeOp::DnsResolveHost
            | RuntimeOp::DnsReverse
            | RuntimeOp::DnsNameservers
            | RuntimeOp::EnvGet
            | RuntimeOp::EnvGetOr
            | RuntimeOp::EnvSet
            | RuntimeOp::EnvBool
            | RuntimeOp::EnvInt
            | RuntimeOp::EnvPath
            | RuntimeOp::EnvList
            | RuntimeOp::EnvPathList
            | RuntimeOp::EnvPathEntries
            | RuntimeOp::HashParseCheckLine
            | RuntimeOp::MimeLookupExt
            | RuntimeOp::MimeLookupPath
            | RuntimeOp::MimeParse
            | RuntimeOp::FsCwd
            | RuntimeOp::FsDirs
            | RuntimeOp::FsChildren
            | RuntimeOp::FsMetadata
            | RuntimeOp::FsFilesystemStats
            | RuntimeOp::FsMounts
            | RuntimeOp::FsMountFor
            | RuntimeOp::FsReadText
            | RuntimeOp::FsWrite
            | RuntimeOp::FsWriteAtomic
            | RuntimeOp::FsMkdir
            | RuntimeOp::FsRemove
            | RuntimeOp::FsExists
            | RuntimeOp::FsExecutable
            | RuntimeOp::FsWorldWritable
            | RuntimeOp::FsSticky
            | RuntimeOp::FsSetuid
            | RuntimeOp::FsSetgid
            | RuntimeOp::FsOwnerExecutable
            | RuntimeOp::FsGroupExecutable
            | RuntimeOp::FsOtherExecutable
            | RuntimeOp::FsOpenRoot
            | RuntimeOp::FsCloseRoot
            | RuntimeOp::FsRootPath
            | RuntimeOp::FsRootOpenRoot
            | RuntimeOp::FsRootChildren
            | RuntimeOp::FsRootRead
            | RuntimeOp::FsRootReadResult
            | RuntimeOp::FsRootFilesystemStats
            | RuntimeOp::FsRootReadText
            | RuntimeOp::FsRootWrite
            | RuntimeOp::FsRootWriteAtomic
            | RuntimeOp::FsRootMetadata
            | RuntimeOp::FsRootStat
            | RuntimeOp::FsRootExists
            | RuntimeOp::FsRootMkdir
            | RuntimeOp::FsRootRemove
            | RuntimeOp::FsRootReadlink
            | RuntimeOp::FsRootReadlinkResult
            | RuntimeOp::FsRootSymlink
            | RuntimeOp::FsRootChmod
            | RuntimeOp::FsRootInstallFile
            | RuntimeOp::FsCopy
            | RuntimeOp::FsCopyTree
            | RuntimeOp::FsRename
            | RuntimeOp::FsRemoveManifest
            | RuntimeOp::FsInstall
            | RuntimeOp::FsInstallAs
            | RuntimeOp::FsTruncate
            | RuntimeOp::FsChmod
            | RuntimeOp::FsHardlink
            | RuntimeOp::FsChown
            | RuntimeOp::FsChgrp
            | RuntimeOp::FsMkfifo
            | RuntimeOp::FsStat
            | RuntimeOp::FsSetOwner
            | RuntimeOp::FsSetTimes
            | RuntimeOp::FsMknod
            | RuntimeOp::FsMakedev
            | RuntimeOp::FsDevMajor
            | RuntimeOp::FsDevMinor
            | RuntimeOp::FsLink
            | RuntimeOp::FsUmask
            | RuntimeOp::FsStatvfs
            | RuntimeOp::FsRenameNoreplace
            | RuntimeOp::FsDataRanges
            | RuntimeOp::FsCopyFile
            | RuntimeOp::FsFsync
            | RuntimeOp::FsSync
            | RuntimeOp::FsAccess
            | RuntimeOp::FsXattrList
            | RuntimeOp::FsXattrGet
            | RuntimeOp::FsXattrSet
            | RuntimeOp::FsXattrRemove
            | RuntimeOp::FsSyncPath
            | RuntimeOp::FsRenameExchange
            | RuntimeOp::FsPathLimits
            | RuntimeOp::FsSymlink
            | RuntimeOp::FsLock
            | RuntimeOp::FsUnlock
            | RuntimeOp::FsTempFile
            | RuntimeOp::FsProjectRoot
            | RuntimeOp::FsUserRoot
            | RuntimeOp::FsGitroot
            | RuntimeOp::FsIsDir
            | RuntimeOp::FsIsFile
            | RuntimeOp::FsIsSymlink
            | RuntimeOp::FsTempSibling
            | RuntimeOp::FsCloseRootIfOpen
            | RuntimeOp::FsUnlockIfHeld
            | RuntimeOp::FsSymlinkAt
            | RuntimeOp::GroupCurrent
            | RuntimeOp::GroupLookup
            | RuntimeOp::GroupByGid
            | RuntimeOp::GroupAdd
            | RuntimeOp::GroupRemove
            | RuntimeOp::HashDigestFile
            | RuntimeOp::HashDigestStdin
            | RuntimeOp::HashChecksum
            | RuntimeOp::HashChecksumStdin
            | RuntimeOp::HashMd5
            | RuntimeOp::HashSha1
            | RuntimeOp::HashSha256
            | RuntimeOp::HashSha512
            | RuntimeOp::HashCrc32
            | RuntimeOp::HashCrc32c
            | RuntimeOp::IoStdinRead
            | RuntimeOp::IoStdinBytes
            | RuntimeOp::IoStdinText
            | RuntimeOp::IoStdinLine
            | RuntimeOp::IoWriteStderr
            | RuntimeOp::IoWriteStdout
            | RuntimeOp::IoWriteStdoutBytes
            | RuntimeOp::IniDecode
            | RuntimeOp::IniRead
            | RuntimeOp::IniEncode
            | RuntimeOp::IniWrite
            | RuntimeOp::JsonDecode
            | RuntimeOp::JsonEncode
            | RuntimeOp::JsonEncodeLines
            | RuntimeOp::JsonGet
            | RuntimeOp::JsonRead
            | RuntimeOp::JsonRemove
            | RuntimeOp::JsonSet
            | RuntimeOp::JsonWrite
            | RuntimeOp::JsonWriteLines
            | RuntimeOp::LinuxInterfaces
            | RuntimeOp::LinuxRoutes
            | RuntimeOp::LinuxNetworkDump
            | RuntimeOp::LinuxLinkUp
            | RuntimeOp::LinuxLinkDown
            | RuntimeOp::LinuxSetIpv4Address
            | RuntimeOp::LinuxFlushIpv4Addresses
            | RuntimeOp::LinuxAddDefaultIpv4Route
            | RuntimeOp::LinuxDelDefaultIpv4Route
            | RuntimeOp::LinuxDhcpSocket
            | RuntimeOp::LinuxDhcpSend
            | RuntimeOp::LinuxDhcpRecv
            | RuntimeOp::LinuxDhcpClose
            | RuntimeOp::LinuxDhcpSendRelease
            | RuntimeOp::MapEmpty
            | RuntimeOp::ModuleLoad
            | RuntimeOp::NetPool
            | RuntimeOp::NetClosePool
            | RuntimeOp::NetCloseAllPools
            | RuntimeOp::NetStart
            | RuntimeOp::NetRequest
            | RuntimeOp::NetRequestMany
            | RuntimeOp::NetDownloadMany
            | RuntimeOp::NetDownload
            | RuntimeOp::NetUpload
            | RuntimeOp::PathAbsolute
            | RuntimeOp::PathParseBytes
            | RuntimeOp::PatchApply
            | RuntimeOp::ProcessList
            | RuntimeOp::ProcessThreads
            | RuntimeOp::ProcessCurrentPid
            | RuntimeOp::ProcessScriptPath
            | RuntimeOp::ProcessStats
            | RuntimeOp::ProcessWhich
            | RuntimeOp::ProcessPort
            | RuntimeOp::ProcessPorts
            | RuntimeOp::ProcessPortsForPid
            | RuntimeOp::ProcessSignal
            | RuntimeOp::ProcessKill
            | RuntimeOp::ProcessRun
            | RuntimeOp::ProcessSpawn
            | RuntimeOp::ProcessWaitAny
            | RuntimeOp::ProcessWaitReady
            | RuntimeOp::RegexCompile
            | RuntimeOp::RegexFindBytes
            | RuntimeOp::RegexCapturesBytes
            | RuntimeOp::UnixReadFd
            | RuntimeOp::UnixWriteFd
            | RuntimeOp::UnixSeekFd
            | RuntimeOp::UnixCpuFeatures
            | RuntimeOp::CompressionTransform
            | RuntimeOp::CompressionGzipName
            | RuntimeOp::LinuxModulePlan
            | RuntimeOp::LinuxBlockSignatures
            | RuntimeOp::LinuxWipeBlockSignatures
            | RuntimeOp::LinuxFileProject
            | RuntimeOp::LinuxSetFileProject
            | RuntimeOp::LinuxSample
            | RuntimeOp::LinuxUmount
            | RuntimeOp::LinuxBlockdevInfo
            | RuntimeOp::LinuxBlockdevSetReadOnly
            | RuntimeOp::LinuxBlockdevFlush
            | RuntimeOp::LinuxBlockdevRereadPartitionTable
            | RuntimeOp::LinuxFstrim
            | RuntimeOp::LinuxFsfreeze
            | RuntimeOp::UnixPollFd
            | RuntimeOp::SetEmpty
            | RuntimeOp::SetFrom
            | RuntimeOp::ShlexQuote
            | RuntimeOp::ShlexJoin
            | RuntimeOp::SystemHostname
            | RuntimeOp::SystemUname
            | RuntimeOp::SystemMemory
            | RuntimeOp::SystemExecutionUnits
            | RuntimeOp::SystemOsRelease
            | RuntimeOp::TimeNow
            | RuntimeOp::TimeClockResolution
            | RuntimeOp::TimeFormat
            | RuntimeOp::TimeToCalendar
            | RuntimeOp::TimeFromCalendar
            | RuntimeOp::TimeSleep
            | RuntimeOp::TimeMillis
            | RuntimeOp::TimeSeconds
            | RuntimeOp::TimeMeasure
            | RuntimeOp::TimeDurationCompact
            | RuntimeOp::TuiLeftPad
            | RuntimeOp::TuiRightPad
            | RuntimeOp::TuiReadSecret
            | RuntimeOp::LinuxWriteDevice
            | RuntimeOp::LinuxReadDevice
            | RuntimeOp::LinuxBlkid
            | RuntimeOp::LinuxBlockDevices
            | RuntimeOp::LinuxChroot
            | RuntimeOp::LinuxDepmod
            | RuntimeOp::LinuxDmesg
            | RuntimeOp::LinuxDiskUsage
            | RuntimeOp::LinuxFileAttrs
            | RuntimeOp::LinuxFileVersion
            | RuntimeOp::LinuxFsck
            | RuntimeOp::LinuxHalt
            | RuntimeOp::LinuxHwclock
            | RuntimeOp::LinuxInsmod
            | RuntimeOp::LinuxIsMountpoint
            | RuntimeOp::LinuxKillAll
            | RuntimeOp::LinuxLoopAttach
            | RuntimeOp::LinuxLoopDetach
            | RuntimeOp::LinuxLoopList
            | RuntimeOp::LinuxMemInfo
            | RuntimeOp::LinuxMknod
            | RuntimeOp::LinuxMkswap
            | RuntimeOp::LinuxModinfo
            | RuntimeOp::LinuxModprobe
            | RuntimeOp::LinuxModules
            | RuntimeOp::LinuxMount
            | RuntimeOp::LinuxMountAll
            | RuntimeOp::LinuxOpenFiles
            | RuntimeOp::LinuxPartitionTable
            | RuntimeOp::LinuxPivotRoot
            | RuntimeOp::LinuxPoweroff
            | RuntimeOp::LinuxReboot
            | RuntimeOp::LinuxRfkillBlock
            | RuntimeOp::LinuxRfkillList
            | RuntimeOp::LinuxRfkillUnblock
            | RuntimeOp::LinuxRmmod
            | RuntimeOp::LinuxRootDevice
            | RuntimeOp::LinuxSetFileAttrs
            | RuntimeOp::LinuxSetFileVersion
            | RuntimeOp::LinuxSetHwclock
            | RuntimeOp::LinuxSetSystemClock
            | RuntimeOp::LinuxSwapon
            | RuntimeOp::LinuxSwaponAll
            | RuntimeOp::LinuxSwapoff
            | RuntimeOp::LinuxSwapoffAll
            | RuntimeOp::LinuxSwitchRoot
            | RuntimeOp::LinuxSysctlGet
            | RuntimeOp::LinuxSysctlLoadDirs
            | RuntimeOp::LinuxSysctlSet
            | RuntimeOp::LinuxUeventStream
            | RuntimeOp::LinuxUmountAll
            | RuntimeOp::LinuxWritePartitionTable
            | RuntimeOp::UnixExec
            | RuntimeOp::UnixExecEnv
            | RuntimeOp::UnixId
            | RuntimeOp::UnixKillAll
            | RuntimeOp::UnixKillProcessGroup
            | RuntimeOp::UnixNotifyClose
            | RuntimeOp::UnixNotifyReady
            | RuntimeOp::UnixPid1Setup
            | RuntimeOp::UnixReapChildEvents
            | RuntimeOp::UnixSetHostname
            | RuntimeOp::UnixSetTtyAttrs
            | RuntimeOp::UnixShutdownProcessGroups
            | RuntimeOp::UnixSpawnLoggedProcessGroup
            | RuntimeOp::UnixSpawnProcessGroup
            | RuntimeOp::UnixSpawnProcessGroupLog
            | RuntimeOp::UnixSpawnWithTty
            | RuntimeOp::UnixTty
            | RuntimeOp::UnixTtyAttrs
            | RuntimeOp::UnixUptimeSeconds
            | RuntimeOp::UnixWaitPid1Event
            | RuntimeOp::UserCurrent
            | RuntimeOp::UserGroups
            | RuntimeOp::UserLookup
            | RuntimeOp::UserByUid
            | RuntimeOp::UserAdd
            | RuntimeOp::UserRemove
            | RuntimeOp::UtilsCache
            | RuntimeOp::ErrorFailure
    )
}

#[cfg(feature = "native-tests")]
fn lowered_native_test_op_supported(op: RuntimeOp) -> bool {
    matches!(
        op,
        RuntimeOp::TestOk
            | RuntimeOp::TestEq
            | RuntimeOp::TestNe
            | RuntimeOp::TestErrorKind
            | RuntimeOp::TestFail
            | RuntimeOp::TestSkip
            | RuntimeOp::TestTimeout
            | RuntimeOp::TestTempPath
            | RuntimeOp::TestTempDir
            | RuntimeOp::TestTempFile
            | RuntimeOp::TestMock
            | RuntimeOp::TestCalls
            | RuntimeOp::TestLinuxFake
            | RuntimeOp::TestUnixFake
            | RuntimeOp::TestRunScript
            | RuntimeOp::TestRunXsh
            | RuntimeOp::TestRunXshtTrace
            | RuntimeOp::TestExpect
    )
}

fn lower_archive_tar_create_args(
    args: &CheckedApiArguments,
) -> Option<LoweredArchiveTarCreateArgs> {
    Some(LoweredArchiveTarCreateArgs {
        path: args.get("path")?,
        root: args.get("root")?,
        entries: args.get("entries")?,
        compression: args.get("compression"),
        overwrite: args.get("overwrite"),
    })
}

fn lower_path_mkdir_args(args: &CheckedApiArguments) -> LoweredPathMkdirArgs {
    LoweredPathMkdirArgs {
        parents: args.get("parents"),
    }
}

fn lower_path_remove_args(args: &CheckedApiArguments) -> LoweredPathRemoveArgs {
    LoweredPathRemoveArgs {
        missing_ok: args.get("missing_ok"),
    }
}

fn lower_path_write_args(args: &CheckedApiArguments) -> Option<LoweredPathWriteArgs> {
    Some(LoweredPathWriteArgs {
        data: args.get("data")?,
    })
}

fn build_expr(scratch: &Rc<RefCell<BuildScratch>>, row: BuildExprRow) -> BuildExprId {
    scratch.borrow_mut().expr(row)
}

macro_rules! push_build_row {
    ($self:expr, expr, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().expr(row)
    }};
    ($self:expr, stmt, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().stmt(row)
    }};
    ($self:expr, pattern, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().pattern(row)
    }};
    ($self:expr, int, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().int(row)
    }};
    ($self:expr, bool, $row:expr) => {{
        let row = $row;
        $self.scratch.borrow_mut().bool(row)
    }};
}

fn lower_command_word_reference(
    text: &str,
    slots: &SlotScope,
    span: Span,
    scratch: &Rc<RefCell<BuildScratch>>,
    constants: &crate::sema::constants::PreparedConstants,
    wire_enums: &crate::sema::wire_enums::PreparedWireEnums,
    namespace: Option<Name>,
) -> Option<BuildExprId> {
    let (root, segments) = parse_command_word_reference(text)?;
    let mut value = if let Some(slot) = slots.resolve(Name::intern(root)) {
        build_expr(scratch, BuildExprRow::Param(slot))
    } else if let Some(initializer) = constants
        .global_bindings
        .get(&(namespace, Name::intern(root)))
    {
        let origin = constants
            .origins
            .get(initializer)
            .copied()
            .unwrap_or(*initializer);
        let cached = scratch.borrow().prepared_constants.get(&origin).cloned();
        let value = if let Some(value) = cached {
            value
        } else {
            let value =
                lower_literal_constant(constants.values.get(initializer)?, Some(wire_enums))?;
            scratch
                .borrow_mut()
                .prepared_constants
                .insert(origin, value.clone());
            value
        };
        build_expr(
            scratch,
            BuildExprRow::PreparedConstant(super::PreparedConstantValue(value)),
        )
    } else if root == "env" && !segments.is_empty() {
        return lower_env_command_word_reference(&segments, span, scratch);
    } else {
        return None;
    };
    for segment in segments {
        value = match segment {
            CommandWordRefSegment::Field(name) => build_expr(
                scratch,
                BuildExprRow::Field {
                    base: value,
                    name: name.as_str(),
                    span,
                },
            ),
            CommandWordRefSegment::Index(index) => build_expr(
                scratch,
                BuildExprRow::Index {
                    base: value,
                    index: build_expr(scratch, BuildExprRow::Int(index)),
                    span,
                },
            ),
        };
    }
    Some(value)
}

fn lower_env_command_word_reference(
    segments: &[CommandWordRefSegment],
    span: Span,
    scratch: &Rc<RefCell<BuildScratch>>,
) -> Option<BuildExprId> {
    match segments {
        [CommandWordRefSegment::Field(name)] => {
            if *name == "PATH" {
                let name = build_expr(scratch, BuildExprRow::Str(name.to_string().into()));
                Some(build_expr(
                    scratch,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: RuntimeOp::EnvPathList,
                        args: vec![Some(name)],
                        span,
                    },
                ))
            } else {
                let name = build_expr(scratch, BuildExprRow::Str(name.to_string().into()));
                Some(build_expr(
                    scratch,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: RuntimeOp::EnvGet,
                        args: vec![Some(name)],
                        span,
                    },
                ))
            }
        }
        [
            CommandWordRefSegment::Field(type_name),
            CommandWordRefSegment::Field(var_name),
        ] => {
            let op = if *type_name == "Path" {
                RuntimeOp::EnvPath
            } else {
                RuntimeOp::EnvGet
            };
            let name = build_expr(scratch, BuildExprRow::Str(var_name.to_string().into()));
            Some(build_expr(
                scratch,
                BuildExprRow::ModuleCall {
                    cli_plan: None,
                    op,
                    args: vec![Some(name)],
                    span,
                },
            ))
        }
        _ => None,
    }
}

fn lowered_run_capture_type(kind: RunKind) -> Option<LoweredType> {
    match kind {
        RunKind::CaptureText => Some(LoweredType::Str),
        RunKind::CaptureBytes => Some(LoweredType::Bytes),
        // `run.capture --text/--bytes` yields a {status, stdout, stderr} record,
        // not a bare Str/Bytes, so field access on the binding lowers.
        RunKind::CaptureTextRecord | RunKind::CaptureBytesRecord => Some(LoweredType::Record),
        RunKind::StreamText | RunKind::StreamBytes => Some(LoweredType::Stream),
        _ => None,
    }
}

fn lowered_run_status_type(kind: RunKind) -> Option<LoweredType> {
    match kind {
        RunKind::Plain | RunKind::Status => Some(LoweredType::Status),
        _ => None,
    }
}

fn lowered_run_binding_type(kind: RunKind) -> Option<LoweredType> {
    lowered_run_capture_type(kind).or(match kind {
        RunKind::Plain | RunKind::Status => Some(LoweredType::Status),
        _ => None,
    })
}

fn lowered_arena_run_capture_type(
    arena: &AstArena,
    id: crate::syntax::arena::RunFormId,
) -> Option<LoweredType> {
    let run = arena.run_form(id);
    let segment = arena.run_segments(run.segments).first()?;
    lowered_run_capture_type(segment.kind)
}

fn lowered_arena_run_status_type(
    arena: &AstArena,
    id: crate::syntax::arena::RunFormId,
) -> Option<LoweredType> {
    let run = arena.run_form(id);
    let segment = arena.run_segments(run.segments).first()?;
    lowered_run_status_type(segment.kind)
}

fn compact_run_command_asserts_success(
    arena: &AstArena,
    id: crate::syntax::arena::RunFormId,
) -> bool {
    let run = arena.run_form(id);
    run.propagate
        || matches!(
            arena
                .run_segments(run.segments)
                .first()
                .map(|segment| segment.kind),
            Some(RunKind::Plain)
        )
}

fn lower_fs_files_args(args: &CheckedApiArguments) -> Option<LoweredFsFilesArgs> {
    Some(LoweredFsFilesArgs {
        root: args.get("path")?,
        gitignore: args.get("gitignore"),
        stat: args.get("stat"),
        hidden: args.get("hidden"),
        exts: args.get("exts"),
    })
}

fn lower_fs_list_args(args: &CheckedApiArguments) -> Option<LoweredFsListArgs> {
    Some(LoweredFsListArgs {
        path: args.get("path")?,
        stat: args.get("stat"),
        ordered: args.get("ordered"),
    })
}

impl SlotScope {
    /// Build a scope from ordered binding names (function params, top-level slots).
    pub(super) fn from_names<I: IntoIterator<Item = Name>>(names: I) -> Self {
        let indices = names
            .into_iter()
            .enumerate()
            .map(|(slot, name)| (name, slot))
            .collect::<FxHashMap<_, _>>();
        let high_water = indices.len();
        Self {
            indices,
            pattern_slots: None,
            pattern_capture_slots: FxHashSet::default(),
            postfix_receivers: FxHashMap::default(),
            guarded_postfixes: FxHashSet::default(),
            propagated_condition: None,
            deferred_propagation: None,
            bound_call_entries: FxHashSet::default(),
            types: FxHashMap::default(),
            captures: FxHashSet::default(),
            declared: Vec::new(),
            level_start: 0,
            high_water,
        }
    }

    /// High-water slot total: the size the runtime slot array must have.
    pub(super) fn count(&self) -> usize {
        self.high_water
    }

    pub(super) fn resolve(&self, name: Name) -> Option<usize> {
        self.indices.get(&name).copied()
    }

    pub(super) fn is_bound(&self, name: Name) -> bool {
        self.indices.contains_key(&name)
    }

    fn is_bound_non_capture(&self, name: Name) -> bool {
        self.is_bound(name) && !self.captures.contains(&name)
    }

    fn can_bind_pattern(&self, name: Name) -> bool {
        // Retired sibling-arm captures may be reused. A capture may shadow an
        // outer name or a declaration of this scope, which resolves again when
        // the capture retires, but not another live capture.
        self.pattern_slots
            .as_ref()
            .is_some_and(|slots| slots.contains_key(&name))
            || !self.is_bound_non_capture(name)
            || !self.is_declared_here(name)
            || self
                .resolve(name)
                .is_some_and(|slot| !self.pattern_capture_slots.contains(&slot))
    }

    fn declare_pattern_binding(&mut self, name: Name) -> usize {
        self.pattern_slots
            .as_ref()
            .and_then(|slots| slots.get(&name))
            .copied()
            .unwrap_or_else(|| self.declare_pattern_capture(name))
    }

    fn declare_pattern_capture(&mut self, name: Name) -> usize {
        let slot = self.declare(name);
        self.pattern_capture_slots.insert(slot);
        slot
    }

    /// Whether the innermost scope already declared `name`.
    fn is_declared_here(&self, name: Name) -> bool {
        self.declared[self.level_start..]
            .iter()
            .any(|(declared, ..)| *declared == name)
    }

    fn binding_type(&self, name: Name) -> Option<&Type> {
        self.types.get(&name)
    }

    /// Allocate the next dense slot and bind `name` to it.
    pub(super) fn declare(&mut self, name: Name) -> usize {
        self.declare_with_type(name, None)
    }

    fn declare_with_type(&mut self, name: Name, ty: Option<Type>) -> usize {
        let slot = self.high_water;
        self.high_water += 1;
        let previous = self.indices.insert(name, slot);
        let previous_capture = self.captures.remove(&name);
        let previous_ty = match ty {
            Some(ty) => self.types.insert(name, ty),
            None => self.types.remove(&name),
        };
        self.declared
            .push((name, previous, previous_ty, previous_capture));
        slot
    }

    fn declare_capture(&mut self, name: Name) -> usize {
        // A captured top-level binding is visible to the function but was not
        // declared in its body, so a local declaration may shadow it.
        let slot = self.high_water;
        self.high_water += 1;
        self.indices.insert(name, slot);
        self.captures.insert(name);
        slot
    }

    /// Allocate the next dense slot without binding a source-visible name.
    pub(super) fn reserve(&mut self, _tag: &str) -> usize {
        let slot = self.high_water;
        self.high_water += 1;
        slot
    }

    /// Drop `name` from resolution while keeping its slot reserved by `high_water`.
    ///
    /// A retired pattern capture also leaves the scope's declarations, so a
    /// later `let` of the same name is not mistaken for a redeclaration, and
    /// any binding it shadowed resolves again.
    pub(super) fn retire(&mut self, name: Name, slot: usize, _tag: &str) {
        let position = self
            .declared
            .iter()
            .rposition(|(declared, ..)| *declared == name);
        match position {
            Some(position)
                if position >= self.level_start && self.indices.get(&name) == Some(&slot) =>
            {
                let (_, previous, previous_ty, previous_capture) = self.declared.remove(position);
                match previous {
                    Some(previous) => self.indices.insert(name, previous),
                    None => self.indices.remove(&name),
                };
                match previous_ty {
                    Some(ty) => self.types.insert(name, ty),
                    None => self.types.remove(&name),
                };
                if previous_capture {
                    self.captures.insert(name);
                }
            }
            _ => {
                self.indices.remove(&name);
            }
        }
    }

    /// Snapshot bindings on entering a nested block scope.
    ///
    /// The snapshot restores the state `enter` observed; the scope it opens
    /// owns every declaration made from here until `exit`.
    pub(super) fn enter(&mut self) -> SlotSnapshot {
        let snapshot = SlotSnapshot {
            declared_len: self.declared.len(),
            level_start: self.level_start,
            high_water: self.high_water,
        };
        self.level_start = self.declared.len();
        snapshot
    }

    /// Restore name resolution to the block-entry snapshot, dropping block-local
    /// bindings while keeping every slot index allocated inside the block.
    /// A block-local declaration that shadowed an outer binding restores the
    /// outer slot; a freshly-introduced one is dropped.
    pub(super) fn exit(&mut self, snapshot: SlotSnapshot) {
        self.level_start = snapshot.level_start;
        for (name, previous, previous_ty, previous_capture) in
            self.declared[snapshot.declared_len..].iter().rev()
        {
            match previous {
                Some(slot) => {
                    self.indices.insert(*name, *slot);
                }
                None => {
                    self.indices.remove(name);
                }
            }
            match previous_ty {
                Some(ty) => {
                    self.types.insert(*name, ty.clone());
                }
                None => {
                    self.types.remove(name);
                }
            }
            if *previous_capture {
                self.captures.insert(*name);
            } else {
                self.captures.remove(name);
            }
        }
        self.declared.truncate(snapshot.declared_len);
        self.high_water = self.high_water.max(snapshot.high_water);
    }

    /// Consume the scope, yielding `(name, slot)` entries (top-level slot metadata).
    pub(super) fn into_entries(self) -> impl Iterator<Item = (Name, usize)> {
        self.indices.into_iter()
    }
}
use super::{
    BuildBoolId, BuildBoolRow, BuildExprId, BuildExprRow, BuildIntId, BuildIntRow, BuildPatternId,
    BuildPatternIdSlots, BuildPatternRow, BuildScratch, BuildStmtId, BuildStmtRow, BuildTopKind,
    BuildTopStmtId, BuildTopStmtRow, COMPACT_CALL_BLOCKER_KIND_COUNT,
    COMPACT_COMMAND_BLOCKER_KIND_COUNT, COMPACT_EXPR_KIND_COUNT, COMPACT_STMT_KIND_COUNT,
    COMPACT_TYPE_EXPR_TAG_COUNT, CompactLowerConstructProbeOutput, Flow, FunctionBuild,
    LowerableFunctions, LoweredAssignPath, LoweredAssignStep, LoweredCallArg, LoweredCompFields,
    LoweredCompTarget, LoweredErrorExpr, LoweredErrorPatternFields, LoweredFmtPart,
    LoweredFunctionBlocker, LoweredFunctionKey, LoweredFunctionKind, LoweredFunctionUnit,
    LoweredModuleExport, LoweredModuleExportKind, LoweredParamChecks, LoweredParamDefaults,
    LoweredParamKinds, LoweredParamNames, LoweredParamRest, LoweredPipelineStage,
    LoweredReturnKind, LoweredRunArg, LoweredRunArgKind, LoweredRunEnv, LoweredRunPipelineSegment,
    LoweredRunRedirection, LoweredStrPredicate, LoweredTopLevelBinding, LoweredTopLevelSlot,
    LoweredTopLevelSlots, LoweredType, LoweredTypeCheck, LoweredValue, ProgramBuild, ReduceByOp,
    ScanBytes, ScanCheck, ScanCondition, StmtFlow, lowered_method_name,
};

/// The bounded type a declaration names, over the base its alias resolves
/// to. The checker accepted the declaration, so the base fits; a base that
/// does not is left unbounded rather than given bounds it cannot have.
fn compact_bounded_type(range: crate::sema::validated::IntRange, base: Type) -> Type {
    Type::bounded(range, base.clone()).unwrap_or(base)
}

pub(super) fn lowered_arena_type(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Option<LoweredType> {
    lowered_arena_type_inner(arena, ty, declarations, 0)
}

fn lowered_arena_type_inner(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    depth: usize,
) -> Option<LoweredType> {
    if depth > declarations.types.len() {
        return None;
    }
    let index = ty.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Applied => lowered_checked_type(
            &declarations
                .record_constructors
                .resolve_type(arena, ty, None),
        ),
        ArenaTypeExprTag::Named => {
            let name = Name::from_symbol(crate::symbol::Symbol::from_raw(data.lhs));
            if let Some(lowered) = lowered_builtin_type_name(&name.as_str()) {
                return Some(lowered);
            }
            if standard_record_type(&name.as_str()).is_some() {
                return Some(LoweredType::Record);
            }
            if declarations.error_families_by_name.contains_key(&name) {
                return Some(LoweredType::Error);
            }
            match declarations.types.get(&name) {
                // A bounded integer is stored as its base.
                Some(CompactTypeDefInfo::Alias(alias) | CompactTypeDefInfo::Bounded(alias, _)) => {
                    lowered_arena_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Record(_)) => Some(LoweredType::Record),
                Some(CompactTypeDefInfo::Module(_)) => Some(LoweredType::Module),
                Some(CompactTypeDefInfo::TagUnion) => Some(LoweredType::Tag),
                None => Some(LoweredType::Record),
            }
        }
        ArenaTypeExprTag::Qualified => {
            let name = Name::from_symbol(crate::symbol::Symbol::from_raw(data.rhs));
            match declarations.types.get(&name) {
                // A bounded integer is stored as its base.
                Some(CompactTypeDefInfo::Alias(alias) | CompactTypeDefInfo::Bounded(alias, _)) => {
                    lowered_arena_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Record(_)) => Some(LoweredType::Record),
                Some(CompactTypeDefInfo::Module(_)) => Some(LoweredType::Module),
                Some(CompactTypeDefInfo::TagUnion) => Some(LoweredType::Tag),
                None => Some(LoweredType::Record),
            }
        }
        // A validated type is stored as its base.
        ArenaTypeExprTag::List | ArenaTypeExprTag::NonEmpty => Some(LoweredType::List),
        ArenaTypeExprTag::Map => Some(LoweredType::Map),
        ArenaTypeExprTag::Stream => Some(LoweredType::Stream),
        ArenaTypeExprTag::Set => Some(LoweredType::Set),
        ArenaTypeExprTag::Module => Some(LoweredType::Module),
        ArenaTypeExprTag::Result => Some(LoweredType::Result),
        // A union has no single runtime representation; its members keep
        // their own.
        ArenaTypeExprTag::Optional | ArenaTypeExprTag::Union => Some(LoweredType::Any),
        // A typed callable is the dynamic handle of its kind at run time.
        ArenaTypeExprTag::Callable => Some(if arena.callable_type_expr(ty).pure {
            LoweredType::Pure
        } else {
            LoweredType::Proc
        }),
    }
}

pub(super) fn probe_compact_lower_constructed_bodies(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
) -> CompactLowerConstructProbeOutput {
    let mut probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources: None,
        current_namespace: None,
        functions: None,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput {
            expr_type_facts: declarations.bodies.expr_types.len(),
            ..CompactLowerConstructProbeOutput::default()
        },
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::default(),
    };
    probe.probe_program();
    probe.output
}

/// Where a program's standard-library implementation calls resolve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum StdlibLowerLinkage {
    /// The program contains the embedded implementation modules it calls.
    Local,
    /// A loading program already prepared them; this program lowers its
    /// standard calls to functions resolved through the runtime's dynamic
    /// function table instead of reparsing embedded source.
    External,
}

pub(super) fn lower_compact_function_units_into(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    sources: &SourceMap,
    stdlib_linkage: StdlibLowerLinkage,
    mut emit: impl FnMut(LoweredFunctionUnit) -> Result<(), super::indexed::IrBuildError>,
) -> Result<(), super::indexed::IrBuildError> {
    // Every function's body must be walked to find its call edges, and finding
    // them once per function would make preparation quadratic in program size.
    // One index answers the whole loop: each unit's dependencies and its
    // component metadata come from the same walk of every body.
    let index = Rc::new(CompactFunctionIndex::new(program));
    let candidates = index
        .defs
        .iter()
        .map(|function| function.key)
        .collect::<Vec<_>>();
    let function_defs = Rc::new(RefCell::new(Some(Rc::clone(&index))));
    let spread_programs = Rc::default();
    let empty_pures = FxHashSet::default();
    let empty_procs = FxHashSet::default();
    let empty_qualified_pures = FxHashSet::default();
    let empty_qualified_procs = FxHashSet::default();
    let functions = LowerableFunctions::all_with_candidates(
        &empty_pures,
        &empty_procs,
        &empty_qualified_pures,
        &empty_qualified_procs,
        &candidates,
    );
    // `top_level_known` is a prefix scan of a module's statements, and the
    // functions of one module come out of the index in statement order, so one
    // cursor per module produces every function's prefix in a single pass. A
    // fresh scan per function would make preparation quadratic in a module's
    // size.
    let recorder = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources: Some(sources),
        current_namespace: None,
        functions: Some(&functions),
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage,
        function_defs: Rc::clone(&function_defs),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::clone(&spread_programs),
    };
    let mut prefixes: FxHashMap<Option<Name>, CompactTopLevelPrefix> = FxHashMap::default();
    for function in index.defs.iter().copied() {
        let prefix = prefixes
            .entry(function.namespace)
            .or_insert_with(|| CompactTopLevelPrefix::for_namespace(program, function.namespace));
        prefix.advance_to_recording(&recorder, function.id);
        let top_level_known = prefix.known.clone();
        let mut probe = CompactLowerConstructProbe {
            program,
            declarations,
            bodies: &declarations.bodies,
            source,
            sources: Some(sources),
            current_namespace: function.namespace,
            functions: Some(&functions),
            top_level_known,
            output: CompactLowerConstructProbeOutput::default(),
            last_blocker_detail: None,
            stdlib_linkage,
            function_defs: Rc::clone(&function_defs),
            scratch: Rc::new(RefCell::new(BuildScratch::default())),
            spread_programs: Rc::clone(&spread_programs),
        };
        let (scc_member_count, scc_group) = index.scc_metadata(&function);
        let unit = probe.lower_function_unit(
            function,
            compact_function_dependency_keys(&index, program, &function),
            scc_member_count,
            scc_group,
        );
        emit(unit)?;
    }
    Ok(())
}

pub(super) fn lower_compact_top_level_program_with_probe(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    sources: &SourceMap,
    functions: &LowerableFunctions<'_>,
) -> (ProgramBuild, CompactLowerConstructProbeOutput) {
    let mut probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources: Some(sources),
        current_namespace: None,
        functions: Some(functions),
        top_level_known: compact_top_level_known(
            program,
            declarations,
            source,
            Some(sources),
            None,
            Some(functions),
        ),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::default(),
    };
    let root = program.statement_ids().collect::<Vec<_>>();
    let lowered = probe.lower_program_statements(&root);
    (lowered, probe.output)
}

fn compact_top_level_known(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    sources: Option<&SourceMap>,
    namespace: Option<Name>,
    functions: Option<&LowerableFunctions<'_>>,
) -> FxHashMap<Name, LoweredTopLevelBinding> {
    let statements = match namespace {
        Some(namespace) => program
            .modules
            .iter()
            .find(|module| module.name == namespace)
            .map(|module| program.module_statements(module).collect::<Vec<_>>())
            .unwrap_or_default(),
        None => program.statement_ids().collect::<Vec<_>>(),
    };
    let probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources,
        current_namespace: namespace,
        functions,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::default(),
    };
    probe.collect_top_level_known(&statements)
}

/// The bindings visible to one function definition.
///
/// Used by the construct probe, which walks a module's statements itself and so
/// cannot share the cursor `lower_compact_function_units_into` keeps.
fn compact_function_top_level_known(
    program: &ArenaProgram,
    declarations: &CompactDeclOutput,
    source: &str,
    sources: Option<&SourceMap>,
    namespace: Option<Name>,
    function_id: FunctionDefId,
    functions: Option<&LowerableFunctions<'_>>,
) -> FxHashMap<Name, LoweredTopLevelBinding> {
    let probe = CompactLowerConstructProbe {
        program,
        declarations,
        bodies: &declarations.bodies,
        source,
        sources,
        current_namespace: namespace,
        functions,
        top_level_known: FxHashMap::default(),
        output: CompactLowerConstructProbeOutput::default(),
        last_blocker_detail: None,
        stdlib_linkage: StdlibLowerLinkage::Local,
        function_defs: Rc::new(RefCell::new(None)),
        scratch: Rc::new(RefCell::new(BuildScratch::default())),
        spread_programs: Rc::default(),
    };
    let mut prefix = CompactTopLevelPrefix::for_namespace(program, namespace);
    prefix.advance_to_recording(&probe, function_id);
    prefix.known
}

/// One module's `top_level_known` prefix, advanced as its functions are lowered.
///
/// The bindings visible to a function are those of the statements before its own
/// definition, so the functions of a module share one scan: the cursor stops at
/// the definition statement it was asked about and every later function resumes
/// from there.
struct CompactTopLevelPrefix {
    statements: Vec<StmtId>,
    known: FxHashMap<Name, LoweredTopLevelBinding>,
    position: usize,
}

impl CompactTopLevelPrefix {
    fn for_namespace(program: &ArenaProgram, namespace: Option<Name>) -> Self {
        let statements = match namespace {
            Some(namespace) => program
                .modules
                .iter()
                .find(|module| module.name == namespace)
                .map(|module| program.module_statements(module).collect::<Vec<_>>())
                .unwrap_or_default(),
            None => program.statement_ids().collect::<Vec<_>>(),
        };
        Self {
            statements,
            known: top_level_known_with_runtime_bindings(),
            position: 0,
        }
    }

    /// Record every statement before `function_id`'s own definition.
    fn advance_to_recording(
        &mut self,
        recorder: &CompactLowerConstructProbe<'_, '_>,
        function_id: FunctionDefId,
    ) {
        while let Some(stmt) = self.statements.get(self.position).copied() {
            if compact_stmt_contains_function_def(recorder.program, stmt, function_id) {
                break;
            }
            recorder.record_top_level_binding(stmt, &mut self.known);
            self.position += 1;
        }
    }
}

fn compact_stmt_contains_function_def(
    program: &ArenaProgram,
    stmt: StmtId,
    function_id: FunctionDefId,
) -> bool {
    match program.arena.stmt(stmt).kind {
        ArenaStmtKind::Export(inner) => {
            compact_stmt_contains_function_def(program, inner, function_id)
        }
        ArenaStmtKind::PureDef(id)
        | ArenaStmtKind::ProcDef(id)
        | ArenaStmtKind::CliMain(id)
        | ArenaStmtKind::StreamDef(id) => id == function_id,
        _ => false,
    }
}

/// Every function definition in a program, with its key index and the strongly
/// connected component metadata for each purity class.
///
/// Building any of this walks function bodies, so it is done once per lowering
/// pass rather than once per function.
struct CompactFunctionIndex {
    defs: Vec<CompactFunctionDef>,
    index_of: FxHashMap<LoweredFunctionKey, usize>,
    /// `[pure, proc]` component metadata keyed by function.
    scc: [FxHashMap<LoweredFunctionKey, (usize, Option<usize>)>; 2],
}

impl CompactFunctionIndex {
    fn new(program: &ArenaProgram) -> Self {
        let defs = compact_function_defs(program);
        let index_of = defs
            .iter()
            .enumerate()
            .map(|(index, function)| (function.key, index))
            .collect::<FxHashMap<_, _>>();
        let scc = [
            compact_scc_groups(program, &defs, true),
            compact_scc_groups(program, &defs, false),
        ];
        Self {
            defs,
            index_of,
            scc,
        }
    }

    fn scc_metadata(&self, function: &CompactFunctionDef) -> (usize, Option<usize>) {
        let class = usize::from(!function.pure);
        self.scc[class]
            .get(&function.key)
            .copied()
            .unwrap_or((1, None))
    }

    fn definition(&self, key: LoweredFunctionKey) -> Option<&CompactFunctionDef> {
        self.index_of
            .get(&key)
            .and_then(|index| self.defs.get(*index))
    }
}

/// Component metadata for every function of one purity.
///
/// A recursive group is reported with its member count so the runtime can size
/// the frame stack for it; a solitary function reports `(1, None)`.
fn compact_scc_groups(
    program: &ArenaProgram,
    defs: &[CompactFunctionDef],
    pure: bool,
) -> FxHashMap<LoweredFunctionKey, (usize, Option<usize>)> {
    let selected = defs
        .iter()
        .filter(|candidate| candidate.pure == pure)
        .collect::<Vec<_>>();
    let index_of = selected
        .iter()
        .enumerate()
        .map(|(index, function)| (function.key, index))
        .collect::<FxHashMap<_, _>>();
    let adjacency = selected
        .iter()
        .map(|function| {
            compact_function_call_edges(program, function.id, function.namespace, &index_of)
        })
        .collect::<Vec<_>>();
    let mut groups = FxHashMap::default();
    for (group, scc) in compact_tarjan_sccs(adjacency).into_iter().enumerate() {
        let metadata = if scc.len() > 1 {
            (scc.len(), Some(group))
        } else {
            (1, None)
        };
        for member in scc {
            groups.insert(selected[member].key, metadata);
        }
    }
    groups
}

#[derive(Clone, Copy)]
struct CompactFunctionDef {
    key: LoweredFunctionKey,
    id: FunctionDefId,
    pure: bool,
    namespace: Option<Name>,
    definition_span: Span,
}

fn compact_function_call_edges(
    program: &ArenaProgram,
    id: FunctionDefId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
) -> Vec<usize> {
    let mut edges = Vec::new();
    compact_collect_block_call_edges(
        program,
        program.arena.function_def(id).body,
        namespace,
        index_of,
        &mut edges,
    );
    edges.sort_unstable();
    edges.dedup();
    edges
}

fn compact_function_dependency_keys(
    functions: &CompactFunctionIndex,
    program: &ArenaProgram,
    function: &CompactFunctionDef,
) -> Vec<LoweredFunctionKey> {
    compact_function_call_edges(
        program,
        function.id,
        function.namespace,
        &functions.index_of,
    )
    .into_iter()
    .map(|index| functions.defs[index].key)
    .collect()
}

fn compact_collect_block_call_edges(
    program: &ArenaProgram,
    block: BlockId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    for stmt in program
        .arena
        .stmt_ids(program.arena.block(block).statements)
    {
        compact_collect_stmt_call_edges(program, stmt, namespace, index_of, edges);
    }
}

fn compact_collect_expr_or_run_call_edges(
    program: &ArenaProgram,
    value: ArenaExprOrRun,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    if let ArenaExprOrRun::Expr(expr) = value {
        compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
    }
}

fn compact_collect_stmt_call_edges(
    program: &ArenaProgram,
    id: StmtId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner)
        | ArenaStmtKind::Sugar {
            expansion: inner, ..
        } => compact_collect_stmt_call_edges(program, inner, namespace, index_of, edges),
        ArenaStmtKind::Let { initializer, .. }
        | ArenaStmtKind::Const { initializer, .. }
        | ArenaStmtKind::Var { initializer, .. }
        | ArenaStmtKind::Defer(initializer, _)
        | ArenaStmtKind::Yield(initializer) => {
            compact_collect_expr_or_run_call_edges(
                program,
                initializer,
                namespace,
                index_of,
                edges,
            );
        }
        ArenaStmtKind::Assign { value, .. } => {
            compact_collect_expr_or_run_call_edges(program, value, namespace, index_of, edges);
        }
        ArenaStmtKind::Return(Some(value)) => {
            compact_collect_expr_or_run_call_edges(program, value, namespace, index_of, edges);
        }
        ArenaStmtKind::If {
            branches,
            else_block,
        } => {
            for branch in program.arena.if_branches(branches) {
                compact_collect_expr_call_edges(
                    program,
                    branch.condition,
                    namespace,
                    index_of,
                    edges,
                );
                compact_collect_block_call_edges(program, branch.block, namespace, index_of, edges);
            }
            if let Some(block) = else_block {
                compact_collect_block_call_edges(program, block, namespace, index_of, edges);
            }
        }
        ArenaStmtKind::While { condition, block } => {
            compact_collect_expr_call_edges(program, condition, namespace, index_of, edges);
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaStmtKind::For { iter, block, .. } => {
            compact_collect_expr_call_edges(program, iter, namespace, index_of, edges);
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaStmtKind::Loop { block } => {
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaStmtKind::With {
            bindings,
            body,
            else_block,
        } => {
            for binding in program.arena.with_bindings(bindings) {
                compact_collect_expr_call_edges(
                    program,
                    binding.initializer,
                    namespace,
                    index_of,
                    edges,
                );
            }
            compact_collect_block_call_edges(program, body, namespace, index_of, edges);
            compact_collect_block_call_edges(program, else_block, namespace, index_of, edges);
        }
        ArenaStmtKind::Guard {
            initializer,
            else_block,
            ..
        } => {
            compact_collect_expr_or_run_call_edges(
                program,
                initializer,
                namespace,
                index_of,
                edges,
            );
            compact_collect_block_call_edges(program, else_block, namespace, index_of, edges);
        }
        ArenaStmtKind::Assert { condition, message } => {
            compact_collect_expr_call_edges(program, condition, namespace, index_of, edges);
            if let Some(message) = message {
                compact_collect_expr_call_edges(program, message, namespace, index_of, edges);
            }
        }
        ArenaStmtKind::Break { value: Some(value) }
        | ArenaStmtKind::Expr(value)
        | ArenaStmtKind::Exit(value)
        | ArenaStmtKind::YieldDelegate(value) => {
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
        }
        ArenaStmtKind::Match { value, arms } => {
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
            for arm in program.arena.match_arms(arms) {
                if let Some(guard) = arm.guard {
                    compact_collect_expr_call_edges(program, guard, namespace, index_of, edges);
                }
                compact_collect_block_call_edges(program, arm.block, namespace, index_of, edges);
            }
        }
        _ => {}
    }
}

fn compact_collect_expr_call_edges(
    program: &ArenaProgram,
    id: ExprId,
    namespace: Option<Name>,
    index_of: &FxHashMap<LoweredFunctionKey, usize>,
    edges: &mut Vec<usize>,
) {
    match program.arena.expr(id).kind {
        ArenaExprKind::Call { callee, args } => {
            if let ArenaExprKind::Ident(name) = program.arena.expr(callee).kind
                && let Some(index) = index_of.get(&compact_function_key(namespace, name))
            {
                edges.push(*index);
            }
            compact_collect_expr_call_edges(program, callee, namespace, index_of, edges);
            for arg in program.arena.call_args(args) {
                match arg.kind {
                    ArenaCallArgKind::Positional(expr)
                    | ArenaCallArgKind::Splice { value: expr, .. }
                    | ArenaCallArgKind::NamedSpread { value: expr, .. }
                    | ArenaCallArgKind::Named { value: expr, .. } => {
                        compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
                    }
                }
            }
        }
        ArenaExprKind::List(items) => {
            for item in program.arena.list_element_exprs(items) {
                compact_collect_expr_call_edges(program, item, namespace, index_of, edges);
            }
        }
        ArenaExprKind::Record(fields) => {
            for field in program.arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => {
                        compact_collect_expr_call_edges(program, key, namespace, index_of, edges);
                        compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
                    }
                    ArenaRecordFieldKind::Path { value, .. }
                    | ArenaRecordFieldKind::Named { value, .. }
                    | ArenaRecordFieldKind::Spread { expr: value, .. } => {
                        compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
                    }
                    ArenaRecordFieldKind::Shorthand { .. } => {}
                }
            }
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            for branch in program.arena.if_expr_branches(branches) {
                compact_collect_expr_call_edges(
                    program,
                    branch.condition,
                    namespace,
                    index_of,
                    edges,
                );
                compact_collect_expr_call_edges(program, branch.value, namespace, index_of, edges);
            }
            compact_collect_expr_call_edges(program, else_value, namespace, index_of, edges);
        }
        ArenaExprKind::Match { value, arms }
        | ArenaExprKind::PatternTest { value, arms }
        | ArenaExprKind::PatternCondition { value, arms } => {
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
            for arm in program.arena.match_expr_arms(arms) {
                if let Some(guard) = arm.guard {
                    compact_collect_expr_call_edges(program, guard, namespace, index_of, edges);
                }
                compact_collect_expr_call_edges(program, arm.value, namespace, index_of, edges);
            }
        }
        ArenaExprKind::Unary { expr, .. }
        | ArenaExprKind::Try(expr)
        | ArenaExprKind::Require { value: expr, .. }
        | ArenaExprKind::Convert { value: expr, .. }
        | ArenaExprKind::Field { base: expr, .. }
        | ArenaExprKind::NullSafeField { base: expr, .. } => {
            compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
        }
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            compact_collect_expr_call_edges(program, input, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, call, namespace, index_of, edges);
        }
        ArenaExprKind::ComparisonChain(pairs) => {
            for pair in program.arena.comparison_chain_operands(pairs) {
                compact_collect_expr_call_edges(program, pair, namespace, index_of, edges);
            }
        }
        ArenaExprKind::Binary { left, right, .. }
        | ArenaExprKind::Index {
            base: left,
            index: right,
            ..
        } => {
            compact_collect_expr_call_edges(program, left, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, right, namespace, index_of, edges);
        }
        ArenaExprKind::Slice {
            base, start, end, ..
        } => {
            compact_collect_expr_call_edges(program, base, namespace, index_of, edges);
            if let Some(start) = start {
                compact_collect_expr_call_edges(program, start, namespace, index_of, edges);
            }
            if let Some(end) = end {
                compact_collect_expr_call_edges(program, end, namespace, index_of, edges);
            }
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            for part in program.arena.fmt_parts(parts) {
                if let ArenaFmtPart::Expr(expr, _) = part {
                    compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
                }
            }
        }
        ArenaExprKind::Set(items) => {
            for item in program.arena.list_element_exprs(items) {
                compact_collect_expr_call_edges(program, item, namespace, index_of, edges);
            }
        }
        ArenaExprKind::ListComp { expr, qualifiers }
        | ArenaExprKind::SetComp { expr, qualifiers } => {
            for qualifier in program.arena.comp_qualifiers(qualifiers) {
                compact_collect_expr_call_edges(
                    program,
                    qualifier.expr(),
                    namespace,
                    index_of,
                    edges,
                );
            }
            compact_collect_expr_call_edges(program, expr, namespace, index_of, edges);
        }
        ArenaExprKind::MapComp {
            key,
            value,
            qualifiers,
        } => {
            for qualifier in program.arena.comp_qualifiers(qualifiers) {
                compact_collect_expr_call_edges(
                    program,
                    qualifier.expr(),
                    namespace,
                    index_of,
                    edges,
                );
            }
            compact_collect_expr_call_edges(program, key, namespace, index_of, edges);
            compact_collect_expr_call_edges(program, value, namespace, index_of, edges);
        }
        ArenaExprKind::ErrorContext { message, block }
        | ArenaExprKind::ContextScope {
            input: message,
            block,
            ..
        } => {
            compact_collect_expr_call_edges(program, message, namespace, index_of, edges);
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaExprKind::Capture(block)
        | ArenaExprKind::ValueBlock(block)
        | ArenaExprKind::Loop { block }
        | ArenaExprKind::Collect { block }
        | ArenaExprKind::Retry { block, .. }
        | ArenaExprKind::TempDirScope {
            path: None, block, ..
        } => {
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaExprKind::TempDirScope {
            path: Some(path),
            block,
            ..
        } => {
            compact_collect_expr_call_edges(program, path, namespace, index_of, edges);
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        ArenaExprKind::BuilderCall { call, .. } => {
            compact_collect_expr_call_edges(program, call, namespace, index_of, edges);
        }
        ArenaExprKind::ResourceScope {
            bindings, block, ..
        } => {
            for binding in program.arena.with_bindings(bindings) {
                compact_collect_expr_call_edges(
                    program,
                    binding.initializer,
                    namespace,
                    index_of,
                    edges,
                );
            }
            compact_collect_block_call_edges(program, block, namespace, index_of, edges);
        }
        _ => {}
    }
}

fn compact_tarjan_sccs(adjacency: Vec<Vec<usize>>) -> Vec<Vec<usize>> {
    struct Tarjan {
        adjacency: Vec<Vec<usize>>,
        index: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        next_index: usize,
        sccs: Vec<Vec<usize>>,
    }
    impl Tarjan {
        fn strongconnect(&mut self, v: usize) {
            self.index[v] = Some(self.next_index);
            self.lowlink[v] = self.next_index;
            self.next_index += 1;
            self.stack.push(v);
            self.on_stack[v] = true;
            for i in 0..self.adjacency[v].len() {
                let w = self.adjacency[v][i];
                match self.index[w] {
                    None => {
                        self.strongconnect(w);
                        self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
                    }
                    Some(w_index) if self.on_stack[w] => {
                        self.lowlink[v] = self.lowlink[v].min(w_index);
                    }
                    Some(_) => {}
                }
            }
            if self.lowlink[v] == self.index[v].expect("index set above") {
                let mut scc = Vec::new();
                loop {
                    let w = self.stack.pop().expect("stack non-empty");
                    self.on_stack[w] = false;
                    scc.push(w);
                    if w == v {
                        break;
                    }
                }
                self.sccs.push(scc);
            }
        }
    }
    let mut tarjan = Tarjan {
        index: vec![None; adjacency.len()],
        lowlink: vec![0; adjacency.len()],
        on_stack: vec![false; adjacency.len()],
        stack: Vec::new(),
        next_index: 0,
        sccs: Vec::new(),
        adjacency,
    };
    for v in 0..tarjan.adjacency.len() {
        if tarjan.index[v].is_none() {
            tarjan.strongconnect(v);
        }
    }
    tarjan.sccs
}

fn compact_function_key(namespace: Option<Name>, name: Name) -> LoweredFunctionKey {
    match namespace {
        Some(namespace) => LoweredFunctionKey::Qualified(QualifiedName::new(namespace, name)),
        None => LoweredFunctionKey::Name(name),
    }
}

fn compact_function_defs(program: &ArenaProgram) -> Vec<CompactFunctionDef> {
    let mut functions = Vec::new();
    for stmt in program.statement_ids() {
        collect_compact_function_def(program, stmt, None, &mut functions);
    }
    for module in &program.modules {
        for stmt in program.module_statements(module) {
            collect_compact_function_def(program, stmt, Some(module.name), &mut functions);
        }
    }
    functions
}

pub(super) fn compact_function_keys(program: &ArenaProgram) -> Vec<LoweredFunctionKey> {
    compact_function_defs(program)
        .into_iter()
        .map(|function| function.key)
        .collect()
}

fn collect_compact_function_def(
    program: &ArenaProgram,
    id: StmtId,
    namespace: Option<Name>,
    functions: &mut Vec<CompactFunctionDef>,
) {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner) => {
            collect_compact_function_def(program, inner, namespace, functions);
        }
        ArenaStmtKind::PureDef(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: true,
                namespace,
                definition_span: program.arena.stmt(id).span,
            });
        }
        ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: false,
                namespace,
                definition_span: program.arena.stmt(id).span,
            });
        }
        ArenaStmtKind::StreamDef(def) => {
            let name = program.arena.function_def(def).name;
            functions.push(CompactFunctionDef {
                key: compact_function_key(namespace, name),
                id: def,
                pure: true,
                namespace,
                definition_span: program.arena.stmt(id).span,
            });
        }
        _ => {}
    }
}

struct CompactLowerConstructProbe<'a, 'defs> {
    program: &'a ArenaProgram,
    declarations: &'a CompactDeclOutput,
    bodies: &'a CompactBodyFacts,
    source: &'a str,
    sources: Option<&'a SourceMap>,
    current_namespace: Option<Name>,
    functions: Option<&'a LowerableFunctions<'defs>>,
    top_level_known: FxHashMap<Name, LoweredTopLevelBinding>,
    output: CompactLowerConstructProbeOutput,
    last_blocker_detail: Option<(Span, String)>,
    /// Where standard-library implementation calls in this program resolve.
    stdlib_linkage: StdlibLowerLinkage,
    /// The program's function definitions and key index, built once and shared
    /// by every unit lowered in one pass.
    ///
    /// `compact_function_defs` walks every statement of every module, so
    /// rebuilding it per function makes preparation quadratic in program size.
    function_defs: Rc<RefCell<Option<Rc<CompactFunctionIndex>>>>,
    scratch: Rc<RefCell<BuildScratch>>,
    /// A copy of `program` and `bodies` that named-spread calls append their
    /// argument projections to, shared by every probe over the same two.
    ///
    /// The projections of a call are synthetic nodes, which need a program
    /// that can grow. Copying the program for each call is work proportional
    /// to the whole program per call; this copy is made for the first call of
    /// a pass and extended by the later ones.
    spread_programs: Rc<RefCell<Option<Box<SpreadPrograms>>>>,
}

/// A program and its body facts with the argument projections of
/// named-spread calls appended. Nothing is removed or renumbered, so every
/// node of the original has the same identity here.
struct SpreadPrograms {
    program: ArenaProgram,
    bodies: CompactBodyFacts,
}

#[cfg(test)]
thread_local! {
    /// How many times this thread copied a whole program for a named-spread
    /// call.
    static SPREAD_PROGRAM_COPIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[derive(Clone, Copy, Debug)]
enum CompactTopLevelBlocker {
    Use,
    BindingTarget,
    BindingType,
    BindingExpression,
    AssignTarget,
    AssignExpression,
    Control,
    Command,
    Expression,
    Defer,
    Other,
}

impl CompactTopLevelBlocker {
    fn index(self) -> usize {
        match self {
            Self::Use => 0,
            Self::BindingTarget => 1,
            Self::BindingType => 2,
            Self::BindingExpression => 3,
            Self::AssignTarget => 4,
            Self::AssignExpression => 5,
            Self::Control => 6,
            Self::Command => 7,
            Self::Expression => 8,
            Self::Defer => 9,
            Self::Other => 10,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Use => "use",
            Self::BindingTarget => "binding_target",
            Self::BindingType => "binding_type",
            Self::BindingExpression => "binding_expression",
            Self::AssignTarget => "assign_target",
            Self::AssignExpression => "assign_expression",
            Self::Control => "control",
            Self::Command => "command",
            Self::Expression => "expression",
            Self::Defer => "defer",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CompactFunctionBlocker {
    ReturnType,
    ParamDefault,
    ParamType,
    BlockParams,
    Body,
    NoReturn,
}

impl CompactFunctionBlocker {
    fn index(self) -> usize {
        match self {
            Self::ReturnType => 0,
            Self::ParamDefault => 1,
            Self::ParamType => 2,
            Self::BlockParams => 3,
            Self::Body => 4,
            Self::NoReturn => 5,
        }
    }
}

impl From<CompactFunctionBlocker> for LoweredFunctionBlocker {
    fn from(value: CompactFunctionBlocker) -> Self {
        match value {
            CompactFunctionBlocker::ReturnType => Self::ReturnType,
            CompactFunctionBlocker::ParamDefault => Self::ParamDefault,
            CompactFunctionBlocker::ParamType => Self::ParamType,
            CompactFunctionBlocker::BlockParams => Self::BlockParams,
            CompactFunctionBlocker::Body => Self::Body,
            CompactFunctionBlocker::NoReturn => Self::NoReturn,
        }
    }
}

impl From<LoweredFunctionBlocker> for CompactFunctionBlocker {
    fn from(value: LoweredFunctionBlocker) -> Self {
        match value {
            LoweredFunctionBlocker::ReturnType => Self::ReturnType,
            LoweredFunctionBlocker::ParamDefault => Self::ParamDefault,
            LoweredFunctionBlocker::ParamType => Self::ParamType,
            LoweredFunctionBlocker::BlockParams => Self::BlockParams,
            LoweredFunctionBlocker::Body => Self::Body,
            LoweredFunctionBlocker::NoReturn => Self::NoReturn,
        }
    }
}

fn compact_type_expr_tag_index(tag: ArenaTypeExprTag) -> usize {
    match tag {
        ArenaTypeExprTag::Applied => 8,
        ArenaTypeExprTag::Named => 0,
        ArenaTypeExprTag::Qualified => 1,
        ArenaTypeExprTag::List => 2,
        ArenaTypeExprTag::Map => 3,
        ArenaTypeExprTag::Stream => 4,
        ArenaTypeExprTag::Module => 5,
        ArenaTypeExprTag::Result => 6,
        ArenaTypeExprTag::Optional => 7,
        ArenaTypeExprTag::Union => 9,
        ArenaTypeExprTag::Callable => 10,
        ArenaTypeExprTag::NonEmpty => 11,
        ArenaTypeExprTag::Set => 12,
    }
}

// Indexes 20 and 27 are unassigned: they counted the guarded statement and
// the boolean guard, which now lower as the `if` they expand to.
fn compact_stmt_kind_index(kind: ArenaStmtKind) -> usize {
    match kind {
        ArenaStmtKind::Use(_) => 0,
        ArenaStmtKind::Export(_) => 1,
        ArenaStmtKind::TypeDef(_) => 2,
        ArenaStmtKind::ErrorDef(_) => 3,
        ArenaStmtKind::Let { .. } | ArenaStmtKind::Const { .. } => 4,
        ArenaStmtKind::Var { .. } => 5,
        ArenaStmtKind::Assign { .. } => 6,
        ArenaStmtKind::ProcDef(_) => 7,
        ArenaStmtKind::CliMain(_) => 29,
        ArenaStmtKind::PureDef(_) => 8,
        ArenaStmtKind::StreamDef(_) => 9,
        ArenaStmtKind::SignalHook(_) => 10,
        ArenaStmtKind::Return(_) => 11,
        ArenaStmtKind::YieldDelegate(_) => 12,
        ArenaStmtKind::Yield(_) => 12,
        ArenaStmtKind::Defer(..) => 13,
        ArenaStmtKind::If { .. } => 14,
        ArenaStmtKind::While { .. } => 15,
        ArenaStmtKind::For { .. } => 16,
        ArenaStmtKind::With { .. } => 17,
        ArenaStmtKind::Loop { .. } => 18,
        ArenaStmtKind::Guard { .. } => 19,
        ArenaStmtKind::Break { .. } => 21,
        ArenaStmtKind::Continue => 22,
        ArenaStmtKind::Match { .. } => 23,
        ArenaStmtKind::Command(_) => 24,
        ArenaStmtKind::TailBareIdent(_) => 25,
        ArenaStmtKind::Expr(_) | ArenaStmtKind::Exit(_) => 26,
        ArenaStmtKind::Sugar { .. } => {
            unreachable!("a sugar statement is classified by its expansion")
        }
        ArenaStmtKind::Assert { .. } => 28,
    }
}

fn compact_stmt_kind_label(kind: ArenaStmtKind) -> &'static str {
    match kind {
        ArenaStmtKind::Use(_) => "use",
        ArenaStmtKind::Export(_) => "export",
        ArenaStmtKind::TypeDef(_) => "type_def",
        ArenaStmtKind::ErrorDef(_) => "error_def",
        ArenaStmtKind::Let { .. } | ArenaStmtKind::Const { .. } => "let",
        ArenaStmtKind::Var { .. } => "var",
        ArenaStmtKind::Assign { .. } => "assign",
        ArenaStmtKind::ProcDef(_) => "proc_def",
        ArenaStmtKind::CliMain(_) => "cli_main",
        ArenaStmtKind::PureDef(_) => "pure_def",
        ArenaStmtKind::StreamDef(_) => "stream_def",
        ArenaStmtKind::SignalHook(_) => "signal_hook",
        ArenaStmtKind::Return(_) => "return",
        ArenaStmtKind::YieldDelegate(_) => "yield-delegate",
        ArenaStmtKind::Yield(_) => "yield",
        ArenaStmtKind::Defer(..) => "defer",
        ArenaStmtKind::If { .. } => "if",
        ArenaStmtKind::While { .. } => "while",
        ArenaStmtKind::For { .. } => "for",
        ArenaStmtKind::With { .. } => "with",
        ArenaStmtKind::Loop { .. } => "loop",
        ArenaStmtKind::Guard { .. } => "guard",
        ArenaStmtKind::Break { .. } => "break",
        ArenaStmtKind::Continue => "continue",
        ArenaStmtKind::Match { .. } => "match",
        ArenaStmtKind::Command(_) => "command",
        ArenaStmtKind::TailBareIdent(_) => "tail_bare_ident",
        ArenaStmtKind::Expr(_) | ArenaStmtKind::Exit(_) => "expr",
        ArenaStmtKind::Sugar { .. } => {
            unreachable!("a sugar statement is classified by its expansion")
        }
        ArenaStmtKind::Assert { .. } => "assert",
    }
}

fn compact_stmt_blocker_label(program: &ArenaProgram, stmt: StmtId) -> String {
    match program.arena.stmt(stmt).kind {
        ArenaStmtKind::Sugar { expansion, .. } => compact_stmt_blocker_label(program, expansion),
        ArenaStmtKind::Command(command) => {
            format!(
                "command:{}",
                compact_command_blocker_label(compact_command_blocker_index(program, command))
            )
        }
        kind => compact_stmt_kind_label(kind).to_string(),
    }
}

fn compact_expr_kind_index(kind: ArenaExprKind) -> usize {
    match kind {
        ArenaExprKind::Null => 0,
        ArenaExprKind::Bool(_) => 1,
        ArenaExprKind::Int(_) => 2,
        ArenaExprKind::Float(_) => 3,
        ArenaExprKind::Duration(_) => 4,
        ArenaExprKind::Str(_) => 5,
        ArenaExprKind::PathStr(_) => 6,
        ArenaExprKind::GlobStr(_) => 7,
        ArenaExprKind::FmtString(_) => 8,
        ArenaExprKind::PathFmtString(_) => 9,
        ArenaExprKind::Bytes(_) => 10,
        ArenaExprKind::Ident(_) => 11,
        ArenaExprKind::Item => 12,
        ArenaExprKind::LastStatus => 13,
        ArenaExprKind::List(_) => 14,
        ArenaExprKind::ListComp { .. } => 15,
        ArenaExprKind::MapComp { .. } => 16,
        ArenaExprKind::Record(_) => 17,
        ArenaExprKind::If { .. } => 18,
        ArenaExprKind::Match { .. }
        | ArenaExprKind::PatternTest { .. }
        | ArenaExprKind::PatternCondition { .. } => 19,
        ArenaExprKind::Unary { .. } => 20,
        ArenaExprKind::ComparisonChain(_) => 40,
        ArenaExprKind::Binary { .. } => 21,
        ArenaExprKind::Call { .. } => 22,
        ArenaExprKind::Field { .. } => 23,
        ArenaExprKind::NullSafeField { .. } => 24,
        ArenaExprKind::Index { .. } => 25,
        ArenaExprKind::Slice { .. } => 26,
        ArenaExprKind::EnvString(_) => 27,
        ArenaExprKind::EnvPathList => 28,
        ArenaExprKind::Pipeline { .. } => 29,
        ArenaExprKind::StructuredPipeline { .. } => 30,
        ArenaExprKind::Run(_) => 31,
        ArenaExprKind::Spawn(_) => 32,
        ArenaExprKind::Wait(_) => 33,
        ArenaExprKind::BuilderCall { .. } => 34,
        ArenaExprKind::Try(_) => 35,
        ArenaExprKind::Require { .. } => 36,
        ArenaExprKind::Loop { .. } => 37,
        ArenaExprKind::Capture(_) => 42,
        ArenaExprKind::Retry { .. } => 38,
        ArenaExprKind::ValueBlock(_) => 39,
        ArenaExprKind::ErrorContext { .. } => 44,
        ArenaExprKind::Regex(_) => 41,
        ArenaExprKind::ValuePipelineCall { .. } => 43,
        ArenaExprKind::ContextScope { .. } => 45,
        ArenaExprKind::TempDirScope { .. } => 46,
        ArenaExprKind::Convert { .. } => 47,
        ArenaExprKind::Set(_) => 48,
        ArenaExprKind::SetComp { .. } => 49,
        ArenaExprKind::Collect { .. } => 50,
        ArenaExprKind::ResourceScope { .. } => 51,
    }
}

/// The local that holds the list of one `collect` expression. No identifier
/// can spell it, and each expression has its own, so a nested `collect`
/// neither sees nor shadows the list around it.
fn collect_local(collect: ExprId) -> Name {
    Name::intern(&format!("%collect{}", collect.index()))
}

fn compact_expr_kind_label(kind: ArenaExprKind) -> &'static str {
    match kind {
        ArenaExprKind::Null => "null",
        ArenaExprKind::Bool(_) => "bool",
        ArenaExprKind::Int(_) => "int",
        ArenaExprKind::Float(_) => "float",
        ArenaExprKind::Duration(_) => "duration",
        ArenaExprKind::Str(_) => "str",
        ArenaExprKind::PathStr(_) => "path_str",
        ArenaExprKind::GlobStr(_) => "glob_str",
        ArenaExprKind::FmtString(_) => "fmt_string",
        ArenaExprKind::PathFmtString(_) => "path_fmt_string",
        ArenaExprKind::Bytes(_) => "bytes",
        ArenaExprKind::Ident(_) => "ident",
        ArenaExprKind::Item => "item",
        ArenaExprKind::LastStatus => "last_status",
        ArenaExprKind::List(_) => "list",
        ArenaExprKind::ListComp { .. } => "list_comp",
        ArenaExprKind::MapComp { .. } => "map_comp",
        ArenaExprKind::Set(_) => "set",
        ArenaExprKind::SetComp { .. } => "set_comp",
        ArenaExprKind::Record(_) => "record",
        ArenaExprKind::If { .. } => "if",
        ArenaExprKind::Match { .. } => "match",
        ArenaExprKind::PatternTest { .. } => "pattern_test",
        ArenaExprKind::PatternCondition { .. } => "pattern_condition",
        ArenaExprKind::Unary { .. } => "unary",
        ArenaExprKind::ComparisonChain(_) => "comparison-chain",
        ArenaExprKind::Binary { .. } => "binary",
        ArenaExprKind::Call { .. } => "call",
        ArenaExprKind::Field { .. } => "field",
        ArenaExprKind::NullSafeField { .. } => "null_safe_field",
        ArenaExprKind::Index { .. } => "index",
        ArenaExprKind::Slice { .. } => "slice",
        ArenaExprKind::EnvString(_) => "env_string",
        ArenaExprKind::EnvPathList => "env_path_list",
        ArenaExprKind::Pipeline { .. } => "pipeline",
        ArenaExprKind::StructuredPipeline { .. } => "structured_pipeline",
        ArenaExprKind::Run(_) => "run",
        ArenaExprKind::Spawn(_) => "spawn",
        ArenaExprKind::Wait(_) => "wait",
        ArenaExprKind::BuilderCall { .. } => "builder_call",
        ArenaExprKind::Try(_) => "try",
        ArenaExprKind::Require { .. } => "require",
        ArenaExprKind::Loop { .. } => "loop",
        ArenaExprKind::Capture(_) => "try",
        ArenaExprKind::Retry { .. } => "retry",
        ArenaExprKind::ValueBlock(_) => "value_block",
        ArenaExprKind::ErrorContext { .. } => "error_context",
        ArenaExprKind::Regex(_) => "regex_literal",
        ArenaExprKind::ValuePipelineCall { .. } => "value_pipeline_call",
        ArenaExprKind::ContextScope { .. } => "context_scope",
        ArenaExprKind::TempDirScope { .. } => "tempdir_scope",
        ArenaExprKind::Convert { .. } => "convert",
        ArenaExprKind::Collect { .. } => "collect",
        ArenaExprKind::ResourceScope { .. } => "resource_scope",
    }
}

fn compact_checked_type_is_concrete(ty: &Type) -> bool {
    !matches!(ty, Type::Any | Type::Unknown | Type::Invalid)
        && !ty.contains_any()
        && !ty.contains_inference()
}

/// Disagreements between a representation lowering derives itself and the
/// checker's published type for the same binding, recorded by debug builds.
#[cfg(debug_assertions)]
pub(crate) static LOWERING_DRIFT: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Lowering agrees with the checker up to representation erasure: lowering may
/// keep `Any`, a nullable value erases to its present kind, `UInt` shares the
/// `Int` representation, and an unresolved checker fact makes no claim.
#[cfg(debug_assertions)]
fn note_lowering_drift(
    span: Span,
    kind: Option<LoweredType>,
    lowered: &Type,
    checked: Option<&Type>,
) {
    let Some(checked) = checked.filter(|ty| checked_fact_is_resolved(ty)) else {
        return;
    };
    let present = checked.optional_inner().unwrap_or(checked);
    let kind_agrees = match (kind, lowered_checked_type(present)) {
        (None | Some(LoweredType::Any), _) | (_, None) => true,
        (Some(kind), Some(expected)) => kind == expected,
    };
    if !kind_agrees || lowered != checked {
        LOWERING_DRIFT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(format!(
                "{span:?}: lowering {kind:?} {lowered}, checker {checked}"
            ));
    }
}

fn concrete_checked_fact(ty: &Type) -> bool {
    compact_checked_type_is_concrete(ty)
        && !matches!(
            ty.result_ok(),
            Some(Type::Any | Type::Unknown | Type::Invalid)
        )
}

fn compact_call_blocker_index(program: &ArenaProgram, callee: ExprId) -> usize {
    match program.arena.expr(callee).kind {
        ArenaExprKind::Ident(_) => 0,
        ArenaExprKind::Field { base, .. } => {
            if matches!(program.arena.expr(base).kind, ArenaExprKind::Ident(_)) {
                1
            } else {
                2
            }
        }
        ArenaExprKind::NullSafeField { base, .. } => {
            if matches!(program.arena.expr(base).kind, ArenaExprKind::Ident(_)) {
                3
            } else {
                4
            }
        }
        _ => 5,
    }
}

fn compact_call_blocker_label(program: &ArenaProgram, callee: ExprId) -> Option<String> {
    match program.arena.expr(callee).kind {
        ArenaExprKind::Ident(name) => Some(name.as_str().to_string()),
        ArenaExprKind::Field { base, name } => match program.arena.expr(base).kind {
            ArenaExprKind::Ident(module) => Some(format!("{}.{}", module.as_str(), name.as_str())),
            _ => Some(format!("<field>.{}", name.as_str())),
        },
        ArenaExprKind::NullSafeField { base, name } => match program.arena.expr(base).kind {
            ArenaExprKind::Ident(module) => Some(format!("{}?.{}", module.as_str(), name.as_str())),
            _ => Some(format!("<null-safe-field>.{}", name.as_str())),
        },
        _ => None,
    }
}

fn record_compact_call_blocker_label(
    counts: &mut BTreeMap<String, u32>,
    program: &ArenaProgram,
    callee: ExprId,
) {
    if let Some(label) = compact_call_blocker_label(program, callee) {
        *counts.entry(label).or_insert(0) += 1;
    }
}

fn record_compact_call_blocker_span(
    samples: &mut BTreeMap<String, Vec<Span>>,
    program: &ArenaProgram,
    callee: ExprId,
) {
    let Some(label) = compact_call_blocker_label(program, callee) else {
        return;
    };
    let samples = samples.entry(label).or_default();
    if samples.len() < 8 {
        samples.push(program.arena.expr(callee).span);
    }
}

fn record_compact_stmt_blocker_span(
    samples: &mut BTreeMap<String, Vec<Span>>,
    program: &ArenaProgram,
    stmt: StmtId,
) {
    let label = compact_stmt_blocker_label(program, stmt);
    let samples = samples.entry(label).or_default();
    if samples.len() < 8 {
        samples.push(program.arena.stmt(stmt).span);
    }
}

fn compact_error_family_name(program: &ArenaProgram, id: ExprId) -> Option<Name> {
    match program.arena.expr(id).kind {
        ArenaExprKind::Ident(name) => Some(name),
        ArenaExprKind::Field { base, name } => {
            let base = compact_error_family_name(program, base)?;
            Some(Name::intern(format!("{base}.{name}")))
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum CompactErrorFamilyKey {
    Local(Name),
    Qualified(QualifiedName),
}

fn compact_error_family_key(program: &ArenaProgram, id: ExprId) -> Option<CompactErrorFamilyKey> {
    match program.arena.expr(id).kind {
        ArenaExprKind::Ident(name) => Some(CompactErrorFamilyKey::Local(name)),
        ArenaExprKind::Field { base, name } => match program.arena.expr(base).kind {
            ArenaExprKind::Ident(namespace) => Some(CompactErrorFamilyKey::Qualified(
                QualifiedName::new(namespace, name),
            )),
            _ => compact_error_family_name(program, id).map(CompactErrorFamilyKey::Local),
        },
        _ => None,
    }
}

fn compact_error_family_display(key: CompactErrorFamilyKey) -> String {
    match key {
        CompactErrorFamilyKey::Local(name) => name.to_string(),
        // An error carries its family's declared name wherever it is built.
        CompactErrorFamilyKey::Qualified(name) => name.member.to_string(),
    }
}

fn compact_error_family_info(
    declarations: &crate::sema::check::CompactDeclOutput,
    key: CompactErrorFamilyKey,
) -> Option<&crate::sema::check::ErrorFamilyInfo> {
    match key {
        CompactErrorFamilyKey::Local(name) => declarations.error_families_by_name.get(&name),
        CompactErrorFamilyKey::Qualified(name) => declarations.qualified_error_families.get(&name),
    }
}

fn compact_expr_call_blocker_callee(program: &ArenaProgram, expr: ExprId) -> Option<ExprId> {
    match program.arena.expr(expr).kind {
        ArenaExprKind::Call { callee, .. } => Some(callee),
        ArenaExprKind::Try(inner) => match program.arena.expr(inner).kind {
            ArenaExprKind::Call { callee, .. } => Some(callee),
            _ => None,
        },
        _ => None,
    }
}

fn print_flush_arg(
    program: &ArenaProgram,
    source: &str,
    sources: Option<&SourceMap>,
    arg: &crate::syntax::arena::ArenaCommandArg,
) -> bool {
    let arg_span = program.arena.span(arg.span);
    if sources
        .and_then(|sources| sources.span_text(arg_span))
        .is_some_and(|value| value == "--flush")
    {
        return true;
    }

    let ArenaCommandArgKind::Word(parts) = &arg.kind else {
        return false;
    };
    let parts = program.arena.word_parts(*parts).collect::<Vec<_>>();
    let [ArenaWordPart::Bare(text)] = parts.as_slice() else {
        return false;
    };
    let value = program.arena.text_value(text, source);

    value == Some("--flush")
}

fn compact_command_blocker_index(
    program: &ArenaProgram,
    command: crate::syntax::arena::CommandStmtId,
) -> usize {
    match program.arena.command_stmt(command).command {
        ArenaCommand::Proc { .. } => 0,
        ArenaCommand::Core {
            name: CoreCommand::Print,
            ..
        } => 1,
        ArenaCommand::Core {
            name: CoreCommand::Eprint,
            ..
        } => 2,
        ArenaCommand::Core {
            name: CoreCommand::Cd,
            ..
        } => 3,
        ArenaCommand::Core {
            name: CoreCommand::Env,
            ..
        } => 4,
        ArenaCommand::Run(_) => 5,
    }
}

fn compact_command_blocker_label(index: usize) -> &'static str {
    match index {
        0 => "proc",
        1 => "core_print",
        2 => "core_eprint",
        3 => "core_cd",
        4 => "core_env",
        5 => "run",
        _ => "unknown",
    }
}

fn compact_use_import_namespace(
    program: &ArenaProgram,
    use_id: crate::syntax::arena::UseStmtId,
) -> Option<Name> {
    let use_stmt = program.arena.use_stmt(use_id);
    use_stmt
        .alias
        .or_else(|| program.arena.names(use_stmt.path).last())
}

fn compact_module_exports_for_use(
    program: &ArenaProgram,
    key: &str,
    _namespace: Name,
    _functions: Option<&LowerableFunctions<'_>>,
) -> Option<Vec<LoweredModuleExport>> {
    let module = program
        .modules
        .iter()
        .find(|module| module.key.as_str() == key)?;
    let function_namespace = module.name;
    let mut exports = Vec::new();
    for stmt in program.module_statements(module) {
        let ArenaStmtKind::Export(inner) = program.arena.stmt(stmt).kind else {
            continue;
        };
        match program.arena.stmt(inner).kind {
            ArenaStmtKind::Let { target, .. }
            | ArenaStmtKind::Const { target, .. }
            | ArenaStmtKind::Var { target, .. } => {
                let ArenaBindingTargetKind::Name(name) = program.arena.binding_target(target).kind
                else {
                    return None;
                };
                exports.push(LoweredModuleExport {
                    name,
                    kind: LoweredModuleExportKind::Value,
                    function_namespace: None,
                });
            }
            ArenaStmtKind::ProcDef(def) => {
                let name = program.arena.function_def(def).name;
                exports.push(LoweredModuleExport {
                    name,
                    kind: LoweredModuleExportKind::Proc,
                    function_namespace: Some(function_namespace),
                });
            }
            ArenaStmtKind::PureDef(def) => {
                let name = program.arena.function_def(def).name;
                exports.push(LoweredModuleExport {
                    name,
                    kind: LoweredModuleExportKind::Pure,
                    function_namespace: Some(function_namespace),
                });
            }
            ArenaStmtKind::StreamDef(_)
            | ArenaStmtKind::TypeDef(_)
            | ArenaStmtKind::ErrorDef(_) => {}
            _ => return None,
        }
    }
    Some(exports)
}

pub(super) fn lower_literal_constant(
    value: &crate::sema::constants::LiteralConstant,
    enums: Option<&crate::sema::wire_enums::PreparedWireEnums>,
) -> Option<LoweredValue> {
    use crate::sema::constants::LiteralConstant as C;
    Some(match value {
        C::Regex(literal) => LoweredValue::Regex(Box::new(RegexValue {
            pattern: literal.pattern.to_string(),
            regex: literal.prepared.get()?.as_ref().ok()?.clone(),
        })),
        C::Tag {
            family,
            variant,
            fields,
        } => LoweredValue::Tag(Box::new(super::LoweredTagValue {
            type_name: *family,
            wire: enums.and_then(|enums| enums.mappings.get(family)).cloned(),
            name: Arc::from(variant.as_str().as_str()),
            fields: fields
                .iter()
                .map(|value| lower_literal_constant(value, enums))
                .collect::<Option<Vec<_>>>()?,
        })),
        C::Null => LoweredValue::Null,
        C::Bool(value) => LoweredValue::Bool(*value),
        C::Int(value) => LoweredValue::Int(*value),
        C::Float(value) => LoweredValue::Float(crate::runtime::value::FloatValue::new(
            f64::from_bits(*value),
        )),
        C::Duration(millis) => LoweredValue::Duration(DurationValue { millis: *millis }),
        C::Str(value) => LoweredValue::Str(value.clone()),
        C::Bytes(value) => LoweredValue::Bytes(value.clone()),
        C::Path(value) => LoweredValue::Path(PathValue::from_text(value).ok()?),
        C::EmptyMap => LoweredValue::Map(Arc::new(BTreeMap::new())),
        C::Map(values) => LoweredValue::Map(Arc::new(
            values
                .iter()
                .map(|(key, value)| Some((key.clone(), lower_literal_constant(value, enums)?)))
                .collect::<Option<BTreeMap<_, _>>>()?,
        )),
        C::List(values) => LoweredValue::SharedList(Arc::new(
            values
                .iter()
                .map(|value| lower_literal_constant(value, enums))
                .collect::<Option<Vec<_>>>()?,
        )),
        C::Set(values) => LoweredValue::Set(values.clone()),
        C::Record(values) => LoweredValue::Record(Arc::new(
            values
                .iter()
                .map(|(name, value)| {
                    Some((
                        Arc::<str>::from(name.as_str().as_str()),
                        lower_literal_constant(value, enums)?,
                    ))
                })
                .collect::<Option<BTreeMap<_, _>>>()?,
        )),
    })
}

fn lower_const_param_default(
    arena: &AstArena,
    expr: ExprId,
    kind: LoweredType,
    expected: Option<&Type>,
) -> Option<LoweredValue> {
    if let ArenaExprKind::Record(fields) = arena.expr(expr).kind
        && (matches!(expected, Some(Type::Map(_, _)))
            || arena
                .record_fields(fields)
                .iter()
                .any(|field| matches!(field.kind, ArenaRecordFieldKind::Computed { .. })))
    {
        let element = match expected {
            Some(Type::Map(_, item)) => Some(item.as_ref()),
            _ => None,
        };
        let mut values = BTreeMap::new();
        for field in arena.record_fields(fields) {
            match field.kind {
                ArenaRecordFieldKind::Computed { key, value, .. } => {
                    let key_type = match expected {
                        Some(Type::Map(key, _)) => Some(key.as_ref()),
                        _ => None,
                    };
                    let key = lower_const_param_default(arena, key, LoweredType::Any, key_type)?;
                    let key =
                        super::lowered_ops::lowered_map_literal_key(&key, arena.expr(expr).span)
                            .ok()?;
                    values.insert(
                        key,
                        lower_const_param_default(arena, value, LoweredType::Any, element)?,
                    );
                }
                ArenaRecordFieldKind::Named { name, value, .. } => {
                    values.insert(
                        crate::map_key::MapKey::from(name.as_str().as_str()),
                        lower_const_param_default(arena, value, LoweredType::Any, element)?,
                    );
                }
                ArenaRecordFieldKind::Spread { expr, .. } => {
                    let LoweredValue::Map(spread) =
                        lower_const_param_default(arena, expr, LoweredType::Map, expected)?
                    else {
                        return None;
                    };
                    values.extend(
                        spread
                            .iter()
                            .map(|(key, value)| (key.clone(), value.clone())),
                    );
                }
                ArenaRecordFieldKind::Shorthand { .. } | ArenaRecordFieldKind::Path { .. } => {
                    return None;
                }
            }
        }
        let value = LoweredValue::Map(Arc::new(values));
        return lowered_value_matches(kind, &value).then_some(value);
    }
    if let Some(constant) =
        crate::sema::constants::LiteralConstant::analyze(arena, expr, &FxHashMap::default())
    {
        let constant = if let Some(expected) = expected {
            constant.in_type(expected)
        } else {
            constant
        };
        let value = lower_literal_constant(&constant, None)?;
        return lowered_value_matches(kind, &value).then_some(value);
    }
    let value = match arena.expr(expr).kind {
        ArenaExprKind::Null => LoweredValue::Null,
        ArenaExprKind::Bool(value) => LoweredValue::Bool(value),
        ArenaExprKind::Int(value) => LoweredValue::Int(arena.int_literal(value).value()?),
        ArenaExprKind::Float(value) => LoweredValue::Float(crate::runtime::value::FloatValue::new(
            arena.float_literal(value).value()?,
        )),
        ArenaExprKind::Duration(value) => LoweredValue::Duration(DurationValue {
            millis: arena.duration_literal(value).millis()?,
        }),
        ArenaExprKind::Regex(value) => {
            let literal = arena.regex_literal(value);
            let regex = literal.prepared.get()?.as_ref().ok()?.clone();
            LoweredValue::Regex(Box::new(RegexValue {
                pattern: literal.pattern.to_string(),
                regex,
            }))
        }
        ArenaExprKind::Str(value) => LoweredValue::Str(arena.string_literal(value).clone()),
        ArenaExprKind::PathStr(value) => {
            LoweredValue::Path(PathValue::from_text(arena.string_literal(value).as_ref()).ok()?)
        }
        ArenaExprKind::Call { callee, args }
            if kind == LoweredType::Path
                && matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "Path") =>
        {
            let value = single_positional_arena_call_arg(arena.call_args(args))?;
            let ArenaExprKind::Str(value) = arena.expr(value).kind else {
                return None;
            };
            LoweredValue::Path(PathValue::from_text(arena.string_literal(value).as_ref()).ok()?)
        }
        ArenaExprKind::Bytes(value) => LoweredValue::Bytes(arena.bytes_literal(value).clone()),
        ArenaExprKind::List(items) => {
            let mut values = Vec::new();
            for item in arena.list_elements(items) {
                let value = lower_const_param_default(arena, item.value, LoweredType::Any, None)?;
                if item.splice_span.is_some() {
                    match value {
                        LoweredValue::List(items) => values.extend(items),
                        LoweredValue::SharedList(items) => values.extend(items.iter().cloned()),
                        _ => return None,
                    }
                } else {
                    values.push(value);
                }
            }
            LoweredValue::List(values)
        }
        ArenaExprKind::Record(fields) => {
            let mut values = BTreeMap::new();
            for field in arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Computed { .. } => return None,
                    ArenaRecordFieldKind::Named { name, value, .. } => {
                        values.insert(
                            Arc::<str>::from(name.as_str().as_str()),
                            lower_const_param_default(arena, value, LoweredType::Any, None)?,
                        );
                    }
                    ArenaRecordFieldKind::Spread { expr, .. } => {
                        let spread =
                            lower_const_param_default(arena, expr, LoweredType::Any, None)?;
                        match spread {
                            LoweredValue::Record(spread) => values.extend(
                                spread
                                    .iter()
                                    .map(|(key, value)| (key.clone(), value.clone())),
                            ),
                            LoweredValue::RecordVec(spread) => {
                                for (name, value) in spread.iter() {
                                    values.insert(
                                        Arc::<str>::from(name.as_str().as_str()),
                                        value.clone(),
                                    );
                                }
                            }
                            _ => return None,
                        }
                    }
                    ArenaRecordFieldKind::Shorthand { .. } | ArenaRecordFieldKind::Path { .. } => {
                        return None;
                    }
                }
            }
            LoweredValue::Record(Arc::new(values))
        }
        _ => return None,
    };
    lowered_value_matches(kind, &value).then_some(value)
}

fn compact_body_tail_stmt_kind(program: &ArenaProgram, block: BlockId) -> usize {
    program
        .arena
        .stmt_ids(program.arena.block(block).statements)
        .last()
        .map(|stmt| {
            compact_stmt_kind_index(program.arena.stmt(program.arena.core_stmt_id(stmt)).kind)
        })
        .unwrap_or(COMPACT_STMT_KIND_COUNT - 1)
}

fn compact_body_tail_call_blocker_callee(program: &ArenaProgram, block: BlockId) -> Option<ExprId> {
    let stmt = program
        .arena
        .stmt_ids(program.arena.block(block).statements)
        .last()?;
    match program.arena.stmt(program.arena.core_stmt_id(stmt)).kind {
        ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr))) | ArenaStmtKind::Expr(expr) => {
            compact_expr_call_blocker_callee(program, expr)
        }
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Const {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        }
        | ArenaStmtKind::Var {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        } => compact_expr_call_blocker_callee(program, expr),
        _ => None,
    }
}

fn compact_body_tail_command_blocker(
    program: &ArenaProgram,
    block: BlockId,
) -> Option<crate::syntax::arena::CommandStmtId> {
    let stmt = program
        .arena
        .stmt_ids(program.arena.block(block).statements)
        .last()?;
    match program.arena.stmt(program.arena.core_stmt_id(stmt)).kind {
        ArenaStmtKind::Command(command) => Some(command),
        _ => None,
    }
}

const _: [(); COMPACT_TYPE_EXPR_TAG_COUNT] = [(); 13];
const _: [(); COMPACT_STMT_KIND_COUNT] = [(); 30];
const _: [(); COMPACT_EXPR_KIND_COUNT] = [(); 52];
const _: [(); COMPACT_CALL_BLOCKER_KIND_COUNT] = [(); 6];
const _: [(); COMPACT_COMMAND_BLOCKER_KIND_COUNT] = [(); 6];

fn stream_item_type(ty: &Type) -> Option<&Type> {
    match ty.unvalidated() {
        Type::List(item) | Type::Stream(item) => Some(item.as_ref()),
        _ => None,
    }
}

impl<'p> CompactLowerConstructProbe<'p, '_> {
    fn text_value_in_span<'a>(
        &'a self,
        text: &'a crate::syntax::arena::ArenaText,
        _context: Span,
    ) -> Option<&'a str> {
        match text {
            crate::syntax::arena::ArenaText::Source(text_source) => {
                let start = text_source.bytes.start as usize;
                let span = Span::new(
                    text_source.source_id,
                    start,
                    start + text_source.bytes.len as usize,
                );
                self.program
                    .arena
                    .text_value(text, self.source)
                    .or_else(|| self.sources.and_then(|sources| sources.span_text(span)))
            }
            crate::syntax::arena::ArenaText::Cooked(value) => Some(value.as_ref()),
        }
    }

    fn bare_text_value_in_span<'a>(
        &'a self,
        text: &'a crate::syntax::arena::ArenaText,
        context: Span,
    ) -> Option<&'a str> {
        let value = self.text_value_in_span(text, context)?;
        if value.is_empty() {
            self.sources
                .and_then(|sources| sources.span_text(context))
                .filter(|text| !text.is_empty())
                .or(Some(value))
        } else {
            Some(value)
        }
    }

    fn text_value<'a>(&'a self, text: &'a crate::syntax::arena::ArenaText) -> Option<&'a str> {
        match text {
            crate::syntax::arena::ArenaText::Source(text_source) => {
                let start = text_source.bytes.start as usize;
                let span = Span::new(
                    text_source.source_id,
                    start,
                    start + text_source.bytes.len as usize,
                );
                self.program
                    .arena
                    .text_value(text, self.source)
                    .or_else(|| self.sources.and_then(|sources| sources.span_text(span)))
            }
            crate::syntax::arena::ArenaText::Cooked(value) => Some(value.as_ref()),
        }
    }

    fn probe_program(&mut self) {
        let root = self.program.statement_ids().collect::<Vec<_>>();
        self.top_level_known = self.collect_top_level_known(&root);
        self.lower_top_level_program(&root);
        for stmt in root {
            self.probe_function_stmt(stmt);
        }
        for module in &self.program.modules {
            let statements = self.program.module_statements(module).collect::<Vec<_>>();
            self.current_namespace = Some(module.name);
            self.top_level_known = self.collect_top_level_known(&statements);
            self.lower_top_level_program(&statements);
            for stmt in statements {
                self.probe_function_stmt(stmt);
            }
            self.current_namespace = None;
        }
    }

    fn collect_top_level_known(
        &self,
        statements: &[StmtId],
    ) -> FxHashMap<Name, LoweredTopLevelBinding> {
        let mut known = top_level_known_with_runtime_bindings();
        for stmt in statements {
            self.record_top_level_binding(*stmt, &mut known);
        }
        known
    }

    fn append_immutable_top_level_captures(&self, slots: &mut SlotScope) -> LoweredTopLevelSlots {
        let mut bindings = self
            .top_level_known
            .iter()
            .filter(|(name, binding)| {
                binding.slot
                    && slots.resolve(**name).is_none()
                    && !self
                        .declarations
                        .prepared_constants
                        .global_bindings
                        .contains_key(&(self.current_namespace, **name))
            })
            .map(|(name, binding)| (*name, binding.kind, binding.mutable))
            .collect::<Vec<_>>();
        bindings.sort_unstable_by_key(|(name, _, _)| *name);

        let mut captures: LoweredTopLevelSlots = Default::default();
        for (name, kind, mutable) in bindings {
            let slot = slots.declare_capture(name);
            captures.push(LoweredTopLevelSlot {
                name,
                slot,
                kind,
                mutable,
            });
        }
        captures
    }

    fn lower_top_level_program(&mut self, statements: &[StmtId]) {
        self.lower_program_statements(statements);
    }

    fn lower_program_statements(&mut self, statements: &[StmtId]) -> ProgramBuild {
        let mut known = top_level_known_with_runtime_bindings();
        let mut lowered = ProgramBuild {
            statements: Vec::with_capacity(statements.len()),
            scratch: self.scratch.clone(),
        };
        for stmt in statements {
            self.output.top_level_statements += 1;
            let blockers_before = self.output.blocker_events;
            let mut item = self.lower_top_level_stmt(*stmt, &known);
            if self.output.blocker_events != blockers_before {
                item = None;
            }
            if item.is_some() {
                self.output.constructed_top_level_statements += 1;
            } else if !construct_top_level_stmt_is_skippable(self.program, *stmt) {
                let blocker = self.top_level_blocker_kind(*stmt);
                self.output.top_level_blockers[blocker.index()] += 1;
                self.record_top_level_blocker_detail(*stmt, blocker);
            }
            lowered.statements.push(item);
            self.record_top_level_binding(*stmt, &mut known);
        }
        lowered
    }

    fn probe_function_stmt(&mut self, id: StmtId) {
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner) => self.probe_function_stmt(inner),
            ArenaStmtKind::PureDef(def) => {
                self.output.functions += 1;
                let previous_known = std::mem::replace(
                    &mut self.top_level_known,
                    compact_function_top_level_known(
                        self.program,
                        self.declarations,
                        self.source,
                        self.sources,
                        self.current_namespace,
                        def,
                        self.functions,
                    ),
                );
                let function = CompactFunctionDef {
                    key: compact_function_key(
                        self.current_namespace,
                        self.program.arena.function_def(def).name,
                    ),
                    id: def,
                    pure: true,
                    namespace: self.current_namespace,
                    definition_span: self.program.arena.stmt(id).span,
                };
                let definitions = self.function_index();
                let dependencies =
                    compact_function_dependency_keys(&definitions, self.program, &function);
                let (scc_member_count, scc_group) = definitions.scc_metadata(&function);
                let unit =
                    self.lower_function_unit(function, dependencies, scc_member_count, scc_group);
                if unit.is_lowered() {
                    self.output.constructed_functions += 1;
                } else if let Some(blocker) = unit.blocker() {
                    let blocker = CompactFunctionBlocker::from(blocker);
                    self.output.function_blockers[blocker.index()] += 1;
                    self.record_function_blocker_detail(def, blocker);
                }
                self.top_level_known = previous_known;
            }
            ArenaStmtKind::ProcDef(def) | ArenaStmtKind::CliMain(def) => {
                self.output.functions += 1;
                let previous_known = std::mem::replace(
                    &mut self.top_level_known,
                    compact_function_top_level_known(
                        self.program,
                        self.declarations,
                        self.source,
                        self.sources,
                        self.current_namespace,
                        def,
                        self.functions,
                    ),
                );
                let function = CompactFunctionDef {
                    key: compact_function_key(
                        self.current_namespace,
                        self.program.arena.function_def(def).name,
                    ),
                    id: def,
                    pure: false,
                    namespace: self.current_namespace,
                    definition_span: self.program.arena.stmt(id).span,
                };
                let definitions = self.function_index();
                let dependencies =
                    compact_function_dependency_keys(&definitions, self.program, &function);
                let (scc_member_count, scc_group) = definitions.scc_metadata(&function);
                let unit =
                    self.lower_function_unit(function, dependencies, scc_member_count, scc_group);
                if unit.is_lowered() {
                    self.output.constructed_functions += 1;
                    if !self.program.arena.function_def(def).test_declaration
                        && self.program.arena.function_def(def).name == Name::intern("main")
                    {
                        self.output.constructed_auto_main_functions += 1;
                    }
                } else if let Some(blocker) = unit.blocker() {
                    let blocker = CompactFunctionBlocker::from(blocker);
                    self.output.function_blockers[blocker.index()] += 1;
                    self.record_function_blocker_detail(def, blocker);
                }
                self.top_level_known = previous_known;
            }
            ArenaStmtKind::StreamDef(_def) => {
                self.output.functions += 1;
                self.output.constructed_functions += 1;
            }
            _ => {}
        }
    }

    fn lower_function_unit(
        &mut self,
        function: CompactFunctionDef,
        dependency_edges: Vec<LoweredFunctionKey>,
        scc_member_count: usize,
        scc_group: Option<usize>,
    ) -> LoweredFunctionUnit {
        let def = self.program.arena.function_def(function.id);
        let source_span = self
            .program
            .arena
            .span(self.program.arena.block(def.body).span);
        let definition_span = function.definition_span;
        match self.lower_function_with_blocker(function.id, function.pure) {
            Ok(body) => {
                let param_count = body.params.len();
                let capture_count = body.captures.len();
                let slot_count = body.slot_count;
                LoweredFunctionUnit {
                    key: function.key,
                    kind: if function.pure {
                        LoweredFunctionKind::Pure
                    } else {
                        LoweredFunctionKind::Proc
                    },
                    source_span,
                    definition_span,
                    owner: function.namespace,
                    param_count,
                    capture_count,
                    slot_count,
                    dependency_edges,
                    body: Some(body),
                    blocker: None,
                    blocker_detail: None,
                    scc_member_count,
                    scc_group,
                }
            }
            Err(blocker) => {
                let blocker_detail = self.last_blocker_detail.clone();
                LoweredFunctionUnit {
                    key: function.key,
                    kind: if function.pure {
                        LoweredFunctionKind::Pure
                    } else {
                        LoweredFunctionKind::Proc
                    },
                    source_span,
                    definition_span,
                    owner: function.namespace,
                    param_count: self.program.arena.params(def.params).len(),
                    capture_count: 0,
                    slot_count: 0,
                    dependency_edges,
                    body: None,
                    blocker: Some(blocker.into()),
                    blocker_detail,
                    scc_member_count,
                    scc_group,
                }
            }
        }
    }

    fn record_function_blocker_detail(
        &mut self,
        id: crate::syntax::arena::FunctionDefId,
        blocker: CompactFunctionBlocker,
    ) {
        let def = self.program.arena.function_def(id);
        match blocker {
            CompactFunctionBlocker::ReturnType => {
                let tag = self.program.arena.type_expr_tags[def.return_ty.index()];
                self.output.function_return_type_tags[compact_type_expr_tag_index(tag)] += 1;
            }
            CompactFunctionBlocker::ParamType => {
                for param in self.program.arena.params(def.params) {
                    if lowered_arena_type(&self.program.arena, param.ty, self.declarations)
                        .is_none()
                    {
                        let tag = self.program.arena.type_expr_tags[param.ty.index()];
                        self.output.function_param_type_tags[compact_type_expr_tag_index(tag)] += 1;
                    }
                }
            }
            CompactFunctionBlocker::Body => {
                let index = compact_body_tail_stmt_kind(self.program, def.body);
                self.output.function_body_tail_stmt_kinds[index] += 1;
                if let Some(command) = compact_body_tail_command_blocker(self.program, def.body) {
                    let index = compact_command_blocker_index(self.program, command);
                    self.output.function_body_tail_command_kinds[index] += 1;
                }
                if let Some(callee) = compact_body_tail_call_blocker_callee(self.program, def.body)
                {
                    record_compact_call_blocker_label(
                        &mut self.output.function_body_tail_call_callees,
                        self.program,
                        callee,
                    );
                }
            }
            CompactFunctionBlocker::ParamDefault
            | CompactFunctionBlocker::BlockParams
            | CompactFunctionBlocker::NoReturn => {}
        }
    }

    fn lower_function_with_blocker(
        &mut self,
        id: crate::syntax::arena::FunctionDefId,
        _pure: bool,
    ) -> Result<FunctionBuild, CompactFunctionBlocker> {
        let def = self.program.arena.function_def(id);
        let body_span = self
            .program
            .arena
            .span(self.program.arena.block(def.body).span);
        let inferred_kind = self
            .declarations
            .function_return_types
            .get(&body_span)
            .and_then(|ty| {
                let storage_kind = |ty: &Type| match ty {
                    Type::Null | Type::Optional(_) => Some(LoweredType::Any),
                    _ => lowered_checked_type(ty),
                };
                if let Type::Result(ok, _) = ty {
                    storage_kind(ok).map(LoweredReturnKind::Result)
                } else if matches!(ty, Type::Optional(inner) if matches!(**inner, Type::Result(_, _)))
                {
                    Some(LoweredReturnKind::OptionalResult)
                } else {
                    storage_kind(ty).map(LoweredReturnKind::Plain)
                }
            });
        let return_kind = match inferred_kind.or_else(|| self.lowered_return_kind(def.return_ty)) {
            Some(kind) => kind,
            None => {
                self.last_blocker_detail = Some((
                    self.program.arena.type_expr_span(def.return_ty),
                    "unsupported return type annotation".to_string(),
                ));
                return Err(CompactFunctionBlocker::ReturnType);
            }
        };
        let mut param_kinds: LoweredParamKinds = Default::default();
        let mut param_checks: LoweredParamChecks = Default::default();
        let mut param_rest: LoweredParamRest = Default::default();
        let mut param_defaults: LoweredParamDefaults = Default::default();
        let mut params: LoweredParamNames = Default::default();
        let mut checked_params = Vec::new();
        let mut expression_defaults = Vec::new();
        for (index, param) in self.program.arena.params(def.params).iter().enumerate() {
            let expected = self
                .declarations
                .parameter_types
                .get(&self.program.arena.span(param.span))
                .cloned()
                .unwrap_or_else(|| {
                    compact_runtime_type_in_namespace(
                        &self.program.arena,
                        param.ty,
                        self.declarations,
                        self.current_namespace,
                    )
                });
            let kind = match &expected {
                Type::Optional(_) | Type::Null => Some(LoweredType::Any),
                _ => lowered_checked_type(&expected),
            }
            .ok_or(CompactFunctionBlocker::ParamType)?;
            // A parameter of a callable type records the checked type, so
            // the verifier can tie a typed call through the parameter to it.
            let check = if matches!(expected, Type::Callable(_)) {
                Some(LoweredTypeCheck {
                    schema: None,
                    ty: expected.clone(),
                    name: Arc::from(expected.to_string()),
                })
            } else if param.ty_defaulted {
                (lowered_type_needs_static_check(kind) || expected.validated().is_some()).then(
                    || LoweredTypeCheck {
                        schema: None,
                        ty: expected.clone(),
                        name: Arc::from(expected.to_string()),
                    },
                )
            } else {
                compact_type_check(
                    kind,
                    &self.program.arena,
                    param.ty,
                    self.declarations,
                    self.current_namespace,
                )
            };
            let default = if let Some(expr) = param.default {
                let prepared = self
                    .declarations
                    .prepared_constants
                    .analyze_expression(&self.program.arena, expr)
                    .map(|constant| constant.in_type(&expected))
                    .and_then(|constant| {
                        lower_literal_constant(&constant, Some(&self.declarations.wire_enums))
                    })
                    .filter(|value| lowered_value_matches(kind, value))
                    .or_else(|| {
                        lower_const_param_default(&self.program.arena, expr, kind, Some(&expected))
                    });
                if let Some(value) = prepared {
                    Some(value)
                } else {
                    expression_defaults.push((index, expr, kind, check.clone()));
                    Some(LoweredValue::OmittedArgument)
                }
            } else {
                None
            };
            param_kinds.push(kind);
            param_checks.push(check);
            param_rest.push(param.rest);
            param_defaults.push(default);
            params.push(param.name);
            checked_params.push(expected);
        }
        if !def.test_declaration && !self.program.arena.block(def.body).params.is_empty() {
            self.last_blocker_detail = Some((
                self.program
                    .arena
                    .span(self.program.arena.block(def.body).span),
                "function body block parameters are not lowerable".to_string(),
            ));
            return Err(CompactFunctionBlocker::BlockParams);
        }
        // NOTE: nested loops are supported by the lowered runtime (break/continue
        // use StmtFlow which correctly scopes to the innermost loop).
        // The check is removed — it was an early indexed-lowering safety measure that is no longer needed.
        // Parameter slots exist at entry, but their names are hidden while
        // lowering defaults so every default resolves in the outer environment.
        let mut slots = SlotScope::from_names([]);
        for _ in &params {
            slots.reserve("parameter");
        }
        let captures = self.append_immutable_top_level_captures(&mut slots);
        let blockers_before = self.output.blocker_events;
        let mut default_prefix = Vec::new();
        for (slot, expr, kind, check) in expression_defaults {
            let value = self
                .lower_expr(expr, &mut slots, Some(def.name), None)
                .ok_or(CompactFunctionBlocker::ParamDefault)?;
            default_prefix.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::DefaultParameter {
                    slot,
                    value,
                    kind,
                    check,
                    span: self.program.arena.expr(expr).span
                }
            ));
        }
        for (slot, (name, ty)) in params.iter().copied().zip(checked_params).enumerate() {
            slots.indices.insert(name, slot);
            slots.types.insert(name, ty);
            slots.captures.remove(&name);
        }
        let mut body = self
            .lower_tail_block(def.body, &mut slots, Some(def.name), None)
            .ok_or_else(|| {
                if self.last_blocker_detail.is_none() {
                    self.last_blocker_detail = Some((
                        self.program
                            .arena
                            .span(self.program.arena.block(def.body).span),
                        "unsupported statement in body".to_string(),
                    ));
                }
                CompactFunctionBlocker::Body
            })?;
        default_prefix.append(&mut body);
        body = default_prefix;
        // The construct probe is permissive: it substitutes `Unit` for any
        // sub-expression/statement it cannot lower so it can finish traversing
        // and tally blockers. That placeholder must never be committed as real
        // code. If lowering this body produced any blocker (e.g. a forward
        // reference to a not-yet-lowered function), refuse to commit so the
        // fixpoint retries once dependencies are available, or the function
        // falls back honestly.
        if self.output.blocker_events != blockers_before {
            if self.last_blocker_detail.is_none() {
                self.last_blocker_detail = Some((
                    self.program
                        .arena
                        .span(self.program.arena.block(def.body).span),
                    "unsupported statement in body".to_string(),
                ));
            }
            return Err(CompactFunctionBlocker::Body);
        }
        let can_return = {
            let scratch = self.scratch.borrow();
            lowered_body_can_return(&scratch, &body)
        };
        if !can_return {
            if matches!(return_kind, LoweredReturnKind::Plain(LoweredType::Stream)) {
            } else if lowered_return_kind_accepts_unit_fallthrough(return_kind) {
                body.push(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Return {
                        value: push_build_row!(self, expr, BuildExprRow::Unit),
                    }
                ));
            } else {
                self.last_blocker_detail = Some((
                    self.program
                        .arena
                        .span(self.program.arena.block(def.body).span),
                    "function body may fall through without returning".to_string(),
                ));
                return Err(CompactFunctionBlocker::NoReturn);
            }
        }
        let has_defers = {
            let scratch = self.scratch.borrow();
            lowered_body_has_defers(&scratch, &body)
        };
        Ok(FunctionBuild {
            params,
            param_kinds,
            param_checks,
            param_rest,
            param_defaults,
            captures,
            return_kind,
            return_check: self
                .declarations
                .function_return_types
                .get(&body_span)
                .cloned()
                .or_else(|| {
                    Some(compact_runtime_type_in_namespace(
                        &self.program.arena,
                        def.return_ty,
                        self.declarations,
                        self.current_namespace,
                    ))
                })
                .filter(Type::has_unsigned_constraint)
                .map(|ty| LoweredTypeCheck {
                    name: Arc::from(ty.to_string()),
                    ty,
                    schema: None,
                }),
            slot_count: slots.count(),
            body,
            has_defers,
            scratch: self.scratch.clone(),
        })
    }

    fn record_top_level_blocker_detail(&mut self, id: StmtId, blocker: CompactTopLevelBlocker) {
        if let ArenaStmtKind::Export(inner)
        | ArenaStmtKind::Sugar {
            expansion: inner, ..
        } = self.program.arena.stmt(id).kind
        {
            self.record_top_level_blocker_detail(inner, blocker);
            return;
        }
        let label = blocker.label().to_string();
        let samples = self
            .output
            .top_level_blocker_sample_spans
            .entry(label)
            .or_default();
        if samples.len() < 8 {
            samples.push(self.program.arena.stmt(id).span);
        }
        let kind = self.program.arena.stmt(id).kind;
        let index = compact_stmt_kind_index(kind);
        self.output.top_level_blocker_stmt_kinds[index] += 1;
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Let {
                ty,
                initializer: ArenaExprOrRun::Expr(value),
                ..
            }
            | ArenaStmtKind::Const {
                ty,
                initializer: ArenaExprOrRun::Expr(value),
                ..
            }
            | ArenaStmtKind::Var {
                ty,
                initializer: ArenaExprOrRun::Expr(value),
                ..
            } => match blocker {
                CompactTopLevelBlocker::BindingType => {
                    if let Some(ty) = ty {
                        let tag = self.program.arena.type_expr_tags[ty.index()];
                        self.output.top_level_binding_type_annotation_tags
                            [compact_type_expr_tag_index(tag)] += 1;
                    } else {
                        let kind = self.program.arena.expr(value).kind;
                        self.output.top_level_binding_type_expr_kinds
                            [compact_expr_kind_index(kind)] += 1;
                    }
                    if let Some(callee) = compact_expr_call_blocker_callee(self.program, value) {
                        self.output.top_level_binding_type_call_blockers
                            [compact_call_blocker_index(self.program, callee)] += 1;
                        record_compact_call_blocker_label(
                            &mut self.output.top_level_binding_type_call_callees,
                            self.program,
                            callee,
                        );
                    }
                }
                CompactTopLevelBlocker::BindingExpression => {
                    let kind = self.program.arena.expr(value).kind;
                    self.output.top_level_binding_expression_expr_kinds
                        [compact_expr_kind_index(kind)] += 1;
                    if let Some(callee) = compact_expr_call_blocker_callee(self.program, value) {
                        self.output.top_level_binding_expression_call_blockers
                            [compact_call_blocker_index(self.program, callee)] += 1;
                        record_compact_call_blocker_label(
                            &mut self.output.top_level_binding_expression_call_callees,
                            self.program,
                            callee,
                        );
                    }
                }
                _ => {}
            },
            ArenaStmtKind::Expr(value) if matches!(blocker, CompactTopLevelBlocker::Expression) => {
                let kind = self.program.arena.expr(value).kind;
                self.output.top_level_expression_expr_kinds[compact_expr_kind_index(kind)] += 1;
                if let Some(callee) = compact_expr_call_blocker_callee(self.program, value) {
                    self.output.top_level_expression_call_blockers
                        [compact_call_blocker_index(self.program, callee)] += 1;
                    record_compact_call_blocker_label(
                        &mut self.output.top_level_expression_call_callees,
                        self.program,
                        callee,
                    );
                }
            }
            ArenaStmtKind::Command(command)
                if matches!(blocker, CompactTopLevelBlocker::Command) =>
            {
                let index = compact_command_blocker_index(self.program, command);
                self.output.top_level_command_kinds[index] += 1;
            }
            _ => {}
        }
    }

    fn top_level_blocker_kind(&self, id: StmtId) -> CompactTopLevelBlocker {
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner)
            | ArenaStmtKind::Sugar {
                expansion: inner, ..
            } => self.top_level_blocker_kind(inner),
            ArenaStmtKind::Use(_) => CompactTopLevelBlocker::Use,
            ArenaStmtKind::Let { target, ty, .. }
            | ArenaStmtKind::Const { target, ty, .. }
            | ArenaStmtKind::Var { target, ty, .. } => {
                if simple_binding_target(self.program, target).is_none() {
                    return CompactTopLevelBlocker::BindingTarget;
                }
                if ty.is_some_and(|ty| {
                    lowered_arena_type(&self.program.arena, ty, self.declarations).is_none()
                }) {
                    return CompactTopLevelBlocker::BindingType;
                }
                CompactTopLevelBlocker::BindingExpression
            }
            ArenaStmtKind::Assign {
                target,
                value: ArenaExprOrRun::Expr(_),
                ..
            } => {
                if !matches!(
                    self.program.arena.assign_target(target).kind,
                    ArenaAssignTargetKind::Name(_)
                ) {
                    return CompactTopLevelBlocker::AssignTarget;
                }
                CompactTopLevelBlocker::AssignExpression
            }
            ArenaStmtKind::Assign { .. } => CompactTopLevelBlocker::AssignExpression,
            ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::Match { .. } => CompactTopLevelBlocker::Control,
            ArenaStmtKind::Command(_) => CompactTopLevelBlocker::Command,
            ArenaStmtKind::Expr(_) | ArenaStmtKind::Exit(_) => CompactTopLevelBlocker::Expression,
            ArenaStmtKind::Defer(..) => CompactTopLevelBlocker::Defer,
            ArenaStmtKind::SignalHook(_) => CompactTopLevelBlocker::Other,
            _ => CompactTopLevelBlocker::Other,
        }
    }

    fn lower_top_level_stmt(
        &mut self,
        id: StmtId,
        known: &FxHashMap<Name, LoweredTopLevelBinding>,
    ) -> Option<BuildTopStmtId> {
        match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Export(inner)
            | ArenaStmtKind::Sugar {
                expansion: inner, ..
            } => self.lower_top_level_stmt(inner, known),
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = self.program.arena.use_stmt(use_id);
                let key = use_stmt.resolved.clone()?;
                let path = self.program.arena.names(use_stmt.path).collect::<Vec<_>>();
                let namespace = compact_use_import_namespace(self.program, use_id)?;
                let exports = compact_module_exports_for_use(
                    self.program,
                    key.as_ref(),
                    namespace,
                    self.functions,
                )?;
                let module = self
                    .program
                    .modules
                    .iter()
                    .find(|module| module.key.as_str() == key.as_ref())?;
                let module_statement_ids =
                    self.program.module_statements(module).collect::<Vec<_>>();
                let module_lowered = {
                    let mut probe = CompactLowerConstructProbe {
                        program: self.program,
                        declarations: self.declarations,
                        bodies: self.bodies,
                        source: self.source,
                        sources: self.sources,
                        current_namespace: Some(module.name),
                        functions: self.functions,
                        top_level_known: compact_top_level_known(
                            self.program,
                            self.declarations,
                            self.source,
                            self.sources,
                            Some(module.name),
                            self.functions,
                        ),
                        output: CompactLowerConstructProbeOutput::default(),
                        last_blocker_detail: None,
                        stdlib_linkage: StdlibLowerLinkage::Local,
                        function_defs: Rc::new(RefCell::new(None)),
                        scratch: self.scratch.clone(),
                        spread_programs: Rc::clone(&self.spread_programs),
                    };
                    probe.lower_program_statements(&module_statement_ids)
                };
                let module_statements = module_statement_ids
                    .into_iter()
                    .zip(module_lowered.statements)
                    .filter_map(|(stmt, lowered)| {
                        Some((self.program.arena.stmt(stmt).span, lowered?))
                    })
                    .collect();
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Use {
                        key,
                        alias: use_stmt.alias,
                        path,
                        namespace,
                        exports,
                        module_statements,
                        span: self.program.arena.stmt(id).span,
                    },
                    known,
                    SlotScope::default(),
                ))
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: initializer @ ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: initializer @ ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: initializer @ ArenaExprOrRun::Expr(value),
            } => {
                if let ArenaBindingTargetKind::Record { .. } =
                    self.program.arena.binding_target(target).kind
                {
                    let mut slots = top_level_slots(known);
                    let checked = self.top_level_binding_checked_type(ty, value);
                    let source = self.lower_binding_expr_value(
                        ty,
                        checked.as_ref(),
                        value,
                        self.program.arena.stmt(id).span,
                        &mut slots,
                        None,
                        None,
                    )?;
                    let target =
                        self.lower_comp_target_typed(target, &mut slots, checked.as_ref())?;
                    let field_names = slots
                        .indices
                        .iter()
                        .filter_map(|(name, slot)| {
                            if known.contains_key(name) {
                                None
                            } else {
                                Some((*name, *slot))
                            }
                        })
                        .collect();
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::LetRecord {
                            source,
                            fields: field_names,
                            target,
                            mutable: matches!(
                                self.program.arena.stmt(id).kind,
                                ArenaStmtKind::Var { .. }
                            ),
                            span: self.program.arena.stmt(id).span,
                        },
                        known,
                        slots,
                    ));
                }
                let target = simple_binding_target(self.program, target)?;
                if is_discard_name(target) {
                    let mut slots = top_level_slots(known);
                    let value = self.lower_expr(value, &mut slots, None, None)?;
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::Discard {
                            value,
                            span: match initializer {
                                ArenaExprOrRun::Expr(value) => self.program.arena.expr(value).span,
                                ArenaExprOrRun::Run(_) => unreachable!("expr initializer matched"),
                            },
                        },
                        known,
                        slots,
                    ));
                }
                let annotation = ty;
                let (ty, validation) = match ty {
                    Some(ty) => {
                        let lowered =
                            lowered_arena_type(&self.program.arena, ty, self.declarations)?;
                        (
                            Some(lowered),
                            compact_type_check(
                                lowered,
                                &self.program.arena,
                                ty,
                                self.declarations,
                                self.current_namespace,
                            ),
                        )
                    }
                    None => (None, None),
                };
                let mut slots = top_level_slots(known);
                let checked = self.lower_binding_checked_type(annotation, value);
                let value = if self.is_empty_record_in_map_context(value, annotation) {
                    push_build_row!(self, expr, BuildExprRow::EmptyMap)
                } else {
                    self.lower_expr(value, &mut slots, None, None)?
                };
                let value = if annotation.is_none() {
                    match checked {
                        Some(ty) => self.checked_unsigned_value(
                            value,
                            &ty,
                            self.program.arena.stmt(id).span,
                        ),
                        None => value,
                    }
                } else {
                    value
                };
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Let {
                        target,
                        ty,
                        validation,
                        mutable: matches!(
                            self.program.arena.stmt(id).kind,
                            ArenaStmtKind::Var { .. }
                        ),
                        value,
                        value_span: match initializer {
                            ArenaExprOrRun::Expr(value) => self.program.arena.expr(value).span,
                            ArenaExprOrRun::Run(_) => unreachable!("expr initializer matched"),
                        },
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            } => {
                let target = simple_binding_target(self.program, target)?;
                if is_discard_name(target) {
                    let mut slots = top_level_slots(known);
                    let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::Discard {
                            value,
                            span: self
                                .program
                                .arena
                                .span(self.program.arena.run_form(run).span),
                        },
                        known,
                        slots,
                    ));
                }
                let (ty, validation) = match ty {
                    Some(ty) => {
                        let lowered =
                            lowered_arena_type(&self.program.arena, ty, self.declarations)?;
                        (
                            Some(lowered),
                            compact_type_check(
                                lowered,
                                &self.program.arena,
                                ty,
                                self.declarations,
                                self.current_namespace,
                            ),
                        )
                    }
                    None => (None, None),
                };
                let mut slots = top_level_slots(known);
                let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Let {
                        target,
                        ty,
                        validation,
                        mutable: matches!(
                            self.program.arena.stmt(id).kind,
                            ArenaStmtKind::Var { .. }
                        ),
                        value,
                        value_span: self
                            .program
                            .arena
                            .span(self.program.arena.run_form(run).span),
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Assign { target, op, value } => {
                let ArenaAssignTargetKind::Name(target) =
                    self.program.arena.assign_target(target).kind
                else {
                    // An environment assignment writes no binding.
                    if let Some(root) = self.assign_target_root_name(target)
                        && !known.get(&root).is_some_and(|binding| binding.mutable)
                    {
                        return None;
                    }
                    let mut slots = top_level_slots(known);
                    let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::Stmt(lowered),
                        known,
                        slots,
                    ));
                };
                if known.get(&target).is_some_and(|binding| {
                    binding
                        .checked
                        .as_ref()
                        .is_some_and(Type::has_unsigned_constraint)
                }) {
                    let mut slots = top_level_slots(known);
                    let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                    return Some(lowered_top_level(
                        &self.scratch,
                        BuildTopKind::Stmt(lowered),
                        known,
                        slots,
                    ));
                }
                let mut slots = top_level_slots(known);
                let value = match value {
                    ArenaExprOrRun::Expr(expr) => self.lower_expr(expr, &mut slots, None, None)?,
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, &mut slots, None, None)?
                    }
                };
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Assign {
                        target,
                        op,
                        value,
                        span: self.program.arena.stmt(id).span,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Guard { .. } => {
                // The success bindings outlive the statement, so the guard is
                // the source of a binding step that publishes its new slots.
                let mut slots = top_level_slots(known);
                let outer = slots.indices.clone();
                let span = self.program.arena.stmt(id).span;
                let guard = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                let fields = slots
                    .indices
                    .iter()
                    .filter(|(name, slot)| outer.get(*name) != Some(*slot))
                    .map(|(name, slot)| (*name, *slot))
                    .collect();
                let source = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ValueBlock {
                        body: vec![guard],
                        span
                    }
                );
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::LetRecord {
                        source,
                        fields,
                        target: LoweredCompTarget::Discard,
                        mutable: false,
                        span,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::If { .. }
            | ArenaStmtKind::While { .. }
            | ArenaStmtKind::For { .. }
            | ArenaStmtKind::Match { .. }
            | ArenaStmtKind::Loop { .. }
            | ArenaStmtKind::With { .. } => {
                let mut slots = top_level_slots(known);
                let lowered = self.lower_stmt_with_blocker_guard(id, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Stmt(lowered),
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Command(command) => {
                let mut slots = top_level_slots(known);
                let lowered = self
                    .lower_print_stmt(command, &mut slots, None, None)
                    .or_else(|| self.lower_cd_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_env_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_run_stmt(command, &mut slots, None, None))
                    .or_else(|| self.lower_proc_stmt(command, &mut slots, None, None))?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Stmt(lowered),
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Expr(value) => {
                let mut slots = top_level_slots(known);
                let value = self.lower_expr(value, &mut slots, None, None)?;
                let kind = BuildTopKind::Expr(value);
                Some(lowered_top_level(&self.scratch, kind, known, slots))
            }
            ArenaStmtKind::Exit(status) => {
                let mut slots = top_level_slots(known);
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_exit(status, span, &mut slots, None, None)?;
                let kind = BuildTopKind::Expr(value);
                Some(lowered_top_level(&self.scratch, kind, known, slots))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(value), trigger) => {
                let mut slots = top_level_slots(known);
                let value = self.lower_deferred_expr(value, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Defer {
                        value,
                        span: self.program.arena.stmt(id).span,
                        on_error: trigger == DeferTrigger::Error,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Run(run), trigger) => {
                let mut slots = top_level_slots(known);
                let value = self.lower_run_binding_value(run, &mut slots, None, None)?;
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::Defer {
                        value,
                        span: self.program.arena.stmt(id).span,
                        on_error: trigger == DeferTrigger::Error,
                    },
                    known,
                    slots,
                ))
            }
            ArenaStmtKind::SignalHook(hook_id) => {
                let hook = self.program.arena.signal_hook(hook_id);
                let mut slot_scope = top_level_slots(known);
                let body = self.lower_block(hook.body, &mut slot_scope, None, None)?;
                let slot_count = slot_scope.count();
                let hook_slots: Vec<LoweredTopLevelSlot> = known
                    .iter()
                    .filter_map(|(&name, binding)| {
                        slot_scope.resolve(name).map(|slot| LoweredTopLevelSlot {
                            name,
                            slot,
                            kind: binding.kind,
                            mutable: binding.mutable,
                        })
                    })
                    .collect();
                Some(lowered_top_level(
                    &self.scratch,
                    BuildTopKind::SignalHook {
                        signal: hook.signal,
                        pre_cancel: hook.options.pre_cancel.clone(),
                        body,
                        slots: hook_slots,
                        slot_count,
                        span: self.program.arena.stmt(id).span,
                    },
                    known,
                    slot_scope,
                ))
            }
            _ => None,
        }
    }

    fn record_top_level_binding(
        &self,
        id: StmtId,
        known: &mut FxHashMap<Name, LoweredTopLevelBinding>,
    ) {
        let stmt = self.program.arena.stmt(id);
        match stmt.kind {
            // `export let X = …` inside a module makes `X` a module-scope binding
            // that the module's functions may capture; record the inner binding.
            ArenaStmtKind::Export(inner) => self.record_top_level_binding(inner, known),
            ArenaStmtKind::Use(use_id) => {
                let use_stmt = self.program.arena.use_stmt(use_id);
                let Some(resolved) = use_stmt.resolved.as_ref() else {
                    return;
                };
                let Some(namespace) = compact_use_import_namespace(self.program, use_id) else {
                    return;
                };
                if use_stmt.alias.is_none()
                    && let Some(module) = self
                        .program
                        .modules
                        .iter()
                        .find(|module| module.key.as_str() == resolved.as_ref())
                {
                    for stmt in self.program.module_statements(module) {
                        let ArenaStmtKind::Export(inner) = self.program.arena.stmt(stmt).kind
                        else {
                            continue;
                        };
                        match self.program.arena.stmt(inner).kind {
                            ArenaStmtKind::Let {
                                target,
                                ty,
                                initializer: ArenaExprOrRun::Expr(_),
                            }
                            | ArenaStmtKind::Const {
                                target,
                                ty,
                                initializer: ArenaExprOrRun::Expr(_),
                            }
                            | ArenaStmtKind::Var {
                                target,
                                ty,
                                initializer: ArenaExprOrRun::Expr(_),
                            } => {
                                let Some(name) = simple_binding_target(self.program, target) else {
                                    continue;
                                };
                                if is_discard_name(name) {
                                    continue;
                                }
                                known.insert(
                                    name,
                                    LoweredTopLevelBinding {
                                        kind: self.top_level_binding(inner, ty, false).kind,
                                        checked: None,
                                        mutable: false,
                                        slot: true,
                                    },
                                );
                            }
                            ArenaStmtKind::ProcDef(def) => {
                                let name = self.program.arena.function_def(def).name;
                                known.insert(
                                    name,
                                    LoweredTopLevelBinding {
                                        kind: LoweredType::Proc,
                                        checked: None,
                                        mutable: false,
                                        slot: false,
                                    },
                                );
                            }
                            ArenaStmtKind::PureDef(def) => {
                                let name = self.program.arena.function_def(def).name;
                                known.insert(
                                    name,
                                    LoweredTopLevelBinding {
                                        kind: LoweredType::Pure,
                                        checked: None,
                                        mutable: false,
                                        slot: false,
                                    },
                                );
                            }
                            _ => {}
                        }
                    }
                }
                known.insert(
                    namespace,
                    LoweredTopLevelBinding {
                        kind: LoweredType::Module,
                        checked: None,
                        mutable: false,
                        slot: true,
                    },
                );
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer,
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer,
            } => {
                let mutable = matches!(stmt.kind, ArenaStmtKind::Var { .. });
                if let (ArenaBindingTargetKind::Record { .. }, ArenaExprOrRun::Expr(value)) =
                    (&self.program.arena.binding_target(target).kind, initializer)
                {
                    let checked = self.top_level_binding_checked_type(ty, value);
                    for (name, checked) in
                        record_binding_types(self.program, target, checked.as_ref())
                    {
                        let kind = checked
                            .as_ref()
                            .and_then(lowered_checked_type)
                            .unwrap_or(LoweredType::Any);
                        known.insert(
                            name,
                            LoweredTopLevelBinding {
                                kind,
                                checked,
                                mutable,
                                slot: true,
                            },
                        );
                    }
                    return;
                }
                let Some(name) = simple_binding_target(self.program, target) else {
                    return;
                };
                if !is_discard_name(name) {
                    known.insert(name, self.top_level_binding(id, ty, mutable));
                }
            }
            ArenaStmtKind::Guard {
                target,
                initializer,
                ..
            } => {
                let checked = match initializer {
                    ArenaExprOrRun::Expr(value) => self.concrete_checked_type(value),
                    ArenaExprOrRun::Run(_) => None,
                };
                let optional = self.bodies.optional_binding_guards.contains(&id);
                for (name, checked) in record_binding_types(
                    self.program,
                    target,
                    checked
                        .as_ref()
                        .and_then(|ty| guard_bound_type(ty, optional)),
                ) {
                    let kind = checked
                        .as_ref()
                        .and_then(lowered_checked_type)
                        .unwrap_or(LoweredType::Any);
                    known.insert(
                        name,
                        LoweredTopLevelBinding {
                            kind,
                            checked,
                            mutable: false,
                            slot: true,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    /// A top-level binding's representation comes from its annotation, else
    /// from the checker's published binding type.
    fn top_level_binding(
        &self,
        id: StmtId,
        ty: Option<TypeExprId>,
        mutable: bool,
    ) -> LoweredTopLevelBinding {
        if let Some(ty) = ty {
            #[cfg(debug_assertions)]
            self.note_annotation_drift(id, ty);
            return LoweredTopLevelBinding {
                kind: lowered_arena_type(&self.program.arena, ty, self.declarations)
                    .unwrap_or(LoweredType::Any),
                checked: Some(compact_runtime_type_in_namespace(
                    &self.program.arena,
                    ty,
                    self.declarations,
                    self.current_namespace,
                )),
                mutable,
                slot: true,
            };
        }
        let checked = self
            .declarations
            .local_binding_types
            .get(&self.program.arena.stmt(id).span);
        LoweredTopLevelBinding {
            kind: checked
                .and_then(lowered_checked_type)
                .unwrap_or(LoweredType::Any),
            checked: checked.filter(|ty| concrete_checked_fact(ty)).cloned(),
            mutable,
            slot: true,
        }
    }

    /// Lowering interprets binding annotations itself; the checker publishes
    /// its own reading of the same annotation as the binding type.
    #[cfg(debug_assertions)]
    fn note_annotation_drift(&self, id: StmtId, ty: TypeExprId) {
        let span = self.program.arena.stmt(id).span;
        let kind = lowered_arena_type(&self.program.arena, ty, self.declarations);
        let annotated = compact_runtime_type_in_namespace(
            &self.program.arena,
            ty,
            self.declarations,
            self.current_namespace,
        );
        note_lowering_drift(
            span,
            kind,
            &annotated,
            self.declarations.local_binding_types.get(&span),
        );
    }

    fn top_level_binding_checked_type(
        &self,
        ty: Option<TypeExprId>,
        value: ExprId,
    ) -> Option<Type> {
        ty.map(|ty| {
            compact_runtime_type_in_namespace(
                &self.program.arena,
                ty,
                self.declarations,
                self.current_namespace,
            )
        })
        .or_else(|| self.concrete_checked_type(value))
    }

    /// The checker's published type for `value` when it is concrete, including
    /// a concrete `Result` success type.
    fn concrete_checked_type(&self, value: ExprId) -> Option<Type> {
        self.bodies
            .expr_types
            .get(&value)
            .filter(|ty| concrete_checked_fact(ty))
            .cloned()
    }

    fn lower_binding_checked_type(&self, ty: Option<TypeExprId>, value: ExprId) -> Option<Type> {
        ty.map(|ty| {
            compact_runtime_type_in_namespace(
                &self.program.arena,
                ty,
                self.declarations,
                self.current_namespace,
            )
        })
        .or_else(|| {
            self.bodies
                .expr_types
                .get(&value)
                .filter(|ty| !matches!(ty, Type::Invalid))
                .cloned()
        })
    }

    fn checked_unsigned_value(&mut self, value: BuildExprId, ty: &Type, span: Span) -> BuildExprId {
        if !ty.has_unsigned_constraint() {
            return value;
        }
        let check = LoweredTypeCheck {
            ty: ty.clone(),
            name: Arc::from(ty.to_string()),
            schema: None,
        };
        push_build_row!(
            self,
            expr,
            BuildExprRow::CheckedValue { value, check, span }
        )
    }

    fn lower_checked_call_values(
        &mut self,
        args: &[ExprId],
        types: &[Type],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(Vec<BuildExprId>, Vec<(BuildExprId, usize)>)> {
        if !types.iter().any(Type::has_unsigned_constraint) {
            return Some((
                self.lower_expr_ids(args, slots, current_function, item_slot)?,
                Vec::new(),
            ));
        }
        let mut fields = Vec::with_capacity(args.len());
        let mut bindings = Vec::with_capacity(args.len());
        // Evaluate authored operands before this call's parameter validation.
        for (arg, ty) in args.iter().zip(types) {
            let value = self.lower_expr(*arg, slots, current_function, item_slot)?;
            let slot = slots.reserve("checked call argument");
            bindings.push((value, slot));
            let value = push_build_row!(self, expr, BuildExprRow::Param(slot));
            fields.push(self.checked_unsigned_value(value, ty, self.program.arena.expr(*arg).span));
        }
        Some((fields, bindings))
    }

    fn require_uint_key(&mut self, value: BuildExprId, span: Span) -> BuildExprId {
        let check = LoweredTypeCheck {
            ty: Type::UInt,
            name: Arc::from("UInt"),
            schema: None,
        };
        let checked = push_build_row!(self, expr, BuildExprRow::Require { value, check, span });
        push_build_row!(
            self,
            expr,
            BuildExprRow::Try {
                value: checked,
                span
            }
        )
    }

    fn lower_binding_expr_value(
        &mut self,
        ty: Option<TypeExprId>,
        checked_ty: Option<&Type>,
        value: ExprId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let lowered = self.lower_expr(value, slots, current_function, item_slot)?;
        let Some(ty) = ty else {
            return Some(match checked_ty {
                Some(ty) => self.checked_unsigned_value(lowered, ty, span),
                None => lowered,
            });
        };
        let kind = self
            .bodies
            .expr_types
            .get(&value)
            .map(|ty| ty.optional_inner().unwrap_or(ty))
            .and_then(lowered_checked_type)
            .unwrap_or(LoweredType::Any);
        if lowered_type_needs_static_check(kind) || matches!(checked_ty, Some(Type::UInt)) {
            let check = LoweredTypeCheck {
                schema: None,
                ty: checked_ty.cloned().unwrap_or_else(|| {
                    compact_runtime_type_in_namespace(
                        &self.program.arena,
                        ty,
                        self.declarations,
                        self.current_namespace,
                    )
                }),
                name: compact_type_expr_name(&self.program.arena, ty),
            };
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Require {
                            value: lowered,
                            check,
                            span,
                        }
                    ),
                    span
                }
            ))
        } else {
            Some(lowered)
        }
    }

    fn lower_module_argument_values(
        &mut self,
        args: &[Option<ExprId>],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<Option<BuildExprId>>> {
        args.iter()
            .map(|arg| match arg {
                Some(arg) => self
                    .lower_expr(*arg, slots, current_function, item_slot)
                    .map(Some),
                None => Some(None),
            })
            .collect()
    }

    /// The checker's selected overload, with argument expressions in parameter
    /// slot order and their concrete parameter types.
    fn checked_method_call_args(
        &self,
        call: ExprId,
        args: &[ArenaCallArg],
    ) -> Option<Vec<(ExprId, Type)>> {
        let plan = self
            .bodies
            .api_calls
            .get(&call)
            .filter(|plan| plan.receiver.is_some())?;
        let mut values = vec![None; plan.params.len()];
        for (arg, &slot) in args.iter().zip(&plan.argument_slots) {
            values[slot] = Some((compact_call_arg_expr(arg)?, plan.params[slot].clone()));
        }
        Some(values.into_iter().flatten().collect())
    }

    fn module_call_plan(&self, call: ExprId) -> Option<&'p CheckedApiCall> {
        self.bodies
            .api_calls
            .get(&call)
            .filter(|plan| plan.receiver.is_none())
    }

    fn lowered_method_supported_for_receiver(
        &self,
        base: ExprId,
        name: Name,
        arg_count: usize,
        slots: &SlotScope,
    ) -> bool {
        if let ArenaExprKind::Ident(binding) = self.program.arena.expr(base).kind
            && let Some(ty) = slots.binding_type(binding)
        {
            // A slot declared as a union holds one member wherever a method
            // is called on it, and which member is the checker's decision,
            // published as this receiver's type. Without that fact the call
            // is not lowered: the declared union cannot say which member's
            // methods apply.
            let declared_union = match ty {
                Type::Union(_) => true,
                Type::Optional(inner) => matches!(inner.as_ref(), Type::Union(_)),
                _ => false,
            };
            if declared_union {
                return self.checked_expr_type(base).is_some_and(|narrowed| {
                    self.lowered_method_supported_for_type(&narrowed, name, arg_count)
                });
            }
            // A slot of a base type holds a validated value where a type
            // test proved it, and the checker then resolved the call against
            // the validated type; that fact, not the declaration, says which
            // methods the receiver has.
            if let Some(narrowed) = self
                .checked_expr_type(base)
                .filter(|narrowed| narrowed.validated().is_some())
            {
                return self.lowered_method_supported_for_type(&narrowed, name, arg_count);
            }
            return self.lowered_method_supported_for_type(ty, name, arg_count);
        }
        let Some(ty) = self
            .checked_expr_type(base)
            .or_else(|| self.concrete_checked_type(base))
        else {
            // A local without a declared slot type (a loop item, a pattern
            // capture) may hold a union member, and only the checker's fact
            // says which. Without it the call is not lowered.
            return !matches!(
                self.program.arena.expr(base).kind,
                ArenaExprKind::Ident(binding) if slots.resolve(binding).is_some()
            );
        };
        self.lowered_method_supported_for_type(&ty, name, arg_count)
    }

    fn lowered_method_supported_for_type(&self, ty: &Type, name: Name, arg_count: usize) -> bool {
        lowered_method_supported_for_type(ty, name, arg_count)
    }

    fn loop_item_checked_type(&self, iter: ExprId) -> Option<Type> {
        self.checked_expr_type(iter)
            .or_else(|| self.concrete_checked_type(iter))
            .or_else(|| self.bodies.expr_types.get(&iter).cloned())
            .and_then(|ty| ty.iteration_item_type())
    }

    /// A set where a loop, a comprehension, or a pipeline reads items: its
    /// elements in key order.
    fn set_as_list(&mut self, set: BuildExprId, span: Span) -> BuildExprId {
        push_build_row!(
            self,
            expr,
            BuildExprRow::Method {
                receiver: set,
                name: Name::intern("to_list").as_str(),
                args: Vec::new(),
                span,
            }
        )
    }

    /// The set of a lowered list's elements.
    fn list_as_set(&mut self, elements: Vec<BuildExprId>, span: Span) -> BuildExprId {
        let list = push_build_row!(self, expr, BuildExprRow::List(elements));
        self.set_of_list(list, span)
    }

    fn set_of_list(&mut self, list: BuildExprId, span: Span) -> BuildExprId {
        push_build_row!(
            self,
            expr,
            BuildExprRow::Method {
                receiver: list,
                name: Name::intern("to_set").as_str(),
                args: Vec::new(),
                span,
            }
        )
    }

    fn lower_direct_iterable(
        &mut self,
        iter: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let checked = self
            .checked_expr_type(iter)
            .or_else(|| self.concrete_checked_type(iter))
            .or_else(|| self.bodies.expr_types.get(&iter).cloned());
        let lowered = self.lower_expr(iter, slots, current_function, item_slot)?;
        if matches!(checked, Some(Type::Set(_))) {
            let span = self.program.arena.expr(iter).span;
            return Some(self.set_as_list(lowered, span));
        }
        if matches!(checked, Some(Type::Result(ok, _)) if matches!(ok.as_ref(), Type::Map(_, _) | Type::Str | Type::Bytes))
        {
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: lowered,
                    span: self.program.arena.expr(iter).span
                }
            ))
        } else {
            Some(lowered)
        }
    }

    /// Extract the concrete ok and err payload types from a match scrutinee so
    /// `Ok(binding)`/`Err(binding)` arms can declare their slot with a checked
    /// type. Returns `None` for either side when the scrutinee is not a
    /// `Result` or when that payload type is not concrete, so callers keep the
    /// previous untyped-slot behavior instead of forcing `Any`.
    fn compact_match_scrutinee_result_types(
        &self,
        scrutinee: ExprId,
    ) -> (Option<Type>, Option<Type>) {
        let scrutinee_ty = self
            .checked_expr_type(scrutinee)
            .or_else(|| self.concrete_checked_type(scrutinee));
        match scrutinee_ty {
            Some(Type::Result(ok, err)) => {
                let ok_ty = compact_checked_type_is_concrete(&ok).then(|| ok.as_ref().clone());
                let err_ty = compact_checked_type_is_concrete(&err).then(|| err.as_ref().clone());
                (ok_ty, err_ty)
            }
            _ => (None, None),
        }
    }

    /// The checker's published type for `value`.
    fn checked_expr_type(&self, value: ExprId) -> Option<Type> {
        self.bodies
            .expr_types
            .get(&value)
            .filter(|ty| checked_fact_is_resolved(ty))
            .cloned()
    }

    // Conditions and returned Bool values keep ordinary short-circuit behavior;
    // only statement assertions ask the chain to retain failed pair values.
    fn mark_comparison_chain_assertion(&mut self, value: BuildExprId) {
        if let BuildExprRow::ComparisonChain { assertion, .. } =
            &mut self.scratch.borrow_mut().expressions[value.index()]
        {
            *assertion = true;
        }
    }

    fn is_empty_record_in_map_context(&self, value: ExprId, ty: Option<TypeExprId>) -> bool {
        ty.is_some_and(|ty| self.program.arena.type_expr_tags[ty.index()] == ArenaTypeExprTag::Map)
            && matches!(self.program.arena.expr(value).kind, ArenaExprKind::Record(fields) if self.program.arena.record_fields(fields).is_empty())
    }

    fn lowered_return_kind(&self, ty: TypeExprId) -> Option<LoweredReturnKind> {
        let tag = self.program.arena.type_expr_tags[ty.index()];
        let data = self.program.arena.type_expr_data[ty.index()];
        if tag == ArenaTypeExprTag::Result {
            return Some(LoweredReturnKind::Result(lowered_arena_type(
                &self.program.arena,
                TypeExprId::from_index(data.lhs as usize),
                self.declarations,
            )?));
        }
        if tag == ArenaTypeExprTag::Optional
            && self.program.arena.type_expr_tags[data.lhs as usize] == ArenaTypeExprTag::Result
        {
            return Some(LoweredReturnKind::OptionalResult);
        }
        Some(LoweredReturnKind::Plain(lowered_arena_type(
            &self.program.arena,
            ty,
            self.declarations,
        )?))
    }

    fn lower_tail_block(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let Some((&tail, prefix)) = ids.split_last() else {
            return Some(Vec::new());
        };
        let tail = self.program.arena.core_stmt_id(tail);
        let mut lowered = Vec::with_capacity(ids.len());
        for stmt in prefix {
            lowered.push(self.lower_stmt_with_blocker_guard(
                *stmt,
                slots,
                current_function,
                item_slot,
            )?);
        }
        if self.bodies.statement_positions.get(&tail)
            == Some(&crate::sema::check::StatementPosition::Statement)
        {
            lowered.push(self.lower_stmt_with_blocker_guard(
                tail,
                slots,
                current_function,
                item_slot,
            )?);
            return Some(lowered);
        }
        let tail = match self.program.arena.stmt(tail).kind {
            ArenaStmtKind::Expr(expr) => push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: self.lower_expr(expr, slots, current_function, item_slot)?,
                }
            ),
            ArenaStmtKind::Exit(status) => {
                let span = self.program.arena.stmt(tail).span;
                push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Return {
                        value: self.lower_exit(status, span, slots, current_function, item_slot)?,
                    }
                )
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let value = self
                    .lower_bare_ident_stmt(tail, name, slots)
                    .unwrap_or(push_build_row!(self, expr, BuildExprRow::Unit));
                push_build_row!(self, stmt, BuildStmtRow::Return { value })
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => match self.lower_tail_if_stmt(
                branches,
                else_block,
                slots,
                current_function,
                item_slot,
            ) {
                Some(stmt) => stmt,
                None => {
                    return self
                        .lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)
                        .map(|stmt| {
                            lowered.push(stmt);
                            lowered
                        });
                }
            },
            ArenaStmtKind::Match { value, arms } => push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: match self.lower_match_stmt_as_expr(
                        value,
                        arms,
                        self.program.arena.stmt(tail).span,
                        slots,
                        current_function,
                        item_slot,
                    ) {
                        Some(value) => value,
                        None => {
                            // Retain statement control flow when an arm cannot
                            // supply a lowered value expression.
                            if let Some(stmt) = self.lower_tail_match_stmt(
                                value,
                                arms,
                                self.program.arena.stmt(tail).span,
                                slots,
                                current_function,
                                item_slot,
                            ) {
                                lowered.push(stmt);
                                return Some(lowered);
                            }
                            return self
                                .lower_stmt_with_blocker_guard(
                                    tail,
                                    slots,
                                    current_function,
                                    item_slot,
                                )
                                .map(|stmt| {
                                    lowered.push(stmt);
                                    lowered
                                });
                        }
                    },
                }
            ),
            // A tail `run.text cmd` supplies the body's value.
            ArenaStmtKind::Command(command_id)
                if self.bodies.statement_positions.get(&tail)
                    == Some(&crate::sema::check::StatementPosition::Value) =>
            {
                if matches!(
                    self.program.arena.command_stmt(command_id).command,
                    ArenaCommand::Run(_)
                ) {
                    let value =
                        self.lower_tail_stmt_as_expr(tail, slots, current_function, item_slot)?;
                    push_build_row!(self, stmt, BuildStmtRow::Return { value })
                } else {
                    // Every other command's value is Unit. A return type that
                    // accepts the Unit fallthrough (checked before lowering)
                    // runs the command as the statement it is and completes
                    // with that Unit.
                    self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?
                }
            }
            _ => self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?,
        };
        lowered.push(tail);
        Some(lowered)
    }

    fn lower_block(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        if !self.program.arena.block(block).params.is_empty() {
            return Some(Vec::new());
        }
        let saved = slots.enter();
        let lowered = self.lower_block_in_current_scope(block, slots, current_function, item_slot);
        slots.exit(saved);
        lowered
    }

    /// `items`, a list, appended to the local of the `collect` expression
    /// that the checker gave this yield. That expression encloses the yield
    /// in the same function, so its local is in scope.
    fn lower_collect_yield(
        &mut self,
        id: StmtId,
        items: BuildExprId,
        slots: &SlotScope,
    ) -> Option<BuildStmtId> {
        let collect = *self.bodies.collect_yields.get(&id)?;
        let slot = slots.resolve(collect_local(collect))?;
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Assign {
                slot,
                op: AssignOp::Add,
                value: items,
                check: None,
                span: self.program.arena.stmt(id).span,
            }
        ))
    }

    /// A retry attempt has its own lexical scope. Its implicit tail uses the
    /// distinct value flow so explicit returns and loop transfers keep their
    /// enclosing targets, while propagation failures remain retryable.
    fn lower_retry_block(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        if !self.program.arena.block(block).params.is_empty() {
            return Some(Vec::new());
        }
        let saved = slots.enter();
        let result = (|| {
            let statements = self.program.arena.block(block).statements;
            let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
            let Some((&tail, prefix)) = ids.split_last() else {
                return Some(Vec::new());
            };
            let mut lowered = Vec::with_capacity(ids.len());
            for stmt in prefix {
                lowered.push(self.lower_stmt_with_blocker_guard(
                    *stmt,
                    slots,
                    current_function,
                    item_slot,
                )?);
            }
            let tail_stmt = if self.bodies.statement_positions.get(&tail)
                == Some(&crate::sema::check::StatementPosition::Statement)
            {
                self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?
            } else if let Some(value) =
                self.lower_tail_stmt_as_expr(tail, slots, current_function, item_slot)
            {
                push_build_row!(self, stmt, BuildStmtRow::Value { value })
            } else {
                self.lower_stmt_with_blocker_guard(tail, slots, current_function, item_slot)?
            };
            lowered.push(tail_stmt);
            Some(lowered)
        })();
        slots.exit(saved);
        result
    }

    fn lower_block_in_current_scope(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildStmtId>> {
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let mut lowered = Vec::with_capacity(ids.len());
        for stmt in ids {
            lowered.push(self.lower_stmt_with_blocker_guard(
                stmt,
                slots,
                current_function,
                item_slot,
            )?);
        }
        Some(lowered)
    }

    fn lower_stmt_with_blocker_guard(
        &mut self,
        id: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let blockers_before = self.output.blocker_events;
        match self.lower_stmt(id, slots, current_function, item_slot) {
            Some(stmt) => Some(stmt),
            None => {
                if self.output.blocker_events == blockers_before {
                    self.record_lower_stmt_blocker(id);
                    self.output.constructed_statements += 1;
                    self.output.blocker_events += 1;
                }
                None
            }
        }
    }

    fn lower_stmt(
        &mut self,
        id: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        if let ArenaStmtKind::Sugar { expansion, .. } = self.program.arena.stmt(id).kind {
            return self.lower_stmt(expansion, slots, current_function, item_slot);
        }
        self.output.statements += 1;
        let lowered = match self.program.arena.stmt(id).kind {
            ArenaStmtKind::Sugar { .. } => unreachable!("lowered through its expansion above"),
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(value),
            } => {
                if let ArenaBindingTargetKind::Record { .. } =
                    self.program.arena.binding_target(target).kind
                {
                    let checked = self.lower_binding_checked_type(ty, value);
                    let source = self.lower_binding_expr_value(
                        ty,
                        checked.as_ref(),
                        value,
                        self.program.arena.stmt(id).span,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let target = self.lower_comp_target_typed(target, slots, checked.as_ref())?;
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::LetRecord {
                            source,
                            target,
                            span: self.program.arena.stmt(id).span,
                        }
                    ));
                }
                let name = simple_binding_target(self.program, target)?;
                if is_discard_name(name) {
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Expr {
                            value: self.lower_expr(value, slots, current_function, item_slot)?,
                            span: self.program.arena.stmt(id).span,
                        }
                    ));
                }
                // A declaration may shadow a binding an enclosing scope made:
                // the inner scope resolves the new slot and the outer binding
                // comes back when it ends. Only a name this same scope already
                // declared is refused, which the checker reports before
                // lowering ever runs.
                if slots.is_declared_here(name) {
                    {
                        self.record_lower_stmt_blocker(id);
                        self.output.constructed_statements += 1;
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Expr {
                                value: push_build_row!(self, expr, BuildExprRow::Unit),
                                span: self.program.arena.stmt(id).span,
                            }
                        ));
                    }
                }
                #[cfg(debug_assertions)]
                if let Some(ty) = ty {
                    self.note_annotation_drift(id, ty);
                }
                let binding_ty = self
                    .declarations
                    .local_binding_types
                    .get(&self.program.arena.stmt(id).span)
                    .cloned()
                    .or_else(|| self.lower_binding_checked_type(ty, value));
                if let Some(ty) = ty
                    && lowered_arena_type(&self.program.arena, ty, self.declarations).is_none()
                    && !matches!(binding_ty, Some(ref ty) if !matches!(ty, Type::Unknown | Type::Invalid))
                {
                    return None;
                }
                let value = if self.is_empty_record_in_map_context(value, ty) {
                    push_build_row!(self, expr, BuildExprRow::EmptyMap)
                } else {
                    self.lower_binding_expr_value(
                        ty,
                        binding_ty.as_ref(),
                        value,
                        self.program.arena.stmt(id).span,
                        slots,
                        current_function,
                        item_slot,
                    )?
                };
                let binding_is_int = matches!(binding_ty, Some(Type::Int));
                let slot = slots.declare_with_type(name, binding_ty);
                if binding_is_int
                    && let Some(value) = self.lower_int_expr_candidate(&value)
                    && !self.lowered_int_expr_needs_type_context(&value)
                {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::LetInt { slot, value }
                    ))
                } else if let Some(value) = self.lower_bool_expr_candidate(&value)
                    && !self.lowered_bool_expr_needs_type_context(&value)
                {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::LetBool { slot, value }
                    ))
                } else {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Let { slot, value }
                    ))
                }
            }
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Const {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Run(run),
            } => {
                let name = simple_binding_target(self.program, target)?;
                if is_discard_name(name) {
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Expr {
                            value: self.lower_run_binding_value(
                                run,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            span: self.program.arena.stmt(id).span,
                        }
                    ));
                }
                // A declaration may shadow a binding an enclosing scope made:
                // the inner scope resolves the new slot and the outer binding
                // comes back when it ends. Only a name this same scope already
                // declared is refused, which the checker reports before
                // lowering ever runs.
                if slots.is_declared_here(name) {
                    {
                        self.record_lower_stmt_blocker(id);
                        self.output.constructed_statements += 1;
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Expr {
                                value: push_build_row!(self, expr, BuildExprRow::Unit),
                                span: self.program.arena.stmt(id).span,
                            }
                        ));
                    }
                }
                if let Some(ty) = ty {
                    lowered_arena_type(&self.program.arena, ty, self.declarations)?;
                }
                let value =
                    self.lower_run_binding_value(run, slots, current_function, item_slot)?;
                let binding_ty = ty
                    .map(|ty| {
                        compact_runtime_type_in_namespace(
                            &self.program.arena,
                            ty,
                            self.declarations,
                            self.current_namespace,
                        )
                    })
                    .or_else(|| {
                        self.declarations
                            .local_binding_types
                            .get(&self.program.arena.stmt(id).span)
                            .cloned()
                    });
                let slot = slots.declare_with_type(name, binding_ty);
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let { slot, value }
                ))
            }
            ArenaStmtKind::Assign { target, value, .. }
                if let ArenaAssignTargetKind::Env(name) =
                    self.program.arena.assign_target(target).kind =>
            {
                self.lower_env_assignment(id, name, value, slots, current_function, item_slot)
            }
            ArenaStmtKind::Assign { target, op, value } => {
                let root_name = self.assign_target_root_name(target)?;
                let slot = if let Some(slot) = slots.resolve(root_name) {
                    slot
                } else if op == AssignOp::Set
                    && matches!(
                        self.program.arena.assign_target(target).kind,
                        ArenaAssignTargetKind::Name(_)
                    )
                {
                    slots.declare(root_name)
                } else {
                    {
                        self.record_lower_stmt_blocker(id);
                        self.output.constructed_statements += 1;
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Expr {
                                value: push_build_row!(self, expr, BuildExprRow::Unit),
                                span: self.program.arena.stmt(id).span,
                            }
                        ));
                    }
                };
                let check = self
                    .assign_target_checked_type(target, slots)
                    .filter(Type::has_unsigned_constraint)
                    .map(|ty| LoweredTypeCheck {
                        name: Arc::from(ty.to_string()),
                        ty,
                        schema: None,
                    });
                let value_is_int = match value {
                    ArenaExprOrRun::Expr(expr) => {
                        matches!(self.checked_expr_type(expr), Some(Type::Int))
                    }
                    ArenaExprOrRun::Run(_) => false,
                };
                let value = match value {
                    ArenaExprOrRun::Expr(expr) => {
                        self.lower_expr(expr, slots, current_function, item_slot)?
                    }
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, slots, current_function, item_slot)?
                    }
                };
                if check.is_some() {
                    let span = self.program.arena.stmt(id).span;
                    return Some(match self.program.arena.assign_target(target).kind {
                        ArenaAssignTargetKind::Name(_) => push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Assign {
                                slot,
                                op,
                                value,
                                check,
                                span
                            }
                        ),
                        _ => {
                            let path =
                                self.lower_assign_path(target, slots, current_function, item_slot)?;
                            push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignPath {
                                    slot,
                                    path: LoweredAssignPath(path),
                                    op,
                                    value,
                                    check,
                                    span
                                }
                            )
                        }
                    });
                }
                match self.program.arena.assign_target(target).kind {
                    ArenaAssignTargetKind::Name(_) if op == AssignOp::Set => {
                        if let Some(value) = self.lower_bool_expr_candidate(&value)
                            && !self.lowered_bool_expr_needs_type_context(&value)
                        {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignBool { slot, value }
                            ))
                        } else if value_is_int
                            && let Some(value) = self.lower_int_expr_candidate(&value)
                            && !self.lowered_int_expr_needs_type_context(&value)
                        {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignInt {
                                    slot,
                                    op,
                                    value,
                                    span: self.program.arena.stmt(id).span,
                                }
                            ))
                        } else {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::Assign {
                                    slot,
                                    op,
                                    value,
                                    check: None,
                                    span: self.program.arena.stmt(id).span,
                                }
                            ))
                        }
                    }
                    ArenaAssignTargetKind::Name(_) => Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Assign {
                            slot,
                            op,
                            value,
                            check: None,
                            span: self.program.arena.stmt(id).span,
                        }
                    )),
                    ArenaAssignTargetKind::Field { base, name }
                        if matches!(
                            self.program.arena.assign_target(base).kind,
                            ArenaAssignTargetKind::Name(_)
                        ) =>
                    {
                        if value_is_int
                            && let Some(value) = self.lower_int_expr_candidate(&value)
                            && !self.lowered_int_expr_needs_type_context(&value)
                        {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignFieldInt {
                                    slot,
                                    field: Arc::<str>::from(name.as_str().as_str()),
                                    op,
                                    value,
                                    span: self.program.arena.stmt(id).span,
                                }
                            ))
                        } else {
                            Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::AssignField {
                                    slot,
                                    field: Arc::<str>::from(name.as_str().as_str()),
                                    op,
                                    value,
                                    span: self.program.arena.stmt(id).span,
                                }
                            ))
                        }
                    }
                    _ => {
                        let path =
                            self.lower_assign_path(target, slots, current_function, item_slot)?;
                        Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::AssignPath {
                                slot,
                                path: LoweredAssignPath(path),
                                op,
                                value,
                                check: None,
                                span: self.program.arena.stmt(id).span,
                            }
                        ))
                    }
                }
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                let branches = self.program.arena.if_branches(branches).to_vec();
                let has_pattern = branches.iter().any(|branch| {
                    matches!(
                        self.program.arena.expr(branch.condition).kind,
                        ArenaExprKind::PatternCondition { .. }
                    )
                });
                let mut lowered = Vec::with_capacity(branches.len());
                for branch in branches {
                    let saved = slots.enter();
                    let condition = self.lower_pattern_condition_parts(
                        branch.condition,
                        slots,
                        current_function,
                        item_slot,
                    );
                    let body = self.lower_block(branch.block, slots, current_function, item_slot);
                    slots.exit(saved);
                    let (condition, captures) = condition?;
                    lowered.push((condition, body?, captures));
                }
                let else_body = match else_block {
                    Some(block) => {
                        Some(self.lower_block(block, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                if lowered.iter().any(|(_, _, captures)| !captures.is_empty()) || has_pattern {
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::PatternIf {
                            branches: lowered,
                            else_body,
                            span: self.program.arena.stmt(id).span
                        }
                    ));
                }
                let lowered = lowered
                    .into_iter()
                    .map(|(condition, body, _)| (condition, body))
                    .collect::<Vec<_>>();
                let mut bool_branches = Vec::with_capacity(lowered.len());
                for (condition, body) in &lowered {
                    let Some(condition) = self.lower_bool_expr_candidate(condition) else {
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::If {
                                branches: lowered,
                                else_body,
                            }
                        ));
                    };
                    bool_branches.push((condition, body.clone()));
                }
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::IfBool {
                        branches: bool_branches,
                        else_body,
                    }
                ))
            }
            ArenaStmtKind::While { condition, block } => {
                let has_pattern = matches!(
                    self.program.arena.expr(condition).kind,
                    ArenaExprKind::PatternCondition { .. }
                );
                let saved = slots.enter();
                let condition = self.lower_pattern_condition_parts(
                    condition,
                    slots,
                    current_function,
                    item_slot,
                );
                let body = self.lower_block(block, slots, current_function, item_slot);
                slots.exit(saved);
                let (condition, captures) = condition?;
                let body = body?;
                if has_pattern {
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::PatternWhile {
                            condition,
                            body,
                            captures,
                            span: self.program.arena.stmt(id).span
                        }
                    ));
                }
                if let Some(condition) = self.lower_bool_expr_candidate(&condition) {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::WhileBool { condition, body }
                    ))
                } else {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::While { condition, body }
                    ))
                }
            }
            ArenaStmtKind::For {
                target,
                iter,
                block,
            } => {
                if let ArenaBindingTargetKind::Record { .. } =
                    self.program.arena.binding_target(target).kind
                {
                    if !self.program.arena.block(block).params.is_empty() {
                        return None;
                    }
                    let item_ty = self.loop_item_checked_type(iter);
                    let iter =
                        self.lower_direct_iterable(iter, slots, current_function, item_slot)?;
                    let saved = slots.enter();
                    let target = self.lower_comp_target_typed(target, slots, item_ty.as_ref())?;
                    let body =
                        self.lower_block_in_current_scope(block, slots, current_function, None)?;
                    slots.exit(saved);
                    return Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::ForRecord {
                            target,
                            iter,
                            body,
                            span: self.program.arena.stmt(id).span,
                        }
                    ));
                }
                let name = simple_binding_target(self.program, target)?;
                if !self.program.arena.block(block).params.is_empty() {
                    {
                        self.record_lower_stmt_blocker(id);
                        self.output.constructed_statements += 1;
                        return Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::Expr {
                                value: push_build_row!(self, expr, BuildExprRow::Unit),
                                span: self.program.arena.stmt(id).span,
                            }
                        ));
                    }
                }
                // Recognize `for line in <text>.lines()` and lower it to the
                // streaming ForStrLines node (avoids materializing the line list).
                let str_lines_base = if let ArenaExprKind::Call { callee, args } =
                    self.program.arena.expr(iter).kind
                {
                    if self.program.arena.call_args(args).is_empty() {
                        if let ArenaExprKind::Field { base, name } =
                            self.program.arena.expr(callee).kind
                        {
                            (name.as_str() == "lines").then_some(base)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                // The text/iter is evaluated once, before the loop scope opens.
                let text_or_iter = self.lower_direct_iterable(
                    str_lines_base.unwrap_or(iter),
                    slots,
                    current_function,
                    item_slot,
                )?;
                let item_ty = if let Some(base) = str_lines_base {
                    self.checked_expr_type(base)
                        .or_else(|| self.concrete_checked_type(base))
                        .and_then(|ty| match ty {
                            Type::Bytes => Some(Type::Bytes),
                            Type::Str => Some(Type::Str),
                            _ => None,
                        })
                        .or(Some(Type::Str))
                } else {
                    self.loop_item_checked_type(iter)
                };
                // The loop variable is declared in the loop's own scope, so it may
                // shadow an outer binding; `exit` restores the outer slot.
                let saved = slots.enter();
                let slot = slots.declare_with_type(name, item_ty);
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                let span = self.program.arena.stmt(id).span;
                if str_lines_base.is_some() {
                    let body = self.try_lower_scan_bytes(slot, &body, span).unwrap_or(body);
                    if let Some(scan) = self.try_lower_scan_lines(&text_or_iter, slot, &body, span)
                    {
                        Some(scan)
                    } else {
                        Some(push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::ForStrLines {
                                slot,
                                text: text_or_iter,
                                body,
                                span,
                            }
                        ))
                    }
                } else {
                    Some(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::For {
                            slot,
                            iter: text_or_iter,
                            body,
                            span,
                        }
                    ))
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                if let Some(stmt) = self.lower_str_match_stmt(
                    value,
                    arms,
                    self.program.arena.stmt(id).span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(stmt);
                }
                if let Some(stmt) = self.lower_tag_match_stmt(
                    value,
                    arms,
                    self.program.arena.stmt(id).span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(stmt);
                }
                let (ok_binding_ty, err_binding_ty) =
                    self.compact_match_scrutinee_result_types(value);
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                let arms = self.program.arena.match_arms(arms).to_vec();
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    if !self.program.arena.block(arm.block).params.is_empty() {
                        {
                            self.record_lower_stmt_blocker(id);
                            self.output.constructed_statements += 1;
                            return Some(push_build_row!(
                                self,
                                stmt,
                                BuildStmtRow::Expr {
                                    value: push_build_row!(self, expr, BuildExprRow::Unit),
                                    span: self.program.arena.stmt(id).span,
                                }
                            ));
                        }
                    }
                    let (pattern, cleanup) = self.lower_pattern(
                        arm.pattern,
                        slots,
                        ok_binding_ty.as_ref(),
                        err_binding_ty.as_ref(),
                    )?;
                    let body = match self.lower_block(arm.block, slots, current_function, item_slot)
                    {
                        Some(body) => body,
                        None => {
                            cleanup_lowered_pattern_slots(slots, cleanup);
                            {
                                self.record_lower_stmt_blocker(id);
                                self.output.constructed_statements += 1;
                                return Some(push_build_row!(
                                    self,
                                    stmt,
                                    BuildStmtRow::Expr {
                                        value: push_build_row!(self, expr, BuildExprRow::Unit),
                                        span: self.program.arena.stmt(id).span,
                                    }
                                ));
                            }
                        }
                    };
                    let guard = match arm.guard {
                        Some(guard_expr) => {
                            Some(self.lower_expr(guard_expr, slots, current_function, item_slot)?)
                        }
                        None => None,
                    };
                    cleanup_lowered_pattern_slots(slots, cleanup);
                    lowered_arms.push((pattern, guard, body));
                }
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Match {
                        value,
                        arms: lowered_arms,
                        span: self.program.arena.stmt(id).span,
                    }
                ))
            }
            ArenaStmtKind::With {
                bindings,
                body,
                else_block,
            } => {
                let saved = slots.enter();
                let result = (|| {
                    let mut lowered_bindings = Vec::new();
                    let mut captures = Vec::new();
                    let mut error_ty = None;
                    for binding in self.program.arena.with_bindings(bindings).to_vec() {
                        let checked = self.lower_binding_checked_type(None, binding.initializer);
                        let input = match self.program.arena.expr(binding.initializer).kind {
                            ArenaExprKind::Try(input) => input,
                            _ => binding.initializer,
                        };
                        if let Some(Type::Result(_, error)) =
                            self.lower_binding_checked_type(None, input)
                        {
                            error_ty = Some(match error_ty {
                                None => *error,
                                Some(previous) if previous == *error => previous,
                                Some(_) => Type::Error,
                            });
                        }
                        let value = self.lower_expr(
                            binding.initializer,
                            slots,
                            current_function,
                            item_slot,
                        )?;
                        let ty = checked.map(|ty| match ty {
                            Type::Result(ok, _) => *ok,
                            other => other,
                        });
                        let slot = if binding.name.as_str() == "_" {
                            slots.reserve("with discard")
                        } else {
                            slots.declare_with_type(binding.name, ty)
                        };
                        captures.push(slot);
                        lowered_bindings.push((slot, value));
                    }
                    let body = self.lower_block(body, slots, current_function, item_slot)?;
                    Some((lowered_bindings, body, captures, error_ty))
                })();
                slots.exit(saved);
                let (bindings, body, mut captures, error_ty) = result?;
                let error_ty = self
                    .bodies
                    .handler_input_types
                    .get(&else_block)
                    .cloned()
                    .or(error_ty);
                let saved = slots.enter();
                let else_param_slot = self
                    .program
                    .arena
                    .block_params(self.program.arena.block(else_block).params)
                    .first()
                    .filter(|param| param.name.as_str() != "_")
                    .map(|param| slots.declare_with_type(param.name, error_ty));
                captures.extend(else_param_slot);
                let else_body = self.lower_block_in_current_scope(
                    else_block,
                    slots,
                    current_function,
                    item_slot,
                );
                slots.exit(saved);
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::With {
                        bindings,
                        body,
                        else_param_slot,
                        else_body: else_body?,
                        captures,
                        span: self.program.arena.stmt(id).span
                    }
                ))
            }
            ArenaStmtKind::Guard {
                target,
                initializer,
                else_block,
                ..
            } => {
                let checked = match initializer {
                    ArenaExprOrRun::Expr(expr) => self.lower_binding_checked_type(None, expr),
                    ArenaExprOrRun::Run(_) => None,
                };
                // The checker decided whether this guard unwraps a `Result` or
                // tests an optional for `null`.
                let optional = self.bodies.optional_binding_guards.contains(&id);
                let success_ty = checked
                    .as_ref()
                    .and_then(|ty| guard_bound_type(ty, optional));
                let value = match initializer {
                    ArenaExprOrRun::Expr(expr) => {
                        self.lower_expr(expr, slots, current_function, item_slot)?
                    }
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, slots, current_function, item_slot)?
                    }
                };
                // The else block runs in its own scope with the error param
                // bound; the success binding lives in the enclosing scope.
                let saved = slots.enter();
                let else_param_slot = self
                    .program
                    .arena
                    .block_params(self.program.arena.block(else_block).params)
                    .first()
                    // A null optional carries no error to bind.
                    .filter(|param| !optional && param.name.as_str() != "_")
                    .map(|param| {
                        slots.declare_with_type(
                            param.name,
                            checked.as_ref().and_then(|ty| match ty {
                                Type::Result(_, error) => Some((**error).clone()),
                                _ => None,
                            }),
                        )
                    });
                let else_body = self.lower_block_in_current_scope(
                    else_block,
                    slots,
                    current_function,
                    item_slot,
                );
                slots.exit(saved);
                let else_body = else_body?;
                let target = self.lower_comp_target_typed(target, slots, success_ty)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Guard {
                        target,
                        value,
                        else_param_slot,
                        else_body,
                        span: self.program.arena.stmt(id).span,
                        optional,
                    }
                ))
            }
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(value))) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Return(Some(ArenaExprOrRun::Run(run))) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: self.lower_run_binding_value(run, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Return(None) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Return {
                    value: push_build_row!(self, expr, BuildExprRow::Unit),
                }
            )),
            // The checker decided that this yield appends to a `collect`
            // expression: it adds to that expression's local.
            ArenaStmtKind::YieldDelegate(value) if self.bodies.collect_yields.contains_key(&id) => {
                let items = self.lower_expr(value, slots, current_function, item_slot)?;
                self.lower_collect_yield(id, items, slots)
            }
            ArenaStmtKind::Yield(value) if self.bodies.collect_yields.contains_key(&id) => {
                let item = match value {
                    ArenaExprOrRun::Expr(value) => {
                        self.lower_expr(value, slots, current_function, item_slot)?
                    }
                    ArenaExprOrRun::Run(run) => {
                        self.lower_run_binding_value(run, slots, current_function, item_slot)?
                    }
                };
                let items = push_build_row!(self, expr, BuildExprRow::List(vec![item]));
                self.lower_collect_yield(id, items, slots)
            }
            ArenaStmtKind::YieldDelegate(value) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::YieldDelegate {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                    span: self.program.arena.stmt(id).span,
                }
            )),
            ArenaStmtKind::Yield(ArenaExprOrRun::Expr(value)) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Yield {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Yield(ArenaExprOrRun::Run(run)) => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Yield {
                    value: self.lower_run_binding_value(run, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Loop { block } => {
                let body = self.lower_block(block, slots, current_function, item_slot)?;
                Some(push_build_row!(self, stmt, BuildStmtRow::Loop { body }))
            }
            ArenaStmtKind::Break { value: None } => {
                Some(push_build_row!(self, stmt, BuildStmtRow::Break))
            }
            ArenaStmtKind::Break { value: Some(value) } => Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::BreakValue {
                    value: self.lower_expr(value, slots, current_function, item_slot)?,
                }
            )),
            ArenaStmtKind::Continue => Some(push_build_row!(self, stmt, BuildStmtRow::Continue)),
            ArenaStmtKind::Command(command) => self
                .lower_print_stmt(command, slots, current_function, item_slot)
                .or_else(|| self.lower_cd_stmt(command, slots, current_function, item_slot))
                .or_else(|| self.lower_env_stmt(command, slots, current_function, item_slot))
                .or_else(|| self.lower_run_stmt(command, slots, current_function, item_slot))
                .or_else(|| self.lower_proc_stmt(command, slots, current_function, item_slot)),
            ArenaStmtKind::Assert { condition, message } => {
                let span = self.program.arena.expr(condition).span;
                let value = self.lower_expr(condition, slots, current_function, item_slot)?;
                self.mark_comparison_chain_assertion(value);
                let message = match message {
                    Some(message) => {
                        Some(self.lower_expr(message, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Assert {
                        value,
                        message,
                        span
                    }
                ))
            }
            ArenaStmtKind::Expr(value) => {
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Expr { value, span }
                ))
            }
            ArenaStmtKind::Exit(status) => {
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_exit(status, span, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Expr { value, span }
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Expr(value), trigger) => {
                let value = self.lower_deferred_expr(value, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Defer {
                        value,
                        on_error: trigger == DeferTrigger::Error,
                    }
                ))
            }
            ArenaStmtKind::Defer(ArenaExprOrRun::Run(run), trigger) => {
                let value =
                    self.lower_run_binding_value(run, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Defer {
                        value,
                        on_error: trigger == DeferTrigger::Error,
                    }
                ))
            }
            ArenaStmtKind::TailBareIdent(name) => {
                let span = self.program.arena.stmt(id).span;
                let value = self.lower_bare_ident_stmt(id, name, slots)?;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Expr { value, span }
                ))
            }
            _ => None,
        };
        match lowered {
            Some(lowered) => {
                self.output.constructed_statements += 1;
                Some(lowered)
            }
            None => {
                self.record_lower_stmt_blocker(id);
                self.output.constructed_statements += 1;
                self.output.blocker_events += 1;
                Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Expr {
                        value: push_build_row!(self, expr, BuildExprRow::Unit),
                        span: self.program.arena.stmt(id).span,
                    }
                ))
            }
        }
    }

    fn record_lower_stmt_blocker(&mut self, id: StmtId) -> Option<BuildStmtId> {
        let id = self.program.arena.core_stmt_id(id);
        let kind = self.program.arena.stmt(id).kind;
        let stmt_span = self.program.arena.stmt(id).span;
        self.output.statement_blockers[compact_stmt_kind_index(kind)] += 1;
        let label = compact_stmt_blocker_label(self.program, id);
        let keep_nested = self.last_blocker_detail.as_ref().is_some_and(|(span, _)| {
            span.source_id == stmt_span.source_id
                && span.start() >= stmt_span.start()
                && span.end() <= stmt_span.end()
        });
        if !keep_nested {
            self.last_blocker_detail = Some((stmt_span, format!("statement `{label}`")));
        }
        record_compact_stmt_blocker_span(
            &mut self.output.statement_blocker_sample_spans,
            self.program,
            id,
        );
        None
    }

    fn lower_print_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Core {
            name,
            args,
            env,
            block,
        } = stmt.command
        else {
            return None;
        };
        if !matches!(name, CoreCommand::Print | CoreCommand::Eprint)
            || !env.is_empty()
            || block.is_some()
        {
            return None;
        }
        let args = self.program.arena.command_args(args).to_vec();
        let flush = args
            .first()
            .is_some_and(|arg| print_flush_arg(self.program, self.source, self.sources, arg));
        let print_args = if flush { &args[1..] } else { args.as_slice() };
        let mut lowered = Vec::with_capacity(print_args.len());
        for arg in print_args {
            lowered.push(self.lower_command_arg(arg, slots, current_function, item_slot)?);
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Print {
                args: lowered,
                stderr: name == CoreCommand::Eprint,
                flush,
                propagate_result: stmt.propagate,
                span: self.program.arena.span(stmt.span),
            }
        ))
    }

    fn lower_run_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Run(run) = stmt.command else {
            return None;
        };
        if lowered_arena_run_status_type(&self.program.arena, run).is_some() {
            let assert_success = compact_run_command_asserts_success(&self.program.arena, run);
            return Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Run {
                    value: self.lower_run_status_value(
                        run,
                        assert_success,
                        slots,
                        current_function,
                        item_slot,
                    )?,
                    propagate_result: assert_success,
                }
            ));
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Run {
                value: self.lower_run_binding_value(run, slots, current_function, item_slot)?,
                propagate_result: false,
            }
        ))
    }

    fn lower_cd_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Core {
            name: CoreCommand::Cd,
            args,
            env,
            block,
        } = stmt.command
        else {
            return None;
        };
        if !env.is_empty() {
            return None;
        }
        let args = self.program.arena.command_args(args);
        let [target] = args else {
            return None;
        };
        let target = self.lower_command_arg(target, slots, current_function, item_slot)?;
        let body = match block {
            Some(block) => self.lower_block(block, slots, current_function, item_slot)?,
            None => Vec::new(),
        };
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Cd {
                target,
                body,
                span: self.program.arena.span(stmt.span),
            }
        ))
    }

    fn lower_env_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Core {
            name: CoreCommand::Env,
            args,
            env,
            block,
        } = stmt.command
        else {
            return None;
        };
        if !self.program.arena.command_args(args).is_empty() {
            return None;
        }
        let assignments = self.program.arena.env_assignments(env).to_vec();
        let mut lowered_env = Vec::with_capacity(assignments.len());
        for assignment in &assignments {
            lowered_env.push(self.lower_run_env(assignment, slots, current_function, item_slot)?);
        }
        let body = match block {
            Some(block) => self.lower_block(block, slots, current_function, item_slot)?,
            None => Vec::new(),
        };
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Env {
                env: lowered_env,
                body,
            }
        ))
    }

    fn lower_proc_stmt(
        &mut self,
        id: crate::syntax::arena::CommandStmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let stmt = self.program.arena.command_stmt(id);
        let ArenaCommand::Proc { name, args } = stmt.command else {
            return None;
        };
        let name_text = name.as_str();
        let (module, api) = super::standard_module_command_name(name_text.as_str())?;
        let op = api_spec().module_op(module, api)?;
        let args = self.program.arena.command_args(args).to_vec();
        let mut lowered = Vec::with_capacity(args.len());
        for arg in &args {
            lowered.push(self.lower_command_arg(arg, slots, current_function, item_slot)?);
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Proc {
                op,
                args: lowered,
                propagate_result: stmt.propagate,
                span: self.program.arena.span(stmt.span),
            }
        ))
    }

    fn lower_command_arg(
        &mut self,
        arg: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        match &arg.kind {
            ArenaCommandArgKind::Typed(expr) => {
                self.lower_expr(*expr, slots, current_function, item_slot)
            }
            ArenaCommandArgKind::Word(parts) => {
                let span = self.program.arena.span(arg.span);
                let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
                if let [ArenaWordPart::Bare(text)] = parts.as_slice() {
                    let text = self.bare_text_value_in_span(text, span)?;
                    if let Some(value) = lower_command_word_reference(
                        text,
                        slots,
                        span,
                        &self.scratch,
                        &self.declarations.prepared_constants,
                        &self.declarations.wire_enums,
                        self.current_namespace,
                    ) {
                        return Some(value);
                    }
                }
                if let [ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr)] =
                    parts.as_slice()
                {
                    return self.lower_expr(*expr, slots, current_function, item_slot);
                }
                let mut lowered = Vec::with_capacity(parts.len());
                for part in parts {
                    match part {
                        ArenaWordPart::Bare(text) => {
                            lowered.push(LoweredFmtPart::Text(Arc::from(
                                self.bare_text_value_in_span(&text, span)?,
                            )));
                        }
                        ArenaWordPart::Quoted(text) => {
                            lowered.push(LoweredFmtPart::Text(Arc::from(
                                self.text_value_in_span(&text, span)?,
                            )));
                        }
                        ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr) => {
                            let span = self.program.arena.expr(expr).span;
                            lowered.push(LoweredFmtPart::Expr(
                                self.lower_expr(expr, slots, current_function, item_slot)?,
                                span,
                                None,
                            ));
                        }
                    }
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::FmtString(lowered)
                ))
            }
            ArenaCommandArgKind::SpliceName(name) => {
                self.lower_splice_name(*name, self.program.arena.span(arg.span), slots)
            }
            ArenaCommandArgKind::SpliceExpr(expr) => {
                self.lower_expr(*expr, slots, current_function, item_slot)
            }
        }
    }

    fn lower_spawn_expr(
        &mut self,
        target: ArenaSpawnTarget,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        match target {
            ArenaSpawnTarget::Command(command) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::SpawnCommand {
                    command: self.lower_expr(command, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaSpawnTarget::Run(run) => {
                let form = self.program.arena.run_form(run);
                if form.propagate {
                    return None;
                }
                let [segment] = self.program.arena.run_segments(form.segments) else {
                    return None;
                };
                if !matches!(segment.kind, RunKind::Plain | RunKind::Status) || segment.grouped {
                    return None;
                }
                let target = Box::new(self.lower_run_arg(
                    &segment.target,
                    slots,
                    current_function,
                    item_slot,
                )?);
                let args = self.program.arena.command_args(segment.args).to_vec();
                let mut lowered_args = Vec::with_capacity(args.len());
                for arg in &args {
                    lowered_args.push(self.lower_run_arg(
                        arg,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                let env = self.program.arena.env_assignments(segment.env).to_vec();
                let mut lowered_env = Vec::with_capacity(env.len());
                for assignment in &env {
                    lowered_env.push(self.lower_run_env(
                        assignment,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                let redirections = self
                    .program
                    .arena
                    .redirections(segment.redirections)
                    .to_vec()
                    .into_iter()
                    .map(|redirection| {
                        self.lower_run_redirection(&redirection, slots, current_function, item_slot)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let timeout = match segment.timeout {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let cpu_max = match segment.cpu_max {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let accept = match segment.accept {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let run = LoweredSpawnRun {
                    target,
                    args: lowered_args,
                    env: lowered_env,
                    redirections,
                    timeout,
                    cpu_max,
                    accept,
                    span,
                };
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::SpawnRun(Box::new(run))
                ))
            }
        }
    }

    fn lower_run_binding_value(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate(
            id,
            slots,
            current_function,
            item_slot,
            true,
            lowered_run_binding_type,
        )
    }

    fn lower_run_status_value(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        assert_success: bool,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate_and_assert(
            id,
            slots,
            current_function,
            item_slot,
            true,
            assert_success,
            lowered_run_status_type,
        )
    }

    fn lower_run_value_with_propagate(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        allow_propagate: bool,
        allowed_type: fn(RunKind) -> Option<LoweredType>,
    ) -> Option<BuildExprId> {
        self.lower_run_value_with_propagate_and_assert(
            id,
            slots,
            current_function,
            item_slot,
            allow_propagate,
            false,
            allowed_type,
        )
    }

    fn lower_run_value_with_propagate_and_assert(
        &mut self,
        id: crate::syntax::arena::RunFormId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        allow_propagate: bool,
        assert_success: bool,
        allowed_type: fn(RunKind) -> Option<LoweredType>,
    ) -> Option<BuildExprId> {
        let run = self.program.arena.run_form(id);
        if run.propagate && !allow_propagate {
            return None;
        }
        let segments = self.program.arena.run_segments(run.segments).to_vec();
        if segments.is_empty() {
            return None;
        }
        if segments.len() == 1 {
            let segment = &segments[0];
            allowed_type(segment.kind)?;
            let target = Box::new(self.lower_run_arg(
                &segment.target,
                slots,
                current_function,
                item_slot,
            )?);
            let args = self.program.arena.command_args(segment.args).to_vec();
            let mut lowered_args = Vec::with_capacity(args.len());
            for arg in &args {
                lowered_args.push(self.lower_run_arg(arg, slots, current_function, item_slot)?);
            }
            let env = self.program.arena.env_assignments(segment.env).to_vec();
            let mut lowered_env = Vec::with_capacity(env.len());
            for assignment in &env {
                lowered_env.push(self.lower_run_env(
                    assignment,
                    slots,
                    current_function,
                    item_slot,
                )?);
            }
            let redirections = self
                .program
                .arena
                .redirections(segment.redirections)
                .to_vec()
                .into_iter()
                .map(|redirection| {
                    self.lower_run_redirection(&redirection, slots, current_function, item_slot)
                })
                .collect::<Option<Vec<_>>>()?;
            let timeout = match segment.timeout {
                Some(expr) => Some(self.lower_expr(expr, slots, current_function, item_slot)?),
                None => None,
            };
            let cpu_max = match segment.cpu_max {
                Some(expr) => Some(self.lower_expr(expr, slots, current_function, item_slot)?),
                None => None,
            };
            let accept = match segment.accept {
                Some(expr) => Some(self.lower_expr(expr, slots, current_function, item_slot)?),
                None => None,
            };
            // Capture/stream kinds return a Result and are unwrapped by an
            // external `Try`. Plain/Status return a bare Status on success, so
            // `?` propagation is handled inside eval_lowered_run_capture via the
            // `propagate` flag instead.
            let capture_kind = lowered_run_capture_type(segment.kind).is_some();
            let propagate_internally = run.propagate && !capture_kind;
            let capture = LoweredRunCapture {
                kind: segment.kind,
                target,
                args: lowered_args,
                env: lowered_env,
                redirections,
                timeout,
                cpu_max,
                accept,
                propagate: propagate_internally,
                assert_success,
                span: self.program.arena.span(run.span),
            };
            let capture = push_build_row!(self, expr, BuildExprRow::RunCapture(Box::new(capture)));
            if run.propagate && capture_kind {
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: capture,
                        span: self.program.arena.span(run.span)
                    }
                ))
            } else {
                Some(capture)
            }
        } else {
            let mut lowered_segments = Vec::with_capacity(segments.len());
            for segment in &segments {
                allowed_type(segment.kind)?;
                let target =
                    self.lower_run_arg(&segment.target, slots, current_function, item_slot)?;
                let args = self.program.arena.command_args(segment.args).to_vec();
                let mut lowered_args = Vec::with_capacity(args.len());
                for arg in &args {
                    lowered_args.push(self.lower_run_arg(
                        arg,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                let env = self.program.arena.env_assignments(segment.env).to_vec();
                let mut lowered_env = Vec::with_capacity(env.len());
                for assignment in &env {
                    lowered_env.push(self.lower_run_env(
                        assignment,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                let redirections = self
                    .program
                    .arena
                    .redirections(segment.redirections)
                    .to_vec()
                    .into_iter()
                    .map(|redirection| {
                        self.lower_run_redirection(&redirection, slots, current_function, item_slot)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let timeout = match segment.timeout {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let cpu_max = match segment.cpu_max {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                let accept = match segment.accept {
                    Some(expr) => {
                        Some(self.lower_expr(expr, slots, current_function, item_slot)?)
                    }
                    None => None,
                };
                lowered_segments.push(LoweredRunPipelineSegment {
                    kind: segment.kind,
                    target,
                    args: lowered_args,
                    env: lowered_env,
                    redirections,
                    timeout,
                    cpu_max,
                    accept,
                });
            }
            // A capturing head yields a Result unwrapped by an external `Try`.
            // A statement-position status pipeline asserts success like a lone
            // `run`: it yields a Result the statement row propagates.
            let capture_kind = lowered_run_capture_type(segments[0].kind).is_some();
            let pipeline = push_build_row!(
                self,
                expr,
                BuildExprRow::RunPipeline {
                    segments: lowered_segments,
                    propagate: !capture_kind && (run.propagate || assert_success),
                    span: self.program.arena.span(run.span),
                }
            );
            if run.propagate && (capture_kind || !assert_success) {
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: pipeline,
                        span: self.program.arena.span(run.span)
                    }
                ))
            } else {
                Some(pipeline)
            }
        }
    }

    fn lower_run_redirection(
        &mut self,
        redirection: &crate::syntax::arena::ArenaRedirection,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredRunRedirection> {
        let target = match &redirection.target {
            crate::syntax::arena::ArenaRedirectionTarget::Path(target) => {
                self.lower_run_redirection_path_target(target, slots, current_function, item_slot)?
            }
            crate::syntax::arena::ArenaRedirectionTarget::Fd(target) => {
                self.lower_run_arg(target, slots, current_function, item_slot)?
            }
        };
        Some(LoweredRunRedirection {
            kind: redirection.kind,
            target,
            span: self.program.arena.span(redirection.span),
        })
    }

    fn lower_run_redirection_path_target(
        &mut self,
        target: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredRunArg> {
        if let ArenaCommandArgKind::Word(parts) = &target.kind {
            let span = self.program.arena.span(target.span);
            let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
            if let [ArenaWordPart::Bare(text)] = parts.as_slice() {
                let text = self.bare_text_value_in_span(text, span)?;
                if let Some(slot) = slots.resolve(Name::intern(text)) {
                    return Some(LoweredRunArg {
                        kind: LoweredRunArgKind::Single(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Param(slot)
                        )),
                        span,
                    });
                }
            }
        }
        self.lower_run_arg(target, slots, current_function, item_slot)
    }

    fn lower_run_env(
        &mut self,
        assignment: &crate::syntax::arena::ArenaEnvAssignment,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredRunEnv> {
        let value = match &assignment.value {
            crate::syntax::arena::ArenaEnvAssignmentValue::CommandArg(arg) => {
                if matches!(
                    arg.kind,
                    ArenaCommandArgKind::SpliceName(_) | ArenaCommandArgKind::SpliceExpr(_)
                ) {
                    return None;
                }
                self.lower_run_arg(arg, slots, current_function, item_slot)?
            }
            crate::syntax::arena::ArenaEnvAssignmentValue::Expr(expr) => LoweredRunArg {
                kind: LoweredRunArgKind::Single(self.lower_expr(
                    *expr,
                    slots,
                    current_function,
                    item_slot,
                )?),
                span: self.program.arena.expr(*expr).span,
            },
        };
        Some(LoweredRunEnv {
            name: assignment.name,
            value,
        })
    }

    fn lower_run_arg(
        &mut self,
        arg: &crate::syntax::arena::ArenaCommandArg,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredRunArg> {
        let span = self.program.arena.span(arg.span);
        let kind =
            match &arg.kind {
                ArenaCommandArgKind::Typed(expr) => LoweredRunArgKind::Single(self.lower_expr(
                    *expr,
                    slots,
                    current_function,
                    item_slot,
                )?),
                ArenaCommandArgKind::Word(parts) => {
                    let parts = self.program.arena.word_parts(*parts).collect::<Vec<_>>();
                    if let [ArenaWordPart::Shorthand(expr) | ArenaWordPart::Interpolation(expr)] =
                        parts.as_slice()
                    {
                        LoweredRunArgKind::SingleOrSplice(self.lower_expr(
                            *expr,
                            slots,
                            current_function,
                            item_slot,
                        )?)
                    } else {
                        let mut lowered = Vec::with_capacity(parts.len());
                        for part in parts {
                            match part {
                                ArenaWordPart::Bare(text) => {
                                    lowered.push(LoweredFmtPart::Text(Arc::from(
                                        self.bare_text_value_in_span(&text, span)?,
                                    )));
                                }
                                ArenaWordPart::Quoted(text) => {
                                    lowered.push(LoweredFmtPart::Text(Arc::from(
                                        self.text_value_in_span(&text, span)?,
                                    )));
                                }
                                ArenaWordPart::Shorthand(expr)
                                | ArenaWordPart::Interpolation(expr) => {
                                    lowered.push(LoweredFmtPart::Expr(
                                        self.lower_expr(expr, slots, current_function, item_slot)?,
                                        self.program.arena.expr(expr).span,
                                        None,
                                    ));
                                }
                            }
                        }
                        LoweredRunArgKind::Single(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::PathFmtString {
                                parts: lowered,
                                span
                            }
                        ))
                    }
                }
                ArenaCommandArgKind::SpliceName(name) => LoweredRunArgKind::Splice(
                    self.lower_splice_name(*name, self.program.arena.span(arg.span), slots)?,
                ),
                ArenaCommandArgKind::SpliceExpr(expr) => LoweredRunArgKind::Splice(
                    self.lower_expr(*expr, slots, current_function, item_slot)?,
                ),
            };
        Some(LoweredRunArg { kind, span })
    }

    fn lower_optional_expr(
        &mut self,
        id: Option<ExprId>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Option<BuildExprId>> {
        match id {
            Some(id) => Some(Some(self.lower_expr(
                id,
                slots,
                current_function,
                item_slot,
            )?)),
            None => Some(None),
        }
    }

    fn lower_postfix_receiver(
        &mut self,
        base: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        if let Some(receiver) = slots.postfix_receivers.get(&base) {
            return Some(*receiver);
        }
        let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Try {
                value: receiver,
                span: self.program.arena.expr(base).span
            }
        ))
    }

    fn lower_optional_postfix(
        &mut self,
        id: ExprId,
        base: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let span = self.program.arena.expr(id).span;
        let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
        let slot = slots.reserve("optional receiver");
        let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
        let previous = slots.postfix_receivers.insert(base, bound);
        slots.guarded_postfixes.insert(id);
        let selected = self.lower_expr(id, slots, current_function, item_slot);
        slots.guarded_postfixes.remove(&id);
        match previous {
            Some(previous) => {
                slots.postfix_receivers.insert(base, previous);
            }
            None => {
                slots.postfix_receivers.remove(&base);
            }
        }
        let selected = selected?;
        let absent = push_build_row!(self, expr, BuildExprRow::Null);
        let null_pattern =
            push_build_row!(self, pattern, BuildPatternRow::Literal(LoweredValue::Null));
        let present_pattern = push_build_row!(self, pattern, BuildPatternRow::Bind { slot });
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::MatchExpr {
                value: receiver,
                arms: vec![
                    (null_pattern, None, absent),
                    (present_pattern, None, selected)
                ],
                span,
            }
        ))
    }

    fn lower_pattern_condition_parts(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(BuildExprId, Vec<usize>)> {
        let ArenaExprKind::PatternCondition { value, arms } = self.program.arena.expr(id).kind
        else {
            return Some((
                self.lower_expr(id, slots, current_function, item_slot)?,
                Vec::new(),
            ));
        };
        let span = self.program.arena.expr(id).span;
        let (ok_ty, err_ty) = self.compact_match_scrutinee_result_types(value);
        // Resolve the subject before installing captures that may shadow it.
        let subject = self.lower_expr(value, slots, current_function, item_slot)?;
        let pattern = self.program.arena.match_expr_arms(arms)[0].pattern;
        let (pattern, cleanup) =
            self.lower_pattern(pattern, slots, ok_ty.as_ref(), err_ty.as_ref())?;
        let captures = cleanup.into_iter().map(|(_, slot)| slot).collect();
        let wildcard = push_build_row!(self, pattern, BuildPatternRow::Wildcard);
        let yes = push_build_row!(self, expr, BuildExprRow::Bool(true));
        let no = push_build_row!(self, expr, BuildExprRow::Bool(false));
        let mut arms = vec![(pattern, None, yes), (wildcard, None, no)];
        if self.bodies.optional_binding_conditions.contains(&id) {
            // An optional binding fails on `null` before its pattern, which
            // would otherwise accept and bind it.
            let null_pattern =
                push_build_row!(self, pattern, BuildPatternRow::Literal(LoweredValue::Null));
            let absent = push_build_row!(self, expr, BuildExprRow::Bool(false));
            arms.insert(0, (null_pattern, None, absent));
        }
        Some((
            push_build_row!(
                self,
                expr,
                BuildExprRow::MatchExpr {
                    value: subject,
                    arms,
                    span
                }
            ),
            captures,
        ))
    }

    fn lower_expr(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        // The checker decided that this condition operand's `Result[Bool]`
        // propagates. It is the operand under a `?`, and nothing here decides
        // that again.
        if slots.propagated_condition != Some(id)
            && self.bodies.propagating_conditions.contains(&id)
        {
            let outer = slots.propagated_condition.replace(id);
            let operand = self.lower_expr(id, slots, current_function, item_slot);
            slots.propagated_condition = outer;
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: operand?,
                    span: self.program.arena.expr(id).span
                }
            ));
        }
        if let Some(value) = self.declarations.prepared_constants.values.get(&id) {
            let origin = self
                .declarations
                .prepared_constants
                .origins
                .get(&id)
                .copied()
                .unwrap_or(id);
            let cached = self
                .scratch
                .borrow()
                .prepared_constants
                .get(&origin)
                .cloned();
            let value = if let Some(value) = cached {
                value
            } else {
                let value = lower_literal_constant(value, Some(&self.declarations.wire_enums))?;
                self.scratch
                    .borrow_mut()
                    .prepared_constants
                    .insert(origin, value.clone());
                value
            };
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::PreparedConstant(super::PreparedConstantValue(value))
            ));
        }
        if let Some(receiver) = slots.postfix_receivers.get(&id) {
            return Some(*receiver);
        }
        if !slots.guarded_postfixes.contains(&id) {
            let base = match self.program.arena.expr(id).kind {
                ArenaExprKind::NullSafeField { base, .. }
                | ArenaExprKind::Index {
                    base,
                    guarded: true,
                    ..
                }
                | ArenaExprKind::Slice {
                    base,
                    guarded: true,
                    ..
                } => Some(base),
                ArenaExprKind::Call { callee, .. } => match self.program.arena.expr(callee).kind {
                    ArenaExprKind::NullSafeField { base, .. } => Some(base),
                    _ => None,
                },
                _ => None,
            };
            if let Some(base) = base {
                let ty = self
                    .checked_expr_type(base)
                    .or_else(|| {
                        self.bodies
                            .expr_types
                            .get(&base)
                            .filter(|ty| compact_checked_type_is_concrete(ty))
                            .cloned()
                    })
                    .or_else(|| self.concrete_checked_type(base));
                if matches!(ty, Some(Type::Optional(_))) {
                    return self.lower_optional_postfix(
                        id,
                        base,
                        slots,
                        current_function,
                        item_slot,
                    );
                }
            }
        }
        self.output.expressions += 1;
        let span = self.program.arena.expr(id).span;
        let checked_creation = matches!(
            self.program.arena.expr(id).kind,
            ArenaExprKind::List(_)
                | ArenaExprKind::ListComp { .. }
                | ArenaExprKind::MapComp { .. }
                | ArenaExprKind::Record(_)
                | ArenaExprKind::If { .. }
                | ArenaExprKind::Match { .. }
                | ArenaExprKind::Binary {
                    op: BinaryOp::ResultFallback,
                    ..
                }
                | ArenaExprKind::Call { .. }
        )
        .then(|| self.bodies.expr_types.get(&id).cloned())
        .flatten()
        .filter(Type::has_unsigned_constraint);
        let lowered = match self.program.arena.expr(id).kind {
            ArenaExprKind::Field { .. } | ArenaExprKind::Call { .. }
                if self.bodies.inferred_variants.contains_key(&id) =>
            {
                self.lower_inferred_variant(id, slots, current_function, item_slot)
            }
            ArenaExprKind::Null => Some(push_build_row!(self, expr, BuildExprRow::Null)),
            ArenaExprKind::Int(value) => self
                .program
                .arena
                .int_literal(value)
                .value()
                .map(|value| push_build_row!(self, expr, BuildExprRow::Int(value))),
            ArenaExprKind::Float(value) => self
                .program
                .arena
                .float_literal(value)
                .value()
                .map(crate::runtime::value::FloatValue::new)
                .map(|value| push_build_row!(self, expr, BuildExprRow::Float(value))),
            ArenaExprKind::Duration(value) => self
                .program
                .arena
                .duration_literal(value)
                .millis()
                .map(|millis| {
                    push_build_row!(self, expr, BuildExprRow::Duration(DurationValue { millis }))
                }),
            ArenaExprKind::Bool(value) => {
                Some(push_build_row!(self, expr, BuildExprRow::Bool(value)))
            }
            // A literal the checker typed as a Path is a Path constant, the
            // same value `p"..."` builds.
            ArenaExprKind::Str(value) if self.bodies.path_literals.contains(&id) => {
                let path = self.program.arena.string_literal(value);
                PathValue::from_text(path.as_ref())
                    .ok()
                    .map(|value| push_build_row!(self, expr, BuildExprRow::Path(value)))
            }
            ArenaExprKind::Str(value) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Str(self.program.arena.string_literal(value).clone(),)
            )),
            ArenaExprKind::PathStr(value) => {
                let path = self.program.arena.string_literal(value);
                PathValue::from_text(path.as_ref())
                    .ok()
                    .map(|value| push_build_row!(self, expr, BuildExprRow::Path(value)))
            }
            ArenaExprKind::Regex(value) => {
                let literal = self.program.arena.regex_literal(value);
                let regex = literal.prepared.get()?.as_ref().ok()?.clone();
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::PreparedRegex(RegexValue {
                        pattern: literal.pattern.to_string(),
                        regex,
                    })
                ))
            }
            ArenaExprKind::Bytes(value) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Bytes(self.program.arena.bytes_literal(value).clone(),)
            )),
            ArenaExprKind::Ident(name) => self.lower_bare_ident(name, slots),
            ArenaExprKind::Item => {
                item_slot.map(|slot| push_build_row!(self, expr, BuildExprRow::Param(slot)))
            }
            ArenaExprKind::FmtString(parts) => {
                self.lower_fmt_string(parts, slots, current_function, item_slot)
            }
            ArenaExprKind::PathFmtString(parts) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::PathFmtString {
                    parts: self.lower_fmt_parts(parts, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaExprKind::GlobStr(value) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Glob {
                    pattern: self.program.arena.string_literal(value).clone(),
                    span,
                }
            )),
            ArenaExprKind::LastStatus => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::LastStatus { span }
            )),
            ArenaExprKind::Run(run) => {
                self.lower_run_binding_value(run, slots, current_function, item_slot)
            }
            ArenaExprKind::Spawn(form) => {
                self.lower_spawn_expr(form.target, span, slots, current_function, item_slot)
            }
            ArenaExprKind::Wait(wait) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Wait {
                    target: self.lower_expr(wait.target, slots, current_function, item_slot,)?,
                    span,
                }
            )),
            ArenaExprKind::Set(items) => {
                let items = self.program.arena.list_elements(items).collect::<Vec<_>>();
                let mut lowered = Vec::with_capacity(items.len());
                for item in items {
                    lowered.push(self.lower_expr(
                        item.value,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                Some(self.list_as_set(lowered, span))
            }
            ArenaExprKind::SetComp {
                expr: body,
                qualifiers,
            } => {
                let saved = slots.enter();
                let qualifiers =
                    self.lower_comp_qualifiers(qualifiers, slots, current_function, item_slot)?;
                let value = self.lower_expr(body, slots, current_function, item_slot)?;
                slots.exit(saved);
                let list = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListComp {
                        value,
                        qualifiers,
                        span
                    }
                );
                Some(self.set_of_list(list, span))
            }
            // Braces of bare names that the checker read as a set.
            ArenaExprKind::Record(fields)
                if matches!(self.bodies.expr_types.get(&id), Some(Type::Set(_))) =>
            {
                let mut lowered = Vec::new();
                for field in self.program.arena.record_fields(fields).to_vec() {
                    let ArenaRecordFieldKind::Shorthand { name, .. } = field.kind else {
                        return None;
                    };
                    lowered.push(self.lower_bare_ident(name, slots)?);
                }
                Some(self.list_as_set(lowered, span))
            }
            ArenaExprKind::Record(fields) => {
                if self
                    .program
                    .arena
                    .record_fields(fields)
                    .iter()
                    .any(|field| matches!(field.kind, ArenaRecordFieldKind::Path { .. }))
                {
                    self.lower_record(fields, slots, current_function, item_slot)
                } else if self
                    .bodies
                    .expr_types
                    .get(&id)
                    .is_some_and(|ty| matches!(ty, Type::Map(_, _)))
                    || self
                        .program
                        .arena
                        .record_fields(fields)
                        .iter()
                        .any(|field| matches!(field.kind, ArenaRecordFieldKind::Computed { .. }))
                {
                    self.lower_map_literal(id, fields, slots, current_function, item_slot)
                } else {
                    self.lower_record(fields, slots, current_function, item_slot)
                }
            }
            ArenaExprKind::List(items) => {
                let items = self.program.arena.list_elements(items).collect::<Vec<_>>();
                if items.iter().any(|item| item.splice_span.is_some()) {
                    let mut lowered = Vec::with_capacity(items.len());
                    for item in items {
                        let item_span = item
                            .splice_span
                            .map(|span| self.program.arena.span(span))
                            .unwrap_or(self.program.arena.expr(item.value).span);
                        lowered.push((
                            item.splice_span.is_some(),
                            self.lower_expr(item.value, slots, current_function, item_slot)?,
                            item_span,
                        ));
                    }
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ListBuild(lowered)
                    ))
                } else {
                    let mut lowered = Vec::with_capacity(items.len());
                    for item in items {
                        lowered.push(self.lower_expr(
                            item.value,
                            slots,
                            current_function,
                            item_slot,
                        )?);
                    }
                    Some(push_build_row!(self, expr, BuildExprRow::List(lowered)))
                }
            }
            ArenaExprKind::ListComp {
                expr: body,
                qualifiers,
            } => {
                let saved = slots.enter();
                let qualifiers =
                    self.lower_comp_qualifiers(qualifiers, slots, current_function, item_slot)?;
                let value = self.lower_expr(body, slots, current_function, item_slot)?;
                slots.exit(saved);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListComp {
                        value,
                        qualifiers,
                        span
                    }
                ))
            }
            ArenaExprKind::MapComp {
                key,
                value,
                qualifiers,
            } => {
                let saved = slots.enter();
                let qualifiers =
                    self.lower_comp_qualifiers(qualifiers, slots, current_function, item_slot)?;
                let key_span = self.program.arena.expr(key).span;
                let key = self.lower_expr(key, slots, current_function, item_slot)?;
                let key = if matches!(self.bodies.expr_types.get(&id), Some(Type::Map(key_ty, _)) if **key_ty == Type::UInt)
                {
                    self.require_uint_key(key, key_span)
                } else {
                    key
                };
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                slots.exit(saved);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MapComp {
                        key,
                        value,
                        qualifiers,
                        span
                    }
                ))
            }
            ArenaExprKind::StructuredPipeline { input, stages } => {
                let stages = self.program.arena.stream_stages(stages).to_vec();
                let mut lowered_stages = Vec::with_capacity(stages.len());
                for stage in &stages {
                    let input_ty = self
                        .declarations
                        .stream_stage_types
                        .get(&(self.current_namespace, self.program.arena.span(stage.span)))?
                        .input
                        .clone();
                    lowered_stages.push(self.lower_pipeline_stage(
                        stage,
                        slots,
                        current_function,
                        stream_item_type(&input_ty),
                    )?);
                }
                self.fuse_par_map_flat_map_reduce_by(&mut lowered_stages);
                let lowered_input = self.lower_expr(input, slots, current_function, item_slot)?;
                let lowered_input =
                    if matches!(self.bodies.expr_types.get(&input), Some(Type::Set(_))) {
                        let input_span = self.program.arena.expr(input).span;
                        self.set_as_list(lowered_input, input_span)
                    } else {
                        lowered_input
                    };
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ListPipeline {
                        input: lowered_input,
                        stages: lowered_stages,
                        span,
                    }
                ))
            }
            // Each conversion is an operation the language already has,
            // under `?`. The checker chose which; an expression it published
            // no conversion for was rejected and is not lowered.
            ArenaExprKind::Convert { value, target } => {
                let conversion = *self.bodies.conversions.get(&id)?;
                let value = self.lower_expr(value, slots, current_function, item_slot)?;
                let method = |name: &str| BuildExprRow::Method {
                    receiver: value,
                    name: Name::intern(name).as_str(),
                    args: Vec::new(),
                    span,
                };
                let path_from_bytes = |bytes| BuildExprRow::ModuleCall {
                    cli_plan: None,
                    op: RuntimeOp::PathParseBytes,
                    args: vec![Some(bytes)],
                    span,
                };
                let operation = match conversion {
                    Conversion::TextToInt => method("parse_int"),
                    Conversion::TextToUInt => method("parse_uint"),
                    Conversion::TextToFloat => method("parse_float"),
                    Conversion::BytesToText => method("utf8"),
                    Conversion::BytesToPath => path_from_bytes(value),
                    Conversion::TextToPath => path_from_bytes(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::BytesFromText,
                            args: vec![Some(value)],
                            span,
                        }
                    )),
                    Conversion::IntToUInt => BuildExprRow::Require {
                        value,
                        check: LoweredTypeCheck {
                            schema: Some(self.prepared_schema(Type::UInt)),
                            ty: Type::UInt,
                            name: Arc::from("UInt"),
                        },
                        span,
                    },
                    Conversion::IntToBounded => {
                        let ty = compact_runtime_type_in_namespace(
                            &self.program.arena,
                            target,
                            self.declarations,
                            self.current_namespace,
                        );
                        BuildExprRow::Require {
                            value,
                            check: LoweredTypeCheck {
                                schema: Some(self.prepared_schema(ty.clone())),
                                ty,
                                name: compact_type_expr_name(&self.program.arena, target),
                            },
                            span,
                        }
                    }
                };
                let operation = push_build_row!(self, expr, operation);
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: operation,
                        span
                    }
                ))
            }
            ArenaExprKind::Require { value, schema } => {
                let (ty, name) = if let Some(schema) = schema {
                    lowered_arena_type(&self.program.arena, schema, self.declarations)?;
                    (
                        compact_runtime_type_in_namespace(
                            &self.program.arena,
                            schema,
                            self.declarations,
                            self.current_namespace,
                        ),
                        compact_type_expr_name(&self.program.arena, schema),
                    )
                } else {
                    let target = self.bodies.requirement_targets.get(&id)?;
                    (target.ty.clone(), target.name.clone())
                };
                let prepared = self.prepared_schema(ty.clone());
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Require {
                        value: self.lower_expr(value, slots, current_function, item_slot)?,
                        check: LoweredTypeCheck {
                            schema: Some(prepared),
                            ty,
                            name
                        },
                        span,
                    }
                ))
            }
            // `0 - x` is only right for Int: a Float operand took the Int
            // fast path inside loops ("lowered expression expected Int") and
            // `0.0 - 0.0` loses the sign of `-0.0`, so Floats scale by -1.0.
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr,
            } if self.checked_expr_type(expr) == Some(Type::Float) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Binary {
                    op: BinaryOp::Mul,
                    left: self.lower_expr(expr, slots, current_function, item_slot)?,
                    right: push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Float(crate::runtime::value::FloatValue::new(-1.0))
                    ),
                    span,
                }
            )),
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr,
            } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Binary {
                    op: BinaryOp::Sub,
                    left: push_build_row!(self, expr, BuildExprRow::Int(0)),
                    right: self.lower_expr(expr, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaExprKind::Unary {
                op: UnaryOp::Not,
                expr,
            } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::IfExpr {
                    branches: vec![(
                        self.lower_expr(expr, slots, current_function, item_slot)?,
                        push_build_row!(self, expr, BuildExprRow::Bool(false)),
                    )],
                    else_value: push_build_row!(self, expr, BuildExprRow::Bool(true)),
                    span,
                }
            )),
            ArenaExprKind::ComparisonChain(pairs) => {
                let pairs = self.program.arena.expr_ids(pairs).collect::<Vec<_>>();
                let pairs = pairs
                    .into_iter()
                    .map(|pair| self.lower_expr(pair, slots, current_function, item_slot))
                    .collect::<Option<Vec<_>>>()?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ComparisonChain {
                        pairs,
                        assertion: false
                    }
                ))
            }
            ArenaExprKind::Binary { op, left, right } if lowered_binary_op(op) => {
                let uint_key = matches!(op, BinaryOp::In | BinaryOp::NotIn)
                    && matches!(self.checked_expr_type(right), Some(Type::Map(key, _)) if *key == Type::UInt);
                let lowered_left = self.lower_expr(left, slots, current_function, item_slot)?;
                let lowered_left = if uint_key {
                    self.require_uint_key(lowered_left, self.program.arena.expr(left).span)
                } else {
                    lowered_left
                };
                // Only the operands' checked types tell `s + s < s + s` on Str
                // from Int arithmetic; the int fast path compared Str slots as
                // Ints at runtime.
                let non_int = |operand: ExprId| {
                    self.bodies
                        .expr_types
                        .get(&operand)
                        .or(self.checked_expr_type(operand).as_ref())
                        .is_some_and(|ty| !matches!(ty, Type::Int | Type::UInt))
                };
                let non_int_operands = non_int(left)
                    || non_int(right)
                    || self.checked_expr_type(left) == Some(Type::Duration)
                    || self.checked_expr_type(right) == Some(Type::Duration);
                let value = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Binary {
                        op,
                        left: lowered_left,
                        right: self.lower_expr(right, slots, current_function, item_slot)?,
                        span,
                    }
                );
                if non_int_operands {
                    self.scratch
                        .borrow_mut()
                        .non_int_binary_expressions
                        .insert(value.index());
                }
                let ty = if op == BinaryOp::Add {
                    self.checked_expr_type(id)
                } else {
                    None
                };
                Some(match ty {
                    Some(ty) => self.checked_unsigned_value(value, &ty, span),
                    None => value,
                })
            }
            ArenaExprKind::Binary {
                op: BinaryOp::ResultFallback,
                left,
                right,
            } => {
                if self
                    .bodies
                    .proven_nonnull_fallback_receivers
                    .contains(&left)
                {
                    return self.lower_expr(left, slots, current_function, item_slot);
                }
                if let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(right).kind {
                    // A handler without `|name|` reads the error as its item `.`.
                    let parameter = match self
                        .program
                        .arena
                        .block_params(self.program.arena.block(block).params)
                    {
                        [] => None,
                        [parameter] => Some(parameter.name),
                        _ => return None,
                    };
                    let result_ty = self
                        .bodies
                        .expr_types
                        .get(&left)
                        .filter(|ty| compact_checked_type_is_concrete(ty))
                        .cloned()
                        .or_else(|| self.checked_expr_type(left))?;
                    let Type::Result(_, error_ty) = result_ty else {
                        return None;
                    };
                    let left = self.lower_expr(left, slots, current_function, item_slot)?;
                    let saved = slots.enter();
                    let (error_slot, handler_item_slot) = match parameter {
                        Some(parameter) => (
                            slots.declare_with_type(parameter, Some(*error_ty)),
                            item_slot,
                        ),
                        None => {
                            let slot = slots.reserve("fallback.error");
                            (slot, Some(slot))
                        }
                    };
                    let handler = self.lower_block_value_expr(
                        block,
                        slots,
                        current_function,
                        handler_item_slot,
                    );
                    slots.exit(saved);
                    let handler = handler?;
                    let success_slot = slots.reserve("fallback.success");
                    let success = push_build_row!(self, expr, BuildExprRow::Param(success_slot));
                    let error_pattern = push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::ResultErr {
                            slot: Some(error_slot),
                            unit_only: false
                        }
                    );
                    let success_pattern = push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::ResultOk {
                            slot: Some(success_slot),
                            unit_only: false
                        }
                    );
                    let value = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::MatchExpr {
                            value: left,
                            arms: vec![
                                (success_pattern, None, success),
                                (error_pattern, None, handler)
                            ],
                            span,
                        }
                    );
                    return Some(match checked_creation {
                        Some(ty) => self.checked_unsigned_value(value, &ty, span),
                        None => value,
                    });
                }
                let left_ty = self
                    .checked_expr_type(left)
                    .or_else(|| {
                        self.bodies
                            .expr_types
                            .get(&left)
                            .filter(|ty| compact_checked_type_is_concrete(ty))
                            .cloned()
                    })
                    .or_else(|| self.concrete_checked_type(left));
                let left = self.lower_expr(left, slots, current_function, item_slot)?;
                let right = self.lower_expr(right, slots, current_function, item_slot)?;
                if matches!(left_ty, Some(Type::Optional(_))) {
                    let slot = slots.reserve("optional fallback");
                    let null_pattern = push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Literal(LoweredValue::Null)
                    );
                    let present_pattern =
                        push_build_row!(self, pattern, BuildPatternRow::Bind { slot });
                    let present = push_build_row!(self, expr, BuildExprRow::Param(slot));
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::MatchExpr {
                            value: left,
                            arms: vec![
                                (null_pattern, None, right),
                                (present_pattern, None, present)
                            ],
                            span,
                        }
                    ))
                } else {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ResultFallback { left, right }
                    ))
                }
            }
            ArenaExprKind::If {
                branches,
                else_value,
            } => {
                let branches = self.program.arena.if_expr_branches(branches).to_vec();
                let has_pattern = branches.iter().any(|branch| {
                    matches!(
                        self.program.arena.expr(branch.condition).kind,
                        ArenaExprKind::PatternCondition { .. }
                    )
                });
                let mut lowered = Vec::with_capacity(branches.len());
                for branch in branches {
                    let saved = slots.enter();
                    let condition = self.lower_pattern_condition_parts(
                        branch.condition,
                        slots,
                        current_function,
                        item_slot,
                    );
                    let value = self.lower_expr(branch.value, slots, current_function, item_slot);
                    slots.exit(saved);
                    let (condition, captures) = condition?;
                    lowered.push((condition, value?, captures));
                }
                let else_value = self.lower_expr(else_value, slots, current_function, item_slot)?;
                if has_pattern {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PatternIf {
                            branches: lowered,
                            else_value,
                            span
                        }
                    ))
                } else {
                    let branches = lowered
                        .into_iter()
                        .map(|(condition, value, _)| (condition, value))
                        .collect();
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::IfExpr {
                            branches,
                            else_value,
                            span
                        }
                    ))
                }
            }
            ArenaExprKind::PatternCondition { .. } => self
                .lower_pattern_condition_parts(id, slots, current_function, item_slot)
                .map(|(condition, _)| condition),
            ArenaExprKind::Match { value, arms } | ArenaExprKind::PatternTest { value, arms } => {
                if let Some(expr) =
                    self.lower_str_match_expr(value, arms, span, slots, current_function, item_slot)
                {
                    return Some(match checked_creation {
                        Some(ty) => self.checked_unsigned_value(expr, &ty, span),
                        None => expr,
                    });
                }
                if let Some(expr) =
                    self.lower_tag_match_expr(value, arms, span, slots, current_function, item_slot)
                {
                    return Some(match checked_creation {
                        Some(ty) => self.checked_unsigned_value(expr, &ty, span),
                        None => expr,
                    });
                }
                let arms = self.program.arena.match_expr_arms(arms).to_vec();
                let (ok_binding_ty, err_binding_ty) =
                    self.compact_match_scrutinee_result_types(value);
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    let (pattern, cleanup) = self.lower_pattern(
                        arm.pattern,
                        slots,
                        ok_binding_ty.as_ref(),
                        err_binding_ty.as_ref(),
                    )?;
                    let value = self
                        .lower_expr(arm.value, slots, current_function, item_slot)
                        .unwrap_or(push_build_row!(self, expr, BuildExprRow::Unit));
                    let guard = match arm.guard {
                        Some(guard_expr) => {
                            Some(self.lower_expr(guard_expr, slots, current_function, item_slot)?)
                        }
                        None => None,
                    };
                    cleanup_lowered_pattern_slots(slots, cleanup);
                    lowered_arms.push((pattern, guard, value));
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MatchExpr {
                        value: self.lower_expr(value, slots, current_function, item_slot)?,
                        arms: lowered_arms,
                        span,
                    }
                ))
            }
            ArenaExprKind::Field { base, name } => {
                if let Some(env_expr) = self.lower_env_field(base, name, span) {
                    return Some(env_expr);
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && self.compact_qualified_tag_variant_arity(module, name) == Some(0)
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Tag {
                            type_name: self
                                .compact_tag_type_name(Name::intern(format!("{module}.{name}")))?,
                            wire: self.compact_tag_wire(Name::intern(format!("{module}.{name}"))),
                            name: Arc::<str>::from(name.as_str().as_str()),
                            fields: Default::default(),
                        }
                    ));
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Field {
                        base: self.lower_expr(base, slots, current_function, item_slot)?,
                        name: name.as_str(),
                        span,
                    }
                ))
            }
            ArenaExprKind::NullSafeField { base, name } => {
                let base = self.lower_postfix_receiver(base, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Field {
                        base,
                        name: name.as_str(),
                        span
                    }
                ))
            }
            ArenaExprKind::Index {
                base,
                index,
                guarded,
            } => {
                let uint_key = matches!(self.checked_expr_type(base), Some(Type::Map(key, _)) if *key == Type::UInt);
                let lowered_base = if guarded {
                    self.lower_postfix_receiver(base, slots, current_function, item_slot)?
                } else {
                    self.lower_expr(base, slots, current_function, item_slot)?
                };
                // The checker decided that this index counts from the end;
                // its literal is not evaluated.
                if let Some(distance) = self
                    .bodies
                    .from_end_indexes
                    .get(&id)
                    .copied()
                    .and_then(super::EndDistance::new)
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::IndexFromEnd {
                            base: lowered_base,
                            distance,
                            span
                        }
                    ));
                }
                let lowered_index = self.lower_expr(index, slots, current_function, item_slot)?;
                let lowered_index = if uint_key {
                    self.require_uint_key(lowered_index, self.program.arena.expr(index).span)
                } else {
                    lowered_index
                };
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Index {
                        base: lowered_base,
                        index: lowered_index,
                        span
                    }
                ))
            }
            ArenaExprKind::Slice {
                base,
                start,
                end,
                guarded,
            } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Slice {
                    base: if guarded {
                        self.lower_postfix_receiver(base, slots, current_function, item_slot)?
                    } else {
                        self.lower_expr(base, slots, current_function, item_slot)?
                    },
                    start: match start {
                        Some(start) =>
                            Some(self.lower_expr(start, slots, current_function, item_slot,)?),
                        None => None,
                    },
                    end: match end {
                        Some(end) =>
                            Some(self.lower_expr(end, slots, current_function, item_slot,)?),
                        None => None,
                    },
                    span,
                }
            )),
            ArenaExprKind::Try(expr) => {
                if let ArenaExprKind::Call { args, .. } = self.program.arena.expr(expr).kind
                    && self.program.arena.call_args(args).iter().any(|arg| {
                        matches!(
                            arg.kind,
                            ArenaCallArgKind::Named { .. } | ArenaCallArgKind::NamedSpread { .. }
                        )
                    })
                {
                    let value = self.lower_expr(expr, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Try {
                            value,
                            span: self.propagation_span(id, expr, slots)
                        }
                    ));
                }
                let expr_span = self.program.arena.expr(expr).span;
                if let ArenaExprKind::Call { callee, args } = self.program.arena.expr(expr).kind {
                    let args_vec = self.program.arena.call_args(args).to_vec();
                    if let ArenaExprKind::Field { base, name } =
                        self.program.arena.expr(callee).kind
                        && let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    {
                        if module == "fs" && name == "files" {
                            let options =
                                lower_fs_files_args(&self.checked_api_arguments(expr, &args_vec)?)?;
                            return Some(push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try {
                                    value: push_build_row!(
                                        self,
                                        expr,
                                        BuildExprRow::FsFiles {
                                            root: self.lower_expr(
                                                options.root,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            gitignore: self.lower_optional_expr(
                                                options.gitignore,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            stat: self.lower_optional_expr(
                                                options.stat,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            hidden: self.lower_optional_expr(
                                                options.hidden,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            exts: match options.exts {
                                                Some(exts) => Some(self.lower_expr(
                                                    exts,
                                                    slots,
                                                    current_function,
                                                    item_slot,
                                                )?),
                                                None => None,
                                            },
                                            result_wrapped: true,
                                            span: expr_span,
                                        }
                                    ),
                                    span: self.propagation_span(id, expr, slots)
                                }
                            ));
                        }
                        if module == "fs" && name == "walk" {
                            let options =
                                lower_fs_files_args(&self.checked_api_arguments(expr, &args_vec)?)?;
                            return Some(push_build_row!(
                                self,
                                expr,
                                BuildExprRow::Try {
                                    value: push_build_row!(
                                        self,
                                        expr,
                                        BuildExprRow::FsWalk {
                                            root: self.lower_expr(
                                                options.root,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            gitignore: self.lower_optional_expr(
                                                options.gitignore,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            stat: self.lower_optional_expr(
                                                options.stat,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            hidden: self.lower_optional_expr(
                                                options.hidden,
                                                slots,
                                                current_function,
                                                item_slot,
                                            )?,
                                            exts: match options.exts {
                                                Some(exts) => Some(self.lower_expr(
                                                    exts,
                                                    slots,
                                                    current_function,
                                                    item_slot,
                                                )?),
                                                None => None,
                                            },
                                            result_wrapped: true,
                                            span: expr_span,
                                        }
                                    ),
                                    span: self.propagation_span(id, expr, slots)
                                }
                            ));
                        }
                    }
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: self.lower_expr(expr, slots, current_function, item_slot)?,
                        span: self.propagation_span(id, expr, slots)
                    }
                ))
            }
            ArenaExprKind::Call { callee, args } => {
                self.lower_call(id, callee, args, slots, current_function, item_slot)
            }
            ArenaExprKind::BuilderCall { call, block } => self.lower_process_command_builder(
                call,
                block,
                slots,
                current_function,
                item_slot,
                span,
            ),
            ArenaExprKind::Capture(block) => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Capture {
                    body: self.lower_retry_block(block, slots, current_function, item_slot)?,
                    span,
                }
            )),
            ArenaExprKind::ValuePipelineCall { input, call, hole } => {
                let input = self.lower_expr(input, slots, current_function, item_slot)?;
                let slot = slots.reserve("value pipeline input");
                let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
                let previous = slots.postfix_receivers.insert(hole, bound);
                let selected = self.lower_expr(call, slots, current_function, item_slot);
                match previous {
                    Some(previous) => {
                        slots.postfix_receivers.insert(hole, previous);
                    }
                    None => {
                        slots.postfix_receivers.remove(&hole);
                    }
                }
                let selected = selected?;
                let pattern = push_build_row!(self, pattern, BuildPatternRow::Bind { slot });
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MatchExpr {
                        value: input,
                        arms: vec![(pattern, None, selected)],
                        span
                    }
                ))
            }
            ArenaExprKind::ErrorContext { message, block } => {
                let message = self.lower_expr(message, slots, current_function, item_slot)?;
                let tail = self
                    .program
                    .arena
                    .stmt_ids(self.program.arena.block(block).statements)
                    .last();
                let statement_body = tail.is_some_and(|tail| {
                    self.bodies.statement_positions.get(&tail)
                        == Some(&crate::sema::check::StatementPosition::Statement)
                });
                let body = if statement_body {
                    self.lower_block(block, slots, current_function, item_slot)?
                } else {
                    self.lower_retry_block(block, slots, current_function, item_slot)?
                };
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ErrorContext {
                        message,
                        body,
                        span
                    }
                ))
            }
            ArenaExprKind::ContextScope {
                kind, input, block, ..
            } => {
                let input = self.lower_expr(input, slots, current_function, item_slot)?;
                let body = self.lower_retry_block(block, slots, current_function, item_slot)?;
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ContextScope {
                        kind,
                        input,
                        body,
                        span
                    }
                ))
            }
            ArenaExprKind::TempDirScope { path, block, .. } => {
                self.lower_tempdir_scope(path, block, span, slots, current_function, item_slot)
            }
            ArenaExprKind::ResourceScope {
                bindings, block, ..
            } => self.lower_resource_scope(
                id,
                bindings,
                block,
                span,
                slots,
                current_function,
                item_slot,
            ),
            ArenaExprKind::ValueBlock(block) => {
                self.lower_block_value_expr(block, slots, current_function, item_slot)
            }
            ArenaExprKind::Loop { block } => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Loop {
                    body: self.lower_block(block, slots, current_function, item_slot)?,
                    span,
                }
            )),
            // A block that binds an empty list to a local of its own, runs
            // the body as an inner block, and has the local as its value. The
            // body's yields append to that local (`lower_collect_yield`).
            ArenaExprKind::Collect { block } => {
                let saved = slots.enter();
                let slot = slots.declare(collect_local(id));
                let lowered = (|| {
                    let empty = push_build_row!(self, expr, BuildExprRow::List(Vec::new()));
                    let bind =
                        push_build_row!(self, stmt, BuildStmtRow::Let { slot, value: empty });
                    let body = self.lower_block(block, slots, current_function, item_slot)?;
                    let body = push_build_row!(self, expr, BuildExprRow::ValueBlock { body, span });
                    let run = push_build_row!(self, stmt, BuildStmtRow::Expr { value: body, span });
                    let list = push_build_row!(self, expr, BuildExprRow::Param(slot));
                    let value = push_build_row!(self, stmt, BuildStmtRow::Value { value: list });
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ValueBlock {
                            body: vec![bind, run, value],
                            span,
                        }
                    ))
                })();
                slots.exit(saved);
                lowered
            }
            ArenaExprKind::Retry {
                schedule,
                delays,
                pattern,
                block,
            } => {
                let delays = self.program.arena.expr_ids(delays).collect::<Vec<_>>();
                let mut lowered_delays = Vec::with_capacity(delays.len());
                for delay in delays {
                    lowered_delays.push(self.lower_expr(
                        delay,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Retry {
                        delays: lowered_delays,
                        pattern: match pattern {
                            Some(pattern) =>
                                Some(self.lower_pattern(pattern, slots, None, None)?.0),
                            None => None,
                        },
                        body: self.lower_retry_block(block, slots, current_function, item_slot)?,
                        span,
                        schedule,
                    }
                ))
            }
            // `e"NAME"` is exactly `env.get("NAME")`.
            ArenaExprKind::EnvString(name) => {
                let name = push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: RuntimeOp::EnvGet,
                        args: vec![Some(name)],
                        span,
                    }
                ))
            }
            ArenaExprKind::EnvPathList => Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ModuleCall {
                    cli_plan: None,
                    op: RuntimeOp::EnvPathList,
                    args: Vec::new(),
                    span,
                }
            )),
            _ => None,
        };
        match lowered {
            Some(lowered) => {
                self.output.constructed_expressions += 1;
                Some(match checked_creation {
                    Some(ty) => self.checked_unsigned_value(lowered, &ty, span),
                    None => lowered,
                })
            }
            None => {
                let kind = self.program.arena.expr(id).kind;
                let blocker_label = compact_expr_kind_label(kind.clone());
                let blocker_index = compact_expr_kind_index(kind);
                let mut blocker_detail = (
                    self.program.arena.expr(id).span,
                    format!("expression `{blocker_label}`"),
                );
                if blocker_index == 22
                    && let ArenaExprKind::Call { callee, .. } = self.program.arena.expr(id).kind
                {
                    self.output.call_blockers[compact_call_blocker_index(self.program, callee)] +=
                        1;
                    record_compact_call_blocker_label(
                        &mut self.output.call_blocker_callees,
                        self.program,
                        callee,
                    );
                    record_compact_call_blocker_span(
                        &mut self.output.call_blocker_sample_spans,
                        self.program,
                        callee,
                    );
                    if let Some(label) = compact_call_blocker_label(self.program, callee) {
                        blocker_detail = (
                            self.program.arena.expr(callee).span,
                            format!("call `{label}`"),
                        );
                    }
                }
                self.last_blocker_detail = Some(blocker_detail);
                self.output.expression_blockers[blocker_index] += 1;
                self.output.constructed_expressions += 1;
                self.output.blocker_events += 1;
                Some(push_build_row!(self, expr, BuildExprRow::Unit))
            }
        }
    }

    /// `env.PATH` methods act on the runtime environment overlay, so a view
    /// held in a binding is evaluated only for its effects.
    fn lower_env_path_method_call(
        &mut self,
        call: ExprId,
        callee: ExprId,
        args_vec: &[ArenaCallArg],
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind else {
            return None;
        };
        let plan =
            self.bodies.api_calls.get(&call).filter(|plan| {
                plan.receiver == Some(crate::modules::MethodReceiver::EnvPathList)
            })?;
        let arguments = self
            .checked_api_arguments(call, args_vec)?
            .ordered()
            .into_iter()
            .collect::<Option<Vec<_>>>()?;
        let mut bindings = Vec::new();
        if !matches!(
            self.program.arena.expr(base).kind,
            ArenaExprKind::EnvPathList
        ) && !is_env_module_expr(&self.program.arena, base)
        {
            let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
            bindings.push((receiver, slots.reserve("environment path receiver")));
        }
        let args = arguments
            .iter()
            .map(|argument| {
                self.lower_expr(*argument, slots, current_function, item_slot)
                    .map(Some)
            })
            .collect::<Option<Vec<_>>>()?;
        let value = push_build_row!(
            self,
            expr,
            BuildExprRow::ModuleCall {
                cli_plan: None,
                op: plan.sig.op,
                args,
                span
            }
        );
        Some(self.wrap_argument_bindings(value, bindings, span))
    }

    fn lower_env_field(
        &mut self,
        base: crate::syntax::arena::ExprId,
        name: crate::symbol::Name,
        span: crate::source::Span,
    ) -> Option<BuildExprId> {
        let base_kind = self.program.arena.expr(base).kind;
        match base_kind {
            ArenaExprKind::Ident(base_name) if base_name == "env" => {
                if name == "PATH" {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::EnvPathList,
                            args: Vec::new(),
                            span,
                        }
                    ));
                }
                let name = push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
                Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: RuntimeOp::EnvGet,
                        args: vec![Some(name)],
                        span,
                    }
                ))
            }
            ArenaExprKind::Field {
                base: inner_base,
                name: type_name,
            } => {
                let inner_kind = self.program.arena.expr(inner_base).kind;
                if let ArenaExprKind::Ident(inner_name) = inner_kind {
                    if inner_name == "env" {
                        let arg =
                            push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
                        let op = match type_name.as_str().as_str() {
                            "Path" => RuntimeOp::EnvPath,
                            "PathList" => RuntimeOp::EnvPathList,
                            _ => RuntimeOp::EnvGet,
                        };
                        Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: None,
                                op,
                                args: vec![Some(arg)],
                                span,
                            }
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn lower_fmt_string(
        &mut self,
        parts: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::FmtString(self.lower_fmt_parts(
                parts,
                slots,
                current_function,
                item_slot,
            )?)
        ))
    }

    fn lower_fmt_parts(
        &mut self,
        parts: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredFmtPart>> {
        let parts = self.program.arena.fmt_parts(parts).collect::<Vec<_>>();
        let mut lowered = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                ArenaFmtPart::Text(text) => {
                    lowered.push(LoweredFmtPart::Text(Arc::from(self.text_value(&text)?)));
                }
                ArenaFmtPart::Expr(expr, spec) => {
                    let span = self.program.arena.expr(expr).span;
                    lowered.push(LoweredFmtPart::Expr(
                        self.lower_expr(expr, slots, current_function, item_slot)?,
                        span,
                        spec,
                    ));
                }
            }
        }
        Some(lowered)
    }

    fn lower_map_literal(
        &mut self,
        id: ExprId,
        fields: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let uint_key = matches!(self.bodies.expr_types.get(&id), Some(Type::Map(key, _)) if **key == Type::UInt);
        // A quoted label of a map the checker keyed by Path is a Path key.
        let path_key = matches!(self.bodies.expr_types.get(&id), Some(Type::Map(key, _)) if **key == Type::Path);
        let mut entries = Vec::new();
        for field in self.program.arena.record_fields(fields).to_vec() {
            let key_span = match &field.kind {
                ArenaRecordFieldKind::Computed { key, .. } => {
                    Some(self.program.arena.expr(*key).span)
                }
                _ => None,
            };
            let (key, value, span) = match field.kind {
                ArenaRecordFieldKind::Computed { key, value, span } => (
                    Some(self.lower_expr(key, slots, current_function, item_slot)?),
                    self.lower_expr(value, slots, current_function, item_slot)?,
                    self.program.arena.span(span),
                ),
                ArenaRecordFieldKind::Named { name, value, span } => (
                    Some(if path_key {
                        let path = PathValue::from_text(name.as_str().as_str()).ok()?;
                        push_build_row!(self, expr, BuildExprRow::Path(path))
                    } else {
                        push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Str(Arc::from(name.as_str().as_str()))
                        )
                    }),
                    self.lower_expr(value, slots, current_function, item_slot)?,
                    self.program.arena.span(span),
                ),
                ArenaRecordFieldKind::Shorthand { name, span } => (
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Str(Arc::from(name.as_str().as_str()))
                    )),
                    push_build_row!(self, expr, BuildExprRow::Param(slots.resolve(name)?)),
                    self.program.arena.span(span),
                ),
                ArenaRecordFieldKind::Spread { expr, span } => (
                    None,
                    self.lower_expr(expr, slots, current_function, item_slot)?,
                    self.program.arena.span(span),
                ),
                ArenaRecordFieldKind::Path { .. } => return None,
            };
            let key = key.map(|key| {
                if uint_key {
                    self.require_uint_key(key, key_span.unwrap_or(span))
                } else {
                    key
                }
            });
            entries.push((key, value, span));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::MapLiteral(entries)
        ))
    }

    fn lower_record(
        &mut self,
        fields: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let fields = self.program.arena.record_fields(fields).to_vec();
        if fields
            .iter()
            .any(|field| matches!(field.kind, ArenaRecordFieldKind::Path { .. }))
        {
            let ArenaRecordFieldKind::Spread { expr, span } = fields.first()?.kind else {
                return None;
            };
            let span = self.program.arena.span(span);
            let base = self.lower_expr(expr, slots, current_function, item_slot)?;
            let mut updates = Vec::new();
            for field in fields.into_iter().skip(1) {
                let (path, value, span) = match field.kind {
                    ArenaRecordFieldKind::Path { path, value, span } => (
                        self.program.arena.names(path).collect(),
                        self.lower_expr(value, slots, current_function, item_slot)?,
                        self.program.arena.span(span),
                    ),
                    ArenaRecordFieldKind::Named { name, value, span } => (
                        vec![name],
                        self.lower_expr(value, slots, current_function, item_slot)?,
                        self.program.arena.span(span),
                    ),
                    ArenaRecordFieldKind::Shorthand { name, span } => (
                        vec![name],
                        self.lower_bare_ident(name, slots)?,
                        self.program.arena.span(span),
                    ),
                    ArenaRecordFieldKind::Spread { .. } | ArenaRecordFieldKind::Computed { .. } => {
                        return None;
                    }
                };
                updates.push((path, value, span));
            }
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::RecordUpdate {
                    base,
                    updates: LoweredRecordUpdates(updates),
                    span
                }
            ));
        }
        let mut lowered = Vec::with_capacity(fields.len());
        for field in fields {
            match field.kind {
                ArenaRecordFieldKind::Computed { .. } => return None,
                ArenaRecordFieldKind::Path { .. } => return None,
                ArenaRecordFieldKind::Named { name, value, .. } => {
                    lowered.push(LoweredRecordEntry::Field(
                        name,
                        self.lower_expr(value, slots, current_function, item_slot)?,
                    ));
                }
                ArenaRecordFieldKind::Shorthand { name, .. } => {
                    lowered.push(LoweredRecordEntry::Field(
                        name,
                        push_build_row!(self, expr, BuildExprRow::Param(slots.resolve(name)?)),
                    ));
                }
                ArenaRecordFieldKind::Spread { expr, .. } => {
                    lowered.push(LoweredRecordEntry::Spread(self.lower_expr(
                        expr,
                        slots,
                        current_function,
                        item_slot,
                    )?));
                }
            }
        }
        Some(push_build_row!(self, expr, BuildExprRow::Record(lowered)))
    }

    fn lower_process_command_builder(
        &mut self,
        call: ExprId,
        block: crate::syntax::arena::BuilderBlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        span: Span,
    ) -> Option<BuildExprId> {
        let (module, name, args) = match self.program.arena.expr(call).kind {
            ArenaExprKind::Field { base, name } => {
                let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind else {
                    return None;
                };
                (module, name, None)
            }
            ArenaExprKind::Call { callee, args } => {
                let ArenaExprKind::Field { base, name } = self.program.arena.expr(callee).kind
                else {
                    return None;
                };
                let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind else {
                    return None;
                };
                (module, name, Some(args))
            }
            _ => return None,
        };
        if module != "process" || name != "command" {
            return None;
        }
        if args.is_some_and(|args| !self.program.arena.call_args(args).is_empty()) {
            return None;
        }

        let entries = self
            .program
            .arena
            .builder_entries(self.program.arena.builder_block(block).entries)
            .to_vec();
        let mut lowered = Vec::with_capacity(entries.len());
        let mut run_seen = false;
        for entry in entries {
            match entry.kind {
                ArenaBuilderEntryKind::Field { name, value } => {
                    lowered.push(LoweredProcessCommandBuilderEntry::Field {
                        name,
                        value: self.lower_expr(value, slots, current_function, item_slot)?,
                        span: self.program.arena.span(entry.span),
                    });
                }
                ArenaBuilderEntryKind::Stmt(stmt) => {
                    let ArenaStmtKind::Command(command) = self.program.arena.stmt(stmt).kind else {
                        return None;
                    };
                    let command_stmt = self.program.arena.command_stmt(command);
                    let ArenaCommand::Run(run) = command_stmt.command else {
                        return None;
                    };
                    if command_stmt.propagate || run_seen {
                        return None;
                    }
                    let run_form = self.program.arena.run_form(run);
                    if run_form.propagate {
                        return None;
                    }
                    let [segment] = self.program.arena.run_segments(run_form.segments) else {
                        return None;
                    };
                    if !matches!(segment.kind, RunKind::Plain | RunKind::Status)
                        || !self
                            .program
                            .arena
                            .redirections(segment.redirections)
                            .is_empty()
                    {
                        return None;
                    }
                    let target =
                        self.lower_run_arg(&segment.target, slots, current_function, item_slot)?;
                    let args = self.program.arena.command_args(segment.args).to_vec();
                    let mut lowered_args = Vec::with_capacity(args.len());
                    for arg in &args {
                        lowered_args.push(self.lower_run_arg(
                            arg,
                            slots,
                            current_function,
                            item_slot,
                        )?);
                    }
                    let env = self.program.arena.env_assignments(segment.env).to_vec();
                    let mut lowered_env = Vec::with_capacity(env.len());
                    for assignment in &env {
                        lowered_env.push(self.lower_run_env(
                            assignment,
                            slots,
                            current_function,
                            item_slot,
                        )?);
                    }
                    lowered.push(LoweredProcessCommandBuilderEntry::Run {
                        target,
                        args: lowered_args,
                        env: lowered_env,
                        timeout: match segment.timeout {
                            Some(value) => {
                                Some(self.lower_expr(value, slots, current_function, item_slot)?)
                            }
                            None => None,
                        },
                        cpu_max: match segment.cpu_max {
                            Some(value) => {
                                Some(self.lower_expr(value, slots, current_function, item_slot)?)
                            }
                            None => None,
                        },
                        accept: match segment.accept {
                            Some(value) => {
                                Some(self.lower_expr(value, slots, current_function, item_slot)?)
                            }
                            None => None,
                        },
                        span: self.program.arena.span(command_stmt.span),
                    });
                    run_seen = true;
                }
                ArenaBuilderEntryKind::Entry { .. } | ArenaBuilderEntryKind::Task { .. } => {
                    return None;
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::ProcessCommandBuilder {
                entries: lowered,
                span,
            }
        ))
    }

    fn lower_expr_ids(
        &mut self,
        ids: &[ExprId],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<BuildExprId>> {
        let mut lowered = Vec::with_capacity(ids.len());
        for id in ids {
            lowered.push(self.lower_expr(*id, slots, current_function, item_slot)?);
        }
        Some(lowered)
    }

    fn lower_call_args(
        &mut self,
        args: &[ArenaCallArg],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredCallArg>> {
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            match arg.kind {
                ArenaCallArgKind::Positional(expr) => {
                    lowered.push(LoweredCallArg::Single(self.lower_expr(
                        expr,
                        slots,
                        current_function,
                        item_slot,
                    )?));
                }
                ArenaCallArgKind::Splice { value, .. } => {
                    lowered.push(LoweredCallArg::Splice(self.lower_expr(
                        value,
                        slots,
                        current_function,
                        item_slot,
                    )?));
                }
                ArenaCallArgKind::Named { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                    return None;
                }
            }
        }
        Some(lowered)
    }

    /// Route a script-backed method call to its prepared implementation.
    ///
    /// The receiver becomes the implementation function's first argument, so
    /// the embedded body sees exactly the parameters the method signature
    /// declares plus the receiver it was invoked on.
    fn lower_script_method_call(
        &mut self,
        call: ExprId,
        base: ExprId,
        name: Name,
        args: &[ArenaCallArg],
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let script = api_spec().script_method_impl(&name.as_str())?;
        let namespace = self.internal_namespace(script.module)?;
        let function = Name::intern(script.function);
        let qualified = QualifiedName::new(namespace, function);
        if !self.compact_qualified_function_available(qualified)
            && self.stdlib_linkage != StdlibLowerLinkage::External
        {
            return None;
        }
        // The method signature excludes the receiver; the implementation
        // function declares it first, so bind the declared parameters against
        // the arguments after the leading receiver slot.
        let params = self
            .compact_qualified_function_sig(namespace, function)
            .map(|sig| {
                sig.params
                    .iter()
                    .skip(1)
                    .cloned()
                    .collect::<Vec<CallableParamType>>()
            });
        let mut lowered = vec![LoweredCallArg::Single(self.lower_expr(
            base,
            slots,
            current_function,
            item_slot,
        )?)];
        lowered.extend(
            self.lower_function_call_args(
                args,
                params.as_deref(),
                self.bodies
                    .api_calls
                    .get(&call)
                    .filter(|plan| plan.receiver.is_some())
                    .and_then(|plan| script_argument_slots(plan, params.as_deref()?)),
                Some((LoweredFunctionKey::Qualified(qualified), 1)),
                slots,
                current_function,
                item_slot,
            )?,
        );
        if self.stdlib_linkage == StdlibLowerLinkage::External {
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ExternalCall {
                    function: qualified,
                    args: lowered,
                    span,
                }
            ));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Call {
                function: LoweredFunctionKey::Qualified(qualified),
                args: lowered,
                span,
            }
        ))
    }

    /// Lower a specialized `hash.verify_file` call to its embedded
    /// implementation.
    ///
    /// The public form carries its algorithm in the checksum argument's *name*,
    /// and a name is not a value, so the implementation function takes the
    /// algorithm as an explicit third argument rather than mirroring the public
    /// parameters. The file is hashed by the retained digest primitives the
    /// implementation selects by that name, and everything after the digest —
    /// length and hexadecimal validation, comparison, and the failure
    /// composition — is the embedded function's own work.
    fn lower_hash_verify_file_call(
        &mut self,
        options: &LoweredHashVerifyFileArgs,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
        span: Span,
    ) -> Option<BuildExprId> {
        let namespace = self.internal_namespace("hash")?;
        let function = QualifiedName::new(namespace, Name::intern("verify_file"));
        let path = self.lower_expr(options.path, slots, current_function, item_slot)?;
        let expected = self.lower_expr(options.expected, slots, current_function, item_slot)?;
        let algorithm = push_build_row!(self, expr, BuildExprRow::Str(options.algorithm.into()));
        let args = vec![
            LoweredCallArg::Single(path),
            LoweredCallArg::Single(expected),
            LoweredCallArg::Single(algorithm),
        ];
        if self.stdlib_linkage == StdlibLowerLinkage::External {
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ExternalCall {
                    function,
                    args,
                    span,
                }
            ));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Call {
                function: LoweredFunctionKey::Qualified(function),
                args,
                span,
            }
        ))
    }

    /// Route a public entry whose implementation is embedded XSH to the
    /// prepared implementation function.
    ///
    /// Returns `None` for native entries, for a spelling the registry does not
    /// bind to a script, and when the implementation module was not prepared —
    /// the latter is a preparation defect that surfaces as a missing-target
    /// diagnostic rather than as a silent fallback to a deleted native body.
    fn lower_script_module_call(
        &mut self,
        call: ExprId,
        args: &[ArenaCallArg],
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        // The checker's selected overload routes each form of a public name to
        // its own implementation function.
        let script = self.module_call_plan(call)?.sig.script_impl()?;
        let namespace = self.internal_namespace(script.module)?;
        let function = Name::intern(script.function);
        let qualified = QualifiedName::new(namespace, function);
        if !self.compact_qualified_function_available(qualified)
            && self.stdlib_linkage != StdlibLowerLinkage::External
        {
            // No prepared implementation and no loading program to provide one:
            // this is a preparation defect, so do not quietly fall back.
            return None;
        }
        let params = self
            .compact_qualified_function_sig(namespace, function)
            .map(|sig| sig.params.clone());
        let args = self
            .lower_function_call_args(
                args,
                params.as_deref(),
                self.module_call_plan(call)
                    .and_then(|plan| script_argument_slots(plan, params.as_deref()?)),
                Some((LoweredFunctionKey::Qualified(qualified), 0)),
                slots,
                current_function,
                item_slot,
            )?
            .into_iter()
            .collect();
        if self.stdlib_linkage == StdlibLowerLinkage::External {
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ExternalCall {
                    function: qualified,
                    args,
                    span,
                }
            ));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Call {
                function: LoweredFunctionKey::Qualified(qualified),
                args,
                span,
            }
        ))
    }

    /// The program's function index, built once per lowering pass.
    fn function_index(&self) -> Rc<CompactFunctionIndex> {
        let uninitialised = self.function_defs.borrow().is_none();
        if uninitialised {
            let index = Rc::new(CompactFunctionIndex::new(self.program));
            *self.function_defs.borrow_mut() = Some(index);
        }
        Rc::clone(
            self.function_defs
                .borrow()
                .as_ref()
                .expect("function index was just built"),
        )
    }

    /// The private representation operation a declared bridge function lowers
    /// to, when this call is inside that function's own implementation module.
    ///
    /// The owner test is what keeps the bridge narrow: a call is rewritten
    /// only from the module that declares the bridge, so no other embedded
    /// module and no user source can reach the operation even if a spelling
    /// collides.
    fn compact_bridge_op(&self, key: LoweredFunctionKey) -> Option<RuntimeOp> {
        let LoweredFunctionKey::Qualified(qualified) = key else {
            return None;
        };
        if self.current_namespace != Some(qualified.namespace) {
            return None;
        }
        let module = crate::stdlib::find_by_namespace(&qualified.namespace.as_str())?;
        let function = qualified.member.as_str();
        crate::stdlib::bridge_op(module, function.as_str())
    }

    /// The interned namespace of an embedded implementation module, when this
    /// program can reach it.
    ///
    /// A program that prepared the module has its functions among the checked
    /// declarations. A program linked to a loading program's prepared modules
    /// has none of its own, so the namespace is accepted whenever the
    /// implementation catalog defines it.
    fn internal_namespace(&self, identity: &str) -> Option<Name> {
        crate::stdlib::find(identity)?;
        let text = crate::stdlib::namespace_text(identity);
        let namespace = Name::intern(text);
        if self.stdlib_linkage == StdlibLowerLinkage::External {
            return Some(namespace);
        }
        self.declarations
            .qualified_pures
            .keys()
            .chain(self.declarations.qualified_procs.keys())
            .chain(self.declarations.qualified_streams.keys())
            .any(|qualified| qualified.namespace == namespace)
            .then_some(namespace)
    }

    /// Save each source entry once and project a spread's visible fields before
    /// beginning the next entry. Slots preserve this order when a callable's
    /// parameter order differs from its written argument order.
    pub(super) fn lower_expanded_argument_values(
        &mut self,
        expanded: &[crate::sema::arguments::ExpandedArgument],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<LoweredArgumentValues> {
        use crate::sema::arguments::ArgumentValueSource;
        let mut bindings = Vec::new();
        let mut values = Vec::new();
        let mut record_entry = None;
        for arg in expanded {
            let value = match arg.value {
                ArgumentValueSource::Expression(expr)
                | ArgumentValueSource::PositionalSplice(expr) => {
                    record_entry = None;
                    self.lower_expr(expr, slots, current_function, item_slot)?
                }
                ArgumentValueSource::RecordField { record, field } => {
                    let base = match record_entry {
                        Some((entry, value)) if entry == arg.entry_index => value,
                        _ => {
                            let value =
                                self.lower_expr(record, slots, current_function, item_slot)?;
                            let slot = slots.reserve("named spread record");
                            bindings.push((value, slot));
                            let value = push_build_row!(self, expr, BuildExprRow::Param(slot));
                            record_entry = Some((arg.entry_index, value));
                            value
                        }
                    };
                    // The record is bound where the spread is written, which
                    // fixes when it is evaluated. Reading a field of the
                    // bound record has no effect and cannot fail, so the
                    // field is read where it is passed: a binding of its own
                    // would nest one level per field, and every pass over
                    // the lowered program recurses through that nesting.
                    values.push(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Field {
                            base,
                            name: field.as_str(),
                            span: arg.span
                        }
                    ));
                    continue;
                }
            };
            let slot = slots.reserve("call argument");
            bindings.push((value, slot));
            values.push(push_build_row!(self, expr, BuildExprRow::Param(slot)));
        }
        Some(LoweredArgumentValues { values, bindings })
    }

    /// Sequence slot initialization around an ordinary value expression.
    pub(super) fn wrap_argument_bindings(
        &mut self,
        mut value: BuildExprId,
        bindings: Vec<(BuildExprId, usize)>,
        span: Span,
    ) -> BuildExprId {
        for (subject, slot) in bindings.into_iter().rev() {
            let pattern = push_build_row!(self, pattern, BuildPatternRow::Bind { slot });
            value = push_build_row!(
                self,
                expr,
                BuildExprRow::MatchExpr {
                    value: subject,
                    arms: vec![(pattern, None, value)],
                    span,
                }
            );
        }
        value
    }

    fn lower_named_spread_call(
        &mut self,
        id: ExprId,
        callee: ExprId,
        args: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        use crate::syntax::arena::ArenaCallArgInput;
        let expanded =
            expand_named_arguments(self.program, self.program.arena.call_args(args), |expr| {
                self.bodies
                    .expr_types
                    .get(&expr)
                    .filter(|ty| compact_checked_type_is_concrete(ty))
                    .cloned()
                    .or_else(|| self.checked_expr_type(expr))
            })
            .ok()?;
        let mut bindings = Vec::new();
        let mut overrides = Vec::new();
        // Namespaces and static function names have no runtime receiver. A
        // method's value receiver is evaluated before its argument entries.
        if let ArenaExprKind::Field { base, .. } = self.program.arena.expr(callee).kind {
            let namespace = self
                .resolved_compact_error_family_key(base)
                .and_then(|key| compact_error_family_info(self.declarations, key))
                .is_some()
                || matches!(self.checked_expr_type(base), Some(Type::ErrorFamily(_)))
                || matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(module) if slots.resolve(module).is_none() && matches!(self.checked_expr_type(base), Some(Type::Module(_))))
                || matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(module) if module == "Path" || api_spec().module(&module.as_str()).is_some() || self.declarations.error_families_by_name.contains_key(&module))
                || self
                    .declarations
                    .record_constructors
                    .resolve_call(&self.program.arena, callee, self.current_namespace)
                    .is_some();
            if !namespace {
                let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
                let slot = slots.reserve("call receiver");
                bindings.push((receiver, slot));
                let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
                overrides.push((base, slots.postfix_receivers.insert(base, bound)));
            }
        }
        // A typed call's callee may be any expression; it is evaluated before
        // the argument entries, like a method's receiver.
        if self.bodies.typed_callable_calls.contains_key(&id)
            && !matches!(
                self.program.arena.expr(callee).kind,
                ArenaExprKind::Ident(_) | ArenaExprKind::Field { .. }
            )
        {
            let value = self.lower_expr(callee, slots, current_function, item_slot)?;
            let slot = slots.reserve("typed callee");
            bindings.push((value, slot));
            let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
            overrides.push((callee, slots.postfix_receivers.insert(callee, bound)));
        }
        let lowered =
            self.lower_expanded_argument_values(&expanded, slots, current_function, item_slot);
        let call_was_bound = slots.bound_call_entries.insert(id);
        let result = (|| {
            let lowered = lowered?;
            bindings.extend(lowered.bindings);
            if expanded
                .iter()
                .all(|arg| !matches!(arg.value, ArgumentValueSource::RecordField { .. }))
            {
                for (arg, bound) in expanded.iter().zip(lowered.values.iter().copied()) {
                    let (ArgumentValueSource::Expression(expr)
                    | ArgumentValueSource::PositionalSplice(expr)) = arg.value
                    else {
                        unreachable!();
                    };
                    overrides.push((expr, slots.postfix_receivers.insert(expr, bound)));
                }
                let value =
                    self.lower_call(id, callee, args, slots, current_function, item_slot)?;
                return Some(self.wrap_argument_bindings(
                    value,
                    bindings,
                    self.program.arena.expr(id).span,
                ));
            }
            // Synthetic projections exist only during static lowering. Existing
            // expression IDs and source argument ranges remain unchanged. The
            // copy they are appended to is taken out of the cell while this
            // call is lowered over it and put back afterwards.
            let taken = self.spread_programs.borrow_mut().take();
            let mut extended = taken.unwrap_or_else(|| {
                #[cfg(test)]
                SPREAD_PROGRAM_COPIES.with(|copies| copies.set(copies.get() + 1));
                Box::new(SpreadPrograms {
                    program: self.program.clone(),
                    bodies: self.bodies.clone(),
                })
            });
            let SpreadPrograms {
                program: temporary,
                bodies,
            } = &mut *extended;
            let mut inputs = Vec::new();
            for (arg, bound) in expanded.iter().zip(lowered.values) {
                let expr = match arg.value {
                    ArgumentValueSource::Expression(expr)
                    | ArgumentValueSource::PositionalSplice(expr) => expr,
                    ArgumentValueSource::RecordField { record, field } => {
                        let expr = temporary
                            .arena
                            .append_argument_projection(record, field, arg.span);
                        bodies.expr_types.insert(expr, arg.ty.clone());
                        expr
                    }
                };
                overrides.push((expr, slots.postfix_receivers.insert(expr, bound)));
                inputs.push(if let Some(name) = arg.name {
                    ArenaCallArgInput::Named {
                        name,
                        value: expr,
                        span: arg.span,
                    }
                } else if matches!(arg.value, ArgumentValueSource::PositionalSplice(_)) {
                    ArenaCallArgInput::Splice {
                        value: expr,
                        span: arg.span,
                    }
                } else {
                    ArenaCallArgInput::Positional(expr)
                });
            }
            let args = temporary.arena.append_call_arguments(&inputs);
            let (temporary, bodies) = (&*temporary, &*bodies);
            let mut child = CompactLowerConstructProbe {
                program: temporary,
                bodies,
                declarations: self.declarations,
                source: self.source,
                sources: self.sources,
                current_namespace: self.current_namespace,
                functions: self.functions,
                top_level_known: self.top_level_known.clone(),
                output: std::mem::take(&mut self.output),
                last_blocker_detail: self.last_blocker_detail.take(),
                stdlib_linkage: self.stdlib_linkage,
                function_defs: Rc::clone(&self.function_defs),
                scratch: Rc::clone(&self.scratch),
                // The child's program is the extended copy itself.
                spread_programs: Rc::default(),
            };
            let value = child.lower_call(id, callee, args, slots, current_function, item_slot);
            self.output = child.output;
            self.last_blocker_detail = child.last_blocker_detail;
            *self.spread_programs.borrow_mut() = Some(extended);
            value.map(|value| {
                self.wrap_argument_bindings(value, bindings, self.program.arena.expr(id).span)
            })
        })();
        if call_was_bound {
            slots.bound_call_entries.remove(&id);
        }
        for (expr, previous) in overrides.into_iter().rev() {
            if let Some(previous) = previous {
                slots.postfix_receivers.insert(expr, previous);
            } else {
                slots.postfix_receivers.remove(&expr);
            }
        }
        result
    }

    fn checked_path_method_arguments(
        &self,
        call: ExprId,
        args: &[ArenaCallArg],
    ) -> Option<CheckedApiArguments> {
        self.bodies
            .api_calls
            .get(&call)
            .filter(|plan| plan.receiver == Some(crate::modules::MethodReceiver::Path))
            .and_then(|_| self.checked_api_arguments(call, args))
    }

    /// The data and mode of a checked `PATH.write(data, mode)`. A write with
    /// a mode is a separate overload and lowers as an ordinary operation
    /// call with the receiver first; only the two-operand write has its own
    /// instruction.
    fn path_write_mode_args(
        &self,
        call: ExprId,
        args: &[ArenaCallArg],
    ) -> Option<(ExprId, ExprId)> {
        let checked = self.checked_path_method_arguments(call, args)?;
        Some((checked.get("data")?, checked.get("mode")?))
    }

    /// The operands of a checked `PATH.chmod(mode)` or
    /// `PATH.chmod(mode, follow_symlinks: B)` after the receiver, in parameter
    /// order. The name is shared with the rooted method, which is dispatched
    /// by name, so the `Path` method cannot take the route of a method whose
    /// name lowering does not know and is given its operation here.
    fn path_chmod_operands(
        &self,
        call: ExprId,
        args: &[ArenaCallArg],
    ) -> Option<Vec<Option<ExprId>>> {
        let checked = self.checked_path_method_arguments(call, args)?;
        (checked.sig.op == RuntimeOp::FsChmod).then(|| checked.ordered())
    }

    /// The module operation a checked `Path` method runs as when it has no
    /// lowering of its own, with its operands after the receiver in parameter
    /// order. The registry gives such a method the operation of the function
    /// that takes the path first, so a method added there needs only that
    /// operation to be executable.
    fn path_method_module_operands(
        &self,
        call: ExprId,
        name: Name,
        args: &[ArenaCallArg],
    ) -> Option<(RuntimeOp, Vec<Option<ExprId>>)> {
        if lowered_method_name(&name.as_str()) {
            return None;
        }
        let checked = self.checked_path_method_arguments(call, args)?;
        let op = checked.sig.op;
        lowered_module_op_supported(op).then(|| (op, checked.ordered()))
    }

    fn checked_api_arguments(
        &self,
        call: ExprId,
        args: &[ArenaCallArg],
    ) -> Option<CheckedApiArguments> {
        let plan = self.bodies.api_calls.get(&call)?;
        if plan.argument_slots.len() != args.len() {
            return None;
        }
        let mut values = vec![None; plan.sig.params.len()];
        for (arg, &slot) in args.iter().zip(&plan.argument_slots) {
            if values
                .get_mut(slot)?
                .replace(compact_call_arg_expr(arg)?)
                .is_some()
            {
                return None;
            }
        }
        Some(CheckedApiArguments {
            sig: plan.sig,
            values,
        })
    }

    /// The checker's binding of a user callable call's source entries to
    /// parameter slots.
    fn call_argument_slots(&self, call: ExprId) -> Option<&'p [usize]> {
        self.bodies
            .argument_bindings
            .get(&self.program.arena.expr(call).span)
            .map(|binding| binding.argument_slots.as_slice())
    }

    /// Arrange named entries in their checked slots. Named calls evaluate their
    /// entries in source order before reaching here, so slot order is free.
    fn lower_function_call_args(
        &mut self,
        args: &[ArenaCallArg],
        params: Option<&[CallableParamType]>,
        argument_slots: Option<&[usize]>,
        definition: Option<(LoweredFunctionKey, usize)>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredCallArg>> {
        if !args
            .iter()
            .any(|arg| matches!(arg.kind, ArenaCallArgKind::Named { .. }))
        {
            return self.lower_call_args(args, slots, current_function, item_slot);
        }
        let (params, argument_slots) = (params?, argument_slots?);
        let mut values = Vec::new();
        #[allow(clippy::needless_range_loop)]
        for slot in 0..=argument_slots.iter().copied().max()? {
            let mut entries = args
                .iter()
                .zip(argument_slots)
                .filter(|(_, bound)| **bound == slot)
                .peekable();
            if entries.peek().is_none() {
                if params[slot].rest {
                    continue;
                }
                let definitions = self.function_index();
                let parameter = definition.and_then(|(key, skip)| {
                    definitions.definition(key).and_then(|def| {
                        self.program
                            .arena
                            .params(self.program.arena.function_def(def.id).params)
                            .get(slot + skip)
                    })
                });
                let Some(default) = parameter.and_then(|parameter| parameter.default) else {
                    values.push(LoweredCallArg::Default(
                        slot + definition.map_or(0, |(_, skip)| skip),
                    ));
                    continue;
                };
                let value = if let Some(constant) = crate::sema::constants::LiteralConstant::analyze(
                    &self.program.arena,
                    default,
                    &FxHashMap::default(),
                ) {
                    self.lower_record_default(&constant.in_type(&params[slot].ty))?
                } else {
                    self.lower_expr(default, slots, current_function, item_slot)?
                };
                values.push(LoweredCallArg::Single(value));
            }
            for (arg, _) in entries {
                values.push(match arg.kind {
                    ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => {
                        LoweredCallArg::Single(self.lower_expr(
                            value,
                            slots,
                            current_function,
                            item_slot,
                        )?)
                    }
                    ArenaCallArgKind::Splice { value, .. } => LoweredCallArg::Splice(
                        self.lower_expr(value, slots, current_function, item_slot)?,
                    ),
                    ArenaCallArgKind::NamedSpread { .. } => return None,
                });
            }
        }
        Some(values)
    }

    fn lower_record_default(
        &mut self,
        value: &crate::sema::constants::LiteralConstant,
    ) -> Option<BuildExprId> {
        use crate::sema::constants::LiteralConstant as C;
        let row = match value {
            C::Regex(_) | C::Tag { .. } => {
                BuildExprRow::PreparedConstant(super::PreparedConstantValue(
                    lower_literal_constant(value, Some(&self.declarations.wire_enums))?,
                ))
            }
            C::Null => BuildExprRow::Null,
            C::Bool(value) => BuildExprRow::Bool(*value),
            C::Int(value) => BuildExprRow::Int(*value),
            C::Float(value) => BuildExprRow::Float(crate::runtime::value::FloatValue::new(
                f64::from_bits(*value),
            )),
            C::Duration(millis) => BuildExprRow::Duration(DurationValue { millis: *millis }),
            C::Str(value) => BuildExprRow::Str(value.clone()),
            C::Bytes(value) => BuildExprRow::Bytes(value.clone()),
            C::Path(value) => BuildExprRow::Path(PathValue::from_text(value).ok()?),
            C::EmptyMap => BuildExprRow::EmptyMap,
            C::Map(_) | C::Set(_) => BuildExprRow::PreparedConstant(super::PreparedConstantValue(
                lower_literal_constant(value, Some(&self.declarations.wire_enums))?,
            )),
            C::List(values) => BuildExprRow::List(
                values
                    .iter()
                    .map(|value| self.lower_record_default(value))
                    .collect::<Option<Vec<_>>>()?,
            ),
            C::Record(values) => BuildExprRow::Record(
                values
                    .iter()
                    .map(|(name, value)| {
                        Some(LoweredRecordEntry::Field(
                            *name,
                            self.lower_record_default(value)?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?,
            ),
        };
        Some(push_build_row!(self, expr, row))
    }

    fn prepared_cli_plan(
        &self,
        call: &LoweredModuleCallArgs,
        args: &[ArenaCallArg],
    ) -> Option<Arc<crate::modules::cli::CliDescriptorPlan>> {
        if call.semantic_rule == crate::modules::signature::SemanticRule::CliCommands {
            let index = if call.args.len() == 2 { 1 } else { 2 };
            let commands = crate::sema::arguments::ArgumentValueSource::Expression(
                call.args.get(index).copied().flatten()?,
            );
            let fallback = call
                .args
                .get(3)
                .copied()
                .flatten()
                .map(crate::sema::arguments::ArgumentValueSource::Expression);
            return self
                .declarations
                .prepared_constants
                .cli_commands_plan(&self.program.arena, commands, fallback)?
                .ok();
        }
        if call.semantic_rule != crate::modules::signature::SemanticRule::CliDescriptor {
            return None;
        }
        let schema = crate::modules::cli::descriptor_argument(args)?;
        self.declarations
            .prepared_constants
            .cli_descriptor_plan(&self.program.arena, schema, call.op == RuntimeOp::CliApplet)?
            .ok()
    }

    fn lower_call(
        &mut self,
        id: ExprId,
        callee: ExprId,
        args: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let span = self.program.arena.expr(id).span;
        let args_vec = self.program.arena.call_args(args).to_vec();
        if !slots.bound_call_entries.contains(&id)
            && args_vec.iter().any(|arg| {
                matches!(
                    arg.kind,
                    ArenaCallArgKind::NamedSpread { .. } | ArenaCallArgKind::Named { .. }
                )
            })
        {
            return self.lower_named_spread_call(
                id,
                callee,
                args,
                slots,
                current_function,
                item_slot,
            );
        }
        // The checker decided this callee is a value of a callable type and
        // bound the arguments to that type's parameters. Named entries were
        // evaluated in source order above, as for a call by name, so slot
        // order is free here.
        if let Some(callable) = self.bodies.typed_callable_calls.get(&id).cloned() {
            let args = self.lower_function_call_args(
                &args_vec,
                Some(&callable.sig.params),
                self.call_argument_slots(id),
                None,
                slots,
                current_function,
                item_slot,
            )?;
            let callee = self.lower_expr(callee, slots, current_function, item_slot)?;
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::TypedCall {
                    callee,
                    pure: callable.pure,
                    signature: Type::Callable(callable),
                    args,
                    span,
                }
            ));
        }
        if let Some(alias) = self
            .declarations
            .static_callable_aliases
            .get(&self.program.arena.expr(callee).span)
            .cloned()
        {
            let args = self.lower_function_call_args(
                &args_vec,
                Some(&alias.signature.params),
                self.call_argument_slots(id),
                None,
                slots,
                current_function,
                item_slot,
            )?;
            let callee = match self.program.arena.expr(callee).kind {
                ArenaExprKind::Field { base, .. } if alias.method_call => base,
                _ => callee,
            };
            let callee = self.lower_expr(callee, slots, current_function, item_slot)?;
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::DynamicCall { callee, args, span }
            ));
        }
        if let ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } = self.program.arena.expr(callee).kind
            && self.checked_expr_type(base)
                .or_else(|| self.concrete_checked_type(base)).is_some_and(|ty| {
                ty == Type::FsRoot
                    || matches!(ty, Type::Result(ref inner, _) if **inner == Type::FsRoot)
                    || matches!(ty, Type::Optional(ref inner) if **inner == Type::FsRoot && slots.postfix_receivers.contains_key(&base))
            })
        {
            let plan = self.bodies.api_calls.get(&id).filter(|plan| plan.receiver.is_some())?;
            let mut order = vec![None; plan.params.len()];
            for (index, &slot) in plan.argument_slots.iter().enumerate() { order[slot] = Some(index); }
            let receiver = if matches!(self.checked_expr_type(base), Some(Type::Result(_, _))) {
                self.lower_postfix_receiver(base, slots, current_function, item_slot)?
            } else { self.lower_expr(base, slots, current_function, item_slot)? };
            let receiver_slot = slots.reserve("filesystem root receiver");
            let mut bindings = vec![(receiver, receiver_slot)];
            let mut evaluated = Vec::with_capacity(args_vec.len());
            // Evaluate the receiver and argument entries in source order before
            // arranging host slots. Named arguments never reorder effects.
            for arg in &args_vec {
                let value = self.lower_expr(compact_call_arg_expr(arg)?, slots, current_function, item_slot)?;
                let slot = slots.reserve("filesystem root argument");
                bindings.push((value, slot));
                evaluated.push(push_build_row!(self, expr, BuildExprRow::Param(slot)));
            }
            let mut arguments = vec![Some(push_build_row!(self, expr, BuildExprRow::Param(receiver_slot)))];
            arguments.extend(order.into_iter().map(|argument| argument.map(|index| evaluated[index])));
            let call = push_build_row!(self, expr, BuildExprRow::ModuleCall { cli_plan: None, op: plan.sig.op, args: arguments, span });
            return Some(self.wrap_argument_bindings(call, bindings, span));
        }
        if let Some(definition) = self.declarations.record_constructors.resolve_call(
            &self.program.arena,
            callee,
            self.current_namespace,
        ) {
            let defaults = self
                .declarations
                .record_constructors
                .defaults(definition)
                .cloned()
                .unwrap_or_default();
            let schema = self
                .declarations
                .record_constructor_types
                .get(&callee)
                .cloned()
                .or_else(|| {
                    self.declarations.record_constructors.constructor_type(
                        &self.program.arena,
                        callee,
                        self.current_namespace,
                    )
                })?;
            let mut supplied = FxHashSet::default();
            let mut fields = Vec::new();
            for (index, arg) in args_vec.iter().enumerate() {
                // A positional argument supplies the field the checker bound it to.
                let (name, value) = match arg.kind {
                    ArenaCallArgKind::Named { name, value, .. } => (name, value),
                    ArenaCallArgKind::Positional(value) => (
                        *self.bodies.record_constructor_fields.get(&id)?.get(index)?,
                        value,
                    ),
                    ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                        return None;
                    }
                };
                supplied.insert(name);
                let literal = crate::sema::constants::LiteralConstant::analyze(
                    &self.program.arena,
                    value,
                    &FxHashMap::default(),
                );
                let lowered =
                    if let (Some(literal), Type::Record(schema_fields)) = (literal, &schema) {
                        let contextual = literal.clone().in_type(schema_fields.get(&name)?);
                        if contextual != literal {
                            self.lower_record_default(&contextual)?
                        } else {
                            self.lower_expr(value, slots, current_function, item_slot)?
                        }
                    } else {
                        self.lower_expr(value, slots, current_function, item_slot)?
                    };
                fields.push(LoweredRecordEntry::Field(name, lowered));
            }
            for (name, value) in &defaults {
                if !supplied.contains(name) {
                    fields.push(LoweredRecordEntry::Field(
                        *name,
                        self.lower_record_default(&match &schema {
                            Type::Record(fields) => value.clone().in_type(fields.get(name)?),
                            _ => return None,
                        })?,
                    ));
                }
            }
            let value = push_build_row!(self, expr, BuildExprRow::Record(fields));
            let check = LoweredTypeCheck {
                schema: None,
                ty: schema,
                name: match self.program.arena.expr(callee).kind {
                    ArenaExprKind::Ident(name) => name.to_string(),
                    ArenaExprKind::Field { base, name } => match self.program.arena.expr(base).kind
                    {
                        ArenaExprKind::Ident(namespace) => format!("{namespace}.{name}"),
                        _ => return None,
                    },
                    _ => return None,
                }
                .into(),
            };
            let checked = push_build_row!(self, expr, BuildExprRow::Require { value, check, span });
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: checked,
                    span
                }
            ));
        }
        if let Some((error, bindings)) =
            self.lower_compact_error_expr(id, callee, &args_vec, slots, current_function, item_slot)
        {
            let value = push_build_row!(self, expr, BuildExprRow::Error(Box::new(error)));
            return Some(self.wrap_argument_bindings(value, bindings, span));
        }
        match self.program.arena.expr(callee).kind {
            ArenaExprKind::Field { base, name } => {
                if let Some(env_call) = self.lower_env_path_method_call(
                    id,
                    callee,
                    &args_vec,
                    span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(env_call);
                }
                let runtime_module = !matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(module) if slots.resolve(module).is_none());
                if runtime_module
                    && let Some(Type::Module(exports)) = self.checked_expr_type(base)
                    && let Some(
                        ModuleExportType::Pure { sig, .. } | ModuleExportType::Proc { sig, .. },
                    ) = exports.get(&name)
                {
                    let params = sig.params.clone();
                    let args = self.lower_function_call_args(
                        &args_vec,
                        Some(&params),
                        self.call_argument_slots(id),
                        None,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let callee = self.lower_expr(callee, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall { callee, args, span }
                    ));
                }
                if name == "call" {
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    let call = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_expr(base, slots, current_function, item_slot,)?,
                            args,
                            span,
                        }
                    );
                    // The checker typed `.call` on a dynamic proc handle as
                    // a Result without knowing what the proc returns.
                    if self.checked_expr_type(base) == Some(Type::Proc) {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ProcCallResult(call)
                        ));
                    }
                    return Some(call);
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind {
                    if let Some(arity) = self.compact_qualified_tag_variant_arity(module, name) {
                        let positional = positional_call_args(&args_vec)?;
                        if positional.len() != arity {
                            return None;
                        }
                        let types = self.compact_tag_variant_field_types(Name::intern(format!(
                            "{module}.{name}"
                        )))?;
                        let (fields, bindings) = self.lower_checked_call_values(
                            &positional,
                            &types,
                            slots,
                            current_function,
                            item_slot,
                        )?;
                        let value = push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Tag {
                                type_name: self.compact_tag_type_name(Name::intern(format!(
                                    "{module}.{name}"
                                )))?,
                                wire: self
                                    .compact_tag_wire(Name::intern(format!("{module}.{name}"))),
                                name: Arc::<str>::from(name.as_str().as_str()),
                                fields,
                            }
                        );
                        return Some(self.wrap_argument_bindings(value, bindings, span));
                    }
                    if module.as_str() == "error" && name.as_str() == "fail" {
                        // The checker bound the sole `message` parameter.
                        let [argument] = args_vec.as_slice() else {
                            return None;
                        };
                        let message = compact_call_arg_expr(argument)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Fail {
                                message: self.lower_expr(
                                    message,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module.as_str() == "error" && name.as_str() == "failure" {
                        // The checker bound the sole `message` parameter.
                        let [argument] = args_vec.as_slice() else {
                            return None;
                        };
                        let message = self.lower_expr(
                            compact_call_arg_expr(argument)?,
                            slots,
                            current_function,
                            item_slot,
                        )?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: None,
                                op: RuntimeOp::ErrorFailure,
                                args: vec![Some(message)],
                                span,
                            }
                        ));
                    }
                    if module == "Path"
                        && name == "parse_bytes"
                        && let [argument] = args_vec.as_slice()
                    {
                        let bytes = self.lower_expr(
                            compact_call_arg_expr(argument)?,
                            slots,
                            current_function,
                            item_slot,
                        )?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: None,
                                op: RuntimeOp::PathParseBytes,
                                args: vec![Some(bytes)],
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "children" {
                        let options =
                            lower_fs_list_args(&self.checked_api_arguments(id, &args_vec)?)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsList {
                                op: RuntimeOp::FsChildren,
                                path: self.lower_expr(
                                    options.path,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                stat: match options.stat {
                                    Some(stat) => Some(self.lower_expr(
                                        stat,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                ordered: match options.ordered {
                                    Some(ordered) => Some(self.lower_expr(
                                        ordered,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "files" {
                        let options =
                            lower_fs_files_args(&self.checked_api_arguments(id, &args_vec)?)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsFiles {
                                root: self.lower_expr(
                                    options.root,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                gitignore: self.lower_optional_expr(
                                    options.gitignore,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                stat: self.lower_optional_expr(
                                    options.stat,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                hidden: self.lower_optional_expr(
                                    options.hidden,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                exts: match options.exts {
                                    Some(exts) => Some(self.lower_expr(
                                        exts,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                result_wrapped: true,
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "walk" {
                        let options =
                            lower_fs_files_args(&self.checked_api_arguments(id, &args_vec)?)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsWalk {
                                root: self.lower_expr(
                                    options.root,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                gitignore: self.lower_optional_expr(
                                    options.gitignore,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                stat: self.lower_optional_expr(
                                    options.stat,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                hidden: self.lower_optional_expr(
                                    options.hidden,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                exts: match options.exts {
                                    Some(exts) => Some(self.lower_expr(
                                        exts,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                result_wrapped: true,
                                span,
                            }
                        ));
                    }
                    if module == "fs" && name == "tempdir" && args_vec.is_empty() {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::FsTempDir { span }
                        ));
                    }
                    if module == "archive" && name == "tar_create" {
                        let options = lower_archive_tar_create_args(
                            &self.checked_api_arguments(id, &args_vec)?,
                        )?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ArchiveTarCreate {
                                path: self.lower_expr(
                                    options.path,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                root: self.lower_expr(
                                    options.root,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                entries: self.lower_expr(
                                    options.entries,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                compression: match options.compression {
                                    Some(expr) => Some(self.lower_expr(
                                        expr,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                overwrite: match options.overwrite {
                                    Some(expr) => Some(self.lower_expr(
                                        expr,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?),
                                    None => None,
                                },
                                span,
                            }
                        ));
                    }
                    if module == "process" && name == "command_argv" {
                        let options = lower_process_command_argv_args(
                            &self.checked_api_arguments(id, &args_vec)?,
                        )?;
                        let command = LoweredProcessCommandArgv {
                            target: self.lower_expr(
                                options.target,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            argv: self.lower_expr(
                                options.argv,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            cwd: match options.cwd {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            env: match options.env {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stdin: match options.stdin {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stdout: match options.stdout {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stderr: match options.stderr {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stdout_append: match options.stdout_append {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            stderr_append: match options.stderr_append {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            timeout: match options.timeout {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            detach: match options.detach {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            new_session: match options.new_session {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            ignore_hup: match options.ignore_hup {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            cpu_max: match options.cpu_max {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            accept: match options.accept {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        };
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ProcessCommandArgv(Box::new(command))
                        ));
                    }
                    if let Some(script_call) = self.lower_script_module_call(
                        id,
                        &args_vec,
                        span,
                        slots,
                        current_function,
                        item_slot,
                    ) {
                        return Some(script_call);
                    }
                    if let Some(module_call) =
                        lowered_module_call_args(module, name, &args_vec, self.module_call_plan(id))
                    {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: self.prepared_cli_plan(&module_call, &args_vec),
                                op: module_call.op,
                                args: self.lower_module_argument_values(
                                    &module_call.args,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                }
                if name == "read_text" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadText {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "read_bytes" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadBytes {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "exists" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExists {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "executable" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExecutable {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "du" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathDu {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "metadata" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMetadata {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "readlink" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadlink {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "resolve" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathResolve {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "chmod"
                    && let Some(operands) = self.path_chmod_operands(id, &args_vec)
                {
                    let mut args = vec![Some(self.lower_expr(
                        base,
                        slots,
                        current_function,
                        item_slot,
                    )?)];
                    for operand in operands {
                        args.push(match operand {
                            Some(operand) => Some(self.lower_expr(
                                operand,
                                slots,
                                current_function,
                                item_slot,
                            )?),
                            None => None,
                        });
                    }
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::FsChmod,
                            args,
                            span,
                        }
                    ));
                }
                if name == "write"
                    && let Some((data, mode)) = self.path_write_mode_args(id, &args_vec)
                {
                    let args = vec![
                        Some(self.lower_expr(base, slots, current_function, item_slot)?),
                        Some(self.lower_expr(data, slots, current_function, item_slot)?),
                        Some(self.lower_expr(mode, slots, current_function, item_slot)?),
                    ];
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::FsWrite,
                            args,
                            span,
                        }
                    ));
                }
                if name == "write" || name == "write_atomic" {
                    let options =
                        lower_path_write_args(&self.checked_path_method_arguments(id, &args_vec)?)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathWrite {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            data: self.lower_expr(
                                options.data,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            atomic: name == "write_atomic",
                            span,
                        }
                    ));
                }
                if name == "mkdir" {
                    let options =
                        lower_path_mkdir_args(&self.checked_path_method_arguments(id, &args_vec)?);
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMkdir {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            parents: match options.parents {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        }
                    ));
                }
                if name == "remove"
                    && let Some(options) = self
                        .checked_path_method_arguments(id, &args_vec)
                        .map(|args| lower_path_remove_args(&args))
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathRemove {
                            path: self.lower_expr(base, slots, current_function, item_slot,)?,
                            missing_ok: match options.missing_ok {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        }
                    ));
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && module == "hash"
                    && name == "verify_file"
                    && let Some(options) = lower_hash_verify_file_args(&args_vec)
                {
                    return self.lower_hash_verify_file_call(
                        &options,
                        slots,
                        current_function,
                        item_slot,
                        span,
                    );
                }
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && let Some(module_call) =
                        lowered_module_call_args(module, name, &args_vec, self.module_call_plan(id))
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: self.prepared_cli_plan(&module_call, &args_vec),
                            op: module_call.op,
                            args: self.lower_module_argument_values(
                                &module_call.args,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            span,
                        }
                    ));
                }
                let positional = positional_call_args(&args_vec);
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                    && let Some(positional) = positional.as_ref()
                {
                    if module == "Path" && name == "parse_bytes" && positional.len() == 1 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: None,
                                op: RuntimeOp::PathParseBytes,
                                args:
                                    self.lower_expr_ids(
                                        positional,
                                        slots,
                                        current_function,
                                        item_slot,
                                    )?
                                    .into_iter()
                                    .map(Some)
                                    .collect(),
                                span,
                            }
                        ));
                    }
                    if module == "regex" && name == "compile" && positional.len() == 1 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::RegexCompile {
                                pattern: self.lower_expr(
                                    positional[0],
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "map" && name == "empty" && positional.is_empty() {
                        return Some(push_build_row!(self, expr, BuildExprRow::EmptyMap));
                    }
                    // `set.empty()` and `set.from(items)` build the set the
                    // checker typed them as.
                    if module == "set"
                        && matches!(self.bodies.expr_types.get(&id), Some(Type::Set(_)))
                    {
                        if name == "empty" && positional.is_empty() {
                            return Some(self.list_as_set(Vec::new(), span));
                        }
                        if name == "from" && positional.len() == 1 {
                            let items =
                                self.lower_expr(positional[0], slots, current_function, item_slot)?;
                            return Some(self.set_of_list(items, span));
                        }
                    }
                    if module == "bytes" && name == "concat" && positional.len() == 1 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::BytesConcat {
                                arg: self.lower_expr(
                                    positional[0],
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "json" && name == "encode" && positional.len() == 1 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::JsonEncode {
                                value: self.lower_expr(
                                    positional[0],
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "archive" && name == "tar_list" && positional.len() == 1 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ArchiveTarList {
                                path: self.lower_expr(
                                    positional[0],
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "archive" && name == "tar_extract" && positional.len() == 2 {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ArchiveTarExtract {
                                path: self.lower_expr(
                                    positional[0],
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                dest: self.lower_expr(
                                    positional[1],
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                    if module == "hash"
                        && name == "verify_file"
                        && let Some(options) = lower_hash_verify_file_args(&args_vec)
                    {
                        return self.lower_hash_verify_file_call(
                            &options,
                            slots,
                            current_function,
                            item_slot,
                            span,
                        );
                    }
                    if let Some(script_call) = self.lower_script_module_call(
                        id,
                        &args_vec,
                        span,
                        slots,
                        current_function,
                        item_slot,
                    ) {
                        return Some(script_call);
                    }
                    if let Some(module_call) =
                        lowered_module_call_args(module, name, &args_vec, self.module_call_plan(id))
                    {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: self.prepared_cli_plan(&module_call, &args_vec),
                                op: module_call.op,
                                args: self.lower_module_argument_values(
                                    &module_call.args,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?,
                                span,
                            }
                        ));
                    }
                }
                // Named entries reach user functions through the checker's binding.
                if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind {
                    let qualified = self.compact_qualified_function_key(module, name);
                    if self.compact_qualified_function_available(qualified) {
                        let params = self
                            .compact_qualified_function_sig(module, name)
                            .map(|sig| sig.params.clone());
                        let args = self
                            .lower_function_call_args(
                                &args_vec,
                                params.as_deref(),
                                self.call_argument_slots(id),
                                Some((LoweredFunctionKey::Qualified(qualified), 0)),
                                slots,
                                current_function,
                                item_slot,
                            )?
                            .into_iter()
                            .collect();
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Call {
                                function: LoweredFunctionKey::Qualified(qualified),
                                args,
                                span,
                            }
                        ));
                    }
                }
                if !lowered_method_name(&name.as_str()) {
                    if name == "read_text" && args_vec.is_empty() {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::PathReadText {
                                path: self.lower_expr(base, slots, current_function, item_slot,)?,
                                span,
                            }
                        ));
                    }
                    if name == "read_bytes" && args_vec.is_empty() {
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::PathReadBytes {
                                path: self.lower_expr(base, slots, current_function, item_slot,)?,
                                span,
                            }
                        ));
                    }
                    if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                        && let Some(call_args) = lowered_module_call_args(
                            module,
                            name,
                            &args_vec,
                            self.module_call_plan(id),
                        )
                    {
                        let mut lowered = Vec::with_capacity(call_args.args.len());
                        for arg in call_args.args {
                            lowered.push(match arg {
                                Some(arg) => Some(self.lower_expr(
                                    arg,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            });
                        }
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: None,
                                op: call_args.op,
                                args: lowered,
                                span,
                            }
                        ));
                    }
                    if let Some((op, operands)) =
                        self.path_method_module_operands(id, name, &args_vec)
                    {
                        let mut args = vec![Some(self.lower_expr(
                            base,
                            slots,
                            current_function,
                            item_slot,
                        )?)];
                        for operand in operands {
                            args.push(match operand {
                                Some(operand) => Some(self.lower_expr(
                                    operand,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            });
                        }
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: None,
                                op,
                                args,
                                span,
                            }
                        ));
                    }
                    // A checked `Path` method that reaches here has no route. A
                    // dynamic call would find no such method at run time, so
                    // the call is refused now.
                    if self.checked_path_method_arguments(id, &args_vec).is_some() {
                        return None;
                    }
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_expr(callee, slots, current_function, item_slot,)?,
                            args,
                            span,
                        }
                    ));
                }
                // Recognize `<text>.starts_with(n)` / `.ends_with(n)` and lower to the
                // direct StrPredicate node (the bool-condition path then specializes it
                // into StrPredicateSlot/TrimStrPredicateSlot for slot/trim receivers).
                // A Path receiver compares whole components, so it takes the method
                // call below instead of the byte predicate.
                let str_predicate = match name.as_str().as_str() {
                    _ if matches!(
                        self.checked_expr_type(base).as_ref().map(Type::unvalidated),
                        Some(Type::Path)
                    ) =>
                    {
                        None
                    }
                    "starts_with" if args_vec.len() == 1 => Some(LoweredStrPredicate::StartsWith),
                    "ends_with" if args_vec.len() == 1 => Some(LoweredStrPredicate::EndsWith),
                    _ => None,
                };
                if let Some(predicate) = str_predicate
                    && let Some(positional) = positional_call_args(&args_vec)
                    && positional.len() == 1
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::StrPredicate {
                            receiver: self.lower_expr(base, slots, current_function, item_slot,)?,
                            predicate,
                            needle: self.lower_expr(
                                positional[0],
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            span,
                        }
                    ));
                }
                if let Some(script_call) = self.lower_script_method_call(
                    id,
                    base,
                    name,
                    &args_vec,
                    span,
                    slots,
                    current_function,
                    item_slot,
                ) {
                    return Some(script_call);
                }
                let method_args = self.checked_method_call_args(id, &args_vec).or_else(|| {
                    positional_call_args(&args_vec)
                        .map(|args| args.into_iter().map(|arg| (arg, Type::Any)).collect())
                })?;
                if !self.lowered_method_supported_for_receiver(base, name, method_args.len(), slots)
                {
                    return None;
                }
                let receiver = self.lower_expr(base, slots, current_function, item_slot)?;
                let mut bindings = Vec::new();
                let uint_key = matches!(self.checked_expr_type(base), Some(Type::Map(key, _)) if *key == Type::UInt);
                let checked = method_args.iter().enumerate().any(|(position, (_, ty))| {
                    ty.has_unsigned_constraint() && !(uint_key && position == 0)
                });
                let (receiver, mut lowered_args) = if checked {
                    let slot = slots.reserve("method receiver");
                    bindings.push((receiver, slot));
                    let receiver = push_build_row!(self, expr, BuildExprRow::Param(slot));
                    let args: Vec<_> = method_args.iter().map(|(arg, _)| *arg).collect();
                    let types: Vec<_> = method_args
                        .iter()
                        .enumerate()
                        .map(|(position, (_, ty))| {
                            if uint_key && position == 0 {
                                Type::Int
                            } else {
                                ty.clone()
                            }
                        })
                        .collect();
                    let (values, argument_bindings) = self.lower_checked_call_values(
                        &args,
                        &types,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    bindings.extend(argument_bindings);
                    (receiver, values)
                } else {
                    (
                        receiver,
                        self.lower_expr_ids(
                            &method_args.iter().map(|(arg, _)| *arg).collect::<Vec<_>>(),
                            slots,
                            current_function,
                            item_slot,
                        )?,
                    )
                };
                if uint_key && let Some(value) = lowered_args.first_mut() {
                    *value = self
                        .require_uint_key(*value, self.program.arena.expr(method_args[0].0).span);
                }
                if lowered_str_byte_op(&name.as_str(), &lowered_args) {
                    return Some(match name.as_str().as_str() {
                        "byte_len" => {
                            push_build_row!(self, expr, BuildExprRow::StrByteLen { receiver, span })
                        }
                        "byte_at" => {
                            let mut args = lowered_args.into_iter();
                            push_build_row!(
                                self,
                                expr,
                                BuildExprRow::StrByteAt {
                                    receiver,
                                    index: args.next().unwrap(),
                                    span,
                                }
                            )
                        }
                        _ => unreachable!(),
                    });
                }
                let value = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Method {
                        receiver,
                        name: name.as_str(),
                        args: lowered_args,
                        span
                    }
                );
                Some(self.wrap_argument_bindings(value, bindings, span))
            }
            ArenaExprKind::NullSafeField { base, name } => {
                if name == "read_text" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadText {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "read_bytes" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadBytes {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "exists" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExists {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "executable" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathExecutable {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "du" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathDu {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "metadata" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMetadata {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "readlink" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathReadlink {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "resolve" && args_vec.is_empty() {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathResolve {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            span,
                        }
                    ));
                }
                if name == "chmod"
                    && let Some(operands) = self.path_chmod_operands(id, &args_vec)
                {
                    let mut args = vec![Some(self.lower_postfix_receiver(
                        base,
                        slots,
                        current_function,
                        item_slot,
                    )?)];
                    for operand in operands {
                        args.push(match operand {
                            Some(operand) => Some(self.lower_expr(
                                operand,
                                slots,
                                current_function,
                                item_slot,
                            )?),
                            None => None,
                        });
                    }
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::FsChmod,
                            args,
                            span,
                        }
                    ));
                }
                if name == "write"
                    && let Some((data, mode)) = self.path_write_mode_args(id, &args_vec)
                {
                    let args = vec![
                        Some(self.lower_postfix_receiver(
                            base,
                            slots,
                            current_function,
                            item_slot,
                        )?),
                        Some(self.lower_expr(data, slots, current_function, item_slot)?),
                        Some(self.lower_expr(mode, slots, current_function, item_slot)?),
                    ];
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::FsWrite,
                            args,
                            span,
                        }
                    ));
                }
                if name == "write" || name == "write_atomic" {
                    let options =
                        lower_path_write_args(&self.checked_path_method_arguments(id, &args_vec)?)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathWrite {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            data: self.lower_expr(
                                options.data,
                                slots,
                                current_function,
                                item_slot,
                            )?,
                            atomic: name == "write_atomic",
                            span,
                        }
                    ));
                }
                if name == "mkdir" {
                    let options =
                        lower_path_mkdir_args(&self.checked_path_method_arguments(id, &args_vec)?);
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathMkdir {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            parents: match options.parents {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        }
                    ));
                }
                if name == "remove"
                    && let Some(options) = self
                        .checked_path_method_arguments(id, &args_vec)
                        .map(|args| lower_path_remove_args(&args))
                {
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathRemove {
                            path: self.lower_postfix_receiver(
                                base,
                                slots,
                                current_function,
                                item_slot
                            )?,
                            missing_ok: match options.missing_ok {
                                Some(expr) => Some(self.lower_expr(
                                    expr,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            },
                            span,
                        }
                    ));
                }
                if let Some((op, operands)) = self.path_method_module_operands(id, name, &args_vec)
                {
                    let mut args = vec![Some(self.lower_postfix_receiver(
                        base,
                        slots,
                        current_function,
                        item_slot,
                    )?)];
                    for operand in operands {
                        args.push(match operand {
                            Some(operand) => Some(self.lower_expr(
                                operand,
                                slots,
                                current_function,
                                item_slot,
                            )?),
                            None => None,
                        });
                    }
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op,
                            args,
                            span,
                        }
                    ));
                }
                // A checked `Path` method without a route is refused here
                // too, instead of becoming a dynamic call that fails when run.
                if !lowered_method_name(&name.as_str())
                    && self.checked_path_method_arguments(id, &args_vec).is_some()
                {
                    return None;
                }
                let method_args = self.checked_method_call_args(id, &args_vec).or_else(|| {
                    positional_call_args(&args_vec)
                        .map(|args| args.into_iter().map(|arg| (arg, Type::Any)).collect())
                })?;
                if !lowered_method_name(&name.as_str()) {
                    if let ArenaExprKind::Ident(module) = self.program.arena.expr(base).kind
                        && let Some(call_args) = lowered_module_call_args(
                            module,
                            name,
                            &args_vec,
                            self.module_call_plan(id),
                        )
                    {
                        let mut lowered = Vec::with_capacity(call_args.args.len());
                        for arg in call_args.args {
                            lowered.push(match arg {
                                Some(arg) => Some(self.lower_expr(
                                    arg,
                                    slots,
                                    current_function,
                                    item_slot,
                                )?),
                                None => None,
                            });
                        }
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::ModuleCall {
                                cli_plan: None,
                                op: call_args.op,
                                args: lowered,
                                span,
                            }
                        ));
                    }
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_expr(callee, slots, current_function, item_slot,)?,
                            args,
                            span,
                        }
                    ));
                }
                if !self.lowered_method_supported_for_receiver(base, name, method_args.len(), slots)
                {
                    return None;
                }
                let receiver =
                    self.lower_postfix_receiver(base, slots, current_function, item_slot)?;
                let uint_key = matches!(self.checked_expr_type(base), Some(Type::Map(key, _)) if *key == Type::UInt);
                let args: Vec<_> = method_args.iter().map(|(arg, _)| *arg).collect();
                let types: Vec<_> = method_args
                    .iter()
                    .enumerate()
                    .map(|(position, (_, ty))| {
                        if uint_key && position == 0 {
                            Type::Int
                        } else {
                            ty.clone()
                        }
                    })
                    .collect();
                let checked = types.iter().any(Type::has_unsigned_constraint);
                let mut bindings = Vec::new();
                let receiver = if checked {
                    let slot = slots.reserve("method receiver");
                    bindings.push((receiver, slot));
                    push_build_row!(self, expr, BuildExprRow::Param(slot))
                } else {
                    receiver
                };
                let (mut lowered_args, argument_bindings) = self.lower_checked_call_values(
                    &args,
                    &types,
                    slots,
                    current_function,
                    item_slot,
                )?;
                bindings.extend(argument_bindings);
                if uint_key && let Some(value) = lowered_args.first_mut() {
                    *value = self
                        .require_uint_key(*value, self.program.arena.expr(method_args[0].0).span);
                }
                let value = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Method {
                        receiver,
                        name: name.as_str(),
                        args: lowered_args,
                        span
                    }
                );
                Some(self.wrap_argument_bindings(value, bindings, span))
            }
            ArenaExprKind::Ident(name) => {
                let positional = positional_call_args(&args_vec);
                if let Some(arity) = self.compact_tag_variant_arity(name) {
                    let positional = positional.as_ref()?;
                    if positional.len() != arity {
                        return None;
                    }
                    let types = self.compact_tag_variant_field_types(name)?;
                    let (fields, bindings) = self.lower_checked_call_values(
                        positional,
                        &types,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let value = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Tag {
                            type_name: self.compact_tag_type_name(name)?,
                            wire: self.compact_tag_wire(name),
                            name: Arc::<str>::from(name.as_str().as_str()),
                            fields,
                        }
                    );
                    return Some(self.wrap_argument_bindings(value, bindings, span));
                }
                if name == "Err" {
                    let argument_slots = self.call_argument_slots(id)?;
                    let expanded = crate::sema::arguments::expand_named_arguments(
                        self.program,
                        &args_vec,
                        |_| None,
                    )
                    .ok()?;
                    if expanded.len() == 1 {
                        let crate::sema::arguments::ArgumentValueSource::Expression(value) =
                            expanded[0].value
                        else {
                            return None;
                        };
                        let value = self.lower_expr(value, slots, current_function, item_slot)?;
                        return Some(push_build_row!(
                            self,
                            expr,
                            BuildExprRow::Err { value, cause: None }
                        ));
                    }
                    let lowered = self.lower_expanded_argument_values(
                        &expanded,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let mut value = None;
                    let mut cause = None;
                    for (argument, &slot) in lowered.values.into_iter().zip(argument_slots) {
                        if slot == 0 {
                            value = Some(argument);
                        } else {
                            cause = Some(argument);
                        }
                    }
                    let result = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Err {
                            value: value?,
                            cause
                        }
                    );
                    return Some(self.wrap_argument_bindings(result, lowered.bindings, span));
                }
                if name == "Ok"
                    && let Some(positional) = positional.as_ref()
                {
                    let value = match positional.as_slice() {
                        [] if name == "Ok" => push_build_row!(self, expr, BuildExprRow::Unit),
                        [value] => self.lower_expr(*value, slots, current_function, item_slot)?,
                        _ => return None,
                    };
                    return Some(push_build_row!(self, expr, BuildExprRow::Ok(value)));
                }
                if name == "range"
                    && let Some(positional) = positional.as_ref()
                {
                    let (start, end) = match positional.as_slice() {
                        [end] => (None, *end),
                        [start, end] => (Some(*start), *end),
                        _ => return None,
                    };
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Range {
                            start: match start {
                                Some(start) => {
                                    self.lower_expr(start, slots, current_function, item_slot)?
                                }
                                None => push_build_row!(self, expr, BuildExprRow::Int(0)),
                            },
                            end: self.lower_expr(end, slots, current_function, item_slot)?,
                            span,
                        }
                    ));
                }
                if name == "Path"
                    && let Some(positional) = positional.as_ref()
                {
                    let [value] = positional.as_slice() else {
                        return None;
                    };
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PathFrom {
                            value: self.lower_expr(*value, slots, current_function, item_slot,)?,
                            span,
                        }
                    ));
                }
                if name == "env"
                    && let Some(positional) = positional.as_ref()
                {
                    let [value] = positional.as_slice() else {
                        return None;
                    };
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: RuntimeOp::EnvGet,
                            args: vec![Some(self.lower_expr(
                                *value,
                                slots,
                                current_function,
                                item_slot
                            )?)],
                            span,
                        }
                    ));
                }
                if slots.resolve(name).is_some() && current_function != Some(name) {
                    let args =
                        self.lower_call_args(&args_vec, slots, current_function, item_slot)?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DynamicCall {
                            callee: self.lower_bare_ident(name, slots)?,
                            args,
                            span,
                        }
                    ));
                }
                let self_call = current_function == Some(name);
                let function_key = if self_call {
                    None
                } else {
                    Some(self.compact_unqualified_function_key(name)?)
                };
                if !self_call && function_key.is_none() {
                    return None;
                }
                let params = self
                    .compact_unqualified_function_sig(name)
                    .map(|sig| sig.params.clone());
                let lowered_args = self.lower_function_call_args(
                    &args_vec,
                    params.as_deref(),
                    self.call_argument_slots(id),
                    function_key
                        .or_else(|| self.compact_unqualified_function_key(name))
                        .map(|key| (key, 0)),
                    slots,
                    current_function,
                    item_slot,
                )?;
                if let Some(bridge) = function_key.and_then(|key| self.compact_bridge_op(key)) {
                    // A declared representation bridge: the runtime provides
                    // the body, so the call carries the operation and the
                    // bound arguments instead of a function identity.
                    let args = lowered_args
                        .into_iter()
                        .map(|arg| match arg {
                            LoweredCallArg::Single(expr) => Some(expr),
                            LoweredCallArg::Splice(_) | LoweredCallArg::Default(_) => None,
                        })
                        .collect::<Option<Vec<_>>>()?;
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::ModuleCall {
                            cli_plan: None,
                            op: bridge,
                            args: args.into_iter().map(Some).collect(),
                            span,
                        }
                    ));
                }
                if self_call {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::SelfCall {
                            args: lowered_args,
                            span,
                        }
                    ))
                } else if self.compact_direct_pure_call_candidate(
                    function_key.expect("checked unqualified function key"),
                ) {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::DirectPureCall {
                            function: function_key.expect("checked unqualified function key"),
                            args: lowered_args,
                            span,
                        }
                    ))
                } else {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Call {
                            function: function_key.expect("checked unqualified function key"),
                            args: lowered_args,
                            span,
                        }
                    ))
                }
            }
            _ => None,
        }
    }

    fn lower_compact_error_expr(
        &mut self,
        call: ExprId,
        callee: ExprId,
        args: &[ArenaCallArg],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(LoweredErrorExpr, Vec<(BuildExprId, usize)>)> {
        if let ArenaExprKind::Ident(name) = self.program.arena.expr(callee).kind
            && name == Name::ERROR
        {
            let mut kind = None;
            let mut message = None;
            for (index, arg) in args.iter().enumerate() {
                let (name, value) = match arg.kind {
                    ArenaCallArgKind::Named { name, value, .. } => (Some(name), value),
                    ArenaCallArgKind::Positional(value) => (None, value),
                    ArenaCallArgKind::Splice { .. } | ArenaCallArgKind::NamedSpread { .. } => {
                        return None;
                    }
                };
                let ArenaExprKind::Str(text) = self.program.arena.expr(value).kind else {
                    return None;
                };
                let text = self.program.arena.string_literal(text).to_string();
                match name {
                    Some(name) if name == "kind" => kind = Some(text),
                    Some(name) if name == "message" => message = Some(text),
                    None if index == 0 => kind = Some(text),
                    None if index == 1 => message = Some(text),
                    _ => {}
                }
            }
            return Some((
                LoweredErrorExpr::Simple {
                    kind: kind.unwrap_or_default(),
                    message: message.unwrap_or_default(),
                },
                Vec::new(),
            ));
        }

        let ArenaExprKind::Field {
            base,
            name: variant,
        } = self.program.arena.expr(callee).kind
        else {
            return None;
        };
        let family_key = self.resolved_compact_error_family_key(base)?;
        self.lower_error_variant_payload(
            call,
            family_key,
            variant,
            args,
            slots,
            current_function,
            item_slot,
        )
    }

    /// An error variant value from its constructor arguments. The checker
    /// decided which payload field each argument fills; a call it published
    /// no binding for is not lowered.
    fn lower_error_variant_payload(
        &mut self,
        call: ExprId,
        family_key: CompactErrorFamilyKey,
        variant: Name,
        args: &[ArenaCallArg],
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<(LoweredErrorExpr, Vec<(BuildExprId, usize)>)> {
        let family_name = compact_error_family_display(family_key);
        let info = compact_error_family_info(self.declarations, family_key)
            .and_then(|family| family.variants.get(&variant))?;
        let binding = self.bodies.error_constructors.get(&call)?.clone();
        if binding.fields.len() != args.len() {
            return None;
        }
        let mut fields = Vec::with_capacity(info.fields.len());
        let mut bindings = Vec::new();
        let checked = info.fields.values().any(Type::has_unsigned_constraint);
        for (arg, field) in args.iter().zip(&binding.fields) {
            let (ArenaCallArgKind::Named { value, .. } | ArenaCallArgKind::Positional(value)) =
                arg.kind
            else {
                return None;
            };
            let lowered = self.lower_expr(value, slots, current_function, item_slot)?;
            let lowered = if checked {
                let slot = slots.reserve("error payload argument");
                bindings.push((lowered, slot));
                let bound = push_build_row!(self, expr, BuildExprRow::Param(slot));
                self.checked_unsigned_value(
                    bound,
                    info.fields.get(field)?,
                    self.program.arena.expr(value).span,
                )
            } else {
                lowered
            };
            fields.push((Arc::<str>::from(field.as_str().as_str()), lowered));
        }
        if binding.default_message {
            // A variant without a payload always carries its message, so a
            // `{message}` pattern binds what `.message` reads when the
            // constructor omitted it.
            let message = push_build_row!(
                self,
                expr,
                BuildExprRow::Str(format!("{family_name}.{variant}").into())
            );
            fields.push((Arc::<str>::from("message"), message));
        }
        Some((
            LoweredErrorExpr::Structured {
                family: family_name,
                variant: variant.to_string(),
                fields,
                facets: info.facets.clone(),
            },
            bindings,
        ))
    }

    // Constructor namespaces resolve through their defining module, including
    // import aliases; they have no runtime receiver to evaluate.
    fn resolved_compact_error_family_key(&self, base: ExprId) -> Option<CompactErrorFamilyKey> {
        Some(self.resolve_compact_error_family_owner(compact_error_family_key(self.program, base)?))
    }

    fn resolve_compact_error_family_owner(
        &self,
        key: CompactErrorFamilyKey,
    ) -> CompactErrorFamilyKey {
        match key {
            CompactErrorFamilyKey::Local(name) => CompactErrorFamilyKey::Local(name),
            CompactErrorFamilyKey::Qualified(name) => {
                CompactErrorFamilyKey::Qualified(QualifiedName::new(
                    self.compact_imported_module_owner(name.namespace)
                        .unwrap_or(name.namespace),
                    name.member,
                ))
            }
        }
    }

    /// A leading-dot variant, built from the declaration the checker selected
    /// from the expected type. `id` is the call for `.Name(args)` and the
    /// member expression for a bare `.Name`.
    fn lower_inferred_variant(
        &mut self,
        id: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let variant = self.bodies.inferred_variants.get(&id)?.clone();
        let span = self.program.arena.expr(id).span;
        let args = match self.program.arena.expr(id).kind {
            ArenaExprKind::Call { args, .. } => self.program.arena.call_args(args).to_vec(),
            _ => Vec::new(),
        };
        match variant {
            crate::sema::check::InferredVariant::Tag {
                type_name,
                variant,
                field_types,
            } => {
                let positional = positional_call_args(&args)?;
                if positional.len() != field_types.len() {
                    return None;
                }
                let (fields, bindings) = self.lower_checked_call_values(
                    &positional,
                    &field_types,
                    slots,
                    current_function,
                    item_slot,
                )?;
                let value = push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Tag {
                        type_name,
                        wire: self
                            .declarations
                            .wire_enums
                            .mappings
                            .get(&type_name)
                            .cloned(),
                        name: Arc::<str>::from(variant.as_str().as_str()),
                        fields,
                    }
                );
                Some(self.wrap_argument_bindings(value, bindings, span))
            }
            crate::sema::check::InferredVariant::Error { family, variant } => {
                let key = match family.as_str().split_once('.') {
                    Some((namespace, member)) => CompactErrorFamilyKey::Qualified(
                        QualifiedName::new(Name::intern(namespace), Name::intern(member)),
                    ),
                    None => CompactErrorFamilyKey::Local(family),
                };
                let key = self.resolve_compact_error_family_owner(key);
                let (error, bindings) = self.lower_error_variant_payload(
                    id,
                    key,
                    variant,
                    &args,
                    slots,
                    current_function,
                    item_slot,
                )?;
                let value = push_build_row!(self, expr, BuildExprRow::Error(Box::new(error)));
                Some(self.wrap_argument_bindings(value, bindings, span))
            }
        }
    }

    fn compact_function_available(&self, name: Name) -> bool {
        self.functions.map_or_else(
            || {
                self.declarations.pures.contains_key(&name)
                    || self.declarations.procs.contains_key(&name)
                    || self.declarations.streams.contains_key(&name)
            },
            |functions| functions.contains(LoweredFunctionKey::Name(name)),
        )
    }

    fn compact_unqualified_function_key(&self, name: Name) -> Option<LoweredFunctionKey> {
        if let Some(namespace) = self.current_namespace {
            let qualified = QualifiedName::new(namespace, name);
            if self.compact_qualified_function_available(qualified) {
                return Some(LoweredFunctionKey::Qualified(qualified));
            }
            return self.compact_imported_unqualified_function_key(name);
        }
        if self.compact_function_available(name) {
            return Some(LoweredFunctionKey::Name(name));
        }
        self.compact_imported_unqualified_function_key(name)
    }

    fn compact_unqualified_function_sig(
        &self,
        name: Name,
    ) -> Option<&crate::sema::check::CompactFunctionSig> {
        if let Some(namespace) = self.current_namespace {
            let qualified = QualifiedName::new(namespace, name);
            return self
                .declarations
                .qualified_pures
                .get(&qualified)
                .or_else(|| self.declarations.qualified_procs.get(&qualified))
                .or_else(|| self.declarations.qualified_streams.get(&qualified))
                .or_else(|| self.compact_imported_unqualified_function_sig(name));
        }
        self.declarations
            .pures
            .get(&name)
            .or_else(|| self.declarations.procs.get(&name))
            .or_else(|| self.declarations.streams.get(&name))
            .or_else(|| self.compact_imported_unqualified_function_sig(name))
    }

    fn compact_direct_pure_call_candidate(&self, key: LoweredFunctionKey) -> bool {
        let definitions = self.function_index();
        let Some(function) = definitions.definition(key) else {
            return false;
        };
        if !function.pure {
            return false;
        }
        let def = self.program.arena.function_def(function.id);
        let span = self
            .program
            .arena
            .span(self.program.arena.block(def.body).span);
        span.end().saturating_sub(span.start()) <= 12 * 1024
    }

    fn compact_imported_unqualified_function_key(&self, name: Name) -> Option<LoweredFunctionKey> {
        if !matches!(
            self.top_level_known.get(&name).map(|binding| binding.kind),
            Some(LoweredType::Pure | LoweredType::Proc | LoweredType::Stream)
        ) {
            return None;
        }
        self.compact_unique_qualified_function(name)
            .map(LoweredFunctionKey::Qualified)
    }

    fn compact_imported_unqualified_function_sig(
        &self,
        name: Name,
    ) -> Option<&crate::sema::check::CompactFunctionSig> {
        let qualified = self.compact_unique_qualified_function(name)?;
        self.declarations
            .qualified_pures
            .get(&qualified)
            .or_else(|| self.declarations.qualified_procs.get(&qualified))
            .or_else(|| self.declarations.qualified_streams.get(&qualified))
    }

    fn compact_unique_qualified_function(&self, name: Name) -> Option<QualifiedName> {
        let mut found = None;
        for qualified in self
            .declarations
            .qualified_pures
            .keys()
            .chain(self.declarations.qualified_procs.keys())
            .chain(self.declarations.qualified_streams.keys())
            .copied()
            .filter(|qualified| qualified.member == name)
        {
            if !self.compact_qualified_function_available(qualified) {
                continue;
            }
            if found.replace(qualified).is_some() {
                return None;
            }
        }
        found
    }

    fn compact_qualified_function_available(&self, name: QualifiedName) -> bool {
        self.functions.map_or_else(
            || {
                self.declarations.qualified_pures.contains_key(&name)
                    || self.declarations.qualified_procs.contains_key(&name)
                    || self.declarations.qualified_streams.contains_key(&name)
            },
            |functions| functions.contains(LoweredFunctionKey::Qualified(name)),
        )
    }

    fn compact_qualified_function_sig(
        &self,
        module: Name,
        name: Name,
    ) -> Option<&crate::sema::check::CompactFunctionSig> {
        let qualified = self.compact_qualified_function_key(module, name);
        self.declarations
            .qualified_pures
            .get(&qualified)
            .or_else(|| self.declarations.qualified_procs.get(&qualified))
            .or_else(|| self.declarations.qualified_streams.get(&qualified))
    }

    fn compact_qualified_function_key(&self, module: Name, name: Name) -> QualifiedName {
        QualifiedName::new(
            self.compact_imported_module_owner(module).unwrap_or(module),
            name,
        )
    }

    fn compact_imported_module_owner(&self, alias: Name) -> Option<Name> {
        let module_owner = |use_id| {
            let use_stmt = self.program.arena.use_stmt(use_id);
            let imported_as = use_stmt
                .alias
                .or_else(|| self.program.arena.names(use_stmt.path).last());
            if imported_as != Some(alias) {
                return None;
            }
            let key = use_stmt.resolved.as_deref()?;
            self.program
                .modules
                .iter()
                .find(|module| module.key.as_str() == key)
                .map(|module| module.name)
        };

        if let Some(namespace) = self.current_namespace {
            let module = self
                .program
                .modules
                .iter()
                .find(|module| module.name == namespace)?;
            return self
                .program
                .module_statements(module)
                .find_map(|statement| match self.program.arena.stmt(statement).kind {
                    ArenaStmtKind::Use(use_id) => module_owner(use_id),
                    _ => None,
                });
        }

        self.program.statement_ids().find_map(|statement| {
            match self.program.arena.stmt(statement).kind {
                ArenaStmtKind::Use(use_id) => module_owner(use_id),
                _ => None,
            }
        })
    }

    fn lower_bare_ident_stmt(
        &self,
        statement: StmtId,
        name: Name,
        slots: &SlotScope,
    ) -> Option<BuildExprId> {
        let prepared = &self.declarations.prepared_constants;
        if let Some(origin) = prepared
            .tail_bindings
            .get(&self.program.arena.stmt(statement).span)
        {
            let cached = self
                .scratch
                .borrow()
                .prepared_constants
                .get(origin)
                .cloned();
            let value = if let Some(value) = cached {
                value
            } else {
                let value = lower_literal_constant(
                    prepared.values.get(origin)?,
                    Some(&self.declarations.wire_enums),
                )?;
                self.scratch
                    .borrow_mut()
                    .prepared_constants
                    .insert(*origin, value.clone());
                value
            };
            return Some(push_build_row!(
                self,
                expr,
                BuildExprRow::PreparedConstant(super::PreparedConstantValue(value))
            ));
        }
        self.lower_bare_ident(name, slots)
    }

    // `@name` in a command splices a local or, like a `$name` word, a module
    // constant; a constant is not a slot, so it resolves through the
    // prepared constants the way command-word references do.
    fn lower_splice_name(&self, name: Name, span: Span, slots: &SlotScope) -> Option<BuildExprId> {
        self.lower_bare_ident(name, slots).or_else(|| {
            lower_command_word_reference(
                name.as_str().as_str(),
                slots,
                span,
                &self.scratch,
                &self.declarations.prepared_constants,
                &self.declarations.wire_enums,
                self.current_namespace,
            )
        })
    }

    fn lower_bare_ident(&self, name: Name, slots: &SlotScope) -> Option<BuildExprId> {
        slots
            .resolve(name)
            .map(|slot| push_build_row!(self, expr, BuildExprRow::Param(slot)))
            .or_else(|| {
                if let Some(key) = self.compact_unqualified_function_key(name) {
                    // Function bodies lower with every function as an in-flight
                    // candidate, which `pure_contains` counts as pure; the
                    // definition decides, so a proc alias called in a body
                    // resolves as a proc instead of failing at runtime.
                    let pure = self.function_index().definition(key).map_or_else(
                        || {
                            self.functions
                                .is_none_or(|functions| functions.pure_contains(key))
                        },
                        |function| function.pure,
                    );
                    return Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::FunctionRef {
                            function: match key {
                                LoweredFunctionKey::Name(name) => name.into(),
                                LoweredFunctionKey::Qualified(name) => name.into(),
                            },
                            pure,
                        }
                    ));
                }
                if self.compact_tag_variant_arity(name) != Some(0) {
                    return None;
                }
                Some({
                    push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Tag {
                            type_name: self.compact_tag_type_name(name)?,
                            wire: self.compact_tag_wire(name),
                            name: Arc::<str>::from(name.as_str().as_str()),
                            fields: Default::default(),
                        }
                    )
                })
            })
    }

    fn fuse_par_map_flat_map_reduce_by(&self, stages: &mut Vec<LoweredPipelineStage>) {
        let mut fused = Vec::with_capacity(stages.len());
        let mut index = 0;
        while index < stages.len() {
            if let Some((slot, body, jobs, value)) = Self::lowered_par_map_parts(&stages[index]) {
                if index + 2 < stages.len()
                    && self.lowered_flat_map_is_identity(&stages[index + 1])
                    && let Some((reduce_item_slot, reduce_body, reduce_value, op)) =
                        Self::lowered_reduce_by_parts(&stages[index + 2])
                {
                    fused.push(LoweredPipelineStage::ParMapFlatMapReduceBy {
                        slot,
                        body,
                        jobs,
                        value,
                        flatten: true,
                        reduce_item_slot,
                        reduce_body,
                        reduce_value,
                        op,
                    });
                    index += 3;
                    continue;
                }
                if index + 1 < stages.len()
                    && let Some((reduce_item_slot, reduce_body, reduce_value, op)) =
                        Self::lowered_reduce_by_parts(&stages[index + 1])
                {
                    fused.push(LoweredPipelineStage::ParMapFlatMapReduceBy {
                        slot,
                        body,
                        jobs,
                        value,
                        flatten: false,
                        reduce_item_slot,
                        reduce_body,
                        reduce_value,
                        op,
                    });
                    index += 2;
                    continue;
                }
            }
            fused.push(stages[index].clone());
            index += 1;
        }
        *stages = fused;
    }

    fn lowered_par_map_parts(
        stage: &LoweredPipelineStage,
    ) -> Option<(
        usize,
        Option<Vec<BuildStmtId>>,
        Option<BuildExprId>,
        BuildExprId,
    )> {
        match stage {
            LoweredPipelineStage::ParMap { slot, jobs, value } => {
                Some((*slot, None, *jobs, *value))
            }
            LoweredPipelineStage::ParMapBlock {
                slot,
                body,
                jobs,
                value,
            } => Some((*slot, Some(body.clone()), *jobs, *value)),
            _ => None,
        }
    }

    fn lowered_flat_map_is_identity(&self, stage: &LoweredPipelineStage) -> bool {
        let (slot, body, value) = match stage {
            LoweredPipelineStage::FlatMap { slot, value } => (*slot, true, *value),
            LoweredPipelineStage::FlatMapBlock { slot, body, value } => {
                (*slot, body.is_empty(), *value)
            }
            _ => return false,
        };
        if !body {
            return false;
        }
        matches!(
            self.scratch.borrow().expressions.get(value.index()),
            Some(BuildExprRow::Param(param)) if *param == slot
        )
    }

    fn lowered_reduce_by_parts(
        stage: &LoweredPipelineStage,
    ) -> Option<(usize, Vec<BuildStmtId>, BuildExprId, ReduceByOp)> {
        match stage {
            LoweredPipelineStage::ReduceBy {
                item_slot,
                body,
                value,
                op,
                jobs: None,
            } => Some((*item_slot, body.clone(), *value, *op)),
            _ => None,
        }
    }

    fn lower_pipeline_stage(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<LoweredPipelineStage> {
        // The normalized stage keeps the span but carries the callable as its block.
        if stage.block.is_none()
            && let Some(entry) = self
                .bodies
                .argument_bindings
                .get(&self.program.arena.span(stage.span))
                .and_then(|binding| binding.callable_entry)
        {
            let callee =
                compact_call_arg_expr(self.program.arena.call_args(stage.args).get(entry)?)?;
            // Checked aliases carry the same argument and effect contract as
            // named declarations while retaining their captured callable handle.
            let stable = self
                .declarations
                .static_callable_aliases
                .contains_key(&self.program.arena.expr(callee).span)
                || match self.program.arena.expr(callee).kind {
                    ArenaExprKind::Ident(name) => {
                        slots.resolve(name).is_none()
                            && self.compact_unqualified_function_key(name).is_some()
                    }
                    ArenaExprKind::Field { base, name } => {
                        matches!(self.program.arena.expr(base).kind, ArenaExprKind::Ident(namespace)
                    if (crate::sema::stage_arguments::stage_namespace_owner(self.program, namespace, self.current_namespace).is_some()
                        || slots.resolve(namespace).is_none()) && (self.compact_qualified_function_available(self.compact_qualified_function_key(namespace, name))
                        || api_spec().module_overloads(&namespace.as_str(), &name.as_str()).is_some()))
                    }
                    _ => false,
                };
            if !stable {
                return None;
            }
            let mut temporary = self.program.clone();
            let mut bodies = self.bodies.clone();
            let mut normalized = stage.clone();
            normalized.args = temporary.arena.append_call_arguments(
                &crate::sema::stage_arguments::stage_configuration_arguments(
                    self.program,
                    stage,
                    entry,
                ),
            );
            let span = self.program.arena.expr(callee).span;
            let (block, item, call, stmt) =
                temporary.arena.append_stage_callable_block(callee, span);
            normalized.block = Some(block);
            bodies
                .expr_types
                .insert(item, item_ty.cloned().unwrap_or(Type::Any));
            // The checker types the synthetic one-item call at the callee's span.
            bodies
                .expr_types
                .insert(call, self.bodies.expr_types.get(&callee)?.clone());
            if let Some(plan) = self.bodies.api_calls.get(&callee) {
                bodies.api_calls.insert(call, plan.clone());
            }
            let unit = matches!(stage.kind, StreamStageKind::Each | StreamStageKind::Tee);
            bodies.statement_positions.insert(
                stmt,
                if unit {
                    crate::sema::check::StatementPosition::Statement
                } else {
                    crate::sema::check::StatementPosition::Value
                },
            );
            let mut child = CompactLowerConstructProbe {
                program: &temporary,
                bodies: &bodies,
                declarations: self.declarations,
                source: self.source,
                sources: self.sources,
                current_namespace: self.current_namespace,
                functions: self.functions,
                top_level_known: self.top_level_known.clone(),
                output: std::mem::take(&mut self.output),
                last_blocker_detail: self.last_blocker_detail.take(),
                stdlib_linkage: self.stdlib_linkage,
                function_defs: Rc::clone(&self.function_defs),
                scratch: Rc::clone(&self.scratch),
                // The child's program is this stage's own copy.
                spread_programs: Rc::default(),
            };
            let result = child.lower_pipeline_stage(&normalized, slots, current_function, item_ty);
            self.output = child.output;
            self.last_blocker_detail = child.last_blocker_detail;
            return result;
        }
        if !xsh_registry::stream_parameters::stage_parameters(stage.kind.as_str()).is_empty() {
            return self.lower_configured_pipeline_stage(stage, slots, current_function, item_ty);
        }
        match stage.kind {
            StreamStageKind::TextStreamLines => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::TextLines)
            }
            StreamStageKind::JsonLines | StreamStageKind::JsonStream => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::JsonLines)
            }
            StreamStageKind::Enumerate => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Enumerate)
            }
            StreamStageKind::Zip => None,
            StreamStageKind::Sort => None,
            StreamStageKind::Sum => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Sum)
            }
            StreamStageKind::Collect => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Collect)
            }
            StreamStageKind::First => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::First)
            }
            StreamStageKind::Last => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Last)
            }
            StreamStageKind::Min => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Min)
            }
            StreamStageKind::Max => {
                if stage.block.is_some() || !stage.args.is_empty() {
                    return None;
                }
                Some(LoweredPipelineStage::Max)
            }
            StreamStageKind::SortBy => None,
            StreamStageKind::UniqueBy => {
                if let Some((slot, key)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::UniqueBy { slot, key });
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::UniqueBy { slot, key })
            }
            StreamStageKind::GroupBy => {
                if let Some((slot, key)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::GroupBy { slot, key });
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::GroupBy { slot, key })
            }
            StreamStageKind::Count => {
                if !stage.args.is_empty() {
                    if stage.block.is_some() {
                        return None;
                    }
                    if let Some((slot, key)) = self.try_lower_pipeline_stage_shorthand(
                        stage,
                        slots,
                        current_function,
                        item_ty,
                    ) {
                        return Some(LoweredPipelineStage::CountBy { slot, key });
                    }
                    return None;
                }
                if stage.block.is_none() {
                    return Some(LoweredPipelineStage::Count);
                }
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::CountBy { slot, key })
            }
            StreamStageKind::Where => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Where { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Where { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::WhereBlock { slot, body, value })
            }
            StreamStageKind::Map => {
                if !stage.args.is_empty() {
                    if stage.block.is_some() {
                        return None;
                    }
                    let args = self.program.arena.call_args(stage.args);
                    let [arg] = args else {
                        return None;
                    };
                    let ArenaCallArgKind::Positional(expr) = arg.kind else {
                        return None;
                    };
                    let (slot, _cleanup) =
                        self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                    let value = self.lower_expr(expr, slots, current_function, Some(slot))?;
                    cleanup_pipeline_stage_item_slot(slots, _cleanup, slot);
                    return Some(LoweredPipelineStage::Map { slot, value });
                }
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Map { slot, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::MapBlock { slot, body, value })
            }
            StreamStageKind::FlatMap => {
                if let Some((slot, value)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::FlatMap { slot, value });
                }
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::FlatMap { slot, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::FlatMapBlock { slot, body, value })
            }
            StreamStageKind::Any => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Any { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::Any { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::AnyBlock { slot, body, value })
            }
            StreamStageKind::All => {
                if let Some((slot, predicate)) =
                    self.try_lower_pipeline_stage_shorthand(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::All { slot, predicate });
                }
                if let Some((slot, predicate)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::All { slot, predicate });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::AllBlock { slot, body, value })
            }
            StreamStageKind::Take | StreamStageKind::Drop => None,
            StreamStageKind::Repeat => None,
            StreamStageKind::Range => None,
            StreamStageKind::BytesChunks => None,
            StreamStageKind::Batch => None,
            StreamStageKind::ParMap => None,
            StreamStageKind::Each => {
                let block = stage.block?;
                let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                let saved = slots.enter();
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                Some(LoweredPipelineStage::Each { slot, body })
            }
            StreamStageKind::Tee => {
                let block = stage.block?;
                let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
                let saved = slots.enter();
                let body =
                    self.lower_block_in_current_scope(block, slots, current_function, Some(slot))?;
                slots.exit(saved);
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                Some(LoweredPipelineStage::Tee { slot, body })
            }
            StreamStageKind::TablePrint => None,
            StreamStageKind::ReduceBy => None,
            StreamStageKind::Shuffle => None,
            StreamStageKind::Fold | StreamStageKind::Reduce => None,
        }
    }

    // The first consumed configuration value initializes source-ordered checked
    // temporaries. Later configuration fields read those slots at the same stage
    // boundary; record spreads and effectful expressions are never repeated.
    fn lower_configured_pipeline_stage(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<LoweredPipelineStage> {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        let params = crate::sema::stage_arguments::stage_argument_params(stage.kind.as_str());
        let span = self.program.arena.span(stage.span);
        let argument_slots = &self.bodies.argument_bindings.get(&span)?.argument_slots;
        let expanded = expand_named_arguments(
            self.program,
            self.program.arena.call_args(stage.args),
            |expr| {
                self.bodies
                    .expr_types
                    .get(&expr)
                    .cloned()
                    .or_else(|| self.checked_expr_type(expr))
            },
        )
        .ok()?;
        let lowered =
            self.lower_expanded_argument_values(&expanded, slots, current_function, None)?;
        let mut values = vec![None; params.len()];
        let mut types = vec![None; params.len()];
        let mut booleans = vec![Some(false); params.len()];
        for ((argument, &slot), value) in expanded.iter().zip(argument_slots).zip(lowered.values) {
            values[slot] = Some(value);
            types[slot] = Some(argument.ty.clone());
            booleans[slot] = match argument.value {
                ArgumentValueSource::Expression(expr) => match self.program.arena.expr(expr).kind {
                    ArenaExprKind::Bool(value) => Some(value),
                    _ => None,
                },
                _ => None,
            };
        }
        let wrap = |this: &mut Self, value| {
            this.wrap_argument_bindings(value, lowered.bindings.clone(), span)
        };
        let record = |this: &mut Self| {
            let fields = params
                .iter()
                .zip(&values)
                .filter_map(|(parameter, value)| {
                    value.map(|value| LoweredRecordEntry::Field(parameter.name, value))
                })
                .collect();
            let record = push_build_row!(this, expr, BuildExprRow::Record(fields));
            wrap(this, record)
        };
        match stage.kind {
            StreamStageKind::ParMap => {
                let jobs = values[0].map(|value| wrap(self, value));
                if let Some((slot, value)) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)
                {
                    return Some(LoweredPipelineStage::ParMap { slot, jobs, value });
                }
                let (slot, body, value) =
                    self.lower_pipeline_stage_block(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::ParMapBlock {
                    slot,
                    body,
                    jobs,
                    value,
                })
            }
            StreamStageKind::Sort => Some(LoweredPipelineStage::Sort {
                descending: values[0].map(|value| wrap(self, value)),
            }),
            StreamStageKind::SortBy => {
                let (slot, key) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                Some(LoweredPipelineStage::SortBy {
                    slot,
                    key,
                    descending: values[0].map(|value| wrap(self, value)),
                })
            }
            StreamStageKind::Batch => match values.as_slice() {
                [Some(count), None, None] => Some(LoweredPipelineStage::BatchCount {
                    count: wrap(self, *count),
                }),
                [None, Some(max_bytes), None] => Some(LoweredPipelineStage::BatchMaxBytes {
                    max_bytes: wrap(self, *max_bytes),
                }),
                [None, None, Some(_)] if booleans[2] == Some(true) => {
                    Some(LoweredPipelineStage::BatchMaxArgv { max_argv: None })
                }
                _ => Some(LoweredPipelineStage::BatchLimits {
                    configuration: record(self),
                }),
            },
            StreamStageKind::ReduceBy => {
                let (item_slot, value) =
                    self.lower_pipeline_stage_expr(stage, slots, current_function, item_ty)?;
                let body = Vec::new();
                if booleans[..3].iter().all(Option::is_some) {
                    let op = match booleans[..3]
                        .iter()
                        .position(|value| *value == Some(true))?
                    {
                        0 => ReduceByOp::Sum,
                        1 => ReduceByOp::Min,
                        _ => ReduceByOp::Max,
                    };
                    let jobs = values[3].map(|value| wrap(self, value));
                    Some(LoweredPipelineStage::ReduceBy {
                        item_slot,
                        body,
                        value,
                        op,
                        jobs,
                    })
                } else {
                    Some(LoweredPipelineStage::ReduceByConfigured {
                        item_slot,
                        body,
                        value,
                        configuration: record(self),
                    })
                }
            }
            StreamStageKind::Take => Some(LoweredPipelineStage::Take(wrap(self, values[0]?))),
            StreamStageKind::Drop => Some(LoweredPipelineStage::Drop(wrap(self, values[0]?))),
            StreamStageKind::Repeat => Some(LoweredPipelineStage::Repeat {
                count: wrap(self, values[0]?),
            }),
            StreamStageKind::Range => Some(LoweredPipelineStage::Range {
                start: wrap(self, values[0]?),
                end: values[1]?,
            }),
            StreamStageKind::BytesChunks => Some(LoweredPipelineStage::BytesChunks {
                size: wrap(self, values[0]?),
            }),
            StreamStageKind::Zip => Some(LoweredPipelineStage::Zip {
                other: wrap(self, values[0]?),
            }),
            StreamStageKind::Fold | StreamStageKind::Reduce => {
                let initial = wrap(self, values[0]?);
                self.lower_pipeline_stage_fold(
                    stage,
                    slots,
                    current_function,
                    item_ty,
                    initial,
                    types[0].clone(),
                )
            }
            StreamStageKind::Shuffle => Some(LoweredPipelineStage::Shuffle {
                seed: values[0].map(|value| wrap(self, value)),
            }),
            StreamStageKind::TablePrint => Some(match values[0] {
                Some(value) => LoweredPipelineStage::TablePrintConfigured {
                    columns: wrap(self, value),
                },
                None => LoweredPipelineStage::TablePrint { columns: None },
            }),
            _ => None,
        }
    }

    fn try_lower_pipeline_stage_shorthand(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        _item_ty: Option<&Type>,
    ) -> Option<(usize, BuildExprId)> {
        if stage.block.is_some() || stage.args.is_empty() {
            return None;
        }
        let args = self.program.arena.call_args(stage.args);
        let [arg] = args else {
            return None;
        };
        let ArenaCallArgKind::Positional(expr) = arg.kind else {
            return None;
        };
        let slot = slots.reserve("pipeline.item");
        let value = self.lower_expr(expr, slots, current_function, Some(slot))?;
        Some((slot, value))
    }

    // The bound initializer type stays on the accumulator slot. Nested pipeline
    // tails resolve their field and stage argument types from that slot, so
    // retaining the checked type is necessary for valid compositions to lower.
    // Callback parameters have their own lexical scope and may shadow outer names.
    fn lower_pipeline_stage_fold(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
        initial: BuildExprId,
        acc_ty: Option<Type>,
    ) -> Option<LoweredPipelineStage> {
        let block = stage.block?;
        let saved = slots.enter();
        let params = self
            .program
            .arena
            .block_params(self.program.arena.block(block).params);
        let acc_slot = match params {
            [] => slots.reserve("pipeline.acc"),
            [acc] | [acc, _] => {
                if slots.is_declared_here(acc.name) {
                    slots.exit(saved);
                    return None;
                }
                slots.declare_with_type(acc.name, acc_ty)
            }
            _ => {
                slots.exit(saved);
                return None;
            }
        };
        let item_slot = match params {
            [_, item] => {
                if slots.is_declared_here(item.name) {
                    slots.exit(saved);
                    return None;
                }
                slots.declare_with_type(item.name, item_ty.cloned())
            }
            _ => slots.reserve("pipeline.item"),
        };
        let body = Vec::new();
        let value =
            match self.lower_block_value_expr(block, slots, current_function, Some(item_slot)) {
                Some(value) => value,
                None => {
                    slots.exit(saved);
                    return None;
                }
            };
        slots.exit(saved);
        Some(LoweredPipelineStage::Fold {
            acc_slot,
            item_slot,
            initial,
            body,
            value,
        })
    }

    fn lower_pipeline_stage_expr(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<(usize, BuildExprId)> {
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (slot, cleanup) = self.lower_pipeline_stage_item_slot(stage, slots, item_ty)?;
        let lowered = match statements.as_slice() {
            [stmt] => self
                .lower_tail_stmt_as_expr(*stmt, slots, current_function, Some(slot))
                .or_else(|| {
                    self.lower_block_value_expr(block, slots, current_function, Some(slot))
                }),
            _ => self.lower_block_value_expr(block, slots, current_function, Some(slot)),
        };
        let expr = match lowered {
            Some(expr) => expr,
            None => {
                cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
                return None;
            }
        };
        cleanup_pipeline_stage_item_slot(slots, cleanup, slot);
        Some((slot, expr))
    }

    fn lower_pipeline_stage_block(
        &mut self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_ty: Option<&Type>,
    ) -> Option<(usize, Vec<BuildStmtId>, BuildExprId)> {
        let block = stage.block?;
        let statements = self.program.arena.block(block).statements;
        let ids = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let (&tail, prefix) = ids.split_last()?;
        let saved = slots.enter();
        let (slot, _cleanup) = match self.lower_pipeline_stage_item_slot(stage, slots, item_ty) {
            Some(value) => value,
            None => {
                slots.exit(saved);
                return None;
            }
        };
        let mut body = Vec::with_capacity(prefix.len());
        for stmt in prefix {
            let Some(lowered) =
                self.lower_stmt_with_blocker_guard(*stmt, slots, current_function, Some(slot))
            else {
                slots.exit(saved);
                return None;
            };
            body.push(lowered);
        }
        let value = match self.lower_tail_stmt_as_expr(tail, slots, current_function, Some(slot)) {
            Some(value) => value,
            None => {
                slots.exit(saved);
                return None;
            }
        };
        slots.exit(saved);
        Some((slot, body, value))
    }

    fn lower_pipeline_stage_item_slot(
        &self,
        stage: &ArenaStreamStage,
        slots: &mut SlotScope,
        item_ty: Option<&Type>,
    ) -> Option<(usize, Option<Name>)> {
        let block = stage.block?;
        let params = self
            .program
            .arena
            .block_params(self.program.arena.block(block).params);
        match params {
            [] => Some((slots.reserve("pipeline.item"), None)),
            [param] => {
                // Retirement restores any outer slot and its checked type after
                // the callback, so this temporary binding may shadow a local.
                Some((
                    slots.declare_with_type(param.name, item_ty.cloned()),
                    Some(param.name),
                ))
            }
            _ => None,
        }
    }

    /// Lower an `if` in tail (return) position into a `BuildStmtRow::If`/`IfBool`
    /// whose branch bodies (and else body) are lowered as tail-blocks — each
    /// branch's trailing expression becomes a `Return`. This handles a tail
    /// `if cond { a } else { b }` whose branches produce a value, which a plain
    /// statement-if would discard. Returns `None` if any branch cannot lower.
    fn lower_tail_if_stmt(
        &mut self,
        branches: crate::syntax::arena::ArenaRange,
        else_block: Option<BlockId>,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let branches = self.program.arena.if_branches(branches).to_vec();
        let mut lowered = Vec::with_capacity(branches.len());
        for branch in &branches {
            // Sibling branches restore both ordinary locals and captures.
            let saved = slots.enter();
            let (condition, captures) = self.lower_pattern_condition_parts(
                branch.condition,
                slots,
                current_function,
                item_slot,
            )?;
            let body = self.lower_tail_block(branch.block, slots, current_function, item_slot);
            slots.exit(saved);
            lowered.push((condition, body?, captures));
        }
        let else_body = match else_block {
            Some(block) => {
                let saved = slots.enter();
                let body = self.lower_tail_block(block, slots, current_function, item_slot);
                slots.exit(saved);
                Some(body?)
            }
            None => None,
        };
        let has_pattern = branches.iter().any(|branch| {
            matches!(
                self.program.arena.expr(branch.condition).kind,
                ArenaExprKind::PatternCondition { .. }
            )
        });
        if has_pattern {
            return Some(push_build_row!(
                self,
                stmt,
                BuildStmtRow::PatternIf {
                    branches: lowered,
                    else_body,
                    span: self.program.arena.expr(branches[0].condition).span
                }
            ));
        }
        let lowered = lowered
            .into_iter()
            .map(|(condition, body, _)| (condition, body))
            .collect::<Vec<_>>();
        let mut bool_branches = Vec::with_capacity(lowered.len());
        for (condition, body) in &lowered {
            let Some(condition) = self.lower_bool_expr_candidate(condition) else {
                return Some(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::If {
                        branches: lowered,
                        else_body,
                    }
                ));
            };
            bool_branches.push((condition, body.clone()));
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::IfBool {
                branches: bool_branches,
                else_body,
            }
        ))
    }

    /// Lower a `match` in tail (return) position into a `BuildStmtRow::Match`
    /// whose arm bodies are lowered as tail-blocks — each arm's trailing
    /// expression becomes a `Return`. This handles arms whose body is a
    /// multi-statement block producing a value (e.g. `P => { let a = ..; a }`),
    /// which cannot be expressed as a `MatchExpr` (there is no block-expression
    /// form). Returns `None` if any arm cannot be lowered.
    fn lower_tail_match_stmt(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let (ok_binding_ty, err_binding_ty) = self.compact_match_scrutinee_result_types(value);
        let value = self.lower_expr(value, slots, current_function, item_slot)?;
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = Vec::with_capacity(arms.len());
        for arm in arms {
            if !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let (pattern, cleanup) = self.lower_pattern(
                arm.pattern,
                slots,
                ok_binding_ty.as_ref(),
                err_binding_ty.as_ref(),
            )?;
            // Each arm body gets its own scope so block-local bindings (e.g.
            // `var parts`) don't leak into sibling arms. (The regular match path
            // gets this from `lower_block`; the tail path uses `lower_tail_block`
            // which doesn't scope on its own.)
            let saved = slots.enter();
            let body = self.lower_tail_block(arm.block, slots, current_function, item_slot);
            slots.exit(saved);
            let body = match body {
                Some(body) => body,
                None => {
                    cleanup_lowered_pattern_slots(slots, cleanup);
                    return None;
                }
            };
            let guard = match arm.guard {
                Some(guard_expr) => {
                    match self.lower_expr(guard_expr, slots, current_function, item_slot) {
                        Some(guard) => Some(guard),
                        None => {
                            cleanup_lowered_pattern_slots(slots, cleanup);
                            return None;
                        }
                    }
                }
                None => None,
            };
            cleanup_lowered_pattern_slots(slots, cleanup);
            lowered_arms.push((pattern, guard, body));
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Match {
                value,
                arms: lowered_arms,
                span,
            }
        ))
    }

    fn lower_match_stmt_as_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        if let Some(expr) =
            self.lower_str_match_stmt_as_expr(value, arms, span, slots, current_function, item_slot)
        {
            return Some(expr);
        }
        if let Some(expr) =
            self.lower_tag_match_stmt_as_expr(value, arms, span, slots, current_function, item_slot)
        {
            return Some(expr);
        }
        let arms = self.program.arena.match_arms(arms).to_vec();
        let (ok_binding_ty, err_binding_ty) = self.compact_match_scrutinee_result_types(value);
        let mut lowered_arms = Vec::with_capacity(arms.len());
        for arm in arms {
            if !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let (pattern, cleanup) = self.lower_pattern(
                arm.pattern,
                slots,
                ok_binding_ty.as_ref(),
                err_binding_ty.as_ref(),
            )?;
            let value =
                match self.lower_block_value_expr(arm.block, slots, current_function, item_slot) {
                    Some(value) => value,
                    None => {
                        cleanup_lowered_pattern_slots(slots, cleanup);
                        return None;
                    }
                };
            let guard = match arm.guard {
                Some(guard_expr) => {
                    Some(self.lower_expr(guard_expr, slots, current_function, item_slot)?)
                }
                None => None,
            };
            cleanup_lowered_pattern_slots(slots, cleanup);
            lowered_arms.push((pattern, guard, value));
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::MatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                span,
            }
        ))
    }

    fn lower_str_match_stmt_as_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() || !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let statements = self.program.arena.block(arm.block).statements;
            let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
            let [stmt] = statements.as_slice() else {
                return None;
            };
            match self.pattern_str_literals(arm.pattern)? {
                Some(patterns) => {
                    let value =
                        self.lower_arm_value_expr(*stmt, slots, current_function, item_slot)?;
                    for pattern in patterns {
                        lowered_arms.entry(pattern).or_insert_with(|| value);
                    }
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback = Some(self.lower_arm_value_expr(
                        *stmt,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::StrMatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    fn lower_str_match_stmt(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() || !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            match self.pattern_str_literals(arm.pattern)? {
                Some(patterns) => {
                    let body = self.lower_block(arm.block, slots, current_function, item_slot)?;
                    for pattern in patterns {
                        lowered_arms.entry(pattern).or_insert_with(|| body.clone());
                    }
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback =
                        Some(self.lower_block(arm.block, slots, current_function, item_slot)?);
                }
            }
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::StrMatch {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    fn lower_str_match_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let arms = self.program.arena.match_expr_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() {
                return None;
            }
            match self.pattern_str_literals(arm.pattern)? {
                Some(patterns) => {
                    let value = self.lower_expr(arm.value, slots, current_function, item_slot)?;
                    for pattern in patterns {
                        lowered_arms.entry(pattern).or_insert_with(|| value);
                    }
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback =
                        Some(self.lower_expr(arm.value, slots, current_function, item_slot)?);
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::StrMatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    fn lower_tag_match_stmt_as_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let Type::Tag(type_name) = self.checked_expr_type(value)? else {
            return None;
        };
        if self
            .declarations
            .wire_enums
            .mappings
            .contains_key(&type_name)
        {
            return None;
        }
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() || !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            let statements = self.program.arena.block(arm.block).statements;
            let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
            let [stmt] = statements.as_slice() else {
                return None;
            };
            match self.pattern_tag_name(arm.pattern)? {
                Some(pattern) => {
                    let value =
                        self.lower_arm_value_expr(*stmt, slots, current_function, item_slot)?;
                    lowered_arms.entry(pattern).or_insert(value);
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback = Some(self.lower_arm_value_expr(
                        *stmt,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::TagMatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    fn lower_tag_match_stmt(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let Type::Tag(type_name) = self.checked_expr_type(value)? else {
            return None;
        };
        if self
            .declarations
            .wire_enums
            .mappings
            .contains_key(&type_name)
        {
            return None;
        }
        let arms = self.program.arena.match_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() || !self.program.arena.block(arm.block).params.is_empty() {
                return None;
            }
            match self.pattern_tag_name(arm.pattern)? {
                Some(pattern) => {
                    let body = self.lower_block(arm.block, slots, current_function, item_slot)?;
                    lowered_arms.entry(pattern).or_insert(body);
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback =
                        Some(self.lower_block(arm.block, slots, current_function, item_slot)?);
                }
            }
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::TagMatch {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    fn lower_tag_match_expr(
        &mut self,
        value: ExprId,
        arms: crate::syntax::arena::ArenaRange,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let Type::Tag(type_name) = self.checked_expr_type(value)? else {
            return None;
        };
        if self
            .declarations
            .wire_enums
            .mappings
            .contains_key(&type_name)
        {
            return None;
        }
        let arms = self.program.arena.match_expr_arms(arms).to_vec();
        let mut lowered_arms = FxHashMap::default();
        let mut fallback = None;
        for (index, arm) in arms.iter().enumerate() {
            if arm.guard.is_some() {
                return None;
            }
            match self.pattern_tag_name(arm.pattern)? {
                Some(pattern) => {
                    let value = self.lower_expr(arm.value, slots, current_function, item_slot)?;
                    lowered_arms.entry(pattern).or_insert(value);
                }
                None => {
                    if index + 1 != arms.len() {
                        return None;
                    }
                    fallback =
                        Some(self.lower_expr(arm.value, slots, current_function, item_slot)?);
                }
            }
        }
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::TagMatchExpr {
                value: self.lower_expr(value, slots, current_function, item_slot)?,
                arms: lowered_arms,
                fallback,
                span,
            }
        ))
    }

    fn pattern_str_literal(&mut self, pattern: PatternId) -> Option<Option<Arc<str>>> {
        let lowered = match self.program.arena.pattern(pattern).kind {
            ArenaPatternKind::Wildcard => Some(None),
            // A literal the checker typed as a Path matches a Path subject,
            // which the text-keyed match cannot compare.
            ArenaPatternKind::Literal(expr) => match self.program.arena.expr(expr).kind {
                ArenaExprKind::Str(value) if !self.bodies.path_literals.contains(&expr) => {
                    Some(Some(self.program.arena.string_literal(value).clone()))
                }
                _ => None,
            },
            ArenaPatternKind::Alternation(alts) => {
                let first = self.program.arena.pattern_ids(alts).next()?;
                return self.pattern_str_literal(first);
            }
            _ => None,
        }?;
        self.output.patterns += 1;
        self.output.constructed_patterns += 1;
        Some(lowered)
    }

    /// Like `pattern_str_literal` but expands an alternation `"a" | "b" | …`
    /// into all its literal arms. Returns `Some(None)` for a wildcard (fallback),
    /// `Some(Some(vec))` for one-or-more string literals, `None` if unsupported.
    fn pattern_str_literals(&mut self, pattern: PatternId) -> Option<Option<Vec<Arc<str>>>> {
        match self.program.arena.pattern(pattern).kind {
            ArenaPatternKind::Alternation(alts) => {
                let mut literals = Vec::new();
                for alt in self.program.arena.pattern_ids(alts).collect::<Vec<_>>() {
                    // Each alternative must itself be a string literal.
                    {
                        let literal = self.pattern_str_literal(alt)??;
                        literals.push(literal)
                    }
                }
                if literals.is_empty() {
                    return None;
                }
                Some(Some(literals))
            }
            _ => Some(
                self.pattern_str_literal(pattern)?
                    .map(|literal| vec![literal]),
            ),
        }
    }

    /// A pattern's kind, with a target-typed `.Name` replaced by the
    /// qualified pattern the checker resolved it to. `None` when the checker
    /// published no resolution, so such a pattern is never lowered by name.
    fn qualified_pattern_kind(&self, id: PatternId) -> Option<ArenaPatternKind> {
        let kind = &self.program.arena.pattern(id).kind;
        if !kind.is_inferred_variant() {
            return Some(kind.clone());
        }
        Some(
            self.bodies
                .inferred_variant_patterns
                .get(&id)?
                .qualify(kind),
        )
    }

    fn pattern_tag_name(&mut self, pattern: PatternId) -> Option<Option<Arc<str>>> {
        let lowered = match self.qualified_pattern_kind(pattern)? {
            ArenaPatternKind::Wildcard => Some(None),
            ArenaPatternKind::Constructor { name, arg: None }
                if self.compact_tag_variant_arity(name) == Some(0) =>
            {
                Some(Some(Arc::<str>::from(name.as_str().as_str())))
            }
            _ => None,
        }?;
        self.output.patterns += 1;
        self.output.constructed_patterns += 1;
        Some(lowered)
    }

    fn lower_arm_value_expr(
        &mut self,
        stmt: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let stmt = self.program.arena.core_stmt_id(stmt);
        match self.program.arena.stmt(stmt).kind {
            ArenaStmtKind::Expr(expr) => self.lower_expr(expr, slots, current_function, item_slot),
            ArenaStmtKind::Exit(status) => {
                let span = self.program.arena.stmt(stmt).span;
                self.lower_exit(status, span, slots, current_function, item_slot)
            }
            ArenaStmtKind::TailBareIdent(name) => self.lower_bare_ident_stmt(stmt, name, slots),
            _ => None,
        }
    }

    /// Lowers `exit STATUS` to the expression that ends the script: it
    /// evaluates the status and never yields a value, so it stands wherever
    /// the statement does, as a statement or as the tail of a block.
    fn lower_exit(
        &mut self,
        status: ExprId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::Abort {
                status: self.lower_expr(status, slots, current_function, item_slot)?,
                span,
            }
        ))
    }

    /// The place a `?` reports: itself with its operand, or the operand alone
    /// when it is the `?` of a deferred call.
    fn propagation_span(&self, propagation: ExprId, operand: ExprId, slots: &SlotScope) -> Span {
        if slots.deferred_propagation == Some(propagation) {
            self.program.arena.expr(operand).span
        } else {
            self.program.arena.expr(propagation).span
        }
    }

    /// Cleanup bodies use statement position even for their final expression.
    fn lower_deferred_expr(
        &mut self,
        value: ExprId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        if let ArenaExprKind::ValueBlock(block) = self.program.arena.expr(value).kind {
            let body = self.lower_block(block, slots, current_function, item_slot)?;
            let span = self.program.arena.expr(value).span;
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ValueBlock { body, span }
            ))
        } else {
            let outer = slots.deferred_propagation.replace(value);
            let lowered = self.lower_expr(value, slots, current_function, item_slot);
            slots.deferred_propagation = outer;
            let lowered = lowered?;
            // A deferred `Result[Unit]` fails its action with or without a
            // `?`. Lowering the bare form as the propagation it is gives its
            // failure the action's own place; the evaluator's handling of a
            // deferred `Err` value stays as the fallback.
            let checked = self
                .checked_expr_type(value)
                .or_else(|| self.bodies.expr_types.get(&value).cloned());
            if matches!(checked, Some(Type::Result(..)))
                && !matches!(self.program.arena.expr(value).kind, ArenaExprKind::Try(_))
            {
                let span = self.program.arena.expr(value).span;
                return Some(push_build_row!(
                    self,
                    expr,
                    BuildExprRow::Try {
                        value: lowered,
                        span
                    }
                ));
            }
            Some(lowered)
        }
    }

    /// `tempdir NAME { body }` is the scope that `let root = fs.tempdir()?`,
    /// `defer root.close()?`, and `let NAME = root.host_path()?` open, so the
    /// directory is removed after the body's own cleanup on every exit, by the
    /// same defer machinery. Only a failure to create the directory becomes the
    /// scope's `Err`; the body's tail (or `Unit`) is its `Ok`.
    ///
    /// `tempdir NAME at PATH { body }` is the same scope over a directory the
    /// program names: `PATH` is evaluated once, whatever is there is removed,
    /// the directory is created, and its removal is deferred around the body.
    /// A failure to clear or create it is the scope's `Err`.
    fn lower_tempdir_scope(
        &mut self,
        path: Option<ExprId>,
        block: BlockId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let Some(path) = path else {
            return self.lower_fresh_tempdir_scope(block, span, slots, current_function, item_slot);
        };
        // The path is evaluated where the scope is written, before the
        // directory name exists.
        let path = self.lower_expr(path, slots, current_function, item_slot)?;
        let saved = slots.enter();
        let result = (|| {
            let at_slot = slots.reserve("tempdir.at");
            let at =
                |lowerer: &mut Self| push_build_row!(lowerer, expr, BuildExprRow::Param(at_slot));
            let remove = |lowerer: &mut Self| {
                let path = at(lowerer);
                let missing_ok = push_build_row!(lowerer, expr, BuildExprRow::Bool(true));
                push_build_row!(
                    lowerer,
                    expr,
                    BuildExprRow::FsRemove {
                        path,
                        missing_ok: Some(missing_ok),
                        span,
                    }
                )
            };
            let failed_slot = slots.reserve("tempdir.failed");
            // An arm that hands a failed step's `Err` on as the scope's value.
            let failed_arm = |lowerer: &mut Self| {
                let failed = push_build_row!(
                    lowerer,
                    pattern,
                    BuildPatternRow::Bind { slot: failed_slot }
                );
                let failure = push_build_row!(lowerer, expr, BuildExprRow::Param(failed_slot));
                (failed, None, failure)
            };
            let done = |lowerer: &mut Self| {
                push_build_row!(
                    lowerer,
                    pattern,
                    BuildPatternRow::ResultOk {
                        slot: None,
                        unit_only: false
                    }
                )
            };

            let scope = {
                let inner = slots.enter();
                let scope = (|| {
                    let removal = remove(self);
                    let removal = push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Try {
                            value: removal,
                            span
                        }
                    );
                    let mut body = vec![push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Defer {
                            value: removal,
                            on_error: false,
                        }
                    )];
                    let value = at(self);
                    let path_slot = match self
                        .program
                        .arena
                        .block_params(self.program.arena.block(block).params)
                    {
                        [param] if param.name.as_str() != "_" => {
                            slots.declare_with_type(param.name, Some(Type::Path))
                        }
                        _ => slots.reserve("tempdir.path"),
                    };
                    body.push(push_build_row!(
                        self,
                        stmt,
                        BuildStmtRow::Let {
                            slot: path_slot,
                            value
                        }
                    ));
                    self.lower_tempdir_body(block, body, span, slots, current_function, item_slot)
                })();
                slots.exit(inner);
                scope?
            };

            let created = {
                let path = at(self);
                push_build_row!(
                    self,
                    expr,
                    BuildExprRow::FsMkdir {
                        path,
                        parents: None,
                        span,
                    }
                )
            };
            let entered = {
                let arms = vec![(done(self), None, scope), failed_arm(self)];
                push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MatchExpr {
                        value: created,
                        arms,
                        span,
                    }
                )
            };
            let cleared = remove(self);
            let outcome = {
                let arms = vec![(done(self), None, entered), failed_arm(self)];
                push_build_row!(
                    self,
                    expr,
                    BuildExprRow::MatchExpr {
                        value: cleared,
                        arms,
                        span,
                    }
                )
            };
            let body = vec![
                push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let {
                        slot: at_slot,
                        value: path
                    }
                ),
                push_build_row!(self, stmt, BuildStmtRow::Value { value: outcome }),
            ];
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ValueBlock { body, span }
            ))
        })();
        slots.exit(saved);
        result
    }

    /// A managed `with` scope, built from rows that exist: each value is
    /// bound and its release deferred, so every way out of the body releases
    /// what was opened, in reverse. A body that reaches its end is followed
    /// by the same releases under a capture, whose `Err` is the scope's: the
    /// first release that fails, with any later one reported as a cleanup
    /// failure. That capture sets a flag once its own releases are
    /// registered, and the outer ones do nothing from then on, so a release
    /// that failed is not attempted a second time.
    fn lower_resource_scope(
        &mut self,
        scope: ExprId,
        bindings: crate::syntax::arena::ArenaRange,
        block: BlockId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let bindings = self.program.arena.with_bindings(bindings).to_vec();
        // The checker publishes one kind per binding, or nothing for a scope
        // it rejected.
        let kinds = self
            .bodies
            .resource_scopes
            .get(&scope)
            .filter(|kinds| kinds.len() == bindings.len())?
            .clone();
        let saved = slots.enter();
        let result = (|| {
            let closing_slot = slots.reserve("with.closing");
            let release = |lowerer: &mut Self,
                           slot: usize,
                           kind: crate::modules::ManagedResource,
                           unless_closing: bool| {
                let value = push_build_row!(lowerer, expr, BuildExprRow::Param(slot));
                let call = push_build_row!(
                    lowerer,
                    expr,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op: kind.release_op(),
                        args: vec![Some(value)],
                        span,
                    }
                );
                let mut release =
                    push_build_row!(lowerer, expr, BuildExprRow::Try { value: call, span });
                if unless_closing {
                    let closing = push_build_row!(lowerer, expr, BuildExprRow::Param(closing_slot));
                    let nothing = push_build_row!(lowerer, expr, BuildExprRow::Unit);
                    release = push_build_row!(
                        lowerer,
                        expr,
                        BuildExprRow::IfExpr {
                            branches: vec![(closing, nothing)],
                            else_value: release,
                            span,
                        }
                    );
                }
                push_build_row!(
                    lowerer,
                    stmt,
                    BuildStmtRow::Defer {
                        value: release,
                        on_error: false,
                    }
                )
            };
            let mut body = Vec::new();
            let open = push_build_row!(self, expr, BuildExprRow::Bool(false));
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Let {
                    slot: closing_slot,
                    value: open
                }
            ));
            let mut held = Vec::with_capacity(bindings.len());
            for (binding, kind) in bindings.iter().zip(kinds) {
                let ty = self.lower_binding_checked_type(None, binding.initializer);
                let value =
                    self.lower_expr(binding.initializer, slots, current_function, item_slot)?;
                let slot = if binding.name.as_str() == "_" {
                    slots.reserve("with.resource")
                } else {
                    slots.declare_with_type(binding.name, ty)
                };
                body.push(push_build_row!(
                    self,
                    stmt,
                    BuildStmtRow::Let { slot, value }
                ));
                body.push(release(self, slot, kind, true));
                held.push((slot, kind));
            }
            // The body is a block of its own: its defers run and its handles
            // are cleaned up before anything is released.
            let finished = self.lower_tempdir_body(
                block,
                Vec::new(),
                span,
                slots,
                current_function,
                item_slot,
            )?;
            let finished_slot = slots.reserve("with.finished");
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Let {
                    slot: finished_slot,
                    value: finished
                }
            ));
            let mut closing = Vec::with_capacity(held.len() + 2);
            for (slot, kind) in held {
                closing.push(release(self, slot, kind, false));
            }
            let handed_over = push_build_row!(self, expr, BuildExprRow::Bool(true));
            closing.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Assign {
                    slot: closing_slot,
                    op: AssignOp::Set,
                    value: handed_over,
                    check: None,
                    span,
                }
            ));
            let finished = push_build_row!(self, expr, BuildExprRow::Param(finished_slot));
            let value = push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: finished,
                    span
                }
            );
            closing.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
            let outcome = push_build_row!(
                self,
                expr,
                BuildExprRow::Capture {
                    body: closing,
                    span
                }
            );
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Value { value: outcome }
            ));
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ValueBlock { body, span }
            ))
        })();
        slots.exit(saved);
        result
    }

    /// The statements of a `tempdir` body after `prefix`, which holds the
    /// deferred removal and the binding of the directory name, as the value
    /// block whose value is `Ok` of the body's tail.
    fn lower_tempdir_body(
        &mut self,
        block: BlockId,
        prefix: Vec<BuildStmtId>,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let mut body = prefix;
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let mut tail = None;
        if let Some((&last, prefix)) = statements.split_last() {
            for &stmt in prefix {
                body.push(self.lower_stmt_with_blocker_guard(
                    stmt,
                    slots,
                    current_function,
                    item_slot,
                )?);
            }
            if self.bodies.statement_positions.get(&last)
                != Some(&crate::sema::check::StatementPosition::Statement)
                && let Some(value) =
                    self.lower_tail_stmt_as_expr(last, slots, current_function, item_slot)
            {
                tail = Some(value);
            } else {
                body.push(self.lower_stmt_with_blocker_guard(
                    last,
                    slots,
                    current_function,
                    item_slot,
                )?);
            }
        }
        let tail = tail.unwrap_or_else(|| push_build_row!(self, expr, BuildExprRow::Unit));
        let value = push_build_row!(self, expr, BuildExprRow::Ok(tail));
        body.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::ValueBlock { body, span }
        ))
    }

    fn lower_fresh_tempdir_scope(
        &mut self,
        block: BlockId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let created = push_build_row!(self, expr, BuildExprRow::FsTempDir { span });
        let failed_slot = slots.reserve("tempdir.failed");
        let failed = push_build_row!(self, pattern, BuildPatternRow::Bind { slot: failed_slot });
        let failure = push_build_row!(self, expr, BuildExprRow::Param(failed_slot));
        let saved = slots.enter();
        let result = (|| {
            let root_slot = slots.reserve("tempdir.root");
            let opened = push_build_row!(
                self,
                pattern,
                BuildPatternRow::ResultOk {
                    slot: Some(root_slot),
                    unit_only: false
                }
            );
            let root_method = |lowerer: &mut Self, op: RuntimeOp| {
                let receiver = push_build_row!(lowerer, expr, BuildExprRow::Param(root_slot));
                let call = push_build_row!(
                    lowerer,
                    expr,
                    BuildExprRow::ModuleCall {
                        cli_plan: None,
                        op,
                        args: vec![Some(receiver)],
                        span,
                    }
                );
                push_build_row!(lowerer, expr, BuildExprRow::Try { value: call, span })
            };
            let mut body = Vec::new();
            let close = root_method(self, RuntimeOp::FsCloseRoot);
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Defer {
                    value: close,
                    on_error: false,
                }
            ));
            let path = root_method(self, RuntimeOp::FsRootPath);
            let path_slot = match self
                .program
                .arena
                .block_params(self.program.arena.block(block).params)
            {
                [param] if param.name.as_str() != "_" => {
                    slots.declare_with_type(param.name, Some(Type::Path))
                }
                _ => slots.reserve("tempdir.path"),
            };
            body.push(push_build_row!(
                self,
                stmt,
                BuildStmtRow::Let {
                    slot: path_slot,
                    value: path
                }
            ));
            let scope =
                self.lower_tempdir_body(block, body, span, slots, current_function, item_slot)?;
            Some((opened, scope))
        })();
        slots.exit(saved);
        let (opened, scope) = result?;
        Some(push_build_row!(
            self,
            expr,
            BuildExprRow::MatchExpr {
                value: created,
                arms: vec![(opened, None, scope), (failed, None, failure)],
                span,
            }
        ))
    }

    /// Keep branch statements and their tail in one lexical scope so the
    /// selected value is evaluated before that scope runs cleanup.
    fn lower_block_value_expr(
        &mut self,
        block: BlockId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let statements = self.program.arena.block(block).statements;
        let statements = self.program.arena.stmt_ids(statements).collect::<Vec<_>>();
        let saved = slots.enter();
        let result = (|| {
            let mut body = Vec::with_capacity(statements.len());
            if let Some((&tail, prefix)) = statements.split_last() {
                for &stmt in prefix {
                    body.push(self.lower_stmt_with_blocker_guard(
                        stmt,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
                if self.bodies.statement_positions.get(&tail)
                    != Some(&crate::sema::check::StatementPosition::Statement)
                    && let Some(value) =
                        self.lower_tail_stmt_as_expr(tail, slots, current_function, item_slot)
                {
                    body.push(push_build_row!(self, stmt, BuildStmtRow::Value { value }));
                } else {
                    body.push(self.lower_stmt_with_blocker_guard(
                        tail,
                        slots,
                        current_function,
                        item_slot,
                    )?);
                }
            }
            let span = self
                .program
                .arena
                .span(self.program.arena.block(block).span);
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::ValueBlock { body, span }
            ))
        })();
        slots.exit(saved);
        result
    }

    /// Lower a single value-producing tail statement to an expression: a bare
    /// expression, a captured run, a tail-bare-ident, or a value-producing `if`/`match` whose
    /// branch blocks retain ordinary lexical statements and a checked tail.
    fn lower_tail_stmt_as_expr(
        &mut self,
        stmt: StmtId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let stmt = self.program.arena.core_stmt_id(stmt);
        let span = self.program.arena.stmt(stmt).span;
        match self.program.arena.stmt(stmt).kind {
            ArenaStmtKind::Expr(expr) => self.lower_expr(expr, slots, current_function, item_slot),
            ArenaStmtKind::Exit(status) => {
                self.lower_exit(status, span, slots, current_function, item_slot)
            }
            ArenaStmtKind::TailBareIdent(name) => self.lower_bare_ident_stmt(stmt, name, slots),
            ArenaStmtKind::Command(command)
                if self.bodies.statement_positions.get(&stmt)
                    == Some(&crate::sema::check::StatementPosition::Value) =>
            {
                let command = self.program.arena.command_stmt(command);
                let ArenaCommand::Run(run) = command.command else {
                    return None;
                };
                lowered_arena_run_capture_type(&self.program.arena, run)?;
                self.lower_run_binding_value(run, slots, current_function, item_slot)
            }
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                // A value-producing `if` needs an `else`.
                let else_block = else_block?;
                let arena_branches = self.program.arena.if_branches(branches).to_vec();
                let has_pattern = arena_branches.iter().any(|branch| {
                    matches!(
                        self.program.arena.expr(branch.condition).kind,
                        ArenaExprKind::PatternCondition { .. }
                    )
                });
                let mut lowered = Vec::with_capacity(arena_branches.len());
                for branch in arena_branches {
                    let saved = slots.enter();
                    let (condition, captures) = self.lower_pattern_condition_parts(
                        branch.condition,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    let value = self.lower_block_value_expr(
                        branch.block,
                        slots,
                        current_function,
                        item_slot,
                    )?;
                    slots.exit(saved);
                    lowered.push((condition, value, captures));
                }
                let else_value =
                    self.lower_block_value_expr(else_block, slots, current_function, item_slot)?;
                if has_pattern {
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::PatternIf {
                            branches: lowered,
                            else_value,
                            span
                        }
                    ))
                } else {
                    let branches = lowered
                        .into_iter()
                        .map(|(condition, value, _)| (condition, value))
                        .collect();
                    Some(push_build_row!(
                        self,
                        expr,
                        BuildExprRow::IfExpr {
                            branches,
                            else_value,
                            span
                        }
                    ))
                }
            }
            ArenaStmtKind::Match { value, arms } => {
                self.lower_match_stmt_as_expr(value, arms, span, slots, current_function, item_slot)
            }
            _ => None,
        }
    }

    fn pattern_capture_names(&self, pattern: PatternId, names: &mut FxHashSet<Name>) {
        match self.program.arena.pattern(pattern).kind {
            ArenaPatternKind::Alias { pattern, name, .. } => {
                names.insert(name);
                self.pattern_capture_names(pattern, names);
            }
            ArenaPatternKind::Group(pattern) => self.pattern_capture_names(pattern, names),
            ArenaPatternKind::Binding(name) if self.compact_tag_variant_arity(name) != Some(0) => {
                names.insert(name);
            }
            ArenaPatternKind::Type {
                binding: Some(name),
                ..
            } => {
                names.insert(name);
            }
            ArenaPatternKind::List { elements, rest } => {
                for child in self.program.arena.pattern_ids(elements).chain(rest) {
                    self.pattern_capture_names(child, names);
                }
            }
            ArenaPatternKind::Record { fields, .. }
            | ArenaPatternKind::ErrorVariant { fields, .. } => {
                for field in self.program.arena.pattern_fields(fields) {
                    self.pattern_capture_names(field.pattern, names);
                }
            }
            ArenaPatternKind::Constructor { arg: Some(arg), .. } => {
                self.pattern_capture_names(arg, names)
            }
            ArenaPatternKind::Tuple(items) | ArenaPatternKind::Text(items) => {
                for child in self.program.arena.pattern_ids(items) {
                    self.pattern_capture_names(child, names);
                }
            }
            ArenaPatternKind::TextHole {
                binding: Some(name),
                ..
            } => {
                names.insert(name);
            }
            ArenaPatternKind::Alternation(items) => {
                if let Some(child) = self.program.arena.pattern_ids(items).next() {
                    self.pattern_capture_names(child, names);
                }
            }
            _ => {}
        }
    }

    fn lower_pattern(
        &mut self,
        id: PatternId,
        slots: &mut SlotScope,
        ok_binding_ty: Option<&Type>,
        err_binding_ty: Option<&Type>,
    ) -> Option<(BuildPatternId, Vec<(Name, usize)>)> {
        self.output.patterns += 1;
        let kind = self.qualified_pattern_kind(id)?;
        let lowered = match &kind {
            ArenaPatternKind::Group(pattern) => {
                self.lower_pattern(*pattern, slots, ok_binding_ty, err_binding_ty)
            }
            ArenaPatternKind::Alias { pattern, name, .. } => {
                let pattern = *pattern;
                let name = *name;
                let (pattern, mut cleanup) =
                    self.lower_pattern(pattern, slots, ok_binding_ty, err_binding_ty)?;
                let slot = slots.declare_pattern_binding(name);
                cleanup.push((name, slot));
                Some((
                    push_build_row!(self, pattern, BuildPatternRow::Alias { pattern, slot }),
                    cleanup,
                ))
            }
            ArenaPatternKind::Alternation(children) => {
                let children: Vec<_> = self.program.arena.pattern_ids(*children).collect();
                let mut names = FxHashSet::default();
                self.pattern_capture_names(id, &mut names);
                let mut names: Vec<_> = names.into_iter().collect();
                names.sort();
                let saved = slots.pattern_slots.clone();
                let mut shared = saved.clone().unwrap_or_default();
                let mut cleanup = Vec::new();
                for name in names {
                    if let std::collections::hash_map::Entry::Vacant(entry) = shared.entry(name) {
                        let slot = slots.declare_pattern_capture(name);
                        entry.insert(slot);
                        cleanup.push((name, slot));
                    }
                }
                slots.pattern_slots = Some(shared);
                let patterns: Option<Vec<_>> = children
                    .into_iter()
                    .map(|child| {
                        self.lower_pattern(child, slots, ok_binding_ty, err_binding_ty)
                            .map(|(pattern, _)| pattern)
                    })
                    .collect();
                slots.pattern_slots = saved;
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Alternation {
                            patterns: patterns?
                        }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::TestName { name, ty } => {
                if name == "Ok" || name == "Err" {
                    let row = if name == "Ok" {
                        BuildPatternRow::ResultOk {
                            slot: None,
                            unit_only: true,
                        }
                    } else {
                        BuildPatternRow::ResultErr {
                            slot: None,
                            unit_only: true,
                        }
                    };
                    return Some((push_build_row!(self, pattern, row), Vec::new()));
                }
                let text = name.as_str();
                if let Some((family, variant)) = text.rsplit_once('.') {
                    let family = Name::intern(family);
                    let variant = Name::intern(variant);
                    if let Some((family, info)) = self.compact_pattern_error_family(family)
                        && info.variants.contains_key(&variant)
                    {
                        return Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::ErrorTest {
                                    family,
                                    variant,
                                    fields: Vec::new()
                                }
                            ),
                            Vec::new(),
                        ));
                    }
                }
                if self.compact_tag_variant_arity(*name) == Some(0) {
                    Some((
                        push_build_row!(
                            self,
                            pattern,
                            BuildPatternRow::Tag {
                                type_name: self.compact_tag_type_name(*name)?,
                                name: compact_pattern_tag_name(*name),
                                slots: Default::default()
                            }
                        ),
                        Vec::new(),
                    ))
                } else {
                    let ty = if let Some(facet) = self.compact_qualified_pattern_facet(*name) {
                        Type::ErrorFacet(facet)
                    } else {
                        compact_pattern_test_type(
                            &self.program.arena,
                            *name,
                            *ty,
                            self.declarations,
                        )
                    };
                    if let Type::ErrorFacet(facet) = ty {
                        Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::Facet {
                                    facet,
                                    result_wrapped: false
                                }
                            ),
                            Vec::new(),
                        ))
                    } else if let Type::Tag(type_name) = ty {
                        let namespace = name
                            .as_str()
                            .rsplit_once('.')
                            .and_then(|(namespace, _)| {
                                self.compact_imported_module_owner(Name::intern(namespace))
                            })
                            .or(self.current_namespace);
                        let variants = if let Some(namespace) = namespace {
                            self.declarations
                                .qualified_tag_variants
                                .iter()
                                .filter_map(|(key, info)| {
                                    (key.namespace == namespace && info.type_name == type_name)
                                        .then_some(key.member)
                                })
                                .collect()
                        } else {
                            self.declarations
                                .tag_variants_by_name
                                .iter()
                                .filter_map(|(name, info)| {
                                    (info.type_name == type_name).then_some(*name)
                                })
                                .collect()
                        };
                        Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::TagType {
                                    type_name,
                                    variants
                                }
                            ),
                            Vec::new(),
                        ))
                    } else {
                        Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::Type { ty, slot: None }
                            ),
                            Vec::new(),
                        ))
                    }
                }
            }
            ArenaPatternKind::List { elements, rest } => {
                let children: Vec<_> = self.program.arena.pattern_ids(*elements).collect();
                let rest_id = *rest;
                let mut elements = Vec::new();
                let mut cleanup = Vec::new();
                for child in children {
                    let (pattern, bindings) = self.lower_pattern(child, slots, None, None)?;
                    elements.push(pattern);
                    cleanup.extend(bindings);
                }
                let rest = if let Some(child) = rest_id {
                    let (pattern, bindings) = self.lower_pattern(child, slots, None, None)?;
                    cleanup.extend(bindings);
                    Some(pattern)
                } else {
                    None
                };
                Some((
                    push_build_row!(self, pattern, BuildPatternRow::List { elements, rest }),
                    cleanup,
                ))
            }
            ArenaPatternKind::Record { fields, .. } => {
                let mut lowered = Vec::new();
                let mut cleanup = Vec::new();
                for field in self.program.arena.pattern_fields(*fields).to_vec() {
                    let (pattern, bindings) =
                        self.lower_pattern(field.pattern, slots, None, None)?;
                    cleanup.extend(bindings);
                    lowered.push((field.name, pattern));
                }
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::RecordTest { fields: lowered }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::Wildcard => Some((
                push_build_row!(self, pattern, BuildPatternRow::Wildcard),
                Vec::new(),
            )),
            // The segments and hole kinds are the checker's compilation of
            // the pattern; the syntax supplies only the name each hole binds.
            ArenaPatternKind::Text(parts) => {
                let compiled = self.bodies.text_patterns.get(&id)?.clone();
                let mut holes = Vec::with_capacity(compiled.holes.len());
                let mut cleanup = Vec::new();
                for part in self.program.arena.pattern_ids(*parts).collect::<Vec<_>>() {
                    let ArenaPatternKind::TextHole { binding, .. } =
                        self.program.arena.pattern(part).kind
                    else {
                        continue;
                    };
                    let row = match binding {
                        Some(name) if slots.can_bind_pattern(name) => {
                            let slot = slots.declare_pattern_binding(name);
                            cleanup.push((name, slot));
                            BuildPatternRow::Bind { slot }
                        }
                        Some(_) => return None,
                        None => BuildPatternRow::Wildcard,
                    };
                    holes.push(push_build_row!(self, pattern, row));
                }
                if holes.len() != compiled.holes.len() {
                    return None;
                }
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Text {
                            holes,
                            kinds: compiled.holes,
                            segments: compiled.segments,
                        }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::Literal(expr) => self
                .lower_pattern_literal(*expr)
                .map(|pattern| (pattern, Vec::new())),
            ArenaPatternKind::Binding(name) if self.compact_tag_variant_arity(*name) == Some(0) => {
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Tag {
                            type_name: self.compact_tag_type_name(*name)?,
                            name: compact_pattern_tag_name(*name),
                            slots: Default::default(),
                        }
                    ),
                    Vec::new(),
                ))
            }
            ArenaPatternKind::Binding(name) if slots.can_bind_pattern(*name) => {
                let slot = slots.declare_pattern_binding(*name);
                Some((
                    push_build_row!(self, pattern, BuildPatternRow::Bind { slot }),
                    vec![(*name, slot)],
                ))
            }
            ArenaPatternKind::Type {
                binding: Some(name),
                ty,
            } if slots.can_bind_pattern(*name) => {
                let lowered_ty = compact_runtime_type_in_namespace(
                    &self.program.arena,
                    *ty,
                    self.declarations,
                    self.current_namespace,
                );
                let slot = slots.declare_pattern_binding(*name);
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Type {
                            ty: lowered_ty,
                            slot: Some(slot),
                        }
                    ),
                    vec![(*name, slot)],
                ))
            }
            ArenaPatternKind::Type { binding: None, ty } => {
                let lowered_ty = compact_runtime_type_in_namespace(
                    &self.program.arena,
                    *ty,
                    self.declarations,
                    self.current_namespace,
                );
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Type {
                            ty: lowered_ty,
                            slot: None,
                        }
                    ),
                    Vec::new(),
                ))
            }
            ArenaPatternKind::ErrorVariant {
                family,
                variant,
                fields,
            } => {
                let qualified = Name::intern(format!("{family}.{variant}"));
                if fields.len == 0 && self.compact_tag_variant_arity(qualified) == Some(0) {
                    return Some((
                        push_build_row!(
                            self,
                            pattern,
                            BuildPatternRow::Tag {
                                type_name: self.compact_tag_type_name(qualified)?,
                                name: *variant,
                                slots: Default::default()
                            }
                        ),
                        Vec::new(),
                    ));
                }
                let family = self
                    .compact_pattern_error_family(*family)
                    .map_or(*family, |(family, _)| family);
                let mut lowered = Vec::new();
                let mut cleanup = Vec::new();
                for field in self.program.arena.pattern_fields(*fields).to_vec() {
                    let (pattern, bindings) =
                        self.lower_pattern(field.pattern, slots, None, None)?;
                    cleanup.extend(bindings);
                    lowered.push((field.name, pattern));
                }
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::ErrorTest {
                            family,
                            variant: *variant,
                            fields: lowered
                        }
                    ),
                    cleanup,
                ))
            }
            ArenaPatternKind::Facet(facet) => Some((
                push_build_row!(
                    self,
                    pattern,
                    BuildPatternRow::Facet {
                        facet: *facet,
                        result_wrapped: false,
                    }
                ),
                Vec::new(),
            )),
            ArenaPatternKind::Constructor { name, arg } => {
                if let Some(arg) = arg
                    && !matches!(
                        self.program.arena.pattern(*arg).kind,
                        ArenaPatternKind::Wildcard | ArenaPatternKind::Binding(_)
                    )
                {
                    if name == "Ok" || name == "Err" {
                        let (inner, cleanup) = self.lower_pattern(*arg, slots, None, None)?;
                        return Some((
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::ResultTest {
                                    ok: name == "Ok",
                                    inner
                                }
                            ),
                            cleanup,
                        ));
                    }
                    let patterns = match self.program.arena.pattern(*arg).kind {
                        ArenaPatternKind::Tuple(fields) => {
                            self.program.arena.pattern_ids(fields).collect::<Vec<_>>()
                        }
                        _ => vec![*arg],
                    };
                    let mut fields = Vec::new();
                    let mut cleanup = Vec::new();
                    for pattern in patterns {
                        let (field, bindings) = self.lower_pattern(pattern, slots, None, None)?;
                        fields.push(field);
                        cleanup.extend(bindings);
                    }
                    return Some((
                        push_build_row!(
                            self,
                            pattern,
                            BuildPatternRow::TagTest {
                                type_name: self.compact_tag_type_name(*name)?,
                                name: compact_pattern_tag_name(*name),
                                fields
                            }
                        ),
                        cleanup,
                    ));
                }
                if name == "Err"
                    && let Some(arg) = arg
                    && let ArenaPatternKind::ErrorVariant {
                        family,
                        variant,
                        fields,
                    } = self.qualified_pattern_kind(*arg)?
                {
                    return self
                        .lower_error_variant_pattern(family, variant, fields, true, slots)
                        .inspect(|_| {
                            self.output.constructed_patterns += 1;
                        });
                }
                if name == "Err"
                    && let Some(arg) = arg
                    && let ArenaPatternKind::Facet(facet) = self.program.arena.pattern(*arg).kind
                {
                    self.output.constructed_patterns += 1;
                    return Some((
                        push_build_row!(
                            self,
                            pattern,
                            BuildPatternRow::Facet {
                                facet,
                                result_wrapped: true,
                            }
                        ),
                        Vec::new(),
                    ));
                }
                if name == "Ok" || name == "Err" {
                    let mut cleanup = Vec::new();
                    let binding_ty = if name == "Ok" {
                        ok_binding_ty
                    } else {
                        err_binding_ty
                    };
                    let (slot, unit_only) =
                        self.lower_result_pattern_slot(*arg, slots, &mut cleanup, binding_ty)?;
                    return Some((
                        if name == "Ok" {
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::ResultOk { slot, unit_only }
                            )
                        } else {
                            push_build_row!(
                                self,
                                pattern,
                                BuildPatternRow::ResultErr { slot, unit_only }
                            )
                        },
                        cleanup,
                    ));
                }
                let arity = self.compact_tag_variant_arity(*name)?;
                let mut cleanup = Vec::new();
                let field_slots =
                    match self.lower_tag_pattern_slots(*arg, arity, slots, &mut cleanup) {
                        Some(field_slots) => field_slots,
                        None => {
                            cleanup_lowered_pattern_slots(slots, cleanup);
                            return None;
                        }
                    };
                Some((
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Tag {
                            type_name: self.compact_tag_type_name(*name)?,
                            name: compact_pattern_tag_name(*name),
                            slots: field_slots,
                        }
                    ),
                    cleanup,
                ))
            }
            _ => None,
        }?;
        self.output.constructed_patterns += 1;
        Some(lowered)
    }

    fn lower_error_variant_pattern(
        &self,
        family: Name,
        variant: Name,
        fields: crate::syntax::arena::ArenaRange,
        result_wrapped: bool,
        slots: &mut SlotScope,
    ) -> Option<(BuildPatternId, Vec<(Name, usize)>)> {
        let mut cleanup = Vec::new();
        let mut lowered = LoweredErrorPatternFields::new();
        for field in self.program.arena.pattern_fields(fields) {
            let slot = self.lower_error_pattern_field(field.pattern, slots, &mut cleanup)?;
            lowered.push((field.name, slot));
        }
        Some((
            push_build_row!(
                self,
                pattern,
                BuildPatternRow::ErrorVariant {
                    family,
                    variant,
                    fields: Box::new(lowered),
                    result_wrapped,
                }
            ),
            cleanup,
        ))
    }

    fn lower_error_pattern_field(
        &self,
        id: PatternId,
        slots: &mut SlotScope,
        cleanup: &mut Vec<(Name, usize)>,
    ) -> Option<Option<usize>> {
        match self.program.arena.pattern(id).kind {
            ArenaPatternKind::Wildcard => Some(None),
            ArenaPatternKind::Binding(name) if slots.can_bind_pattern(name) => {
                let slot = slots.declare_pattern_binding(name);
                cleanup.push((name, slot));
                Some(Some(slot))
            }
            _ => None,
        }
    }

    fn lower_pattern_literal(&self, id: ExprId) -> Option<BuildPatternId> {
        match self.program.arena.expr(id).kind {
            ArenaExprKind::Unary {
                op: UnaryOp::Neg,
                expr,
            } => {
                let value = match self.program.arena.expr(expr).kind {
                    ArenaExprKind::Int(value) => LoweredValue::Int(
                        self.program
                            .arena
                            .int_literal(value)
                            .value()?
                            .checked_neg()?,
                    ),
                    ArenaExprKind::Float(value) => {
                        LoweredValue::Float(crate::runtime::value::FloatValue::new(
                            -self.program.arena.float_literal(value).value()?,
                        ))
                    }
                    _ => return None,
                };
                Some(push_build_row!(
                    self,
                    pattern,
                    BuildPatternRow::Literal(value)
                ))
            }
            ArenaExprKind::Float(value) => self
                .program
                .arena
                .float_literal(value)
                .value()
                .map(crate::runtime::value::FloatValue::new)
                .map(|value| {
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Literal(LoweredValue::Float(value))
                    )
                }),
            ArenaExprKind::Bytes(value) => Some(push_build_row!(
                self,
                pattern,
                BuildPatternRow::Literal(LoweredValue::Bytes(
                    self.program.arena.bytes_literal(value).clone()
                ))
            )),
            ArenaExprKind::Null => Some(push_build_row!(
                self,
                pattern,
                BuildPatternRow::Literal(LoweredValue::Null)
            )),
            ArenaExprKind::Int(value) => {
                self.program.arena.int_literal(value).value().map(|value| {
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Literal(LoweredValue::Int(value))
                    )
                })
            }
            ArenaExprKind::Bool(value) => Some(push_build_row!(
                self,
                pattern,
                BuildPatternRow::Literal(LoweredValue::Bool(value))
            )),
            ArenaExprKind::Duration(value) => self
                .program
                .arena
                .duration_literal(value)
                .millis()
                .map(|millis| {
                    push_build_row!(
                        self,
                        pattern,
                        BuildPatternRow::Literal(LoweredValue::Duration(DurationValue { millis }))
                    )
                }),
            ArenaExprKind::Str(value) if self.bodies.path_literals.contains(&id) => {
                let path = PathValue::from_text(self.program.arena.string_literal(value)).ok()?;
                Some(push_build_row!(
                    self,
                    pattern,
                    BuildPatternRow::Literal(LoweredValue::Path(path))
                ))
            }
            ArenaExprKind::Str(value) => Some(push_build_row!(
                self,
                pattern,
                BuildPatternRow::Literal(LoweredValue::Str(
                    self.program.arena.string_literal(value).clone(),
                ))
            )),
            _ => None,
        }
    }

    fn lower_result_pattern_slot(
        &self,
        arg: Option<PatternId>,
        slots: &mut SlotScope,
        cleanup: &mut Vec<(Name, usize)>,
        binding_type: Option<&Type>,
    ) -> Option<(Option<usize>, bool)> {
        let Some(pattern) = arg else {
            return Some((None, true));
        };
        match self.program.arena.pattern(pattern).kind {
            ArenaPatternKind::Wildcard => Some((None, false)),
            ArenaPatternKind::Binding(name) if slots.can_bind_pattern(name) => {
                let slot = slots.declare_pattern_binding(name);
                if let Some(ty) = binding_type {
                    slots.types.insert(name, ty.clone());
                }
                cleanup.push((name, slot));
                Some((Some(slot), false))
            }
            _ => None,
        }
    }

    fn lower_tag_pattern_slots(
        &self,
        arg: Option<PatternId>,
        arity: usize,
        slots: &mut SlotScope,
        cleanup: &mut Vec<(Name, usize)>,
    ) -> Option<BuildPatternIdSlots> {
        match (arity, arg) {
            (0, None) => Some(Default::default()),
            (1, Some(pattern)) => {
                let mut field_slots = BuildPatternIdSlots::new();
                field_slots.push(self.lower_tag_pattern_field(pattern, slots, cleanup)?);
                Some(field_slots)
            }
            (_, Some(pattern)) => {
                let ArenaPatternKind::Tuple(fields) = self.program.arena.pattern(pattern).kind
                else {
                    return None;
                };
                let fields = self.program.arena.pattern_ids(fields).collect::<Vec<_>>();
                if fields.len() != arity {
                    return None;
                }
                let mut field_slots = BuildPatternIdSlots::with_capacity(fields.len());
                for field in fields {
                    field_slots.push(self.lower_tag_pattern_field(field, slots, cleanup)?);
                }
                Some(field_slots)
            }
            _ => None,
        }
    }

    fn lower_tag_pattern_field(
        &self,
        id: PatternId,
        slots: &mut SlotScope,
        cleanup: &mut Vec<(Name, usize)>,
    ) -> Option<Option<usize>> {
        match self.program.arena.pattern(id).kind {
            ArenaPatternKind::Wildcard => Some(None),
            ArenaPatternKind::Binding(name) => {
                if !slots.can_bind_pattern(name) {
                    return None;
                }
                let slot = slots.declare_pattern_binding(name);
                cleanup.push((name, slot));
                Some(Some(slot))
            }
            _ => None,
        }
    }

    fn lower_comp_qualifiers(
        &mut self,
        range: crate::syntax::arena::ArenaRange,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<super::LoweredCompQualifiers> {
        let mut qualifiers = Vec::new();
        for qualifier in self.program.arena.comp_qualifiers(range).to_vec() {
            match qualifier {
                crate::syntax::arena::ArenaCompQualifier::For { target, iter, span } => {
                    let item_ty = self.loop_item_checked_type(iter);
                    let iter =
                        self.lower_direct_iterable(iter, slots, current_function, item_slot)?;
                    slots.enter();
                    let target =
                        Box::new(self.lower_comp_target_typed(target, slots, item_ty.as_ref())?);
                    qualifiers.push(super::LoweredCompQualifier::For { target, iter, span });
                }
                crate::syntax::arena::ArenaCompQualifier::If { condition, span } => {
                    let condition =
                        self.lower_expr(condition, slots, current_function, item_slot)?;
                    qualifiers.push(super::LoweredCompQualifier::If { condition, span });
                }
            }
        }
        if !matches!(
            qualifiers.first(),
            Some(super::LoweredCompQualifier::For { .. })
        ) {
            return None;
        }
        Some(super::LoweredCompQualifiers(qualifiers))
    }

    fn lower_comp_target_typed(
        &self,
        id: BindingTargetId,
        slots: &mut SlotScope,
        ty: Option<&Type>,
    ) -> Option<LoweredCompTarget> {
        match self.program.arena.binding_target(id).kind {
            ArenaBindingTargetKind::Name(name) => {
                if is_discard_name(name) {
                    return Some(LoweredCompTarget::Discard);
                }
                if slots.is_declared_here(name) {
                    return None;
                }
                Some(LoweredCompTarget::Slot(
                    slots.declare_with_type(name, ty.cloned()),
                ))
            }
            ArenaBindingTargetKind::Record { fields, .. } => {
                let mut lowered = LoweredCompFields::new();
                for field in self.program.arena.destructure_fields(fields) {
                    let field_ty = match ty {
                        Some(Type::Record(fields)) => fields.get(&field.name),
                        _ => None,
                    };
                    let target = self.lower_comp_target_typed(field.target, slots, field_ty)?;
                    lowered.push((
                        field.name,
                        Box::new(target),
                        self.program.arena.span(field.span),
                    ));
                }
                Some(LoweredCompTarget::Record { fields: lowered })
            }
        }
    }

    fn assign_target_checked_type(
        &self,
        id: crate::syntax::arena::AssignTargetId,
        slots: &SlotScope,
    ) -> Option<Type> {
        match self.program.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(name) => slots.binding_type(name).cloned(),
            ArenaAssignTargetKind::Field { base, name } => {
                match self.assign_target_checked_type(base, slots)? {
                    Type::Record(fields) => fields.get(&name).cloned(),
                    _ => None,
                }
            }
            ArenaAssignTargetKind::Index { base, .. } => {
                match self
                    .assign_target_checked_type(base, slots)?
                    .into_unvalidated()
                {
                    Type::List(item) | Type::Map(_, item) => Some(*item),
                    _ => None,
                }
            }
            ArenaAssignTargetKind::Env(_) => None,
        }
    }

    fn lower_assign_path(
        &mut self,
        target: crate::syntax::arena::AssignTargetId,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<Vec<LoweredAssignStep>> {
        match self.program.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(_) => Some(Vec::new()),
            ArenaAssignTargetKind::Env(_) => None,
            ArenaAssignTargetKind::Field { base, name } => {
                let mut path = self.lower_assign_path(base, slots, current_function, item_slot)?;
                path.push(LoweredAssignStep::Field(name));
                Some(path)
            }
            ArenaAssignTargetKind::Index { base, index } => {
                let mut path = self.lower_assign_path(base, slots, current_function, item_slot)?;
                let lowered = self.lower_expr(index, slots, current_function, item_slot)?;
                let lowered = if matches!(self.assign_target_checked_type(base, slots), Some(Type::Map(key, _)) if *key == Type::UInt)
                {
                    self.require_uint_key(lowered, self.program.arena.expr(index).span)
                } else {
                    lowered
                };
                path.push(LoweredAssignStep::Index(lowered));
                Some(path)
            }
        }
    }

    /// `e"NAME" = value` runs as an `EnvSet` call in statement position.
    fn lower_env_assignment(
        &mut self,
        id: StmtId,
        name: Name,
        value: ArenaExprOrRun,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildStmtId> {
        let span = self.program.arena.stmt(id).span;
        let value = match value {
            ArenaExprOrRun::Expr(expr) => {
                self.lower_expr(expr, slots, current_function, item_slot)?
            }
            ArenaExprOrRun::Run(run) => {
                self.lower_run_binding_value(run, slots, current_function, item_slot)?
            }
        };
        let name = push_build_row!(self, expr, BuildExprRow::Str(name.to_string().into()));
        let call = push_build_row!(
            self,
            expr,
            BuildExprRow::ModuleCall {
                cli_plan: None,
                op: RuntimeOp::EnvSet,
                args: vec![Some(name), Some(value)],
                span,
            }
        );
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::Expr { value: call, span }
        ))
    }

    /// The binding an assignment writes through; an environment variable
    /// target has none.
    fn assign_target_root_name(&self, id: crate::syntax::arena::AssignTargetId) -> Option<Name> {
        match self.program.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(name) => Some(name),
            ArenaAssignTargetKind::Env(_) => None,
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => self.assign_target_root_name(base),
        }
    }

    fn compact_pattern_error_family(
        &self,
        family: Name,
    ) -> Option<(Name, crate::sema::check::ErrorFamilyInfo)> {
        if let Some((namespace, member)) = family.as_str().rsplit_once('.') {
            let key =
                self.compact_qualified_function_key(Name::intern(namespace), Name::intern(member));
            let info = self
                .declarations
                .qualified_error_families
                .get(&key)?
                .clone();
            Some((Name::intern(key.to_string()), info))
        } else {
            self.declarations
                .error_families_by_name
                .get(&family)
                .cloned()
                .map(|info| (family, info))
        }
    }

    fn compact_qualified_pattern_facet(&self, name: Name) -> Option<Name> {
        let text = name.as_str();
        let (namespace, facet) = text.rsplit_once('.')?;
        let namespace = self.compact_imported_module_owner(Name::intern(namespace))?;
        let facet = Name::intern(facet);
        self.declarations
            .qualified_error_families
            .iter()
            .any(|(key, info)| {
                key.namespace == namespace
                    && info
                        .variants
                        .values()
                        .any(|variant| variant.facets.contains(&facet))
            })
            .then_some(facet)
    }

    fn prepared_schema(&self, ty: Type) -> Arc<super::require::PreparedSchema> {
        if let Some(schema) = self
            .scratch
            .borrow()
            .prepared_schemas
            .iter()
            .find_map(|(prepared, schema)| (*prepared == ty).then(|| schema.clone()))
        {
            return schema;
        }
        let schema = super::require::PreparedSchema::compile(&ty, &self.declarations.wire_enums);
        self.scratch
            .borrow_mut()
            .prepared_schemas
            .push((ty, schema.clone()));
        schema
    }

    fn compact_tag_type_name(&self, name: Name) -> Option<Name> {
        if let Some((namespace, member)) = name.as_str().rsplit_once('.') {
            return self
                .declarations
                .qualified_tag_variants
                .get(
                    &self.compact_qualified_function_key(
                        Name::intern(namespace),
                        Name::intern(member),
                    ),
                )
                .map(|variant| variant.type_name);
        }
        self.current_namespace
            .and_then(|namespace| {
                self.declarations
                    .qualified_tag_variants
                    .get(&QualifiedName::new(namespace, name))
            })
            .or_else(|| self.declarations.tag_variants_by_name.get(&name))
            .map(|variant| variant.type_name)
    }

    fn compact_tag_wire(
        &self,
        name: Name,
    ) -> Option<Arc<crate::sema::wire_enums::WireEnumMapping>> {
        self.declarations
            .wire_enums
            .mappings
            .get(&self.compact_tag_type_name(name)?)
            .cloned()
    }

    fn compact_tag_variant_field_types(&self, name: Name) -> Option<Vec<Type>> {
        if let Some((namespace, member)) = name.as_str().rsplit_once('.') {
            return self
                .declarations
                .qualified_tag_variants
                .get(
                    &self.compact_qualified_function_key(
                        Name::intern(namespace),
                        Name::intern(member),
                    ),
                )
                .map(|variant| variant.field_types.clone());
        }
        self.current_namespace
            .and_then(|namespace| {
                self.declarations
                    .qualified_tag_variants
                    .get(&QualifiedName::new(namespace, name))
            })
            .or_else(|| self.declarations.tag_variants_by_name.get(&name))
            .map(|variant| variant.field_types.clone())
    }

    fn compact_tag_variant_arity(&self, name: Name) -> Option<usize> {
        if let Some((namespace, member)) = name.as_str().rsplit_once('.') {
            return self.compact_qualified_tag_variant_arity(
                Name::intern(namespace),
                Name::intern(member),
            );
        }
        if let Some(namespace) = self.current_namespace
            && let Some(variant) = self
                .declarations
                .qualified_tag_variants
                .get(&QualifiedName::new(namespace, name))
        {
            return Some(variant.field_count);
        }
        self.declarations
            .tag_variants_by_name
            .get(&name)
            .map(|variant| variant.field_count)
    }

    fn compact_qualified_tag_variant_arity(&self, module: Name, name: Name) -> Option<usize> {
        self.declarations
            .qualified_tag_variants
            .get(&self.compact_qualified_function_key(module, name))
            .map(|variant| variant.field_count)
    }
}

fn construct_top_level_stmt_is_skippable(program: &ArenaProgram, id: StmtId) -> bool {
    if construct_is_main_at_args_call(program, id) {
        return true;
    }
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Export(inner)
        | ArenaStmtKind::Sugar {
            expansion: inner, ..
        } => construct_top_level_stmt_is_skippable(program, inner),
        ArenaStmtKind::Use(use_id) => construct_use_stmt_is_skippable(program, use_id),
        ArenaStmtKind::Expr(expr) if construct_expr_is_reveal_type_call(program, expr) => true,
        ArenaStmtKind::TypeDef(_)
        | ArenaStmtKind::ErrorDef(_)
        | ArenaStmtKind::ProcDef(_)
        | ArenaStmtKind::CliMain(_)
        | ArenaStmtKind::PureDef(_)
        | ArenaStmtKind::StreamDef(_) => true,
        _ => false,
    }
}

fn construct_use_stmt_is_skippable(
    program: &ArenaProgram,
    id: crate::syntax::arena::UseStmtId,
) -> bool {
    let use_stmt = program.arena.use_stmt(id);
    if use_stmt.alias.is_some() || use_stmt.resolved.is_some() {
        return false;
    }
    let mut path = program.arena.names(use_stmt.path);
    let Some(name) = path.next() else {
        return false;
    };
    path.next().is_none() && api_spec().is_standard_module(&name.as_str())
}

fn construct_expr_is_reveal_type_call(program: &ArenaProgram, expr: ExprId) -> bool {
    let ArenaExprKind::Call { callee, .. } = program.arena.expr(expr).kind else {
        return false;
    };
    matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == "reveal_type")
}

fn construct_is_main_at_args_call(program: &ArenaProgram, id: StmtId) -> bool {
    match program.arena.stmt(id).kind {
        ArenaStmtKind::Expr(expr) => construct_is_main_at_args_expr(program, expr),
        _ => false,
    }
}

fn construct_is_main_at_args_expr(program: &ArenaProgram, id: ExprId) -> bool {
    match program.arena.expr(id).kind {
        ArenaExprKind::Try(inner) => construct_is_main_at_args_expr(program, inner),
        ArenaExprKind::Call { callee, args } => {
            matches!(program.arena.expr(callee).kind, ArenaExprKind::Ident(name) if name == Name::intern("main"))
                && matches!(
                    program.arena.call_args(args),
                    [arg] if construct_is_args_call_arg(program, arg)
                )
        }
        _ => false,
    }
}

fn construct_is_args_call_arg(program: &ArenaProgram, arg: &ArenaCallArg) -> bool {
    let ArenaCallArgKind::Splice { value, .. } = arg.kind else {
        return false;
    };
    matches!(program.arena.expr(value).kind, ArenaExprKind::Ident(name) if name == Name::intern("args"))
}

fn is_env_module_expr(
    arena: &crate::syntax::arena::AstArena,
    expr: crate::syntax::arena::ExprId,
) -> bool {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(name) => name == "env",
        ArenaExprKind::Field { base, .. } => is_env_module_expr(arena, base),
        _ => false,
    }
}

fn compact_pattern_tag_name(name: Name) -> Name {
    name.as_str()
        .rsplit_once('.')
        .map_or(name, |(_, member)| Name::intern(member))
}

fn compact_pattern_test_type(
    arena: &AstArena,
    name: Name,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Type {
    if declarations.error_families_by_name.contains_key(&name) {
        return Type::ErrorFamily(name);
    }
    if declarations.error_families_by_name.values().any(|family| {
        family
            .variants
            .values()
            .any(|variant| variant.facets.contains(&name))
    }) || xsh_registry::errors::ErrorFacet::from_name(&name.as_str()).is_some()
    {
        return Type::ErrorFacet(name);
    }
    compact_runtime_type(arena, ty, declarations)
}

fn compact_runtime_type(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Type {
    compact_runtime_type_in_namespace(arena, ty, declarations, None)
}

fn compact_runtime_type_in_namespace(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    namespace: Option<Name>,
) -> Type {
    let resolved = declarations
        .record_constructors
        .resolve_type(arena, ty, namespace);
    if !matches!(resolved, Type::Unknown | Type::Invalid) {
        return resolved;
    }
    compact_runtime_type_inner(arena, ty, declarations, 0)
}

fn compact_runtime_type_inner(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    depth: usize,
) -> Type {
    if depth > declarations.types.len() {
        return Type::Unknown;
    }
    let index = ty.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Applied => Type::Invalid,
        ArenaTypeExprTag::Named => {
            let name = Name::from_symbol(Symbol::from_raw(data.lhs));
            if let Some(builtin) = BuiltinTypeName::parse(&name.as_str()) {
                return Type::from_builtin_name(builtin);
            }
            if let Some(record) = standard_record_type(&name.as_str()) {
                return record;
            }
            if declarations.error_families_by_name.contains_key(&name) {
                return Type::ErrorFamily(name);
            }
            match declarations.types.get(&name) {
                Some(CompactTypeDefInfo::Alias(alias)) => {
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Bounded(alias, range)) => compact_bounded_type(
                    *range,
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1),
                ),
                Some(CompactTypeDefInfo::Record(_)) => {
                    compact_record_type(arena, name, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                Some(CompactTypeDefInfo::TagUnion) => Type::Tag(name),
                None => Type::ErasedRecord,
            }
        }
        ArenaTypeExprTag::Qualified => {
            let name = Name::from_symbol(Symbol::from_raw(data.rhs));
            match declarations.types.get(&name) {
                Some(CompactTypeDefInfo::Alias(alias)) => {
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Bounded(alias, range)) => compact_bounded_type(
                    *range,
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1),
                ),
                Some(CompactTypeDefInfo::Record(_)) => {
                    compact_record_type(arena, name, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                Some(CompactTypeDefInfo::TagUnion) => Type::Tag(name),
                None => Type::ErasedRecord,
            }
        }
        ArenaTypeExprTag::List => Type::List(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::NonEmpty => Type::non_empty(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        )),
        ArenaTypeExprTag::Set => Type::Set(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Map => Type::Map(
            Box::new(
                TypeExprId::from_optional_raw(data.rhs).map_or(Type::Str, |id| {
                    compact_runtime_type_inner(arena, id, declarations, depth)
                }),
            ),
            Box::new(compact_runtime_type_inner(
                arena,
                TypeExprId::from_index(data.lhs as usize),
                declarations,
                depth,
            )),
        ),
        ArenaTypeExprTag::Stream => Type::Stream(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Module => Type::DynamicModule,
        ArenaTypeExprTag::Result => Type::Result(
            Box::new(compact_runtime_type_inner(
                arena,
                TypeExprId::from_index(data.lhs as usize),
                declarations,
                depth,
            )),
            Box::new(
                TypeExprId::from_optional_raw(data.rhs).map_or(Type::Error, |err| {
                    compact_runtime_type_inner(arena, err, declarations, depth)
                }),
            ),
        ),
        ArenaTypeExprTag::Optional => Type::Optional(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Union => Type::Union(
            arena
                .union_type_members(ty)
                .map(|member| compact_runtime_type_inner(arena, member, declarations, depth))
                .collect(),
        ),
        // The signature is a checked fact, not something a slot can test: a
        // slot of a callable type holds the dynamic handle of its kind.
        ArenaTypeExprTag::Callable => {
            if arena.callable_type_expr(ty).pure {
                Type::Pure
            } else {
                Type::Proc
            }
        }
    }
}

fn compact_record_type(
    arena: &AstArena,
    name: Name,
    declarations: &CompactDeclOutput,
    depth: usize,
) -> Type {
    if let Some(fields) = declarations.record_schema_fields.get(&name) {
        return Type::Record(
            fields
                .iter()
                .map(|(field, ty)| {
                    (
                        *field,
                        compact_runtime_type_inner(arena, *ty, declarations, depth),
                    )
                })
                .collect(),
        );
    }
    match declarations.types.get(&name) {
        Some(CompactTypeDefInfo::Record(fields)) => Type::Record(fields.clone()),
        _ => Type::Unknown,
    }
}

fn compact_type_check(
    kind: LoweredType,
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    namespace: Option<Name>,
) -> Option<LoweredTypeCheck> {
    let checked = compact_runtime_type_in_namespace(arena, ty, declarations, namespace);
    // A validated type stored as a scalar has a storage kind that says
    // nothing of the validation, so the type itself is tested where an
    // unchecked value arrives.
    (lowered_type_needs_static_check(kind)
        || checked.has_unsigned_constraint()
        || checked.validated().is_some())
    .then(|| LoweredTypeCheck {
        schema: None,
        ty: checked,
        name: compact_type_expr_name(arena, ty),
    })
}

fn lowered_type_needs_static_check(kind: LoweredType) -> bool {
    matches!(
        kind,
        LoweredType::Error
            | LoweredType::Record
            | LoweredType::Module
            | LoweredType::List
            | LoweredType::Stream
            | LoweredType::Map
            | LoweredType::Set
            | LoweredType::Tag
            | LoweredType::Result
            | LoweredType::Any
    )
}

fn compact_type_expr_name(arena: &AstArena, ty: TypeExprId) -> Arc<str> {
    compact_type_expr_name_string(arena, ty).into()
}

fn compact_type_expr_name_string(arena: &AstArena, ty: TypeExprId) -> String {
    let index = ty.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Applied => format!(
            "{}[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize)),
            arena
                .applied_type_arguments(ty)
                .map(|argument| compact_type_expr_name_string(arena, argument))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ArenaTypeExprTag::Named => Name::from_symbol(Symbol::from_raw(data.lhs)).to_string(),
        ArenaTypeExprTag::Qualified => {
            let namespace = Name::from_symbol(Symbol::from_raw(data.lhs));
            let name = Name::from_symbol(Symbol::from_raw(data.rhs));
            format!("{namespace}.{name}")
        }
        ArenaTypeExprTag::List => format!(
            "List[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::NonEmpty => format!(
            "NonEmpty[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Set => format!(
            "Set[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Map => {
            let value =
                compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize));
            match TypeExprId::from_optional_raw(data.rhs) {
                Some(key) => format!(
                    "Map[{}, {value}]",
                    compact_type_expr_name_string(arena, key)
                ),
                None => format!("Map[{value}]"),
            }
        }
        ArenaTypeExprTag::Stream => format!(
            "Stream[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Module => format!(
            "Module[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Result => {
            let ok =
                compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize));
            if let Some(err) = TypeExprId::from_optional_raw(data.rhs) {
                format!(
                    "Result[{ok}, {}]",
                    compact_type_expr_name_string(arena, err)
                )
            } else {
                format!("Result[{ok}]")
            }
        }
        ArenaTypeExprTag::Optional => format!(
            "{}?",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Union => format!(
            "Union[{}]",
            arena
                .union_type_members(ty)
                .map(|member| compact_type_expr_name_string(arena, member))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ArenaTypeExprTag::Callable => {
            let callable = arena.callable_type_expr(ty);
            let params = arena
                .params(callable.params)
                .iter()
                .map(|param| {
                    format!(
                        "{}: {}",
                        param.name,
                        compact_type_expr_name_string(arena, param.ty)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let effects = callable.effects.map_or(String::new(), |effects| {
                format!(
                    " [{}]",
                    arena
                        .effects(effects)
                        .map(|effect| effect.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
            format!(
                "{}({params}){effects} -> {}",
                if callable.pure { "pure" } else { "proc" },
                compact_type_expr_name_string(arena, callable.return_ty)
            )
        }
    }
}

/// The type a `guard let` target receives from a subject of type `subject`:
/// the non-null value of an optional guard, the `Ok` payload otherwise.
fn guard_bound_type(subject: &Type, optional: bool) -> Option<&Type> {
    match subject {
        Type::Optional(present) if optional => Some(present),
        _ if optional => None,
        subject => subject.result_ok(),
    }
}

fn record_binding_types(
    program: &ArenaProgram,
    target: BindingTargetId,
    ty: Option<&Type>,
) -> Vec<(Name, Option<Type>)> {
    match program.arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(name) => {
            if is_discard_name(name) {
                Vec::new()
            } else {
                vec![(name, ty.cloned())]
            }
        }
        ArenaBindingTargetKind::Record { fields, .. } => program
            .arena
            .destructure_fields(fields)
            .iter()
            .flat_map(|field| {
                let field_ty = match ty {
                    Some(Type::Record(schema)) => schema.get(&field.name),
                    _ => None,
                };
                record_binding_types(program, field.target, field_ty)
            })
            .collect(),
    }
}

fn simple_binding_target(program: &ArenaProgram, id: BindingTargetId) -> Option<Name> {
    match program.arena.binding_target(id).kind {
        ArenaBindingTargetKind::Name(name) => Some(name),
        ArenaBindingTargetKind::Record { .. } => None,
    }
}

fn is_discard_name(name: Name) -> bool {
    name == "_"
}

fn lowered_checked_type(ty: &Type) -> Option<LoweredType> {
    match ty {
        Type::Unit => Some(LoweredType::Unit),
        Type::Int | Type::UInt => Some(LoweredType::Int),
        Type::Float => Some(LoweredType::Float),
        Type::Duration => Some(LoweredType::Duration),
        Type::Bool => Some(LoweredType::Bool),
        Type::Str => Some(LoweredType::Str),
        Type::Bytes => Some(LoweredType::Bytes),
        Type::Digest => Some(LoweredType::Digest),
        Type::Regex => Some(LoweredType::Regex),
        Type::Status => Some(LoweredType::Status),
        Type::Path => Some(LoweredType::Path),
        Type::Command => Some(LoweredType::Command),
        Type::ProcessHandle => Some(LoweredType::ProcessHandle),
        Type::NetJob => Some(LoweredType::NetJob),
        Type::FsRoot => Some(LoweredType::FsRoot),
        Type::Pure => Some(LoweredType::Pure),
        Type::Proc => Some(LoweredType::Proc),
        Type::Callable(callable) if callable.pure => Some(LoweredType::Pure),
        Type::Callable(_) => Some(LoweredType::Proc),
        Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError => {
            Some(LoweredType::Error)
        }
        Type::ErasedRecord | Type::Record(_) => Some(LoweredType::Record),
        Type::Module(_) | Type::DynamicModule => Some(LoweredType::Module),
        Type::List(_) => Some(LoweredType::List),
        Type::Stream(_) => Some(LoweredType::Stream),
        Type::Set(_) => Some(LoweredType::Set),
        Type::Map(_, _) => Some(LoweredType::Map),
        Type::Tag(_) => Some(LoweredType::Tag),
        Type::Result(_, _) => Some(LoweredType::Result),
        Type::Any | Type::Unknown | Type::Invalid => Some(LoweredType::Any),
        // A union value is stored as whichever member it is, so its slot
        // has no single storage kind.
        Type::Union(_) => Some(LoweredType::Any),
        // A validated value is stored as a value of its base.
        Type::Validated(validated) => lowered_checked_type(validated.base()),
        _ => None,
    }
}

fn lowered_method_supported_for_type(ty: &Type, name: Name, arg_count: usize) -> bool {
    match ty {
        Type::Any | Type::Unknown => lowered_method_name(&name.as_str()),
        Type::Invalid => true,
        Type::Optional(inner) => lowered_method_supported_for_type(inner, name, arg_count),
        // The checker rejects a method call on a union that has not been
        // narrowed to one member, so no checked receiver has this type. A
        // receiver that does is not guessed at from its members.
        Type::Union(_) => false,
        // The checker resolved the call against the validation's own
        // receiver when that lists the method, and against the base
        // otherwise.
        Type::Validated(validated) => {
            validated
                .validation()
                .method_receiver()
                .and_then(|receiver| api_spec().method_overloads(receiver, &name.as_str()))
                .is_some_and(|methods| {
                    methods
                        .iter()
                        .any(|method| method.sig.params.len() == arg_count)
                })
                || lowered_method_supported_for_type(validated.base(), name, arg_count)
        }
        Type::Result(ok, _) => {
            name == "context" && (arg_count == 1 || arg_count == 2)
                || lowered_method_supported_for_type(ok, name, arg_count)
        }
        Type::Int | Type::UInt => {
            (name == "float" && arg_count == 0)
                || (matches!(name.as_str().as_str(), "bit_and" | "bit_or" | "clear_bits")
                    && arg_count == 1)
        }
        Type::Float => match name.as_str().as_str() {
            "floor" | "ceil" | "round" | "sqrt" | "exp" | "ln" | "sin" | "cos" | "tan" | "abs" => {
                arg_count == 0
            }
            "format" => arg_count <= 1,
            "format_number" => (1..=2).contains(&arg_count),
            "pow" | "log" | "atan2" => arg_count == 1,
            _ => false,
        },
        Type::Str => match name.as_str().as_str() {
            "trim"
            | "lower"
            | "upper"
            | "reverse"
            | "lines"
            | "words"
            | "parse_int"
            | "parse_int_decimal"
            | "parse_uint"
            | "parse_uint_positive"
            | "parse_float"
            | "base64_decode"
            | "base32_decode"
            | "count_lines"
            | "count_words"
            | "count_chars"
            | "is_empty"
            | "byte_len" => arg_count == 0,
            "fields" | "squeeze" => arg_count <= 1,
            "split" => arg_count == 1 || arg_count == 2,
            "wrap" | "delete" | "starts_with" | "ends_with" => arg_count == 1,
            "replace" | "translate" => arg_count == 2,
            "byte_at" => arg_count == 1,
            "byte_slice" | "find" => arg_count == 1 || arg_count == 2,
            _ => false,
        },
        Type::Bytes => match name.as_str().as_str() {
            "trim" | "lines" | "count_lines" | "len" | "lower" | "base64" | "base32" | "md5"
            | "sha1" | "sha256" | "sha512" | "utf8" | "is_empty" => arg_count == 0,
            "dump" | "strings" => arg_count <= 1,
            "chunks" | "compare" | "starts_with" | "ends_with" => arg_count == 1,
            "byte_at" => arg_count == 1,
            "slice" => arg_count == 1 || arg_count == 2,
            _ => false,
        },
        Type::Digest => matches!(name.as_str().as_str(), "hex" | "base64") && arg_count == 0,
        Type::Regex => match name.as_str().as_str() {
            "matches" | "find" | "captures" => arg_count == 1,
            "replace" => arg_count == 2,
            _ => false,
        },
        Type::Status => match name.as_str().as_str() {
            "exited" | "signaled" | "exit_code" | "signal_number" | "shell_code" => arg_count == 0,
            "exited_with" => arg_count == 1,
            _ => false,
        },
        Type::Path => match name.as_str().as_str() {
            "display" | "name" | "basename" | "dirname" | "ext" | "normalize" | "parent"
            | "lines" | "bytes_lines" | "read_text" | "read_bytes" | "exists" | "executable"
            | "du" | "metadata" | "readlink" | "resolve" | "remove_dir" | "unlink"
            | "read_lines" | "components" | "bytes" => arg_count == 0,
            "ext_or" => arg_count == 1,
            "with_ext" | "strip_prefix" | "relative_to" | "touch_from" | "truncate"
            | "hardlink" | "write" | "write_atomic" | "starts_with" | "ends_with"
            | "write_lines" | "glob" | "rglob" => arg_count == 1,
            "copy" | "rename" | "mkdir" | "remove" => arg_count == 1 || arg_count == 2,
            "touch" => arg_count <= 1,
            _ => false,
        },
        Type::ErasedRecord | Type::Record(_) | Type::Module(_) | Type::DynamicModule => {
            name == "get" && arg_count == 1
                || matches!(name.as_str().as_str(), "keys" | "len") && arg_count == 0
        }
        Type::Set(_) => match name.as_str().as_str() {
            "len" | "is_empty" | "to_list" => arg_count == 0,
            "add" | "remove" => arg_count == 1,
            _ => false,
        },
        Type::List(_) => match name.as_str().as_str() {
            "collect" | "len" | "is_empty" | "to_set" => arg_count == 0,
            "push" | "extend" => arg_count == 1,
            "get" => arg_count == 1,
            "join" => arg_count <= 1,
            _ => false,
        },
        Type::Map(_, _) => match name.as_str().as_str() {
            "len" | "keys" | "values" | "is_empty" => arg_count == 0,
            "remove" => arg_count == 1,
            "get" => arg_count == 1,
            "set" | "push" => arg_count == 2,
            _ => false,
        },
        Type::ProcessHandle => name == "cancel" && arg_count <= 2,
        Type::FsRoot => api_spec()
            .method_overloads(crate::modules::MethodReceiver::FsRoot, &name.as_str())
            .is_some_and(|methods| {
                methods.iter().any(|method| {
                    arg_count <= method.sig.params.len()
                        && arg_count
                            >= method
                                .sig
                                .params
                                .iter()
                                .filter(|param| !param.defaulted)
                                .count()
                })
            }),
        Type::NetJob => matches!(name.as_str().as_str(), "wait" | "cancel") && arg_count == 0,
        Type::Stream(_) => name == "collect" && arg_count == 0,
        _ => false,
    }
}

fn checked_fact_is_resolved(ty: &Type) -> bool {
    !ty.contains_inference()
        && match ty {
            Type::Unknown | Type::Invalid => false,
            Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
                checked_fact_is_resolved(inner)
            }
            Type::Map(key, value) | Type::Result(key, value) => {
                checked_fact_is_resolved(key) && checked_fact_is_resolved(value)
            }
            Type::Record(fields) => fields.values().all(checked_fact_is_resolved),
            Type::Validated(validated) => checked_fact_is_resolved(validated.base()),
            _ => true,
        }
}

fn lowered_builtin_type_name(name: &str) -> Option<LoweredType> {
    match BuiltinTypeName::parse(name)? {
        BuiltinTypeName::Any | BuiltinTypeName::Unknown => Some(LoweredType::Any),
        BuiltinTypeName::Unit => Some(LoweredType::Unit),
        BuiltinTypeName::Int | BuiltinTypeName::UInt => Some(LoweredType::Int),
        BuiltinTypeName::Float => Some(LoweredType::Float),
        BuiltinTypeName::Duration => Some(LoweredType::Duration),
        BuiltinTypeName::Bool => Some(LoweredType::Bool),
        BuiltinTypeName::Str => Some(LoweredType::Str),
        BuiltinTypeName::Bytes => Some(LoweredType::Bytes),
        BuiltinTypeName::Digest => Some(LoweredType::Digest),
        BuiltinTypeName::Regex => Some(LoweredType::Regex),
        BuiltinTypeName::Status => Some(LoweredType::Status),
        BuiltinTypeName::Path | BuiltinTypeName::RelPath => Some(LoweredType::Path),
        BuiltinTypeName::Command => Some(LoweredType::Command),
        BuiltinTypeName::ProcessHandle => Some(LoweredType::ProcessHandle),
        BuiltinTypeName::NetJob => Some(LoweredType::NetJob),
        BuiltinTypeName::FsRoot => Some(LoweredType::FsRoot),
        BuiltinTypeName::Pure => Some(LoweredType::Pure),
        BuiltinTypeName::Proc => Some(LoweredType::Proc),
        BuiltinTypeName::Error | BuiltinTypeName::ProcessError => Some(LoweredType::Error),
        BuiltinTypeName::Record => Some(LoweredType::Record),
        BuiltinTypeName::Module => Some(LoweredType::Module),
        BuiltinTypeName::Result => Some(LoweredType::Result),
        BuiltinTypeName::Null | BuiltinTypeName::Map | BuiltinTypeName::EnvPathList => None,
    }
}

fn top_level_known_with_runtime_bindings() -> FxHashMap<Name, LoweredTopLevelBinding> {
    let mut known = FxHashMap::default();
    let args = LoweredTopLevelBinding {
        kind: LoweredType::List,
        checked: None,
        mutable: false,
        slot: true,
    };
    known.insert(Name::intern("args"), args);
    known
}

pub(super) fn top_level_slots(known: &FxHashMap<Name, LoweredTopLevelBinding>) -> SlotScope {
    let mut names = known
        .iter()
        .filter_map(|(name, binding)| binding.slot.then_some(*name))
        .collect::<Vec<_>>();
    names.sort_unstable();
    let mut slots = SlotScope::from_names(names.iter().copied());
    for name in names {
        let Some(binding) = known.get(&name) else {
            continue;
        };
        if let Some(ty) = binding
            .checked
            .clone()
            .or_else(|| type_for_lowered_type(binding.kind))
        {
            slots.types.insert(name, ty);
        }
    }
    slots
}

fn type_for_lowered_type(kind: LoweredType) -> Option<Type> {
    match kind {
        LoweredType::Unit => Some(Type::Unit),
        LoweredType::Int => Some(Type::Int),
        LoweredType::Float => Some(Type::Float),
        LoweredType::Duration => Some(Type::Duration),
        LoweredType::Bool => Some(Type::Bool),
        LoweredType::Str => Some(Type::Str),
        LoweredType::Bytes => Some(Type::Bytes),
        LoweredType::Digest => Some(Type::Digest),
        LoweredType::Regex => Some(Type::Regex),
        LoweredType::Status => Some(Type::Status),
        LoweredType::Path => Some(Type::Path),
        LoweredType::Command => Some(Type::Command),
        LoweredType::ProcessHandle => Some(Type::ProcessHandle),
        LoweredType::NetJob => Some(Type::NetJob),
        LoweredType::FsRoot => Some(Type::FsRoot),
        LoweredType::Pure => Some(Type::Pure),
        LoweredType::Proc => Some(Type::Proc),
        LoweredType::Error => Some(Type::Error),
        LoweredType::Record => Some(Type::ErasedRecord),
        LoweredType::Module => Some(Type::DynamicModule),
        LoweredType::List => Some(Type::List(Box::new(Type::Any))),
        LoweredType::Stream => Some(Type::Stream(Box::new(Type::Any))),
        LoweredType::Map => Some(Type::Map(Box::new(Type::Str), Box::new(Type::Any))),
        // The element type is not part of the kind.
        LoweredType::Set => None,
        LoweredType::Tag => None,
        LoweredType::Result => Some(Type::Result(Box::new(Type::Any), Box::new(Type::Error))),
        LoweredType::Any => Some(Type::Any),
    }
}

pub(super) fn lowered_top_level(
    scratch: &Rc<RefCell<BuildScratch>>,
    kind: BuildTopKind,
    known: &FxHashMap<Name, LoweredTopLevelBinding>,
    slot_indexes: SlotScope,
) -> BuildTopStmtId {
    let slot_count = slot_indexes.count();
    let mut slots: LoweredTopLevelSlots = slot_indexes
        .into_entries()
        .filter_map(|(name, slot)| {
            let binding = known.get(&name)?;
            if !binding.slot {
                return None;
            }
            Some(LoweredTopLevelSlot {
                name,
                slot,
                kind: binding.kind,
                mutable: binding.mutable,
            })
        })
        .collect();
    slots.sort_unstable_by_key(|slot| slot.slot);
    scratch.borrow_mut().top_stmt(BuildTopStmtRow {
        kind,
        slots,
        slot_count,
    })
}

impl CompactLowerConstructProbe<'_, '_> {
    fn lower_int_expr_candidate(&self, expr: &BuildExprId) -> Option<BuildIntId> {
        if self
            .scratch
            .borrow()
            .non_int_binary_expressions
            .contains(&expr.index())
        {
            return None;
        }
        let row = {
            let scratch = self.scratch.borrow();
            scratch.expressions[expr.index()].clone()
        };
        match &row {
            BuildExprRow::Int(value) => Some(push_build_row!(self, int, BuildIntRow::Int(*value))),
            BuildExprRow::Param(slot) => Some(push_build_row!(self, int, BuildIntRow::Slot(*slot))),
            BuildExprRow::Binary {
                op, left, right, ..
            } if matches!(
                op,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem
            ) =>
            {
                Some(push_build_row!(
                    self,
                    int,
                    BuildIntRow::Binary {
                        op: *op,
                        left: self.lower_int_expr_candidate(left)?,
                        right: self.lower_int_expr_candidate(right)?,
                    }
                ))
            }
            BuildExprRow::StrByteLen { receiver, span } => {
                let receiver_row = {
                    let scratch = self.scratch.borrow();
                    scratch.expressions[receiver.index()].clone()
                };
                match receiver_row {
                    BuildExprRow::Param(slot) => Some(push_build_row!(
                        self,
                        int,
                        BuildIntRow::StrByteLenSlot { slot, span: *span }
                    )),
                    _ => None,
                }
            }
            BuildExprRow::Method {
                receiver,
                name,
                args,
                span,
            } if name.as_str() == "count_lines" && args.is_empty() => {
                let receiver_row = {
                    let scratch = self.scratch.borrow();
                    scratch.expressions[receiver.index()].clone()
                };
                match receiver_row {
                    BuildExprRow::Param(slot) => Some(push_build_row!(
                        self,
                        int,
                        BuildIntRow::StrCountLinesSlot { slot, span: *span }
                    )),
                    _ => None,
                }
            }
            BuildExprRow::MatchExpr { value, arms, .. } => {
                // Optional fallback lowers to an exhaustive null/present match.
                // Fuse only a literal alternative; effects retain lazy evaluation.
                let [
                    (null_pattern, None, fallback),
                    (present_pattern, None, present),
                ] = arms.as_slice()
                else {
                    return None;
                };
                let scratch = self.scratch.borrow();
                if !matches!(
                    scratch.patterns[null_pattern.index()],
                    BuildPatternRow::Literal(LoweredValue::Null)
                ) {
                    return None;
                }
                let BuildPatternRow::Bind { slot: present_slot } =
                    scratch.patterns[present_pattern.index()]
                else {
                    return None;
                };
                if !matches!(scratch.expressions[present.index()], BuildExprRow::Param(slot) if slot == present_slot)
                {
                    return None;
                }
                let BuildExprRow::StrByteAt {
                    receiver,
                    index,
                    span,
                } = scratch.expressions[value.index()].clone()
                else {
                    return None;
                };
                let BuildExprRow::Param(slot) = scratch.expressions[receiver.index()] else {
                    return None;
                };
                drop(scratch);
                let default = self.lowered_inert_int_literal(*fallback)?;
                Some(push_build_row!(
                    self,
                    int,
                    BuildIntRow::StrByteAtSlot {
                        slot,
                        index: self.lower_int_expr_candidate(&index)?,
                        default: if default == -1 {
                            None
                        } else {
                            Some(push_build_row!(self, int, BuildIntRow::Int(default)))
                        },
                        span,
                    }
                ))
            }
            _ => None,
        }
    }

    fn lower_bool_expr_candidate(&self, expr: &BuildExprId) -> Option<BuildBoolId> {
        let row = {
            let scratch = self.scratch.borrow();
            scratch.expressions[expr.index()].clone()
        };
        match &row {
            BuildExprRow::Bool(value) => {
                Some(push_build_row!(self, bool, BuildBoolRow::Bool(*value)))
            }
            BuildExprRow::Param(slot) => {
                Some(push_build_row!(self, bool, BuildBoolRow::Slot(*slot)))
            }
            BuildExprRow::Binary {
                op,
                left,
                right,
                span,
            } => match op {
                BinaryOp::In | BinaryOp::NotIn => {
                    let receiver_row = self.scratch.borrow().expressions[right.index()].clone();
                    let BuildExprRow::Param(slot) = receiver_row else {
                        return None;
                    };
                    let needle_row = self.scratch.borrow().expressions[left.index()].clone();
                    let candidate = if let BuildExprRow::Str(needle) = needle_row {
                        push_build_row!(
                            self,
                            bool,
                            BuildBoolRow::StrContainsSlot {
                                slot,
                                needle,
                                span: *span
                            }
                        )
                    } else {
                        let needle = self.lowered_literal_value(left)?;
                        push_build_row!(
                            self,
                            bool,
                            BuildBoolRow::ContainsSlot {
                                slot,
                                needle,
                                span: *span
                            }
                        )
                    };
                    Some(if *op == BinaryOp::NotIn {
                        push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                    } else {
                        candidate
                    })
                }
                BinaryOp::And => Some(push_build_row!(
                    self,
                    bool,
                    BuildBoolRow::And(
                        self.lower_bool_expr_candidate(left)?,
                        self.lower_bool_expr_candidate(right)?,
                    )
                )),
                BinaryOp::Or => Some(push_build_row!(
                    self,
                    bool,
                    BuildBoolRow::Or(
                        self.lower_bool_expr_candidate(left)?,
                        self.lower_bool_expr_candidate(right)?,
                    )
                )),
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge => {
                    if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                        if self.lowered_empty_string_literal(right)
                            && let Some((slot, span)) = self.lowered_trim_slot(left)
                        {
                            let candidate = push_build_row!(
                                self,
                                bool,
                                BuildBoolRow::TrimEmptySlot { slot, span }
                            );
                            return Some(if *op == BinaryOp::Eq {
                                candidate
                            } else {
                                push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                            });
                        }
                        if self.lowered_empty_string_literal(left)
                            && let Some((slot, span)) = self.lowered_trim_slot(right)
                        {
                            let candidate = push_build_row!(
                                self,
                                bool,
                                BuildBoolRow::TrimEmptySlot { slot, span }
                            );
                            return Some(if *op == BinaryOp::Eq {
                                candidate
                            } else {
                                push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                            });
                        }
                        if let Some(value) = self.lowered_bool_literal(right) {
                            let candidate = self.lower_bool_expr_candidate(left)?;
                            return Some(
                                if (*op == BinaryOp::Eq && value) || (*op == BinaryOp::Ne && !value)
                                {
                                    candidate
                                } else {
                                    push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                                },
                            );
                        }
                        if let Some(value) = self.lowered_bool_literal(left) {
                            let candidate = self.lower_bool_expr_candidate(right)?;
                            return Some(
                                if (*op == BinaryOp::Eq && value) || (*op == BinaryOp::Ne && !value)
                                {
                                    candidate
                                } else {
                                    push_build_row!(self, bool, BuildBoolRow::Not(candidate))
                                },
                            );
                        }
                    }
                    if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
                        let left_row = self.scratch.borrow().expressions[left.index()].clone();
                        if let BuildExprRow::Param(slot) = left_row
                            && let Some(value) = self.lowered_literal_value(right)
                        {
                            return Some(push_build_row!(
                                self,
                                bool,
                                BuildBoolRow::LiteralCompareSlot {
                                    op: *op,
                                    slot,
                                    value,
                                }
                            ));
                        }
                        let right_row = self.scratch.borrow().expressions[right.index()].clone();
                        if let BuildExprRow::Param(slot) = right_row
                            && let Some(value) = self.lowered_literal_value(left)
                        {
                            return Some(push_build_row!(
                                self,
                                bool,
                                BuildBoolRow::LiteralCompareSlot {
                                    op: *op,
                                    slot,
                                    value,
                                }
                            ));
                        }
                    }
                    let left = self.lower_int_expr_candidate(left)?;
                    let right = self.lower_int_expr_candidate(right)?;
                    if self.lowered_int_expr_needs_type_context(&left)
                        || self.lowered_int_expr_needs_type_context(&right)
                    {
                        return None;
                    }
                    Some(push_build_row!(
                        self,
                        bool,
                        BuildBoolRow::IntCompare {
                            op: *op,
                            left,
                            right,
                        }
                    ))
                }
                _ => None,
            },
            BuildExprRow::StrPredicate {
                receiver,
                predicate,
                needle,
                span,
            } => {
                let needle = self.lowered_needle_bytes(needle)?;
                let receiver_row = self.scratch.borrow().expressions[receiver.index()].clone();
                if let BuildExprRow::Param(slot) = receiver_row {
                    return Some(push_build_row!(
                        self,
                        bool,
                        BuildBoolRow::StrPredicateSlot {
                            slot,
                            predicate: *predicate,
                            needle,
                            span: *span,
                        }
                    ));
                }
                if let Some((slot, trim_span)) = self.lowered_trim_slot(receiver) {
                    return Some(push_build_row!(
                        self,
                        bool,
                        BuildBoolRow::TrimStrPredicateSlot {
                            slot,
                            predicate: *predicate,
                            needle,
                            span: trim_span,
                        }
                    ));
                }
                None
            }
            _ => None,
        }
    }

    fn lowered_inert_int_literal(&self, expr: BuildExprId) -> Option<i64> {
        match self.scratch.borrow().expressions[expr.index()].clone() {
            BuildExprRow::Int(value) => Some(value),
            BuildExprRow::Binary {
                op: BinaryOp::Sub,
                left,
                right,
                ..
            } => {
                let scratch = self.scratch.borrow();
                match (
                    &scratch.expressions[left.index()],
                    &scratch.expressions[right.index()],
                ) {
                    (BuildExprRow::Int(0), BuildExprRow::Int(value)) => value.checked_neg(),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn lowered_literal_value(&self, expr: &BuildExprId) -> Option<LoweredValue> {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        match &row {
            BuildExprRow::Null => Some(LoweredValue::Null),
            BuildExprRow::Unit => Some(LoweredValue::Unit),
            BuildExprRow::Int(value) => Some(LoweredValue::Int(*value)),
            BuildExprRow::Float(value) => Some(LoweredValue::Float(*value)),
            BuildExprRow::Duration(value) => Some(LoweredValue::Duration(value.clone())),
            BuildExprRow::Bool(value) => Some(LoweredValue::Bool(*value)),
            BuildExprRow::Str(value) => Some(LoweredValue::Str(value.clone())),
            BuildExprRow::Bytes(value) => Some(LoweredValue::Bytes(value.clone())),
            _ => None,
        }
    }

    fn lowered_int_expr_needs_type_context(&self, expr: &BuildIntId) -> bool {
        let row = self.scratch.borrow().ints[expr.index()].clone();
        match &row {
            BuildIntRow::Slot(_) => true,
            BuildIntRow::Int(_)
            | BuildIntRow::Binary { .. }
            | BuildIntRow::StrByteLenSlot { .. }
            | BuildIntRow::StrCountLinesSlot { .. }
            | BuildIntRow::StrByteAtSlot { .. } => false,
        }
    }

    fn lowered_empty_string_literal(&self, expr: &BuildExprId) -> bool {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        matches!(&row, BuildExprRow::Str(value) if value.is_empty())
            || matches!(&row, BuildExprRow::Bytes(value) if value.is_empty())
    }

    /// Extract a literal `Str` or `Bytes` needle as bytes, for the byte-level
    /// predicate fast paths. `Str` needles use their UTF-8 bytes, which makes
    /// byte `starts_with`/`ends_with`/`contains` equivalent to the `Str` ops.
    fn lowered_needle_bytes(&self, expr: &BuildExprId) -> Option<Arc<[u8]>> {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        match &row {
            BuildExprRow::Str(value) => Some(value.as_bytes().into()),
            BuildExprRow::Bytes(value) => Some(value.clone()),
            _ => None,
        }
    }

    fn lowered_trim_slot(&self, expr: &BuildExprId) -> Option<(usize, Span)> {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        let BuildExprRow::Method {
            receiver,
            name,
            args,
            span,
        } = &row
        else {
            return None;
        };
        if name.as_str() != "trim" || !args.is_empty() {
            return None;
        }
        let receiver = self.scratch.borrow().expressions[receiver.index()].clone();
        let BuildExprRow::Param(slot) = receiver else {
            return None;
        };
        Some((slot, *span))
    }

    fn lowered_bool_literal(&self, expr: &BuildExprId) -> Option<bool> {
        let row = self.scratch.borrow().expressions[expr.index()].clone();
        match &row {
            BuildExprRow::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn lowered_bool_expr_needs_type_context(&self, expr: &BuildBoolId) -> bool {
        let row = self.scratch.borrow().bools[expr.index()].clone();
        match &row {
            BuildBoolRow::Slot(_) => true,
            BuildBoolRow::Not(inner) => self.lowered_bool_expr_needs_type_context(inner),
            _ => false,
        }
    }

    /// Returns whether a lowered statement list is *guaranteed* to hit an explicit
    /// `Return`/propagation on every control-flow path. This decides whether the
    /// compact lowerer must append an implicit `Return ok unit` for unit/Result[Unit]
    /// procs (and whether value-returning procs are well-formed). It must be
    /// CONSERVATIVE: the runtime never treats a bare tail `Expr` statement as a
    /// return (it yields `StmtFlow::None` for a non-error value), and an `if`
    /// Try to lower a ForStrLines body into a `ScanLines` node for faster
    /// execution. Returns `Some(ScanLines)` if the body matches the simple scanner
    /// pattern: an optional `let trimmed = line.trim()` followed by an `IfBool`
    /// where every branch is a counter increment.
    fn try_lower_scan_lines(
        &self,
        text: &BuildExprId,
        line_slot: usize,
        body: &[BuildStmtId],
        span: Span,
    ) -> Option<BuildStmtId> {
        let text_row = self.scratch.borrow().expressions[text.index()].clone();
        let text_slot = match text_row {
            BuildExprRow::Param(slot) => slot,
            _ => return None,
        };
        let (if_stmt, trimmed_slot) = match body {
            [if_stmt] => (*if_stmt, None),
            [trim_stmt, if_stmt] => {
                let trimmed = self.scratch.borrow().statements[trim_stmt.index()].clone();
                let BuildStmtRow::Let { slot, value } = trimmed else {
                    return None;
                };
                let expression = self.scratch.borrow().expressions[value.index()].clone();
                let BuildExprRow::Method {
                    receiver,
                    name,
                    args,
                    ..
                } = expression
                else {
                    return None;
                };
                if name.as_str() != "trim" || !args.is_empty() {
                    return None;
                }
                if !matches!(
                    self.scratch.borrow().expressions[receiver.index()],
                    BuildExprRow::Param(param) if param == line_slot
                ) {
                    return None;
                }
                (*if_stmt, Some(slot))
            }
            _ => return None,
        };
        let mut checks = Vec::new();
        if !self.collect_scan_checks(if_stmt, trimmed_slot, &mut checks) {
            return None;
        }
        Some(push_build_row!(
            self,
            stmt,
            BuildStmtRow::ScanLines {
                text_slot,
                line_slot,
                checks,
                span,
            }
        ))
    }

    fn try_lower_scan_bytes(
        &self,
        line_slot: usize,
        body: &[BuildStmtId],
        span: Span,
    ) -> Option<Vec<BuildStmtId>> {
        if let Some(lowered) = self.try_lower_scan_bytes_direct(line_slot, body, span) {
            return Some(lowered);
        }
        let mut changed = false;
        let mut lowered = Vec::with_capacity(body.len());
        for stmt in body {
            let row = self.scratch.borrow().statements[stmt.index()].clone();
            let replacement = match row {
                BuildStmtRow::If {
                    branches,
                    else_body,
                } => {
                    let mut branch_changed = false;
                    let branches = branches
                        .into_iter()
                        .map(|(condition, branch)| {
                            if let Some(branch) =
                                self.try_lower_scan_bytes(line_slot, &branch, span)
                            {
                                branch_changed = true;
                                (condition, branch)
                            } else {
                                (condition, branch)
                            }
                        })
                        .collect();
                    let else_body = else_body.and_then(|branch| {
                        self.try_lower_scan_bytes(line_slot, &branch, span)
                            .inspect(|_| branch_changed = true)
                            .or(Some(branch))
                    });
                    branch_changed.then(|| {
                        push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::If {
                                branches,
                                else_body,
                            }
                        )
                    })
                }
                BuildStmtRow::IfBool {
                    branches,
                    else_body,
                } => {
                    let mut branch_changed = false;
                    let branches = branches
                        .into_iter()
                        .map(|(condition, branch)| {
                            if let Some(branch) =
                                self.try_lower_scan_bytes(line_slot, &branch, span)
                            {
                                branch_changed = true;
                                (condition, branch)
                            } else {
                                (condition, branch)
                            }
                        })
                        .collect();
                    let else_body = else_body.and_then(|branch| {
                        self.try_lower_scan_bytes(line_slot, &branch, span)
                            .inspect(|_| branch_changed = true)
                            .or(Some(branch))
                    });
                    branch_changed.then(|| {
                        push_build_row!(
                            self,
                            stmt,
                            BuildStmtRow::IfBool {
                                branches,
                                else_body,
                            }
                        )
                    })
                }
                _ => None,
            };
            if let Some(stmt) = replacement {
                changed = true;
                lowered.push(stmt);
            } else {
                lowered.push(*stmt);
            }
        }
        changed.then_some(lowered)
    }

    fn try_lower_scan_bytes_direct(
        &self,
        line_slot: usize,
        body: &[BuildStmtId],
        span: Span,
    ) -> Option<Vec<BuildStmtId>> {
        let (while_index, while_id) = body.iter().enumerate().find_map(|(index, stmt)| {
            matches!(
                self.scratch.borrow().statements[stmt.index()],
                BuildStmtRow::WhileBool { .. } | BuildStmtRow::While { .. }
            )
            .then_some((index, *stmt))
        })?;
        let (index_slot, line_len_slot, loop_body) =
            match self.scratch.borrow().statements[while_id.index()].clone() {
                BuildStmtRow::WhileBool { condition, body } => {
                    let BuildBoolRow::IntCompare {
                        op: BinaryOp::Lt,
                        left,
                        right,
                    } = self.scratch.borrow().bools[condition.index()].clone()
                    else {
                        return None;
                    };
                    (
                        self.scan_bytes_int_slot(left)?,
                        self.scan_bytes_int_slot(right)?,
                        body,
                    )
                }
                BuildStmtRow::While { condition, body } => {
                    let BuildExprRow::Binary {
                        op: BinaryOp::Lt,
                        left,
                        right,
                        ..
                    } = self.scratch.borrow().expressions[condition.index()].clone()
                    else {
                        return None;
                    };
                    (
                        self.scan_bytes_expr_slot(left)?,
                        self.scan_bytes_expr_slot(right)?,
                        body,
                    )
                }
                _ => return None,
            };
        if loop_body.len() != 3 {
            return None;
        }
        let (ch_slot, next_slot) = match (
            self.scratch.borrow().statements[loop_body[0].index()].clone(),
            self.scratch.borrow().statements[loop_body[1].index()].clone(),
        ) {
            (
                BuildStmtRow::LetInt {
                    slot: ch_slot,
                    value: byte_value,
                },
                BuildStmtRow::LetInt {
                    slot: next_slot,
                    value: next_value,
                },
            ) if self.scan_bytes_byte_at(byte_value, line_slot, index_slot, 0)
                && self.scan_bytes_byte_at(next_value, line_slot, index_slot, 1) =>
            {
                (ch_slot, next_slot)
            }
            (
                BuildStmtRow::Let {
                    slot: ch_slot,
                    value: byte_value,
                },
                BuildStmtRow::Let {
                    slot: next_slot,
                    value: next_value,
                },
            ) if self.scan_bytes_byte_at_expr(byte_value, line_slot, index_slot, 0)
                && self.scan_bytes_byte_at_expr(next_value, line_slot, index_slot, 1) =>
            {
                (ch_slot, next_slot)
            }
            _ => return None,
        };
        let control = self.scratch.borrow().statements[loop_body[2].index()].clone();
        if let BuildStmtRow::If {
            branches,
            else_body: Some(_),
        } = control.clone()
        {
            if branches.len() != 5 {
                return None;
            }
            let block_depth_slot =
                self.scan_bytes_expr_compare_slot(branches[0].0, BinaryOp::Gt, Some(0), None)?;
            let in_string_slot = self.scan_bytes_expr_slot(branches[1].0)?;
            if !self.scan_bytes_expr_quote_condition(branches[2].0, ch_slot)
                || !self.scan_bytes_expr_pair_condition(branches[3].0, ch_slot, next_slot, 47, 47)
                || !self.scan_bytes_expr_pair_condition(branches[4].0, ch_slot, next_slot, 47, 42)
            {
                return None;
            }
            let comment_seen_slot = self.scan_bytes_expr_true_assignment(&branches[0].1)?;
            let code_seen_slot = self.scan_bytes_expr_true_assignment(&branches[1].1)?;
            let escaped_slot = self.scan_bytes_expr_nested_slot(&branches[1].1)?;
            let string_delim_slot =
                self.scan_bytes_expr_delimiter_assignment(&branches[2].1, ch_slot)?;
            let config = ScanBytes {
                line_slot,
                block_depth_slot,
                code_seen_slot,
                comment_seen_slot,
                in_string_slot,
                string_delim_slot,
                escaped_slot,
                nested: false,
                span,
            };
            let scan = push_build_row!(self, stmt, BuildStmtRow::ScanBytes { config });
            let mut lowered = body.to_vec();
            lowered[while_index] = scan;
            return Some(lowered);
        }
        let BuildStmtRow::IfBool {
            branches,
            else_body,
        } = control
        else {
            return None;
        };
        if branches.len() != 5 || else_body.is_none() {
            return None;
        }
        let block_depth_slot =
            self.scan_bytes_compare_slot(branches[0].0, BinaryOp::Gt, Some(0), None)?;
        let in_string_slot = self.scan_bytes_bool_slot(branches[1].0)?;
        if !self.scan_bytes_quote_condition(branches[2].0, ch_slot)
            || !self.scan_bytes_pair_condition(branches[3].0, ch_slot, next_slot, 47, 47)
            || !self.scan_bytes_pair_condition(branches[4].0, ch_slot, next_slot, 47, 42)
        {
            return None;
        }
        let comment_seen_slot = self.scan_bytes_true_assignment(&branches[0].1)?;
        let code_seen_slot = self.scan_bytes_true_assignment(&branches[1].1)?;
        let escaped_slot = self.scan_bytes_nested_bool_slot(&branches[1].1)?;
        let string_delim_slot = self.scan_bytes_delimiter_assignment(&branches[2].1, ch_slot)?;
        if line_len_slot == index_slot
            || block_depth_slot == code_seen_slot
            || block_depth_slot == comment_seen_slot
            || in_string_slot == escaped_slot
        {
            return None;
        }
        let config = ScanBytes {
            line_slot,
            block_depth_slot,
            code_seen_slot,
            comment_seen_slot,
            in_string_slot,
            string_delim_slot,
            escaped_slot,
            nested: false,
            span,
        };
        let scan = push_build_row!(self, stmt, BuildStmtRow::ScanBytes { config });
        let mut lowered = body.to_vec();
        lowered[while_index] = scan;
        Some(lowered)
    }

    fn scan_bytes_int_slot(&self, value: BuildIntId) -> Option<usize> {
        match self.scratch.borrow().ints[value.index()] {
            BuildIntRow::Slot(slot) => Some(slot),
            _ => None,
        }
    }

    fn scan_bytes_expr_slot(&self, value: BuildExprId) -> Option<usize> {
        match self.scratch.borrow().expressions[value.index()] {
            BuildExprRow::Param(slot) => Some(slot),
            _ => None,
        }
    }

    fn scan_bytes_byte_at(
        &self,
        value: BuildIntId,
        line_slot: usize,
        index_slot: usize,
        offset: i64,
    ) -> bool {
        let BuildIntRow::StrByteAtSlot {
            slot,
            index,
            default,
            ..
        } = self.scratch.borrow().ints[value.index()].clone()
        else {
            return false;
        };
        if slot != line_slot || default.is_some() {
            return false;
        }
        match (self.scratch.borrow().ints[index.index()].clone(), offset) {
            (BuildIntRow::Slot(slot), 0) => slot == index_slot,
            (
                BuildIntRow::Binary {
                    op: BinaryOp::Add,
                    left,
                    right,
                },
                1,
            ) => {
                self.scan_bytes_int_slot(left) == Some(index_slot)
                    && matches!(
                        self.scratch.borrow().ints[right.index()],
                        BuildIntRow::Int(1)
                    )
            }
            _ => false,
        }
    }

    fn scan_bytes_byte_at_expr(
        &self,
        value: BuildExprId,
        line_slot: usize,
        index_slot: usize,
        offset: i64,
    ) -> bool {
        let BuildExprRow::StrByteAt {
            receiver, index, ..
        } = self.scratch.borrow().expressions[value.index()].clone()
        else {
            return false;
        };
        if self.scan_bytes_expr_slot(receiver) != Some(line_slot) {
            return false;
        }
        match (
            self.scratch.borrow().expressions[index.index()].clone(),
            offset,
        ) {
            (BuildExprRow::Param(slot), 0) => slot == index_slot,
            (
                BuildExprRow::Binary {
                    op: BinaryOp::Add,
                    left,
                    right,
                    ..
                },
                1,
            ) => {
                self.scan_bytes_expr_slot(left) == Some(index_slot)
                    && matches!(
                        self.scratch.borrow().expressions[right.index()],
                        BuildExprRow::Int(1)
                    )
            }
            _ => false,
        }
    }

    fn scan_bytes_expr_compare_slot(
        &self,
        value: BuildExprId,
        expected_op: BinaryOp,
        expected_right: Option<i64>,
        expected_left: Option<usize>,
    ) -> Option<usize> {
        let BuildExprRow::Binary {
            op, left, right, ..
        } = self.scratch.borrow().expressions[value.index()].clone()
        else {
            return None;
        };
        if op != expected_op {
            return None;
        }
        if let Some(expected_right) = expected_right
            && !matches!(self.scratch.borrow().expressions[right.index()], BuildExprRow::Int(value) if value == expected_right)
        {
            return None;
        }
        let slot = self.scan_bytes_expr_slot(left)?;
        if expected_left.is_some_and(|expected| expected != slot) {
            return None;
        }
        Some(slot)
    }

    fn scan_bytes_expr_pair_condition(
        &self,
        value: BuildExprId,
        left_slot: usize,
        right_slot: usize,
        left_value: i64,
        right_value: i64,
    ) -> bool {
        let BuildExprRow::Binary {
            op: BinaryOp::And,
            left,
            right,
            ..
        } = self.scratch.borrow().expressions[value.index()].clone()
        else {
            return false;
        };
        self.scan_bytes_expr_compare_slot(left, BinaryOp::Eq, Some(left_value), Some(left_slot))
            .is_some_and(|_| {
                self.scan_bytes_expr_compare_slot(
                    right,
                    BinaryOp::Eq,
                    Some(right_value),
                    Some(right_slot),
                )
                .is_some()
            })
    }

    fn scan_bytes_expr_quote_condition(&self, value: BuildExprId, ch_slot: usize) -> bool {
        let mut values = Vec::new();
        self.scan_bytes_expr_quote_values(value, ch_slot, &mut values);
        values.sort_unstable();
        values == [34, 39, 96]
    }

    fn scan_bytes_expr_quote_values(
        &self,
        value: BuildExprId,
        ch_slot: usize,
        values: &mut Vec<i64>,
    ) {
        match self.scratch.borrow().expressions[value.index()].clone() {
            BuildExprRow::Binary {
                op: BinaryOp::Or,
                left,
                right,
                ..
            } => {
                self.scan_bytes_expr_quote_values(left, ch_slot, values);
                self.scan_bytes_expr_quote_values(right, ch_slot, values);
            }
            BuildExprRow::Binary {
                op: BinaryOp::Eq,
                left,
                right,
                ..
            } if self.scan_bytes_expr_slot(left) == Some(ch_slot) => {
                if let BuildExprRow::Int(value) = self.scratch.borrow().expressions[right.index()] {
                    values.push(value);
                }
            }
            _ => {}
        }
    }

    fn scan_bytes_expr_true_assignment(&self, statements: &[BuildStmtId]) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            match self.scratch.borrow().statements[stmt.index()].clone() {
                BuildStmtRow::Assign {
                    check: None,
                    slot,
                    op: AssignOp::Set,
                    value,
                    ..
                } => matches!(
                    self.scratch.borrow().expressions[value.index()],
                    BuildExprRow::Bool(true)
                )
                .then_some(slot),
                BuildStmtRow::AssignBool { slot, value } => matches!(
                    self.scratch.borrow().bools[value.index()],
                    BuildBoolRow::Bool(true)
                )
                .then_some(slot),
                _ => None,
            }
        })
    }

    fn scan_bytes_expr_nested_slot(&self, statements: &[BuildStmtId]) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::If { branches, .. } =
                self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            branches
                .first()
                .and_then(|(condition, _)| self.scan_bytes_expr_slot(*condition))
        })
    }

    fn scan_bytes_expr_delimiter_assignment(
        &self,
        statements: &[BuildStmtId],
        ch_slot: usize,
    ) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::Assign {
                check: None,
                slot,
                op: AssignOp::Set,
                value,
                ..
            } = self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            (self.scan_bytes_expr_slot(value) == Some(ch_slot)).then_some(slot)
        })
    }

    fn scan_bytes_bool_slot(&self, value: BuildBoolId) -> Option<usize> {
        match self.scratch.borrow().bools[value.index()] {
            BuildBoolRow::Slot(slot) => Some(slot),
            _ => None,
        }
    }

    fn scan_bytes_compare_slot(
        &self,
        value: BuildBoolId,
        expected_op: BinaryOp,
        expected_right: Option<i64>,
        expected_left: Option<usize>,
    ) -> Option<usize> {
        let BuildBoolRow::IntCompare { op, left, right } =
            self.scratch.borrow().bools[value.index()].clone()
        else {
            return None;
        };
        if op != expected_op {
            return None;
        }
        if let Some(expected_right) = expected_right
            && !matches!(self.scratch.borrow().ints[right.index()], BuildIntRow::Int(value) if value == expected_right)
        {
            return None;
        }
        let slot = self.scan_bytes_int_slot(left)?;
        if expected_left.is_some_and(|expected| expected != slot) {
            return None;
        }
        Some(slot)
    }

    fn scan_bytes_pair_condition(
        &self,
        value: BuildBoolId,
        left_slot: usize,
        right_slot: usize,
        left_value: i64,
        right_value: i64,
    ) -> bool {
        let BuildBoolRow::And(left, right) = self.scratch.borrow().bools[value.index()].clone()
        else {
            return false;
        };
        self.scan_bytes_compare_slot(left, BinaryOp::Eq, Some(left_value), Some(left_slot))
            .is_some_and(|_| {
                self.scan_bytes_compare_slot(
                    right,
                    BinaryOp::Eq,
                    Some(right_value),
                    Some(right_slot),
                )
                .is_some()
            })
    }

    fn scan_bytes_quote_condition(&self, value: BuildBoolId, ch_slot: usize) -> bool {
        let BuildBoolRow::Or(left, right) = self.scratch.borrow().bools[value.index()].clone()
        else {
            return false;
        };
        let mut values = Vec::new();
        self.scan_bytes_quote_values(left, ch_slot, &mut values);
        self.scan_bytes_quote_values(right, ch_slot, &mut values);
        values.sort_unstable();
        values == [34, 39, 96]
    }

    fn scan_bytes_quote_values(&self, value: BuildBoolId, ch_slot: usize, values: &mut Vec<i64>) {
        match self.scratch.borrow().bools[value.index()].clone() {
            BuildBoolRow::Or(left, right) => {
                self.scan_bytes_quote_values(left, ch_slot, values);
                self.scan_bytes_quote_values(right, ch_slot, values);
            }
            _ => {
                let BuildBoolRow::IntCompare {
                    op: BinaryOp::Eq,
                    left,
                    right,
                } = self.scratch.borrow().bools[value.index()].clone()
                else {
                    return;
                };
                if self.scan_bytes_int_slot(left) == Some(ch_slot)
                    && let BuildIntRow::Int(value) = self.scratch.borrow().ints[right.index()]
                {
                    values.push(value);
                }
            }
        }
    }

    fn scan_bytes_true_assignment(&self, statements: &[BuildStmtId]) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::AssignBool { slot, value } =
                self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            matches!(
                self.scratch.borrow().bools[value.index()],
                BuildBoolRow::Bool(true)
            )
            .then_some(slot)
        })
    }

    fn scan_bytes_nested_bool_slot(&self, statements: &[BuildStmtId]) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::IfBool { branches, .. } =
                self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            branches
                .first()
                .and_then(|(condition, _)| self.scan_bytes_bool_slot(*condition))
        })
    }

    fn scan_bytes_delimiter_assignment(
        &self,
        statements: &[BuildStmtId],
        ch_slot: usize,
    ) -> Option<usize> {
        statements.iter().find_map(|stmt| {
            let BuildStmtRow::AssignInt {
                slot,
                op: AssignOp::Set,
                value,
                ..
            } = self.scratch.borrow().statements[stmt.index()].clone()
            else {
                return None;
            };
            (self.scan_bytes_int_slot(value) == Some(ch_slot)).then_some(slot)
        })
    }

    fn collect_scan_checks(
        &self,
        stmt: BuildStmtId,
        trimmed_slot: Option<usize>,
        checks: &mut Vec<ScanCheck>,
    ) -> bool {
        let BuildStmtRow::IfBool {
            branches,
            else_body,
        } = self.scratch.borrow().statements[stmt.index()].clone()
        else {
            return false;
        };
        for (condition, branch_body) in &branches {
            if branch_body.len() != 1 {
                return false;
            }
            let assignment = self.scratch.borrow().statements[branch_body[0].index()].clone();
            let counter_slot = match assignment {
                BuildStmtRow::Assign {
                    check: None,
                    slot,
                    op: AssignOp::Add,
                    value,
                    ..
                } if matches!(
                    self.scratch.borrow().expressions[value.index()],
                    BuildExprRow::Int(1)
                ) =>
                {
                    slot
                }
                BuildStmtRow::AssignInt {
                    slot,
                    op: AssignOp::Add,
                    value,
                    ..
                } if matches!(
                    self.scratch.borrow().ints[value.index()],
                    BuildIntRow::Int(1)
                ) =>
                {
                    slot
                }
                _ => return false,
            };
            let condition = self.scratch.borrow().bools[condition.index()].clone();
            let scan_condition = match condition {
                BuildBoolRow::TrimEmptySlot { .. } => ScanCondition::TrimEmpty,
                BuildBoolRow::LiteralCompareSlot {
                    op: BinaryOp::Eq,
                    slot,
                    value: LoweredValue::Bytes(value),
                } if trimmed_slot == Some(slot) && value.is_empty() => ScanCondition::TrimEmpty,
                BuildBoolRow::TrimStrPredicateSlot {
                    predicate: LoweredStrPredicate::StartsWith,
                    needle,
                    ..
                } => ScanCondition::TrimStartsWith(needle.to_vec()),
                BuildBoolRow::StrPredicateSlot {
                    predicate: LoweredStrPredicate::StartsWith,
                    slot,
                    needle,
                    ..
                } => {
                    if trimmed_slot == Some(slot) {
                        ScanCondition::TrimStartsWith(needle.to_vec())
                    } else {
                        ScanCondition::StartsWith(needle.to_vec())
                    }
                }
                _ => return false,
            };
            checks.push(ScanCheck {
                condition: scan_condition,
                counter_slot,
            });
        }
        let Some(else_body) = else_body else {
            return true;
        };
        if else_body.len() != 1 {
            return false;
        }
        self.collect_scan_checks(else_body[0], trimmed_slot, checks)
    }
}

/// Recursively check whether a lowered statement body contains any `Defer`
/// statements (including those nested inside `If` branches, `Retry` bodies, etc.).
pub(super) fn lowered_body_has_defers(scratch: &BuildScratch, statements: &[BuildStmtId]) -> bool {
    fn stmt_has_defers(scratch: &BuildScratch, stmt: &BuildStmtId) -> bool {
        match &scratch.statements[stmt.index()] {
            BuildStmtRow::Defer { .. } => true,
            BuildStmtRow::If {
                branches,
                else_body,
            } => {
                branches
                    .iter()
                    .any(|(_, body)| lowered_body_has_defers(scratch, body))
                    || else_body
                        .as_ref()
                        .is_some_and(|b| lowered_body_has_defers(scratch, b))
            }
            BuildStmtRow::IfBool {
                branches,
                else_body,
            } => {
                branches
                    .iter()
                    .any(|(_, body)| lowered_body_has_defers(scratch, body))
                    || else_body
                        .as_ref()
                        .is_some_and(|b| lowered_body_has_defers(scratch, b))
            }
            BuildStmtRow::While { body, .. } | BuildStmtRow::WhileBool { body, .. } => {
                lowered_body_has_defers(scratch, body)
            }
            BuildStmtRow::For { body, .. }
            | BuildStmtRow::ForRecord { body, .. }
            | BuildStmtRow::ForStrLines { body, .. } => lowered_body_has_defers(scratch, body),
            BuildStmtRow::Match { arms, .. } => arms
                .iter()
                .any(|(_, _, body)| lowered_body_has_defers(scratch, body)),
            BuildStmtRow::StrMatch { arms, fallback, .. } => {
                arms.values()
                    .any(|body| lowered_body_has_defers(scratch, body))
                    || fallback
                        .as_ref()
                        .is_some_and(|b| lowered_body_has_defers(scratch, b))
            }
            BuildStmtRow::TagMatch { arms, fallback, .. } => {
                arms.values()
                    .any(|body| lowered_body_has_defers(scratch, body))
                    || fallback
                        .as_ref()
                        .is_some_and(|b| lowered_body_has_defers(scratch, b))
            }
            BuildStmtRow::Guard { else_body, .. } => lowered_body_has_defers(scratch, else_body),
            BuildStmtRow::With {
                body, else_body, ..
            } => {
                lowered_body_has_defers(scratch, body)
                    || lowered_body_has_defers(scratch, else_body)
            }
            BuildStmtRow::Cd { body, .. } | BuildStmtRow::Env { body, .. } => {
                lowered_body_has_defers(scratch, body)
            }
            _ => false,
        }
    }
    statements.iter().any(|stmt| stmt_has_defers(scratch, stmt))
}

/// without an `else` (or a non-exhaustive `match`) can fall through. So a body
/// "can return" only when every reachable path provably ends in a `Return`.
pub(super) fn lowered_body_can_return(scratch: &BuildScratch, statements: &[BuildStmtId]) -> bool {
    statements
        .iter()
        .any(|stmt| match &scratch.statements[stmt.index()] {
            BuildStmtRow::Return { .. } => true,
            BuildStmtRow::Defer { .. } => false,
            BuildStmtRow::Yield { .. } | BuildStmtRow::YieldDelegate { .. } => false,
            BuildStmtRow::ScanLines { .. } => false,
            BuildStmtRow::ScanBytes { .. } => false,
            BuildStmtRow::Break | BuildStmtRow::BreakValue { .. } => false,
            BuildStmtRow::Continue => false,
            BuildStmtRow::If {
                branches,
                else_body,
            } => lowered_branch_chain_can_return(
                scratch,
                branches.iter().map(|(condition, body)| {
                    (
                        lowered_expr_bool_literal(scratch, condition),
                        body.as_slice(),
                    )
                }),
                else_body.as_deref(),
            ),
            BuildStmtRow::PatternIf {
                branches,
                else_body,
                ..
            } => lowered_branch_chain_can_return(
                scratch,
                branches.iter().map(|(condition, body, _)| {
                    (
                        lowered_expr_bool_literal(scratch, condition),
                        body.as_slice(),
                    )
                }),
                else_body.as_deref(),
            ),
            BuildStmtRow::IfBool {
                branches,
                else_body,
            } => lowered_branch_chain_can_return(
                scratch,
                branches.iter().map(|(condition, body)| {
                    (
                        lowered_bool_row_literal(scratch, condition),
                        body.as_slice(),
                    )
                }),
                else_body.as_deref(),
            ),
            BuildStmtRow::While { body, .. }
            | BuildStmtRow::PatternWhile { body, .. }
            | BuildStmtRow::WhileBool { body, .. }
            | BuildStmtRow::For { body, .. }
            | BuildStmtRow::ForRecord { body, .. }
            | BuildStmtRow::ForStrLines { body, .. } => {
                let _ = body;
                false
            }
            BuildStmtRow::Cd { body, .. } | BuildStmtRow::Env { body, .. } => {
                lowered_body_can_return(scratch, body)
            }
            BuildStmtRow::Match { arms, .. } => lowered_match_body_can_return(scratch, arms),
            BuildStmtRow::StrMatch { arms, fallback, .. } => {
                !arms.is_empty()
                    && arms
                        .values()
                        .all(|body| lowered_body_can_return(scratch, body))
                    && fallback
                        .as_ref()
                        .is_some_and(|body| lowered_body_can_return(scratch, body))
            }
            BuildStmtRow::TagMatch { arms, fallback, .. } => {
                !arms.is_empty()
                    && arms
                        .values()
                        .all(|body| lowered_body_can_return(scratch, body))
                    && fallback
                        .as_ref()
                        .is_some_and(|body| lowered_body_can_return(scratch, body))
            }
            // A guard's success path falls through to later statements, so the
            // guard alone never guarantees a return.
            BuildStmtRow::Guard { .. } => false,
            BuildStmtRow::With {
                body, else_body, ..
            } => {
                lowered_body_can_return(scratch, body)
                    && lowered_body_can_return(scratch, else_body)
            }
            BuildStmtRow::DefaultParameter { .. }
            | BuildStmtRow::Let { .. }
            | BuildStmtRow::LetRecord { .. }
            | BuildStmtRow::LetInt { .. }
            | BuildStmtRow::LetBool { .. }
            | BuildStmtRow::Assign { .. }
            | BuildStmtRow::AssignInt { .. }
            | BuildStmtRow::AssignField { .. }
            | BuildStmtRow::AssignFieldInt { .. }
            | BuildStmtRow::AssignPath { .. }
            | BuildStmtRow::AssignBool { .. }
            | BuildStmtRow::Value { .. }
            | BuildStmtRow::Assert { .. }
            | BuildStmtRow::Expr { .. }
            | BuildStmtRow::Run { .. }
            | BuildStmtRow::Print { .. }
            | BuildStmtRow::Proc { .. }
            | BuildStmtRow::Loop { .. } => false,
        })
}

fn lowered_return_kind_accepts_unit_fallthrough(kind: LoweredReturnKind) -> bool {
    matches!(
        kind,
        LoweredReturnKind::Plain(LoweredType::Unit)
            | LoweredReturnKind::Result(LoweredType::Unit)
            // `Any` (and the erased optionals lowering maps onto it) accepts
            // a Unit fallthrough, which is how a command-tailed body
            // completes an `Any`- or `Any?`-returning function.
            | LoweredReturnKind::Plain(LoweredType::Any)
            | LoweredReturnKind::Result(LoweredType::Any)
            | LoweredReturnKind::OptionalResult
    )
}

fn lowered_expr_bool_literal(scratch: &BuildScratch, expr: &BuildExprId) -> Option<bool> {
    match &scratch.expressions[expr.index()] {
        BuildExprRow::Bool(value) => Some(*value),
        _ => None,
    }
}

fn lowered_bool_row_literal(scratch: &BuildScratch, expr: &BuildBoolId) -> Option<bool> {
    match &scratch.bools[expr.index()] {
        BuildBoolRow::Bool(value) => Some(*value),
        _ => None,
    }
}

/// Whether an `if`-style chain returns on every reachable path. A
/// literal-false condition is unreachable, and a literal-true one makes every
/// later branch and the else unreachable; a no-else chain can fall through.
fn lowered_branch_chain_can_return<'a>(
    scratch: &BuildScratch,
    branches: impl Iterator<Item = (Option<bool>, &'a [BuildStmtId])>,
    else_body: Option<&[BuildStmtId]>,
) -> bool {
    for (literal, body) in branches {
        match literal {
            Some(false) => continue,
            Some(true) => return lowered_body_can_return(scratch, body),
            None => {
                if !lowered_body_can_return(scratch, body) {
                    return false;
                }
            }
        }
    }
    else_body.is_some_and(|body| lowered_body_can_return(scratch, body))
}

pub(super) fn lowered_match_body_can_return(
    scratch: &BuildScratch,
    arms: &[(BuildPatternId, Option<BuildExprId>, Vec<BuildStmtId>)],
) -> bool {
    !arms.is_empty()
        && arms
            .iter()
            .all(|(_, _, body)| lowered_body_can_return(scratch, body))
}

pub(super) fn cleanup_lowered_pattern_slots(slots: &mut SlotScope, cleanup: Vec<(Name, usize)>) {
    for (name, slot) in cleanup {
        slots.retire(name, slot, "pattern");
    }
}

pub(super) fn lowered_error_value_has_facet(value: &Value, facet: &str) -> bool {
    match value {
        Value::Error(error) => error.facets.iter().any(|f| f == facet),
        Value::RunError(error) => error.facets().iter().any(|f| f == facet),
        _ => false,
    }
}

pub(super) fn lowered_error_variant_matches(
    family: &Name,
    variant: &Name,
    fields: &LoweredErrorPatternFields,
    value: &Value,
    slots: &mut [LoweredValue],
    bind: bool,
) -> bool {
    match value {
        Value::Error(error) => {
            crate::runtime::value::error_family_matches(error.family_name(), *family)
                && error.variant_name() == *variant
                && lowered_error_pattern_fields_match(&error.payload, fields, slots, bind)
        }
        Value::RunError(error) if *family == Name::PROCESS_ERROR => {
            if error.variant_name() != variant.as_str() {
                return false;
            }
            let payload = error.payload();
            lowered_error_pattern_fields_match(&payload, fields, slots, bind)
        }
        _ => false,
    }
}

fn lowered_error_pattern_fields_match(
    payload: &RecordMap,
    fields: &LoweredErrorPatternFields,
    slots: &mut [LoweredValue],
    bind: bool,
) -> bool {
    for (name, slot) in fields {
        let Some(value) = payload.get(&name.as_str()) else {
            return false;
        };
        if let Some(slot) = slot {
            let Some(value) = lowered_value_from_runtime_any(value) else {
                return false;
            };
            if bind {
                slots[*slot] = value;
            }
        }
    }
    true
}

pub(super) fn lowered_str_key(value: &LoweredValue) -> Option<&str> {
    match value {
        LoweredValue::Str(value) => Some(value.as_ref()),
        LoweredValue::StrView(value) => Some(value.as_str()),
        _ => None,
    }
}

pub(super) fn lowered_record_field<'a>(
    value: &'a LoweredValue,
    field: &str,
) -> Option<&'a LoweredValue> {
    match value {
        LoweredValue::Record(entries) | LoweredValue::Module(entries) => entries.get(field),
        LoweredValue::RecordVec(entries) => lowered_record_vec_get(entries, field),
        LoweredValue::Stats { .. } | LoweredValue::StatsBlob(_) => None,
        _ => None,
    }
}

/// Take a shared container's contents out of its `Arc`.
///
/// Reuses the storage when this handle is the only owner and copies otherwise,
/// so merging a value the caller has already given up does not copy it.
pub(super) fn take_shared<T: Clone>(shared: Arc<T>) -> T {
    Arc::try_unwrap(shared).unwrap_or_else(|still_shared| (*still_shared).clone())
}

pub(super) fn lowered_sum_records(mut acc: LoweredValue, val: LoweredValue) -> LoweredValue {
    match (&mut acc, val) {
        (LoweredValue::Record(acc_map), LoweredValue::Record(val_map)) => {
            let acc_map = Arc::make_mut(acc_map);
            for (key, value) in take_shared(val_map) {
                match acc_map.get_mut(&key) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        acc_map.insert(key, value);
                    }
                }
            }
        }
        (LoweredValue::RecordVec(acc_map), LoweredValue::RecordVec(val_map)) => {
            let acc_map = Arc::make_mut(acc_map);
            for (key, value) in take_shared(val_map) {
                match lowered_record_vec_get_mut(acc_map, &key.as_str()) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        lowered_record_vec_insert(acc_map, key, value);
                    }
                }
            }
        }
        (LoweredValue::Record(acc_map), LoweredValue::RecordVec(val_map)) => {
            let acc_map = Arc::make_mut(acc_map);
            for (key, value) in take_shared(val_map) {
                let key_text = key.as_str();
                match acc_map.get_mut::<str>(key_text.as_str()) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        acc_map.insert(Arc::<str>::from(key_text.as_str()), value);
                    }
                }
            }
        }
        (LoweredValue::RecordVec(acc_map), LoweredValue::Record(val_map)) => {
            let acc_map = Arc::make_mut(acc_map);
            for (key, value) in take_shared(val_map) {
                match lowered_record_vec_get_mut(acc_map, key.as_ref()) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        lowered_record_vec_insert(acc_map, Name::intern(key.as_ref()), value);
                    }
                }
            }
        }
        _ => {}
    }
    acc
}

pub(super) fn lowered_sum_values(acc: LoweredValue, val: LoweredValue) -> LoweredValue {
    match (acc, val) {
        (LoweredValue::Int(a), LoweredValue::Int(b)) => LoweredValue::Int(a + b),
        (LoweredValue::Float(a), LoweredValue::Float(b)) => {
            LoweredValue::Float(crate::runtime::value::FloatValue::new(a.0 + b.0))
        }
        (LoweredValue::List(mut acc), LoweredValue::List(value)) => {
            acc.extend(value);
            LoweredValue::List(acc)
        }
        (LoweredValue::List(mut acc), LoweredValue::SharedList(value)) => {
            acc.extend(value.iter().cloned());
            LoweredValue::List(acc)
        }
        (LoweredValue::SharedList(acc), LoweredValue::List(value)) => {
            let mut acc = (*acc).clone();
            acc.extend(value);
            LoweredValue::List(acc)
        }
        (LoweredValue::SharedList(acc), LoweredValue::SharedList(value)) => {
            let mut acc = (*acc).clone();
            acc.extend(value.iter().cloned());
            LoweredValue::List(acc)
        }
        (LoweredValue::Record(acc_map), LoweredValue::Record(val_map)) => {
            let mut acc_map = take_shared(acc_map);
            for (key, value) in take_shared(val_map) {
                match acc_map.get_mut(&key) {
                    Some(acc_value) => {
                        *acc_value = lowered_sum_values(
                            std::mem::replace(acc_value, LoweredValue::Unit),
                            value,
                        );
                    }
                    None => {
                        acc_map.insert(key, value);
                    }
                }
            }
            LoweredValue::Record(Arc::new(acc_map))
        }
        (acc @ LoweredValue::RecordVec(_), val @ LoweredValue::RecordVec(_))
        | (acc @ LoweredValue::Record(_), val @ LoweredValue::RecordVec(_))
        | (acc @ LoweredValue::RecordVec(_), val @ LoweredValue::Record(_)) => {
            lowered_sum_records(acc, val)
        }
        (acc, _) => acc,
    }
}

pub(super) fn lowered_tag_key(value: &LoweredValue) -> Option<&str> {
    match value {
        LoweredValue::Tag(tag) if tag.fields.is_empty() => Some(tag.name.as_ref()),
        _ => None,
    }
}

pub(super) fn lowered_match_no_arm(span: Span) -> RuntimeError {
    RuntimeError::new("match-no-arm", "match did not match any arm").with_span(span)
}

pub(super) fn lowered_stmt_flow_to_flow(flow: StmtFlow) -> Flow {
    match flow {
        StmtFlow::None => Flow::Continue(Value::Unit),
        StmtFlow::Value(value) | StmtFlow::Return(value) => Flow::Return(value.into_value()),
        StmtFlow::Propagate(_) => {
            unreachable!("lowered propagation must be handled with evaluator context")
        }
        StmtFlow::Break(value) => Flow::Break(value.map(LoweredValue::into_value)),
        StmtFlow::Continue => Flow::ContinueLoop,
    }
}

pub(super) fn cleanup_pipeline_stage_item_slot(
    slots: &mut SlotScope,
    cleanup: Option<Name>,
    slot: usize,
) {
    if let Some(name) = cleanup {
        slots.retire(name, slot, "pipeline.item");
    }
}

#[cfg(test)]
mod named_spread_tests {
    /// Builds `source` and returns how many whole-program copies its
    /// named-spread calls made.
    fn program_copies(source: &str) -> u64 {
        let name = "named-spread.xsh";
        let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
            name,
            crate::loader::entry_source_from_text(name, source.to_string()),
            Vec::new(),
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let source_id = sources.files()[0].id();
        let before = super::SPREAD_PROGRAM_COPIES.with(std::cell::Cell::get);
        let built = super::super::FullBuilder::build_compact(
            &parsed.arena,
            &declarations,
            source,
            std::sync::Arc::new(sources),
            source_id,
        );
        assert!(built.is_ok(), "the program lowers");
        super::SPREAD_PROGRAM_COPIES.with(std::cell::Cell::get) - before
    }

    // The projections of a named-spread call are appended to a copy of the
    // program. Each call once made its own copy, which is work proportional
    // to the whole program per call; the calls of one pass now share one.
    #[test]
    fn named_spread_calls_share_one_program_copy_per_pass() {
        const CALLS: usize = 40;
        let mut source = String::from(
            "type Point = {x: Int, y: Int}\n\nproc sum(x: Int, y: Int) -> Int {\n  x + y\n}\n\n",
        );
        for index in 0..CALLS {
            source.push_str(&format!(
                "proc call{index}(point: Point) -> Int {{\n  sum(...point)\n}}\n\n"
            ));
        }
        source.push_str("let origin: Point = {x: 1, y: 2}\n");
        for index in 0..CALLS {
            source.push_str(&format!(
                "print f\"{{sum(...origin) + call{index}(origin)}}\"\n"
            ));
        }
        let copies = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || program_copies(&source))
            .expect("spawn the lowering thread")
            .join()
            .expect("the program lowers");
        // The functions are one pass and the top-level statements another;
        // 80 calls once made 80 copies or more.
        assert!((1..=4).contains(&copies), "{copies} program copies");
    }
}

#[cfg(all(test, debug_assertions))]
mod drift_tests {
    /// Lower every repository program and the embedded standard library, and
    /// require each representation lowering derives itself to agree with the
    /// checker's published type.
    #[test]
    fn corpus_lowering_agrees_with_checked_types() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                let mut paths = Vec::new();
                for dir in ["core", "dev", "examples", "showcase", "stdlib", "tests"] {
                    super::super::indexed::tests::collect_xsh_paths(&root.join(dir), &mut paths);
                }
                paths.sort();
                let mut lowered = 0;
                for path in paths {
                    let name = path.to_string_lossy();
                    let source = std::fs::read_to_string(&path).expect("corpus source is readable");
                    let (sources, parsed) = crate::loader::parse_load_entry_source_arena_only(
                        &name,
                        crate::loader::entry_source_from_text(&name, source.clone()),
                        Vec::new(),
                    );
                    let declarations =
                        crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
                    if !parsed.diagnostics.is_empty() || !declarations.diagnostics.is_empty() {
                        continue;
                    }
                    let source_id = sources.files()[0].id();
                    lowered += usize::from(
                        super::super::FullBuilder::build_compact(
                            &parsed.arena,
                            &declarations,
                            &source,
                            std::sync::Arc::new(sources),
                            source_id,
                        )
                        .is_ok(),
                    );
                }
                let drift = std::mem::take(&mut *super::LOWERING_DRIFT.lock().unwrap());
                assert!(lowered > 300, "only {lowered} corpus programs lowered");
                assert!(drift.is_empty(), "{}", drift.join("\n"));
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
